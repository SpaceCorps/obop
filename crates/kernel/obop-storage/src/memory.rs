//! The in-memory backend: tests, `memory:` databases and the contract's reference.

use super::*;
use crate::history::{Backfilled, HistoryQuery};
use crate::sysfields::{self, ChangeRow, Clock};

/// The change history (D75): a list that only grows, except that an erasure rewrites the values
/// of the entries of one record. Nothing else changes or removes an entry (`import` with a wipe
/// replaces the lot).
#[derive(Default, Clone)]
struct MemHistory {
    rows: Vec<ChangeRow>,
    last_seq: i64,
}

impl MemHistory {
    fn append(&mut self, rows: &[ChangeRow]) {
        for r in rows {
            self.last_seq += 1;
            self.rows.push(ChangeRow {
                seq: self.last_seq,
                txid: 0,
                ..r.clone()
            });
        }
    }
}

#[derive(Default)]
pub(crate) struct Memory {
    rows: Mutex<BTreeMap<String, Vec<(String, Fields)>>>,
    /// Insertion order across models, for `export`.
    order: Mutex<Vec<(String, String)>>,
    /// Always taken after `rows` and `order`.
    history: Mutex<MemHistory>,
    clock: Clock,
    locks: Arc<LocalLocks>,
    /// key -> (value, expiry in ms; 0 = never)
    counters: Mutex<BTreeMap<String, (u64, u64)>>,
    kv: Mutex<BTreeMap<String, (String, u64)>>,
}

impl Memory {
    fn note(&self, model: &str, id: &str) {
        self.order.lock().unwrap().push((model.into(), id.into()));
    }

    /// A store whose stamps read `clock` instead of the host clock (tests).
    #[cfg(test)]
    pub(crate) fn with_clock(clock: Clock) -> Self {
        Self {
            clock,
            ..Self::default()
        }
    }
}

fn live(expiry: u64, now: u64) -> bool {
    expiry == 0 || expiry > now
}

