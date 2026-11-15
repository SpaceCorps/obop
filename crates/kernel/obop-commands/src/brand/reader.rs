//! The reader of the instance brand (docs/spec/branding.md, section 4): the cache every page asks,
//! and the two reads that never answer from it.
//!
//! **`current()`** returns the brand for display. Its cell keeps the outcome of the last good
//! refill (one unit: the row and the theme setting) for at most the bound, two seconds. The cell
//! sits behind a standard mutex that is held for a moment and never across a call of the store.
//! The refill is single flight and a caller that has a last good value never waits for it:
//!
//! - A caller whose cell is stale (empty, or older than the bound) takes the refill mutex with
//!   `try_lock`. There are three results.
//! - *Obtained*: this caller owns the refill. It first looks at the cell again, because a caller
//!   that read the stale time just before the previous flight stored its outcome gets the lock right
//!   after that flight and finds the cell fresh; it then reads nothing, so one expiry costs one read.
//!   If the cell is still stale it reads the row and the theme and stores the outcome.
//! - *Poisoned*: an earlier owner panicked while it held the lock. The lock is obtained all the
//!   same and recovered with `into_inner`; it is never taken for "a refill is running", or the last
//!   good brand would be served for ever and no refill would run again.
//! - *Would block*: a refill is running. A caller with a last good value serves it at once. A caller
//!   with none (a process that never had a good read) blocks on the refill mutex and, when it gets
//!   it, reads the outcome the owner left in the slot of the flight (a good value or the failure; the
//!   default for a failure) and does not read the store itself. It refills for itself, as an owner,
//!   in the three cases where that slot is not an answer for it: the slot is empty (the owner
//!   panicked), `apply` committed while that flight ran (its outcome may be the row that was
//!   replaced), or the slot belongs to a flight that had already finished when this caller arrived
//!   (the lock was held by another waiter that was reading it, not by a running flight: the slot is
//!   not the cache, and a caller that comes later never reads it). Each finished flight takes the
//!   next number of a counter; a caller notes the counter when it finds the cell stale and uses a
//!   slot only if the number in it is newer.
//!
//! A failed refill is never kept: the cell stays as it was, the owner gets the last good value of the
//! cell as it is when the read fails (the default if there is none, marked [`Shown::is_unread`] so
//! that a cache above the reader can tell it from the answer "no brand is set"), and the next caller
//! starts a refill of its own. When `apply` changes the row in this process it bumps a counter and, once its
//! batch has committed, replaces the brand part of the cell; a refill stores its outcome only if the
//! counter is the one it started under, so a refill that read the old row before the change cannot
//! put the replaced brand back. The cell is dated by the start of the read, not by its end, so a slow
//! store cannot keep an old row for longer than the bound after the flight ended.
//!
//! **`fresh()`** is the uncached read of the row and the theme that `get` makes (and the command
//! line `obop admin brand status`, and the name that signs a Slack message, which is read for itself
//! because a message that has left cannot be taken back, and `current()` can give a caller behind a
//! failed first refill the default with no fault noted on its thread, docs/ISSUES.md PS3-9): a store
//! failure is an error, never `set: false`. **[`strict`]** reads the row alone, every time, for the
//! text that is written into the person's own device.
//!
//! The reader is a total function of what the store holds: it never panics on a row. Its own lines
//! (the sink) are written after the refill lock is released, and the default sink ignores a stderr
//! that cannot be written. It does not hide a store failure from D74: the read is not run inside
//! `fault::detached`, so the request that met the fault is answered with the 503 by the layer above.

use super::enrol::unfit_for_a_label;
use super::row::{self, Fallback, Shown};
use super::{MODEL, ROW_ID, SETTING_ID, SETTING_MODEL, UNREADABLE};
use obop_brand::Brand;
use obop_storage::{Backend, Fields};
use serde_json::{Map, Value as J};
use std::collections::HashSet;
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, TryLockError};
use std::time::{Duration, Instant};

/// Where the one line about a stored field that fell back is written. The default writes to
/// stderr; a test injects a sink and counts the lines.
pub type Sink = Arc<dyn Fn(&str) + Send + Sync>;

/// How long a good refill is kept: the bound D67 sets for the one unauthenticated route that reads
/// the database.
pub const DEFAULT_BOUND: Duration = Duration::from_secs(2);

#[derive(Default)]
struct Cell {
    /// The last good brand, of any age.
    shown: Option<Arc<Shown>>,
    /// When the read of the last good refill began (not when it ended). `apply` replaces `shown`
    /// and leaves this alone.
    at: Option<Instant>,
    /// Bumped by every `apply` that committed.
    generation: u64,
}

