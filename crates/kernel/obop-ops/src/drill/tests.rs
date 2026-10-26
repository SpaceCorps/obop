use super::*;
use crate::backup::Created;
use crate::catalog::Catalog;
use obop_storage::blob::NewBlob;
use serde_json::json;

const KEY: &str = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
const OTHER: &str = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
const NOW: u64 = 1_800_000_000;

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Dir {
        static N: AtomicU64 = AtomicU64::new(0);
        let d = std::env::temp_dir().join(format!(
            "obop-drill-{tag}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Dir(d)
    }
    fn p(&self, n: &str) -> PathBuf {
        self.0.join(n)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn ring() -> KeyRing {
    KeyRing::from_secrets(KEY, &[]).unwrap()
}

fn keys() -> Keys {
    Keys::from_ring(ring())
}

fn f(v: serde_json::Value) -> obop_storage::Fields {
    v.as_object().unwrap().clone()
}

/// A database with accounts, a sealed secret, a record with awkward values and an uploaded file.
fn source() -> (Arc<dyn Backend>, obop_storage::blob::Catalog) {
    let b = obop_storage::open("memory:").unwrap();
    // A database is stamped when it is made, before anything is written to it.
    Migrator::new(b.clone(), ring()).stamp_if_fresh().unwrap();
    let r = ring();
    b.insert(
        "auth_user",
        "ada",
        &f(json!({"password_hash": "x", "groups": ["base.user"]})),
    )
    .unwrap();
    b.insert(
        "connection",
        "c1",
        &f(json!({"credentials": r.seal("connection-credentials", b"{\"k\":1}").unwrap()})),
    )
    .unwrap();
    b.insert(
        "party",
        "p1",
        &f(json!({"name": "Zoë \"Ünï\"", "big": 9_007_199_254_740_993_i64, "n": null})),
    )
    .unwrap();
    let blobs = obop_storage::blob::Catalog::memory(b.clone());
    blobs
        .put(
            &(0..=255u8).cycle().take(3000).collect::<Vec<u8>>(),
            NewBlob {
                content_type: "image/png".into(),
                uploader: "ada".into(),
                ..NewBlob::default()
            },
        )
        .unwrap();
    (b, blobs)
}

fn make(dir: &Dir, k: &Keys, schema: u64) -> (Created, Arc<dyn Backend>) {
    make_in(dir, "b", k, schema)
}

fn make_in(dir: &Dir, sub: &str, k: &Keys, schema: u64) -> (Created, Arc<dyn Backend>) {
    let (b, blobs) = source();
    let latest = obop_migrate::steps().len() as u64;
    let c = backup::create_sealed(
        &b,
        Some(&blobs),
        schema.min(latest),
        &dir.p(sub),
        k,
        &mut |_| {},
    )
    .unwrap();
    (c, b)
}

fn options(c: &Created, k: Keys) -> Options {
    Options {
        backup: c.data_path.clone(),
        keys: k,
        ring: ring(),
        scratch: Scratch::TempSqlite,
        avoid: None,
        plane: "main".into(),
        now: NOW,
        migrate: false,
    }
}

fn check<'a>(r: &'a Report, name: &str) -> &'a DrillCheck {
    r.record
        .checks
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("no check {name} in {}", r.render()))
}

