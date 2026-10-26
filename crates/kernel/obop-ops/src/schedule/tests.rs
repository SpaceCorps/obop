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
        // The timer waits after a failure; a day later it tries again.
        clock.advance(DAY);
    }
    // Three backups, none copied: nothing is deleted though only two are within retention.
    assert_eq!(Catalog::new(&*b).backups().unwrap().len(), 3);
    // While the sink stays down the one past retention stays too, with the reason said.
    let o = sched.tick(&b, None, 2).unwrap().unwrap();
    assert!(o.pruned.is_empty(), "{:?}", o.pruned);
    assert!(
        o.notes.iter().any(|n| n.contains("no verified copy")),
        "{:?}",
        o.notes
    );
    assert_eq!(Catalog::new(&*b).backups().unwrap().len(), 4);
    // The sink comes back. The next run copies what it missed (the old backups are not stranded
    // for good), and only then do the two past retention go.
    *sink.fail_put.lock().unwrap() = false;
    clock.advance(DAY);
    let o = sched.tick(&b, None, 2).unwrap().unwrap();
    assert!(o.copy_failures.is_empty());
    assert_eq!(o.pruned.len(), 3, "{:?} {:?}", o.pruned, o.notes);
    let left = Catalog::new(&*b).backups().unwrap();
    assert_eq!(left.len(), 2);
    assert!(left.iter().all(|b| !b.copies.is_empty()));
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

// ---- review of D73 -------------------------------------------------------------------------

fn flaky_db() -> (Arc<crate::testkit::Flaky>, Arc<dyn Backend>) {
    let f = crate::testkit::Flaky::new(db());
    let b: Arc<dyn Backend> = f.clone();
    (f, b)
}

fn ids(b: &Arc<dyn Backend>) -> Vec<String> {
    Catalog::new(&**b)
        .backups()
        .unwrap()
        .into_iter()
        .map(|r| r.id)
        .collect()
}

#[test]
fn a_file_that_cannot_be_removed_stays_in_the_catalogue_and_is_not_reported_removed() {
    let d = Dir::new("stuck");
    let b = db();
    let clock = FakeClock::at(START);
    let mut s = settings(&d);
    s.retention = Retention {
        daily: 1,
        weekly: 0,
        monthly: 0,
    };
    let first = take(&b, None, 2, &s, "scheduled", &clock).unwrap().backup;
    // The file cannot be removed: something that is not a file stands in its place.
    std::fs::remove_file(&first.path).unwrap();
    std::fs::create_dir(&first.path).unwrap();
    std::fs::write(Path::new(&first.path).join("x"), b"x").unwrap();
    clock.advance(DAY);
    let o = take(&b, None, 2, &s, "scheduled", &clock).unwrap();
    assert!(o.pruned.is_empty(), "{:?}", o.pruned);
    assert!(
        o.notes.iter().any(|n| n.contains("cannot remove")),
        "{:?}",
        o.notes
    );
    assert!(
        ids(&b).contains(&first.id),
        "the record of a file that is still there is kept"
    );
    // Once the obstacle is gone, the next run removes it and says so.
    std::fs::remove_dir_all(&first.path).unwrap();
    std::fs::write(&first.path, b"data").unwrap();
    clock.advance(DAY);
    let o = take(&b, None, 2, &s, "scheduled", &clock).unwrap();
    assert!(o.pruned.contains(&first.id), "{:?} {:?}", o.pruned, o.notes);
    assert!(!ids(&b).contains(&first.id));
    assert!(!Path::new(&first.path).exists());
}

#[test]
fn a_recorded_path_is_absolute_and_another_hosts_files_are_left_to_it() {
    let d = Dir::new("paths");
    let b = db();
    let clock = FakeClock::at(START);
    // A relative backup directory is recorded as the absolute path it meant.
    let rel = PathBuf::from(format!("obop-sched-rel-{}", std::process::id()));
    let mut s = Settings::new(rel.join("bk"), keys());
    s.retention = Retention {
        daily: 1,
        weekly: 0,
        monthly: 0,
    };
    let r = take(&b, None, 2, &s, "scheduled", &clock).unwrap().backup;
    assert!(Path::new(&r.path).is_absolute(), "{}", r.path);
    assert!(Path::new(&r.path).exists());
    let _ = std::fs::remove_dir_all(&rel);
    // A file under no directory this process was given is not removed by it.
    let mut other = settings(&d);
    other.retention = s.retention;
    other.host = Some("host-b".into());
    let mut theirs = Catalog::new(&*b).backups().unwrap().remove(0);
    theirs.id = "obop-backup-20200101T000000Z.obbk".into();
    theirs.created_at = START - 100 * DAY;
    theirs.host = Some("host-a".into());
    theirs.path = d.p("somewhere/theirs.obbk").display().to_string();
    Catalog::new(&*b).record_backup(&theirs).unwrap();
    clock.advance(DAY);
    let o = take(&b, None, 2, &other, "scheduled", &clock).unwrap();
    assert!(!o.pruned.contains(&theirs.id), "{:?}", o.pruned);
    assert!(ids(&b).contains(&theirs.id));
    assert!(
        o.notes.iter().any(|n| n.contains("left to the host")),
        "{:?}",
        o.notes
    );
}

