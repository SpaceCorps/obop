//! Per-model extensions that later features hang on `ModelDef` (wave 2 of the storage steps).
//!
//! `ModelDef` carries one member, `ext: ModelExt`, so that adding record history (D75), declared
//! indexes (D78) or immutability guards (D77) never changes every `ModelDef` literal again. These
//! are types only: nothing reads them yet, no manifest key sets them, and every existing model has
//! `ModelExt::default()`, which means "no history setting, no declared index, no guard".

use crate::Lit;

/// What a model declares for the features that arrive after the first storage step.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModelExt {
    /// Record history (D75).
    pub history: History,
    /// Indexes the package declares for this model (D78); the automatic ones are not listed here.
    pub indexes: Vec<IndexDef>,
    /// Database-enforced immutability (D77); none: the model is not guarded.
    pub guard: Option<Guard>,
    /// The fields of the model include the platform's system fields (D75). Set by
    /// `ModelDef::with_system_fields` and by nothing else: a model whose own declared field has a
    /// reserved name (a module installed before the name was reserved) never has it, so that
    /// field stays an ordinary, writable field.
    pub system_fields: bool,
}

/// Record history of one model (D75). A model that does not turn it off keeps the full history,
/// for ever.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct History {
    /// Whether the platform keeps history for this model and offers the generated command
    /// `<model>.history`. `false` in `History::default()`, and no manifest key sets it: the
    /// command layer switches it on for the models it records history for, which it does not yet
    /// (the first slice of D75 only prepares the storage), so no model has the command yet.
    pub enabled: bool,
    /// Why the model keeps no history (`history = "none"` and `history_reason` in the manifest);
    /// `None` means it keeps the full history.
    pub off_reason: Option<&'static str>,
    /// Days to keep history entries; `None` keeps them for ever.
    pub retention_days: Option<u32>,
}

// ---- declared indexes (D78) ----------------------------------------------------------------

/// The kind of an index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexKind {
    /// Ordered index over one to four stored fields.
    Btree,
    /// Trigram index over exactly one text field, for `contains`.
    Trigram,
    /// Members index over a `shared_with` field; created automatically only.
    Members,
}

/// The comparison of one condition of a partial index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CondOp {
    Eq,
    Ne,
    In,
    /// `true`: the field is null; `false`: the field is not null.
    IsNull(bool),
}

/// One condition of a partial index (`where` in the manifest). Literals are the same type the
/// rules use; `Lit::Null` is never valid here, `CondOp::IsNull` covers it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexCond {
    pub field: &'static str,
    pub op: CondOp,
    pub values: Vec<Lit>,
}

/// An index on one model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexDef {
    /// Stored fields in order, each with `true` for descending. The id is always appended.
    pub cols: Vec<(&'static str, bool)>,
    pub kind: IndexKind,
    /// Conditions of a partial index; empty for a plain index.
    pub filter: Vec<IndexCond>,
}

// ---- immutability guards (D77) --------------------------------------------------------------

/// The states of a governed select field in which a rule applies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateSet {
    pub field: &'static str,
    pub values: Vec<&'static str>,
}

/// An edge that leaves an immutable state, by a declared action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exit {
    /// The state the action moves the record to.
    pub to: &'static str,
    /// The declared transition action that is allowed to do it.
    pub action: &'static str,
    /// Stored fields that action may change on the way out.
    pub may_change: Vec<&'static str>,
}

/// A date field and the setting that holds the lock date; records dated on or before it are frozen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockDate {
    pub field: &'static str,
    pub setting: &'static str,
}

/// The guard a model declares (D77): when its records are frozen, and what may still change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Guard {
    /// The records in these states are frozen.
    pub immutable_when: Option<StateSet>,
    /// Stored fields that stay editable on a frozen record.
    pub mutable_fields: Vec<&'static str>,
    /// The only moves a frozen record may make.
    pub exit_to: Vec<Exit>,
    /// The records in these states cannot be deleted.
    pub no_delete_when: Option<StateSet>,
    /// Records dated on or before the lock date are frozen.
    pub lock_date: Option<LockDate>,
    /// A ref field of this model: a record is frozen while its parent is.
    pub children_of: Option<&'static str>,
    /// Records can be created and read, never changed or deleted.
    pub append_only: bool,
}
