use super::*;
use crate::catalog::{Attestation, BackupRecord, Catalog, Copy, DrillRecord};
use crate::pgadmin::{ArchiverStatus, FakeServer};

const NOW: u64 = 1_800_000_000;

fn backup(age: u64, format: u64, copied: bool) -> BackupRecord {
    BackupRecord {
        id: format!("b{age}"),
        path: "/x".into(),
        created_at: NOW - age,
        format,
        key_id: (format == 2).then(|| "abcd1234".into()),
        bytes: 100,
        records: 5,
        schema_version: 2,
        duration_ms: 1500,
        verified: true,
        authenticated: format == 2,
        reason: "scheduled".into(),
        copies: if copied {
            vec![Copy {
                sink: "fs:/mnt".into(),
                name: format!("b{age}"),
                verified_at: NOW - age,
            }]
        } else {
            Vec::new()
        },
    }
}

fn attest(age: u64) -> Attestation {
    Attestation {
        plane: "main".into(),
        host: "db1".into(),
        provider: "Acme".into(),
        mechanism: "volume encryption AES-256".into(),
        key_custody: "Acme KMS".into(),
        key_readers: "the KMS only".into(),
        at: NOW - age,
        evidence: "ticket 7".into(),
        attested_by: "root".into(),
    }
}

fn drill(age: u64, passed: bool) -> DrillRecord {
    DrillRecord {
        id: format!("drill-{age}"),
        at: NOW - age,
        plane: "main".into(),
        backup_id: "b".into(),
        rpo_seconds: 100,
        rto_seconds: 20,
        records: 5,
        checks: Vec::new(),
        passed,
    }
}

/// A database with everything in order.
fn healthy() -> std::sync::Arc<dyn Backend> {
    let b = obop_storage::open("memory:").unwrap();
    let c = Catalog::new(&*b);
    c.record_backup(&backup(3600, 2, true)).unwrap();
    c.record_drill(&drill(86_400 * 10, true)).unwrap();
    c.attest(&attest(86_400 * 30)).unwrap();
    b
}

fn run(
    b: &dyn Backend,
    server: Option<&dyn ServerInfo>,
    sslmode: Option<&str>,
    local: bool,
) -> Report {
    check(&Inputs {
        backend: b,
        production: true,
        server,
        sslmode,
        host_is_local: local,
        plane: "main",
        now: NOW,
    })
    .unwrap()
}

fn level(r: &Report, name: &str) -> Level {
    r.find(name)
        .unwrap_or_else(|| panic!("no check {name}: {}", r.render()))
        .level
}

#[test]
fn a_well_configured_host_with_fresh_evidence_passes_every_check() {
    let b = healthy();
    let s = FakeServer::well_configured();
    let r = run(&*b, Some(&s), Some("verify-full"), false);
    assert!(!r.failed(), "{}", r.render());
    assert_eq!(r.worst(), Level::Pass, "{}", r.render());
    for n in [
        "wal_level",
        "archive_mode",
        "archive_command",
        "archive_timeout",
        "archiver",
        "tls",
        "backup",
        "backup_encrypted",
        "backup_offsite",
        "restore_drill",
        "storage_attestation",
    ] {
        assert_eq!(level(&r, n), Level::Pass, "{n}: {}", r.render());
    }
    assert!(r.render().contains("storage_attestation"));
}

#[test]
fn each_setting_that_breaks_point_in_time_recovery_fails_and_says_what_to_set() {
    let b = healthy();
    for (k, v, check) in [
        ("wal_level", "minimal", "wal_level"),
        ("archive_mode", "off", "archive_mode"),
        ("archive_command", "", "archive_command"),
        ("archive_command", "(disabled)", "archive_command"),
        ("archive_command", "/bin/true", "archive_command"),
        ("archive_timeout", "0", "archive_timeout"),
        ("archive_timeout", "5min", "archive_timeout"),
        ("fsync", "off", "durability"),
        ("full_page_writes", "off", "durability"),
    ] {
        let s = FakeServer::well_configured().with(k, v);
        let r = run(&*b, Some(&s), Some("verify-full"), false);
        assert_eq!(level(&r, check), Level::Fail, "{k}={v}: {}", r.render());
        assert!(r.failed());
    }
    // An archive library instead of a command is fine.
    let s = FakeServer::well_configured()
        .with("archive_command", "")
        .with("archive_library", "basic_archive");
    assert_eq!(
        level(&run(&*b, Some(&s), None, true), "archive_command"),
        Level::Pass
    );
    // The command is never printed: it may hold a credential.
    let s = FakeServer::well_configured()
        .with("archive_command", "aws s3 cp %p s3://k --secret HUNTER2");
    assert!(!run(&*b, Some(&s), None, true).render().contains("HUNTER2"));
}

