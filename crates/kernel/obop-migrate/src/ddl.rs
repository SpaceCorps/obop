//! What a DDL statement costs (OBR-DAT-051, D73), and the one policy every DDL statement must run
//! under.
//!
//! Nothing in the product issues DDL on the serving path today except the schema creation of
//! `obop-storage` and the unique indexes of D57; the typed layout, history tables, guard triggers
//! and index builds of D75 to D77 will. They all go through one component (the "DDL component",
//! D77 wave 2) that must: classify the statement with [`classify`] before running it, run it under
//! [`Policy`] (`lock_timeout`, `statement_timeout`, the cross-process migration lock, a bounded
//! jittered retry on lock timeout), and refuse a [`Class::Blocking`] or [`Class::Rewrite`]
//! statement on a table above [`WINDOW_ROWS`] rows unless the operator named a maintenance window.
//! This module is the part that needs no database: the table of known forms and the policy.
//! Classes follow PostgreSQL 17 and 18; an unknown form is refused, not guessed.

use std::time::Duration;

/// How long a DDL statement holds back readers and writers of its table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Class {
    /// Not DDL, or DDL that changes no stored data and takes its lock only for an instant.
    None,
    /// A catalogue change only: held for an instant whatever the table size.
    Instant,
    /// Runs beside normal reads and writes (`CONCURRENTLY`, `VALIDATE CONSTRAINT`).
    Concurrent,
    /// Scans the table while holding a lock that blocks writes (or reads and writes).
    Blocking,
    /// Rewrites the table or its indexes under an exclusive lock.
    Rewrite,
}

impl Class {
    pub fn as_str(self) -> &'static str {
        match self {
            Class::None => "none",
            Class::Instant => "instant",
            Class::Concurrent => "concurrent",
            Class::Blocking => "blocking",
            Class::Rewrite => "rewrite",
        }
    }

    pub fn parse(s: &str) -> Option<Class> {
        Some(match s {
            "none" => Class::None,
            "instant" => Class::Instant,
            "concurrent" => Class::Concurrent,
            "blocking" => Class::Blocking,
            "rewrite" => Class::Rewrite,
            _ => return Option::None,
        })
    }

    /// Whether the statement keeps writers (or everyone) out for as long as the table is big.
    pub fn holds_a_long_lock(self) -> bool {
        matches!(self, Class::Blocking | Class::Rewrite)
    }
}

/// A table above this many rows needs a named maintenance window for a long-lock statement.
pub const WINDOW_ROWS: u64 = 1_000_000;

/// Whether `class` on a table of `rows` rows may run only in a maintenance window.
pub fn needs_window(class: Class, rows: u64) -> bool {
    class.holds_a_long_lock() && rows > WINDOW_ROWS
}

/// Strip comments, collapse whitespace and quoted identifiers' case, upper-case keywords.
fn normalise(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    let b: Vec<char> = sql.chars().collect();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == '-' && b.get(i + 1) == Some(&'-') {
            while i < b.len() && b[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && b.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < b.len() && !(b[i] == '*' && b[i + 1] == '/') {
                i += 1;
            }
            i += 2;
        } else if c.is_whitespace() {
            if !out.ends_with(' ') && !out.is_empty() {
                out.push(' ');
            }
            i += 1;
        } else {
            out.push(c.to_ascii_uppercase());
            i += 1;
        }
    }
    out.trim().trim_end_matches(';').trim().to_string()
}

fn has(s: &str, needle: &str) -> bool {
    s.contains(needle)
}

/// Defaults that PostgreSQL evaluates once (a constant, or a stable function such as `now()`)
/// leave the table alone; a volatile one rewrites it.
fn volatile_default(s: &str) -> bool {
    [
        "RANDOM(",
        "GEN_RANDOM_UUID(",
        "UUID_GENERATE_V4(",
        "CLOCK_TIMESTAMP(",
        "TIMEOFDAY(",
        "NEXTVAL(",
        "TXID_CURRENT(",
        "GEN_RANDOM_BYTES(",
    ]
    .iter()
    .any(|f| s.contains(f))
}

