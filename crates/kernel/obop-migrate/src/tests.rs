use super::*;
use obop_keys::legacy;

const KEY: &str = "0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a";

pub(crate) fn ring() -> KeyRing {
    KeyRing::from_secrets(KEY, &[]).unwrap()
}

pub(crate) fn mem() -> Arc<dyn Backend> {
    obop_storage::open("memory:").unwrap()
}

pub(crate) fn put_log(id: &str) -> Op {
    let mut f = Fields::new();
    f.insert("n".into(), J::from(1));
    Op::Put {
        model: "log".into(),
        id: id.into(),
        fields: f,
    }
}

/// Three steps that each leave a mark in the `log` model.
pub(crate) fn marking() -> Vec<Step> {
    vec![
        Step::new(1, "one", Run::Ops(|_| Ok(vec![put_log("one")]))).source("fn one() {}"),
        Step::new(
            2,
            "two",
            Run::Ops(|_| Ok(vec![put_log("two-a"), put_log("two-b")])),
        )
        .class(Class::Backfill)
        .source("fn two() {}"),
        Step::new(
            3,
            "three",
            Run::Direct(|c| {
                if c.dry_run {
                    return Ok("would mark".into());
                }
                c.backend.counter_add("ran:three", 0, false)?;
                c.backend.apply(&[put_log("three")])?;
                Ok("done".into())
            }),
        )
        .source("fn three() {}"),
    ]
}

pub(crate) fn version(b: &Arc<dyn Backend>) -> Option<u64> {
    b.get(META_MODEL, META_ID)
        .unwrap()
        .and_then(|f| f["version"].as_u64())
}

#[test]
fn the_real_steps_are_numbered_without_gaps() {
    let m = Migrator::new(mem(), ring());
    assert_eq!(m.latest() as usize, steps().len());
    assert!(m.latest() >= 2);
}

#[test]
fn a_fresh_database_is_stamped_and_an_old_one_is_not() {
    let b = mem();
    let m = Migrator::new(b.clone(), ring());
    assert!(m.status().unwrap().empty);
    assert!(m.stamp_if_fresh().unwrap());
    assert_eq!(version(&b), Some(u64::from(m.latest())));
    assert!(!m.stamp_if_fresh().unwrap(), "only once");
    assert!(m.status().unwrap().pending.is_empty());
    // A database that already holds records but no version record is older than versioning.
    let old = mem();
    old.insert("auth_user", "ada", &Fields::new()).unwrap();
    let m = Migrator::new(old.clone(), ring());
    assert!(!m.stamp_if_fresh().unwrap());
    let st = m.status().unwrap();
    assert_eq!(
        (st.stored, st.pending.len(), st.empty),
        (None, m.latest() as usize, false)
    );
    assert!(st.describe().contains("pending"));
}

#[test]
fn steps_run_in_order_once_and_the_second_run_does_nothing() {
    let b = mem();
    let m = Migrator::with_steps(b.clone(), ring(), marking());
    let r = m.run(false).unwrap();
    assert_eq!(
        r.applied.iter().map(|(v, _, _)| *v).collect::<Vec<_>>(),
        [1, 2, 3]
    );
    assert_eq!(version(&b), Some(3));
    for id in ["one", "two-a", "two-b", "three"] {
        assert!(b.get("log", id).unwrap().is_some(), "{id}");
    }
    // The history says what ran, in order.
    let hist: Vec<_> = b
        .list(META_MODEL)
        .unwrap()
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    for id in ["schema", "step-0001", "step-0002", "step-0003"] {
        assert!(hist.contains(&id.to_string()), "{id} in {hist:?}");
    }
    let again = m.run(false).unwrap();
    assert!(again.applied.is_empty());
    assert_eq!(b.counter_get("ran:three").unwrap(), 1);
}

#[test]
fn a_dry_run_changes_nothing_and_a_run_resumes_from_the_stored_version() {
    let b = mem();
    let m = Migrator::with_steps(b.clone(), ring(), marking());
    let r = m.run(true).unwrap();
    assert!(r.dry_run && r.applied.len() == 3);
    assert!(r.applied[1].2.contains("2 write"));
    assert_eq!(version(&b), None);
    assert_eq!(b.total().unwrap(), 0);
    // A database already at version 2 only runs step 3.
    let partial = Migrator::with_steps(b.clone(), ring(), marking()[..2].to_vec());
    partial.run(false).unwrap();
    let r = m.run(false).unwrap();
    assert_eq!(r.applied.len(), 1);
    assert_eq!(r.applied[0].0, 3);
}

#[test]
fn a_failing_step_rolls_back_its_writes_and_leaves_the_earlier_steps() {
    let b = mem();
    let mut steps = marking();
    steps[1].run = Run::Ops(|_| {
        // Writes something, then asks to merge into a record that does not exist: the batch
        // fails as a whole.
        Ok(vec![
            put_log("half"),
            Op::Merge {
                model: "log".into(),
                id: "nope".into(),
                patch: Fields::new(),
            },
        ])
    });
    let m = Migrator::with_steps(b.clone(), ring(), steps);
    let e = m.run(false).unwrap_err();
    assert!(e.contains("step 2") && e.contains("version 1"), "{e}");
    assert_eq!(version(&b), Some(1), "step 1 stays");
    assert!(b.get("log", "one").unwrap().is_some());
    assert!(
        b.get("log", "half").unwrap().is_none(),
        "nothing of step 2 was kept"
    );
    assert!(b.get(META_MODEL, "step-0002").unwrap().is_none());
    // A step that errors by itself is the same.
    let mut steps = marking();
    steps[2].run = Run::Direct(|_| Err("boom".into()));
    let m = Migrator::with_steps(mem(), ring(), steps);
    assert!(m.run(false).unwrap_err().contains("boom"));
}

