//! The write context (D75, D77): who is changing records, by which route, as part of which batch,
//! and which governed state changes ("tickets") the change is allowed to make.
//!
//! The command layer installs a context for the duration of one call ([`scope`]); the storage
//! layer reads it where a write reaches it ([`current_or_system`]). A thread-local is used on
//! purpose: custom handlers and observers (`party.erase`, connector handlers, payment webhooks)
//! write through the same `Backend` methods as everyone else, and one ambient value covers all of
//! them without a new argument on each. The scope is not `Send`: it belongs to the thread that
//! opened it, and a worker thread starts with no context (it writes as the system).
//!
//! Nothing in this module changes what a write does by itself. The `Tracked` wrapper hands the
//! context to `Backend::apply_ctx`, and the backends ignore it until a model is tracked.

use serde_json::json;
use std::cell::RefCell;
use std::marker::PhantomData;
use std::sync::OnceLock;

/// `created_by` / `updated_by` of a change the system made on its own: seeds, migrations, a
/// worker. It contains a colon, which no login can (`valid_name` allows `A-Za-z0-9._-@`).
pub const ACTOR_SYSTEM: &str = "system:internal";
/// The actor of a change a visitor of the public site made without signing in.
pub const ACTOR_PUBLIC: &str = "public:anonymous";

/// By which route a change arrived. `surface` is where the call came from (`rest`, `mcp`, `cli`,
/// `connector`, `public`, `system`), `path` how the storage layer was reached (`direct`, `batch`,
/// `effect`, `migration`, `erase`); `command` is the id of the command that ran.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Via {
    pub surface: String,
    pub path: String,
    pub command: String,
    pub request_id: Option<String>,
    /// The first 12 hex digits of the stored token hash, for a call that came with a token.
    pub token_ref: Option<String>,
    /// The call was made by an agent (its `by` is the agent's human owner).
    pub agent: bool,
    /// Who approved the plan this call executes, if one was.
    pub approver: Option<String>,
}

/// A permit to make one governed state change: the action `action` on the record `(model, id)`.
/// The database accepts a change of a governed field only along a declared edge whose action has
/// a ticket (D77).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionRun {
    pub model: String,
    pub id: String,
    pub action: String,
}

/// Everything a write needs to know about its caller. One struct for D75 (who and how) and D77
/// (tickets), so there is one value to pass and to scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteCtx {
    /// The login of the person (for an agent: its human owner), or a reserved actor id.
    pub by: String,
    pub via: Via,
    /// Rows of one bulk call share it.
    pub batch_id: Option<String>,
    /// A migration run, for the entries a data step writes.
    pub run_id: Option<String>,
    /// The record a governed transition moved; other records of the call are effects.
    pub primary: Option<(String, String)>,
    /// Tickets for governed state changes (D77).
    pub actions: Vec<ActionRun>,
}

impl WriteCtx {
    /// The context of a change nobody asked for: `system:internal`, surface `system`.
    pub fn system(command: &str) -> Self {
        Self::new(ACTOR_SYSTEM, "system", command)
    }

    pub fn new(by: &str, surface: &str, command: &str) -> Self {
        Self {
            by: by.to_string(),
            via: Via {
                surface: surface.to_string(),
                path: "direct".to_string(),
                command: command.to_string(),
                ..Via::default()
            },
            batch_id: None,
            run_id: None,
            primary: None,
            actions: Vec::new(),
        }
    }

    pub fn path(mut self, path: &str) -> Self {
        self.via.path = path.to_string();
        self
    }

    pub fn batch(mut self, id: &str) -> Self {
        self.batch_id = Some(id.to_string());
        self
    }

    pub fn run(mut self, id: &str) -> Self {
        self.run_id = Some(id.to_string());
        self
    }

    pub fn primary(mut self, model: &str, id: &str) -> Self {
        self.primary = Some((model.to_string(), id.to_string()));
        self
    }

    /// Add a ticket for the action `action` on `(model, id)`.
    pub fn ticket(mut self, model: &str, id: &str, action: &str) -> Self {
        self.actions.push(ActionRun {
            model: model.to_string(),
            id: id.to_string(),
            action: action.to_string(),
        });
        self
    }

    /// What the database sees of this context: the transaction-local setting `obop.ctx`. Only the
    /// tickets are sent (`{"actions":[{"m":..,"i":..,"a":..}]}`); the actor and the route stay in
    /// Rust, so an ordinary write costs no extra round trip. `None` when there are no tickets.
    pub(crate) fn setting(&self) -> Option<String> {
        if self.actions.is_empty() {
            return None;
        }
        let actions: Vec<_> = self
            .actions
            .iter()
            .map(|a| json!({"m": a.model, "i": a.id, "a": a.action}))
            .collect();
        Some(json!({ "actions": actions }).to_string())
    }
}

/// The context of a write that happens outside any scope, built once.
pub(crate) fn system_ctx() -> &'static WriteCtx {
    static SYSTEM: OnceLock<WriteCtx> = OnceLock::new();
    SYSTEM.get_or_init(|| WriteCtx::system(""))
}