/// The cost class of one statement, or why it cannot be classified. Only the forms obop's
/// migrations are allowed to use are known; anything else is an error.
pub fn classify(sql: &str) -> Result<Class, String> {
    let s = normalise(sql);
    if s.is_empty() {
        return Err("empty statement".into());
    }
    let unknown = || {
        format!(
            "cannot classify {:?}: add the form to ddl::classify with its cost, or use a known one",
            s.chars().take(80).collect::<String>()
        )
    };
    let first = s.split(' ').next().unwrap_or("");
    match first {
        "INSERT" | "SELECT" | "WITH" | "DELETE" | "COMMENT" | "ANALYZE" | "VACUUM"
            if !has(&s, "VACUUM FULL") =>
        {
            Ok(Class::None)
        }
        "UPDATE" => Ok(Class::None),
        "VACUUM" | "CLUSTER" => Ok(Class::Rewrite),
        "LOCK" => Ok(Class::Blocking),
        "REINDEX" => Ok(if has(&s, "CONCURRENTLY") {
            Class::Concurrent
        } else {
            Class::Blocking
        }),
        "REFRESH" if has(&s, "MATERIALIZED VIEW") => Ok(if has(&s, "CONCURRENTLY") {
            Class::Concurrent
        } else {
            Class::Blocking
        }),
        "CREATE" => create(&s).ok_or_else(unknown),
        "DROP" => drop(&s).ok_or_else(unknown),
        "ALTER" if s.starts_with("ALTER TABLE") => alter_table(&s).ok_or_else(unknown),
        "ALTER" if s.starts_with("ALTER INDEX") || s.starts_with("ALTER SEQUENCE") => {
            Ok(Class::Instant)
        }
        _ => Err(unknown()),
    }
}

fn create(s: &str) -> Option<Class> {
    let rest = s.strip_prefix("CREATE ")?;
    let rest = rest
        .strip_prefix("OR REPLACE ")
        .or_else(|| rest.strip_prefix("UNIQUE "))
        .unwrap_or(rest);
    if rest.starts_with("INDEX") {
        return Some(if has(s, "CONCURRENTLY") {
            Class::Concurrent
        } else {
            Class::Blocking
        });
    }
    if rest.starts_with("MATERIALIZED VIEW") {
        return Some(if has(s, "WITH NO DATA") {
            Class::Instant
        } else {
            Class::Blocking
        });
    }
    // New objects: nothing else holds them yet. CREATE TABLE ... AS SELECT copies data, but only
    // into the table it creates.
    for k in [
        "TABLE",
        "UNLOGGED TABLE",
        "VIEW",
        "SEQUENCE",
        "TRIGGER",
        "FUNCTION",
        "PROCEDURE",
        "TYPE",
        "EXTENSION",
        "SCHEMA",
        "ROLE",
        "POLICY",
    ] {
        if rest.starts_with(k) {
            return Some(Class::Instant);
        }
    }
    None
}

fn drop(s: &str) -> Option<Class> {
    let rest = s.strip_prefix("DROP ")?;
    if rest.starts_with("INDEX") {
        return Some(if has(s, "CONCURRENTLY") {
            Class::Concurrent
        } else {
            Class::Instant
        });
    }
    for k in [
        "TABLE",
        "VIEW",
        "MATERIALIZED VIEW",
        "SEQUENCE",
        "TRIGGER",
        "FUNCTION",
        "PROCEDURE",
        "TYPE",
        "SCHEMA",
        "POLICY",
        "EXTENSION",
    ] {
        if rest.starts_with(k) {
            return Some(Class::Instant);
        }
    }
    None
}