impl Cell {
    fn fresh(&self, bound: Duration) -> Option<Arc<Shown>> {
        match (&self.shown, self.at) {
            (Some(shown), Some(at)) if at.elapsed() < bound => Some(shown.clone()),
            _ => None,
        }
    }
}

/// What the owner of a flight leaves for the callers that waited for it.
struct Slot {
    /// The good value, or `None` when the flight failed.
    outcome: Option<Arc<Shown>>,
    /// The counter of the cell when the flight started.
    generation: u64,
    /// The number the flight took when it finished (`Reader::finished`): a caller that noted a
    /// number at least this high arrived after the flight and does not read its slot.
    epoch: u64,
}

/// The state of the refill mutex: the slot of the flight that ran last. The next flight clears it
/// when it starts, so a flight that panics leaves it empty.
#[derive(Default)]
struct Flight {
    slot: Option<Slot>,
}

/// The reader of the brand. Held by the dispatcher (`Dispatcher::brand`); there is no global.
pub struct Reader {
    /// `None`: an inert reader, which answers the default brand and reads nothing.
    backend: Option<Arc<dyn Backend>>,
    bound: Duration,
    cell: Mutex<Cell>,
    refill: Mutex<Flight>,
    /// How many flights have finished (left a slot). Written under the refill lock.
    finished: AtomicU64,
    /// The fields already written to the sink: one line per field and process.
    noted: Mutex<HashSet<String>>,
    sink: Sink,
    inert: Arc<Shown>,
    /// Hooks for the tests; empty and free in a build that is not a test build.
    probe: probe::Probe,
}

/// The uncached read of the row and the theme setting, with the row as it is stored.
pub struct Fresh {
    shown: Shown,
    stored: Option<Fields>,
    /// The `site` object of the instance settings, from the read that gave the theme (empty when
    /// the settings hold none): what the public site's entries of `effective` are decided from.
    site: Map<String, J>,
}

impl Fresh {
    /// What the pages would show of the row that was just read.
    pub fn shown(&self) -> &Shown {
        &self.shown
    }

    /// Whether a row exists.
    pub fn set(&self) -> bool {
        self.stored.is_some()
    }

    /// A stored text field as stored (empty when absent, null or not text). For `logo_svg` the
    /// answer of `get` does not use this: the artwork is served by the logo route.
    pub fn stored_text(&self, field: &str) -> &str {
        self.stored
            .as_ref()
            .and_then(|f| row::text_of(f, field).ok())
            .unwrap_or("")
    }

    /// The stored row as it is, or none.
    pub fn stored(&self) -> Option<&Fields> {
        self.stored.as_ref()
    }

    /// The `site` object of the instance settings as the read left it.
    pub(crate) fn site(&self) -> &Map<String, J> {
        &self.site
    }
}

