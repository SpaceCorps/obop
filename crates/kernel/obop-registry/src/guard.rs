//! The immutability guard of a model (D77, decision part 1, `guard_level = command_only`): the
//! builder methods for modules written in Rust and the install gate of `[model.guard]`.
//!
//! The types are in `ext.rs` (`Guard`, `StateSet`, `Exit`, `LockDate`). The command layer checks
//! them on every write a unit makes (`obop_commands` `engine/frozen.rs`); the database trigger,
//! the guard rows and the role split of D77 are later packages.
//!
//! A record is frozen when its state field is in `immutable_when`, when its lock-date field falls
//! on or before the lock date (or inside a closed period), or when the parent it names in
//! `children_of` is frozen. A frozen record changes only in `mutable_fields`, or along an
//! `exit_to` edge by its action (with that edge's `may_change`), and is never deleted.

use crate::ext::Exit;
use crate::{FieldKind, Guard, LockDate, ModelDef, Module, StateSet, Violation, intern};
use std::sync::Arc;

/// How many parents `children_of` may chain through (a deeper chain counts as frozen).
pub const MAX_GUARD_DEPTH: usize = 4;

/// The options a `closed_by` model of periods must have.
pub const CLOSED_OPTION: &str = "closed";

fn interned(v: &[&str]) -> Vec<&'static str> {
    v.iter().map(|s| intern(s)).collect()
}

// ---- builders for modules written in Rust ---------------------------------------------------

impl Guard {
    /// A guard that freezes the records whose `field` is one of `values`.
    pub fn frozen_in(field: &str, values: &[&str]) -> Self {
        Self {
            immutable_when: Some(StateSet {
                field: intern(field),
                values: interned(values),
            }),
            ..Self::default()
        }
    }

    /// A guard that freezes a record while the parent named in the ref `field` is frozen.
    pub fn child_of(field: &str) -> Self {
        Self::default().children_of(field)
    }

    /// Records can be created and read, never changed or deleted.
    pub fn append_only() -> Self {
        Self {
            append_only: true,
            ..Self::default()
        }
    }

    /// `mutable_fields`: stored fields that stay editable on a frozen record.
    pub fn mutable(mut self, fields: &[&str]) -> Self {
        self.mutable_fields = interned(fields);
        self
    }

    /// One `exit_to` edge: the declared action that may move a frozen record to `to`, and the
    /// fields it may change on the way.
    pub fn exit(mut self, to: &str, action: &str, may_change: &[&str]) -> Self {
        self.exit_to.push(Exit {
            to: intern(to),
            action: intern(action),
            may_change: interned(may_change),
        });
        self
    }

    /// `no_delete_when`: the records in these states cannot be deleted (frozen or not).
    pub fn no_delete_in(mut self, field: &str, values: &[&str]) -> Self {
        self.no_delete_when = Some(StateSet {
            field: intern(field),
            values: interned(values),
        });
        self
    }

    /// `lock_date = { field, setting }`: records dated on or before the date in the instance
    /// setting `setting` (a dotted key, `accounting.lock_date`) are frozen.
    pub fn lock_by_setting(mut self, field: &str, setting: &str) -> Self {
        self.lock_date = Some(LockDate {
            field: intern(field),
            setting: Some(intern(setting)),
            closed_by: None,
        });
        self
    }

    /// `lock_date = { field, closed_by }`: records dated inside a closed period of `model` are
    /// frozen.
    pub fn lock_by_closed(mut self, field: &str, model: &str) -> Self {
        self.lock_date = Some(LockDate {
            field: intern(field),
            setting: None,
            closed_by: Some(intern(model)),
        });
        self
    }

    /// `children_of`: a ref field of this model; the record is frozen while its parent is.
    pub fn children_of(mut self, field: &str) -> Self {
        self.children_of = Some(intern(field));
        self
    }

    /// `message`: what an interface says about a frozen record.
    pub fn message(mut self, text: &str) -> Self {
        self.message = Some(intern(text));
        self
    }

    /// Whether the guard declares no rule (a `message` alone is no rule).
    pub fn is_empty(&self) -> bool {
        Self {
            message: None,
            ..self.clone()
        } == Self::default()
    }
}

