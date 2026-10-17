//! What an update does to existing records (D63).
//!
//! A new version of a package may add models, fields, options and actions on its own: records
//! stay as they are and the new parts start empty. A change that would leave a stored record
//! unable to satisfy the new shape is *breaking*: a removed or retyped field, a removed select
//! option, a removed model, a new required field with records already stored. A breaking change
//! is refused unless the new version declares a `[[migration]]` step that resolves it:
//!
//! - `rename_field`: copies the value to the new name and removes the old one;
//! - `set_default`: writes a value into every record where the field is empty;
//! - `drop_field`: deletes the values of a field that the new version no longer has
//!   (needs `confirm = true`).
//!
//! The steps run as one atomic batch together with the new package record, each guarded by a
//! compare-and-set on the value it read, so a record changed by a user in the meantime makes
//! the whole batch fail cleanly (and it is retried against the fresh data) instead of losing the
//! edit. Everything here is pure except `build_ops`, which reads records.

use crate::manifest::{FieldM, Manifest, MigrationM};
use crate::semver::{Req, Version};
use obop_storage::{Backend, Fields, Op};
use serde_json::{Value as J, json};
use std::collections::BTreeMap;

/// One thing that differs between two versions of a package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// `model_added`, `model_removed`, `field_added`, `field_removed`, `field_retyped`,
    /// `field_required`, `option_added`, `option_removed`, `action_added`, `action_removed`.
    pub kind: &'static str,
    pub model: String,
    pub field: Option<String>,
    pub detail: String,
    /// Whether the change can leave a stored record invalid on its own.
    pub breaking: bool,
    /// The migration step that resolves it, in words, when there is one.
    pub resolved_by: Option<String>,
}

/// The structural difference between the installed manifest and the new one.
pub fn diff(old: &Manifest, new: &Manifest) -> Vec<Change> {
    let mut out = Vec::new();
    let ch = |kind, model: &str, field: Option<&str>, detail: String, breaking| Change {
        kind,
        model: model.to_string(),
        field: field.map(String::from),
        detail,
        breaking,
        resolved_by: None,
    };
    for nm in &new.model {
        let Some(om) = old.model.iter().find(|m| m.name == nm.name) else {
            out.push(ch(
                "model_added",
                &nm.name,
                None,
                format!("new model {} with {} field(s)", nm.name, nm.field.len()),
                false,
            ));
            continue;
        };
        for nf in &nm.field {
            match om.field.iter().find(|f| f.name == nf.name) {
                None => out.push(ch(
                    "field_added",
                    &nm.name,
                    Some(&nf.name),
                    format!(
                        "new {} field {}{}",
                        nf.kind,
                        nf.name,
                        if nf.required { " (required)" } else { "" }
                    ),
                    nf.required && nf.computed.is_none(),
                )),
                Some(of) => {
                    if of.kind != nf.kind || of.to != nf.to {
                        out.push(ch(
                            "field_retyped",
                            &nm.name,
                            Some(&nf.name),
                            format!(
                                "{} changes from {}{} to {}{}",
                                nf.name,
                                of.kind,
                                of.to.as_ref().map_or(String::new(), |t| format!(" -> {t}")),
                                nf.kind,
                                nf.to.as_ref().map_or(String::new(), |t| format!(" -> {t}")),
                            ),
                            true,
                        ));
                    } else {
                        if nf.required && !of.required && nf.computed.is_none() {
                            out.push(ch(
                                "field_required",
                                &nm.name,
                                Some(&nf.name),
                                format!("{} becomes required", nf.name),
                                true,
                            ));
                        }
                        for o in &nf.options {
                            if !of.options.contains(o) {
                                out.push(ch(
                                    "option_added",
                                    &nm.name,
                                    Some(&nf.name),
                                    format!("{}: new option {o}", nf.name),
                                    false,
                                ));
                            }
                        }
                        for o in &of.options {
                            if !nf.options.contains(o) {
                                out.push(ch(
                                    "option_removed",
                                    &nm.name,
                                    Some(&nf.name),
                                    format!(
                                        "{}: option {o} is removed; records may still hold it",
                                        nf.name
                                    ),
                                    true,
                                ));
                            }
                        }
                    }
                }
            }
        }
        for of in &om.field {
            if !nm.field.iter().any(|f| f.name == of.name) {
                out.push(ch(
                    "field_removed",
                    &nm.name,
                    Some(&of.name),
                    format!("field {} is removed", of.name),
                    true,
                ));
            }
        }
        for a in &nm.action {
            if !om.action.iter().any(|x| x.name == a.name) {
                out.push(ch(
                    "action_added",
                    &nm.name,
                    None,
                    format!("new action {}", a.name),
                    false,
                ));
            }
        }
        for a in &om.action {
            if !nm.action.iter().any(|x| x.name == a.name) {
                out.push(ch(
                    "action_removed",
                    &nm.name,
                    None,
                    format!("action {} is removed", a.name),
                    false,
                ));
            }
        }
    }
    for om in &old.model {
        if !new.model.iter().any(|m| m.name == om.name) {
            out.push(ch(
                "model_removed",
                &om.name,
                None,
                format!(
                    "model {} is removed; its records would be orphaned",
                    om.name
                ),
                true,
            ));
        }
    }
    out
}

