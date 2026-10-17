//! `connectors`: connect external systems and let people connect their own accounts, with the
//! controls that support GDPR compliance built in (D54).
//!
//! The engines live in `obop-connectors`; this crate is the module: registry metadata (so REST,
//! MCP, the CLI and the backoffice are generated), the guards that scope every record to its
//! owner, and the handlers that call the engines.

mod guards;
mod h;
mod handlers;
mod labels;
pub mod meta;

pub use guards::blocked_model;
pub use meta::{ADMIN, USER, module};

use obop_commands::{Dispatcher, Store};
use obop_connectors::Runtime;
use std::sync::Arc;

/// Bind the module's commands to the dispatcher. `rt` is the shared runtime (store, secrets,
/// the hardened client); call `rt.start()` afterwards to run events, retries and the schedule.
pub fn register(d: &Arc<Dispatcher>, store: &Store, rt: &Arc<Runtime>) {
    let m = module();
    handlers::install(d, &m, store, rt);
    guards::install(d, rt);
}

#[cfg(test)]
mod tests;
