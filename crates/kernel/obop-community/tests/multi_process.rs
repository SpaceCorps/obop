//! Two server processes on one database (B1): each handle below is a separate `Forum` and
//! `Status` with its own memory copy over its own connection to the same database, exactly what
//! two `obop platform serve` processes hold. The file (SQLite) and PostgreSQL runs open the
//! database twice; PostgreSQL runs only with `OBOP_TEST_POSTGRES_URL`.

use obop_community::status::{CheckResult, Impact, IncStatus, Status};
use obop_community::store::Forum;
use obop_storage::Backend;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

const NOW: u64 = 1_800_000_000;

fn two_forums(a: Arc<dyn Backend>, b: Arc<dyn Backend>) -> (Forum, Forum) {
    let staff: HashSet<String> = HashSet::new();
    let fa = Forum::open(a, staff.clone()).unwrap();
    let fb = Forum::open(b, staff).unwrap();
    fa.set_sync_interval(Duration::ZERO);
    fb.set_sync_interval(Duration::ZERO);
    (fa, fb)
}

fn forum_stays_consistent(a: Arc<dyn Backend>, b: Arc<dyn Backend>) {
    let (fa, fb) = two_forums(a, b);
    let tag = std::process::id();
    let ada = format!("ada{tag}@example.test");
    let bob = format!("bob{tag}@example.test");
    // A registers; B knows the member without restarting.
    fa.register(&ada, Some(&format!("ada{tag}")), NOW).unwrap();
    assert!(fb.lock().members.contains_key(&ada));
    // A name taken on A is taken on B.
    assert!(fb.register(&bob, Some(&format!("ada{tag}")), NOW).is_err());
    fb.register(&bob, Some(&format!("bob{tag}")), NOW).unwrap();
    // A thread made on A, a reply made on B, both visible on both.
    let t = fa
        .create_thread(&ada, "general", "Hello from A", "first post", NOW)
        .unwrap();
    assert!(fb.lock().threads.contains_key(&t));
    let r = fb.reply(&bob, t, "reply from B", NOW + 1).unwrap();
    assert_eq!(fa.lock().posts[&r].body, "reply from B");
    assert_eq!(fa.lock().thread_posts(t).len(), 2);

    // Parallel posts from both processes: every id is unique and nothing is lost.
    let ids: Vec<u64> = std::thread::scope(|s| {
        let mut hs = Vec::new();
        for i in 0..6 {
            let (f, who) = if i % 2 == 0 { (&fa, &ada) } else { (&fb, &bob) };
            hs.push(s.spawn(move || {
                // Staff are exempt from the hourly rate limit, so use few posts per member.
                (0..3)
                    .map(|n| {
                        f.reply(who, t, &format!("parallel {i}-{n}"), NOW + 10 + n as u64)
                            .map_err(|e| format!("{e:?}"))
                    })
                    .collect::<Vec<_>>()
            }));
        }
        hs.into_iter()
            .flat_map(|h| h.join().unwrap())
            .filter_map(Result::ok)
            .collect()
    });
    let unique: HashSet<_> = ids.iter().collect();
    assert_eq!(
        unique.len(),
        ids.len(),
        "no two processes handed out one id"
    );
    // The posting limit (10 an hour) applies across processes: ada and bob each made at most 10.
    let total_a = fa.lock().thread_posts(t).len();
    let total_b = fb.lock().thread_posts(t).len();
    assert_eq!(total_a, total_b, "both processes see the same thread");
    assert_eq!(total_a, 2 + ids.len());
    // The limit counts what the other process wrote: B's member already has posts from B only,
    // but A pushes ada over her limit using posts both of them wrote.
    let mut refused = 0;
    for n in 0..12 {
        if matches!(
            fb.reply(&ada, t, &format!("flood {n}"), NOW + 100 + n),
            Err(obop_community::store::FErr::Rate(_))
        ) {
            refused += 1;
        }
    }
    assert!(refused >= 1, "the hourly limit holds across processes");
}