thread_local! {
    static CURRENT: RefCell<Option<WriteCtx>> = const { RefCell::new(None) };
}

/// Restores the context that was current before [`scope`] when it is dropped, also when the
/// thread unwinds. Scopes nest; drop them in the reverse order of opening (the natural order of
/// local variables). Not `Send`: the context belongs to the thread that set it.
#[must_use = "the context is only current while the scope is alive"]
pub struct Scope {
    previous: Option<WriteCtx>,
    _not_send: PhantomData<*const ()>,
}

/// Make `ctx` the context of the writes this thread makes until the returned scope is dropped.
pub fn scope(ctx: WriteCtx) -> Scope {
    let previous = CURRENT.with(|c| c.borrow_mut().replace(ctx));
    Scope {
        previous,
        _not_send: PhantomData,
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        let previous = self.previous.take();
        CURRENT.with(|c| *c.borrow_mut() = previous);
    }
}

/// The context of this thread, if a scope is open.
pub fn current() -> Option<WriteCtx> {
    CURRENT.with(|c| c.borrow().clone())
}

/// The context of this thread, or the system's when no scope is open.
pub fn current_or_system() -> WriteCtx {
    current().unwrap_or_else(|| system_ctx().clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_a_scope_the_system_writes() {
        assert_eq!(current(), None);
        let c = current_or_system();
        assert_eq!(c.by, ACTOR_SYSTEM);
        assert_eq!(
            (c.via.surface.as_str(), c.via.path.as_str()),
            ("system", "direct")
        );
        assert!(c.actions.is_empty() && c.batch_id.is_none() && c.primary.is_none());
        assert_eq!(&c, system_ctx());
    }

    #[test]
    fn a_scope_is_current_until_it_is_dropped_and_scopes_nest() {
        let outer = WriteCtx::new("ann", "rest", "crm.lead.create");
        let inner = WriteCtx::new("bob", "mcp", "crm.lead.update").path("effect");
        let a = scope(outer.clone());
        assert_eq!(current(), Some(outer.clone()));
        {
            let _b = scope(inner.clone());
            assert_eq!(current(), Some(inner.clone()));
            assert_eq!(current_or_system().via.path, "effect");
        }
        assert_eq!(
            current(),
            Some(outer),
            "the inner scope gave the outer one back"
        );
        drop(a);
        assert_eq!(current(), None);
    }

    #[test]
    fn a_scope_is_restored_when_the_thread_unwinds() {
        let before = WriteCtx::new("ann", "rest", "a");
        let _keep = scope(before.clone());
        let r = std::panic::catch_unwind(|| {
            let _s = scope(WriteCtx::new("bob", "cli", "b"));
            assert_eq!(current().unwrap().by, "bob");
            panic!("a handler failed");
        });
        assert!(r.is_err());
        assert_eq!(current(), Some(before));
    }

    #[test]
    fn another_thread_starts_with_no_context() {
        let _s = scope(WriteCtx::new("ann", "rest", "a"));
        let seen = std::thread::spawn(|| (current(), current_or_system().by))
            .join()
            .unwrap();
        assert_eq!(seen, (None, ACTOR_SYSTEM.to_string()));
        assert_eq!(current().unwrap().by, "ann");
    }

    #[test]
    fn the_builders_set_one_thing_each() {
        let c = WriteCtx::new("ann", "rest", "sales.order.confirm")
            .path("batch")
            .batch("b-1")
            .run("m@1->2")
            .primary("sale_order", "o1")
            .ticket("sale_order", "o1", "confirm");
        assert_eq!(c.by, "ann");
        assert_eq!(
            (
                c.via.surface.as_str(),
                c.via.path.as_str(),
                c.via.command.as_str()
            ),
            ("rest", "batch", "sales.order.confirm")
        );
        assert_eq!(c.batch_id.as_deref(), Some("b-1"));
        assert_eq!(c.run_id.as_deref(), Some("m@1->2"));
        assert_eq!(c.primary, Some(("sale_order".into(), "o1".into())));
        assert_eq!(
            c.actions,
            [ActionRun {
                model: "sale_order".into(),
                id: "o1".into(),
                action: "confirm".into()
            }]
        );
    }

    #[test]
    fn the_database_setting_carries_the_tickets_and_nothing_else() {
        // No tickets, no setting: an ordinary write costs no extra round trip.
        let plain = WriteCtx::new("ann", "rest", "x").batch("b").run("r");
        assert_eq!(plain.setting(), None);
        let c = plain
            .ticket("invoice", "i1", "post")
            .ticket("invoice", "i2", "reopen");
        let text = c.setting().unwrap();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            v,
            json!({"actions": [
                {"m": "invoice", "i": "i1", "a": "post"},
                {"m": "invoice", "i": "i2", "a": "reopen"},
            ]})
        );
        // Only `actions`: the actor, the route and the batch never reach the database setting.
        assert_eq!(v.as_object().unwrap().len(), 1);
        for secret in ["ann", "rest", "\"b\"", "\"r\""] {
            assert!(!text.contains(secret), "{secret} leaked into {text}");
        }
    }
}