/// A migration step that applies to an upgrade from `from`.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub op: String,
    pub model: String,
    pub field: String,
    pub to: Option<String>,
    pub value: Option<J>,
    pub description: String,
}

impl Step {
    /// The step in words, for the plan and the audit entry.
    pub fn summary(&self) -> String {
        match self.op.as_str() {
            "rename_field" => format!(
                "rename {}.{} to {}",
                self.model,
                self.field,
                self.to.as_deref().unwrap_or("?")
            ),
            "set_default" => format!(
                "fill empty {}.{} with {}",
                self.model,
                self.field,
                self.value.as_ref().map_or("?".into(), J::to_string)
            ),
            "drop_field" => format!("delete the values of {}.{}", self.model, self.field),
            other => other.to_string(),
        }
    }

    pub fn from_json(v: &J) -> Option<Step> {
        let t = |k: &str| v.get(k).and_then(J::as_str).map(String::from);
        Some(Step {
            op: t("op")?,
            model: t("model")?,
            field: t("field")?,
            to: t("to"),
            value: v.get("value").filter(|x| !x.is_null()).cloned(),
            description: t("description").unwrap_or_default(),
        })
    }

    pub fn to_json(&self) -> J {
        json!({"op": self.op, "model": self.model, "field": self.field, "to": self.to,
               "value": self.value, "description": self.description, "summary": self.summary()})
    }
}

fn to_json(v: &toml::Value) -> J {
    match v {
        toml::Value::String(s) => J::from(s.as_str()),
        toml::Value::Integer(i) => J::from(*i),
        toml::Value::Float(f) => J::from(*f),
        toml::Value::Boolean(b) => J::from(*b),
        _ => J::Null,
    }
}

fn step_of(m: &MigrationM) -> Step {
    Step {
        op: m.op.clone(),
        model: m.model.clone(),
        field: m.field.clone(),
        to: m.to.clone(),
        value: m.value.as_ref().map(to_json),
        description: m.description.clone(),
    }
}

/// The steps of `new` that apply when upgrading from `from`, in declaration order.
pub fn steps_for(new: &Manifest, from: &Version) -> Vec<Step> {
    new.migration
        .iter()
        .filter(|m| Req::parse(&m.from).is_ok_and(|r| r.matches(from)))
        .map(step_of)
        .collect()
}

/// The data side of an update: what differs, which steps run, what stops it.
#[derive(Debug, Clone, Default)]
pub struct DataPlan {
    pub changes: Vec<Change>,
    pub steps: Vec<Step>,
    /// Breaking changes no step resolves, in words.
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
}

fn field_of<'a>(m: &'a Manifest, model: &str, field: &str) -> Option<&'a FieldM> {
    m.model
        .iter()
        .find(|x| x.name == model)?
        .field
        .iter()
        .find(|f| f.name == field)
}