/// The class of one clause of `ALTER TABLE`.
fn clause(c: &str) -> Option<Class> {
    let c = c.trim();
    if c.starts_with("ADD CONSTRAINT")
        || c.starts_with("ADD PRIMARY KEY")
        || c.starts_with("ADD UNIQUE")
        || c.starts_with("ADD FOREIGN KEY")
        || c.starts_with("ADD CHECK")
        || c.starts_with("ADD EXCLUDE")
    {
        // An index that already exists, or a constraint added without the scan, is a catalogue
        // change; the scan is then `VALIDATE CONSTRAINT`.
        return Some(if has(c, "NOT VALID") || has(c, "USING INDEX") {
            Class::Instant
        } else {
            Class::Blocking
        });
    }
    if c.starts_with("VALIDATE CONSTRAINT") {
        return Some(Class::Concurrent);
    }
    if c.starts_with("ADD ") {
        // `ADD [COLUMN]`.
        if has(c, "GENERATED") && (has(c, "STORED") || has(c, "AS IDENTITY")) {
            return Some(Class::Rewrite);
        }
        if has(c, " SERIAL") || has(c, " BIGSERIAL") || has(c, " SMALLSERIAL") {
            return Some(Class::Rewrite);
        }
        if has(c, "DEFAULT") && volatile_default(c) {
            return Some(Class::Rewrite);
        }
        // A nullable column, or one whose default is evaluated once: catalogue only (PostgreSQL 11+).
        return Some(Class::Instant);
    }
    if c.starts_with("DROP ") {
        // `DROP [COLUMN]` and `DROP CONSTRAINT`: catalogue only (the space is reclaimed later).
        return Some(Class::Instant);
    }
    if c.starts_with("ALTER ") {
        if has(c, " TYPE ") || has(c, "SET DATA TYPE") {
            return Some(Class::Rewrite);
        }
        if has(c, "SET NOT NULL") {
            return Some(Class::Blocking);
        }
        if has(c, "ADD GENERATED") || has(c, "SET EXPRESSION") {
            return Some(Class::Rewrite);
        }
        if has(c, "DROP NOT NULL")
            || has(c, "SET DEFAULT")
            || has(c, "DROP DEFAULT")
            || has(c, "SET STATISTICS")
            || has(c, "SET STORAGE")
            || has(c, "DROP IDENTITY")
        {
            return Some(Class::Instant);
        }
        return None;
    }
    if c.starts_with("RENAME") || c.starts_with("OWNER TO") || c.starts_with("SET SCHEMA") {
        return Some(Class::Instant);
    }
    if c.starts_with("ENABLE")
        || c.starts_with("DISABLE")
        || c.starts_with("FORCE ROW")
        || c.starts_with("NO FORCE")
    {
        return Some(Class::Instant);
    }
    if c.starts_with("SET LOGGED")
        || c.starts_with("SET UNLOGGED")
        || c.starts_with("SET TABLESPACE")
    {
        return Some(Class::Rewrite);
    }
    if c.starts_with("SET (") || c.starts_with("RESET (") {
        return Some(Class::Instant);
    }
    if c.starts_with("ATTACH PARTITION") {
        return Some(Class::Blocking);
    }
    if c.starts_with("DETACH PARTITION") {
        return Some(if has(c, "CONCURRENTLY") {
            Class::Concurrent
        } else {
            Class::Instant
        });
    }
    None
}

fn alter_table(s: &str) -> Option<Class> {
    let mut rest = s.strip_prefix("ALTER TABLE")?.trim_start();
    if let Some(r) = rest.strip_prefix("IF EXISTS ") {
        rest = r;
    }
    if let Some(r) = rest.strip_prefix("ONLY ") {
        rest = r;
    }
    // Skip the table name (one token, possibly schema-qualified).
    let (_, clauses) = rest.split_once(' ')?;
    // Split the clauses at top-level commas.
    let mut depth = 0i32;
    let mut cur = String::new();
    let mut parts = Vec::new();
    for ch in clauses.chars() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        if ch == ',' && depth == 0 {
            parts.push(std::mem::take(&mut cur));
        } else {
            cur.push(ch);
        }
    }
    parts.push(cur);
    let mut worst = Class::None;
    for p in parts {
        worst = worst.max(clause(&p)?);
    }
    Some(worst)
}

