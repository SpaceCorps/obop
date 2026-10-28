//! `mileage`: trips recorded by the Mileage app, checked on the server, reviewed and confirmed in
//! the back office (docs/spec/mobile.md 5, D87).
//!
//! The module is built in. The rules that the generated commands cannot state (a write right
//! that depends on the state of the record, a read right that depends on it, the separation of
//! duties, the distance computed from the children, a refusal a client may repeat) are written
//! here, in guards and handlers, and hold on REST, MCP and the command line alike because the
//! dispatcher is the one path of all three. What stays a generic capability for later is listed in
//! the spec (5.8) and in docs/ISSUES.md.

mod audit;
mod guards;
mod handlers;
mod model;
pub mod policy;
pub mod track;

pub use model::module;

use obop_commands::{Dispatcher, Store};
use std::sync::Arc;

/// Microseconds since the Unix epoch. Replaceable so tests can place `confirmed_at` in a month.
pub type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

pub fn system_clock() -> Clock {
    Arc::new(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_micros() as i64)
    })
}

/// Bind the commands of the module to the dispatcher.
pub fn register(d: &Dispatcher, store: &Store) {
    register_with_clock(d, store, system_clock());
}

/// As [`register`], with the clock `confirm` stamps `confirmed_at` from.
pub fn register_with_clock(d: &Dispatcher, store: &Store, clock: Clock) {
    let m = module();
    d.register_crud(&m, store);
    let db = store.backend();
    guards::install(d, &m.id, db.clone());
    handlers::install(d, &m, db.clone(), clock.clone());
    audit::install(d, db, clock);
}

#[cfg(test)]
mod tests;