/// Judge the update from `old` (installed) to `new`. `count` says how many records a model has:
/// a new required field is only a problem where there are records to fill.
pub fn plan_data(old: &Manifest, new: &Manifest, count: &dyn Fn(&str) -> usize) -> DataPlan {
    let from = old.semver();
    let steps = steps_for(new, &from);
    let mut changes = diff(old, new);
    let mut blockers = Vec::new();
    let mut warnings = Vec::new();
    for c in &mut changes {
        if !c.breaking {
            continue;
        }
        let field = c.field.clone().unwrap_or_default();
        // A model with no records has nothing a change could leave invalid.
        if count(&c.model) == 0 {
            c.resolved_by = Some(format!("no {} records exist yet", c.model));
            continue;
        }
        let resolver: Option<&Step> = match c.kind {
            "field_removed" => steps.iter().find(|s| {
                s.model == c.model
                    && s.field == field
                    && matches!(s.op.as_str(), "drop_field" | "rename_field")
            }),
            "field_added" | "field_required" => steps
                .iter()
                .find(|s| s.model == c.model && s.field == field && s.op == "set_default")
                .or_else(|| {
                    // A field filled by a rename is not new data.
                    steps.iter().find(|s| {
                        s.model == c.model
                            && s.op == "rename_field"
                            && s.to.as_deref() == Some(field.as_str())
                    })
                }),
            _ => None,
        };
        if let Some(s) = resolver {
            c.resolved_by = Some(s.summary());
            continue;
        }
        let records = count(&c.model);
        match c.kind {
            "field_added" | "field_required" if records == 0 => {
                c.resolved_by = Some(format!("no {} records exist yet", c.model));
            }
            "field_added" | "field_required" => blockers.push(format!(
                "{}: {} has {records} record(s) with no value for {field}, which is required. Declare [[migration]] from = \"{}\", op = \"set_default\", model = \"{}\", field = \"{field}\", value = ... to fill them, or make the field optional",
                c.model, c.model, old.module.version, c.model
            )),
            "field_removed" => blockers.push(format!(
                "{}: field {field} is removed. Declare a [[migration]] with op = \"drop_field\" and confirm = true to delete its values, or op = \"rename_field\" if it moved",
                c.model
            )),
            "field_retyped" => blockers.push(format!(
                "{}: {} — a stored value cannot be converted automatically. Keep the old field, add the new one under another name, and move the data with your own import",
                c.model, c.detail
            )),
            "option_removed" => blockers.push(format!(
                "{}: {} Keep the option, or move the records to another option first",
                c.model, c.detail
            )),
            "model_removed" => blockers.push(format!(
                "{}: {}. An update cannot remove a model; uninstall the package instead (its data stays in the database)",
                c.model, c.detail
            )),
            _ => {}
        }
    }
    // Steps that do not match anything in the old version are harmless but worth a word.
    for s in &steps {
        let old_has = field_of(old, &s.model, &s.field).is_some();
        match s.op.as_str() {
            "rename_field" | "drop_field" if !old_has => warnings.push(format!(
                "migration: {} has no field {} in version {}; the step does nothing",
                s.model, s.field, old.module.version
            )),
            _ => {}
        }
        if s.op == "drop_field" {
            warnings.push(format!(
                "this update deletes the values of {}.{}; it cannot be rolled back (a backup can)",
                s.model, s.field
            ));
        }
    }
    for c in &changes {
        if c.kind == "field_added"
            && !c.breaking
            && field_of(new, &c.model, c.field.as_deref().unwrap_or("")).is_some_and(|f| f.unique)
            && count(&c.model) > 1
        {
            warnings.push(format!(
                "{}.{} is unique: empty values of existing records count as equal",
                c.model,
                c.field.clone().unwrap_or_default()
            ));
        }
    }
    DataPlan {
        changes,
        steps,
        blockers,
        warnings,
    }
}

