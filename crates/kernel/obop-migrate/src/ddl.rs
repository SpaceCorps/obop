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
//! Classes follow PostgreSQL 17 and 18.
//!
//! [`classify`] reads the statement as tokens, so the text of a string literal or of a quoted
//! name is never mistaken for a keyword. It takes one statement at a time (two are an error),
//! prices a `VACUUM` with an option list, prices the constraints and defaults of an added column
//! (a `UNIQUE` or `PRIMARY KEY` column builds an index; a default that calls a function not known
//! to be stable rewrites the table), and refuses every form it has no price for.

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

/// One lexical item of a statement. The text of literals and quoted names is dropped (or kept
/// apart from keywords) so that only a bare word can be a keyword.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    /// A bare word, upper-cased.
    Word(String),
    /// A quoted name.
    Ident(String),
    /// A string literal or a dollar-quoted body.
    Str,
    /// A number or a parameter.
    Num(String),
    Sym(char),
}

/// The statement as tokens: comments and whitespace dropped, literals and quoted names set apart.
fn tokenize(sql: &str) -> Result<Vec<Tok>, String> {
    let b: Vec<char> = sql.chars().collect();
    let mut out: Vec<Tok> = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == '-' && b.get(i + 1) == Some(&'-') {
            while i < b.len() && b[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && b.get(i + 1) == Some(&'*') {
            // Block comments nest in PostgreSQL.
            let mut depth = 1;
            i += 2;
            while depth > 0 {
                match (b.get(i), b.get(i + 1)) {
                    (Some('/'), Some('*')) => {
                        depth += 1;
                        i += 2;
                    }
                    (Some('*'), Some('/')) => {
                        depth -= 1;
                        i += 2;
                    }
                    (Some(_), _) => i += 1,
                    (None, _) => return Err("a comment is not closed".into()),
                }
            }
        } else if c == '\'' {
            // `E'...'` lets a backslash escape the quote; a plain literal does not.
            let escapes = matches!(out.last(), Some(Tok::Word(w)) if w == "E")
                && i > 0
                && matches!(b[i - 1], 'e' | 'E');
            if escapes {
                out.pop();
            }
            i += 1;
            loop {
                match b.get(i) {
                    None => return Err("a string literal is not closed".into()),
                    Some('\\') if escapes => i += 2,
                    Some('\'') if b.get(i + 1) == Some(&'\'') => i += 2,
                    Some('\'') => {
                        i += 1;
                        break;
                    }
                    Some(_) => i += 1,
                }
            }
            out.push(Tok::Str);
        } else if c == '"' {
            let mut name = String::new();
            i += 1;
            loop {
                match b.get(i) {
                    None => return Err("a quoted name is not closed".into()),
                    Some('"') if b.get(i + 1) == Some(&'"') => {
                        name.push('"');
                        i += 2;
                    }
                    Some('"') => {
                        i += 1;
                        break;
                    }
                    Some(ch) => {
                        name.push(*ch);
                        i += 1;
                    }
                }
            }
            out.push(Tok::Ident(name));
        } else if c == '$' && b.get(i + 1).is_some_and(|d| d.is_ascii_digit()) {
            let start = i;
            i += 1;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            out.push(Tok::Num(b[start..i].iter().collect()));
        } else if c == '$' {
            // A dollar-quoted body: `$tag$ ... $tag$`.
            let mut j = i + 1;
            while j < b.len() && (b[j].is_alphanumeric() || b[j] == '_') {
                j += 1;
            }
            if b.get(j) != Some(&'$') {
                out.push(Tok::Sym('$'));
                i += 1;
                continue;
            }
            let tag: String = b[i..=j].iter().collect();
            let rest: String = b[j + 1..].iter().collect();
            match rest.find(&tag) {
                Some(at) => i = j + 1 + rest[..at].chars().count() + tag.chars().count(),
                None => return Err("a dollar-quoted body is not closed".into()),
            }
            out.push(Tok::Str);
        } else if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < b.len() && (b[i].is_alphanumeric() || b[i] == '_' || b[i] == '$') {
                i += 1;
            }
            let w: String = b[start..i].iter().collect();
            out.push(Tok::Word(w.to_ascii_uppercase()));
        } else if c.is_ascii_digit() {
            let start = i;
            while i < b.len() && (b[i].is_alphanumeric() || b[i] == '.' || b[i] == '_') {
                i += 1;
            }
            out.push(Tok::Num(b[start..i].iter().collect()));
        } else {
            out.push(Tok::Sym(c));
            i += 1;
        }
    }
    Ok(out)
}

