//! The `Tracked` wrapper (K14): the layer that makes a write know who made it.
//!
//! `open` returns every backend wrapped in a `Tracked`. It forwards every method of [`Backend`]
//! to the backend underneath and adds three things:
//!
//! - **`apply_ctx(ctx, ops)`** is the write primitive that carries a [`WriteCtx`] (D75, D77). It
//!   calls `apply_tracked(ctx, ops, tracking)` of the backend underneath, which is what the three
//!   stores implement; their `apply_checked` is the same call with the system's context and no
//!   tracked models. Only `Tracked` implements `apply_ctx` (the stores must not: a store whose
//!   `apply_ctx` called `apply_checked` while its `apply_checked` called `apply_ctx` would call
//!   itself until the stack ran out).
//! - **The list of tracked models** ([`Tracking`]). A write through the old methods (`insert`,
//!   `update`, `delete`, `insert_many`, `apply`, `apply_checked`) to a tracked model is routed to
//!   `apply_ctx` with the ambient context ([`ctx::current_or_system`]), so that code that does not
//!   know about contexts still writes through the one primitive. No model is tracked yet, so
//!   nothing is routed yet.
//! - **The refusal to write a pseudo-model** ([`PSEUDO_MODELS`]): a name that stands for something
//!   other than records in a backup stream.
//!
//! Every method of [`Backend`] must be forwarded here: a method that is left out silently uses the
//! trait's default, which for a feature that lives in the store (pushdown, history, guards) means
//! the feature is off and nothing says so. The test `tracked_forwards_every_backend_method` reads
//! the trait and this file and fails for any method that is not in the list below; its sibling
//! `the_other_wrappers_forward_every_backend_method_too` does the same for `Guarded`, for the
//! `Flaky` of `testing` and for the `Flaky` of `obop-ops` (it reads that file by path).

use crate::ctx::{self, WriteCtx};
use crate::history::{Backfilled, HistoryQuery};
use crate::sysfields::ChangeRow;
use crate::{ApplyError, Backend, Fields, J, LockGuard, Op, Result, Row, RowSink, Swap};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, RwLock};

/// Model names that are not records: they name rows of a backup stream that the store keeps
/// elsewhere (the change history of D75 travels as `record_change` rows). A write to one of them
/// would create a real record that collides with the exported rows, so [`Tracked`] refuses it.
/// Reads are not refused (they find nothing), and `import` is not either: it is how those rows
/// come back.
///
/// The install gate refuses the same name in a package: `RESERVED_MODELS` of `obop-registry`
/// lists it, together with three names of models that are stored as records or are virtual and
/// so are not refused here. A package installed before the gate did that still loads, and its
/// writes to a model of that name are refused here with this message (docs/ISSUES.md, K21).
pub const PSEUDO_MODELS: [&str; 1] = ["record_change"];

fn is_pseudo(model: &str) -> bool {
    PSEUDO_MODELS.contains(&model)
}

fn pseudo_refusal(model: &str) -> String {
    format!(
        "{model} is a reserved model name (it names rows of a backup, not records) and cannot be \
         written as a record"
    )
}

/// How a field is written into a change-history entry (D75).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Treat {
    /// The value as it is.
    Plain,
    /// Only the fact that the field changed (class `secret`, or a field nobody declared).
    Redact,
    /// The value, sealed under the key ring (class `special_category`, `confidential_hr`).
    Seal,
}

/// What the store keeps for a tracked model.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Track {
    /// Record a history entry for every change.
    pub history: bool,
    /// Fields that are not written as they are; a field not listed is redacted.
    pub fields: BTreeMap<String, Treat>,
    /// Declared fields an erasure keeps (not personal, not sensitive).
    pub keep_on_erase: BTreeSet<String>,
    /// Days to keep history entries; `None` keeps them forever.
    pub retention_days: Option<u32>,
}

