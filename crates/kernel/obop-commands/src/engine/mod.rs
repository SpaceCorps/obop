//! The model engine (D56 to D58): the part of the command layer that knows what a model's rules
//! mean. Generated create, update, delete and action handlers call into it, so every surface
//! (REST, MCP, CLI) gets the same behaviour.
//!
//! - `unit`: a unit of work. Reads see the writes made so far; `commit` applies all of them in
//!   one storage transaction, or none.
//! - `computed`: computed fields, evaluated on read.
//! - `access`: row-level visibility.
//! - `seq`: sequence numbers, counters and uniqueness.
//! - `effects`: running an action: assertions, the transition, sequences and effects.
//! - `crud`: the generated create, update and delete, built from the above.

pub(crate) mod access;
pub(crate) mod bulk;
pub(crate) mod computed;
pub(crate) mod crud;
pub(crate) mod effects;
pub(crate) mod seq;
pub(crate) mod unit;

use crate::{ADMIN_GROUP, ANY_GROUP, ModelMeta, Principal, Store, Value};
use obop_registry::ModelDef;
use obop_values::Zones;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

pub(crate) type Models = Arc<RwLock<HashMap<String, ModelMeta>>>;

/// Seconds since the Unix epoch. Replaceable so tests can cross a year end.
pub type ClockFn = Arc<dyn Fn() -> i64 + Send + Sync>;

pub(crate) fn system_clock() -> ClockFn {
    Arc::new(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64)
    })
}

/// The time zone settings of the instance (D72): the database and the platform zone.
pub(crate) type SharedZones = Arc<RwLock<Arc<Zones>>>;

/// Everything the generated handlers share.
#[derive(Clone)]
pub(crate) struct Engine {
    pub store: Store,
    pub models: Models,
    pub clock: Arc<RwLock<ClockFn>>,
    pub zones: SharedZones,
    /// Set when a test replaced the clock: `$now` is then whole seconds of that clock, not the
    /// system time down to the microsecond.
    pub clock_replaced: Arc<AtomicBool>,
}

impl Engine {
    pub fn def(&self, name: &str) -> Option<Arc<ModelDef>> {
        self.models.read().unwrap().get(name).map(|m| m.def.clone())
    }

    pub fn now(&self) -> i64 {
        (self.clock.read().unwrap())()
    }

    /// The current time in microseconds since the epoch.
    pub fn now_micros(&self) -> i64 {
        if self.clock_replaced.load(Ordering::Relaxed) {
            return self.now().saturating_mul(1_000_000);
        }
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_micros() as i64)
    }

    pub fn zones(&self) -> Arc<Zones> {
        self.zones.read().unwrap().clone()
    }

    pub fn backend(&self) -> &dyn obop_storage::Backend {
        &*self.store.backend
    }

    /// A unit of work over this engine's backend, with the usual cap on the records it may touch.
    ///
    /// Every handler starts its unit here and not with `Unit::new(eng.backend())`, so what a unit
    /// needs to know about the call it serves (who writes, which rules guard the records) is
    /// added in this one place later and no handler changes (K18).
    pub fn unit(&self) -> unit::Unit<'_> {
        unit::Unit::new(self.backend())
    }

    /// As `unit`, with another cap on the records the unit may touch (bulk work).
    pub fn unit_with_cap(&self, cap: usize) -> unit::Unit<'_> {
        unit::Unit::with_cap(self.backend(), cap)
    }

    /// As `unit`, with no cap (sequence and counter work that is bounded by its own rules).
    pub fn unit_unbounded(&self) -> unit::Unit<'_> {
        unit::Unit::unbounded(self.backend())
    }
}

/// Who is calling, as far as the engine needs to know: for row-level access and for the group
/// checks of effects. Built from the principal by the dispatcher, which places it in the input
/// under `_actor*` keys that clients cannot send.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Actor {
    /// The person: for an agent, the user it acts for.
    pub id: String,
    /// The principal holds the instance administrator group (and, for an agent, its scopes allow
    /// it).
    pub admin: bool,
    /// Groups held and usable (scopes applied).
    pub groups: BTreeSet<String>,
    /// The principal as the audit trail names it (`alice`, `alice/agent`).
    pub caller: String,
    pub agent: bool,
}

pub(crate) const A_ID: &str = "_actor";
pub(crate) const A_ADMIN: &str = "_actor_admin";
pub(crate) const A_GROUPS: &str = "_actor_groups";
pub(crate) const A_CALLER: &str = "_actor_caller";
pub(crate) const A_AGENT: &str = "_actor_agent";

impl Actor {
    pub fn of(p: &Principal) -> Self {
        Self {
            id: p.id.strip_suffix("/agent").unwrap_or(&p.id).to_string(),
            admin: p.allows(ADMIN_GROUP),
            groups: p.groups.iter().filter(|g| p.allows(g)).cloned().collect(),
            caller: p.id.clone(),
            agent: p.kind == crate::PrincipalKind::Agent,
        }
    }

    /// Whether the actor may use `group`. `*` is every signed-in principal.
    pub fn holds(&self, group: &str) -> bool {
        group == ANY_GROUP || self.admin || self.groups.contains(group)
    }

    pub(crate) fn stamp(&self, m: &mut BTreeMap<String, Value>) {
        m.insert(A_ID.into(), Value::text(&self.id));
        m.insert(A_ADMIN.into(), Value::Bool(self.admin));
        m.insert(
            A_GROUPS.into(),
            Value::List(self.groups.iter().map(|g| Value::text(g)).collect()),
        );
        m.insert(A_CALLER.into(), Value::text(&self.caller));
        m.insert(A_AGENT.into(), Value::Bool(self.agent));
    }