/// The statements of a text, split at `;`. Empty ones are dropped.
fn statements(toks: Vec<Tok>) -> Vec<Vec<Tok>> {
    let mut all = Vec::new();
    let mut cur = Vec::new();
    for t in toks {
        if t == Tok::Sym(';') {
            if !cur.is_empty() {
                all.push(std::mem::take(&mut cur));
            }
        } else {
            cur.push(t);
        }
    }
    if !cur.is_empty() {
        all.push(cur);
    }
    all
}

fn show(t: &[Tok]) -> String {
    let mut s = String::new();
    for x in t {
        if !s.is_empty() {
            s.push(' ');
        }
        match x {
            Tok::Word(w) => s.push_str(w),
            Tok::Ident(n) => s.push_str(&format!("\"{n}\"")),
            Tok::Str => s.push_str("'...'"),
            Tok::Num(n) => s.push_str(n),
            Tok::Sym(c) => s.push(*c),
        }
    }
    s
}

fn at(t: &[Tok], i: usize, w: &str) -> bool {
    matches!(t.get(i), Some(Tok::Word(x)) if x == w)
}

/// The tokens start with these words.
fn starts(t: &[Tok], ws: &[&str]) -> bool {
    ws.iter().enumerate().all(|(i, w)| at(t, i, w))
}

/// The words occur, side by side, anywhere in the tokens.
fn has(t: &[Tok], ws: &[&str]) -> bool {
    (0..t.len()).any(|i| ws.iter().enumerate().all(|(k, w)| at(t, i + k, w)))
}

/// The tokens after the leading words, if they start with them.
fn strip<'a>(t: &'a [Tok], ws: &[&str]) -> Option<&'a [Tok]> {
    starts(t, ws).then(|| &t[ws.len()..])
}

/// Functions a default may call and still be evaluated once, when the column is added (stable
/// or immutable). A call to anything else is taken to be volatile: it rewrites the table.
const STABLE_IN_DEFAULTS: [&str; 17] = [
    "NOW",
    "TRANSACTION_TIMESTAMP",
    "STATEMENT_TIMESTAMP",
    "CURRENT_SETTING",
    "COALESCE",
    "NULLIF",
    "LOWER",
    "UPPER",
    "ABS",
    "ROUND",
    "TO_JSONB",
    "JSONB_BUILD_OBJECT",
    "JSON_BUILD_OBJECT",
    "TO_TIMESTAMP",
    "MAKE_INTERVAL",
    "CAST",
    "ROW",
];

/// Words that are followed by a bracket without being a function call: constraints, and the
/// types that take a length or a precision.
const NOT_CALLS: [&str; 22] = [
    "CHECK",
    "REFERENCES",
    "UNIQUE",
    "PRIMARY",
    "GENERATED",
    "NOT",
    "NULL",
    "CONSTRAINT",
    "DEFAULT",
    "USING",
    "DEFERRABLE",
    "ON",
    "AS",
    "VARCHAR",
    "CHAR",
    "CHARACTER",
    "NUMERIC",
    "DECIMAL",
    "TIMESTAMP",
    "TIMESTAMPTZ",
    "TIME",
    "INTERVAL",
];

/// A column default that calls a function not known to be stable: PostgreSQL evaluates it per
/// row and rewrites the table.
fn volatile_default(c: &[Tok]) -> bool {
    let Some(d) = (0..c.len()).find(|&i| at(c, i, "DEFAULT")) else {
        return false;
    };
    (d + 1..c.len()).any(|i| {
        let Tok::Word(name) = &c[i] else {
            return false;
        };
        c.get(i + 1) == Some(&Tok::Sym('('))
            && !NOT_CALLS.contains(&name.as_str())
            && !STABLE_IN_DEFAULTS.contains(&name.as_str())
    })
}

