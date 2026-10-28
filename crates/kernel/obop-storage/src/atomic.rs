//! Atomic batches: the primitive for checks that must not race (the last unit of stock, a
//! voucher's last use, a state that changes once). Each backend runs [`run_ops`] inside its own
//! transaction (a lock in memory, `BEGIN IMMEDIATE` in SQLite, row locks in PostgreSQL).

use crate::sysfields::{ChangeRow, Stage};
use crate::tracked::Tracking;
use crate::{Fields, Op, Result, WriteCtx};
use serde_json::Value as J;

/// Why an atomic batch was not applied. Nothing was changed in either case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyError {
    /// The step at `index` could not be applied (a bound was crossed, a record is missing or
    /// already exists, an expected value differs).
    Conflict {
        index: usize,
        reason: String,
    },
    Storage(String),
}

impl std::fmt::Display for ApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Conflict { index, reason } => write!(f, "step {index}: {reason}"),
            Self::Storage(e) => f.write_str(e),
        }
    }
}

/// The records a batch works on, inside one transaction.
pub(crate) trait TxView {
    fn tx_get(&mut self, model: &str, id: &str) -> Result<Option<Fields>>;
    fn tx_insert(&mut self, model: &str, id: &str, fields: &Fields) -> Result<()>;
    fn tx_put(&mut self, model: &str, id: &str, fields: &Fields) -> Result<()>;
    fn tx_delete(&mut self, model: &str, id: &str) -> Result<()>;
    /// Hand the database the tickets of the write (D77): the transaction-local setting
    /// `obop.ctx`, which its guard triggers read. Backends without guards ignore it.
    fn tx_context(&mut self, _json: &str) -> Result<()> {
        Ok(())
    }
    /// The first read of a record of a tracked model in a transaction (D75): the record, locked.
    /// A backend that can learn the clock in the same statement may do so here and answer from it
    /// in `tx_now`: PostgreSQL does (the record and the database's clock in one statement, the
    /// clock read after the lock is granted); the others have this default and the clock of the
    /// process.
    fn tx_get_clock(&mut self, model: &str, id: &str) -> Result<Option<Fields>> {
        self.tx_get(model, id)
    }
    /// The instant a stamp takes, in microseconds since 1970: later at every call. A backend with
    /// a clock of its own answers with it; the default is the clock of the process.
    fn tx_now(&mut self) -> Result<i64> {
        Ok(crate::sysfields::process_micros())
    }
    /// Append entries to the change history, in order, in this transaction. A backend that keeps
    /// no history refuses, so that a model that records history cannot lose it unnoticed.
    fn tx_history(&mut self, rows: &[ChangeRow]) -> Result<()> {
        let _ = rows;
        Err("this storage backend keeps no change history".into())
    }
    /// Erase the values from the entries of the history of a record, except the fields in `keep`,
    /// and append the entry that says it was done (`at` is microseconds since 1970). Returns how
    /// many entries were rewritten.
    #[allow(clippy::too_many_arguments)]
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
        let _ = (model, id, keep, actor, via, row_version, at);
        Err("this storage backend keeps no change history".into())
    }
}

/// Run `ops` in the transaction `tx`. `ctx` says who writes and with which tickets; `tracking`
/// says which models keep system fields and history (D75): a step on a model that is not tracked
/// does exactly what it always did, a step on a tracked one goes through the planner of
/// `sysfields`, which stamps the record, compares it with what it was and writes the history. The
/// tickets of `ctx` reach the database before the first step (D77).
pub(crate) fn run_ops(
    tx: &mut dyn TxView,
    ops: &[Op],
    ctx: &WriteCtx,
    tracking: &Tracking,
) -> std::result::Result<(), ApplyError> {
    if let Some(setting) = ctx.setting() {
        tx.tx_context(&setting).map_err(ApplyError::Storage)?;
    }
    let erases = ops.iter().any(|o| matches!(o, Op::EraseHistory { .. }));
    // The common case, no model tracked and nothing to erase, keeps no state at all.
    let mut stage = (!tracking.is_empty() || erases).then(|| Stage::new(ctx, tracking));
    for (index, op) in ops.iter().enumerate() {
        if let Some(stage) = stage.as_mut() {
            if let Some(track) = stage.track_of(op.model()) {
                stage.op(tx, index, op, &track)?;
                continue;
            }
            if let Op::EraseHistory { model, id } = op {
                stage.queue_erase(model, id);
                continue;
            }
        }
        plain(tx, index, op)?;
    }
    match stage {
        Some(stage) => stage.finish(tx),
        None => Ok(()),
    }
}