#[test]
fn prune_without_the_sink_configured_removes_nothing_the_sink_does_not_hold() {
    let d = Dir::new("nosink");
    let b = db();
    let clock = FakeClock::at(START);
    let mut s = settings(&d);
    // Backup A was taken before any sink was configured; backup B was copied by a run that had
    // one (recorded by hand here: the sink is a share this shell does not have).
    let a = take(&b, None, 2, &s, "scheduled", &clock).unwrap().backup;
    clock.advance(DAY);
    let newest = take(&b, None, 2, &s, "scheduled", &clock).unwrap().backup;
    s.retention = Retention {
        daily: 1,
        weekly: 0,
        monthly: 0,
    };
    let mut rec = newest.clone();
    rec.copies.push(Copy {
        sink: "fs:/mnt/offsite".into(),
        name: rec.id.clone(),
        verified_at: START,
        sha256: None,
    });
    Catalog::new(&*b).record_backup(&rec).unwrap();
    // A shell with no sink configured: the dry plan and the real run both remove nothing, and
    // say why.
    let plan = prune_plan(&b, &s, &clock).unwrap();
    assert!(plan.remove.is_empty());
    assert!(
        plan.notes.iter().any(|n| n.contains("no sink")),
        "{:?}",
        plan.notes
    );
    let (pruned, _) = prune(&b, &s, &clock).unwrap();
    assert!(pruned.is_empty());
    assert!(Path::new(&a.path).exists());
    // With the operator's explicit word it goes ahead, and the dry plan says the same.
    s.ignore_recorded_sinks = true;
    let plan = prune_plan(&b, &s, &clock).unwrap();
    assert_eq!(
        plan.remove.iter().map(|r| r.id.clone()).collect::<Vec<_>>(),
        [a.id.clone()]
    );
    let (pruned, _) = prune(&b, &s, &clock).unwrap();
    assert_eq!(pruned, [a.id]);
}

#[test]
fn a_copy_that_is_no_longer_in_the_sink_does_not_license_deleting_the_local_file() {
    let d = Dir::new("stale");
    let b = db();
    let clock = FakeClock::at(START);
    let mut s = settings(&d);
    s.retention = Retention {
        daily: 1,
        weekly: 0,
        monthly: 0,
    };
    s.sinks.push(Arc::new(FsSink::new(d.p("off")).allow_delete(true)));
    let first = take(&b, None, 2, &s, "scheduled", &clock).unwrap().backup;
    assert_eq!(first.copies.len(), 1);
    // A lifecycle rule (or a person) empties the sink.
    for f in std::fs::read_dir(d.p("off")).unwrap().flatten() {
        std::fs::remove_file(f.path()).unwrap();
    }
    clock.advance(DAY);
    // The next run first copies what the sink lacks again?  No: the record says it has it. The
    // proof at the moment of deleting is what stops the deletion.
    let plan = prune_plan(&b, &s, &clock).unwrap();
    assert!(plan.remove.is_empty() || plan.remove.iter().all(|r| r.id != first.id));
    let o = take(&b, None, 2, &s, "scheduled", &clock).unwrap();
    assert!(!o.pruned.contains(&first.id), "{:?}", o.pruned);
    assert!(
        o.notes.iter().any(|n| n.contains("not there or has changed")),
        "{:?}",
        o.notes
    );
    assert!(Path::new(&first.path).exists(), "the local file stays");
    // The stale record was corrected, so the next run copies it again and then it may go.
    clock.advance(DAY);
    let o = take(&b, None, 2, &s, "scheduled", &clock).unwrap();
    assert!(
        o.pruned.contains(&first.id),
        "{:?} {:?}",
        o.pruned,
        o.notes
    );
    assert_eq!(
        std::fs::read_dir(d.p("off")).unwrap().count() % 2,
        0,
        "the sink holds data and manifest pairs"
    );
}

