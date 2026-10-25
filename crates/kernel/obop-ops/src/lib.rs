//! Operator tools that work on a whole database: backup, restore and verification
//! ([`backup`]), scheduled backups with retention and off-host copies ([`schedule`],
//! [`retention`], [`sink`], [`catalog`]), migration gates ([`gates`]), restore drills ([`drill`]),
//! storage checks ([`doctor`], [`pgadmin`]), and secret rotation ([`secrets`]). The `obop`
//! binary wires them to the command line; nothing here talks to a network except the database
//! it is given and the sinks it is handed.

pub mod backup;
pub mod catalog;
pub mod doctor;
pub mod drill;
pub mod gates;
pub mod pgadmin;
pub mod retention;
pub mod schedule;
pub mod secrets;
pub mod sink;

/// One line in the security log (`security_event`) in the shape `obop-commands` writes it, so a
/// migration rollback shows up beside the other things that happened to the instance. A failure
/// to write is reported and never stops the action being logged.
pub(crate) fn audit_event(
    backend: &dyn obop_storage::Backend,
    at: u64,
    actor: &str,
    kind: &str,
    detail: &str,
) {
    let mut f = obop_storage::Fields::new();
    f.insert("user".into(), actor.into());
    f.insert("kind".into(), kind.into());
    f.insert("actor".into(), actor.into());
    f.insert("detail".into(), detail.into());
    f.insert("at".into(), backup::rfc3339(at).into());
    let id = format!("{at:012}-{}", obop_keys::hex(&obop_keys::random::<6>()));
    if let Err(e) = backend.insert("security_event", &id, &f) {
        eprintln!("could not write security event {kind}: {e}");
    }
}
