//! K5: how long a process waits for the migration lock is a setting (owner default of
//! 2026-11-18: `OBOP_MIGRATION_LOCK_WAIT_SECS`, default 300 s); every other lock keeps 60 s.

use crate::{
    LOCK_WAIT, MIGRATION_LOCK, MIGRATION_LOCK_WAIT_DEFAULT, lock_wait, migration_lock_wait_from,
};
use std::time::Duration;

#[test]
fn the_migration_lock_wait_is_a_setting_with_a_default_of_five_minutes() {
    assert_eq!(MIGRATION_LOCK_WAIT_DEFAULT, Duration::from_secs(300));
    assert_eq!(migration_lock_wait_from(None), Ok(Duration::from_secs(300)));
    assert_eq!(
        migration_lock_wait_from(Some(" ")),
        Ok(Duration::from_secs(300))
    );
    assert_eq!(
        migration_lock_wait_from(Some("900")),
        Ok(Duration::from_secs(900))
    );
    assert_eq!(
        migration_lock_wait_from(Some("1")),
        Ok(Duration::from_secs(1))
    );
    for bad in ["0", "-5", "5m", "86401", "1.5"] {
        let e = migration_lock_wait_from(Some(bad)).unwrap_err();
        assert!(e.contains("OBOP_MIGRATION_LOCK_WAIT_SECS"), "{bad}: {e}");
    }
}

#[test]
fn only_the_migration_lock_takes_the_setting() {
    assert_eq!(lock_wait("obop.write"), Ok(LOCK_WAIT));
    assert_eq!(LOCK_WAIT, Duration::from_secs(60));
    if std::env::var_os("OBOP_MIGRATION_LOCK_WAIT_SECS").is_none() {
        assert_eq!(lock_wait(MIGRATION_LOCK), Ok(MIGRATION_LOCK_WAIT_DEFAULT));
    }
    assert_eq!(MIGRATION_LOCK, "obop.migrate");
}
