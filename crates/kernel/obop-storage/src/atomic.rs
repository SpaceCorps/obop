//! Atomic batches: the primitive for checks that must not race (the last unit of stock, a
//! voucher's last use, a state that changes once). Each backend runs [`run_ops`] inside its own
//! transaction (a lock in memory, `BEGIN IMMEDIATE` in SQLite, row locks in PostgreSQL).

use crate::{Fields, Op, Result};
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
}

pub(crate) fn run_ops(tx: &mut dyn TxView, ops: &[Op]) -> std::result::Result<(), ApplyError> {
    let conflict = |index: usize, reason: &str| ApplyError::Conflict {
        index,
        reason: reason.into(),
    };
    let st = ApplyError::Storage;
    for (index, op) in ops.iter().enumerate() {
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
        }
    }
    Ok(())
}
