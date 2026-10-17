//! The in-memory backend: tests, `memory:` databases and the contract's reference.

use super::*;

#[derive(Default)]
pub(crate) struct Memory {
    rows: Mutex<BTreeMap<String, Vec<(String, Fields)>>>,
    /// Insertion order across models, for `export`.
    order: Mutex<Vec<(String, String)>>,
    locks: Arc<LocalLocks>,
    /// key -> (value, expiry in ms; 0 = never)
    counters: Mutex<BTreeMap<String, (u64, u64)>>,
    kv: Mutex<BTreeMap<String, (String, u64)>>,
}

impl Memory {
    fn note(&self, model: &str, id: &str) {
        self.order.lock().unwrap().push((model.into(), id.into()));
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
        struct M<'a>(&'a mut BTreeMap<String, Vec<(String, Fields)>>);
        impl TxView for M<'_> {
            fn tx_get(&mut self, model: &str, id: &str) -> Result<Option<Fields>> {
                Ok(self
                    .0
                    .get(model)
                    .and_then(|r| r.iter().find(|(i, _)| i == id))
                    .map(|(_, f)| f.clone()))
            }
            fn tx_insert(&mut self, model: &str, id: &str, fields: &Fields) -> Result<()> {
                self.0
                    .entry(model.into())
                    .or_default()
                    .push((id.into(), fields.clone()));
                Ok(())
            }
            fn tx_put(&mut self, model: &str, id: &str, fields: &Fields) -> Result<()> {
                if let Some((_, f)) = self
                    .0
                    .get_mut(model)
                    .and_then(|r| r.iter_mut().find(|(i, _)| i == id))
                {
                    *f = fields.clone();
                }
                Ok(())
            }
            fn tx_delete(&mut self, model: &str, id: &str) -> Result<()> {
                if let Some(r) = self.0.get_mut(model) {
                    r.retain(|(i, _)| i != id);
                }
                Ok(())
            }
        }
        let mut g = self.rows.lock().unwrap();
        let backup = g.clone();
        let r = run_ops(&mut M(&mut g), ops);
        if r.is_err() {
            *g = backup;
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
            order
                .iter()
                .filter_map(|(m, i)| {
                    rows.get(m)?
                        .iter()
                        .find(|(x, _)| x == i)
                        .map(|(_, f)| (m.clone(), i.clone(), f.clone()))
                })
                .collect()
        };
        for chunk in all.chunks(batch.max(1)) {
            sink(chunk)?;
        }
        Ok(())
    }
    fn total(&self) -> Result<usize> {
        Ok(self.rows.lock().unwrap().values().map(Vec::len).sum())
    }
    fn import(
        &self,
        wipe: bool,
        next: &mut dyn FnMut() -> Result<Option<Vec<Row>>>,
    ) -> Result<usize> {
        let mut rows = self.rows.lock().unwrap();
        let mut order = self.order.lock().unwrap();
        let (mut r2, mut o2) = if wipe {
            (BTreeMap::new(), Vec::new())
        } else {
            (rows.clone(), order.clone())
        };
        let mut n = 0;
        while let Some(batch) = next()? {
            for (model, id, fields) in batch {
                let v = r2.entry(model.clone()).or_default();
                if v.iter().any(|(i, _)| *i == id) {
                    return Err(format!("duplicate {model} {id}"));
                }
                v.push((id.clone(), fields));
                o2.push((model, id));
                n += 1;
            }
        }
        *rows = r2;
        *order = o2;
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
