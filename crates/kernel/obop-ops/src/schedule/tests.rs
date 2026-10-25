use super::*;
use crate::catalog::Catalog;
use crate::sink::FsSink;
use crate::sink::fake::MemorySink;
use obop_keys::KeyRing;
use serde_json::json;

const KEY: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
/// 2025-01-01T00:00:00Z.
const START: u64 = 1_735_689_600;
const DAY: u64 = 86_400;

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Dir {
        static N: AtomicU64 = AtomicU64::new(0);
        let d = std::env::temp_dir().join(format!(
            "obop-sched-{tag}-{}-{}",
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
    fn files(&self, n: &str) -> usize {
        std::fs::read_dir(self.p(n)).map_or(0, |d| d.count())
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn keys() -> Keys {
    Keys::from_ring(KeyRing::from_secrets(KEY, &[]).unwrap())
}

fn db() -> Arc<dyn Backend> {
    let b = obop_storage::open("memory:").unwrap();
    let f = |v: serde_json::Value| v.as_object().unwrap().clone();
    b.insert("party", "p1", &f(json!({"name": "Ada"}))).unwrap();
    b.insert("party", "p2", &f(json!({"name": "Bo"}))).unwrap();
    b
}

fn settings(d: &Dir) -> Settings {
    Settings::new(d.p("local"), keys())
}

#[test]
fn a_run_backs_up_verifies_records_copies_and_proves_the_copy() {
    let d = Dir::new("run");
    let b = db();
    let clock = FakeClock::at(START);
    let mut s = settings(&d);
    s.sinks.push(Arc::new(FsSink::new(d.p("offsite"))));
    let o = take(&b, None, 2, &s, "manual", &clock).unwrap();
    assert!(o.copy_failures.is_empty() && o.pruned.is_empty());
    let r = &o.backup;
    assert!(
        r.id.starts_with("obop-backup-20250101T000000Z") && r.id.ends_with(".obbk"),
        "{}",
        r.id
    );
    assert!(r.verified && r.authenticated);
    assert_eq!((r.records, r.format, r.created_at), (2, 2, START));
    assert_eq!(r.copies.len(), 1);
    // Data file and manifest are off the host, and the copy's bytes equal the original's.
    let off = FsSink::new(d.p("offsite"));
    assert_eq!(off.list().unwrap().len(), 2);
    assert_eq!(
        off.digest(&r.id).unwrap().0,
        crate::sink::file_digest(Path::new(&r.path)).unwrap().0
    );
    // The catalogue has it.
    let cat = Catalog::new(&*b);
    assert_eq!(cat.latest_verified().unwrap().unwrap().id, r.id);
    // The copy alone restores in another place: the drill from the off-host copy.
    let dst = obop_storage::open("memory:").unwrap();
    let got = d.p("fetched.obbk");
    off.get(&r.id, &got).unwrap();
    off.get(
        &format!("{}.manifest.json", r.id),
        &d.p("fetched.obbk.manifest.json"),
    )
    .unwrap();
    backup::restore_sealed(
        &dst,
        None,
        2,
        &got,
        backup::RestoreOptions::default(),
        &keys(),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(dst.count("party").unwrap(), 2);
}

#[test]
fn a_backup_without_a_key_is_taken_plain_and_says_so() {
    let d = Dir::new("plain");
    let b = db();
    let s = Settings::new(d.p("local"), Keys::none());
    let o = take(&b, None, 2, &s, "manual", &FakeClock::at(START)).unwrap();
    assert!(!o.backup.authenticated && o.backup.format == 1 && o.backup.key_id.is_none());
    assert!(
        o.notes.iter().any(|n| n.contains("not encrypted")),
        "{:?}",
        o.notes
    );
    assert!(o.backup.id.ends_with(".jsonl.gz"));
}

#[test]
fn a_sink_that_fails_leaves_a_verified_local_backup_and_a_named_failure() {
    let d = Dir::new("fail");
    let b = db();
    let sink = Arc::new(MemorySink::default());
    *sink.fail_put.lock().unwrap() = true;
    let mut s = settings(&d);
    s.sinks.push(sink.clone());
    let o = take(&b, None, 2, &s, "scheduled", &FakeClock::at(START)).unwrap();
    assert_eq!(o.copy_failures.len(), 1);
    assert!(o.copy_failures[0].1.contains("unreachable"));
    assert!(o.backup.copies.is_empty() && o.backup.verified);
    // The failure is kept where status commands and metrics can read it, and a successful run
    // clears it.
    let sched = Schedule::new(s, Arc::new(FakeClock::at(START + 2 * DAY)));
    let out = sched.tick(&b, None, 2).unwrap().unwrap();
    assert_eq!(out.copy_failures.len(), 1);
    assert!(
        stats(&*b, START + 2 * DAY)
            .unwrap()
            .last_failure
            .unwrap()
            .contains("unreachable")
    );
    *sink.fail_put.lock().unwrap() = false;
    let sched2 = Schedule::new(
        Settings {
            sinks: vec![sink.clone()],
            ..settings(&d)
        },
        Arc::new(FakeClock::at(START + 4 * DAY)),
    );
    assert!(
        sched2
            .tick(&b, None, 2)
            .unwrap()
            .unwrap()
            .copy_failures
            .is_empty()
    );
    assert!(stats(&*b, START + 4 * DAY).unwrap().last_failure.is_none());
}

#[test]
fn the_timer_takes_a_backup_only_when_one_is_due_and_only_one_process_does() {
    let d = Dir::new("timer");
    let b = db();
    let clock = Arc::new(FakeClock::at(START));
    let sched = Schedule::new(settings(&d), clock.clone());
    assert!(sched.due(&b).unwrap(), "no backup yet");
    assert!(sched.tick(&b, None, 2).unwrap().is_some());
    assert!(!sched.due(&b).unwrap());
    assert!(sched.tick(&b, None, 2).unwrap().is_none());
    clock.advance(23 * 3600 + 59 * 60);
    assert!(sched.tick(&b, None, 2).unwrap().is_none(), "not yet a day");
    clock.advance(60);
    // Another process holds the lease: this one does nothing.
    b.kv_put(LEASE_KEY, "other:1", Some(60_000)).unwrap();
    assert!(sched.tick(&b, None, 2).unwrap().is_none());
    b.kv_delete(LEASE_KEY).unwrap();
    assert!(sched.tick(&b, None, 2).unwrap().is_some());
    assert!(
        b.kv_get(LEASE_KEY).unwrap().is_none(),
        "the lease is let go"
    );
    assert_eq!(Catalog::new(&*b).backups().unwrap().len(), 2);
    // A manual backup does not count as the scheduled one.
    clock.advance(3 * DAY);
    take(&b, None, 2, &settings(&d), "manual", &*clock).unwrap();
    assert!(sched.due(&b).unwrap());
}

#[test]
fn two_timers_started_together_take_one_backup() {
    let d = Dir::new("race");
    let b = db();
    let clock: Arc<dyn Clock> = Arc::new(FakeClock::at(START));
    let results: Vec<bool> = std::thread::scope(|sc| {
        let hs: Vec<_> = (0..4)
            .map(|_| {
                let (b, clock) = (b.clone(), clock.clone());
                let st = settings(&d);
                sc.spawn(move || {
                    Schedule::new(st, clock)
                        .tick(&b, None, 2)
                        .unwrap()
                        .is_some()
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    assert_eq!(results.iter().filter(|r| **r).count(), 1, "{results:?}");
    assert_eq!(Catalog::new(&*b).backups().unwrap().len(), 1);
}

#[test]
fn fourteen_months_of_nightly_backups_leave_35_dailies_and_12_monthlies_on_disk_and_in_the_sink() {
    let d = Dir::new("months");
    let b = db();
    let clock = Arc::new(FakeClock::at(START + 2 * 3600));
    let mut s = settings(&d);
    // Pruning removes copies too when the sink lets it (its lifecycle rule is the usual way).
    s.sinks
        .push(Arc::new(FsSink::new(d.p("offsite")).allow_delete(true)));
    let sched = Schedule::new(s, clock.clone());
    // A backup somebody asked for by hand, long ago: retention never touches it.
    let manual = take(&b, None, 2, &settings(&d), "manual", &*clock).unwrap();
    for _ in 0..426 {
        clock.advance(DAY);
        let o = sched.tick(&b, None, 2).unwrap().unwrap();
        assert!(o.copy_failures.is_empty());
    }
    let cat = Catalog::new(&*b);
    let scheduled: Vec<(String, u64)> = cat
        .backups()
        .unwrap()
        .into_iter()
        .filter(|b| b.reason == "scheduled")
        .map(|b| (b.id, b.created_at))
        .collect();
    let d2 = retention::decide(Retention::default(), &scheduled);
    assert!(d2.delete.is_empty(), "the catalogue is already pruned");
    assert_eq!(d2.kept_by(retention::Why::Daily), 35);
    assert_eq!(d2.kept_by(retention::Why::Monthly), 12);
    assert!((44..=47).contains(&scheduled.len()), "{}", scheduled.len());
    // Files on disk and in the sink are exactly the kept ones, data and manifest each, plus
    // the manual one.
    assert_eq!(d.files("local"), 2 * scheduled.len() + 2);
    assert_eq!(d.files("offsite"), 2 * scheduled.len());
    assert!(
        Path::new(&manual.backup.path).exists(),
        "manual backups stay"
    );
    // The newest backup is the last night's.
    assert_eq!(
        cat.latest_verified().unwrap().unwrap().created_at,
        START + 2 * 3600 + 426 * DAY
    );
}

#[test]
fn a_backup_past_retention_without_a_verified_copy_is_kept() {
    let d = Dir::new("keep");
    let b = db();
    let clock = Arc::new(FakeClock::at(START));
    let sink = Arc::new(MemorySink::default());
    let mut s = settings(&d);
    s.retention = Retention {
        daily: 2,
        weekly: 0,
        monthly: 0,
    };
    s.sinks.push(sink.clone());
    let sched = Schedule::new(s, clock.clone());
    *sink.fail_put.lock().unwrap() = true;
    for _ in 0..3 {
        sched.tick(&b, None, 2).unwrap();
        clock.advance(DAY);
    }
    // Three backups, none copied: nothing is deleted though only two are within retention.
    assert_eq!(Catalog::new(&*b).backups().unwrap().len(), 3);
    *sink.fail_put.lock().unwrap() = false;
    let o = sched.tick(&b, None, 2).unwrap().unwrap();
    // The fourth is copied; the ones past retention still have no copy, so they stay.
    assert!(o.pruned.is_empty(), "{:?}", o.pruned);
    assert!(
        o.notes.iter().any(|n| n.contains("no verified copy")),
        "{:?}",
        o.notes
    );
    assert_eq!(Catalog::new(&*b).backups().unwrap().len(), 4);
}

#[test]
fn stats_say_how_old_the_last_good_backup_is_and_when_it_is_overdue() {
    let d = Dir::new("stats");
    let b = db();
    let none = stats(&*b, START).unwrap();
    assert!(none.backup_age_seconds.is_none() && none.backup_overdue());
    let clock = Arc::new(FakeClock::at(START));
    let mut s = settings(&d);
    s.sinks.push(Arc::new(FsSink::new(d.p("off"))));
    let sched = Schedule::new(s, clock.clone());
    sched.tick(&b, None, 2).unwrap().unwrap();
    let st = stats(&*b, START + 60).unwrap();
    assert_eq!(st.backup_age_seconds, Some(60));
    assert_eq!(st.copy_age_seconds, Some(60));
    assert!(st.backup_bytes.unwrap() > 0 && st.drill_age_seconds.is_none());
    assert!(!st.backup_overdue());
    // Stop the scheduler for 27 hours: the age passes 26 hours at 26 h 00 min 01 s, and the
    // metric `obop_backup_age_seconds` exceeds the alert threshold from then on.
    let at_26h = stats(&*b, START + OVERDUE_AFTER_SECONDS).unwrap();
    assert!(!at_26h.backup_overdue(), "exactly 26 hours is not yet late");
    assert!(
        stats(&*b, START + OVERDUE_AFTER_SECONDS + 1)
            .unwrap()
            .backup_overdue()
    );
    assert!(stats(&*b, START + 27 * 3600).unwrap().backup_overdue());
}