impl ModelDef {
    /// `[model.guard]` (D77).
    pub fn with_guard(mut self, guard: Guard) -> Self {
        self.ext.guard = (guard != Guard::default()).then_some(guard);
        self
    }

    /// The guard, if the model declares one.
    pub fn guard(&self) -> Option<&Guard> {
        self.ext.guard.as_ref()
    }
}

// ---- the gate -------------------------------------------------------------------------------

type Lookup<'a> = &'a dyn Fn(&str) -> Option<Arc<ModelDef>>;

struct Gate<'a> {
    module: &'a Module,
    lookup: Lookup<'a>,
    strict: bool,
    out: Vec<Violation>,
}

impl Gate<'_> {
    fn bad(&mut self, model: &ModelDef, key: &str, problem: String) {
        self.out.push(Violation {
            path: format!("{}.{}.guard.{key}", self.module.id, model.name),
            problem,
        });
    }

    fn model(&self, name: &str) -> Option<Arc<ModelDef>> {
        self.module
            .models
            .iter()
            .find(|m| m.name == name)
            .cloned()
            .map(Arc::new)
            .or_else(|| (self.lookup)(name))
    }
}

/// Is `name` a stored field of `m` that a person may edit: declared, not computed, not a number
/// the system assigns, not governed by an action.
fn stored_plain(m: &ModelDef, name: &str) -> Result<(), String> {
    let Some(f) = m.fields.iter().find(|f| f.name == name) else {
        return Err(format!("{} has no field {name}", m.name));
    };
    if f.computed.is_some() {
        return Err(format!("{name} is computed"));
    }
    if f.sequence.is_some() {
        return Err(format!("{name} is a number the system assigns"));
    }
    if m.governed_fields().contains(&f.name) {
        return Err(format!("{name} moves only through actions"));
    }
    Ok(())
}

/// The problems of a set of states: a governed select of the model, values among its options and
/// (`not_first`) none of them the first option, so that a record is born unfrozen.
fn states_problem(m: &ModelDef, s: &StateSet, not_first: bool) -> Option<String> {
    let Some(f) = m.fields.iter().find(|f| f.name == s.field) else {
        return Some(format!("{} has no field {}", m.name, s.field));
    };
    let FieldKind::Select(opts) = &f.kind else {
        return Some(format!("{} is not a select", s.field));
    };
    if !m.governed_fields().contains(&f.name) {
        return Some(format!(
            "{} is not moved by any action of {}: a guard follows a governed state",
            s.field, m.name
        ));
    }
    if s.values.is_empty() {
        return Some("name at least one state".into());
    }
    for v in &s.values {
        if !opts.contains(v) {
            return Some(format!("{v:?} is not an option of {}", s.field));
        }
        if not_first && opts.first() == Some(v) {
            return Some(format!(
                "{v:?} is the first option of {}, where every record starts: a record would be born frozen",
                s.field
            ));
        }
    }
    None
}

/// Is `s` a dotted setting key: one to four lowercase words.
fn setting_ok(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() <= 4
        && parts.iter().all(|p| {
            let mut c = p.chars();
            c.next().is_some_and(|f| f.is_ascii_lowercase())
                && c.all(|x| x.is_ascii_lowercase() || x.is_ascii_digit() || x == '_')
        })
}

/// What a `closed_by` model lacks to be a model of periods, if anything.
pub fn periods_problem(p: &ModelDef) -> Option<String> {
    for f in ["start_on", "end_on"] {
        match p.fields.iter().find(|x| x.name == f) {
            Some(x) if x.kind == FieldKind::Date => {}
            _ => return Some(format!("{} needs a date field {f}", p.name)),
        }
    }
    if closed_field(p).is_none() {
        return Some(format!(
            "{} needs a select field with the option \"{CLOSED_OPTION}\"",
            p.name
        ));
    }
    None
}

/// The select field of a model of periods that says a period is closed: a governed one with the
/// option `closed` first, else any select with it.
pub fn closed_field(p: &ModelDef) -> Option<&'static str> {
    let has = |f: &&crate::FieldDef| f.kind.options().iter().any(|o| *o == CLOSED_OPTION);
    let governed = p.governed_fields();
    p.fields
        .iter()
        .filter(has)
        .find(|f| governed.contains(&f.name))
        .or_else(|| p.fields.iter().find(has))
        .map(|f| f.name)
}