/// The inverse of an update's steps, kept with the package so it can be rolled back: renames
/// reversed, defaults left (a filled value does no harm), and a list of what cannot be undone.
pub fn inverse(steps: &[Step]) -> (Vec<Step>, Vec<String>) {
    let mut inv = Vec::new();
    let mut irreversible = Vec::new();
    for s in steps.iter().rev() {
        match s.op.as_str() {
            "rename_field" => inv.push(Step {
                op: "rename_field".into(),
                model: s.model.clone(),
                field: s.to.clone().unwrap_or_default(),
                to: Some(s.field.clone()),
                value: None,
                description: format!("undo: {}", s.summary()),
            }),
            "drop_field" => irreversible.push(s.summary()),
            _ => {}
        }
    }
    (inv, irreversible)
}

fn is_empty(v: Option<&J>) -> bool {
    match v {
        None | Some(J::Null) => true,
        Some(J::String(s)) => s.is_empty(),
        _ => false,
    }
}

/// The storage operations for `steps`: one compare-and-set per record that needs a change,
/// guarded by the values the record held when it was read. The whole list is applied in one
/// transaction by the caller.
pub fn build_ops(backend: &dyn Backend, steps: &[Step]) -> Result<Vec<Op>, String> {
    /// A record as read, as it will be, and what must change to get there.
    struct Edit {
        orig: Fields,
        cur: Fields,
        patch: Fields,
    }
    let mut edits: BTreeMap<(String, String), Edit> = BTreeMap::new();
    let mut order: Vec<(String, String)> = Vec::new();
    for s in steps {
        let rows = backend
            .list(&s.model)
            .map_err(|e| format!("reading {}: {e}", s.model))?;
        for (id, orig) in rows {
            let key = (s.model.clone(), id);
            if !edits.contains_key(&key) {
                order.push(key.clone());
            }
            let e = edits.entry(key).or_insert_with(|| Edit {
                orig: orig.clone(),
                cur: orig,
                patch: Fields::new(),
            });
            let set = |e: &mut Edit, k: &str, v: J| {
                e.cur.insert(k.to_string(), v.clone());
                e.patch.insert(k.to_string(), v);
            };
            match s.op.as_str() {
                "rename_field" => {
                    let to = s.to.as_deref().ok_or("rename_field without a target")?;
                    if let Some(v) = e.cur.get(&s.field).cloned().filter(|v| !v.is_null()) {
                        // Never overwrite a value already under the new name.
                        if is_empty(e.cur.get(to)) {
                            set(e, to, v);
                        }
                        set(e, &s.field, J::Null);
                    }
                }
                "set_default" => {
                    if is_empty(e.cur.get(&s.field)) {
                        set(e, &s.field, s.value.clone().unwrap_or(J::Null));
                    }
                }
                "drop_field" => {
                    if e.cur.get(&s.field).is_some_and(|v| !v.is_null()) {
                        set(e, &s.field, J::Null);
                    }
                }
                other => return Err(format!("unknown migration op {other}")),
            }
        }
    }
    Ok(order
        .into_iter()
        .filter_map(|key| {
            let e = edits.remove(&key)?;
            if e.patch.is_empty() {
                return None;
            }
            let expect = e
                .patch
                .keys()
                .map(|k| (k.clone(), e.orig.get(k).cloned().unwrap_or(J::Null)))
                .collect();
            Some(Op::Patch {
                model: key.0,
                id: key.1,
                patch: e.patch,
                expect,
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(version: &str, fields: &str, migrations: &str) -> Manifest {
        Manifest::parse(&format!(
            "[module]\nid = \"mig\"\nversion = \"{version}\"\nname = \"M\"\ndescription = \"d\"\nlicense = \"MIT\"\nrequires_core = \"^0.2\"\n[[model]]\nname = \"mig_item\"\ndescription = \"d\"\nread_group = \"mig.user\"\nwrite_group = \"mig.user\"\n{fields}\n{migrations}"
        ))
        .unwrap()
    }

    fn field(name: &str, extra: &str) -> String {
        format!(
            "[[model.field]]\nname = \"{name}\"\ndescription = \"d\"\nclass = \"none\"\n{extra}\n"
        )
    }

    fn rec(pairs: &[(&str, J)]) -> Fields {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn additive_changes_need_nothing() {
        let old = m("0.1.0", &field("title", ""), "");
        let new = m(
            "0.2.0",
            &format!(
                "{}{}{}",
                field("title", ""),
                field("note", ""),
                field("kind", "kind = \"select\"\noptions = [\"a\"]")
            ),
            "",
        );
        let p = plan_data(&old, &new, &|_| 100);
        assert!(p.blockers.is_empty(), "{:?}", p.blockers);
        assert_eq!(
            p.changes.iter().filter(|c| c.kind == "field_added").count(),
            2
        );
        assert!(p.changes.iter().all(|c| !c.breaking));
    }

    #[test]
    fn breaking_changes_are_refused_without_a_step() {
        let old = m(
            "0.1.0",
            &format!("{}{}", field("title", ""), field("legacy", "")),
            "",
        );
        let new = m(
            "0.2.0",
            &format!(
                "{}{}",
                field("title", "kind = \"number\""),
                field("code", "required = true")
            ),
            "",
        );
        let p = plan_data(&old, &new, &|_| 3);
        let all = p.blockers.join("\n");
        assert!(all.contains("legacy is removed"), "{all}");
        assert!(all.contains("cannot be converted automatically"), "{all}");
        assert!(all.contains("no value for code"), "{all}");
        // With no records nothing can be left invalid, whatever changed.
        let p = plan_data(&old, &new, &|_| 0);
        assert!(p.blockers.is_empty(), "{:?}", p.blockers);
    }

    #[test]
    fn steps_resolve_the_breaking_changes_they_name() {
        let old = m(
            "0.1.0",
            &format!("{}{}", field("title", ""), field("legacy", "")),
            "",
        );
        let new = m(
            "0.2.0",
            &format!("{}{}", field("title", ""), field("code", "required = true")),
            "[[migration]]\nfrom = \"<0.2.0\"\nop = \"drop_field\"\nmodel = \"mig_item\"\nfield = \"legacy\"\nconfirm = true\n[[migration]]\nfrom = \"<0.2.0\"\nop = \"set_default\"\nmodel = \"mig_item\"\nfield = \"code\"\nvalue = \"n/a\"\n",
        );
        let p = plan_data(&old, &new, &|_| 5);
        assert!(p.blockers.is_empty(), "{:?}", p.blockers);
        assert!(
            p.warnings
                .iter()
                .any(|w| w.contains("cannot be rolled back"))
        );
        assert_eq!(p.steps.len(), 2);
        // A step for another starting version does not apply.
        let p = plan_data(&m("0.3.0", &field("title", ""), ""), &new, &|_| 5);
        assert!(p.steps.is_empty());
    }

    #[test]
    fn rename_moves_values_and_set_default_fills_only_the_empty() {
        let b = obop_storage::open("memory:").unwrap();
        b.insert(
            "mig_item",
            "1",
            &rec(&[("old", json!("x")), ("code", json!(""))]),
        )
        .unwrap();
        b.insert(
            "mig_item",
            "2",
            &rec(&[("old", json!("y")), ("code", json!("keep"))]),
        )
        .unwrap();
        b.insert("mig_item", "3", &rec(&[("other", json!(1))]))
            .unwrap();
        let steps = vec![
            Step {
                op: "rename_field".into(),
                model: "mig_item".into(),
                field: "old".into(),
                to: Some("new".into()),
                value: None,
                description: String::new(),
            },
            Step {
                op: "set_default".into(),
                model: "mig_item".into(),
                field: "code".into(),
                to: None,
                value: Some(json!("n/a")),
                description: String::new(),
            },
        ];
        let ops = build_ops(&*b, &steps).unwrap();
        b.apply_checked(&ops).unwrap();
        let r1 = b.get("mig_item", "1").unwrap().unwrap();
        assert_eq!(r1["new"], "x");
        assert!(r1["old"].is_null());
        assert_eq!(r1["code"], "n/a");
        let r2 = b.get("mig_item", "2").unwrap().unwrap();
        assert_eq!(r2["new"], "y");
        assert_eq!(r2["code"], "keep");
        // A record with neither field gets only the default.
        let r3 = b.get("mig_item", "3").unwrap().unwrap();
        assert_eq!(r3["code"], "n/a");
        assert!(!r3.contains_key("new"));
        // Running it again changes nothing.
        assert!(build_ops(&*b, &steps[1..]).unwrap().is_empty());
    }

    #[test]
    fn a_record_edited_meanwhile_fails_the_batch_and_changes_nothing() {
        let b = obop_storage::open("memory:").unwrap();
        b.insert("mig_item", "1", &rec(&[("old", json!("x"))]))
            .unwrap();
        b.insert("mig_item", "2", &rec(&[("old", json!("y"))]))
            .unwrap();
        let steps = vec![Step {
            op: "rename_field".into(),
            model: "mig_item".into(),
            field: "old".into(),
            to: Some("new".into()),
            value: None,
            description: String::new(),
        }];
        let ops = build_ops(&*b, &steps).unwrap();
        // A user edits record 2 after the plan read it.
        b.update("mig_item", "2", &rec(&[("old", json!("edited"))]))
            .unwrap();
        assert!(b.apply_checked(&ops).is_err());
        let r1 = b.get("mig_item", "1").unwrap().unwrap();
        assert_eq!(r1["old"], "x", "the first record was rolled back too");
        assert!(!r1.contains_key("new"));
    }

    #[test]
    fn renames_invert_and_drops_do_not() {
        let steps = vec![
            Step {
                op: "rename_field".into(),
                model: "a".into(),
                field: "x".into(),
                to: Some("y".into()),
                value: None,
                description: String::new(),
            },
            Step {
                op: "drop_field".into(),
                model: "a".into(),
                field: "z".into(),
                to: None,
                value: None,
                description: String::new(),
            },
        ];
        let (inv, irr) = inverse(&steps);
        assert_eq!(inv.len(), 1);
        assert_eq!(
            (inv[0].field.as_str(), inv[0].to.as_deref()),
            ("y", Some("x"))
        );
        assert_eq!(irr.len(), 1);
    }

    #[test]
    fn migration_steps_are_checked_when_the_package_is_read() {
        let bad = |migration: &str| {
            Manifest::parse(&format!(
                "[module]\nid = \"mig\"\nversion = \"0.2.0\"\nname = \"M\"\ndescription = \"d\"\nrequires_core = \"^0.2\"\n[[model]]\nname = \"mig_item\"\ndescription = \"d\"\nread_group = \"mig.user\"\nwrite_group = \"mig.user\"\n{}{}",
                field("title", ""),
                migration
            ))
            .unwrap_err()
        };
        let head = "[[migration]]\nfrom = \"<0.2.0\"\nmodel = \"mig_item\"\n";
        assert!(
            bad(&format!("{head}op = \"drop_field\"\nfield = \"gone\"\n"))
                .contains("confirm = true")
        );
        assert!(
            bad(&format!(
                "{head}op = \"drop_field\"\nfield = \"title\"\nconfirm = true\n"
            ))
            .contains("nothing to drop")
        );
        assert!(
            bad(&format!("{head}op = \"rename_field\"\nfield = \"gone\"\n")).contains("needs `to`")
        );
        assert!(
            bad(&format!(
                "{head}op = \"rename_field\"\nfield = \"gone\"\nto = \"nope\"\n"
            ))
            .contains("not a field")
        );
        assert!(
            bad(&format!(
                "{head}op = \"set_default\"\nfield = \"title\"\nvalue = 3\n"
            ))
            .contains("does not fit")
        );
        assert!(
            bad(&format!("{head}op = \"explode\"\nfield = \"title\"\n")).contains("unknown op")
        );
        assert!(bad("[[migration]]\nfrom = \"nope\"\nop = \"drop_field\"\nmodel = \"mig_item\"\nfield = \"q\"\n").contains("from"));
        assert!(bad("[[migration]]\nfrom = \"<1\"\nop = \"drop_field\"\nmodel = \"other\"\nfield = \"q\"\n").contains("declares no model"));
    }
}
