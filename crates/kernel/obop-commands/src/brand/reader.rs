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
//!   default for a failure) and does not read the store itself.
//!
//! A failed refill is never kept: the cell stays as it was, the owner gets the last good value (the
//! default if there is none) and the next caller starts a refill of its own. When `apply` changes the
//! row in this process it replaces the brand part of the cell once its batch has committed and bumps
//! a counter; a refill stores its outcome only if the counter is the one it started under, so a refill
//! that read the old row before the change cannot put the replaced brand back.
//!
//! **`fresh()`** is the uncached read of the row and the theme that `get` makes: a store failure is
//! an error, never `set: false`. **[`strict`]** reads the row alone, every time, for the text that is
//! written into the person's own device.
//!
//! The reader is a total function of what the store holds: it never panics on a row. It does not
//! hide a store failure from D74: the read is not run inside `fault::detached`, so the request that
//! met the fault is answered with the 503 by the layer above.

use super::row::{self, Fallback, Shown};
use super::{MODEL, ROW_ID, SETTING_ID, SETTING_MODEL, UNREADABLE};
use obop_brand::Brand;
use obop_storage::{Backend, Fields};
use std::collections::HashSet;
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
    /// When the last good refill stored it. `apply` replaces `shown` and leaves this alone.
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
    /// The fields already written to the sink: one line per field and process.
    noted: Mutex<HashSet<String>>,
    sink: Sink,
    inert: Arc<Shown>,
}

/// The uncached read of the row and the theme setting, with the row as it is stored.
pub struct Fresh {
    shown: Shown,
    stored: Option<Fields>,
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
}

fn recover<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Reads the row and then the theme setting. Either failing fails the unit, with the storage error
/// unchanged.
fn read_unit(backend: &dyn Backend) -> Result<(Option<Fields>, Option<String>), String> {
    let stored = backend.get(MODEL, ROW_ID)?;
    let setting = backend.get(SETTING_MODEL, SETTING_ID)?;
    Ok((stored, row::accent_of(setting.as_ref())))
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
            noted: Mutex::new(HashSet::new()),
            sink: Arc::new(|line| eprintln!("{line}")),
            inert: Arc::new(Shown::not_set(None)),
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
    /// cannot be read gives the last good value, and the default in a process that never had one.
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
                let flight = recover(&self.refill);
                let generation = recover(&self.cell).generation;
                match &flight.slot {
                    Some(slot) if slot.generation == generation => slot
                        .outcome
                        .clone()
                        .unwrap_or_else(|| Arc::new(Shown::not_set(None))),
                    // The flight left nothing (its owner panicked), or an `apply` committed while
                    // it ran: its outcome may be the row that was replaced. This caller holds the
                    // lock, so it refills itself.
                    _ => self.own(flight, backend),
                }
            }
        }
    }

    /// The owner of a flight. Looks at the cell again first, then reads and stores the outcome.
    fn own(&self, mut flight: MutexGuard<'_, Flight>, backend: &dyn Backend) -> Arc<Shown> {
        let (last_good, started) = {
            let cell = recover(&self.cell);
            if let Some(shown) = cell.fresh(self.bound) {
                return shown;
            }
            (cell.shown.clone(), cell.generation)
        };
        flight.slot = None;
        match read_unit(backend) {
            Ok((stored, accent)) => {
                let shown = Arc::new(row::read_row(stored.as_ref(), accent));
                self.note(shown.fallbacks());
                {
                    let mut cell = recover(&self.cell);
                    if cell.generation == started {
                        cell.shown = Some(shown.clone());
                        cell.at = Some(Instant::now());
                    }
                }
                flight.slot = Some(Slot {
                    outcome: Some(shown.clone()),
                    generation: started,
                });
                shown
            }
            Err(_) => {
                flight.slot = Some(Slot {
                    outcome: None,
                    generation: started,
                });
                last_good.unwrap_or_else(|| Arc::new(Shown::not_set(None)))
            }
        }
    }

    /// Writes one line for each field that fell back, once per field for the life of the reader.
    /// The line names the field and a fixed reason word; it holds nothing of the stored value.
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
            });
        };
        let (stored, accent) = read_unit(&**backend)?;
        Ok(Fresh {
            shown: row::read_row(stored.as_ref(), accent),
            stored,
        })
    }

    /// As the free function [`strict`] over this reader's store; an inert reader answers the
    /// default brand.
    pub fn strict(&self) -> Result<Brand, String> {
        match &self.backend {
            Some(b) => strict(&**b),
            None => Ok(Brand::default()),
        }
    }
}

/// The brand read from the row alone, every time, never from a cache and without the theme: for
/// the text that is written into the person's own device and cannot be corrected afterwards (the
/// authenticator label, the passkey relying-party name). It fails when the store read fails (the
/// storage error text, unchanged, so that a D74 code word reaches the layer above) or when the
/// stored `name` fails the safety subset (a fixed message that starts with [`UNREADABLE`]). No row
/// at all is not a failure: the default is the answer.
pub fn strict(backend: &dyn Backend) -> Result<Brand, String> {
    let Some(stored) = backend.get(MODEL, ROW_ID)? else {
        return Ok(Brand::default());
    };
    let (brand, fallbacks) = row::read_text(&stored);
    if fallbacks.iter().any(|f| f.field == "name") {
        return Err(format!("{UNREADABLE}: the stored name cannot be shown"));
    }
    Ok(brand)
}
