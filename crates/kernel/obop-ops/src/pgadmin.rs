//! What the operator tools ask the PostgreSQL server about itself (OBR-DAT-048, D73): its
//! settings, the state of WAL archiving, a named restore point, the size of the database.
//!
//! The questions are the [`ServerInfo`] port, so the checks built on them ([`crate::doctor`], the
//! restore point of a migration) are tested with a fake server that says whatever the test needs.
//! [`Pg`] answers them from a real server through the same TLS connector as every other
//! connection (`obop_storage::tls`, D66). It was written against the PostgreSQL documentation and
//! run against the stock `postgres:17` image only; it has not met a managed service, whose
//! superuser-only functions (`pg_create_restore_point`) and settings it may not be allowed to
//! read.

use std::sync::Mutex;

/// `pg_stat_archiver`, reduced to what a check needs.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ArchiverStatus {
    pub archived_count: u64,
    pub failed_count: u64,
    pub last_archived_wal: Option<String>,
    /// Seconds since the last segment was archived, by the server's clock.
    pub last_archived_age: Option<u64>,
    /// Seconds since the last failed attempt.
    pub last_failed_age: Option<u64>,
}

pub trait ServerInfo: Send + Sync {
    /// A server setting (`SHOW name`); `None` when the server does not know it.
    fn setting(&self, name: &str) -> Result<Option<String>, String>;
    /// The archiver's statistics.
    fn archiver(&self) -> Result<ArchiverStatus, String>;
    /// `pg_create_restore_point(name)`: the LSN it is at. Needs the privilege to call it.
    fn create_restore_point(&self, name: &str) -> Result<String, String>;
    /// Whether this is a standby that is still replaying WAL.
    fn in_recovery(&self) -> Result<bool, String>;
    /// Bytes of the database including indexes.
    fn database_bytes(&self) -> Result<u64, String>;
    /// The server version string (`PostgreSQL 17.2 ...`).
    fn version(&self) -> Result<String, String>;
}

/// A connection to a PostgreSQL server.
pub struct Pg {
    client: Mutex<postgres::Client>,
}

impl Pg {
    pub fn connect(url: &str) -> Result<Pg, String> {
        Ok(Pg {
            client: Mutex::new(obop_storage::tls::connect(url)?),
        })
    }

    fn one(
        &self,
        sql: &str,
        params: &[&(dyn postgres::types::ToSql + Sync)],
    ) -> Result<postgres::Row, String> {
        self.client
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .query_one(sql, params)
            .map_err(|e| e.to_string())
    }
}

impl ServerInfo for Pg {
    fn setting(&self, name: &str) -> Result<Option<String>, String> {
        Ok(self
            .one("SELECT current_setting($1, true)", &[&name])?
            .get::<_, Option<String>>(0))
    }

    fn archiver(&self) -> Result<ArchiverStatus, String> {
        let r = self.one(
            "SELECT archived_count, failed_count, last_archived_wal,
                    EXTRACT(EPOCH FROM (now() - last_archived_time))::bigint,
                    EXTRACT(EPOCH FROM (now() - last_failed_time))::bigint
             FROM pg_stat_archiver",
            &[],
        )?;
        let n = |i: usize| r.get::<_, Option<i64>>(i).map(|v| v.max(0) as u64);
        Ok(ArchiverStatus {
            archived_count: n(0).unwrap_or(0),
            failed_count: n(1).unwrap_or(0),
            last_archived_wal: r.get(2),
            last_archived_age: n(3),
            last_failed_age: n(4),
        })
    }

    fn create_restore_point(&self, name: &str) -> Result<String, String> {
        self.one("SELECT pg_create_restore_point($1)::text", &[&name])
            .map(|r| r.get::<_, String>(0))
    }

    fn in_recovery(&self) -> Result<bool, String> {
        Ok(self.one("SELECT pg_is_in_recovery()", &[])?.get(0))
    }

    fn database_bytes(&self) -> Result<u64, String> {
        Ok(self
            .one("SELECT pg_database_size(current_database())", &[])?
            .get::<_, i64>(0)
            .max(0) as u64)
    }

    fn version(&self) -> Result<String, String> {
        Ok(self.one("SELECT version()", &[])?.get(0))
    }
}

/// A connection for a long-running process that asks now and then: made when first needed, made
/// again (once, per question) when a question fails, so a restart of the database does not leave
/// the process asking a dead connection forever.
pub struct Reconnecting {
    url: String,
    inner: Mutex<Option<Pg>>,
}

impl Reconnecting {
    pub fn new(url: &str) -> Reconnecting {
        Reconnecting {
            url: url.to_string(),
            inner: Mutex::new(None),
        }
    }

    fn with<T>(&self, ask: impl Fn(&Pg) -> Result<T, String>) -> Result<T, String> {
        let mut g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let mut last = String::new();
        for _ in 0..2 {
            if g.is_none() {
                *g = Some(Pg::connect(&self.url)?);
            }
            match g.as_ref().map(&ask) {
                Some(Ok(v)) => return Ok(v),
                Some(Err(e)) => {
                    last = e;
                    *g = None;
                }
                None => {}
            }
        }
        Err(last)
    }
}