/// The install gate of `[model.guard]` (D77). `strict`: a model outside the module that a key
/// names must be known through `lookup` (the install of a package with its dependencies).
pub fn check(m: &Module, lookup: Lookup<'_>, strict: bool) -> Vec<Violation> {
    let mut g = Gate {
        module: m,
        lookup,
        strict,
        out: Vec::new(),
    };
    for model in &m.models {
        if let Some(guard) = &model.ext.guard {
            check_model(&mut g, model, guard);
        }
    }
    g.out
}

fn check_model(g: &mut Gate<'_>, m: &ModelDef, guard: &Guard) {
    if let Some(t) = guard.message
        && let Err(p) = crate::labels::text_ok(t, crate::ui::MAX_HINT_TEXT)
    {
        g.bad(m, "message", format!("the message {p}"));
    }
    if guard.is_empty() {
        g.bad(m, "", "declare at least one rule".into());
        return;
    }
    if guard.append_only {
        let alone = Guard {
            append_only: true,
            message: guard.message,
            ..Guard::default()
        };
        if guard != &alone {
            g.bad(
                m,
                "append_only",
                "append_only stands alone: a record that never changes needs no other rule".into(),
            );
        }
        return;
    }
    if let Some(s) = &guard.immutable_when
        && let Some(p) = states_problem(m, s, true)
    {
        g.bad(m, "immutable_when", p);
    }
    if let Some(s) = &guard.no_delete_when
        && let Some(p) = states_problem(m, s, false)
    {
        g.bad(m, "no_delete_when", p);
    }
    let lock_field = guard.lock_date.as_ref().map(|l| l.field);
    for f in &guard.mutable_fields {
        if let Err(p) = stored_plain(m, f) {
            g.bad(m, "mutable_fields", p);
        } else if Some(*f) == lock_field || Some(*f) == guard.children_of {
            g.bad(
                m,
                "mutable_fields",
                format!("{f} decides whether the record is frozen and cannot stay editable"),
            );
        }
    }
    check_exits(g, m, guard);
    if let Some(l) = &guard.lock_date {
        check_lock(g, m, l);
    }
    if let Some(c) = guard.children_of {
        check_parent(g, m, c);
    }
}

fn check_exits(g: &mut Gate<'_>, m: &ModelDef, guard: &Guard) {
    let Some(frozen) = &guard.immutable_when else {
        if !guard.exit_to.is_empty() {
            g.bad(
                m,
                "exit_to",
                "an exit leaves an immutable state: declare immutable_when".into(),
            );
        }
        return;
    };
    for e in &guard.exit_to {
        let t = m
            .commands
            .iter()
            .find(|c| c.name == e.action)
            .and_then(|c| c.transition);
        match t {
            None => g.bad(
                m,
                "exit_to",
                format!(
                    "{} is not a declared action of {} that moves a state",
                    e.action, m.name
                ),
            ),
            Some(t) if t.field != frozen.field || t.to != e.to => g.bad(
                m,
                "exit_to",
                format!(
                    "{} moves {} to {:?}, not {} to {:?}",
                    e.action, t.field, t.to, frozen.field, e.to
                ),
            ),
            Some(t) if t.from.is_some_and(|f| !frozen.values.contains(&f)) => g.bad(
                m,
                "exit_to",
                format!(
                    "{} starts in {:?}, which is not an immutable state",
                    e.action,
                    t.from.unwrap_or_default()
                ),
            ),
            Some(_) => {}
        }
        for f in &e.may_change {
            if let Err(p) = stored_plain(m, f) {
                g.bad(m, "exit_to", format!("{}: may_change: {p}", e.action));
            }
        }
    }
    // Completeness: every action that can start in a frozen state is an exit, so that no action
    // is refused at run time for a reason the package could have stated.
    for (c, t) in m.transitions() {
        if t.field != frozen.field {
            continue;
        }
        let starts_frozen = t.from.is_none_or(|f| frozen.values.contains(&f));
        if starts_frozen && !guard.exit_to.iter().any(|e| e.action == c.name) {
            g.bad(
                m,
                "exit_to",
                format!(
                    "{} can start in an immutable state of {}: list it in exit_to, or give it a from state that is not immutable",
                    c.name, frozen.field
                ),
            );
        }
    }
}