/// Seals a value of a field of class `special_category` or `confidential_hr` before it is written
/// into a history entry, and opens it again (D75). Implemented by the key ring (a later step); a
/// store without one records such a field as `redacted`. `purpose` names what the value is
/// (`history.v1:{model}.{field}`), so a sealed value cannot be moved to another field.
pub trait Sealer: Send + Sync {
    /// The sealed form of `plain`, or `None` when it cannot be sealed (no usable key).
    fn seal(&self, purpose: &str, plain: &str) -> Option<String>;
    /// The text `sealed` holds, or `None` when it is not a value sealed for `purpose` that this
    /// key ring can open.
    fn open(&self, purpose: &str, sealed: &str) -> Option<String>;
}

/// The models a [`Tracked`] store tracks, readable by the backend underneath through
/// `apply_tracked`, and the sealer of the history. Empty means every model behaves as it always
/// did.
#[derive(Default)]
pub struct Tracking {
    models: RwLock<BTreeMap<String, Arc<Track>>>,
    sealer: RwLock<Option<Arc<dyn Sealer>>>,
}

impl Tracking {
    pub const fn new() -> Self {
        Self {
            models: RwLock::new(BTreeMap::new()),
            sealer: RwLock::new(None),
        }
    }

    pub fn set_sealer(&self, sealer: Arc<dyn Sealer>) {
        *self.sealer.write().unwrap_or_else(|e| e.into_inner()) = Some(sealer);
    }

    pub fn sealer(&self) -> Option<Arc<dyn Sealer>> {
        self.sealer
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// The tracking of a write that tracks nothing: what `apply_checked` passes.
    pub(crate) fn empty() -> &'static Tracking {
        static EMPTY: Tracking = Tracking::new();
        &EMPTY
    }

    pub fn get(&self, model: &str) -> Option<Arc<Track>> {
        self.models
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(model)
            .cloned()
    }

    pub fn contains(&self, model: &str) -> bool {
        self.models
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .contains_key(model)
    }

    pub fn is_empty(&self) -> bool {
        self.models
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .is_empty()
    }

    pub fn set(&self, model: &str, how: Track) {
        self.models
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(model.to_string(), Arc::new(how));
    }

    pub fn remove(&self, model: &str) {
        self.models
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(model);
    }

    /// The tracked models, sorted.
    pub fn models(&self) -> Vec<String> {
        self.models
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .keys()
            .cloned()
            .collect()
    }
}

/// See the module documentation.
pub struct Tracked {
    inner: Arc<dyn Backend>,
    tracking: Tracking,
}

impl Tracked {
    pub fn new(inner: Arc<dyn Backend>) -> Arc<Tracked> {
        Arc::new(Tracked {
            inner,
            tracking: Tracking::new(),
        })
    }

    /// `new`, as the trait object `open` returns.
    pub fn wrap(inner: Arc<dyn Backend>) -> Arc<dyn Backend> {
        Self::new(inner)
    }

    pub fn tracking(&self) -> &Tracking {
        &self.tracking
    }

    fn any_tracked<'a>(&self, models: impl IntoIterator<Item = &'a str>) -> bool {
        !self.tracking.is_empty() && models.into_iter().any(|m| self.tracking.contains(m))
    }

    /// The first op that writes a pseudo-model, as the refusal of a batch.
    fn pseudo_op(ops: &[Op]) -> Option<ApplyError> {
        ops.iter().enumerate().find_map(|(index, op)| {
            is_pseudo(op.model()).then(|| ApplyError::Conflict {
                index,
                reason: pseudo_refusal(op.model()),
            })
        })
    }

    /// A batch through the old methods, routed to `apply_ctx` when it touches a tracked model.
    fn routed(&self, ops: &[Op]) -> Option<std::result::Result<(), ApplyError>> {
        self.any_tracked(ops.iter().map(Op::model))
            .then(|| self.apply_ctx(&ctx::current_or_system(), ops))
    }
}