fn status_stays_consistent(a: Arc<dyn Backend>, b: Arc<dyn Backend>) {
    let (sa, sb) = (Status::open(a).unwrap(), Status::open(b).unwrap());
    sa.set_sync_interval(Duration::ZERO);
    sb.set_sync_interval(Duration::ZERO);
    sa.record("api", CheckResult::up(), NOW).unwrap();
    sb.record("api", CheckResult::up(), NOW + 60).unwrap();
    // Day counters add up across processes (neither overwrote the other).
    let day = |s: &Status| {
        let snap = s.snapshot(NOW + 120);
        let c = snap.comps.iter().find(|c| c.id == "api").unwrap();
        c.days.last().and_then(|d| d.pct)
    };
    assert_eq!(day(&sa), Some(100.0));
    assert_eq!(day(&sb), Some(100.0));
    // Incidents opened on both processes at once get distinct ids and show on both.
    let ids: Vec<u64> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..6)
            .map(|i| {
                let st = if i % 2 == 0 { &sa } else { &sb };
                s.spawn(move || {
                    st.create_incident(
                        &format!("incident {i}"),
                        Impact::Minor,
                        IncStatus::Investigating,
                        &["api".to_string()],
                        "looking",
                        NOW + 200,
                    )
                    .unwrap()
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let unique: HashSet<_> = ids.iter().collect();
    assert_eq!(unique.len(), 6);
    assert_eq!(sa.incidents().len(), sb.incidents().len());
    assert!(sa.incidents().len() >= 6);
    // Resolving on one process is seen on the other.
    sb.resolve_incident(ids[0], "fixed", NOW + 300).unwrap();
    assert!(!sa.incident(ids[0]).unwrap().open());
    // Only one process takes each round of checks.
    let every = Duration::from_secs(60);
    let leaders = [sa.lead_checks(every), sb.lead_checks(every)];
    assert_eq!(leaders.iter().filter(|x| **x).count(), 1);
}

fn file_pair() -> (Arc<dyn Backend>, Arc<dyn Backend>, std::path::PathBuf) {
    let path = std::env::temp_dir().join(
        format!(
            "obop-community-mp-{}-{:?}.db",
            std::process::id(),
            std::thread::current().id()
        )
        .replace(['(', ')'], ""),
    );
    let url = format!("sqlite://{}", path.display());
    (
        obop_storage::open(&url).unwrap(),
        obop_storage::open(&url).unwrap(),
        path,
    )
}

fn cleanup(path: &std::path::Path) {
    let dir = path.parent().unwrap();
    let stem = path.file_name().unwrap().to_string_lossy().to_string();
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        if e.file_name().to_string_lossy().starts_with(&stem) {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

#[test]
fn the_forum_stays_consistent_across_two_handles_on_memory() {
    let b = obop_storage::open("memory:").unwrap();
    forum_stays_consistent(b.clone(), b);
}

#[test]
fn the_forum_stays_consistent_across_two_processes_on_one_sqlite_file() {
    let (a, b, path) = file_pair();
    forum_stays_consistent(a, b);
    cleanup(&path);
}

#[test]
fn the_status_page_stays_consistent_across_two_processes_on_one_sqlite_file() {
    let (a, b, path) = file_pair();
    status_stays_consistent(a, b);
    cleanup(&path);
}

#[test]
fn both_stay_consistent_on_postgres_when_available() {
    let Ok(url) = std::env::var("OBOP_TEST_POSTGRES_URL") else {
        eprintln!("skipped: set OBOP_TEST_POSTGRES_URL to run against PostgreSQL");
        return;
    };
    // A scratch schema keeps the run independent of what other tests left in the database.
    forum_stays_consistent(
        obop_storage::open(&url).unwrap(),
        obop_storage::open(&url).unwrap(),
    );
    status_stays_consistent(
        obop_storage::open(&url).unwrap(),
        obop_storage::open(&url).unwrap(),
    );
}
