//! Atomic batches: the primitive for checks that must not race (the last unit of stock, a
//! voucher's last use, a state that changes once). Each backend runs [`run_ops`] inside its own
//! transaction (a lock in memory, `BEGIN IMMEDIATE` in SQLite, row locks in PostgreSQL).

use crate::{Fields, Result};
use serde_json::Value as J;

/// One step of an atomic batch (see [`crate::Backend::apply`]).
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    /// Insert a record; a duplicate id is a conflict.
    Insert {
        model: String,
        id: String,
        fields: Fields,
    },
    /// Add `delta` to the integer `field` (absent or null counts as 0) and refuse the whole batch
    /// if the result leaves `[min, max]` or overflows. With `create`, a missing record is
    /// inserted with those fields first (an upsert counter); without it a missing record is a
    /// conflict. The check and the change happen under one lock or row lock, so two callers
    /// cannot both take the last unit.
    Add {
        model: String,
        id: String,
        field: String,
        delta: i64,
        min: Option<i64>,
        max: Option<i64>,
        create: Option<Fields>,
    },
    /// Merge `patch` into a record only if every `expect` field currently equals the given value
    /// (compare-and-set, for state changes that must happen once). A missing field equals null.
    Patch {
        model: String,
        id: String,
        patch: Fields,
        expect: Vec<(String, J)>,
    },
    /// Remove a record; a missing record is not an error.
    Delete { model: String, id: String },
}

impl Op {
    pub fn add(model: &str, id: &str, field: &str, delta: i64) -> Op {
        Op::Add {
            model: model.into(),
            id: id.into(),
            field: field.into(),
            delta,
            min: None,
            max: None,
            create: None,
        }
    }

    pub fn min(mut self, v: i64) -> Op {
        if let Op::Add { min, .. } = &mut self {
            *min = Some(v);
        }
        self
    }

    pub fn max(mut self, v: i64) -> Op {
        if let Op::Add { max, .. } = &mut self {
            *max = Some(v);
        }
        self
    }

    pub fn or_create(mut self, fields: Fields) -> Op {
        if let Op::Add { create, .. } = &mut self {
            *create = Some(fields);
        }
        self
    }
}

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
            Op::Delete { model, id } => tx.tx_delete(model, id).map_err(st)?,
        }
    }
    Ok(())
}
