//! State the dispatcher shares between server processes (B1, D59): the lock that serialises
//! guarded writes and the store of proposed plans.
//!
//! With a shared backend set (`Dispatcher::set_shared`) the write lock is the backend's
//! cross-process lock (a PostgreSQL advisory lock, a SQLite file lock) and a proposed plan is a
//! sealed, expiring record any process can approve. Without one (tests, embedding) both are
//! process-local, as before.

use crate::{Principal, PrincipalKind, Value};
use obop_keys::KeyRing;
use obop_storage::{Backend, LockGuard};
use serde_json::{Value as J, json};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard};

/// The lock every guarded write takes.
const TXN_KEY: &str = "dispatcher.txn";
/// How long a proposed plan waits for a human.
const PLAN_TTL_MS: u64 = 7 * 24 * 3600 * 1000;

/// The backend and key ring a dispatcher shares with other processes.
#[derive(Clone)]
pub struct Shared {
    pub backend: Arc<dyn Backend>,
    pub(crate) ring: KeyRing,
}

impl Shared {
    pub fn new(backend: Arc<dyn Backend>, ring: KeyRing) -> Self {
        Self { backend, ring }
    }
}

/// A proposed call waiting for approval.
#[derive(Clone)]
pub(crate) struct Pending {
    pub principal: Principal,
    pub id: String,
    pub input: Value,
}

impl Pending {
    fn to_json(&self) -> J {
        let list = |s: &HashSet<String>| {
            let mut v: Vec<&String> = s.iter().collect();
            v.sort();
            json!(v)
        };
        json!({
            "principal": {
                "id": self.principal.id,
                "kind": match self.principal.kind {
                    PrincipalKind::User => "user",
                    PrincipalKind::Agent => "agent",
                },
                "groups": list(&self.principal.groups),
                "scopes": self.principal.scopes.as_ref().map(list),
                "mfa_limited": self.principal.mfa_limited,
            },
            "command": self.id,
            "input": self.input.to_json(),
        })
    }

    fn from_json(j: &J) -> Option<Pending> {
        let p = &j["principal"];
        let set = |v: &J| -> Option<HashSet<String>> {
            v.as_array().map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
        };
        let kind = match p["kind"].as_str()? {
            "user" => PrincipalKind::User,
            "agent" => PrincipalKind::Agent,
            _ => return None,
        };
        // An agent always carries scopes (`Principal::agent` sets them), and `None` means
        // unrestricted. A stored plan of an agent with no scopes list would otherwise rebuild a
        // principal that has every group of its owner; it gets the empty set, which allows
        // nothing, so a plan that cannot say what the agent was narrowed to runs as an agent
        // narrowed to nothing.
        let scopes = match (kind, set(&p["scopes"])) {
            (PrincipalKind::Agent, None) => Some(HashSet::new()),
            (_, scopes) => scopes,
        };
        Some(Pending {
            principal: Principal {
                id: p["id"].as_str()?.to_string(),
                kind,
                groups: set(&p["groups"])?,
                scopes,
                mfa_limited: p["mfa_limited"].as_bool().unwrap_or(false),
                // A mobile app's token is never refused for approval, so it is never in a plan.
                device: None,
            },
            id: j["command"].as_str()?.to_string(),
            input: Value::from_json(&j["input"]),
        })
    }
}

/// Held for the duration of one guarded write.
#[allow(dead_code)]
pub(crate) enum TxnGuard<'a> {
    Local(MutexGuard<'a, ()>),
    Shared(LockGuard),
}

/// Pending plans, in memory or in the shared backend.
#[derive(Default)]
pub(crate) struct Plans {
    local: Mutex<HashMap<String, Pending>>,
}

fn purpose(plan_id: &str) -> String {
    format!("plan:{plan_id}")
}

impl Plans {
    pub fn put(&self, shared: Option<&Shared>, plan_id: &str, p: &Pending) -> Result<(), String> {
        match shared {
            Some(s) => {
                let sealed = s
                    .ring
                    .seal_str(&purpose(plan_id), &p.to_json().to_string())
                    .map_err(String::from)?;
                s.backend
                    .kv_put(&format!("plan:{plan_id}"), &sealed, Some(PLAN_TTL_MS))
            }
            None => {
                self.local
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(plan_id.to_string(), p.clone());
                Ok(())
            }
        }
    }

    pub fn get(&self, shared: Option<&Shared>, plan_id: &str) -> Option<Pending> {
        match shared {
            Some(s) => {
                let sealed = s.backend.kv_get(&format!("plan:{plan_id}")).ok()??;
                let text = s.ring.open_str(&purpose(plan_id), &sealed).ok()?;
                Pending::from_json(&serde_json::from_str(&text).ok()?)
            }
            None => self
                .local
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(plan_id)
                .cloned(),
        }
    }

    /// Take the plan out, so it runs once. A refusal of the store is an error: a plan that is
    /// still there would run again.
    pub fn remove(&self, shared: Option<&Shared>, plan_id: &str) -> Result<(), String> {
        match shared {
            Some(s) => s.backend.kv_delete(&format!("plan:{plan_id}")).map(|_| ()),
            None => {
                self.local
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(plan_id);
                Ok(())
            }
        }
    }
}