#[test]
fn a_drill_of_a_good_sealed_backup_passes_every_check_and_measures_the_recovery() {
    let dir = Dir::new("good");
    let latest = obop_migrate::steps().len() as u64;
    let (c, _src) = make(&dir, &keys(), latest);
    let created = backup::parse_rfc3339(&c.manifest.created_at).unwrap();
    let mut o = options(&c, keys());
    o.now = created + 3600;
    let r = run(&o).unwrap();
    assert!(r.passed(), "{}", r.render());
    for n in [
        "backup_verifies",
        "restore",
        "record_counts",
        "content_digest",
        "schema_version",
        "migration_ledger",
        "accounts",
        "secrets_open",
        "files",
        "smoke",
    ] {
        assert!(check(&r, n).ok, "{n}: {}", r.render());
    }
    assert!(
        check(&r, "backup_verifies")
            .detail
            .contains("authenticated")
    );
    assert!(check(&r, "accounts").detail.starts_with("1 of 1"));
    assert!(check(&r, "files").detail.starts_with("1 of 1"));
    assert_eq!(
        r.record.rpo_seconds, 3600,
        "the age of the backup at the drill"
    );
    assert!(r.record.rto_seconds < 60);
    assert_eq!(r.record.records, c.manifest.records as u64);
    assert!(r.render().contains("DRILL PASSED") && r.render().contains("60 min"));
    assert!(r.record.id.starts_with("drill-"));
    // The record goes where the checks of `obop storage check` read it.
    let prod = obop_storage::open("memory:").unwrap();
    Catalog::new(&*prod).record_drill(&r.record).unwrap();
    assert_eq!(
        crate::schedule::stats(&*prod, NOW + 100)
            .unwrap()
            .drill_age_seconds,
        Some(100 + NOW - o.now)
    );
}

