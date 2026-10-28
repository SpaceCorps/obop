//! A unit of work: the writes of one command, applied together or not at all.
//!
//! The command layer holds one lock while a unit is built and committed, so the reads made while
//! building it (the current quantity of a stock item, the counter of a sequence) cannot be
//! outdated by another command. Writes go to an overlay first, so later steps see earlier ones;
//! `commit` hands the recorded steps to `Backend::apply`, which is one transaction on SQLite and
//! PostgreSQL. Nothing is written before that, so a failure at any step leaves no trace,
//! including no consumed sequence number.

use obop_registry::rules::MAX_EFFECT_RECORDS;
use obop_storage::{Backend, Fields, Op, unique_matches};
use serde_json::Value as J;
use std::cell::RefCell;
use std::collections::HashMap;

pub(crate) type R<T> = Result<T, String>;

/// Read access to records, with or without the writes of a unit in progress.
pub(crate) trait Reads {
    fn get(&self, model: &str, id: &str) -> R<Option<Fields>>;
    fn find(&self, model: &str, field: &str, value: &J) -> R<Vec<(String, Fields)>>;
    /// The records a uniqueness rule (D57) compares with a new value, as [`Backend::find_unique`]
    /// says, seen through the writes of the unit. With the rule's index in the store the cost does
    /// not grow with the model, and the unit's own records are found by key, not by a scan.
    fn find_unique(&self, model: &str, fields: &[&str], values: &[J]) -> R<Vec<(String, Fields)>>;
    fn all(&self, model: &str) -> R<Vec<(String, Fields)>>;
}

/// Reads straight from the store.
pub(crate) struct Plain<'a>(pub &'a dyn Backend);

impl Reads for Plain<'_> {
    fn get(&self, model: &str, id: &str) -> R<Option<Fields>> {
        self.0.get(model, id)
    }
    fn find(&self, model: &str, field: &str, value: &J) -> R<Vec<(String, Fields)>> {
        self.0.find(model, field, value)
    }
    fn find_unique(&self, model: &str, fields: &[&str], values: &[J]) -> R<Vec<(String, Fields)>> {
        self.0.find_unique(model, fields, values)
    }
    fn all(&self, model: &str) -> R<Vec<(String, Fields)>> {
        self.0.list(model)
    }
}

type Key = (String, String);

/// The unit's own records of one model by the key of one uniqueness rule, so that the check of a
/// row does not look at every other row the unit has written (a bulk create of 1,000 rows would
/// otherwise compare each row with all the rows before it).
///
/// Invariant: every live record in the overlay is listed under its current key. A list may also
/// hold an id that was rewound, deleted or changed since; a reader checks each candidate against
/// the overlay, so a stale entry costs one look and never gives a wrong answer.
struct Keyed {
    fields: Vec<String>,
    ids: HashMap<String, Vec<String>>,
}

impl Keyed {
    /// The key of `rec` under this rule: its values for the rule's fields as JSON text, a missing
    /// or null scope field as the empty text. `None` when the rule does not constrain the record
    /// (its first field is missing, null or empty).
    fn key_of(&self, rec: &Fields) -> Option<String> {
        let mut values = Vec::with_capacity(self.fields.len());
        for (i, f) in self.fields.iter().enumerate() {
            values.push(match rec.get(f) {
                None | Some(J::Null) if i > 0 => J::from(""),
                None | Some(J::Null) => return None,
                Some(J::String(s)) if i == 0 && s.is_empty() => return None,
                Some(v) => v.clone(),
            });
        }
        Some(key_text(&values))
    }

    fn note(&mut self, id: &str, rec: &Fields) {
        if let Some(key) = self.key_of(rec) {
            let ids = self.ids.entry(key).or_default();
            if !ids.iter().any(|i| i == id) {
                ids.push(id.to_string());
            }
        }
    }
}

/// Values as one string that is equal for two lists exactly when the lists are equal as JSON.
fn key_text(values: &[J]) -> String {
    let mut out = String::new();
    for v in values {
        out.push_str(&match v {
            // 0.0 and -0.0 are equal as numbers and differ as text.
            J::Number(n) if n.as_f64() == Some(0.0) && !n.is_i64() && !n.is_u64() => {
                "0.0".to_string()
            }
            other => other.to_string(),
        });
        out.push('\u{1f}');
    }
    out
}

/// What one change did to the overlay, so a part of the unit can be taken back (see `Mark`).
struct Undo {
    key: Key,
    before: Option<Option<Fields>>,
    /// The change added the record to `inserted`.
    inserted: bool,
}