pub(crate) fn txn_lock<'a>(
    shared: Option<&Shared>,
    local: &'a Mutex<()>,
) -> Result<TxnGuard<'a>, String> {
    match shared {
        Some(s) => Ok(TxnGuard::Shared(s.backend.lock(TXN_KEY)?)),
        None => Ok(TxnGuard::Local(
            local.lock().unwrap_or_else(|e| e.into_inner()),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pending() -> Pending {
        Pending {
            principal: Principal::agent("ada/agent", &["sales.user", "base.user"], &["sales.user"]),
            id: "sales.order.confirm".into(),
            input: Value::map([
                ("id", Value::text("o1")),
                ("n", Value::Int(3)),
                ("tags", Value::List(vec![Value::text("a"), Value::Null])),
            ]),
        }
    }

    #[test]
    fn a_plan_survives_a_round_trip_through_json() {
        let p = pending();
        let back = Pending::from_json(&p.to_json()).unwrap();
        assert_eq!(back.id, p.id);
        assert_eq!(back.input, p.input);
        assert_eq!(back.principal.id, p.principal.id);
        assert_eq!(back.principal.kind, PrincipalKind::Agent);
        assert_eq!(back.principal.groups, p.principal.groups);
        assert_eq!(back.principal.scopes, p.principal.scopes);
    }

    /// Only a user who is not narrowed is a person. An agent whose scopes are `None` (no
    /// constructor makes one; a stored plan could describe one) is not a person, so the checks that
    /// are for a person alone (the branding commands, the approval, the account routes) refuse it.
    #[test]
    fn an_agent_with_no_scopes_is_not_a_person() {
        let mut agent = Principal::agent("ada/agent", &["platform.admin"], &["platform.admin"]);
        assert!(!agent.is_person());
        agent.scopes = None;
        assert!(
            !agent.is_person(),
            "an unrestricted agent is still an agent"
        );
        assert!(Principal::user("ada", &["platform.admin"]).is_person());
        let mut narrowed = Principal::user("ada", &["platform.admin"]);
        narrowed.scopes = Some(HashSet::new());
        assert!(
            !narrowed.is_person(),
            "a narrowed user is not a person either"
        );
    }

    /// A stored plan of an agent with no scopes list is rebuilt as an agent narrowed to nothing,
    /// never as a principal that holds everything its owner holds and looks like a person.
    #[test]
    fn a_stored_plan_of_an_agent_without_scopes_is_rebuilt_narrowed_to_nothing() {
        for scopes in [Some(J::Null), Some(json!("sales.user")), None] {
            let mut j = pending().to_json();
            match scopes {
                Some(v) => j["principal"]["scopes"] = v,
                None => {
                    j["principal"].as_object_mut().unwrap().remove("scopes");
                }
            }
            let back = Pending::from_json(&j).unwrap();
            assert_eq!(back.principal.kind, PrincipalKind::Agent);
            assert_eq!(back.principal.scopes, Some(HashSet::new()), "{j}");
            assert!(!back.principal.is_person());
            assert!(!back.principal.allows("sales.user"), "holds nothing: {j}");
        }
        // A user's plan keeps the scopes it has: none, and a list.
        let user = Pending {
            principal: Principal::user("ada", &["sales.user"]),
            ..pending()
        };
        let back = Pending::from_json(&user.to_json()).unwrap();
        assert_eq!(back.principal.scopes, None);
        assert!(back.principal.is_person());
        let mut scoped = user.clone();
        scoped.principal.scopes = Some(["sales.user".to_string()].into_iter().collect());
        let back = Pending::from_json(&scoped.to_json()).unwrap();
        assert_eq!(back.principal.scopes, scoped.principal.scopes);
    }

    #[test]
    fn plans_are_sealed_and_visible_to_a_second_handle() {
        let backend = obop_storage::open("memory:").unwrap();
        let ring = KeyRing::for_tests();
        let a = Shared::new(backend.clone(), ring.clone());
        let b = Shared::new(backend.clone(), ring);
        let (pa, pb) = (Plans::default(), Plans::default());
        pa.put(Some(&a), "p1", &pending()).unwrap();
        let raw = backend.kv_get("plan:p1").unwrap().unwrap();
        assert!(raw.starts_with("e2.") && !raw.contains("sales.order"));
        assert_eq!(pb.get(Some(&b), "p1").unwrap().id, "sales.order.confirm");
        pb.remove(Some(&b), "p1").unwrap();
        assert!(
            pa.get(Some(&a), "p1").is_none(),
            "approved or withdrawn once, everywhere"
        );
        // Another plan id cannot read a copied record: the id is bound in.
        pa.put(Some(&a), "p2", &pending()).unwrap();
        backend
            .kv_put(
                "plan:p3",
                &backend.kv_get("plan:p2").unwrap().unwrap(),
                None,
            )
            .unwrap();
        assert!(pa.get(Some(&a), "p3").is_none());
    }
}