fn recover<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Reads the row and then the instance settings (the theme the brand defers to, and the `site`
/// object that `get` reports on). Either failing fails the unit, with the storage error unchanged.
fn read_unit(backend: &dyn Backend) -> Result<(Option<Fields>, Option<Fields>), String> {
    let stored = backend.get(MODEL, ROW_ID)?;
    let setting = backend.get(SETTING_MODEL, SETTING_ID)?;
    Ok((stored, setting))
}

impl Reader {
    /// A reader against a store, with the bound of two seconds.
    pub fn new(backend: Arc<dyn Backend>) -> Reader {
        Reader::with_bound(backend, DEFAULT_BOUND)
    }

    /// A reader with another bound. Tests build one with a zero bound, which re-reads on every
    /// call.
    pub fn with_bound(backend: Arc<dyn Backend>, bound: Duration) -> Reader {
        Reader::build(Some(backend), bound)
    }

    /// A reader with no store: the default brand and nothing else. What a dispatcher that never
    /// ran `register_admin` gives.
    pub fn inert() -> Reader {
        Reader::build(None, DEFAULT_BOUND)
    }

    fn build(backend: Option<Arc<dyn Backend>>, bound: Duration) -> Reader {
        Reader {
            backend,
            bound,
            cell: Mutex::new(Cell::default()),
            refill: Mutex::new(Flight::default()),
            finished: AtomicU64::new(0),
            noted: Mutex::new(HashSet::new()),
            // Not `eprintln!`, which panics when stderr cannot be written: the reader is total.
            sink: Arc::new(|line| {
                let _ = writeln!(std::io::stderr(), "{line}");
            }),
            inert: Arc::new(Shown::not_set(None)),
            probe: probe::Probe::new(),
        }
    }

    /// The same reader writing its lines about fields that fell back to `sink`.
    pub fn with_sink(mut self, sink: Sink) -> Reader {
        self.sink = sink;
        self
    }

    /// Whether this reader has no store.
    pub fn is_inert(&self) -> bool {
        self.backend.is_none()
    }

    /// The brand for display: see the module documentation for the cache. Never fails: a store that
    /// cannot be read gives the last good value, and the default in a process that never had one
    /// (that default answers [`Shown::is_unread`]).
    pub fn current(&self) -> Arc<Shown> {
        let Some(backend) = &self.backend else {
            return self.inert.clone();
        };
        {
            let cell = recover(&self.cell);
            if let Some(shown) = cell.fresh(self.bound) {
                return shown;
            }
        }
        self.after_stale(&**backend)
    }

    /// What a caller does once it has seen the cell stale: take the refill lock with `try_lock`, or
    /// serve or wait if a refill is running. A separate step, so that a test can run it after
    /// another flight has stored, which is the gap between the two that the owner's second look at
    /// the cell exists for.
    pub(super) fn after_stale(&self, backend: &dyn Backend) -> Arc<Shown> {
        // The flights that had finished when this caller found the cell stale: the slot of one of
        // them is not the answer to this caller.
        let seen = self.finished.load(Ordering::Acquire);
        match self.refill.try_lock() {
            Ok(flight) => self.own(flight, backend),
            Err(TryLockError::Poisoned(p)) => self.own(p.into_inner(), backend),
            Err(TryLockError::WouldBlock) => {
                // A refill is running. A caller with a last good value is served at once.
                let last_good = recover(&self.cell).shown.clone();
                if let Some(shown) = last_good {
                    return shown;
                }
                // A caller with none waits for the flight and reads what its owner left.
                self.probe.about_to_wait();
                let flight = recover(&self.refill);
                let generation = recover(&self.cell).generation;
                match &flight.slot {
                    Some(slot) if slot.epoch > seen && slot.generation == generation => slot
                        .outcome
                        .clone()
                        .unwrap_or_else(|| Arc::new(Shown::unread())),
                    // The flight left nothing (its owner panicked), or an `apply` committed while
                    // it ran (its outcome may be the row that was replaced), or the slot is of a
                    // flight that had finished before this caller arrived (the lock was held by
                    // another waiter that was reading it). This caller holds the lock, so it
                    // refills itself.
                    _ => self.own(flight, backend),
                }
            }
        }
    }

    /// The owner of a flight. Looks at the cell again first, then reads and stores the outcome.
    fn own(&self, mut flight: MutexGuard<'_, Flight>, backend: &dyn Backend) -> Arc<Shown> {
        let (began, started) = {
            let cell = recover(&self.cell);
            if let Some(shown) = cell.fresh(self.bound) {
                return shown;
            }
            // What this flight stores is dated from here, not from the end of a read that a slow
            // store may have stretched.
            (Instant::now(), cell.generation)
        };
        flight.slot = None;
        match read_unit(backend) {
            Ok((stored, setting)) => {
                let accent = row::accent_of(setting.as_ref());
                let shown = Arc::new(row::read_row(stored.as_ref(), accent));
                {
                    let mut cell = recover(&self.cell);
                    if cell.generation == started {
                        cell.shown = Some(shown.clone());
                        cell.at = Some(began);
                    }
                }
                flight.slot = Some(Slot {
                    outcome: Some(shown.clone()),
                    generation: started,
                    epoch: self.finished.fetch_add(1, Ordering::AcqRel) + 1,
                });
                // The lines go out with no lock held: a sink is not the reader's to trust.
                drop(flight);
                self.note(shown.fallbacks());
                shown
            }
            Err(_) => {
                // The last good value as the cell holds it now, not as it held it when the read
                // began: an `apply` that committed meanwhile has replaced it, and a failing refill
                // never serves the brand that was replaced or cleared.
                let last_good = recover(&self.cell).shown.clone();
                flight.slot = Some(Slot {
                    outcome: None,
                    generation: started,
                    epoch: self.finished.fetch_add(1, Ordering::AcqRel) + 1,
                });
                last_good.unwrap_or_else(|| Arc::new(Shown::unread()))
            }
        }
    }

    /// Writes one line for each field that fell back, once per field for the life of the reader.
    /// The line names the field and a fixed reason word; it holds nothing of the stored value. The
    /// command it points to is `obop admin brand status` (the command line of `get`, spec section
    /// 3); `crates/obop/tests/brand_cli.rs` runs the command this line names, read from the line
    /// itself, so the two cannot drift apart.
    fn note(&self, fallbacks: &[Fallback]) {
        for f in fallbacks {
            if recover(&self.noted).insert(f.field.clone()) {
                (self.sink)(&format!(
                    "obop: the stored {} of the instance branding is not used ({}); the default is shown instead (obop admin brand status lists it)",
                    f.field, f.code
                ));
            }
        }
    }

    /// The uncached read of the row and the theme that `get` makes: a store failure is returned as
    /// an error with its text unchanged, never as a brand that is not set.
    pub fn fresh(&self) -> Result<Fresh, String> {
        let Some(backend) = &self.backend else {
            return Ok(Fresh {
                shown: Shown::not_set(None),
                stored: None,
                site: Map::new(),
            });
        };
        let (stored, setting) = read_unit(&**backend)?;
        Ok(Fresh {
            shown: row::read_row(stored.as_ref(), row::accent_of(setting.as_ref())),
            stored,
            site: row::site_of(setting.as_ref()),
        })
    }

    /// As the free function [`strict`] over this reader's store; an inert reader answers the
    /// default brand. A caller that writes the name into a device and may be handed an inert reader
    /// (the passkey registration) must ask [`Reader::is_inert`] first and refuse with
    /// [`no_reader_error`](super::no_reader_error): the default here is the answer of a reader with
    /// nothing to read, not of a row that was read.
    pub fn strict(&self) -> Result<Brand, String> {
        match &self.backend {
            Some(b) => strict(&**b),
            None => Ok(Brand::default()),
        }
    }

    /// Called by `apply` once its batch has committed: bumps the counter and, when the cell holds a
    /// value, replaces its brand part with what the new row shows. The cell keeps its theme part
    /// and its age. A process that has not read yet has nothing to replace, and its first read
    /// reads the new row. `row` is the row that was written, or none after a `clear`.
    ///
    /// The cell is held twice, for a moment each, and not while the row is read: the first time to
    /// bump the counter (from then on no refill that began before the change can store) and to take
    /// the accent and the age of the value, the second to put the new value in. The parse and the
    /// hash of the artwork happen between, with no lock held. A refill that began after the bump
    /// read the new row; if one stored in between, the age of the cell has moved and its value
    /// stays.
    pub(crate) fn applied(&self, row: Option<&Fields>) {
        let (accent, at) = {
            let mut cell = recover(&self.cell);
            cell.generation = cell.generation.wrapping_add(1);
            match &cell.shown {
                Some(old) => (old.theme_accent().map(str::to_string), cell.at),
                None => return,
            }
        };
        self.probe.in_apply_gap();
        let built = Arc::new(row::read_row(row, accent));
        let mut cell = recover(&self.cell);
        if cell.at == at {
            cell.shown = Some(built);
        }
    }

    /// Poisons the mutex of the cell, as a thread that panics while it holds it would. For tests.
    #[cfg(test)]
    pub(crate) fn poison_cell(self: &Arc<Self>) {
        let me = self.clone();
        let _ = std::thread::spawn(move || {
            let _held = me.cell.lock().unwrap();
            panic!("a panic while the cell is held");
        })
        .join();
    }

    /// The counter of the cell. For tests.
    #[cfg(test)]
    pub(crate) fn generation(&self) -> u64 {
        recover(&self.cell).generation
    }

    /// Makes the cell stale without touching the last good value, as two seconds going by would.
    /// For tests, also those of other crates (the feature `test-support`): they hold a request to
    /// the bound of the cell without waiting for the clock.
    #[cfg(any(test, feature = "test-support"))]
    pub fn expire(&self) {
        recover(&self.cell).at = None;
    }

    /// Makes the cell fresh for an hour, whatever its age, and keeps its value: the counterpart of
    /// [`Reader::expire`]. For tests of other crates (the feature `test-support`) that hold what a
    /// request costs while the cell is fresh, without a window of the clock to stay inside.
    #[cfg(any(test, feature = "test-support"))]
    pub fn hold_fresh(&self) {
        recover(&self.cell).at = Some(Instant::now() + Duration::from_secs(3600));
    }

    /// Makes the value in the cell `by` older than it is. For tests.
    #[cfg(test)]
    pub(crate) fn back_date(&self, by: Duration) {
        let mut cell = recover(&self.cell);
        if let Some(at) = cell.at {
            cell.at = Some(at.checked_sub(by).expect("the machine has been up longer"));
        }
    }

    /// Holds the refill lock, as a waiter that is reading a slot does. For tests.
    #[cfg(test)]
    pub(crate) fn hold_refill(&self) -> impl Sized + '_ {
        recover(&self.refill)
    }

    /// Whether a thread panicked while it held the refill lock. For tests.
    #[cfg(test)]
    pub(crate) fn refill_is_poisoned(&self) -> bool {
        self.refill.is_poisoned()
    }

    /// Whether the slot of the last flight is empty. For tests.
    #[cfg(test)]
    pub(crate) fn slot_is_empty(&self) -> bool {
        recover(&self.refill).slot.is_none()
    }

    /// How many callers have found a flight running with no last good value to serve: a rendezvous
    /// that is a condition and not a time. For tests, also those of other crates (the feature
    /// `test-support`).
    #[cfg(any(test, feature = "test-support"))]
    pub fn waiting(&self) -> usize {
        self.probe.waiting()
    }

    /// Runs `hook` inside `applied`, between the bump of the counter and the replacement of the
    /// value, with no lock held. For tests.
    #[cfg(test)]
    pub(crate) fn on_applied_gap(&self, hook: probe::Hook) {
        self.probe.set_gap(hook);
    }
}