/// One step on a record of a model that is not tracked.
fn plain(tx: &mut dyn TxView, index: usize, op: &Op) -> std::result::Result<(), ApplyError> {
    let conflict = |index: usize, reason: &str| ApplyError::Conflict {
        index,
        reason: reason.into(),
    };
    let st = ApplyError::Storage;
    {
        match op {
            Op::Insert { model, id, fields } => {
                if tx.tx_get(model, id).map_err(st)?.is_some() {
                    return Err(conflict(index, "already exists"));
                }
                tx.tx_insert(model, id, fields).map_err(st)?;
            }
            Op::Add {
                model,
                id,
                field,
                delta,
                min,
                max,
                create,
            } => {
                let existing = tx.tx_get(model, id).map_err(st)?;
                let creating = existing.is_none();
                let mut rec = match (existing, create) {
                    (Some(r), _) => r,
                    (None, Some(c)) => c.clone(),
                    (None, None) => return Err(conflict(index, "no such record")),
                };
                let cur = match rec.get(field) {
                    None | Some(J::Null) => 0,
                    Some(v) => v
                        .as_i64()
                        .ok_or_else(|| conflict(index, "the field is not an integer"))?,
                };
                let new = cur
                    .checked_add(*delta)
                    .ok_or_else(|| conflict(index, "overflow"))?;
                if min.is_some_and(|m| new < m) {
                    return Err(conflict(index, "below the minimum"));
                }
                if max.is_some_and(|m| new > m) {
                    return Err(conflict(index, "above the maximum"));
                }
                rec.insert(field.clone(), J::from(new));
                if creating {
                    tx.tx_insert(model, id, &rec).map_err(st)?;
                } else {
                    tx.tx_put(model, id, &rec).map_err(st)?;
                }
            }
            Op::Patch {
                model,
                id,
                patch,
                expect,
            } => {
                let Some(mut rec) = tx.tx_get(model, id).map_err(st)? else {
                    return Err(conflict(index, "no such record"));
                };
                if expect
                    .iter()
                    .any(|(k, v)| rec.get(k).unwrap_or(&J::Null) != v)
                {
                    return Err(conflict(index, "the record changed"));
                }
                rec.extend(patch.clone());
                tx.tx_put(model, id, &rec).map_err(st)?;
            }
            Op::Put { model, id, fields } => {
                if tx.tx_get(model, id).map_err(st)?.is_some() {
                    tx.tx_put(model, id, fields).map_err(st)?;
                } else {
                    tx.tx_insert(model, id, fields).map_err(st)?;
                }
            }
            Op::Merge { model, id, patch } => {
                let Some(mut rec) = tx.tx_get(model, id).map_err(st)? else {
                    return Err(conflict(index, "no such record"));
                };
                rec.extend(patch.clone());
                tx.tx_put(model, id, &rec).map_err(st)?;
            }
            Op::Delete { model, id } => {
                if tx.tx_get(model, id).map_err(st)?.is_none() {
                    return Err(conflict(index, "no such record"));
                }
                tx.tx_delete(model, id).map_err(st)?;
            }
            // `run_ops` takes these before this function, whenever the batch has one.
            Op::EraseHistory { .. } => {
                return Err(ApplyError::Storage(
                    "an erasure of history reached the plain path".into(),
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// A transaction that only keeps a log of what it was asked, in order.
    #[derive(Default)]
    struct Log {
        rows: BTreeMap<(String, String), Fields>,
        calls: Vec<String>,
        refuse_context: bool,
    }

    impl TxView for Log {
        fn tx_get(&mut self, model: &str, id: &str) -> Result<Option<Fields>> {
            self.calls.push(format!("get {model}/{id}"));
            Ok(self.rows.get(&(model.into(), id.into())).cloned())
        }
        fn tx_insert(&mut self, model: &str, id: &str, fields: &Fields) -> Result<()> {
            self.calls.push(format!("insert {model}/{id}"));
            self.rows.insert((model.into(), id.into()), fields.clone());
            Ok(())
        }
        fn tx_put(&mut self, model: &str, id: &str, fields: &Fields) -> Result<()> {
            self.calls.push(format!("put {model}/{id}"));
            self.rows.insert((model.into(), id.into()), fields.clone());
            Ok(())
        }
        fn tx_delete(&mut self, model: &str, id: &str) -> Result<()> {
            self.calls.push(format!("delete {model}/{id}"));
            self.rows.remove(&(model.into(), id.into()));
            Ok(())
        }
        fn tx_context(&mut self, json: &str) -> Result<()> {
            self.calls.push(format!("context {json}"));
            if self.refuse_context {
                return Err("the database refused the context".into());
            }
            Ok(())
        }
    }

    fn insert(id: &str) -> Op {
        Op::Insert {
            model: "m".into(),
            id: id.into(),
            fields: Fields::new(),
        }
    }

    #[test]
    fn the_tickets_reach_the_transaction_once_before_the_first_step() {
        let mut tx = Log::default();
        let ctx = WriteCtx::new("ann", "rest", "m.post")
            .ticket("m", "a", "post")
            .ticket("m", "b", "post");
        run_ops(
            &mut tx,
            &[insert("a"), insert("b")],
            &ctx,
            Tracking::empty(),
        )
        .unwrap();
        assert_eq!(tx.calls.len(), 1 + 4, "{:?}", tx.calls);
        assert_eq!(tx.calls[0], format!("context {}", ctx.setting().unwrap()));
        assert_eq!(tx.calls[1], "get m/a");
        assert_eq!(
            tx.calls.iter().filter(|c| c.starts_with("context")).count(),
            1
        );
    }

    #[test]
    fn a_write_without_tickets_sends_no_context() {
        let mut tx = Log::default();
        // Who writes and how does not reach the database: only tickets do.
        let ctx = WriteCtx::new("ann", "rest", "m.create").batch("b").run("r");
        run_ops(&mut tx, &[insert("a")], &ctx, Tracking::empty()).unwrap();
        assert_eq!(tx.calls, ["get m/a", "insert m/a"]);
    }

    #[test]
    fn a_context_the_database_refuses_stops_the_batch_before_any_step() {
        let mut tx = Log {
            refuse_context: true,
            ..Log::default()
        };
        let ctx = WriteCtx::system("x").ticket("m", "a", "post");
        let e = run_ops(&mut tx, &[insert("a")], &ctx, Tracking::empty()).unwrap_err();
        assert_eq!(
            e,
            ApplyError::Storage("the database refused the context".into())
        );
        assert_eq!(tx.calls.len(), 1, "no step ran: {:?}", tx.calls);
        assert!(tx.rows.is_empty());
    }

    #[test]
    fn the_steps_behave_as_before_whatever_the_context() {
        // The context and the tracking are for the layers built on this seam; the steps
        // themselves do exactly what they did.
        let ctx = WriteCtx::new("ann", "rest", "x");
        let mut a = Log::default();
        let mut b = Log::default();
        let ops = [
            insert("a"),
            Op::Merge {
                model: "m".into(),
                id: "a".into(),
                patch: Fields::new(),
            },
            Op::Delete {
                model: "m".into(),
                id: "a".into(),
            },
        ];
        run_ops(&mut a, &ops, &ctx, Tracking::empty()).unwrap();
        run_ops(&mut b, &ops, &WriteCtx::system(""), Tracking::empty()).unwrap();
        assert_eq!(a.calls, b.calls);
    }
}
