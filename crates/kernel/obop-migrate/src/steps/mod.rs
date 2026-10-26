//! The core migration steps, one file each. The text of a step's file is part of its checksum in
//! the ledger (see `ledger::source_digest`): editing an applied step stops every database that
//! recorded it from starting, so a change to what a step does is always a new step.

pub(crate) mod s0001_adopt;
pub(crate) mod s0002_seal_format;
