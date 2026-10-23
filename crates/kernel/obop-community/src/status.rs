//! The status engine: component checks, per-day uptime, incidents (manual and automatic) and
//! scheduled maintenance. Pure logic over the storage backend; HTTP rendering lives in
//! `status_pages`. Time is always passed in so every rule is testable.
//!
//! Storage (JSON documents): `status_day` (`{component}:{day start}` -> up/degraded/down counts,
//! kept 100 days), `status_recent` (`{component}` -> the latest samples and last result),
//! `status_incident`, `status_maint`.
//!
//! Several server processes may share one database (B1, D59): the stored documents are the truth
//! and the memory copy a cache. A write takes a cross-process lock, reloads the copy if another
//! process changed anything, writes through and bumps a shared version; a read reloads when that
//! version moved (checked at most every `sync_every`). The background checker runs on one process
//! per interval (a shared lease), so day counters are not counted twice.

use crate::store::put;
use crate::util::{self, day_start};
use obop_storage::{Backend, Fields};
use serde_json::{Value as J, json};
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

pub const KEEP_DAYS: u64 = 100;
pub const BAR_DAYS: u64 = 90;
/// Consecutive failing checks that open an automatic incident, and passing ones that close it.
pub const AUTO_THRESHOLD: usize = 3;

pub const COMPONENTS: [(&str, &str); 6] = [
    ("website", "comp.website"),
    ("api", "comp.api"),
    ("community", "comp.community"),
    ("provisioning", "comp.provisioning"),
    ("celldb", "comp.celldb"),
    ("instances", "comp.instances"),
];

const T_DAY: &str = "status_day";
const T_RECENT: &str = "status_recent";
const T_INC: &str = "status_incident";
const T_MAINT: &str = "status_maint";

pub fn component_key(id: &str) -> Option<&'static str> {
    COMPONENTS.iter().find(|(c, _)| *c == id).map(|(_, k)| *k)
}