/// A point in a unit that `rewind` returns to, discarding everything done since. A bulk create
/// that keeps the good rows uses it to drop the writes of a row that failed half way.
pub(crate) struct Mark {
    ops: usize,
    undo: usize,
    written: usize,
    touched: usize,
}

pub(crate) struct Unit<'a> {
    backend: &'a dyn Backend,
    /// `None`: deleted in this unit.
    overlay: HashMap<Key, Option<Fields>>,
    /// Ids inserted in this unit, in order, by model.
    inserted: HashMap<String, Vec<String>>,
    ops: Vec<Op>,
    /// Records written, for the cap per action.
    written: usize,
    /// Records inserted or merged, for the uniqueness check.
    touched: Vec<Key>,
    cap: usize,
    undo: Vec<Undo>,
    /// The overlay by the keys of the uniqueness rules asked about so far, by model.
    keyed: RefCell<HashMap<String, Vec<Keyed>>>,
}

impl<'a> Unit<'a> {
    pub fn new(backend: &'a dyn Backend) -> Self {
        Self {
            backend,
            overlay: HashMap::new(),
            inserted: HashMap::new(),
            ops: Vec::new(),
            written: 0,
            touched: Vec::new(),
            cap: MAX_EFFECT_RECORDS,
            undo: Vec::new(),
            keyed: RefCell::new(HashMap::new()),
        }
    }

    /// The record at `key` was just written to the overlay: list it under its key in each rule
    /// that is being asked about.
    fn note_keyed(&mut self, key: &Key) {
        let Some(Some(rec)) = self.overlay.get(key) else {
            return;
        };
        if let Some(rules) = self.keyed.get_mut().get_mut(&key.0) {
            for rule in rules {
                rule.note(&key.1, rec);
            }
        }
    }

    /// A unit that may write up to `cap` records (a bulk create).
    pub fn with_cap(backend: &'a dyn Backend, cap: usize) -> Self {
        let mut u = Self::new(backend);
        u.cap = cap;
        u
    }

    /// Where the unit is now.
    pub fn mark(&self) -> Mark {
        Mark {
            ops: self.ops.len(),
            undo: self.undo.len(),
            written: self.written,
            touched: self.touched.len(),
        }
    }

    /// Take back everything done since `mark`.
    pub fn rewind(&mut self, mark: Mark) {
        while self.undo.len() > mark.undo {
            let Some(u) = self.undo.pop() else { break };
            if u.inserted
                && let Some(ids) = self.inserted.get_mut(&u.key.0)
            {
                ids.pop();
            }
            match u.before {
                None => {
                    self.overlay.remove(&u.key);
                }
                Some(prev) => {
                    self.overlay.insert(u.key.clone(), prev);
                    // The record is back as it was: list it under that key again.
                    self.note_keyed(&u.key);
                }
            }
        }
        self.ops.truncate(mark.ops);
        self.touched.truncate(mark.touched);
        self.written = mark.written;
    }

    /// Add a step that is not a record write of the engine (a replay record, for example).
    pub fn push_op(&mut self, op: Op) {
        self.ops.push(op);
    }

    /// A unit for maintenance work that may touch any number of records (still one transaction).
    pub fn unbounded(backend: &'a dyn Backend) -> Self {
        let mut u = Self::new(backend);
        u.cap = usize::MAX;
        u
    }

    /// Count work against the cap.
    pub fn spend(&mut self, n: usize) -> R<()> {
        self.written += n;
        if self.written > self.cap {
            return Err(format!(
                "this action would touch more than {MAX_EFFECT_RECORDS} records; nothing was changed. Split the work into smaller parts"
            ));
        }
        Ok(())
    }

    /// Records inserted or merged so far (each once), in order.
    pub fn touched(&self) -> Vec<Key> {
        let mut seen = std::collections::HashSet::new();
        self.touched
            .iter()
            .filter(|k| seen.insert((*k).clone()))
            .cloned()
            .collect()
    }

    pub fn insert(&mut self, model: &str, id: &str, fields: Fields) -> R<()> {
        self.spend(1)?;
        let key = (model.to_string(), id.to_string());
        self.ops.push(Op::Insert {
            model: key.0.clone(),
            id: key.1.clone(),
            fields: fields.clone(),
        });
        let before = self.overlay.insert(key.clone(), Some(fields));
        self.undo.push(Undo {
            key: key.clone(),
            before,
            inserted: true,
        });
        self.inserted
            .entry(model.to_string())
            .or_default()
            .push(id.to_string());
        self.note_keyed(&key);
        self.touched.push(key);
        Ok(())
    }