fn check_lock(g: &mut Gate<'_>, m: &ModelDef, l: &LockDate) {
    match m.fields.iter().find(|f| f.name == l.field) {
        Some(f) if f.kind == FieldKind::Date => {}
        Some(f) if f.kind == FieldKind::Datetime => g.bad(
            m,
            "lock_date",
            format!(
                "{} is a datetime; a lock date needs a date field (the UTC date of an instant can differ from the date in the platform zone)",
                l.field
            ),
        ),
        Some(_) => g.bad(m, "lock_date", format!("{} is not a date field", l.field)),
        None => g.bad(m, "lock_date", format!("{} has no field {}", m.name, l.field)),
    }
    match (l.setting, l.closed_by) {
        (Some(s), None) if !setting_ok(s) => g.bad(
            m,
            "lock_date",
            format!("setting {s:?} must be one to four dotted lowercase words"),
        ),
        (Some(_), None) => {}
        (None, Some(p)) => match g.model(p) {
            Some(pm) => {
                if let Some(problem) = periods_problem(&pm) {
                    g.bad(m, "lock_date", format!("closed_by: {problem}"));
                }
            }
            None if g.strict => g.bad(
                m,
                "lock_date",
                format!("closed_by: there is no model {p} in the module or its dependencies"),
            ),
            None => {}
        },
        _ => g.bad(
            m,
            "lock_date",
            "give exactly one of setting and closed_by".into(),
        ),
    }
}

