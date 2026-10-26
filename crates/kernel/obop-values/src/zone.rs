//! Time zones for the `datetime` kind (D72): a pinned IANA database, the platform zone, and the
//! cutting of instants into days, weeks, months and years in a declared zone.
//!
//! The database is the one bundled in the binary (`jiff-tzdb`, a compiled copy of the IANA Time
//! Zone Database); the system's own zoneinfo is never read, so every instance of one build
//! answers the same, and a new database arrives with a new release. Its version string
//! (`2026e`) is reported by `/api/meta`. Tests can swap the source for a fixture
//! ([`ZoneDb::fixture`]) that redefines named zones with POSIX rules.

use crate::datetime::{Bucket, Instant, bucket_label};
use jiff::tz::{TimeZone, TimeZoneDatabase};
use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

/// The version of the database bundled in this build.
pub fn bundled_version() -> &'static str {
    jiff_tzdb::VERSION.unwrap_or("unknown")
}

/// The zone used when the instance does not set one. A deployment names its own with
/// `OBOP_ZONE` (any IANA name).
pub const DEFAULT_ZONE: &str = "UTC";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZoneError(pub String);

impl fmt::Display for ZoneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ZoneError {}

enum Source {
    Bundled(TimeZoneDatabase),
    /// Named rules first, the bundled database for every other name.
    Fixture {
        zones: BTreeMap<String, TimeZone>,
        rest: TimeZoneDatabase,
    },
}

/// A time zone database with a version.
pub struct ZoneDb {
    source: Source,
    version: String,
}

impl fmt::Debug for ZoneDb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ZoneDb({})", self.version)
    }
}

impl ZoneDb {
    /// The database bundled in this build.
    pub fn bundled() -> Self {
        Self {
            source: Source::Bundled(TimeZoneDatabase::bundled()),
            version: bundled_version().to_string(),
        }
    }

    /// A database for tests: each `(name, posix_rule)` redefines that zone with a POSIX TZ rule
    /// such as `CET-1CEST,M3.5.0,M10.5.0/3`, which applies to every year; other names come from
    /// the bundled database. `version` is what the fixture reports.
    pub fn fixture(version: &str, zones: &[(&str, &str)]) -> Result<Self, ZoneError> {
        let mut map = BTreeMap::new();
        for (name, rule) in zones {
            let tz = TimeZone::posix(rule)
                .map_err(|e| ZoneError(format!("fixture rule {rule:?} for {name}: {e}")))?;
            map.insert((*name).to_string(), tz);
        }
        Ok(Self {
            source: Source::Fixture {
                zones: map,
                rest: TimeZoneDatabase::bundled(),
            },
            version: version.to_string(),
        })
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    /// The zone with an IANA name (matched without regard to case; the zone reports the
    /// canonical spelling).
    pub fn zone(&self, name: &str) -> Result<Zone, ZoneError> {
        let bad = || {
            ZoneError(format!(
                "{name:?} is not a time zone of the IANA database {} (use a name such as Europe/Berlin or UTC)",
                self.version
            ))
        };
        if name.is_empty()
            || name.len() > 64
            || !name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_-+/".contains(&c))
        {
            return Err(bad());
        }
        if let Source::Fixture { zones, .. } = &self.source
            && let Some((canon, tz)) = zones.iter().find(|(k, _)| k.eq_ignore_ascii_case(name))
        {
            return Ok(Zone {
                name: canon.clone(),
                tz: tz.clone(),
            });
        }
        let db = match &self.source {
            Source::Bundled(db) => db,
            Source::Fixture { rest, .. } => rest,
        };
        let tz = db.get(name).map_err(|_| bad())?;
        let canon = tz.iana_name().ok_or_else(bad)?.to_string();
        Ok(Zone { name: canon, tz })
    }
}

/// One zone: a name and its rules.
#[derive(Clone)]
pub struct Zone {
    name: String,
    tz: TimeZone,
}

impl fmt::Debug for Zone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Zone({})", self.name)
    }
}

