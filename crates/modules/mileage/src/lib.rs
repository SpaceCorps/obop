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
    guards::install(d, &m.id, db.clone(), clock.clone());
    handlers::install(d, &m, db.clone(), clock.clone());
    audit::install(d, db, clock);
    // The discards declare no `from`; the policy lets them start only before confirm, so `meta`
    // does not count them as a way out of a confirmed trip (U76).
    for a in ["discard", "discard_stuck"] {
        let states = policy::starts_in(a).unwrap_or_default();
        d.starts_only_in(&m.id, &format!("{}.{a}", policy::TRIP), states);
    }
}

/// Removes the route of every private trip that is no longer recording but still holds one
/// (stored by an earlier build of the module). The answer is the number of trips cleared.
pub fn drop_private_routes(store: &Store) -> Result<usize, String> {
    handlers::drop_private_routes(&store.backend()).map(|(cleared, _)| cleared)
}

/// The row of the migration ledger (`obop_migration`) that records that `drop_private_routes`
/// has run to its end on this database (U81).
pub const PRIVATE_ROUTES_STEP: &str = "mileage/0001";

const PRIVATE_ROUTES_NAME: &str =
    "drop_private_routes: remove the route of private trips stored before a private trip lost it";

/// `drop_private_routes` once per database (U81): `serve` calls it at every start, and it does
/// nothing (`Ok(None)`) when the migration ledger holds `PRIVATE_ROUTES_STEP`. Otherwise it runs,
/// and when no trip was left because it changed meanwhile, it writes that row (module `mileage`,
/// kind `rust`, step `0001`, applied, with the count in its note), so that `obop migrate status`
/// lists it and later starts skip the search. A trip that changed meanwhile leaves the row
/// unwritten, so the next start runs it again. The answer is the number of trips cleared.
pub fn drop_private_routes_once(store: &Store) -> Result<Option<usize>, String> {
    use obop_migrate::ledger::{self, Entry};
    let db = store.backend();
    if let Some(row) = db.get(ledger::MODEL, PRIVATE_ROUTES_STEP)?
        && Entry::from_fields(PRIVATE_ROUTES_STEP, &row)
            .is_some_and(|e| e.status == ledger::Status::Applied)
    {
        return Ok(None);
    }
    let began = ledger::now_ms();
    let (cleared, left) = handlers::drop_private_routes(&db)?;
    if left == 0 {
        let entry = Entry {
            id: PRIVATE_ROUTES_STEP.into(),
            module: "mileage".into(),
            kind: "rust".into(),
            step: "0001".into(),
            version: None,
            name: PRIVATE_ROUTES_NAME.into(),
            class: ledger::Class::Backfill,
            ddl: ledger::Ddl::None,
            checksum: ledger::sha256_hex(PRIVATE_ROUTES_NAME.as_bytes()),
            status: ledger::Status::Applied,
            at: ledger::now_secs(),
            duration_ms: ledger::now_ms().saturating_sub(began),
            by: "server".into(),
            role: "application".into(),
            app_version: obop_types::CORE_VERSION.into(),
            backup_id: None,
            restore_point: None,
            note: format!("{cleared} private trip(s) cleared"),
            attempts: 1,
        };
        db.apply(&[entry.put()])?;
    }
    Ok(Some(cleared))
}

#[cfg(test)]
mod tests;
