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
        Some(Pending {
            principal: Principal {
                id: p["id"].as_str()?.to_string(),
                kind: match p["kind"].as_str()? {
                    "user" => PrincipalKind::User,
                    "agent" => PrincipalKind::Agent,
                    _ => return None,
                },
                groups: set(&p["groups"])?,
                scopes: set(&p["scopes"]),
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