    /// Merge `patch` into an existing record.
    pub fn merge(&mut self, model: &str, id: &str, patch: Fields) -> R<()> {
        let Some(mut cur) = self.get(model, id)? else {
            return Err(format!("no {model} {id}"));
        };
        self.spend(1)?;
        let key = (model.to_string(), id.to_string());
        cur.extend(patch.clone());
        self.ops.push(Op::Merge {
            model: key.0.clone(),
            id: key.1.clone(),
            patch,
        });
        let before = self.overlay.insert(key.clone(), Some(cur));
        self.undo.push(Undo {
            key: key.clone(),
            before,
            inserted: false,
        });
        self.note_keyed(&key);
        self.touched.push(key);
        Ok(())
    }

    pub fn delete(&mut self, model: &str, id: &str) -> R<()> {
        if self.get(model, id)?.is_none() {
            return Err(format!("no {model} {id}"));
        }
        self.spend(1)?;
        let key = (model.to_string(), id.to_string());
        self.ops.push(Op::Delete {
            model: key.0.clone(),
            id: key.1.clone(),
        });
        let before = self.overlay.insert(key.clone(), None);
        self.undo.push(Undo {
            key,
            before,
            inserted: false,
        });
        Ok(())
    }

    /// Apply every recorded step, steps that need `apply_checked` included, in one transaction.
    pub fn commit_checked(self) -> R<()> {
        if self.ops.is_empty() {
            return Ok(());
        }
        self.backend
            .apply_checked(&self.ops)
            .map_err(|e| e.to_string())
    }

    /// Apply every recorded step in one transaction.
    pub fn commit(self) -> R<()> {
        if self.ops.is_empty() {
            return Ok(());
        }
        self.backend.apply(&self.ops)
    }
}