impl Backend for Tracked {
    fn insert(&self, model: &str, id: &str, fields: &Fields) -> Result<()> {
        if is_pseudo(model) {
            return Err(pseudo_refusal(model));
        }
        if self.any_tracked([model]) {
            let op = Op::Insert {
                model: model.into(),
                id: id.into(),
                fields: fields.clone(),
            };
            return self
                .apply_ctx(&ctx::current_or_system(), &[op])
                .map_err(|e| match e {
                    ApplyError::Conflict { .. } => format!("duplicate {model} {id}"),
                    ApplyError::Storage(m) => m,
                });
        }
        self.inner.insert(model, id, fields)
    }
    fn get(&self, model: &str, id: &str) -> Result<Option<Fields>> {
        self.inner.get(model, id)
    }
    fn list(&self, model: &str) -> Result<Vec<(String, Fields)>> {
        self.inner.list(model)
    }
    fn scan(
        &self,
        model: &str,
        visit: &mut dyn FnMut(&str, &Fields) -> Result<bool>,
    ) -> Result<()> {
        self.inner.scan(model, visit)
    }
    fn update(&self, model: &str, id: &str, patch: &Fields) -> Result<bool> {
        if is_pseudo(model) {
            return Err(pseudo_refusal(model));
        }
        if self.any_tracked([model]) {
            let op = Op::Merge {
                model: model.into(),
                id: id.into(),
                patch: patch.clone(),
            };
            return match self.apply_ctx(&ctx::current_or_system(), &[op]) {
                Ok(()) => Ok(true),
                // The only step of the batch was refused: there is no such record.
                Err(ApplyError::Conflict { .. }) => Ok(false),
                Err(ApplyError::Storage(m)) => Err(m),
            };
        }
        self.inner.update(model, id, patch)
    }
    fn count(&self, model: &str) -> Result<usize> {
        self.inner.count(model)
    }
    fn delete(&self, model: &str, id: &str) -> Result<bool> {
        if is_pseudo(model) {
            return Err(pseudo_refusal(model));
        }
        if self.any_tracked([model]) {
            let op = Op::Delete {
                model: model.into(),
                id: id.into(),
            };
            return match self.apply_ctx(&ctx::current_or_system(), &[op]) {
                Ok(()) => Ok(true),
                Err(ApplyError::Conflict { .. }) => Ok(false),
                Err(ApplyError::Storage(m)) => Err(m),
            };
        }
        self.inner.delete(model, id)
    }
    fn insert_many(&self, rows: &[(String, String, Fields)]) -> Result<()> {
        if let Some((m, _, _)) = rows.iter().find(|(m, _, _)| is_pseudo(m)) {
            return Err(pseudo_refusal(m));
        }
        if self.any_tracked(rows.iter().map(|(m, _, _)| m.as_str())) {
            let ops: Vec<Op> = rows
                .iter()
                .map(|(model, id, fields)| Op::Insert {
                    model: model.clone(),
                    id: id.clone(),
                    fields: fields.clone(),
                })
                .collect();
            return self
                .apply_ctx(&ctx::current_or_system(), &ops)
                .map_err(|e| match e {
                    ApplyError::Conflict { index, .. } => {
                        let (m, i, _) = &rows[index.min(rows.len() - 1)];
                        format!("duplicate {m} {i}")
                    }
                    ApplyError::Storage(m) => m,
                });
        }
        self.inner.insert_many(rows)
    }
    fn apply(&self, ops: &[Op]) -> Result<()> {
        if let Some(e) = Self::pseudo_op(ops) {
            return Err(e.to_string());
        }
        match self.routed(ops) {
            Some(r) => r.map_err(|e| e.to_string()),
            None => self.inner.apply(ops),
        }
    }
    fn apply_checked(&self, ops: &[Op]) -> std::result::Result<(), ApplyError> {
        if let Some(e) = Self::pseudo_op(ops) {
            return Err(e);
        }
        match self.routed(ops) {
            Some(r) => r,
            None => self.inner.apply_checked(ops),
        }
    }
    fn apply_ctx(&self, ctx: &WriteCtx, ops: &[Op]) -> std::result::Result<(), ApplyError> {
        if let Some(e) = Self::pseudo_op(ops) {
            return Err(e);
        }
        self.inner.apply_tracked(ctx, ops, &self.tracking)
    }
    fn apply_tracked(
        &self,
        ctx: &WriteCtx,
        ops: &[Op],
        tracking: &Tracking,
    ) -> std::result::Result<(), ApplyError> {
        if let Some(e) = Self::pseudo_op(ops) {
            return Err(e);
        }
        self.inner.apply_tracked(ctx, ops, tracking)
    }
    fn track(&self, model: &str, how: Track) {
        self.tracking.set(model, how);
    }
    fn untrack(&self, model: &str) {
        self.tracking.remove(model);
    }
    fn tracked_models(&self) -> Vec<String> {
        self.tracking.models()
    }
    fn set_sealer(&self, sealer: Arc<dyn Sealer>) {
        self.tracking.set_sealer(sealer);
    }
    fn sealer(&self) -> Option<Arc<dyn Sealer>> {
        self.tracking.sealer()
    }
    fn history(&self, q: &HistoryQuery) -> Result<Vec<ChangeRow>> {
        self.inner.history(q)
    }
    fn backfill_sysfields(
        &self,
        models: &[String],
        fallback_at: &str,
        batch: usize,
    ) -> Result<Backfilled> {
        self.inner.backfill_sysfields(models, fallback_at, batch)
    }
    fn find(&self, model: &str, field: &str, value: &J) -> Result<Vec<(String, Fields)>> {
        self.inner.find(model, field, value)
    }
    fn ensure_unique(&self, name: &str, model: &str, fields: &[&str]) -> Result<()> {
        self.inner.ensure_unique(name, model, fields)
    }
    fn kind(&self) -> &'static str {
        self.inner.kind()
    }

    fn lock(&self, key: &str) -> Result<LockGuard> {
        self.inner.lock(key)
    }

    fn counter_add(&self, key: &str, ttl_ms: u64, extend: bool) -> Result<u64> {
        self.inner.counter_add(key, ttl_ms, extend)
    }
    fn counter_get(&self, key: &str) -> Result<u64> {
        self.inner.counter_get(key)
    }
    fn counter_ttl_ms(&self, key: &str) -> Result<Option<u64>> {
        self.inner.counter_ttl_ms(key)
    }
    fn counter_reset(&self, key: &str) -> Result<()> {
        self.inner.counter_reset(key)
    }

    fn kv_put(&self, key: &str, value: &str, ttl_ms: Option<u64>) -> Result<()> {
        self.inner.kv_put(key, value, ttl_ms)
    }
    fn kv_put_new(&self, key: &str, value: &str, ttl_ms: Option<u64>) -> Result<bool> {
        self.inner.kv_put_new(key, value, ttl_ms)
    }
    fn kv_get(&self, key: &str) -> Result<Option<String>> {
        self.inner.kv_get(key)
    }
    fn kv_delete(&self, key: &str) -> Result<bool> {
        self.inner.kv_delete(key)
    }
    fn kv_scan(&self, prefix: &str) -> Result<Vec<(String, String)>> {
        self.inner.kv_scan(prefix)
    }
    fn purge_expired(&self) -> Result<usize> {
        self.inner.purge_expired()
    }

    fn export(&self, batch: usize, sink: &mut RowSink<'_>) -> Result<()> {
        self.inner.export(batch, sink)
    }
    fn total(&self) -> Result<usize> {
        self.inner.total()
    }

    fn swap_fields(&self, swaps: &[Swap]) -> Result<usize> {
        self.inner.swap_fields(swaps)
    }
    fn kv_swap(&self, key: &str, old: &str, new: &str) -> Result<bool> {
        self.inner.kv_swap(key, old, new)
    }

    fn import(
        &self,
        wipe: bool,
        next: &mut dyn FnMut() -> Result<Option<Vec<Row>>>,
    ) -> Result<usize> {
        self.inner.import(wipe, next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::Memory;
    use crate::testing::Flaky;
    use serde_json::json;

    fn fields(v: serde_json::Value) -> Fields {
        v.as_object().cloned().unwrap()
    }

    /// A `Tracked` over a recording `Flaky` over memory.
    fn tracked() -> (Arc<Tracked>, Arc<Flaky>) {
        let flaky = Flaky::new(Arc::new(Memory::default()));
        (Tracked::new(flaky.clone()), flaky)
    }

    // ---- the lint: every method of the trait is forwarded -------------------------------------

    /// The names of the methods (`fn name`, four spaces in) of the block that starts at the line
    /// `header` and ends at the next line that is a lone `}`.
    fn fns_of(src: &str, header: &str) -> Vec<String> {
        let start = src
            .lines()
            .position(|l| l.starts_with(header))
            .unwrap_or_else(|| panic!("`{header}` is not in the source"));
        src.lines()
            .skip(start + 1)
            .take_while(|l| *l != "}")
            .filter_map(|l| l.strip_prefix("    fn "))
            .map(|rest| {
                rest.chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect()
            })
            .collect()
    }

    const TRAIT: &str = "pub trait Backend: Send + Sync {";

    /// The methods of the trait in `lib_src`.
    fn backend_fns(lib_src: &str) -> Vec<String> {
        let all = fns_of(lib_src, TRAIT);
        assert!(all.len() >= 30, "the scan of the trait found {all:?}");
        all
    }

    /// The methods of the trait in `lib_src` that the `impl` at `header` in `src` does not have.
    fn missing_in(lib_src: &str, src: &str, header: &str) -> Vec<String> {
        let have = fns_of(src, header);
        assert!(!have.is_empty(), "no methods found in `{header}`");
        backend_fns(lib_src)
            .into_iter()
            .filter(|f| !have.contains(f))
            .collect()
    }

    /// The methods of the real trait that the `impl` at `header` in `src` does not have.
    fn missing_forwards(src: &str, header: &str) -> Vec<String> {
        missing_in(include_str!("lib.rs"), src, header)
    }

    #[test]
    fn tracked_forwards_every_backend_method() {
        let missing = missing_forwards(include_str!("tracked.rs"), "impl Backend for Tracked {");
        assert!(
            missing.is_empty(),
            "Tracked does not forward {missing:?}: the method would silently use the trait's \
             default (add it to `impl Backend for Tracked`)"
        );
    }

    #[test]
    fn the_other_wrappers_forward_every_backend_method_too() {
        for (src, header) in [
            (include_str!("guard.rs"), "impl Backend for Guarded {"),
            (include_str!("testing.rs"), "impl Backend for Flaky {"),
            // The test double of obop-ops wraps stores that `open` returns (Tracked ones), so a
            // method it does not forward is silently the trait's default for its tests.
            (
                include_str!("../../obop-ops/src/testkit.rs"),
                "impl Backend for Flaky {",
            ),
        ] {
            let missing = missing_forwards(src, header);
            assert!(
                missing.is_empty(),
                "`{header}` does not forward {missing:?}: the method would silently use the \
                 trait's default"
            );
        }
    }

    #[test]
    fn the_lint_notices_a_method_that_is_not_forwarded() {
        // The lint is only worth something if it fails when a forward is missing: take the real
        // text of this file and remove one forwarder, then ask the same question.
        let src = include_str!("tracked.rs");
        let cut = src.replacen(
            "    fn kv_scan(&self, prefix: &str) -> Result<Vec<(String, String)>> {\n        self.inner.kv_scan(prefix)\n    }\n",
            "",
            1,
        );
        assert_ne!(src, cut, "the forwarder to remove was not found");
        assert_eq!(
            missing_forwards(&cut, "impl Backend for Tracked {"),
            ["kv_scan"]
        );
        // And a method added to the trait (with a default, as every new one is) is named for
        // every wrapper that has not been taught about it.
        let with_new = include_str!("lib.rs").replacen(
            "    fn kind(&self) -> &'static str;\n",
            "    fn kind(&self) -> &'static str;\n    fn brand_new(&self) {}\n",
            1,
        );
        assert_ne!(
            with_new,
            include_str!("lib.rs"),
            "the anchor line was not found"
        );
        for (src, header) in [
            (include_str!("tracked.rs"), "impl Backend for Tracked {"),
            (include_str!("guard.rs"), "impl Backend for Guarded {"),
            (include_str!("testing.rs"), "impl Backend for Flaky {"),
        ] {
            assert_eq!(
                missing_in(&with_new, src, header),
                ["brand_new"],
                "{header}"
            );
        }
    }

    #[test]
    fn only_tracked_implements_apply_ctx() {
        // A store whose `apply_ctx` and `apply_checked` call each other would never return.
        for (name, src) in [
            ("memory.rs", include_str!("memory.rs")),
            ("sqlite.rs", include_str!("sqlite.rs")),
            ("pg.rs", include_str!("pg.rs")),
        ] {
            assert!(
                !src.contains("fn apply_ctx"),
                "{name} must not implement apply_ctx"
            );
            assert!(
                src.contains("fn apply_tracked"),
                "{name} implements the write as apply_tracked"
            );
        }
    }

    // ---- behaviour ---------------------------------------------------------------------------

    #[test]
    fn tracking_is_a_list_of_models() {
        let t = Tracking::new();
        assert!(t.is_empty() && t.models().is_empty());
        t.set("b", Track::default());
        t.set(
            "a",
            Track {
                history: true,
                retention_days: Some(30),
                ..Track::default()
            },
        );
        assert_eq!(t.models(), ["a", "b"]);
        assert!(t.contains("a") && !t.contains("c"));
        assert_eq!(t.get("a").unwrap().retention_days, Some(30));
        t.remove("a");
        assert_eq!(t.models(), ["b"]);
        assert!(Tracking::empty().is_empty());
    }

    #[test]
    fn writes_to_untracked_models_do_not_use_the_context_path() {
        let (t, flaky) = tracked();
        t.insert("m", "a", &fields(json!({"n": 1}))).unwrap();
        assert!(t.update("m", "a", &fields(json!({"n": 2}))).unwrap());
        t.apply_checked(&[Op::add("m", "c", "n", 1).or_create(Fields::new())])
            .unwrap();
        t.insert_many(&[("m".into(), "b".into(), Fields::new())])
            .unwrap();
        assert!(t.delete("m", "b").unwrap());
        assert_eq!(t.get("m", "a").unwrap().unwrap()["n"], 2);
        assert!(flaky.seen_writes().is_empty(), "nothing is tracked");
        assert!(t.tracked_models().is_empty());
    }

    #[test]
    fn apply_ctx_hands_the_context_and_the_tracking_to_the_store() {
        let (t, flaky) = tracked();
        t.track("m", Track::default());
        let c = WriteCtx::new("ann", "rest", "m.create")
            .batch("b1")
            .ticket("m", "a", "post");
        let op = Op::Insert {
            model: "m".into(),
            id: "a".into(),
            fields: fields(json!({"n": 1})),
        };
        t.apply_ctx(&c, std::slice::from_ref(&op)).unwrap();
        assert_eq!(t.get("m", "a").unwrap().unwrap()["n"], 1);
        let seen = flaky.seen_writes();
        assert_eq!(seen.len(), 1);
        assert_eq!((&seen[0].0, &seen[0].1[..]), (&c, &[op][..]));
        assert_eq!(seen[0].2, ["m"], "the store sees which models are tracked");
        // A refused step is reported as one, nothing changed.
        let again = Op::Insert {
            model: "m".into(),
            id: "a".into(),
            fields: Fields::new(),
        };
        assert!(matches!(
            t.apply_ctx(&c, &[again]),
            Err(ApplyError::Conflict { index: 0, .. })
        ));
    }

    #[test]
    fn the_old_methods_on_a_tracked_model_go_through_apply_ctx_with_the_ambient_context() {
        let (t, flaky) = tracked();
        t.track("m", Track::default());
        {
            let _s = ctx::scope(WriteCtx::new("ann", "mcp", "m.update"));
            t.insert("m", "a", &fields(json!({"n": 1}))).unwrap();
            assert!(t.update("m", "a", &fields(json!({"k": 2}))).unwrap());
            assert!(
                !t.update("m", "ghost", &Fields::new()).unwrap(),
                "no such record"
            );
            assert!(
                t.insert("m", "a", &Fields::new())
                    .unwrap_err()
                    .contains("duplicate")
            );
            t.insert_many(&[
                ("m".into(), "b".into(), Fields::new()),
                ("other".into(), "x".into(), Fields::new()),
            ])
            .unwrap();
            let e = t
                .insert_many(&[
                    ("m".into(), "c".into(), Fields::new()),
                    ("m".into(), "b".into(), Fields::new()),
                ])
                .unwrap_err();
            assert_eq!(e, "duplicate m b");
            assert!(
                t.get("m", "c").unwrap().is_none(),
                "the batch is all or nothing"
            );
            t.apply_checked(&[Op::Merge {
                model: "m".into(),
                id: "a".into(),
                patch: fields(json!({"z": 3})),
            }])
            .unwrap();
            t.apply(&[Op::Delete {
                model: "m".into(),
                id: "b".into(),
            }])
            .unwrap();
            assert!(t.delete("m", "a").unwrap());
            assert!(!t.delete("m", "a").unwrap());
        }
        let seen = flaky.seen_writes();
        assert_eq!(seen.len(), 10, "every one of them: {seen:?}");
        assert!(
            seen.iter()
                .all(|(c, _, _)| c.by == "ann" && c.via.surface == "mcp")
        );
        // Outside a scope the system writes.
        t.insert("m", "s", &Fields::new()).unwrap();
        let last = flaky.seen_writes().pop().unwrap().0;
        assert_eq!(
            (last.by.as_str(), last.via.surface.as_str()),
            ("system:internal", "system")
        );
        // Untracking sends the model back to the plain path.
        t.untrack("m");
        let n = flaky.seen_writes().len();
        t.insert("m", "u", &Fields::new()).unwrap();
        assert_eq!(flaky.seen_writes().len(), n);
    }

    #[test]
    fn a_pseudo_model_cannot_be_written_as_a_record() {
        let (t, _flaky) = tracked();
        let row = || ("record_change".to_string(), "1".to_string(), Fields::new());
        let ins = Op::Insert {
            model: "record_change".into(),
            id: "1".into(),
            fields: Fields::new(),
        };
        let wrote = [
            t.insert("record_change", "1", &Fields::new())
                .map(|()| true),
            t.update("record_change", "1", &Fields::new()),
            t.delete("record_change", "1"),
            t.insert_many(&[row()]).map(|()| true),
            t.apply(std::slice::from_ref(&ins)).map(|()| true),
        ];
        for r in wrote {
            assert!(r.unwrap_err().contains("reserved model name"));
        }
        for r in [
            t.apply_checked(std::slice::from_ref(&ins)),
            t.apply_ctx(&WriteCtx::system("x"), std::slice::from_ref(&ins)),
            t.apply_tracked(
                &WriteCtx::system("x"),
                std::slice::from_ref(&ins),
                Tracking::empty(),
            ),
        ] {
            assert!(
                matches!(r, Err(ApplyError::Conflict { index: 0, .. })),
                "{r:?}"
            );
        }
        // A batch that names it anywhere is refused whole, with the position of the step.
        let mixed = [
            Op::Insert {
                model: "fine".into(),
                id: "1".into(),
                fields: Fields::new(),
            },
            ins,
        ];
        assert!(matches!(
            t.apply_checked(&mixed),
            Err(ApplyError::Conflict { index: 1, .. })
        ));
        assert!(t.get("fine", "1").unwrap().is_none());
        // Reading it is allowed and finds nothing; so is writing a model that merely starts alike.
        assert!(t.get("record_change", "1").unwrap().is_none());
        assert_eq!(t.count("record_change").unwrap(), 0);
        t.insert("record_changes", "1", &Fields::new()).unwrap();
        // `import` is how those rows come back from a backup, and is not refused here: a row of
        // that name is an entry of the change history, and anything else is refused.
        let entry = crate::sysfields::ChangeRow {
            seq: 1,
            at: crate::sysfields::fmt_micros(1_000_000),
            model: "m".into(),
            record_id: "r".into(),
            op: "create".into(),
            actor: "ann".into(),
            via: Fields::new(),
            row_version: 1,
            changed: Fields::new(),
            sealed: false,
            batch_id: None,
            run_id: None,
            txid: 0,
        };
        let mut rows = Some(vec![(
            "record_change".to_string(),
            "1".to_string(),
            entry.to_fields(),
        )]);
        assert_eq!(t.import(false, &mut || Ok(rows.take())).unwrap(), 1);
        let kept = t.history(&crate::HistoryQuery::of("m", "r", 10)).unwrap();
        assert_eq!(kept, [entry]);
        // A row of that name with the marker that is not an entry is refused, not taken for a
        // record; one without the marker is an ordinary record, as it was before the history.
        let mut marked = Some(vec![(
            "record_change".to_string(),
            "2".to_string(),
            f_marked(),
        )]);
        assert!(t.import(false, &mut || Ok(marked.take())).is_err());
        let mut plain = Some(vec![row()]);
        assert_eq!(t.import(false, &mut || Ok(plain.take())).unwrap(), 1);
        assert!(t.get("record_change", "1").unwrap().is_some());
    }

    /// A row of the backup stream that claims to be an entry and is not one.
    fn f_marked() -> Fields {
        let mut f = Fields::new();
        f.insert(
            crate::sysfields::ENTRY_KEY.into(),
            J::from(crate::sysfields::ENTRY_KIND),
        );
        f
    }

    #[test]
    fn the_wrapper_is_what_open_returns_and_it_changes_nothing_else() {
        let b = crate::open("memory:").unwrap();
        assert_eq!(b.kind(), "memory");
        b.track("t", Track::default());
        assert_eq!(b.tracked_models(), ["t"]);
        let g = b.lock("k").unwrap();
        drop(g);
        assert_eq!(b.counter_add("c", 0, false).unwrap(), 1);
        // Both write entry points of the stores work through the wrapper without calling
        // themselves: the stores define `apply_checked` as `apply_tracked` of the system.
        b.apply_checked(&[Op::Insert {
            model: "t".into(),
            id: "1".into(),
            fields: Fields::new(),
        }])
        .unwrap();
        b.apply_ctx(
            &WriteCtx::new("ann", "cli", "x"),
            &[Op::Merge {
                model: "t".into(),
                id: "1".into(),
                patch: fields(json!({"a": 1})),
            }],
        )
        .unwrap();
        assert_eq!(b.get("t", "1").unwrap().unwrap()["a"], 1);
    }

    #[test]
    fn the_stores_take_the_context_without_calling_themselves() {
        let path = std::env::temp_dir().join(format!("obop-tracked-{}.db", std::process::id()));
        let url = format!("sqlite://{}", path.display());
        for url in ["memory:", "sqlite::memory:", url.as_str()] {
            let b = crate::open(url).unwrap();
            let c = WriteCtx::new("ann", "rest", "x").ticket("m", "1", "post");
            b.apply_ctx(
                &c,
                &[Op::Insert {
                    model: "m".into(),
                    id: "1".into(),
                    fields: Fields::new(),
                }],
            )
            .unwrap();
            assert!(b.get("m", "1").unwrap().is_some(), "{url}");
        }
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
    }
}