#[test]
fn two_runners_started_together_apply_each_step_once() {
    let b = mem();
    let steps = marking();
    std::thread::scope(|s| {
        for _ in 0..4 {
            let (b, steps) = (b.clone(), steps.clone());
            s.spawn(move || {
                Migrator::with_steps(b, ring(), steps).run(false).unwrap();
            });
        }
    });
    assert_eq!(version(&b), Some(3));
    assert_eq!(
        b.counter_get("ran:three").unwrap(),
        1,
        "the direct step ran once"
    );
}

#[test]
fn a_newer_database_is_refused() {
    let b = mem();
    let newer = Migrator::with_steps(b.clone(), ring(), marking());
    newer.run(false).unwrap();
    let older = Migrator::with_steps(b, ring(), marking()[..2].to_vec());
    let st = older.status().unwrap();
    assert!(st.ahead && st.describe().contains("NEWER"));
    let e = older.check_for_serve(true, "x").unwrap_err();
    assert!(e.contains("newer than this build"), "{e}");
    assert!(older.run(false).unwrap_err().contains("newer"));
}

#[test]
fn serving_runs_pending_steps_only_when_asked_and_otherwise_prints_the_command() {
    let b = mem();
    b.insert("auth_user", "ada", &Fields::new()).unwrap();
    let m = Migrator::with_steps(b.clone(), ring(), marking());
    let e = m.check_for_serve(false, "postgres://u@h/db").unwrap_err();
    assert!(e.contains("obop migrate run --db postgres://u@h/db") && e.contains("--auto-migrate"));
    assert_eq!(version(&b), None, "nothing ran");
    let lines = m.check_for_serve(true, "x").unwrap();
    assert_eq!(lines.len(), 3);
    assert!(
        m.check_for_serve(false, "x").unwrap().is_empty(),
        "up to date: serve"
    );
}

#[test]
fn the_seal_format_step_upgrades_first_format_secrets_and_refuses_without_the_key() {
    let b = mem();
    let master = legacy::parse_key(KEY).unwrap();
    let mut u = Fields::new();
    u.insert(
        "totp_secret".into(),
        J::from(legacy::seal_totp_v1(&master, "ada", b"seed")),
    );
    b.insert("auth_user", "ada", &u).unwrap();
    let mut c = Fields::new();
    c.insert(
        "credentials".into(),
        J::from(legacy::seal_box_v1(KEY, "connection-credentials", b"{}")),
    );
    b.insert("connection", "c1", &c).unwrap();
    let m = Migrator::new(b.clone(), ring());
    let r = m.run(false).unwrap();
    assert_eq!(r.applied.len(), m.latest() as usize);
    assert!(
        r.applied[1].2.contains("2 secret(s) re-sealed"),
        "{:?}",
        r.applied
    );
    let rec = b.get("auth_user", "ada").unwrap().unwrap();
    let sealed = rec["totp_secret"].as_str().unwrap();
    assert!(sealed.starts_with("e2."));
    assert_eq!(ring().open("totp:ada", sealed).unwrap(), b"seed");
    // Without the key that sealed them, the step refuses and the version stays at 1.
    let b2 = mem();
    b2.insert("auth_user", "ada", &u).unwrap();
    let other = KeyRing::from_secrets(&"f".repeat(64), &[]).unwrap();
    let e = Migrator::new(b2.clone(), other).run(false).unwrap_err();
    assert!(
        e.contains("OBOP_SECRET_KEY_PREVIOUS") && !e.contains(KEY),
        "{e}"
    );
    assert_eq!(version(&b2), Some(1));
    // No key at all: refuses too.
    let e = Migrator::new(mem_with(&u), KeyRing::default())
        .run(false)
        .unwrap_err();
    assert!(e.contains("OBOP_SECRET_KEY"));
}

fn mem_with(u: &Fields) -> Arc<dyn Backend> {
    let b = mem();
    b.insert("auth_user", "ada", u).unwrap();
    b
}

#[test]
fn it_works_on_a_sqlite_file_shared_by_two_handles() {
    let path = std::env::temp_dir().join(format!("obop-migrate-{}.db", std::process::id()));
    let url = format!("sqlite://{}", path.display());
    let (a, b) = (
        obop_storage::open(&url).unwrap(),
        obop_storage::open(&url).unwrap(),
    );
    a.insert("auth_user", "ada", &Fields::new()).unwrap();
    let steps = marking();
    std::thread::scope(|s| {
        for h in [a.clone(), b.clone()] {
            let steps = steps.clone();
            s.spawn(move || {
                Migrator::with_steps(h, ring(), steps).run(false).unwrap();
            });
        }
    });
    assert_eq!(version(&b), Some(3));
    assert_eq!(a.counter_get("ran:three").unwrap(), 1);
    drop((a, b));
    for ext in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
    }
    let _ = std::fs::remove_file(format!("{}.lock-obop.migrate", path.display()));
}