/// The brand read from the row alone, every time, never from a cache and without the theme: for
/// the text that is written into the person's own device and cannot be corrected afterwards (the
/// authenticator label, the passkey relying-party name). It fails when the store read fails (the
/// storage error text, unchanged, so that a D74 code word reaches the layer above) or when the
/// stored `name` fails the safety subset (a name of nothing but spaces does, since S1c), or passes it
/// and still cannot go into a label (a link character; see `enrol::unfit_for_a_label`, which also
/// keeps an arm for white space only that no stored name reaches since S1c): a fixed message that starts
/// with [`UNREADABLE`]. No row at all is not a failure: the default is the answer.
pub fn strict(backend: &dyn Backend) -> Result<Brand, String> {
    let Some(stored) = backend.get(MODEL, ROW_ID)? else {
        return Ok(Brand::default());
    };
    let (brand, fallbacks) = row::read_text(&stored);
    if fallbacks.iter().any(|f| f.field == "name") {
        return Err(format!("{UNREADABLE}: the stored name cannot be shown"));
    }
    if unfit_for_a_label(brand.name()) {
        return Err(format!(
            "{UNREADABLE}: the stored name cannot be used in a label"
        ));
    }
    Ok(brand)
}

/// What the tests need to see and to hold still inside the reader: a count of the callers that are
/// about to block behind a flight (a rendezvous that is a condition and not a time) and a hook that
/// runs inside `applied`. Outside a test build (and a build with the feature `test-support`, which
/// the tests of other crates ask for) the probe has no state and no cost. It is a module
/// with a `#[cfg]` on it, and not fields and statements with one, so that the scan of `obop-http`
/// that lists every `#[cfg(test)]` on a construct of the code stays as it is.
#[cfg(not(any(test, feature = "test-support")))]
mod probe {
    pub(super) struct Probe;