#[test]
fn a_sink_in_the_backup_directory_is_not_an_off_host_copy_and_deletes_nothing() {
    let d = Dir::new("alias");
    let b = db();
    let clock = FakeClock::at(START);
    let mut s = settings(&d);
    s.retention = Retention {
        daily: 1,
        weekly: 0,
        monthly: 0,
    };
    s.sinks.push(Arc::new(FsSink::new(d.p("local")).allow_delete(true)));
    let o = take(&b, None, 2, &s, "scheduled", &clock).unwrap();
    assert_eq!(o.copy_failures.len(), 1);
    assert!(
        o.copy_failures[0].1.contains("is the directory the backup is in"),
        "{:?}",
        o.copy_failures
    );
    assert!(o.backup.copies.is_empty());
    let st = stats(&*b, START).unwrap();
    assert!(st.copy_age_seconds.is_none(), "no off-host copy is claimed");
    assert!(st.last_failure.is_some());
    clock.advance(DAY);
    let o = take(&b, None, 2, &s, "scheduled", &clock).unwrap();
    assert!(o.pruned.is_empty(), "the only copy is never removed: {:?}", o.pruned);
    assert_eq!(ids(&b).len(), 2);
    assert_eq!(d.files("local"), 4);
}

#[test]
fn a_backup_stamped_in_the_future_does_not_silence_the_schedule_or_the_age_metric() {
    let d = Dir::new("future");
    let b = db();
    // A clock a year ahead takes a scheduled backup.
    let wrong = FakeClock::at(START + 400 * DAY);
    let s = settings(&d);
    take(&b, None, 2, &s, "scheduled", &wrong).unwrap();
    // The clock is corrected. The skewed record is not the newest backup.
    let sched = Schedule::new(settings(&d), Arc::new(FakeClock::at(START + 3 * DAY)));
    assert!(sched.due(&b).unwrap(), "the schedule is not silenced");
    let st = stats(&*b, START + 3 * DAY).unwrap();
    assert_eq!(st.backup_age_seconds, None, "a future backup is not 0 seconds old");
    assert!(st.backup_overdue(), "so the alert can fire");
    assert_eq!(st.future_backups, 1);
    // And a backup taken now becomes the newest at once, and is not due again for a day.
    let o = sched.tick(&b, None, 2).unwrap();
    assert!(o.is_some());
    assert!(!sched.due(&b).unwrap());
    let st = stats(&*b, START + 3 * DAY + 60).unwrap();
    assert_eq!(st.backup_age_seconds, Some(60));
    // A small skew is tolerated: it is not called a fault.
    let ok = take(&b, None, 2, &s, "scheduled", &FakeClock::at(START + 3 * DAY + 100)).unwrap();
    assert!(ok.backup.verified);
    assert_eq!(stats(&*b, START + 3 * DAY).unwrap().future_backups, 1);
}

#[test]
fn a_cron_run_sets_and_clears_the_failure_state_like_the_timer() {
    let d = Dir::new("cron");
    let b = db();
    let clock = FakeClock::at(START);
    let sink = Arc::new(MemorySink::default());
    *sink.fail_put.lock().unwrap() = true;
    let mut s = settings(&d);
    s.sinks.push(sink.clone());
    // `obop backup run` calls `take` directly, with no timer anywhere.
    take(&b, None, 2, &s, "scheduled", &clock).unwrap();
    let st = stats(&*b, START).unwrap();
    assert!(st.last_failure.unwrap().contains("unreachable"));
    *sink.fail_put.lock().unwrap() = false;
    clock.advance(60);
    take(&b, None, 2, &s, "scheduled", &clock).unwrap();
    assert!(
        stats(&*b, START + 60).unwrap().last_failure.is_none(),
        "a later good run clears what an earlier one reported"
    );
    // A manual backup counts too; a backup before a migration is not a run of the job.
    *sink.fail_put.lock().unwrap() = true;
    clock.advance(60);
    take(&b, None, 2, &s, "pre-migration", &clock).unwrap();
    assert!(stats(&*b, START + 120).unwrap().last_failure.is_none());
    clock.advance(60);
    take(&b, None, 2, &s, "manual", &clock).unwrap();
    assert!(
        stats(&*b, START + 180).unwrap().last_failure.is_some(),
        "a manual run through the job is reported like a scheduled one"
    );
}