fn check_parent(g: &mut Gate<'_>, m: &ModelDef, field: &str) {
    let Some(target) = m
        .fields
        .iter()
        .find(|f| f.name == field)
        .and_then(|f| f.kind.target())
    else {
        g.bad(
            m,
            "children_of",
            format!("{field} is not a ref field of {}", m.name),
        );
        return;
    };
    // The chain of parents: each guarded, at most MAX_GUARD_DEPTH hops, no cycle.
    let mut seen = vec![m.name];
    let mut at = target;
    for _ in 0..=MAX_GUARD_DEPTH {
        if seen.contains(&at) {
            g.bad(
                m,
                "children_of",
                format!("the parents of {} come back to {at}", m.name),
            );
            return;
        }
        let Some(pm) = g.model(at) else {
            if g.strict {
                g.bad(
                    m,
                    "children_of",
                    format!("there is no model {at} in the module or its dependencies"),
                );
            }
            return;
        };
        let Some(pg) = pm.guard() else {
            g.bad(
                m,
                "children_of",
                format!("{at} has no guard, so its children are never frozen by it"),
            );
            return;
        };
        seen.push(at);
        match pg.children_of.and_then(|c| {
            pm.fields
                .iter()
                .find(|f| f.name == c)
                .and_then(|f| f.kind.target())
        }) {
            Some(next) => at = next,
            None => return,
        }
    }
    g.bad(
        m,
        "children_of",
        format!("the chain of parents is longer than {MAX_GUARD_DEPTH}"),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CommandDef, FieldDef as F};
    use obop_types::DataClass;

    fn f(name: &'static str) -> F {
        F::new(name, "d", DataClass::None, false, "g.user")
    }

    fn order(guard: Guard) -> ModelDef {
        ModelDef {
            name: "gx_order",
            description: "d",
            read_group: "g.user",
            write_group: "g.user",
            access: None,
            fields: vec![
                f("name"),
                f("note"),
                f("date").kind(FieldKind::Date),
                f("at").datetime(),
                f("state").select(&["draft", "done", "cancelled"]),
                f("total").kind(FieldKind::Money).computed("1"),
            ],
            commands: vec![
                CommandDef::new("finish", "d", "g.user").transition("state", Some("draft"), "done"),
                CommandDef::new("cancel", "d", "g.user").transition("state", None, "cancelled"),
                CommandDef::new("reopen", "d", "g.user").transition(
                    "state",
                    Some("cancelled"),
                    "draft",
                ),
            ],
            ext: Default::default(),
        }
        .with_guard(guard)
    }

    fn line(guard: Guard) -> ModelDef {
        ModelDef {
            name: "gx_order_line",
            description: "d",
            read_group: "g.user",
            write_group: "g.user",
            access: None,
            fields: vec![f("order_id").reference("gx_order"), f("what")],
            commands: vec![],
            ext: Default::default(),
        }
        .with_guard(guard)
    }

    fn period() -> ModelDef {
        ModelDef {
            name: "gx_period",
            description: "d",
            read_group: "g.user",
            write_group: "g.user",
            access: None,
            fields: vec![
                f("start_on").kind(FieldKind::Date),
                f("end_on").kind(FieldKind::Date),
                f("state").select(&["open", "closed"]),
            ],
            commands: vec![],
            ext: Default::default(),
        }
    }

    fn module(models: Vec<ModelDef>) -> Module {
        Module {
            id: "gx".into(),
            name: "G".into(),
            models,
            network_destinations: vec![],
            description: String::new(),
            translations: Default::default(),
            meta: Default::default(),
        }
    }

    fn problems(models: Vec<ModelDef>) -> Vec<String> {
        check(&module(models), &|_| None, true)
            .into_iter()
            .map(|v| format!("{}: {}", v.path, v.problem))
            .collect()
    }

    fn good() -> Guard {
        Guard::frozen_in("state", &["done"])
            .mutable(&["note"])
            .exit("cancelled", "cancel", &["note"])
            .lock_by_closed("date", "gx_period")
    }

    #[test]
    fn a_complete_guard_passes() {
        let p = problems(vec![
            order(good()),
            line(Guard::child_of("order_id")),
            period(),
        ]);
        assert!(p.is_empty(), "{p:?}");
        let p = problems(vec![order(
            Guard::frozen_in("state", &["done"])
                .exit("cancelled", "cancel", &[])
                .lock_by_setting("date", "sales.lock_date"),
        )]);
        assert!(p.is_empty(), "{p:?}");
        assert!(problems(vec![order(Guard::append_only())]).is_empty());
    }

    #[test]
    fn each_wrong_key_is_named() {
        let cases: Vec<(Guard, &str)> = vec![
            (Guard::frozen_in("state", &["draft"]), "born frozen"),
            (Guard::frozen_in("state", &["gone"]), "not an option"),
            (Guard::frozen_in("name", &["x"]), "not a select"),
            (Guard::frozen_in("state", &["done"]), "list it in exit_to"),
            (
                Guard::frozen_in("state", &["done"])
                    .exit("cancelled", "cancel", &[])
                    .mutable(&["total"]),
                "computed",
            ),
            (
                Guard::frozen_in("state", &["done"])
                    .exit("cancelled", "cancel", &[])
                    .mutable(&["state"]),
                "moves only through actions",
            ),
            (
                Guard::frozen_in("state", &["done"])
                    .exit("cancelled", "cancel", &[])
                    .exit("draft", "reopen", &[]),
                "not an immutable state",
            ),
            (
                Guard::frozen_in("state", &["done"]).exit("cancelled", "finish", &[]),
                "moves state to \"done\"",
            ),
            (Guard::default().lock_by_setting("at", "x.lock"), "datetime"),
            (
                Guard::default().lock_by_setting("date", "Bad Key"),
                "dotted lowercase",
            ),
            (
                Guard::default().lock_by_closed("date", "gx_nothing"),
                "no model gx_nothing",
            ),
            (
                Guard::default().lock_by_closed("date", "gx_order"),
                "needs a date field start_on",
            ),
            (Guard::child_of("name"), "not a ref field"),
            (Guard::append_only().mutable(&["note"]), "stands alone"),
        ];
        for (guard, want) in cases {
            let p = problems(vec![order(guard.clone()), period()]);
            assert!(
                p.iter()
                    .any(|x| x.contains(want) && x.contains("gx.gx_order.guard.")),
                "{guard:?}: wanted {want:?}, got {p:?}"
            );
        }
        // A child of a parent without a guard is never frozen by it.
        let mut bare = order(Guard::default());
        bare.ext.guard = None;
        let p = problems(vec![bare, line(Guard::child_of("order_id"))]);
        assert!(p.iter().any(|x| x.contains("has no guard")), "{p:?}");
    }

    #[test]
    fn the_gate_runs_with_the_install_checks() {
        let m = module(vec![order(Guard::frozen_in("state", &["draft"]))]);
        assert!(
            crate::check_module(&m)
                .iter()
                .any(|v| v.path == "gx.gx_order.guard.immutable_when"),
        );
    }
}