    impl Probe {
        pub(super) fn new() -> Probe {
            Probe
        }

        #[inline(always)]
        pub(super) fn about_to_wait(&self) {}

        #[inline(always)]
        pub(super) fn in_apply_gap(&self) {}
    }
}

#[cfg(any(test, feature = "test-support"))]
mod probe {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    pub type Hook = Arc<dyn Fn() + Send + Sync>;

    pub(super) struct Probe {
        waiting: AtomicUsize,
        gap: Mutex<Option<Hook>>,
    }

    impl Probe {
        pub(super) fn new() -> Probe {
            Probe {
                waiting: AtomicUsize::new(0),
                gap: Mutex::new(None),
            }
        }

        /// A caller with no last good value found a flight running and will block on it.
        pub(super) fn about_to_wait(&self) {
            self.waiting.fetch_add(1, Ordering::SeqCst);
        }

        pub(super) fn waiting(&self) -> usize {
            self.waiting.load(Ordering::SeqCst)
        }

        /// `applied` has bumped the counter and holds no lock.
        pub(super) fn in_apply_gap(&self) {
            let hook = self.gap.lock().unwrap_or_else(|e| e.into_inner()).clone();
            if let Some(hook) = hook {
                hook();
            }
        }

        // Only the tests of this crate set a hook; a build for the tests of another crate has none.
        #[allow(dead_code)]
        pub(super) fn set_gap(&self, hook: Hook) {
            *self.gap.lock().unwrap_or_else(|e| e.into_inner()) = Some(hook);
        }
    }
}