/// How a DDL statement must be run (OBR-DAT-051 item 3). Values are the defaults of the
/// requirement; the DDL component reads overrides from the environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    /// `SET lock_timeout`: how long a statement waits for its lock before giving up.
    pub lock_timeout: Duration,
    /// `SET statement_timeout`: the longest a statement may run.
    pub statement_timeout: Duration,
    /// How many times a statement that timed out waiting for a lock is tried again.
    pub max_retries: u32,
    /// The pause before the first retry; doubled each time, up to `max_backoff`, plus jitter.
    pub base_backoff: Duration,
    pub max_backoff: Duration,
}

impl Default for Policy {
    fn default() -> Self {
        Policy {
            lock_timeout: Duration::from_secs(5),
            statement_timeout: Duration::from_secs(30 * 60),
            max_retries: 5,
            base_backoff: Duration::from_millis(500),
            max_backoff: Duration::from_secs(30),
        }
    }
}

impl Policy {
    /// The SQL that sets the two timeouts for the session, in milliseconds.
    pub fn session_sql(&self) -> String {
        format!(
            "SET lock_timeout = {}; SET statement_timeout = {}",
            self.lock_timeout.as_millis(),
            self.statement_timeout.as_millis()
        )
    }

    /// The pause before retry number `attempt` (1 for the first retry): exponential, capped, with
    /// `jitter` (any number; a random one in production) adding up to half the base pause so
    /// two runners that collided do not collide again. Deterministic given `jitter`.
    pub fn backoff(&self, attempt: u32, jitter: u64) -> Duration {
        let exp = self
            .base_backoff
            .saturating_mul(1u32 << attempt.saturating_sub(1).min(16))
            .min(self.max_backoff);
        let spread = (self.base_backoff.as_millis() as u64 / 2).max(1);
        exp + Duration::from_millis(jitter % spread)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twenty_five_known_statements_are_classified_for_postgresql_17_and_18() {
        use Class::*;
        let table: &[(&str, Class)] = &[
            ("ALTER TABLE t ADD COLUMN c int", Instant),
            ("ALTER TABLE t ADD COLUMN c int DEFAULT 0", Instant),
            (
                "ALTER TABLE t ADD COLUMN c timestamptz DEFAULT now()",
                Instant,
            ),
            (
                "ALTER TABLE t ADD COLUMN c uuid DEFAULT gen_random_uuid()",
                Rewrite,
            ),
            ("ALTER TABLE t ADD COLUMN c bigserial", Rewrite),
            (
                "ALTER TABLE t ADD COLUMN c int GENERATED ALWAYS AS (a + 1) STORED",
                Rewrite,
            ),
            ("ALTER TABLE t DROP COLUMN c", Instant),
            ("ALTER TABLE t ALTER COLUMN c TYPE bigint", Rewrite),
            ("ALTER TABLE t ALTER COLUMN c SET NOT NULL", Blocking),
            ("ALTER TABLE t ALTER COLUMN c DROP NOT NULL", Instant),
            ("ALTER TABLE t ALTER COLUMN c SET DEFAULT 5", Instant),
            ("CREATE INDEX i ON t (c)", Blocking),
            ("CREATE UNIQUE INDEX i ON t (c)", Blocking),
            ("CREATE INDEX CONCURRENTLY i ON t (c)", Concurrent),
            ("DROP INDEX i", Instant),
            ("DROP INDEX CONCURRENTLY i", Concurrent),
            (
                "ALTER TABLE t ADD CONSTRAINT f FOREIGN KEY (a) REFERENCES u (id)",
                Blocking,
            ),
            (
                "ALTER TABLE t ADD CONSTRAINT f FOREIGN KEY (a) REFERENCES u (id) NOT VALID",
                Instant,
            ),
            ("ALTER TABLE t ADD CONSTRAINT k CHECK (c > 0)", Blocking),
            (
                "ALTER TABLE t ADD CONSTRAINT k CHECK (c > 0) NOT VALID",
                Instant,
            ),
            ("ALTER TABLE t VALIDATE CONSTRAINT k", Concurrent),
            (
                "ALTER TABLE t ADD CONSTRAINT u UNIQUE USING INDEX i",
                Instant,
            ),
            ("ALTER TABLE t ADD CONSTRAINT p PRIMARY KEY (id)", Blocking),
            ("REINDEX INDEX CONCURRENTLY i", Concurrent),
            ("REINDEX TABLE t", Blocking),
            ("VACUUM FULL t", Rewrite),
            ("ALTER TABLE t DETACH PARTITION p CONCURRENTLY", Concurrent),
            ("CREATE TABLE t (id int)", Instant),
            (
                "CREATE TRIGGER x BEFORE UPDATE ON t FOR EACH ROW EXECUTE FUNCTION f()",
                Instant,
            ),
            ("ALTER TABLE t DISABLE TRIGGER x", Instant),
            ("ALTER TABLE t RENAME TO u", Instant),
            ("UPDATE t SET c = 1 WHERE id = 2", None),
            (
                "ALTER TABLE t ADD COLUMN a int, ALTER COLUMN b TYPE text",
                Rewrite,
            ),
            ("  alter   table t\n  add column c int -- a note\n", Instant),
        ];
        for (sql, want) in table {
            assert_eq!(classify(sql), Ok(*want), "{sql}");
        }
        assert!(table.len() >= 25);
    }

    #[test]
    fn a_form_nobody_has_priced_is_refused_not_guessed() {
        for sql in [
            "",
            "GRANT ALL ON t TO x",
            "ALTER TABLE t FROBNICATE",
            "CREATE WIDGET w",
        ] {
            assert!(classify(sql).is_err(), "{sql:?}");
        }
        let e = classify("ALTER TABLE t FROBNICATE").unwrap_err();
        assert!(
            e.contains("cannot classify") && e.contains("FROBNICATE"),
            "{e}"
        );
    }

    #[test]
    fn a_long_lock_on_a_big_table_needs_a_window() {
        assert!(needs_window(Class::Blocking, 2_000_000));
        assert!(needs_window(Class::Rewrite, WINDOW_ROWS + 1));
        assert!(!needs_window(Class::Blocking, WINDOW_ROWS));
        for c in [Class::None, Class::Instant, Class::Concurrent] {
            assert!(!needs_window(c, u64::MAX), "{c:?}");
        }
        for c in [
            Class::None,
            Class::Instant,
            Class::Concurrent,
            Class::Blocking,
            Class::Rewrite,
        ] {
            assert_eq!(Class::parse(c.as_str()), Some(c));
        }
    }

    #[test]
    fn the_policy_defaults_are_the_requirements_and_the_backoff_is_bounded_and_jittered() {
        let p = Policy::default();
        assert_eq!(p.lock_timeout, Duration::from_secs(5));
        assert_eq!(p.statement_timeout, Duration::from_secs(1800));
        assert_eq!(
            p.session_sql(),
            "SET lock_timeout = 5000; SET statement_timeout = 1800000"
        );
        let (a, b, c) = (p.backoff(1, 0), p.backoff(2, 0), p.backoff(3, 0));
        assert!(a < b && b < c, "{a:?} {b:?} {c:?}");
        assert!(p.backoff(40, 0) <= p.max_backoff, "capped");
        assert_ne!(p.backoff(2, 17), p.backoff(2, 91), "jitter moves it");
        assert!(
            p.backoff(2, 91) < b + p.base_backoff,
            "by less than the base"
        );
        assert_eq!(
            p.backoff(3, 55),
            p.backoff(3, 55),
            "deterministic per jitter"
        );
    }
}