#[test]
fn an_archiver_that_is_late_failing_or_never_ran_fails() {
    let b = healthy();
    let mut s = FakeServer::well_configured();
    s.archiver.last_archived_age = Some(301);
    let r = run(&*b, Some(&s), None, true);
    // Late without a failure: an idle database looks the same, so it warns (the metric alerts).
    assert_eq!(level(&r, "archiver"), Level::Warn);
    assert!(r.find("archiver").unwrap().detail.contains("301 s"));
    assert!(r.find("archiver").unwrap().detail.contains("idle"));
    let mut s = FakeServer::well_configured();
    s.archiver.last_archived_age = Some(900);
    s.archiver.last_failed_age = Some(100);
    assert_eq!(
        level(&run(&*b, Some(&s), None, true), "archiver"),
        Level::Fail
    );
    let mut s = FakeServer::well_configured();
    s.archiver = ArchiverStatus::default();
    assert_eq!(
        level(&run(&*b, Some(&s), None, true), "archiver"),
        Level::Fail
    );
    let mut s = FakeServer::well_configured();
    s.archiver.last_failed_age = Some(10);
    s.archiver.last_archived_age = Some(100);
    let r = run(&*b, Some(&s), None, true);
    assert_eq!(level(&r, "archiver"), Level::Fail);
    assert!(r.find("archiver").unwrap().detail.contains("FAILED"));
    // An old failure followed by a success is not a failure now.
    let mut s = FakeServer::well_configured();
    s.archiver.last_failed_age = Some(5000);
    assert_eq!(
        level(&run(&*b, Some(&s), None, true), "archiver"),
        Level::Pass
    );
}

#[test]
fn tls_is_judged_by_where_the_database_is_and_what_the_url_says() {
    let b = healthy();
    let s = FakeServer::well_configured();
    // A remote host over a plain connection fails; require is a warning; verify-full passes.
    assert_eq!(level(&run(&*b, Some(&s), None, false), "tls"), Level::Fail);
    assert_eq!(
        level(&run(&*b, Some(&s), Some("require"), false), "tls"),
        Level::Warn
    );
    assert_eq!(
        level(&run(&*b, Some(&s), Some("verify-full"), false), "tls"),
        Level::Pass
    );
    // A server with ssl off fails whatever the URL says, unless it is local.
    let off = FakeServer::well_configured().with("ssl", "off");
    assert_eq!(
        level(&run(&*b, Some(&off), Some("require"), false), "tls"),
        Level::Fail
    );
    assert_eq!(level(&run(&*b, Some(&off), None, true), "tls"), Level::Pass);
}

#[test]
fn old_or_missing_evidence_fails_and_a_young_database_that_is_not_postgresql_only_warns() {
    let b = obop_storage::open("memory:").unwrap();
    let s = FakeServer::well_configured();
    let r = run(&*b, Some(&s), Some("verify-full"), false);
    for n in [
        "backup",
        "backup_offsite",
        "restore_drill",
        "storage_attestation",
    ] {
        assert_eq!(level(&r, n), Level::Fail, "{n}: {}", r.render());
    }
    // Stale evidence.
    let c = Catalog::new(&*b);
    c.record_backup(&backup(27 * 3600, 2, true)).unwrap();
    c.record_drill(&drill(36 * 86_400, true)).unwrap();
    c.attest(&attest(366 * 86_400)).unwrap();
    let r = run(&*b, Some(&s), Some("verify-full"), false);
    assert_eq!(level(&r, "backup"), Level::Fail);
    assert!(r.find("backup").unwrap().detail.contains("27 h"));
    assert_eq!(level(&r, "restore_drill"), Level::Fail);
    assert_eq!(level(&r, "storage_attestation"), Level::Fail);
    assert!(
        r.find("storage_attestation")
            .unwrap()
            .detail
            .contains("EXPIRED")
    );
    // An attestation about to expire warns.
    c.attest(&attest(340 * 86_400)).unwrap();
    assert_eq!(
        level(&run(&*b, Some(&s), None, true), "storage_attestation"),
        Level::Warn
    );
    // A plain newest backup fails the encryption check.
    c.record_backup(&backup(60, 1, true)).unwrap();
    assert_eq!(
        level(&run(&*b, Some(&s), None, true), "backup_encrypted"),
        Level::Fail
    );
    // On a development database nothing fails: it warns.
    let dev = check(&Inputs {
        backend: &*obop_storage::open("memory:").unwrap(),
        production: false,
        server: None,
        sslmode: None,
        host_is_local: true,
        plane: "main",
        now: NOW,
    })
    .unwrap();
    assert!(!dev.failed(), "{}", dev.render());
    assert_eq!(dev.worst(), Level::Warn);
    assert!(dev.find("server").unwrap().detail.contains("development"));
}

#[test]
fn a_server_that_cannot_be_asked_is_a_failure_in_production_and_the_last_run_failure_is_shown() {
    let b = healthy();
    let r = run(&*b, None, None, true);
    assert_eq!(level(&r, "server"), Level::Fail);
    b.kv_put(schedule::FAILURE_KEY, "1: s3 is unreachable", None)
        .unwrap();
    let r = run(&*b, None, None, true);
    assert_eq!(level(&r, "backup_failure"), Level::Warn);
    assert!(
        r.find("backup_failure")
            .unwrap()
            .detail
            .contains("unreachable")
    );
    assert_eq!(human(59), "59 s");
    assert_eq!(human(3600), "60 min");
    assert_eq!(human(26 * 3600), "26 h");
    assert_eq!(human(40 * 86_400), "40 days");
}