/// The cost class of one statement, or why it cannot be classified. Only the forms obop's
/// migrations are allowed to use are known; anything else, and more than one statement at a time,
/// is an error.
pub fn classify(sql: &str) -> Result<Class, String> {
    let mut stmts = statements(tokenize(sql)?);
    let t = match stmts.len() {
        0 => return Err("empty statement".into()),
        1 => stmts.remove(0),
        n => {
            return Err(format!(
                "{n} statements in one text: classify and run one statement at a time"
            ));
        }
    };
    let unknown = || {
        format!(
            "cannot classify {:?}: add the form to ddl::classify with its cost, or use a known one",
            show(&t).chars().take(80).collect::<String>()
        )
    };
    let Some(Tok::Word(first)) = t.first() else {
        return Err(unknown());
    };
    match first.as_str() {
        "INSERT" | "SELECT" | "WITH" | "DELETE" | "COMMENT" | "ANALYZE" | "UPDATE" => {
            Ok(Class::None)
        }
        "VACUUM" => Ok(vacuum(&t)),
        "CLUSTER" => Ok(Class::Rewrite),
        "LOCK" => Ok(Class::Blocking),
        "REINDEX" => Ok(if has(&t, &["CONCURRENTLY"]) {
            Class::Concurrent
        } else {
            Class::Blocking
        }),
        "REFRESH" if has(&t, &["MATERIALIZED", "VIEW"]) => Ok(if has(&t, &["CONCURRENTLY"]) {
            Class::Concurrent
        } else {
            Class::Blocking
        }),
        "CREATE" => create(&t).ok_or_else(unknown),
        "DROP" => drop(&t).ok_or_else(unknown),
        "ALTER" if starts(&t, &["ALTER", "TABLE"]) => alter_table(&t).ok_or_else(unknown),
        "ALTER" if starts(&t, &["ALTER", "INDEX"]) || starts(&t, &["ALTER", "SEQUENCE"]) => {
            Ok(Class::Instant)
        }
        _ => Err(unknown()),
    }
}

/// `VACUUM`, `VACUUM (ANALYZE)` and the rest leave the table alone; `FULL`, written either way,
/// rewrites it (`FULL false` and `FULL off` do not).
fn vacuum(t: &[Tok]) -> Class {
    let full = (0..t.len()).any(|i| {
        at(t, i, "FULL")
            && !(at(t, i + 1, "FALSE")
                || at(t, i + 1, "OFF")
                || matches!(t.get(i + 1), Some(Tok::Num(n)) if n == "0"))
    });
    if full { Class::Rewrite } else { Class::None }
}

fn create(t: &[Tok]) -> Option<Class> {
    let rest = strip(t, &["CREATE"])?;
    let rest = strip(rest, &["OR", "REPLACE"])
        .or_else(|| strip(rest, &["UNIQUE"]))
        .unwrap_or(rest);
    if starts(rest, &["INDEX"]) {
        return Some(if has(t, &["CONCURRENTLY"]) {
            Class::Concurrent
        } else {
            Class::Blocking
        });
    }
    if starts(rest, &["MATERIALIZED", "VIEW"]) {
        return Some(if has(t, &["WITH", "NO", "DATA"]) {
            Class::Instant
        } else {
            Class::Blocking
        });
    }
    // New objects: nothing else holds them yet. CREATE TABLE ... AS SELECT copies data, but only
    // into the table it creates.
    for k in [
        &["TABLE"][..],
        &["UNLOGGED", "TABLE"],
        &["VIEW"],
        &["SEQUENCE"],
        &["TRIGGER"],
        &["FUNCTION"],
        &["PROCEDURE"],
        &["TYPE"],
        &["EXTENSION"],
        &["SCHEMA"],
        &["ROLE"],
        &["POLICY"],
    ] {
        if starts(rest, k) {
            return Some(Class::Instant);
        }
    }
    None
}

fn drop(t: &[Tok]) -> Option<Class> {
    let rest = strip(t, &["DROP"])?;
    if starts(rest, &["INDEX"]) {
        return Some(if has(t, &["CONCURRENTLY"]) {
            Class::Concurrent
        } else {
            Class::Instant
        });
    }
    for k in [
        &["TABLE"][..],
        &["VIEW"],
        &["MATERIALIZED", "VIEW"],
        &["SEQUENCE"],
        &["TRIGGER"],
        &["FUNCTION"],
        &["PROCEDURE"],
        &["TYPE"],
        &["SCHEMA"],
        &["POLICY"],
        &["EXTENSION"],
    ] {
        if starts(rest, k) {
            return Some(Class::Instant);
        }
    }
    None
}