    /// The actor a call carries. Without one (a handler called outside the dispatcher) the actor
    /// is nobody: it holds no group and owns nothing.
    pub fn from_input(input: &Value) -> Self {
        let Value::Map(m) = input else {
            return Self::default();
        };
        let text = |k: &str| match m.get(k) {
            Some(Value::Text(t)) => t.clone(),
            _ => String::new(),
        };
        Self {
            id: text(A_ID),
            admin: m.get(A_ADMIN) == Some(&Value::Bool(true)),
            groups: match m.get(A_GROUPS) {
                Some(Value::List(l)) => l
                    .iter()
                    .filter_map(|x| match x {
                        Value::Text(t) => Some(t.clone()),
                        _ => None,
                    })
                    .collect(),
                _ => BTreeSet::new(),
            },
            caller: {
                let c = text(A_CALLER);
                if c.is_empty() { text("_caller") } else { c }
            },
            agent: m.get(A_AGENT) == Some(&Value::Bool(true)),
        }
    }
}

// ---- time ----------------------------------------------------------------------------------

/// Calendar parts of a Unix time in UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Civil {
    pub year: i64,
    pub month: i64,
    pub day: i64,
    pub hour: i64,
    pub minute: i64,
    pub second: i64,
}

pub fn civil(secs: i64) -> Civil {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Days since 1970-01-01 to a proleptic Gregorian date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    Civil {
        year,
        month,
        day,
        hour: rem / 3600,
        minute: rem % 3600 / 60,
        second: rem % 60,
    }
}

impl Civil {
    pub fn date(&self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
    pub fn rfc3339(&self) -> String {
        format!(
            "{}T{:02}:{:02}:{:02}Z",
            self.date(),
            self.hour,
            self.minute,
            self.second
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_time_is_right_across_leap_days_and_year_ends() {
        assert_eq!(civil(0).date(), "1970-01-01");
        assert_eq!(civil(951_782_400).date(), "2000-02-29");
        assert_eq!(civil(1_709_164_800).date(), "2024-02-29");
        assert_eq!(civil(1_735_689_599).rfc3339(), "2024-12-31T23:59:59Z");
        assert_eq!(civil(1_735_689_600).date(), "2025-01-01");
        assert_eq!(civil(-1).rfc3339(), "1969-12-31T23:59:59Z");
    }

    /// The three constructors of a unit differ only in the cap on the records it may touch.
    #[test]
    fn the_unit_constructors_differ_only_in_their_cap() {
        use obop_registry::rules::MAX_EFFECT_RECORDS;
        let eng = crate::Dispatcher::new().engine(&Store::new());

        let mut u = eng.unit();
        u.spend(MAX_EFFECT_RECORDS).unwrap();
        assert!(u.spend(1).is_err());

        let mut u = eng.unit_with_cap(3);
        u.spend(3).unwrap();
        assert!(u.spend(1).is_err());

        let mut u = eng.unit_unbounded();
        u.spend(MAX_EFFECT_RECORDS * 10).unwrap();
    }

    /// Every unit of work, in every handler, starts from the engine (`Engine::unit*`), and the
    /// engine is built in one place (`Dispatcher::engine`). A path that built its own would be a
    /// unit the next change to the constructor does not reach, in the one place where the request
    /// guard and the unit's rules have to be the same for every handler. The scan reads the source
    /// of this crate and ignores the test code (what follows `#[cfg(test)]`, and the files of
    /// tests).
    #[test]
    fn no_handler_builds_its_own_unit_or_engine() {
        use std::path::Path;

        fn sources(dir: &Path, out: &mut Vec<(String, String)>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    sources(&path, out);
                    continue;
                }
                let name = path.file_name().unwrap().to_string_lossy().to_string();
                if !name.ends_with(".rs") || name == "tests.rs" || name.ends_with("_tests.rs") {
                    continue;
                }
                let text = std::fs::read_to_string(&path).unwrap();
                let live = text.split("#[cfg(test)]").next().unwrap().to_string();
                out.push((path.to_string_lossy().replace('\\', "/"), live));
            }
        }
        let mut files = Vec::new();
        sources(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut files,
        );
        assert!(files.len() > 20, "the scan found {} files", files.len());

        // What is built, how a line says so (the definitions `struct Engine {` and `impl Engine {`
        // are not constructions), and the only files that may say it.
        struct Rule {
            what: &'static str,
            says_so: fn(&str) -> bool,
            allowed: &'static [&'static str],
        }
        let rules = [
            Rule {
                what: "a unit",
                says_so: |l| {
                    ["Unit::new(", "Unit::with_cap(", "Unit::unbounded("]
                        .iter()
                        .any(|n| l.contains(n))
                },
                allowed: &["src/engine/mod.rs", "src/engine/unit.rs"],
            },
            Rule {
                what: "an engine",
                says_so: |l| {
                    let t = l.trim_start();
                    t.starts_with("Engine {") || t.contains("= Engine {") || t.contains("(Engine {")
                },
                allowed: &["src/store.rs"],
            },
        ];
        for Rule {
            what,
            says_so,
            allowed,
        } in rules
        {
            let mut seen_where_allowed = false;
            for (path, live) in &files {
                let hits = live
                    .lines()
                    .filter(|l| !l.trim_start().starts_with("//") && says_so(l))
                    .count();
                if allowed.iter().any(|a| path.ends_with(a)) {
                    seen_where_allowed |= hits > 0;
                } else {
                    assert_eq!(hits, 0, "{path} builds {what} of its own");
                }
            }
            assert!(
                seen_where_allowed,
                "nothing builds {what} where it should: the scan looks at the wrong thing"
            );
        }
    }
}