impl Reads for Unit<'_> {
    fn get(&self, model: &str, id: &str) -> R<Option<Fields>> {
        match self.overlay.get(&(model.to_string(), id.to_string())) {
            Some(slot) => Ok(slot.clone()),
            None => self.backend.get(model, id),
        }
    }

    fn find(&self, model: &str, field: &str, value: &J) -> R<Vec<(String, Fields)>> {
        let mut out: Vec<(String, Fields)> = Vec::new();
        for (id, f) in self.backend.find(model, field, value)? {
            match self.overlay.get(&(model.to_string(), id.clone())) {
                Some(Some(now)) if now.get(field) == Some(value) => out.push((id, now.clone())),
                Some(_) => {}
                None => out.push((id, f)),
            }
        }
        // Records the unit changed or added that now match.
        for ((m, id), slot) in &self.overlay {
            if m == model
                && let Some(f) = slot
                && f.get(field) == Some(value)
                && !out.iter().any(|(i, _)| i == id)
            {
                out.push((id.clone(), f.clone()));
            }
        }
        Ok(out)
    }

    fn find_unique(&self, model: &str, fields: &[&str], values: &[J]) -> R<Vec<(String, Fields)>> {
        // What is stored and the unit has not touched counts as it is stored. A stored record the
        // unit changed or deleted is judged by the unit's version, found below.
        let mut out: Vec<(String, Fields)> = Vec::new();
        for (id, f) in self.backend.find_unique(model, fields, values)? {
            if !self.overlay.contains_key(&(model.to_string(), id.clone())) {
                out.push((id, f));
            }
        }
        // The unit's own records with that key: from the rule's list, built from the overlay the
        // first time the rule is asked about and kept up to date by every later write.
        let key = key_text(values);
        let candidates: Vec<String> = {
            let mut by_model = self.keyed.borrow_mut();
            let rules = by_model.entry(model.to_string()).or_default();
            let at = match rules.iter().position(|r| r.fields == fields) {
                Some(at) => at,
                None => {
                    let mut rule = Keyed {
                        fields: fields.iter().map(|f| (*f).to_string()).collect(),
                        ids: HashMap::new(),
                    };
                    for ((m, id), slot) in &self.overlay {
                        if m == model
                            && let Some(rec) = slot
                        {
                            rule.note(id, rec);
                        }
                    }
                    rules.push(rule);
                    rules.len() - 1
                }
            };
            rules[at].ids.get(&key).cloned().unwrap_or_default()
        };
        for id in candidates {
            if let Some(Some(rec)) = self.overlay.get(&(model.to_string(), id.clone()))
                && unique_matches(rec, fields, values)
                && !out.iter().any(|(i, _)| *i == id)
            {
                out.push((id, rec.clone()));
            }
        }
        Ok(out)
    }

    fn all(&self, model: &str) -> R<Vec<(String, Fields)>> {
        let mut out = Vec::new();
        for (id, f) in self.backend.list(model)? {
            match self.overlay.get(&(model.to_string(), id.clone())) {
                Some(Some(now)) => out.push((id, now.clone())),
                Some(None) => {}
                None => out.push((id, f)),
            }
        }
        for id in self.inserted.get(model).into_iter().flatten() {
            if let Some(Some(f)) = self.overlay.get(&(model.to_string(), id.clone()))
                && !out.iter().any(|(i, _)| i == id)
            {
                out.push((id.clone(), f.clone()));
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(k: &str, v: J) -> Fields {
        let mut m = Fields::new();
        m.insert(k.into(), v);
        m
    }

    #[test]
    fn writes_are_invisible_until_commit_and_all_or_nothing() {
        let b = obop_storage::open("memory:").unwrap();
        b.insert("t", "a", &f("n", J::from(1))).unwrap();
        let mut u = Unit::new(&*b);
        u.merge("t", "a", f("n", J::from(2))).unwrap();
        u.insert("t", "b", f("n", J::from(3))).unwrap();
        // The unit sees its own writes; the store does not yet.
        assert_eq!(Reads::get(&u, "t", "a").unwrap().unwrap()["n"], J::from(2));
        assert_eq!(Reads::all(&u, "t").unwrap().len(), 2);
        assert_eq!(Reads::find(&u, "t", "n", &J::from(3)).unwrap().len(), 1);
        assert!(Reads::find(&u, "t", "n", &J::from(1)).unwrap().is_empty());
        assert_eq!(b.get("t", "a").unwrap().unwrap()["n"], J::from(1));
        assert!(b.get("t", "b").unwrap().is_none());
        u.commit().unwrap();
        assert_eq!(b.get("t", "a").unwrap().unwrap()["n"], J::from(2));
        assert!(b.get("t", "b").unwrap().is_some());
        // A step that cannot be applied fails the whole unit.
        let mut u = Unit::new(&*b);
        u.merge("t", "a", f("n", J::from(9))).unwrap();
        u.insert("t", "b", Fields::new()).unwrap(); // duplicate id
        assert!(u.commit().is_err());
        assert_eq!(
            b.get("t", "a").unwrap().unwrap()["n"],
            J::from(2),
            "rolled back"
        );
    }

    #[test]
    fn deletes_and_the_cap() {
        let b = obop_storage::open("memory:").unwrap();
        b.insert("t", "a", &Fields::new()).unwrap();
        let mut u = Unit::new(&*b);
        u.delete("t", "a").unwrap();
        assert!(Reads::get(&u, "t", "a").unwrap().is_none());
        assert!(u.merge("t", "a", Fields::new()).is_err());
        assert!(u.delete("t", "a").is_err());
        let mut u = Unit::new(&*b);
        for i in 0..MAX_EFFECT_RECORDS {
            u.insert("t", &format!("x{i}"), Fields::new()).unwrap();
        }
        let e = u.insert("t", "over", Fields::new()).unwrap_err();
        assert!(e.contains("more than 500 records"), "{e}");
    }

    // Rewinding a unit (D70).
    #[test]
    fn rewind_takes_back_what_was_done_since_a_mark() {
        let b = obop_storage::open("memory:").unwrap();
        b.insert("t", "a", &f("n", J::from(1))).unwrap();
        let mut u = Unit::new(&*b);
        u.insert("t", "kept", f("n", J::from(10))).unwrap();
        let mark = u.mark();
        u.merge("t", "a", f("n", J::from(2))).unwrap();
        u.merge("t", "kept", f("n", J::from(11))).unwrap();
        u.insert("t", "dropped", f("n", J::from(3))).unwrap();
        u.delete("t", "a").unwrap();
        u.rewind(mark);
        assert_eq!(Reads::get(&u, "t", "a").unwrap().unwrap()["n"], J::from(1));
        assert_eq!(
            Reads::get(&u, "t", "kept").unwrap().unwrap()["n"],
            J::from(10)
        );
        assert!(Reads::get(&u, "t", "dropped").unwrap().is_none());
        assert_eq!(Reads::all(&u, "t").unwrap().len(), 2);
        assert_eq!(u.touched(), [("t".to_string(), "kept".to_string())]);
        u.commit().unwrap();
        assert_eq!(b.get("t", "a").unwrap().unwrap()["n"], J::from(1));
        assert_eq!(b.get("t", "kept").unwrap().unwrap()["n"], J::from(10));
        assert!(b.get("t", "dropped").unwrap().is_none());
    }
}