impl Backend for Memory {
    fn insert(&self, model: &str, id: &str, fields: &Fields) -> Result<()> {
        let mut g = self.rows.lock().unwrap();
        let rows = g.entry(model.into()).or_default();
        if rows.iter().any(|(i, _)| i == id) {
            return Err(format!("duplicate {model} {id}"));
        }
        rows.push((id.into(), fields.clone()));
        drop(g);
        self.note(model, id);
        Ok(())
    }
    fn get(&self, model: &str, id: &str) -> Result<Option<Fields>> {
        let g = self.rows.lock().unwrap();
        Ok(g.get(model)
            .and_then(|r| r.iter().find(|(i, _)| i == id))
            .map(|(_, f)| f.clone()))
    }
    fn list(&self, model: &str) -> Result<Vec<(String, Fields)>> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .get(model)
            .cloned()
            .unwrap_or_default())
    }
    fn update(&self, model: &str, id: &str, patch: &Fields) -> Result<bool> {
        let mut g = self.rows.lock().unwrap();
        match g
            .get_mut(model)
            .and_then(|r| r.iter_mut().find(|(i, _)| i == id))
        {
            Some((_, f)) => {
                f.extend(patch.clone());
                Ok(true)
            }
            None => Ok(false),
        }
    }
    fn count(&self, model: &str) -> Result<usize> {
        Ok(self.rows.lock().unwrap().get(model).map_or(0, Vec::len))
    }
    fn delete(&self, model: &str, id: &str) -> Result<bool> {
        let mut g = self.rows.lock().unwrap();
        let Some(rows) = g.get_mut(model) else {
            return Ok(false);
        };
        let before = rows.len();
        rows.retain(|(i, _)| i != id);
        let removed = rows.len() < before;
        drop(g);
        if removed {
            self.order
                .lock()
                .unwrap()
                .retain(|(m, i)| !(m == model && i == id));
        }
        Ok(removed)
    }
    fn insert_many(&self, rows: &[(String, String, Fields)]) -> Result<()> {
        let mut g = self.rows.lock().unwrap();
        for (n, (model, id, _)) in rows.iter().enumerate() {
            let dup_in_batch = rows[..n].iter().any(|(m, i, _)| m == model && i == id);
            if dup_in_batch || g.get(model).is_some_and(|r| r.iter().any(|(i, _)| i == id)) {
                return Err(format!("duplicate {model} {id}"));
            }
        }
        for (model, id, fields) in rows {
            g.entry(model.clone())
                .or_default()
                .push((id.clone(), fields.clone()));
            self.note(model, id);
        }
        Ok(())
    }
    fn scan(
        &self,
        model: &str,
        visit: &mut dyn FnMut(&str, &Fields) -> Result<bool>,
    ) -> Result<()> {
        let g = self.rows.lock().unwrap();
        for (id, f) in g.get(model).into_iter().flatten() {
            if !visit(id, f)? {
                break;
            }
        }
        Ok(())
    }
    fn apply(&self, ops: &[Op]) -> Result<()> {
        self.apply_checked(ops).map_err(|e| e.to_string())
    }
    fn apply_checked(&self, ops: &[Op]) -> std::result::Result<(), ApplyError> {
        self.apply_tracked(ctx::system_ctx(), ops, Tracking::empty())
    }
    fn apply_tracked(
        &self,
        ctx: &WriteCtx,
        ops: &[Op],
        tracking: &Tracking,
    ) -> std::result::Result<(), ApplyError> {
        struct M<'a> {
            rows: &'a mut BTreeMap<String, Vec<(String, Fields)>>,
            history: &'a mut MemHistory,
            clock: &'a Clock,
        }
        impl TxView for M<'_> {
            fn tx_get(&mut self, model: &str, id: &str) -> Result<Option<Fields>> {
                Ok(self
                    .rows
                    .get(model)
                    .and_then(|r| r.iter().find(|(i, _)| i == id))
                    .map(|(_, f)| f.clone()))
            }
            fn tx_insert(&mut self, model: &str, id: &str, fields: &Fields) -> Result<()> {
                self.rows
                    .entry(model.into())
                    .or_default()
                    .push((id.into(), fields.clone()));
                Ok(())
            }
            fn tx_put(&mut self, model: &str, id: &str, fields: &Fields) -> Result<()> {
                if let Some((_, f)) = self
                    .rows
                    .get_mut(model)
                    .and_then(|r| r.iter_mut().find(|(i, _)| i == id))
                {
                    *f = fields.clone();
                }
                Ok(())
            }
            fn tx_delete(&mut self, model: &str, id: &str) -> Result<()> {
                if let Some(r) = self.rows.get_mut(model) {
                    r.retain(|(i, _)| i != id);
                }
                Ok(())
            }
            fn tx_now(&mut self) -> Result<i64> {
                Ok(self.clock.tick())
            }
            fn tx_history(&mut self, rows: &[ChangeRow]) -> Result<()> {
                self.history.append(rows);
                Ok(())
            }
            fn tx_scrub(
                &mut self,
                model: &str,
                id: &str,
                keep: &[String],
                actor: &str,
                via: &Fields,
                row_version: i64,
                at: i64,
            ) -> Result<u64> {
                let mut erased: std::collections::BTreeSet<String> = Default::default();
                let mut entries = 0usize;
                for r in self
                    .history
                    .rows
                    .iter_mut()
                    .filter(|r| r.model == model && r.record_id == id && r.op != "erase")
                {
                    if let Some((changed, names)) = sysfields::scrub_changed(&r.changed, keep) {
                        r.changed = changed;
                        r.sealed = false;
                        entries += 1;
                        erased.extend(names);
                    }
                }
                self.history.append(&[ChangeRow {
                    seq: 0,
                    at: sysfields::fmt_micros(at),
                    model: model.to_string(),
                    record_id: id.to_string(),
                    op: "erase".to_string(),
                    actor: actor.to_string(),
                    via: via.clone(),
                    row_version,
                    changed: sysfields::erase_changed(erased, entries),
                    sealed: false,
                    batch_id: None,
                    run_id: None,
                    txid: 0,
                }]);
                Ok(entries as u64)
            }
        }
        let mut g = self.rows.lock().unwrap();
        let mut h = self.history.lock().unwrap();
        let backup = g.clone();
        // An append is undone by cutting the list back; only an erasure rewrites old entries.
        let (kept_len, kept_seq) = (h.rows.len(), h.last_seq);
        let kept_rows = ops
            .iter()
            .any(|o| matches!(o, Op::EraseHistory { .. }))
            .then(|| h.rows.clone());
        let r = run_ops(
            &mut M {
                rows: &mut g,
                history: &mut h,
                clock: &self.clock,
            },
            ops,
            ctx,
            tracking,
        );
        if r.is_err() {
            *g = backup;
            match kept_rows {
                Some(rows) => h.rows = rows,
                None => h.rows.truncate(kept_len),
            }
            h.last_seq = kept_seq;
        } else {
            // Keep the cross-model insertion order (for `export`) in step with the rows.
            let mut order = self.order.lock().unwrap();
            order.retain(|(m, i)| g.get(m).is_some_and(|r| r.iter().any(|(x, _)| x == i)));
            let seen: std::collections::HashSet<(String, String)> = order.iter().cloned().collect();
            for (m, rows) in g.iter() {
                for (i, _) in rows {
                    if !seen.contains(&(m.clone(), i.clone())) {
                        order.push((m.clone(), i.clone()));
                    }
                }
            }
        }
        r
    }
    fn kind(&self) -> &'static str {
        "memory"
    }

    fn lock(&self, key: &str) -> Result<LockGuard> {
        Ok(Box::new(self.locks.take(key)?))
    }

    fn counter_add(&self, key: &str, ttl_ms: u64, extend: bool) -> Result<u64> {
        let now = now_ms();
        let expiry = if ttl_ms == 0 { 0 } else { now + ttl_ms };
        let mut g = self.counters.lock().unwrap();
        let e = g.entry(key.to_string()).or_insert((0, expiry));
        if !live(e.1, now) {
            *e = (0, expiry);
        } else if extend {
            e.1 = expiry;
        }
        e.0 += 1;
        Ok(e.0)
    }
    fn counter_get(&self, key: &str) -> Result<u64> {
        let now = now_ms();
        Ok(self
            .counters
            .lock()
            .unwrap()
            .get(key)
            .filter(|(_, x)| live(*x, now))
            .map_or(0, |(n, _)| *n))
    }
    fn counter_ttl_ms(&self, key: &str) -> Result<Option<u64>> {
        let now = now_ms();
        Ok(self
            .counters
            .lock()
            .unwrap()
            .get(key)
            .filter(|(_, x)| *x > now)
            .map(|(_, x)| x - now))
    }
    fn counter_reset(&self, key: &str) -> Result<()> {
        self.counters.lock().unwrap().remove(key);
        Ok(())
    }

    fn kv_put(&self, key: &str, value: &str, ttl_ms: Option<u64>) -> Result<()> {
        let exp = ttl_ms.map_or(0, |t| now_ms() + t.max(1));
        self.kv
            .lock()
            .unwrap()
            .insert(key.to_string(), (value.to_string(), exp));
        Ok(())
    }
    fn kv_put_new(&self, key: &str, value: &str, ttl_ms: Option<u64>) -> Result<bool> {
        let now = now_ms();
        let mut g = self.kv.lock().unwrap();
        if g.get(key).is_some_and(|(_, x)| live(*x, now)) {
            return Ok(false);
        }
        let exp = ttl_ms.map_or(0, |t| now + t.max(1));
        g.insert(key.to_string(), (value.to_string(), exp));
        Ok(true)
    }
    fn kv_get(&self, key: &str) -> Result<Option<String>> {
        let now = now_ms();
        Ok(self
            .kv
            .lock()
            .unwrap()
            .get(key)
            .filter(|(_, x)| live(*x, now))
            .map(|(v, _)| v.clone()))
    }
    fn kv_delete(&self, key: &str) -> Result<bool> {
        Ok(self.kv.lock().unwrap().remove(key).is_some())
    }
    fn kv_scan(&self, prefix: &str) -> Result<Vec<(String, String)>> {
        let now = now_ms();
        Ok(self
            .kv
            .lock()
            .unwrap()
            .iter()
            .filter(|(k, (_, x))| k.starts_with(prefix) && live(*x, now))
            .map(|(k, (v, _))| (k.clone(), v.clone()))
            .collect())
    }
    fn purge_expired(&self) -> Result<usize> {
        let now = now_ms();
        let mut kv = self.kv.lock().unwrap();
        let before = kv.len();
        kv.retain(|_, (_, x)| live(*x, now));
        let mut n = before - kv.len();
        let mut c = self.counters.lock().unwrap();
        let before = c.len();
        c.retain(|_, (_, x)| live(*x, now));
        n += before - c.len();
        Ok(n)
    }

    fn export(&self, batch: usize, sink: &mut RowSink<'_>) -> Result<()> {
        // One consistent snapshot: copy under both locks, then hand it out in batches.
        let all: Vec<(String, String, Fields)> = {
            let rows = self.rows.lock().unwrap();
            let order = self.order.lock().unwrap();
            let history = self.history.lock().unwrap();
            // The records in insertion order, then the history as the rows of the model
            // `record_change` (D75), the number of the entry as the id.
            order
                .iter()
                .filter_map(|(m, i)| {
                    rows.get(m)?
                        .iter()
                        .find(|(x, _)| x == i)
                        .map(|(_, f)| (m.clone(), i.clone(), f.clone()))
                })
                .chain(history.rows.iter().map(|r| {
                    (
                        "record_change".to_string(),
                        r.seq.to_string(),
                        r.to_fields(),
                    )
                }))
                .collect()
        };
        for chunk in all.chunks(batch.max(1)) {
            sink(chunk)?;
        }
        Ok(())
    }
    fn total(&self) -> Result<usize> {
        let records: usize = self.rows.lock().unwrap().values().map(Vec::len).sum();
        Ok(records + self.history.lock().unwrap().rows.len())
    }
    fn import(
        &self,
        wipe: bool,
        next: &mut dyn FnMut() -> Result<Option<Vec<Row>>>,
    ) -> Result<usize> {
        let mut rows = self.rows.lock().unwrap();
        let mut order = self.order.lock().unwrap();
        let mut history = self.history.lock().unwrap();
        let (mut r2, mut o2, mut h2) = if wipe {
            (BTreeMap::new(), Vec::new(), MemHistory::default())
        } else {
            (rows.clone(), order.clone(), history.clone())
        };
        let mut n = 0;
        while let Some(batch) = next()? {
            for (model, id, fields) in batch {
                if model == "record_change" {
                    // An entry of the change history: the id is its number.
                    let seq = id
                        .parse::<i64>()
                        .ok()
                        .filter(|s| *s >= 1)
                        .ok_or_else(|| format!("record_change {id}: the id is not a number"))?;
                    if h2.rows.iter().any(|r| r.seq == seq) {
                        return Err(format!("duplicate record_change {seq}"));
                    }
                    h2.rows.push(ChangeRow::from_fields(seq, &fields)?);
                    h2.last_seq = h2.last_seq.max(seq);
                    n += 1;
                    continue;
                }
                let v = r2.entry(model.clone()).or_default();
                if v.iter().any(|(i, _)| *i == id) {
                    return Err(format!("duplicate {model} {id}"));
                }
                v.push((id.clone(), fields));
                o2.push((model, id));
                n += 1;
            }
        }
        h2.rows.sort_by_key(|r| r.seq);
        *rows = r2;
        *order = o2;
        *history = h2;
        if wipe {
            self.kv.lock().unwrap().clear();
            self.counters.lock().unwrap().clear();
        }
        Ok(n)
    }

    fn swap_fields(&self, swaps: &[Swap]) -> Result<usize> {
        let mut g = self.rows.lock().unwrap();
        let mut n = 0;
        for s in swaps {
            let Some((_, f)) = g
                .get_mut(&s.model)
                .and_then(|r| r.iter_mut().find(|(i, _)| *i == s.id))
            else {
                continue;
            };
            if f.get(&s.field).and_then(J::as_str) == Some(s.old.as_str()) {
                f.insert(s.field.clone(), J::from(s.new.clone()));
                n += 1;
            }
        }
        Ok(n)
    }
    fn history(&self, q: &HistoryQuery) -> Result<Vec<ChangeRow>> {
        let h = self.history.lock().unwrap();
        let mut found: Vec<ChangeRow> = h
            .rows
            .iter()
            .filter(|r| {
                r.model == q.model
                    && r.record_id == q.record_id
                    && q.before_seq.is_none_or(|s| r.seq < s)
                    && q.op.as_ref().is_none_or(|o| r.op == *o)
                    && q.since_at.as_ref().is_none_or(|a| r.at >= *a)
            })
            .cloned()
            .collect();
        found.sort_by_key(|r| std::cmp::Reverse(r.seq));
        found.truncate(q.limit);
        Ok(found)
    }
    fn backfill_sysfields(
        &self,
        models: &[String],
        fallback_at: &str,
        batch: usize,
    ) -> Result<Backfilled> {
        if sysfields::parse_micros(fallback_at).is_none() {
            return Err(format!("{fallback_at:?} is not a canonical instant"));
        }
        let mut g = self.rows.lock().unwrap();
        let mut out = Backfilled::default();
        for model in models {
            for (id, f) in g.get_mut(model).into_iter().flatten() {
                if sysfields::row_version(f).is_some() {
                    continue;
                }
                if out.stamped < batch.max(1)
                    && let Some(stamped) = sysfields::backfilled(id, f, fallback_at)
                {
                    *f = stamped;
                    out.stamped += 1;
                } else {
                    out.remaining += 1;
                }
            }
        }
        Ok(out)
    }
    fn kv_swap(&self, key: &str, old: &str, new: &str) -> Result<bool> {
        let mut g = self.kv.lock().unwrap();
        match g.get_mut(key) {
            Some((v, x)) if live(*x, now_ms()) && v == old => {
                *v = new.to_string();
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}