#[test]
fn the_content_digest_is_the_backups_whatever_the_database_it_is_restored_to() {
    let dir = Dir::new("digest");
    let (c, src) = make(&dir, &keys(), 2);
    assert_eq!(
        Some(content_digest(&*src).unwrap()),
        c.manifest.records_sha256
    );
    let db = Scratch::TempSqlite.open(None).unwrap();
    backup::restore_sealed(
        &db.backend,
        None,
        2,
        &c.data_path,
        RestoreOptions::default(),
        &keys(),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(
        Some(content_digest(&*db.backend).unwrap()),
        c.manifest.records_sha256
    );
    assert_eq!(counts(&*db.backend).unwrap(), c.manifest.models);
}

#[test]
fn each_injected_failure_makes_the_drill_fail_loudly() {
    let dir = Dir::new("inject");
    let latest = obop_migrate::steps().len() as u64;
    // 1. The wrong key.
    let (c, _) = make(&dir, &keys(), latest);
    let wrong = Keys::from_ring(KeyRing::from_secrets(OTHER, &[]).unwrap());
    let r = run(&options(&c, wrong)).unwrap();
    assert!(!r.passed());
    assert!(
        check(&r, "backup_verifies")
            .detail
            .starts_with("restore_key_required")
    );
    assert!(r.render().contains("DRILL FAILED"));
    assert_eq!(
        r.record.checks.len(),
        1,
        "it stops at the first thing that is wrong"
    );
    // 2. A corrupt archive: one flipped bit in the data file.
    let mut bytes = std::fs::read(&c.data_path).unwrap();
    let mid = bytes.len() / 2;
    bytes[mid] ^= 4;
    std::fs::write(&c.data_path, &bytes).unwrap();
    let r = run(&options(&c, keys())).unwrap();
    assert!(
        !r.passed() && !check(&r, "backup_verifies").ok,
        "{}",
        r.render()
    );
    // 3. A truncated archive.
    let (c2, _) = {
        let (b, blobs) = source();
        let c = backup::create_sealed(&b, Some(&blobs), latest, &dir.p("b2"), &keys(), &mut |_| {})
            .unwrap();
        (c, b)
    };
    let bytes = std::fs::read(&c2.data_path).unwrap();
    std::fs::write(&c2.data_path, &bytes[..bytes.len() - 20]).unwrap();
    let r = run(&options(&c2, keys())).unwrap();
    assert!(!r.passed() && !check(&r, "backup_verifies").ok);
    // 4. A manifest that is missing: the drill cannot even start, which is also loud.
    let (c3, _) = make_in(&dir, "b3", &keys(), latest);
    std::fs::remove_file(&c3.manifest_path).unwrap();
    assert!(run(&options(&c3, keys())).unwrap_err().contains("manifest"));
    // 5. Sealed secrets that the server key given cannot open: the copy restores but is not whole.
    let (c4, _) = make_in(&dir, "b4", &keys(), latest);
    let mut o = options(&c4, keys());
    o.ring = KeyRing::from_secrets(OTHER, &[]).unwrap();
    let r = run(&o).unwrap();
    assert!(!r.passed());
    assert!(check(&r, "backup_verifies").ok && check(&r, "restore").ok);
    assert!(!check(&r, "secrets_open").ok, "{}", r.render());
    assert!(
        check(&r, "secrets_open").detail.contains("connection"),
        "{}",
        r.render()
    );
}

#[test]
fn a_backup_from_an_older_schema_needs_the_migrate_option_and_then_passes() {
    let dir = Dir::new("old");
    // Version 1 backup of a database whose schema record says 1.
    let (b, _) = source();
    b.apply(&[obop_storage::Op::Put {
        model: obop_migrate::META_MODEL.into(),
        id: obop_migrate::META_ID.into(),
        fields: f(json!({"version": 1, "at": 1})),
    }])
    .unwrap();
    let c = backup::create_sealed(&b, None, 1, &dir.p("old"), &keys(), &mut |_| {}).unwrap();
    let r = run(&options(&c, keys())).unwrap();
    assert!(!r.passed());
    assert!(
        check(&r, "schema_version").detail.contains("--migrate"),
        "{}",
        r.render()
    );
    let mut o = options(&c, keys());
    o.migrate = true;
    let r = run(&o).unwrap();
    assert!(check(&r, "schema_version").ok, "{}", r.render());
    assert!(
        check(&r, "schema_version")
            .detail
            .contains("migrated the copy")
    );
}

#[test]
fn the_scratch_database_is_never_production_and_never_a_database_with_data() {
    let dir = Dir::new("scratch");
    let (c, _) = make(&dir, &keys(), 2);
    let p = dir.p("prod.db");
    let url = format!("sqlite://{}", p.display());
    let mut o = options(&c, keys());
    o.scratch = Scratch::Url {
        url: url.clone(),
        keep: false,
    };
    o.avoid = Some(url.clone());
    assert!(run(&o).unwrap_err().contains("production database"));
    // The comparison ignores user, password and parameters.
    assert!(same_database(
        "postgres://u:p@h:5432/db?sslmode=require",
        "postgres://other@H:5432/db"
    ));
    assert!(!same_database("postgres://h/db1", "postgres://h/db2"));
    // A database with records is refused.
    let busy = obop_storage::open(&url).unwrap();
    busy.insert("x", "1", &f(json!({}))).unwrap();
    drop(busy);
    o.avoid = None;
    assert!(run(&o).unwrap_err().contains("not empty"));
    for bad in ["memory:", "mysql://x"] {
        o.scratch = Scratch::Url {
            url: bad.into(),
            keep: false,
        };
        assert!(
            run(&o).unwrap_err().contains("postgres:// or sqlite://"),
            "{bad}"
        );
    }
}

#[test]
fn a_lent_scratch_database_is_emptied_again_and_a_temporary_one_leaves_no_file() {
    let dir = Dir::new("clear");
    let (c, _) = make(&dir, &keys(), 2);
    let url = format!("sqlite://{}", dir.p("lent.db").display());
    let mut o = options(&c, keys());
    o.scratch = Scratch::Url {
        url: url.clone(),
        keep: false,
    };
    let r = run(&o).unwrap();
    assert!(r.passed(), "{}", r.render());
    assert_eq!(
        obop_storage::open(&url).unwrap().total().unwrap(),
        0,
        "emptied again"
    );
    // With keep, the copy stays for the operator to look at.
    let url2 = format!("sqlite://{}", dir.p("kept.db").display());
    o.scratch = Scratch::Url {
        url: url2.clone(),
        keep: true,
    };
    assert!(run(&o).unwrap().passed());
    assert!(obop_storage::open(&url2).unwrap().total().unwrap() > 0);
    // The temporary database is gone.
    let db = Scratch::TempSqlite.open(None).unwrap();
    let path = db.url.trim_start_matches("sqlite://").to_string();
    db.backend.insert("x", "1", &f(json!({}))).unwrap();
    assert!(Path::new(&path).exists());
    drop(db);
    assert!(!Path::new(&path).exists());
}

#[test]
fn a_plain_backup_drills_too_and_says_it_is_not_authenticated() {
    let dir = Dir::new("plain");
    let (c, _) = make(&dir, &Keys::none(), 2);
    let mut o = options(&c, Keys::none());
    o.ring = ring();
    let r = run(&o).unwrap();
    assert!(r.passed(), "{}", r.render());
    assert!(
        check(&r, "backup_verifies")
            .detail
            .contains("not authenticated")
    );
}

#[test]
fn a_postgres_backup_drills_into_sqlite_and_the_digest_still_matches() {
    let Ok(base) = std::env::var("OBOP_TEST_POSTGRES_URL") else {
        eprintln!("skipped: set OBOP_TEST_POSTGRES_URL to run against PostgreSQL");
        return;
    };
    let dir = Dir::new("pg");
    let name = format!("obop_drill_t_{}", std::process::id());
    let mut admin = postgres::Client::connect(&base, postgres::NoTls).unwrap();
    admin
        .batch_execute(&format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)"))
        .unwrap();
    admin
        .batch_execute(&format!("CREATE DATABASE {name}"))
        .unwrap();
    let (head, _) = base.rsplit_once('/').unwrap();
    let url = format!("{head}/{name}");
    let result = std::panic::catch_unwind(|| {
        let pg = obop_storage::open(&url).unwrap();
        Migrator::new(pg.clone(), ring()).stamp_if_fresh().unwrap();
        let r = ring();
        pg.insert(
            "auth_user",
            "ada",
            &f(json!({"groups": ["a"], "nested": {"z": 1, "a": [1, 2, {"b": null}]}})),
        )
        .unwrap();
        pg.insert(
            "connection",
            "c1",
            &f(json!({"credentials": r.seal("connection-credentials", b"{}").unwrap()})),
        )
        .unwrap();
        let latest = obop_migrate::steps().len() as u64;
        let c =
            backup::create_sealed(&pg, None, latest, &dir.p("pg"), &keys(), &mut |_| {}).unwrap();
        assert_eq!(c.manifest.source, "postgres");
        // Into SQLite.
        let rep = run(&options(&c, keys())).unwrap();
        assert!(rep.passed(), "{}", rep.render());
        // And into a second PostgreSQL database.
        let name2 = format!("{name}_b");
        let mut a = postgres::Client::connect(&base, postgres::NoTls).unwrap();
        a.batch_execute(&format!("DROP DATABASE IF EXISTS {name2} WITH (FORCE)"))
            .unwrap();
        a.batch_execute(&format!("CREATE DATABASE {name2}"))
            .unwrap();
        let mut o = options(&c, keys());
        o.scratch = Scratch::Url {
            url: format!("{head}/{name2}"),
            keep: false,
        };
        o.avoid = Some(url.clone());
        let rep = run(&o).unwrap();
        let ok = rep.passed();
        drop(rep);
        a.batch_execute(&format!("DROP DATABASE IF EXISTS {name2} WITH (FORCE)"))
            .unwrap();
        assert!(ok);
    });
    admin
        .batch_execute(&format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)"))
        .unwrap();
    result.unwrap();
}

// ---- review of D73 -------------------------------------------------------------------------

#[test]
fn a_lent_sqlite_scratch_database_loses_the_uploaded_files_of_the_drill_too() {
    let dir = Dir::new("lentfiles");
    let (c, _) = make(&dir, &keys(), 2);
    assert!(c.manifest.blobs >= 1, "the backup holds an upload");
    let lent = dir.p("lent.db");
    let url = format!("sqlite://{}", lent.display());
    let mut o = options(&c, keys());
    o.scratch = Scratch::Url {
        url: url.clone(),
        keep: false,
    };
    let r = run(&o).unwrap();
    assert!(r.passed(), "{}", r.render());
    assert!(check(&r, "files").ok);
    drop(r);
    assert_eq!(obop_storage::open(&url).unwrap().total().unwrap(), 0);
    let files = obop_storage::blob::open_bytes(&url, None)
        .unwrap()
        .keys()
        .unwrap();
    assert!(files.is_empty(), "uploaded files left behind: {files:?}");
    // With keep, the files stay beside the copy.
    let kept = format!("sqlite://{}", dir.p("kept.db").display());
    o.scratch = Scratch::Url {
        url: kept.clone(),
        keep: true,
    };
    assert!(run(&o).unwrap().passed());
    assert!(
        !obop_storage::blob::open_bytes(&kept, None)
            .unwrap()
            .keys()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_lent_postgres_scratch_database_loses_the_uploaded_files_of_the_drill_too() {
    let Ok(base) = std::env::var("OBOP_TEST_POSTGRES_URL") else {
        eprintln!("skipped: set OBOP_TEST_POSTGRES_URL to run against PostgreSQL");
        return;
    };
    let name = format!("obop_drill_files_{}", std::process::id());
    let mut admin = postgres::Client::connect(&base, postgres::NoTls).unwrap();
    admin
        .batch_execute(&format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)"))
        .unwrap();
    admin
        .batch_execute(&format!("CREATE DATABASE {name}"))
        .unwrap();
    let (head, _) = base.rsplit_once('/').unwrap();
    let url = format!("{head}/{name}");
    let dir = Dir::new("pgfiles");
    let result = std::panic::catch_unwind(|| {
        let (c, _) = make(&dir, &keys(), 2);
        let mut o = options(&c, keys());
        o.scratch = Scratch::Url {
            url: url.clone(),
            keep: false,
        };
        let r = run(&o).unwrap();
        assert!(r.passed(), "{}", r.render());
        drop(r);
        let after = obop_storage::open(&url).unwrap();
        assert_eq!(after.total().unwrap(), 0);
        let files = obop_storage::blob::open_bytes(&url, None)
            .unwrap()
            .keys()
            .unwrap();
        assert!(files.is_empty(), "uploaded files left behind: {files:?}");
    });
    admin
        .batch_execute(&format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)"))
        .unwrap();
    result.unwrap();
}

#[test]
fn the_temporary_scratch_database_lives_in_a_private_directory_that_goes_with_it() {
    let db = Scratch::TempSqlite.open(None).unwrap();
    let file = PathBuf::from(db.url.trim_start_matches("sqlite://"));
    let dir = file.parent().unwrap().to_path_buf();
    assert!(
        dir.file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("obop-scratch-"),
        "{}",
        dir.display()
    );
    // The name carries random bytes, so it cannot be guessed in advance.
    let other = Scratch::TempSqlite.open(None).unwrap();
    assert_ne!(db.url, other.url);
    db.backend.insert("x", "1", &f(json!({}))).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
    drop(db);
    assert!(!dir.exists(), "the whole directory is removed");
    drop(other);
}

#[test]
fn a_drill_without_the_key_carries_the_stable_code_for_the_exit_status() {
    let dir = Dir::new("code");
    let (c, _) = make(&dir, &keys(), 2);
    let r = run(&options(&c, Keys::none())).unwrap();
    assert!(!r.passed());
    assert_eq!(r.error_code(), Some(backup::KEY_REQUIRED));
    // A wrong key is the same code.
    let wrong = Keys::from_ring(KeyRing::from_secrets(OTHER, &[]).unwrap());
    assert_eq!(
        run(&options(&c, wrong)).unwrap().error_code(),
        Some(backup::KEY_REQUIRED)
    );
    // A plain backup offered to an instance that seals its own has another code.
    let (plain, _) = make_in(&dir, "plain", &Keys::none(), 2);
    assert_eq!(
        run(&options(&plain, keys())).unwrap().error_code(),
        Some(backup::NOT_SEALED)
    );
    // A drill that passes, or fails for another reason, has none.
    assert_eq!(run(&options(&c, keys())).unwrap().error_code(), None);
    let mut broken = options(&c, keys());
    broken.migrate = false;
    std::fs::write(&c.data_path, b"damaged").unwrap();
    let r = run(&broken).unwrap();
    assert!(!r.passed() && r.error_code().is_none());
}
