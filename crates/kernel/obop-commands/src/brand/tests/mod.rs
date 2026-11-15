//! Tests of the brand reader and of `apply` (docs/spec/branding.md, section 13: the S1 bullets that
//! are neither a page nor a route nor the model's commands). The store is a memory database
//! behind `double::Spy`, which counts reads per model and can park, fail, slow down or panic a
//! read; the rows that "were written around the command" are put straight into the store.

use super::{MODEL, ROW_ID, Reader, SETTING_ID, SETTING_MODEL, Sink};
use obop_storage::{Backend, Fields, Op};
use serde_json::Value as J;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

mod apply_tests;
mod audit;
mod cache;
mod describe;
mod double;
mod effective;
mod enrol;
mod faults;
mod pins;
mod race;
mod rows;
mod site;
mod strict;
mod switch;
mod total;

pub use double::{Spy, s};

/// The artwork of the placeholder logo (a plain "M"): already in canonical form.
pub const M_ARTWORK: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 64 64\"><rect width=\"64\" height=\"64\" rx=\"14\" fill=\"#495670\"/><path d=\"M 16 48 L 16 16 L 24 16 L 32 30 L 40 16 L 48 16 L 48 48 L 41 48 L 41 29 L 32 44 L 23 29 L 23 48 Z\" fill=\"#ffffff\"/></svg>";

pub fn fields(pairs: &[(&str, J)]) -> Fields {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

/// Puts the brand row into the store as it is, whatever it holds: a row written by a restore, an
/// import, a direct edit or another binary.
pub fn put_row(b: &dyn Backend, pairs: &[(&str, J)]) {
    b.apply_checked(&[Op::Put {
        model: MODEL.into(),
        id: ROW_ID.into(),
        fields: fields(pairs),
    }])
    .unwrap();
}

/// Sets the accent of the instance theme, as `setting.update` stores it.
pub fn put_theme(b: &dyn Backend, accent: &str) {
    let value = serde_json::json!({"theme": {"accent": accent}}).to_string();
    b.apply_checked(&[Op::Put {
        model: SETTING_MODEL.into(),
        id: SETTING_ID.into(),
        fields: fields(&[("value", s(&value))]),
    }])
    .unwrap();
}

/// Puts the instance settings into the store as one JSON object, whatever it holds: the theme and
/// the `site` keys of the public site together, as `setting.update` stores them.
pub fn put_settings(b: &dyn Backend, settings: J) {
    b.apply_checked(&[Op::Put {
        model: SETTING_MODEL.into(),
        id: SETTING_ID.into(),
        fields: fields(&[("value", s(&settings.to_string()))]),
    }])
    .unwrap();
}

/// A sink that keeps the lines it is given.
pub fn lines() -> (Sink, Arc<Mutex<Vec<String>>>) {
    let kept = Arc::new(Mutex::new(Vec::new()));
    let k = kept.clone();
    (
        Arc::new(move |l: &str| k.lock().unwrap().push(l.to_string())),
        kept,
    )
}

/// A reader over the double with the bound given, and the lines it writes.
pub fn reader(spy: &Arc<Spy>, bound: Duration) -> (Arc<Reader>, Arc<Mutex<Vec<String>>>) {
    let (sink, kept) = lines();
    let r = Reader::with_bound(spy.clone(), bound).with_sink(sink);
    (Arc::new(r), kept)
}

/// A bound so long that a test never sees the cell expire by itself.
pub const LONG: Duration = Duration::from_secs(3600);

/// Waits until `n` callers of the reader have found a flight running with no last good value and
/// are about to block on it. A condition and not a time: the owner holds the refill lock until the
/// test lets it go, so a caller that is counted cannot be answered before then. Panics after ten
/// seconds.
pub fn wait_waiting(r: &Reader, n: usize) {
    let until = Instant::now() + Duration::from_secs(10);
    while r.waiting() < n {
        assert!(
            Instant::now() < until,
            "{} callers are waiting, not {n}",
            r.waiting()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// The next `n` messages of a channel. When they do not come within ten seconds the parked reads
/// of the double are let go (so that no thread stays blocked) and the test fails, naming `what`:
/// a regression that makes a caller wait for a running flight fails the test and does not hang it.
pub fn take<T>(rx: &Receiver<T>, n: usize, spy: &double::Spy, what: &str) -> Vec<T> {
    let mut got = Vec::new();
    while got.len() < n {
        match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(x) => got.push(x),
            Err(_) => {
                spy.release();
                panic!("{what}: {} of {n} answers in ten seconds", got.len());
            }
        }
    }
    got
}