impl ServerInfo for Reconnecting {
    fn setting(&self, name: &str) -> Result<Option<String>, String> {
        self.with(|p| p.setting(name))
    }
    fn archiver(&self) -> Result<ArchiverStatus, String> {
        self.with(|p| p.archiver())
    }
    fn create_restore_point(&self, name: &str) -> Result<String, String> {
        self.with(|p| p.create_restore_point(name))
    }
    fn in_recovery(&self) -> Result<bool, String> {
        self.with(|p| p.in_recovery())
    }
    fn database_bytes(&self) -> Result<u64, String> {
        self.with(|p| p.database_bytes())
    }
    fn version(&self) -> Result<String, String> {
        self.with(|p| p.version())
    }
}

/// A server that answers from a table, for tests.
#[derive(Default)]
pub struct FakeServer {
    pub settings: std::collections::BTreeMap<String, String>,
    pub archiver: ArchiverStatus,
    pub restore_points: Mutex<Vec<String>>,
    pub restore_point_error: Option<String>,
    pub recovering: bool,
    pub bytes: u64,
}

impl FakeServer {
    /// A server set up the way docs/DEPLOY.md asks.
    pub fn well_configured() -> FakeServer {
        let mut s = FakeServer::default();
        for (k, v) in [
            ("wal_level", "replica"),
            ("archive_mode", "on"),
            (
                "archive_command",
                "pgbackrest --stanza=obop archive-push %p",
            ),
            ("archive_timeout", "60s"),
            ("ssl", "on"),
            ("data_checksums", "on"),
            ("fsync", "on"),
            ("full_page_writes", "on"),
        ] {
            s.settings.insert(k.into(), v.into());
        }
        s.archiver = ArchiverStatus {
            archived_count: 1200,
            failed_count: 0,
            last_archived_wal: Some("000000010000000000000042".into()),
            last_archived_age: Some(30),
            last_failed_age: None,
        };
        s.bytes = 5_000_000;
        s
    }

    pub fn with(mut self, k: &str, v: &str) -> FakeServer {
        self.settings.insert(k.into(), v.into());
        self
    }
}

impl ServerInfo for FakeServer {
    fn setting(&self, name: &str) -> Result<Option<String>, String> {
        Ok(self.settings.get(name).cloned())
    }
    fn archiver(&self) -> Result<ArchiverStatus, String> {
        Ok(self.archiver.clone())
    }
    fn create_restore_point(&self, name: &str) -> Result<String, String> {
        if let Some(e) = &self.restore_point_error {
            return Err(e.clone());
        }
        self.restore_points.lock().unwrap().push(name.to_string());
        Ok("0/16B3748".into())
    }
    fn in_recovery(&self) -> Result<bool, String> {
        Ok(self.recovering)
    }
    fn database_bytes(&self) -> Result<u64, String> {
        Ok(self.bytes)
    }
    fn version(&self) -> Result<String, String> {
        Ok("PostgreSQL 17.0 (fake)".into())
    }
}

/// A setting such as `60s`, `1min`, `300` (seconds) as seconds; `0` and `-1` as 0.
pub fn seconds(setting: &str) -> Option<u64> {
    let s = setting.trim();
    let (num, unit) = s.split_at(s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len()));
    let n: u64 = num.parse().ok()?;
    Some(match unit.trim() {
        "" | "s" => n,
        "ms" => n / 1000,
        "min" => n * 60,
        "h" => n * 3600,
        "d" => n * 86_400,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_as_the_server_prints_them() {
        for (s, want) in [
            ("60s", Some(60)),
            ("300", Some(300)),
            ("1min", Some(60)),
            ("2h", Some(7200)),
            ("500ms", Some(0)),
            ("0", Some(0)),
            ("1d", Some(86_400)),
            ("-1", None),
            ("soon", None),
            ("5 fortnights", None),
        ] {
            assert_eq!(seconds(s), want, "{s}");
        }
    }

    /// Against a real server, when one is given.
    #[test]
    fn a_real_server_answers_the_questions() {
        let Ok(url) = std::env::var("OBOP_TEST_POSTGRES_URL") else {
            eprintln!("skipped: set OBOP_TEST_POSTGRES_URL to run against PostgreSQL");
            return;
        };
        let pg = Pg::connect(&url).unwrap();
        // The reconnecting wrapper answers the same, and asks again after a failed question.
        let again = Reconnecting::new(&url);
        assert!(again.version().unwrap().starts_with("PostgreSQL"));
        *again.inner.lock().unwrap() = None;
        assert!(again.archiver().is_ok());
        assert!(
            Reconnecting::new("postgres://nobody@127.0.0.1:1/none")
                .version()
                .is_err()
        );
        assert!(pg.version().unwrap().starts_with("PostgreSQL"));
        assert!(pg.setting("wal_level").unwrap().is_some());
        assert_eq!(pg.setting("no_such_setting_at_all").unwrap(), None);
        assert!(pg.database_bytes().unwrap() > 0);
        assert!(!pg.in_recovery().unwrap());
        // pg_stat_archiver exists on every server, archiving or not.
        let a = pg.archiver().unwrap();
        assert!(a.failed_count < u64::MAX);
        // A restore point needs wal_level replica or higher and the privilege (the test
        // container's role is a superuser).
        let level = pg.setting("wal_level").unwrap().unwrap();
        let r = pg.create_restore_point("obop-test-restore-point");
        if level == "minimal" {
            assert!(r.is_err());
        } else {
            assert!(r.is_ok(), "{r:?}");
        }
        assert!(
            pg.create_restore_point(&"x".repeat(100)).is_err(),
            "names are 63 bytes at most"
        );
    }
}