/// English display name of a component, for stored text and the machine endpoints.
pub fn component_en(id: &str) -> &'static str {
    crate::i18n::t(crate::i18n::Lang::En, component_key(id).unwrap_or("?"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    Up,
    Degraded,
    Down,
}

impl Health {
    fn code(self) -> u8 {
        match self {
            Health::Up => 0,
            Health::Degraded => 1,
            Health::Down => 2,
        }
    }
    fn from_code(c: u64) -> Health {
        match c {
            0 => Health::Up,
            1 => Health::Degraded,
            _ => Health::Down,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CheckResult {
    pub health: Health,
    /// For operators (never rendered on the public page except instance names).
    pub detail: String,
    pub pct: Option<f64>,
    /// Names of degraded customer instances.
    pub names: Vec<String>,
}

impl CheckResult {
    pub fn up() -> CheckResult {
        CheckResult {
            health: Health::Up,
            detail: String::new(),
            pct: None,
            names: Vec::new(),
        }
    }
    pub fn down(why: impl Into<String>) -> CheckResult {
        CheckResult {
            health: Health::Down,
            detail: why.into(),
            pct: None,
            names: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Day {
    pub up: u64,
    pub degraded: u64,
    pub down: u64,
}

impl Day {
    pub fn total(&self) -> u64 {
        self.up + self.degraded + self.down
    }

    /// Share of checks that found the component available; degraded counts as available.
    pub fn pct(&self) -> Option<f64> {
        (self.total() > 0).then(|| (self.up + self.degraded) as f64 * 100.0 / self.total() as f64)
    }

    pub fn state(&self) -> DayState {
        if self.total() == 0 {
            DayState::NoData
        } else if self.down * 20 >= self.total() {
            DayState::Outage
        } else if self.down > 0 || self.degraded > 0 {
            DayState::Degraded
        } else {
            DayState::Ok
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DayState {
    Ok,
    Degraded,
    Outage,
    NoData,
}

/// Uptime over several days: available checks over all checks.
pub fn uptime_pct<'a>(days: impl Iterator<Item = &'a Day>) -> Option<f64> {
    let (mut avail, mut total) = (0u64, 0u64);
    for d in days {
        avail += d.up + d.degraded;
        total += d.total();
    }
    (total > 0).then(|| avail as f64 * 100.0 / total as f64)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Impact {
    None,
    Minor,
    Major,
    Critical,
}

impl Impact {
    pub fn code(self) -> &'static str {
        ["none", "minor", "major", "critical"][self as usize]
    }
    pub fn parse(s: &str) -> Option<Impact> {
        Some(match s {
            "none" => Impact::None,
            "minor" => Impact::Minor,
            "major" => Impact::Major,
            "critical" => Impact::Critical,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncStatus {
    Investigating,
    Identified,
    Monitoring,
    Resolved,
}

impl IncStatus {
    pub fn code(self) -> &'static str {
        ["investigating", "identified", "monitoring", "resolved"][self as usize]
    }
    pub fn parse(s: &str) -> Option<IncStatus> {
        Some(match s {
            "investigating" => IncStatus::Investigating,
            "identified" => IncStatus::Identified,
            "monitoring" => IncStatus::Monitoring,
            "resolved" => IncStatus::Resolved,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Update {
    pub status: IncStatus,
    pub body: String,
    pub at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Incident {
    pub id: u64,
    pub title: String,
    pub impact: Impact,
    pub status: IncStatus,
    pub components: Vec<String>,
    pub auto: bool,
    pub created: u64,
    pub updated: u64,
    pub resolved_at: Option<u64>,
    /// Oldest first, as stored; views show newest first.
    pub updates: Vec<Update>,
}

impl Incident {
    pub fn open(&self) -> bool {
        self.status != IncStatus::Resolved
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Maintenance {
    pub id: u64,
    pub title: String,
    pub body: String,
    pub components: Vec<String>,
    pub start: u64,
    pub end: u64,
    pub created: u64,
}

impl Maintenance {
    pub fn active(&self, now: u64) -> bool {
        self.start <= now && now < self.end
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CompState {
    NoData,
    Operational,
    Degraded,
    Partial,
    Major,
    Maintenance,
}

impl CompState {
    pub fn code(self) -> &'static str {
        match self {
            CompState::NoData => "nodata",
            CompState::Operational => "operational",
            CompState::Degraded => "degraded",
            CompState::Partial => "partial",
            CompState::Major => "major",
            CompState::Maintenance => "maintenance",
        }
    }
    /// Statuspage v2 component status.
    pub fn statuspage(self) -> &'static str {
        match self {
            CompState::NoData | CompState::Operational => "operational",
            CompState::Degraded => "degraded_performance",
            CompState::Partial => "partial_outage",
            CompState::Major => "major_outage",
            CompState::Maintenance => "under_maintenance",
        }
    }
    pub fn key(self) -> String {
        format!("state.{}", self.code())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Indicator {
    None,
    Minor,
    Major,
    Critical,
}

impl Indicator {
    pub fn code(self) -> &'static str {
        ["none", "minor", "major", "critical"][self as usize]
    }
    /// The English words Statuspage clients expect.
    pub fn description(self) -> &'static str {
        match self {
            Indicator::None => "All Systems Operational",
            Indicator::Minor => "Partially Degraded Service",
            Indicator::Major => "Partial System Outage",
            Indicator::Critical => "Major System Outage",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Indicator::None => "status.all_ok",
            Indicator::Minor => "status.minor",
            Indicator::Major => "status.major",
            Indicator::Critical => "status.critical",
        }
    }
}

#[derive(Debug, Clone)]
struct Latest {
    at: u64,
    health: Health,
    detail: String,
    pct: Option<f64>,
    names: Vec<String>,
}

#[derive(Default)]
struct Inner {
    days: HashMap<String, BTreeMap<u64, Day>>,
    samples: HashMap<String, Vec<(u64, Health)>>,
    latest: HashMap<String, Latest>,
    incidents: Vec<Incident>,
    maint: Vec<Maintenance>,
    enabled: Vec<String>,
}

pub struct Status {
    backend: Arc<dyn Backend>,
    inner: Mutex<Inner>,
    ring_cap: Mutex<usize>,
    seen: AtomicU64,
    checked: Mutex<Instant>,
    sync_every: Mutex<Duration>,
}

const LOCK: &str = "community.status";
const VERSION: &str = "community.status.version";
const SYNC_EVERY: Duration = Duration::from_millis(250);

/// The status data held for writing (see the module comment).
struct StatusWrite<'a> {
    status: &'a Status,
    g: Option<MutexGuard<'a, Inner>>,
    changed: bool,
    _xl: obop_storage::LockGuard,
}

impl std::ops::Deref for StatusWrite<'_> {
    type Target = Inner;
    fn deref(&self) -> &Inner {
        self.g.as_ref().expect("held until drop")
    }
}

impl std::ops::DerefMut for StatusWrite<'_> {
    fn deref_mut(&mut self) -> &mut Inner {
        self.changed = true;
        self.g.as_mut().expect("held until drop")
    }
}

impl Drop for StatusWrite<'_> {
    fn drop(&mut self) {
        if self.changed
            && let Ok(v) = self.status.backend.counter_add(VERSION, 0, false)
        {
            self.status.seen.store(v, Ordering::Release);
        }
        self.g.take();
    }
}

#[derive(Debug, Clone)]
pub struct DayView {
    pub day: u64,
    pub state: DayState,
    pub pct: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct CompView {
    pub id: String,
    pub key: &'static str,
    pub state: CompState,
    pub u24: Option<f64>,
    pub u7: Option<f64>,
    pub u90: Option<f64>,
    pub days: Vec<DayView>,
    pub pct_up: Option<f64>,
    pub degraded_names: Vec<String>,
    pub detail: String,
    pub checked: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub now: u64,
    pub comps: Vec<CompView>,
    pub indicator: Indicator,
    pub maintenance_now: bool,
    pub active: Vec<Incident>,
    pub past: Vec<Incident>,
    pub maint: Vec<Maintenance>,
    pub last_check: Option<u64>,
    pub last_change: u64,
}

fn s(f: &Fields, k: &str) -> String {
    f.get(k).and_then(J::as_str).unwrap_or("").to_string()
}
fn n(f: &Fields, k: &str) -> u64 {
    f.get(k).and_then(J::as_u64).unwrap_or(0)
}
fn strs(f: &Fields, k: &str) -> Vec<String> {
    f.get(k)
        .and_then(J::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}
fn obj(v: J) -> Fields {
    match v {
        J::Object(m) => m,
        _ => Fields::new(),
    }
}

/// Everything the status engine holds, read from the backend.
fn load_inner(backend: &dyn Backend) -> Result<Inner, String> {
    let mut i = Inner {
        enabled: vec!["api".into(), "community".into()],
        ..Inner::default()
    };
    for (id, f) in backend.list(T_DAY)? {
        let Some((comp, ts)) = id.split_once(':') else {
            continue;
        };
        let Ok(ts) = ts.parse::<u64>() else { continue };
        i.days.entry(comp.to_string()).or_default().insert(
            ts,
            Day {
                up: n(&f, "up"),
                degraded: n(&f, "degraded"),
                down: n(&f, "down"),
            },
        );
    }
    for (comp, f) in backend.list(T_RECENT)? {
        let samples = f
            .get("samples")
            .and_then(J::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|p| {
                        let p = p.as_array()?;
                        Some((p.first()?.as_u64()?, Health::from_code(p.get(1)?.as_u64()?)))
                    })
                    .collect()
            })
            .unwrap_or_default();
        i.samples.insert(comp.clone(), samples);
        if let Some(l) = f.get("latest").and_then(J::as_object) {
            i.latest.insert(
                comp,
                Latest {
                    at: n(l, "at"),
                    health: Health::from_code(n(l, "health")),
                    detail: s(l, "detail"),
                    pct: l.get("pct").and_then(J::as_f64),
                    names: strs(l, "names"),
                },
            );
        }
    }
    for (id, f) in backend.list(T_INC)? {
        let Ok(idn) = id.parse::<u64>() else { continue };
        let updates = f
            .get("updates")
            .and_then(J::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|u| {
                        let u = u.as_object()?;
                        Some(Update {
                            status: IncStatus::parse(&s(u, "status"))?,
                            body: s(u, "body"),
                            at: n(u, "at"),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        i.incidents.push(Incident {
            id: idn,
            title: s(&f, "title"),
            impact: Impact::parse(&s(&f, "impact")).unwrap_or(Impact::None),
            status: IncStatus::parse(&s(&f, "status")).unwrap_or(IncStatus::Investigating),
            components: strs(&f, "components"),
            auto: f.get("auto").and_then(J::as_bool).unwrap_or(false),
            created: n(&f, "created"),
            updated: n(&f, "updated"),
            resolved_at: f.get("resolved_at").and_then(J::as_u64),
            updates,
        });
    }
    i.incidents.sort_by_key(|x| x.id);
    for (id, f) in backend.list(T_MAINT)? {
        let Ok(idn) = id.parse::<u64>() else { continue };
        i.maint.push(Maintenance {
            id: idn,
            title: s(&f, "title"),
            body: s(&f, "body"),
            components: strs(&f, "components"),
            start: n(&f, "start"),
            end: n(&f, "end"),
            created: n(&f, "created"),
        });
    }
    i.maint.sort_by_key(|x| x.id);
    for (c, _) in COMPONENTS {
        if (i.days.contains_key(c) || i.latest.contains_key(c)) && !i.enabled.iter().any(|e| e == c)
        {
            i.enabled.push(c.to_string());
        }
    }
    Ok(i)
}

impl Status {
    /// Load status history from `backend`.
    pub fn open(backend: Arc<dyn Backend>) -> Result<Arc<Status>, String> {
        let seen = backend.counter_get(VERSION)?;
        let i = load_inner(&*backend)?;
        Ok(Arc::new(Status {
            backend,
            inner: Mutex::new(i),
            ring_cap: Mutex::new(1500),
            seen: AtomicU64::new(seen),
            checked: Mutex::new(Instant::now()),
            sync_every: Mutex::new(SYNC_EVERY),
        }))
    }

    /// How often a read may check whether another process changed the data (tests use zero).
    pub fn set_sync_interval(&self, every: Duration) {
        *self.sync_every.lock().unwrap_or_else(|e| e.into_inner()) = every;
    }

    fn sync(&self, force: bool) {
        let every = *self.sync_every.lock().unwrap_or_else(|e| e.into_inner());
        {
            let mut at = self.checked.lock().unwrap_or_else(|e| e.into_inner());
            if !force && at.elapsed() < every {
                return;
            }
            *at = Instant::now();
        }
        let Ok(v) = self.backend.counter_get(VERSION) else {
            return;
        };
        if v == self.seen.load(Ordering::Acquire) {
            return;
        }
        if let Ok(mut fresh) = load_inner(&*self.backend) {
            let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            // Which components are shown is this process's own setting; keep it.
            for e in &g.enabled {
                if !fresh.enabled.contains(e) {
                    fresh.enabled.push(e.clone());
                }
            }
            *g = fresh;
            self.seen.store(v, Ordering::Release);
        }
    }

    /// Only one process per `interval` runs the checks: the first to ask takes the lease.
    pub fn lead_checks(&self, interval: Duration) -> bool {
        let ttl = (interval.as_millis() as u64).saturating_mul(9) / 10;
        self.backend
            .counter_add("community.status.lease", ttl.max(1), false)
            .is_ok_and(|n| n == 1)
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.sync(false);
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The data for a change: waits for the cross-process lock, brings the copy up to date.
    fn write(&self) -> Result<StatusWrite<'_>, String> {
        let xl = self.backend.lock(LOCK)?;
        self.sync(true);
        Ok(StatusWrite {
            status: self,
            g: Some(self.inner.lock().unwrap_or_else(|e| e.into_inner())),
            changed: false,
            _xl: xl,
        })
    }

    /// Choose which components are shown (in `COMPONENTS` order). Unknown ids are ignored.
    pub fn enable(&self, ids: &[&str]) {
        let mut g = self.lock();
        for id in ids {
            if component_key(id).is_some() && !g.enabled.iter().any(|e| e == id) {
                g.enabled.push((*id).to_string());
            }
        }
    }

    /// Keep enough samples to compute 24 hours at the given check interval.
    pub fn set_interval(&self, secs: u64) {
        let cap = (86_400 / secs.max(1) + 10).clamp(60, 20_000) as usize;
        *self.ring_cap.lock().unwrap_or_else(|e| e.into_inner()) = cap;
    }

    // ---------- recording ----------

    /// Record one check result: update the ring, the day counters and the automatic incident.
    pub fn record(&self, comp: &str, res: CheckResult, now: u64) -> Result<(), String> {
        if component_key(comp).is_none() {
            return Err(format!("unknown component {comp}"));
        }
        let cap = *self.ring_cap.lock().unwrap_or_else(|e| e.into_inner());
        let mut g = self.write()?;
        if !g.enabled.iter().any(|e| e == comp) {
            g.enabled.push(comp.to_string());
        }
        // Day counters.
        let ds = day_start(now);
        let day = {
            let days = g.days.entry(comp.to_string()).or_default();
            let d = days.entry(ds).or_default();
            match res.health {
                Health::Up => d.up += 1,
                Health::Degraded => d.degraded += 1,
                Health::Down => d.down += 1,
            }
            *d
        };
        put(
            &*self.backend,
            T_DAY,
            &format!("{comp}:{ds}"),
            obj(json!({"up": day.up, "degraded": day.degraded, "down": day.down})),
        )?;
        // Prune days past the retention window.
        let cutoff = ds.saturating_sub(KEEP_DAYS * 86_400);
        let old: Vec<u64> = g
            .days
            .get(comp)
            .map(|m| m.range(..cutoff).map(|(k, _)| *k).collect())
            .unwrap_or_default();
        for k in old {
            if let Some(m) = g.days.get_mut(comp) {
                m.remove(&k);
            }
            let _ = self.backend.delete(T_DAY, &format!("{comp}:{k}"));
        }
        // Ring of samples and the last result.
        let ring = g.samples.entry(comp.to_string()).or_default();
        ring.push((now, res.health));
        if ring.len() > cap {
            let drop = ring.len() - cap;
            ring.drain(..drop);
        }
        let last3: Vec<Health> = ring
            .iter()
            .rev()
            .take(AUTO_THRESHOLD)
            .map(|x| x.1)
            .collect();
        let samples_json: Vec<J> = ring.iter().map(|(t, h)| json!([t, h.code()])).collect();
        g.latest.insert(
            comp.to_string(),
            Latest {
                at: now,
                health: res.health,
                detail: res.detail.clone(),
                pct: res.pct,
                names: res.names.clone(),
            },
        );
        put(
            &*self.backend,
            T_RECENT,
            comp,
            obj(json!({
                "samples": samples_json,
                "latest": {"at": now, "health": res.health.code(), "detail": res.detail, "pct": res.pct, "names": res.names},
            })),
        )?;
        // Automatic incidents.
        let open_auto = g
            .incidents
            .iter()
            .position(|i| i.auto && i.open() && i.components.iter().any(|c| c == comp));
        if last3.len() == AUTO_THRESHOLD
            && last3.iter().all(|h| *h == Health::Down)
            && open_auto.is_none()
        {
            let title = format!("{} is unavailable", component_en(comp));
            self.create_incident_locked(
                &mut g,
                &title,
                Impact::Major,
                IncStatus::Investigating,
                vec![comp.to_string()],
                "Automated checks failed three times in a row. We are looking into it.",
                true,
                now,
            )?;
        } else if let Some(pos) = open_auto
            && last3.len() == AUTO_THRESHOLD
            && last3.iter().all(|h| *h != Health::Down)
        {
            let id = g.incidents[pos].id;
            self.update_locked(
                &mut g,
                id,
                Some(IncStatus::Resolved),
                None,
                "Automated checks have recovered.",
                now,
            )?;
        }
        Ok(())
    }

    // ---------- incidents ----------

    #[allow(clippy::too_many_arguments)]
    fn create_incident_locked(
        &self,
        g: &mut Inner,
        title: &str,
        impact: Impact,
        status: IncStatus,
        components: Vec<String>,
        body: &str,
        auto: bool,
        now: u64,
    ) -> Result<u64, String> {
        let id = g.incidents.iter().map(|i| i.id).max().unwrap_or(0) + 1;
        let inc = Incident {
            id,
            title: util::truncate_chars(&util::clean_line(title), 140),
            impact,
            status,
            components,
            auto,
            created: now,
            updated: now,
            resolved_at: (status == IncStatus::Resolved).then_some(now),
            updates: vec![Update {
                status,
                body: util::truncate_chars(body.trim(), 4000),
                at: now,
            }],
        };
        self.save_incident(&inc)?;
        g.incidents.push(inc);
        Ok(id)
    }

    fn save_incident(&self, i: &Incident) -> Result<(), String> {
        put(
            &*self.backend,
            T_INC,
            &format!("{:08}", i.id),
            obj(json!({
                "title": i.title, "impact": i.impact.code(), "status": i.status.code(),
                "components": i.components, "auto": i.auto, "created": i.created,
                "updated": i.updated, "resolved_at": i.resolved_at,
                "updates": i.updates.iter().map(|u| json!({"status": u.status.code(), "body": u.body, "at": u.at})).collect::<Vec<_>>(),
            })),
        )
    }

    fn update_locked(
        &self,
        g: &mut Inner,
        id: u64,
        status: Option<IncStatus>,
        impact: Option<Impact>,
        body: &str,
        now: u64,
    ) -> Result<(), String> {
        let inc = g
            .incidents
            .iter_mut()
            .find(|i| i.id == id)
            .ok_or("no such incident")?;
        if let Some(im) = impact {
            inc.impact = im;
        }
        let st = status.unwrap_or(inc.status);
        inc.status = st;
        inc.updated = now;
        if st == IncStatus::Resolved {
            inc.resolved_at.get_or_insert(now);
        } else {
            inc.resolved_at = None;
        }
        inc.updates.push(Update {
            status: st,
            body: util::truncate_chars(body.trim(), 4000),
            at: now,
        });
        let inc = inc.clone();
        self.save_incident(&inc)
    }

    fn valid_components(components: &[String]) -> Result<Vec<String>, String> {
        let mut out: Vec<String> = Vec::new();
        for c in components {
            if component_key(c).is_none() {
                return Err(format!("unknown component {c}"));
            }
            if !out.contains(c) {
                out.push(c.clone());
            }
        }
        Ok(out)
    }

    /// Open an incident by hand (staff form or CLI).
    pub fn create_incident(
        &self,
        title: &str,
        impact: Impact,
        status: IncStatus,
        components: &[String],
        body: &str,
        now: u64,
    ) -> Result<u64, String> {
        if util::clean_line(title).is_empty() {
            return Err("an incident needs a title".into());
        }
        let comps = Self::valid_components(components)?;
        let mut g = self.write()?;
        self.create_incident_locked(&mut g, title, impact, status, comps, body, false, now)
    }

    /// Post an update; `status`/`impact` change the incident when given.
    pub fn update_incident(
        &self,
        id: u64,
        status: Option<IncStatus>,
        impact: Option<Impact>,
        body: &str,
        now: u64,
    ) -> Result<(), String> {
        let mut g = self.write()?;
        self.update_locked(&mut g, id, status, impact, body, now)
    }

    pub fn resolve_incident(&self, id: u64, body: &str, now: u64) -> Result<(), String> {
        self.update_incident(id, Some(IncStatus::Resolved), None, body, now)
    }

    pub fn incident(&self, id: u64) -> Option<Incident> {
        self.lock().incidents.iter().find(|i| i.id == id).cloned()
    }

    /// All incidents, newest first.
    pub fn incidents(&self) -> Vec<Incident> {
        let mut v = self.lock().incidents.clone();
        v.sort_by(|a, b| b.created.cmp(&a.created).then(b.id.cmp(&a.id)));
        v
    }

    pub fn create_maintenance(
        &self,
        title: &str,
        body: &str,
        components: &[String],
        start: u64,
        end: u64,
        now: u64,
    ) -> Result<u64, String> {
        if util::clean_line(title).is_empty() || end <= start {
            return Err("a maintenance window needs a title and an end after its start".into());
        }
        let comps = Self::valid_components(components)?;
        let mut g = self.write()?;
        let id = g.maint.iter().map(|m| m.id).max().unwrap_or(0) + 1;
        let m = Maintenance {
            id,
            title: util::truncate_chars(&util::clean_line(title), 140),
            body: util::truncate_chars(body.trim(), 4000),
            components: comps,
            start,
            end,
            created: now,
        };
        put(
            &*self.backend,
            T_MAINT,
            &format!("{id:08}"),
            obj(
                json!({"title": m.title, "body": m.body, "components": m.components, "start": start, "end": end, "created": now}),
            ),
        )?;
        g.maint.push(m);
        Ok(id)
    }

    // ---------- views ----------

    fn comp_state(g: &Inner, comp: &str, now: u64) -> CompState {
        if g.maint.iter().any(|m| {
            m.active(now) && (m.components.is_empty() || m.components.iter().any(|c| c == comp))
        }) {
            return CompState::Maintenance;
        }
        let mut st = match g.latest.get(comp).map(|l| l.health) {
            Some(Health::Up) => CompState::Operational,
            Some(Health::Degraded) => CompState::Degraded,
            Some(Health::Down) => CompState::Major,
            None => CompState::NoData,
        };
        for i in g
            .incidents
            .iter()
            .filter(|i| i.open() && i.components.iter().any(|c| c == comp))
        {
            let from_incident = match i.impact {
                Impact::None => CompState::Operational,
                Impact::Minor => CompState::Degraded,
                Impact::Major => CompState::Partial,
                Impact::Critical => CompState::Major,
            };
            if st == CompState::NoData {
                st = CompState::Operational;
            }
            st = st.max(from_incident);
        }
        st
    }

    /// Everything a page or API needs, at time `now`.
    pub fn snapshot(&self, now: u64) -> Snapshot {
        let g = self.lock();
        let today = day_start(now);
        let mut comps = Vec::new();
        for (id, key) in COMPONENTS {
            if !g.enabled.iter().any(|e| e == id) {
                continue;
            }
            let days = g.days.get(id);
            let day_at = |ts: u64| days.and_then(|m| m.get(&ts)).copied().unwrap_or_default();
            let bar: Vec<DayView> = (0..BAR_DAYS)
                .rev()
                .map(|ago| {
                    let ts = today.saturating_sub(ago * 86_400);
                    let d = day_at(ts);
                    DayView {
                        day: ts,
                        state: d.state(),
                        pct: d.pct(),
                    }
                })
                .collect();
            let window = |n: u64| {
                let from = today.saturating_sub((n - 1) * 86_400);
                uptime_pct(
                    days.into_iter()
                        .flat_map(|m| m.range(from..=today).map(|(_, d)| d)),
                )
            };
            let u24 = {
                let since = now.saturating_sub(86_400);
                let (mut ok, mut tot) = (0u64, 0u64);
                for (t, h) in g.samples.get(id).into_iter().flatten() {
                    if *t > since {
                        tot += 1;
                        if *h != Health::Down {
                            ok += 1;
                        }
                    }
                }
                (tot > 0).then(|| ok as f64 * 100.0 / tot as f64)
            };
            let latest = g.latest.get(id);
            comps.push(CompView {
                id: id.to_string(),
                key,
                state: Self::comp_state(&g, id, now),
                u24,
                u7: window(7),
                u90: window(BAR_DAYS),
                days: bar,
                pct_up: latest.and_then(|l| l.pct),
                degraded_names: latest.map(|l| l.names.clone()).unwrap_or_default(),
                detail: latest.map(|l| l.detail.clone()).unwrap_or_default(),
                checked: latest.map(|l| l.at),
            });
        }
        let maintenance_now = comps.iter().any(|c| c.state == CompState::Maintenance);
        let majors = comps.iter().filter(|c| c.state == CompState::Major).count();
        let mut indicator = if !comps.is_empty() && majors * 2 >= comps.len() {
            Indicator::Critical
        } else if comps
            .iter()
            .any(|c| matches!(c.state, CompState::Major | CompState::Partial))
        {
            Indicator::Major
        } else if comps.iter().any(|c| c.state == CompState::Degraded) {
            Indicator::Minor
        } else {
            Indicator::None
        };
        // A manual incident with no component still colours the banner.
        for i in g
            .incidents
            .iter()
            .filter(|i| i.open() && i.components.is_empty())
        {
            let ind = match i.impact {
                Impact::None => Indicator::None,
                Impact::Minor => Indicator::Minor,
                Impact::Major => Indicator::Major,
                Impact::Critical => Indicator::Critical,
            };
            if (ind as usize) > (indicator as usize) {
                indicator = ind;
            }
        }
        let mut incs = g.incidents.clone();
        incs.sort_by(|a, b| b.created.cmp(&a.created).then(b.id.cmp(&a.id)));
        let active: Vec<Incident> = incs.iter().filter(|i| i.open()).cloned().collect();
        let since = now.saturating_sub(BAR_DAYS * 86_400);
        let past: Vec<Incident> = incs
            .iter()
            .filter(|i| !i.open() && i.created >= since)
            .cloned()
            .collect();
        let mut maint: Vec<Maintenance> = g.maint.iter().filter(|m| m.end > now).cloned().collect();
        maint.sort_by_key(|m| m.start);
        let last_check = g.latest.values().map(|l| l.at).max();
        let last_change = incs
            .iter()
            .map(|i| i.updated)
            .chain(g.maint.iter().map(|m| m.created))
            .chain(last_check)
            .max()
            .unwrap_or(0);
        Snapshot {
            now,
            comps,
            indicator,
            maintenance_now,
            active,
            past,
            maint,
            last_check,
            last_change,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Checks
// ---------------------------------------------------------------------------------------------

/// The little HTTP client the checks need. Tests substitute a fake.
pub trait Http: Send + Sync {
    fn get(&self, url: &str) -> Result<u16, String>;
    fn head(&self, url: &str) -> Result<u16, String>;
}

fn is_global_ip(ip: std::net::IpAddr) -> bool {
    use std::net::IpAddr;
    match ip {
        IpAddr::V4(a) => {
            let o = a.octets();
            !(a.is_private()
                || a.is_loopback()
                || a.is_link_local()
                || a.is_unspecified()
                || a.is_broadcast()
                || a.is_multicast()
                || a.is_documentation()
                || o[0] == 0
                || (o[0] == 100 && (64..=127).contains(&o[1]))
                || (o[0] == 192 && o[1] == 0 && o[2] == 0)
                || (o[0] == 198 && (18..=19).contains(&o[1]))
                || o[0] >= 240)
        }
        IpAddr::V6(a) => {
            let seg = a.segments();
            !(a.is_loopback()
                || a.is_unspecified()
                || a.is_multicast()
                || (seg[0] & 0xfe00) == 0xfc00
                || (seg[0] & 0xffc0) == 0xfe80
                || (seg[0] == 0x2001 && seg[1] == 0x0db8)
                || a.to_ipv4_mapped()
                    .is_some_and(|v4| !is_global_ip(IpAddr::V4(v4))))
        }
    }
}

/// `Some(host)` when `url` is exactly `https://<hostname>` (optional single trailing slash):
/// no userinfo, port, path, query or fragment, and a plausible DNS name.
pub fn instance_host(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://")?;
    let host = rest.strip_suffix('/').unwrap_or(rest);
    let ok = !host.is_empty()
        && host.len() <= 253
        && host.contains('.')
        && host.split('.').all(|l| {
            !l.is_empty()
                && l.len() <= 63
                && !l.starts_with('-')
                && !l.ends_with('-')
                && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
        && host.parse::<std::net::IpAddr>().is_err();
    ok.then(|| host.to_ascii_lowercase())
}

/// The real client: ureq with short timeouts. With `hardened`, only `https://` URLs of the shape
/// accepted by [`instance_host`], no redirects, and every resolved address must be a global
/// one (checked just before connecting).
pub struct UreqHttp {
    pub hardened: bool,
}

impl UreqHttp {
    fn agent(&self, secs: u64) -> ureq::Agent {
        ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(secs)))
            .max_redirects(0)
            .http_status_as_error(false)
            .https_only(self.hardened)
            .build()
            .into()
    }

    fn guard(&self, url: &str) -> Result<(), String> {
        if !self.hardened {
            return Ok(());
        }
        use std::net::ToSocketAddrs;
        let host = instance_host(url).ok_or("not a plain https host name")?;
        let addrs: Vec<_> = (host.as_str(), 443u16)
            .to_socket_addrs()
            .map_err(|e| format!("dns: {e}"))?
            .collect();
        if addrs.is_empty() || addrs.iter().any(|a| !is_global_ip(a.ip())) {
            return Err("resolves to a non-public address".into());
        }
        Ok(())
    }
}

impl Http for UreqHttp {
    fn get(&self, url: &str) -> Result<u16, String> {
        self.guard(url)?;
        self.agent(5)
            .get(url)
            .call()
            .map(|r| r.status().as_u16())
            .map_err(|e| e.to_string())
    }
    fn head(&self, url: &str) -> Result<u16, String> {
        self.guard(url)?;
        self.agent(5)
            .head(url)
            .call()
            .map(|r| r.status().as_u16())
            .map_err(|e| e.to_string())
    }
}

pub type ProbeFn = Box<dyn Fn() -> CheckResult + Send + Sync>;

pub struct Probe {
    pub component: &'static str,
    pub run: ProbeFn,
}

/// Website and docs: an internal GET of the static site; anything below 400 is up.
pub fn probe_website(url: String, http: Arc<dyn Http>) -> Probe {
    Probe {
        component: "website",
        run: Box::new(move || match http.get(&url) {
            Ok(c) if c < 400 => CheckResult::up(),
            Ok(c) => CheckResult::down(format!("HTTP {c}")),
            Err(e) => CheckResult::down(e),
        }),
    }
}

/// A write, read and delete on a scratch document: proves the storage backend answers.
fn round_trip(backend: &dyn Backend, table: &str) -> Result<(), String> {
    let mut f = Fields::new();
    f.insert("t".into(), J::from(util::now()));
    put(backend, table, "probe", f)?;
    backend
        .get(table, "probe")?
        .ok_or("probe document missing")?;
    backend.delete(table, "probe")?;
    Ok(())
}

pub fn probe_storage(
    component: &'static str,
    backend: Arc<dyn Backend>,
    table: &'static str,
) -> Probe {
    Probe {
        component,
        run: Box::new(move || match round_trip(&*backend, table) {
            Ok(()) => CheckResult::up(),
            Err(e) => CheckResult::down(e),
        }),
    }
}

/// Provisioning: the Sliplane CLI answers `--version`; local and fake provisioners are always up.
pub fn probe_provisioning(
    kind: String,
    run_cli: Arc<dyn Fn() -> Result<(), String> + Send + Sync>,
) -> Probe {
    Probe {
        component: "provisioning",
        run: Box::new(move || {
            if kind != "sliplane" {
                return CheckResult::up();
            }
            match run_cli() {
                Ok(()) => CheckResult::up(),
                Err(e) => CheckResult::down(e),
            }
        }),
    }
}

/// Run `sliplane --version` with a 5 s limit.
pub fn sliplane_version() -> Result<(), String> {
    let mut child = std::process::Command::new("sliplane")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("cannot run the sliplane CLI: {e}"))?;
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(st)) if st.success() => return Ok(()),
            Ok(Some(st)) => return Err(format!("sliplane exited with {st}")),
            Ok(None) if start.elapsed() > Duration::from_secs(5) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("sliplane --version timed out".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(e.to_string()),
        }
    }
}

/// Cell database: `SELECT 1` over the admin URL.
pub fn probe_celldb(url: String) -> Probe {
    Probe {
        component: "celldb",
        run: Box::new(move || {
            let r = (|| -> Result<(), String> {
                let mut conn = obop_storage::tls::PgConnector::new(&url)?;
                conn.config_mut().connect_timeout(Duration::from_secs(5));
                let mut c = conn.connect()?;
                c.simple_query("SELECT 1").map_err(|e| e.to_string())?;
                Ok(())
            })();
            match r {
                Ok(()) => CheckResult::up(),
                Err(e) => CheckResult::down(e),
            }
        }),
    }
}

/// Customer instances: HEAD / on every `running` instance's stored https URL, in parallel
/// (at most 8 at a time, 200 per round). Up when all answer; degraded when some do not; down
/// when most of at least three do not. Only instance names are reported.
pub fn probe_instances(platform_backend: Arc<dyn Backend>, http: Arc<dyn Http>) -> Probe {
    Probe {
        component: "instances",
        run: Box::new(move || {
            let list = match platform_backend.list("platform_instance") {
                Ok(l) => l,
                Err(e) => return CheckResult::down(e),
            };
            let targets: Vec<(String, String)> = list
                .iter()
                .filter(|(_, f)| s(f, "status") == "running")
                .map(|(_, f)| (s(f, "name"), s(f, "url")))
                .take(200)
                .collect();
            if targets.is_empty() {
                return CheckResult {
                    detail: "no running instances".into(),
                    pct: Some(100.0),
                    ..CheckResult::up()
                };
            }
            let results: Vec<(String, bool)> = targets
                .chunks(8)
                .flat_map(|chunk| {
                    std::thread::scope(|sc| {
                        let hs: Vec<_> = chunk
                            .iter()
                            .map(|(name, url)| {
                                let http = http.clone();
                                sc.spawn(move || {
                                    let ok = instance_host(url).is_some()
                                        && http.head(url).is_ok_and(|c| c < 500);
                                    (name.clone(), ok)
                                })
                            })
                            .collect();
                        hs.into_iter()
                            .map(|h| h.join().unwrap_or_else(|_| (String::new(), false)))
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            let total = results.len();
            let bad: Vec<String> = results
                .iter()
                .filter(|(_, ok)| !ok)
                .map(|(n, _)| n.clone())
                .collect();
            let pct = (total - bad.len()) as f64 * 100.0 / total as f64;
            let health = if bad.is_empty() {
                Health::Up
            } else if total >= 3 && bad.len() * 2 > total {
                Health::Down
            } else {
                Health::Degraded
            };
            CheckResult {
                health,
                detail: format!("{} of {total} not responding", bad.len()),
                pct: Some(pct),
                names: bad,
            }
        }),
    }
}

/// Run every probe once and record the results.
pub fn run_checks(status: &Status, probes: &[Probe], now: u64) {
    for p in probes {
        let res = (p.run)();
        if let Err(e) = status.record(p.component, res, now) {
            eprintln!("status: cannot record {}: {e}", p.component);
        }
    }
}

/// Start the background checker thread. Checks run once immediately, then every `interval`.
pub fn spawn_checker(
    status: Arc<Status>,
    probes: Vec<Probe>,
    interval: Duration,
) -> std::thread::JoinHandle<()> {
    status.set_interval(interval.as_secs());
    let ids: Vec<&str> = probes.iter().map(|p| p.component).collect();
    status.enable(&ids);
    std::thread::spawn(move || {
        loop {
            // With several processes on one database, one of them takes each round.
            if status.lead_checks(interval) {
                run_checks(&status, &probes, util::now());
            }
            std::thread::sleep(interval);
        }
    })
}

#[cfg(test)]
pub(crate) fn test_status() -> Arc<Status> {
    Status::open(obop_storage::open("memory:").unwrap()).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    const T0: u64 = 1_800_000_000 - 1_800_000_000 % 86_400 + 3600;

    #[test]
    fn day_math() {
        let d = Day {
            up: 97,
            degraded: 2,
            down: 1,
        };
        assert_eq!(d.total(), 100);
        assert!((d.pct().unwrap() - 99.0).abs() < 1e-9);
        assert_eq!(d.state(), DayState::Degraded);
        assert_eq!(
            Day {
                up: 90,
                degraded: 0,
                down: 10
            }
            .state(),
            DayState::Outage
        );
        assert_eq!(
            Day {
                up: 5,
                ..Day::default()
            }
            .state(),
            DayState::Ok
        );
        assert_eq!(Day::default().state(), DayState::NoData);
        assert_eq!(Day::default().pct(), None);
        let ds = [
            Day {
                up: 10,
                ..Day::default()
            },
            Day {
                up: 5,
                degraded: 0,
                down: 5,
            },
        ];
        assert!((uptime_pct(ds.iter()).unwrap() - 75.0).abs() < 1e-9);
        assert_eq!(uptime_pct([].iter()), None);
    }

    #[test]
    fn records_per_day_and_windows() {
        let st = test_status();
        // Three days ago: 2 down out of 4. Today: all up.
        let three_ago = T0 - 3 * 86_400;
        for (i, h) in [Health::Up, Health::Down, Health::Up, Health::Down]
            .into_iter()
            .enumerate()
        {
            let r = CheckResult {
                health: h,
                ..CheckResult::up()
            };
            st.record("api", r, three_ago + i as u64 * 60).unwrap();
        }
        for i in 0..4 {
            st.record("api", CheckResult::up(), T0 + i * 60).unwrap();
        }
        let snap = st.snapshot(T0 + 300);
        let api = snap.comps.iter().find(|c| c.id == "api").unwrap();
        assert_eq!(api.days.len(), 90);
        assert_eq!(api.days.last().unwrap().state, DayState::Ok);
        assert_eq!(api.days[89 - 3].state, DayState::Outage);
        assert!((api.u7.unwrap() - 75.0).abs() < 1e-9, "{:?}", api.u7);
        assert!((api.u90.unwrap() - 75.0).abs() < 1e-9);
        assert_eq!(api.u24.unwrap(), 100.0, "only the last 24 hours");
        assert_eq!(api.state, CompState::Operational);
    }

    #[test]
    fn retention_drops_old_days() {
        let st = test_status();
        st.record("api", CheckResult::up(), T0 - 150 * 86_400)
            .unwrap();
        st.record("api", CheckResult::up(), T0).unwrap();
        assert_eq!(st.lock().days["api"].len(), 1);
        assert!(
            st.backend
                .get(T_DAY, &format!("api:{}", day_start(T0 - 150 * 86_400)))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn automatic_incident_opens_after_three_failures_and_resolves_after_three_successes() {
        let st = test_status();
        let down = || CheckResult::down("boom");
        st.record("community", down(), T0).unwrap();
        st.record("community", down(), T0 + 60).unwrap();
        assert!(st.incidents().is_empty(), "two failures are not enough");
        st.record("community", CheckResult::up(), T0 + 120).unwrap();
        st.record("community", down(), T0 + 180).unwrap();
        st.record("community", down(), T0 + 240).unwrap();
        assert!(
            st.incidents().is_empty(),
            "the pass in between reset the streak"
        );
        st.record("community", down(), T0 + 300).unwrap();
        let incs = st.incidents();
        assert_eq!(incs.len(), 1);
        assert!(incs[0].auto && incs[0].open());
        assert_eq!(incs[0].components, ["community"]);
        assert_eq!(incs[0].title, "Community is unavailable");
        // Staying down does not open a second one.
        st.record("community", down(), T0 + 360).unwrap();
        assert_eq!(st.incidents().len(), 1);
        assert_eq!(
            st.snapshot(T0 + 361)
                .comps
                .iter()
                .find(|c| c.id == "community")
                .unwrap()
                .state,
            CompState::Major
        );
        // Two successes are not enough; the third resolves it.
        st.record("community", CheckResult::up(), T0 + 420).unwrap();
        st.record("community", CheckResult::up(), T0 + 480).unwrap();
        assert!(st.incidents()[0].open());
        st.record("community", CheckResult::up(), T0 + 540).unwrap();
        let i = &st.incidents()[0];
        assert_eq!(
            (i.status, i.resolved_at),
            (IncStatus::Resolved, Some(T0 + 540))
        );
        assert_eq!(i.updates.len(), 2);
        assert_eq!(
            st.snapshot(T0 + 600)
                .comps
                .iter()
                .find(|c| c.id == "community")
                .unwrap()
                .state,
            CompState::Operational
        );
        // A new outage opens a new incident.
        for k in 0..3 {
            st.record("community", down(), T0 + 700 + k).unwrap();
        }
        assert_eq!(st.incidents().len(), 2);
    }

    #[test]
    fn manual_incident_lifecycle_and_component_state() {
        let st = test_status();
        st.record("api", CheckResult::up(), T0).unwrap();
        assert!(
            st.create_incident("", Impact::Minor, IncStatus::Investigating, &[], "", T0)
                .is_err()
        );
        assert!(
            st.create_incident(
                "x",
                Impact::Minor,
                IncStatus::Investigating,
                &["nope".into()],
                "",
                T0
            )
            .is_err()
        );
        let id = st
            .create_incident(
                "Slow API",
                Impact::Major,
                IncStatus::Investigating,
                &["api".into(), "api".into()],
                "Looking",
                T0 + 10,
            )
            .unwrap();
        let snap = st.snapshot(T0 + 20);
        assert_eq!(
            snap.comps.iter().find(|c| c.id == "api").unwrap().state,
            CompState::Partial
        );
        assert_eq!(snap.indicator, Indicator::Major);
        assert_eq!(snap.active.len(), 1);
        st.update_incident(
            id,
            Some(IncStatus::Identified),
            Some(Impact::Minor),
            "Found it",
            T0 + 30,
        )
        .unwrap();
        let snap = st.snapshot(T0 + 40);
        assert_eq!(
            snap.comps.iter().find(|c| c.id == "api").unwrap().state,
            CompState::Degraded
        );
        assert_eq!(snap.indicator, Indicator::Minor);
        st.resolve_incident(id, "Fixed", T0 + 50).unwrap();
        let snap = st.snapshot(T0 + 60);
        assert!(snap.active.is_empty());
        assert_eq!(snap.past.len(), 1);
        assert_eq!(snap.past[0].updates.len(), 3);
        assert_eq!(snap.indicator, Indicator::None);
        assert!(st.resolve_incident(999, "", T0).is_err());
    }

    #[test]
    fn maintenance_overrides_component_state_in_its_window() {
        let st = test_status();
        st.record("api", CheckResult::up(), T0).unwrap();
        assert!(
            st.create_maintenance("x", "", &[], T0 + 100, T0 + 50, T0)
                .is_err()
        );
        st.create_maintenance(
            "DB upgrade",
            "Short",
            &["api".into()],
            T0 + 100,
            T0 + 200,
            T0,
        )
        .unwrap();
        let before = st.snapshot(T0 + 50);
        assert_eq!(before.comps[0].state, CompState::Operational);
        assert_eq!(before.maint.len(), 1);
        let during = st.snapshot(T0 + 150);
        assert_eq!(during.comps[0].state, CompState::Maintenance);
        assert!(during.maintenance_now);
        assert_eq!(st.snapshot(T0 + 250).maint.len(), 0);
    }

    #[test]
    fn overall_indicator_rules() {
        let st = test_status();
        st.record("api", CheckResult::up(), T0).unwrap();
        st.record("community", CheckResult::up(), T0).unwrap();
        st.record("website", CheckResult::up(), T0).unwrap();
        st.record("provisioning", CheckResult::up(), T0).unwrap();
        assert_eq!(st.snapshot(T0).indicator, Indicator::None);
        st.record(
            "website",
            CheckResult {
                health: Health::Degraded,
                ..CheckResult::up()
            },
            T0 + 1,
        )
        .unwrap();
        assert_eq!(st.snapshot(T0 + 1).indicator, Indicator::Minor);
        st.record("website", CheckResult::down("x"), T0 + 2)
            .unwrap();
        assert_eq!(st.snapshot(T0 + 2).indicator, Indicator::Major);
        st.record("api", CheckResult::down("x"), T0 + 2).unwrap();
        assert_eq!(
            st.snapshot(T0 + 3).indicator,
            Indicator::Critical,
            "half of the components are down"
        );
    }

    #[test]
    fn state_survives_reopening() {
        let backend = obop_storage::open("memory:").unwrap();
        let st = Status::open(backend.clone()).unwrap();
        st.record("celldb", CheckResult::down("x"), T0).unwrap();
        let id = st
            .create_incident(
                "A",
                Impact::Minor,
                IncStatus::Monitoring,
                &["api".into()],
                "b",
                T0,
            )
            .unwrap();
        st.create_maintenance("M", "b", &[], T0 + 10, T0 + 20, T0)
            .unwrap();
        let st2 = Status::open(backend).unwrap();
        assert_eq!(st2.incident(id).unwrap().title, "A");
        let snap = st2.snapshot(T0 + 1);
        assert!(
            snap.comps.iter().any(|c| c.id == "celldb"),
            "components with data stay enabled"
        );
        assert_eq!(snap.maint.len(), 1);
        assert_eq!(
            st2.record("nope", CheckResult::up(), T0),
            Err("unknown component nope".to_string())
        );
    }

    struct FakeHttp {
        down: Vec<&'static str>,
        calls: Mutex<Vec<String>>,
    }
    impl Http for FakeHttp {
        fn get(&self, url: &str) -> Result<u16, String> {
            self.head(url)
        }
        fn head(&self, url: &str) -> Result<u16, String> {
            self.calls.lock().unwrap().push(url.to_string());
            if self.down.iter().any(|d| url.contains(d)) {
                Err("timeout".into())
            } else {
                Ok(200)
            }
        }
    }

    fn instance(b: &dyn Backend, id: &str, name: &str, url: &str, status: &str) {
        let mut f = Fields::new();
        for (k, v) in [("name", name), ("url", url), ("status", status)] {
            f.insert(k.into(), J::from(v));
        }
        b.insert("platform_instance", id, &f).unwrap();
    }

    #[test]
    fn instance_probe_lists_degraded_names_only() {
        let b = obop_storage::open("memory:").unwrap();
        instance(&*b, "1", "Alpha", "https://alpha.example.test", "running");
        instance(&*b, "2", "Beta", "https://beta.example.test/", "running");
        instance(&*b, "3", "Gamma", "https://gamma.example.test", "running");
        instance(
            &*b,
            "4",
            "Down-for-good",
            "https://deleted.example.test",
            "deleted",
        );
        instance(
            &*b,
            "5",
            "Booting",
            "https://booting.example.test",
            "provisioning",
        );
        instance(&*b, "6", "Evil", "http://169.254.169.254/", "running");
        instance(
            &*b,
            "7",
            "Evil2",
            "https://user:pw@evil.example.test",
            "running",
        );
        let http = Arc::new(FakeHttp {
            down: vec!["beta"],
            calls: Mutex::new(vec![]),
        });
        let r = (probe_instances(b, http.clone()).run)();
        let mut names = r.names.clone();
        names.sort();
        assert_eq!(
            names,
            ["Beta", "Evil", "Evil2"],
            "unsafe URLs are never fetched and count as down"
        );
        assert_eq!(r.health, Health::Down, "3 of 5 down");
        assert!((r.pct.unwrap() - 40.0).abs() < 1e-9);
        let calls = http.calls.lock().unwrap();
        assert!(
            calls
                .iter()
                .all(|u| u.starts_with("https://") && !u.contains("169.254") && !u.contains('@')),
            "{calls:?}"
        );
        assert!(
            !calls
                .iter()
                .any(|u| u.contains("deleted") || u.contains("booting"))
        );
    }

    #[test]
    fn instance_probe_no_instances_is_up_and_one_failure_is_degraded() {
        let b = obop_storage::open("memory:").unwrap();
        let http = Arc::new(FakeHttp {
            down: vec!["alpha"],
            calls: Mutex::new(vec![]),
        });
        assert_eq!(
            (probe_instances(b.clone(), http.clone()).run)().health,
            Health::Up
        );
        instance(&*b, "1", "Alpha", "https://alpha.example.test", "running");
        instance(&*b, "2", "Beta", "https://beta.example.test", "running");
        let r = (probe_instances(b, http).run)();
        assert_eq!(
            (r.health, r.names),
            (Health::Degraded, vec!["Alpha".to_string()])
        );
    }

    #[test]
    fn instance_urls_must_be_plain_https_hosts() {
        assert_eq!(
            instance_host("https://acme.sliplane.app").as_deref(),
            Some("acme.sliplane.app")
        );
        assert_eq!(
            instance_host("https://ACME.sliplane.app/").as_deref(),
            Some("acme.sliplane.app")
        );
        for bad in [
            "http://acme.sliplane.app",
            "https://acme.sliplane.app/path",
            "https://u@acme.sliplane.app",
            "https://acme.sliplane.app:8443",
            "https://127.0.0.1",
            "https://[::1]",
            "https://localhost",
            "https://-bad.example.test",
            "https://a..b.test",
            "",
            "https://",
            "ftp://x.example.test",
            "https://acme.sliplane.app?x=1",
            "https://acme.sliplane.app#f",
        ] {
            assert_eq!(instance_host(bad), None, "{bad}");
        }
    }

    #[test]
    fn private_addresses_are_not_global() {
        for ip in [
            "10.1.2.3",
            "127.0.0.1",
            "169.254.169.254",
            "192.168.0.1",
            "172.16.5.5",
            "100.64.0.1",
            "0.0.0.0",
            "::1",
            "fc00::1",
            "fe80::1",
            "::ffff:10.0.0.1",
        ] {
            assert!(!is_global_ip(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["8.8.8.8", "1.1.1.1", "2606:4700:4700::1111"] {
            assert!(is_global_ip(ip.parse().unwrap()), "{ip}");
        }
        assert!(
            UreqHttp { hardened: true }
                .guard("http://example.test")
                .is_err()
        );
        assert!(
            UreqHttp { hardened: true }
                .guard("https://127.0.0.1")
                .is_err()
        );
    }

    #[test]
    fn website_and_provisioning_probes() {
        let ok = Arc::new(FakeHttp {
            down: vec![],
            calls: Mutex::new(vec![]),
        });
        assert_eq!(
            (probe_website("http://127.0.0.1:1/".into(), ok).run)().health,
            Health::Up
        );
        let bad = Arc::new(FakeHttp {
            down: vec!["127"],
            calls: Mutex::new(vec![]),
        });
        assert_eq!(
            (probe_website("http://127.0.0.1:1/".into(), bad).run)().health,
            Health::Down
        );
        let called = Arc::new(AtomicBool::new(false));
        let c = called.clone();
        let cli: Arc<dyn Fn() -> Result<(), String> + Send + Sync> = Arc::new(move || {
            c.store(true, Ordering::SeqCst);
            Err("missing".into())
        });
        assert_eq!(
            (probe_provisioning("local".into(), cli.clone()).run)().health,
            Health::Up
        );
        assert!(
            !called.load(Ordering::SeqCst),
            "local provisioner never runs the CLI"
        );
        assert_eq!(
            (probe_provisioning("sliplane".into(), cli).run)().health,
            Health::Down
        );
        let b = obop_storage::open("memory:").unwrap();
        assert_eq!(
            (probe_storage("api", b, "status_probe").run)().health,
            Health::Up
        );
    }

    #[test]
    fn run_checks_records_each_probe() {
        let st = test_status();
        let b = obop_storage::open("memory:").unwrap();
        let probes = vec![
            probe_storage("api", b.clone(), "status_probe"),
            probe_storage("community", b, "status_probe"),
        ];
        run_checks(&st, &probes, T0);
        assert!(
            st.snapshot(T0)
                .comps
                .iter()
                .all(|c| c.state == CompState::Operational)
        );
    }
}