impl Zone {
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Seconds the local clock is ahead of UTC at this instant.
    pub fn offset_secs(&self, at: Instant) -> i32 {
        // jiff's timestamps end a little before the last day of year 9999 (at a whole second):
        // an instant beyond them has the offset of the last one.
        let us = at.micros().clamp(jiff_min_micros(), jiff_max_micros());
        let ts = jiff::Timestamp::from_microsecond(us).unwrap_or(if us < 0 {
            jiff::Timestamp::MIN
        } else {
            jiff::Timestamp::MAX
        });
        self.tz.to_offset(ts).seconds()
    }

    /// The calendar date on the wall clock of this zone. An instant within a day of either end of
    /// the range can have a local date outside years 0001 to 9999; that date is held at the
    /// first or last day, so every label has four digits and can be used in a filter.
    pub fn date_of(&self, at: Instant) -> (i64, i64, i64) {
        let (y, m, d) = at.civil_at(self.offset_secs(at));
        if y < 1 {
            (1, 1, 1)
        } else if y > 9999 {
            (9999, 12, 31)
        } else {
            (y, m, d)
        }
    }

    /// The label of the period this instant falls in on the wall clock of this zone.
    pub fn bucket(&self, at: Instant, b: Bucket) -> String {
        let (y, m, d) = self.date_of(at);
        bucket_label(y, m, d, b)
    }

    /// `YYYY-MM-DD` of this instant in this zone: the explicit "date in zone" conversion.
    pub fn date_text(&self, at: Instant) -> String {
        self.bucket(at, Bucket::Day)
    }
}

fn jiff_min_micros() -> i64 {
    jiff::Timestamp::MIN.as_second() * 1_000_000
}

fn jiff_max_micros() -> i64 {
    jiff::Timestamp::MAX.as_second() * 1_000_000
}

/// The time settings of an instance: its database and its platform zone.
#[derive(Debug, Clone)]
pub struct Zones {
    db: Arc<ZoneDb>,
    platform: Zone,
}

impl Zones {
    pub fn new(db: Arc<ZoneDb>, platform_zone: &str) -> Result<Self, ZoneError> {
        let platform = db.zone(platform_zone)?;
        Ok(Self { db, platform })
    }

    /// The bundled database and the zone named by `OBOP_ZONE` (default UTC). An unknown name is
    /// an error, so a typo is found when the server starts and not when somebody reads a date.
    pub fn from_env() -> Result<Self, ZoneError> {
        let name = std::env::var("OBOP_ZONE").unwrap_or_else(|_| DEFAULT_ZONE.to_string());
        let name = if name.trim().is_empty() {
            DEFAULT_ZONE
        } else {
            name.trim()
        };
        Self::new(Arc::new(ZoneDb::bundled()), name)
            .map_err(|e| ZoneError(format!("OBOP_ZONE: {e}")))
    }

    pub fn db(&self) -> &Arc<ZoneDb> {
        &self.db
    }

    /// The zone of `today`, and of every bucket that does not name its own.
    pub fn platform(&self) -> &Zone {
        &self.platform
    }

    /// The zone named, or the platform zone.
    pub fn zone(&self, name: Option<&str>) -> Result<Zone, ZoneError> {
        match name {
            None => Ok(self.platform.clone()),
            Some(n) => self.db.zone(n),
        }
    }

    /// `YYYY-MM-DD` today in the platform zone.
    pub fn today(&self, now: Instant) -> String {
        self.platform.date_text(now)
    }
}