/// The class of one clause of `ALTER TABLE`.
fn clause(c: &[Tok]) -> Option<Class> {
    if starts(c, &["ADD", "CONSTRAINT"])
        || starts(c, &["ADD", "PRIMARY", "KEY"])
        || starts(c, &["ADD", "UNIQUE"])
        || starts(c, &["ADD", "FOREIGN", "KEY"])
        || starts(c, &["ADD", "CHECK"])
        || starts(c, &["ADD", "EXCLUDE"])
    {
        // An index that already exists, or a constraint added without the scan, is a catalogue
        // change; the scan is then `VALIDATE CONSTRAINT`.
        return Some(if has(c, &["NOT", "VALID"]) || has(c, &["USING", "INDEX"]) {
            Class::Instant
        } else {
            Class::Blocking
        });
    }
    if starts(c, &["VALIDATE", "CONSTRAINT"]) {
        return Some(Class::Concurrent);
    }
    if starts(c, &["ADD"]) {
        // `ADD [COLUMN]`.
        if has(c, &["GENERATED"]) && (has(c, &["STORED"]) || has(c, &["AS", "IDENTITY"])) {
            return Some(Class::Rewrite);
        }
        if has(c, &["SERIAL"]) || has(c, &["BIGSERIAL"]) || has(c, &["SMALLSERIAL"]) {
            return Some(Class::Rewrite);
        }
        if volatile_default(c) {
            return Some(Class::Rewrite);
        }
        // An inline constraint builds an index or scans the table under the lock.
        if has(c, &["UNIQUE"])
            || has(c, &["PRIMARY", "KEY"])
            || has(c, &["REFERENCES"])
            || has(c, &["CHECK"])
        {
            return Some(Class::Blocking);
        }
        // A nullable column, or one whose default is evaluated once: catalogue only (PostgreSQL 11+).
        return Some(Class::Instant);
    }
    if starts(c, &["DROP"]) {
        // `DROP [COLUMN]` and `DROP CONSTRAINT`: catalogue only (the space is reclaimed later).
        return Some(Class::Instant);
    }
    if starts(c, &["ALTER"]) {
        if has(c, &["TYPE"]) || has(c, &["SET", "DATA", "TYPE"]) {
            return Some(Class::Rewrite);
        }
        if has(c, &["SET", "NOT", "NULL"]) {
            return Some(Class::Blocking);
        }
        if has(c, &["ADD", "GENERATED"]) || has(c, &["SET", "EXPRESSION"]) {
            return Some(Class::Rewrite);
        }
        if has(c, &["DROP", "NOT", "NULL"])
            || has(c, &["SET", "DEFAULT"])
            || has(c, &["DROP", "DEFAULT"])
            || has(c, &["SET", "STATISTICS"])
            || has(c, &["SET", "STORAGE"])
            || has(c, &["DROP", "IDENTITY"])
        {
            return Some(Class::Instant);
        }
        return None;
    }
    if starts(c, &["RENAME"]) || starts(c, &["OWNER", "TO"]) || starts(c, &["SET", "SCHEMA"]) {
        return Some(Class::Instant);
    }
    if starts(c, &["ENABLE"])
        || starts(c, &["DISABLE"])
        || starts(c, &["FORCE", "ROW"])
        || starts(c, &["NO", "FORCE"])
    {
        return Some(Class::Instant);
    }
    if starts(c, &["SET", "LOGGED"])
        || starts(c, &["SET", "UNLOGGED"])
        || starts(c, &["SET", "TABLESPACE"])
    {
        return Some(Class::Rewrite);
    }
    if (starts(c, &["SET"]) || starts(c, &["RESET"])) && c.get(1) == Some(&Tok::Sym('(')) {
        return Some(Class::Instant);
    }
    if starts(c, &["ATTACH", "PARTITION"]) {
        return Some(Class::Blocking);
    }
    if starts(c, &["DETACH", "PARTITION"]) {
        return Some(if has(c, &["CONCURRENTLY"]) {
            Class::Concurrent
        } else {
            Class::Instant
        });
    }
    None
}

