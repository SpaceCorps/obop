//! A unit of work: the writes of one command, applied together or not at all.
//!
//! The command layer holds one lock while a unit is built and committed, so the reads made while
//! building it (the current quantity of a stock item, the counter of a sequence) cannot be
//! outdated by another command. Writes go to an overlay first, so later steps see earlier ones;
//! `commit` hands the recorded steps to `Backend::apply`, which is one transaction on SQLite and
//! PostgreSQL. Nothing is written before that, so a failure at any step leaves no trace,
//! including no consumed sequence number.

use obop_registry::rules::MAX_EFFECT_RECORDS;
use obop_storage::{Backend, Fields, Op};
use serde_json::Value as J;
use std::collections::HashMap;

pub(crate) type R<T> = Result<T, String>;

/// Read access to records, with or without the writes of a unit in progress.
pub(crate) trait Reads {
    fn get(&self, model: &str, id: &str) -> R<Option<Fields>>;
    fn find(&self, model: &str, field: &str, value: &J) -> R<Vec<(String, Fields)>>;
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
    fn all(&self, model: &str) -> R<Vec<(String, Fields)>> {
        self.0.list(model)
    }
}

type Key = (String, String);

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
                    self.overlay.insert(u.key, prev);
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