impl Default for Zones {
    fn default() -> Self {
        Self::new(Arc::new(ZoneDb::bundled()), DEFAULT_ZONE).expect("UTC is always there")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inst(s: &str) -> Instant {
        Instant::parse(s).unwrap()
    }

    fn berlin() -> Zone {
        ZoneDb::bundled().zone("Europe/Berlin").unwrap()
    }

    #[test]
    fn the_bundled_database_has_a_version() {
        let v = ZoneDb::bundled();
        assert!(v.version().len() >= 5, "{}", v.version());
        assert!(v.version().starts_with("20"), "{}", v.version());
    }

    #[test]
    fn month_buckets_in_berlin_follow_the_wall_clock() {
        let z = berlin();
        // 22:30Z is 00:30 on April 1 in summer time.
        assert_eq!(
            z.bucket(inst("2026-03-31T22:30:00Z"), Bucket::Month),
            "2026-04"
        );
        // 23:30Z is 00:30 on November 1 in winter time.
        assert_eq!(
            z.bucket(inst("2026-10-31T23:30:00Z"), Bucket::Month),
            "2026-11"
        );
        // Before summer time starts (2026-03-29 01:00Z): still March.
        assert_eq!(
            z.bucket(inst("2026-03-28T23:30:00Z"), Bucket::Month),
            "2026-03"
        );
        assert_eq!(
            z.bucket(inst("2026-03-28T23:30:00Z"), Bucket::Day),
            "2026-03-29"
        );
        // Summer time: 22:30Z on the 29th is already the 30th.
        assert_eq!(
            z.bucket(inst("2026-03-29T22:30:00Z"), Bucket::Day),
            "2026-03-30"
        );
        // Sunday 23:30 local belongs to the week that ends that Sunday (ISO week 43 of 2026).
        let sunday = inst("2026-10-25T22:30:00Z"); // 23:30 CET, the day winter time began
        assert_eq!(z.bucket(sunday, Bucket::Day), "2026-10-25");
        assert_eq!(z.bucket(sunday, Bucket::IsoWeek), "2026-W43");
        assert_eq!(z.bucket(sunday, Bucket::Week), "2026-10-19");
        // The same instant is Monday in a zone far enough east.
        let tokyo = ZoneDb::bundled().zone("asia/tokyo").unwrap();
        assert_eq!(tokyo.name(), "Asia/Tokyo");
        assert_eq!(tokyo.bucket(sunday, Bucket::Day), "2026-10-26");
        assert_eq!(tokyo.bucket(sunday, Bucket::IsoWeek), "2026-W44");
        assert_eq!(z.bucket(inst("2026-12-31T23:30:00Z"), Bucket::Year), "2027");
    }

    #[test]
    fn the_ambiguous_and_missing_hours_are_distinct_instants_with_the_right_labels() {
        let z = berlin();
        // 2026-10-25: clocks go back from 03:00 to 02:00, so local 02:30 happens twice.
        let (a, b) = (inst("2026-10-25T00:30:00Z"), inst("2026-10-25T01:30:00Z"));
        assert_eq!(z.offset_secs(a), 7200);
        assert_eq!(z.offset_secs(b), 3600);
        assert_eq!(z.bucket(a, Bucket::Day), z.bucket(b, Bucket::Day));
        assert_ne!(a, b);
        // 2026-03-29: 01:30Z is the first moment after the missing hour (03:30 local).
        assert_eq!(z.offset_secs(inst("2026-03-29T00:59:59Z")), 3600);
        assert_eq!(z.offset_secs(inst("2026-03-29T01:00:00Z")), 7200);
    }

    #[test]
    fn a_fixture_database_moves_a_bucket() {
        // Summer time begins on the last Sunday of March in the real rules, which in 2026 is the
        // 29th. This fixture starts it on the last Monday, one day later, with the same end.
        let fx = ZoneDb::fixture(
            "2026x-test",
            &[("Europe/Berlin", "CET-1CEST,M3.5.1,M10.5.0/3")],
        )
        .unwrap();
        assert_eq!(fx.version(), "2026x-test");
        let moved = fx.zone("Europe/Berlin").unwrap();
        let real = berlin();
        let at = inst("2026-03-29T22:30:00Z");
        assert_eq!(real.bucket(at, Bucket::Day), "2026-03-30");
        assert_eq!(moved.bucket(at, Bucket::Day), "2026-03-29");
        // A rule that starts summer time on the first Sunday of April moves a month bucket.
        let april = ZoneDb::fixture("t", &[("Europe/Berlin", "CET-1CEST,M4.1.0,M10.5.0/3")])
            .unwrap()
            .zone("europe/berlin")
            .unwrap();
        let at = inst("2026-03-31T22:30:00Z");
        assert_eq!(real.bucket(at, Bucket::Month), "2026-04");
        assert_eq!(april.bucket(at, Bucket::Month), "2026-03");
        // Other names still come from the bundled database.
        assert_eq!(fx.zone("UTC").unwrap().bucket(at, Bucket::Month), "2026-03");
        assert!(ZoneDb::fixture("t", &[("X", "not a rule")]).is_err());
    }

    #[test]
    fn zone_names_are_validated() {
        let db = ZoneDb::bundled();
        for ok in [
            "UTC",
            "utc",
            "Europe/Berlin",
            "America/Argentina/Buenos_Aires",
            "Etc/GMT+5",
        ] {
            assert!(db.zone(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "Mars/Olympus",
            "Europe/",
            "../etc/passwd",
            "UTC\0",
            "Europe/Berlin ",
            "EST5EDT; drop",
            &"A".repeat(65),
        ] {
            let e = db.zone(bad).unwrap_err().to_string();
            assert!(e.contains("not a time zone"), "{bad:?} -> {e}");
        }
        assert_eq!(db.zone("utc").unwrap().name(), "UTC");
    }

    #[test]
    fn the_platform_zone_comes_from_the_settings() {
        let z = Zones::new(Arc::new(ZoneDb::bundled()), "Europe/Berlin").unwrap();
        assert_eq!(z.today(inst("2026-03-31T22:30:00Z")), "2026-04-01");
        assert_eq!(
            Zones::default().today(inst("2026-03-31T22:30:00Z")),
            "2026-03-31"
        );
        assert!(Zones::new(Arc::new(ZoneDb::bundled()), "Nowhere/Land").is_err());
        assert_eq!(z.zone(Some("UTC")).unwrap().name(), "UTC");
        assert_eq!(z.zone(None).unwrap().name(), "Europe/Berlin");
    }

    #[test]
    fn the_ends_of_the_range_have_the_offsets_and_labels_of_their_zone() {
        let db = ZoneDb::bundled();
        let (la, kolkata, utc) = (
            db.zone("America/Los_Angeles").unwrap(),
            db.zone("Asia/Kolkata").unwrap(),
            db.zone("UTC").unwrap(),
        );
        let last = inst("9999-12-31T23:59:59.999999Z");
        // Winter in Los Angeles, a fixed +05:30 in Kolkata, also in the last day of the range
        // (the clamp used to turn all of these into UTC).
        assert_eq!(la.offset_secs(last), -8 * 3600);
        assert_eq!(kolkata.offset_secs(last), 5 * 3600 + 1800);
        assert_eq!(utc.offset_secs(last), 0);
        let at = inst("9999-12-31T03:00:00Z");
        assert_eq!(la.bucket(at, Bucket::Day), "9999-12-30");
        assert_eq!(kolkata.bucket(at, Bucket::Day), "9999-12-31");
        assert_eq!(la.offset_secs(inst("9999-12-30T22:00:00Z")), -8 * 3600);
        assert_eq!(la.offset_secs(inst("9999-12-30T22:00:01Z")), -8 * 3600);
        // Local dates beyond the range are held at its first and last day.
        assert_eq!(kolkata.bucket(last, Bucket::Day), "9999-12-31");
        assert_eq!(kolkata.bucket(last, Bucket::Year), "9999");
        let first = inst("0001-01-01T00:00:00Z");
        assert_eq!(la.bucket(first, Bucket::Day), "0001-01-01");
        assert_eq!(la.bucket(first, Bucket::Month), "0001-01");
        assert_eq!(la.bucket(first, Bucket::IsoWeek), "0001-W01");
        assert_eq!(kolkata.bucket(first, Bucket::Day), "0001-01-01");
        // Zone rules at the far ends are those of the database's last and first rules.
        assert_eq!(la.offset_secs(inst("9999-07-01T12:00:00Z")), -7 * 3600);
        assert_eq!(
            utc.bucket(
                Instant::from_micros(crate::datetime::MAX_MICROS).unwrap(),
                Bucket::Day
            ),
            "9999-12-31"
        );
    }
}