fn alter_table(t: &[Tok]) -> Option<Class> {
    let mut rest = strip(t, &["ALTER", "TABLE"])?;
    if let Some(r) = strip(rest, &["IF", "EXISTS"]) {
        rest = r;
    }
    if let Some(r) = strip(rest, &["ONLY"]) {
        rest = r;
    }
    // Skip the table name: a name, possibly schema-qualified.
    let is_name = |x: Option<&Tok>| matches!(x, Some(Tok::Word(_) | Tok::Ident(_)));
    if !is_name(rest.first()) {
        return None;
    }
    let mut i = 1;
    while rest.get(i) == Some(&Tok::Sym('.')) && is_name(rest.get(i + 1)) {
        i += 2;
    }
    if rest.get(i) == Some(&Tok::Sym('*')) {
        i += 1;
    }
    let clauses = &rest[i..];
    if clauses.is_empty() {
        return None;
    }
    // Split the clauses at top-level commas.
    let mut depth = 0i32;
    let mut cur: Vec<Tok> = Vec::new();
    let mut parts: Vec<Vec<Tok>> = Vec::new();
    for tok in clauses {
        match tok {
            Tok::Sym('(') => depth += 1,
            Tok::Sym(')') => depth -= 1,
            _ => {}
        }
        if *tok == Tok::Sym(',') && depth == 0 {
            parts.push(std::mem::take(&mut cur));
        } else {
            cur.push(tok.clone());
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

    /// The shapes the review found priced too low: each of these held a long lock or rewrote a
    /// big table while the classifier said it was free, or ran a statement it never read.
    #[test]
    fn the_shapes_a_substring_match_got_wrong_are_priced_or_refused() {
        use Class::*;
        let priced: &[(&str, Class)] = &[
            // VACUUM with an option list.
            ("VACUUM (FULL) big_table", Rewrite),
            ("VACUUM (FULL, ANALYZE) big_table", Rewrite),
            ("vacuum (full true) big_table", Rewrite),
            ("VACUUM (FULL false) big_table", None),
            ("VACUUM (ANALYZE) big_table", None),
            ("VACUUM ANALYZE big_table", None),
            // Inline constraints of an added column build an index or scan the table.
            ("ALTER TABLE big ADD COLUMN c int UNIQUE", Blocking),
            ("ALTER TABLE big ADD COLUMN id bigint PRIMARY KEY", Blocking),
            ("ALTER TABLE big ADD COLUMN c int CHECK (c > 0)", Blocking),
            (
                "ALTER TABLE big ADD COLUMN c int REFERENCES u (id)",
                Blocking,
            ),
            // A default that calls anything not known to be stable rewrites.
            ("ALTER TABLE big ADD COLUMN c uuid DEFAULT uuidv7()", Rewrite),
            (
                "ALTER TABLE big ADD COLUMN c int DEFAULT my_user_fn()",
                Rewrite,
            ),
            (
                "ALTER TABLE big ADD COLUMN c int DEFAULT public.my_user_fn(1)",
                Rewrite,
            ),
            (
                "ALTER TABLE big ADD COLUMN c varchar(10) DEFAULT 'x'::varchar(10)",
                Instant,
            ),
            (
                "ALTER TABLE big ADD COLUMN c timestamptz DEFAULT now() NOT NULL",
                Instant,
            ),
            // The words of a literal or of a quoted name are not keywords.
            (
                "ALTER TABLE big ADD COLUMN c text DEFAULT 'uuid_generate_v4( and TYPE and UNIQUE'",
                Instant,
            ),
            (
                "ALTER TABLE big ALTER COLUMN c SET DEFAULT 'concurrently type'",
                Instant,
            ),
            ("CREATE INDEX idx_concurrently_x ON big (c)", Blocking),
            ("CREATE INDEX \"concurrently\" ON big (c)", Blocking),
            ("CREATE INDEX CONCURRENTLY idx ON big (c)", Concurrent),
            // Names with quotes and schemas.
            (
                "ALTER TABLE \"my schema\".\"big table\" ALTER COLUMN c TYPE bigint",
                Rewrite,
            ),
            ("ALTER TABLE ONLY s.t ADD COLUMN c int", Instant),
        ];
        for (sql, want) in priced {
            assert_eq!(classify(sql), Ok(*want), "{sql}");
        }
        // More than one statement is never one class: the worst of them could be hidden.
        for sql in [
            "SELECT 1; ALTER TABLE big ALTER COLUMN c TYPE bigint",
            "UPDATE t SET c = 1; ALTER TABLE big ALTER COLUMN c TYPE bigint",
            "ALTER TABLE big ADD COLUMN c int; ALTER TABLE big ALTER COLUMN c TYPE bigint",
            "DROP TABLE big; ALTER TABLE big ALTER COLUMN c TYPE bigint",
        ] {
            let e = classify(sql).unwrap_err();
            assert!(e.contains("statements"), "{sql}: {e}");
        }
        // A trailing semicolon and a semicolon inside a literal or a body are not a second statement.
        assert_eq!(classify("ALTER TABLE t ADD COLUMN c int;"), Ok(Instant));
        assert_eq!(
            classify("ALTER TABLE t ADD COLUMN c text DEFAULT 'a;b'"),
            Ok(Instant)
        );
        assert_eq!(
            classify("CREATE FUNCTION f() RETURNS int AS $$ SELECT 1; $$ LANGUAGE sql"),
            Ok(Instant)
        );
        // Text that does not close is an error, not a guess.
        for sql in ["SELECT 'abc", "SELECT \"abc", "SELECT /* abc", "SELECT $$ abc"] {
            assert!(classify(sql).is_err(), "{sql}");
        }
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