#[test]
fn a_persistent_fault_writes_no_backup_a_minute_and_leaves_no_files_behind() {
    let d = Dir::new("loop");
    let (flaky, b) = flaky_db();
    let clock = Arc::new(FakeClock::at(START));
    let sched = Schedule::new(settings(&d), clock.clone());
    // The catalogue cannot be written: every attempt fails after the files were written.
    crate::testkit::Flaky::set(&flaky.fail_apply, true);
    let first = sched.tick(&b, None, 2).unwrap_err();
    assert!(first.contains("could not be recorded"), "{first}");
    assert_eq!(d.files("local"), 0, "an unrecorded backup is removed again");
    // Half a minute later the timer is still waiting; a minute after the failure it tries once
    // more, fails again, and now waits two minutes.
    clock.advance(30);
    assert!(sched.tick(&b, None, 2).unwrap().is_none(), "backing off");
    clock.advance(30);
    assert!(sched.tick(&b, None, 2).unwrap_err().contains("could not be recorded"));
    for later in [30, 60, 20] {
        clock.advance(later);
        assert!(sched.tick(&b, None, 2).unwrap().is_none(), "waiting two minutes");
    }
    assert_eq!(d.files("local"), 0, "no backup was written while it waited");
    assert!(backoff_seconds(1) == 60 && backoff_seconds(2) == 120 && backoff_seconds(30) == 3600);
    // The fault is fixed; after the wait the timer takes its backup and the state is clear.
    crate::testkit::Flaky::set(&flaky.fail_apply, false);
    clock.advance(10);
    assert!(sched.tick(&b, None, 2).unwrap().is_some());
    assert!(stats(&*b, START + 400).unwrap().last_failure.is_none());
}

#[test]
fn a_sink_moved_to_another_path_receives_the_backups_it_missed() {
    let d = Dir::new("respell");
    let b = db();
    let clock = FakeClock::at(START);
    let mut s = settings(&d);
    s.sinks.push(Arc::new(FsSink::new(d.p("off-a"))));
    for _ in 0..3 {
        take(&b, None, 2, &s, "scheduled", &clock).unwrap();
        clock.advance(DAY);
    }
    assert_eq!(d.files("off-a"), 6);
    // The share moves: the same data is configured at another path.
    let mut moved = settings(&d);
    moved.sinks.push(Arc::new(FsSink::new(d.p("off-b"))));
    let o = take(&b, None, 2, &moved, "scheduled", &clock).unwrap();
    assert!(o.copy_failures.is_empty());
    assert_eq!(d.files("off-b"), 8, "all four backups, data and manifest");
    assert!(
        o.notes.iter().any(|n| n.contains("earlier backup(s) copied")),
        "{:?}",
        o.notes
    );
}

#[test]
fn no_scheduled_backup_outlives_the_age_bound_even_when_the_counts_would_keep_it() {
    let d = Dir::new("age");
    let b = db();
    let clock = FakeClock::at(START);
    let s = settings(&d);
    let old = take(&b, None, 2, &s, "scheduled", &clock).unwrap().backup;
    // An outage of 450 days, then the schedule resumes.
    clock.advance(450 * DAY);
    let o = take(&b, None, 2, &s, "scheduled", &clock).unwrap();
    assert!(o.pruned.contains(&old.id), "{:?}", o.pruned);
    assert!(!Path::new(&old.path).exists());
    // Without the bound the counts keep it.
    let d2 = Dir::new("age2");
    let b2 = db();
    let clock2 = FakeClock::at(START);
    let mut s2 = settings(&d2);
    s2.max_age = None;
    let old2 = take(&b2, None, 2, &s2, "scheduled", &clock2).unwrap().backup;
    clock2.advance(450 * DAY);
    let o = take(&b2, None, 2, &s2, "scheduled", &clock2).unwrap();
    assert!(!o.pruned.contains(&old2.id));
    // A manual backup is never aged out.
    clock.advance(60);
    let manual = take(&b, None, 2, &s, "manual", &clock).unwrap().backup;
    clock.advance(900 * DAY);
    take(&b, None, 2, &s, "scheduled", &clock).unwrap();
    assert!(Path::new(&manual.path).exists());
}

#[test]
fn a_failed_export_removes_what_it_began_and_is_reported_for_a_cron_run_too() {
    let d = Dir::new("export");
    let (flaky, b) = flaky_db();
    crate::testkit::Flaky::set(&flaky.fail_export, true);
    let e = take(&b, None, 2, &settings(&d), "scheduled", &FakeClock::at(START)).unwrap_err();
    assert!(e.contains("went away"), "{e}");
    assert_eq!(d.files("local"), 0);
    assert!(stats(&*b, START).unwrap().last_failure.unwrap().contains("went away"));
}
