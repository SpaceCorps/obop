//! The PostgreSQL schema as parts (K13): the DDL text lives here and nowhere else, so that what a
//! process may do at start-up (create what is missing, or only check), what an owner installs and
//! what an inspection compares against are one definition.
//!
//! A [`Part`] is one group of objects: how to create it (`render`, idempotent SQL with a
//! `{schema}` placeholder), how to tell that it is there (`check`, a catalogue lookup that issues
//! no DDL) and a digest of the SQL it renders (see [`Part::digest`]), which a migration step
//! pins so that edited SQL cannot reach existing databases unannounced. The record store has two
//! parts today, `base` (records, counters, short-lived values) and `history` (the change history of
//! D75, `pg_hist`; see [`parts`]); [`BLOB`] is the table of the byte store, which only some
//! deployments use. The guards and the query support of waves D76 to D78 each add a part.
//!
//! Who calls what:
//!
//! - `Pg::connect_options` calls [`ensure`] inside the `obop-schema` advisory lock, so that
//!   processes that start together do not trip over each other, and the byte store
//!   (`blob::DbBytes`) does the same for [`BLOB`]. With [`Ddl::IfMissing`] a part that is not
//!   completely there is created; with [`Ddl::Never`] a *required* part that is not there is an
//!   error and no DDL is issued (a serving role that may not create tables). A part that is not
//!   required (see [`Part::required`]) is only reported by [`inspect`] under `Never`: the process
//!   starts, and the feature of that part says what is missing when it is first used.
//! - [`owner_install`] / [`install_all`] create every part, the byte store's too, unconditionally,
//!   as the role that owns the objects (tests, the control plane, a migration step).
//! - [`inspect`] compares a live database with the parts of the record store and reports what is
//!   wrong. It leaves [`BLOB`] out: a deployment that keeps its files in a directory has no such
//!   table.
//!
//! A check that cannot be answered is an error, never "the part is missing": a refused or failed
//! catalogue query (the connection broke, the statement ran into its limit, the transaction was
//! already aborted) says nothing about the schema, and reading it as "missing" would advise DDL
//! for a complete database. The errors come through `describe`, so a broken connection is a
//! noted `storage_unavailable`.

use crate::pgpool::describe;
use crate::{Ddl, Result};
use postgres::Client;
use postgres::error::SqlState;
use sha2::{Digest, Sha256};

/// The advisory lock that serialises schema work between processes (and with the blob store's
/// table, which is created under the same key).
pub(crate) const LOCK_SQL: &str =
    "SELECT pg_advisory_xact_lock(hashtextextended('obop-schema', 0))";

/// One group of schema objects.
pub struct Part {
    pub name: &'static str,
    /// The idempotent SQL that creates the part. `schema` is an identifier that is safe to put
    /// into SQL as it is (quoted when it needs to be); `{schema}` in the template is replaced
    /// by it.
    pub render: fn(schema: &str) -> String,
    /// Is the part there? A catalogue lookup: it issues no DDL and needs no privilege beyond
    /// connecting. `Err` when the lookup itself failed.
    pub check: fn(&mut Client) -> Result<bool>,
    /// Must a process that may not create it (`Ddl::Never`) refuse to start without it? The
    /// records are needed by everything; the change history is needed only once a model is
    /// tracked (D75), so a database that was installed before it keeps starting and the history
    /// refuses its first write with the reason (see `pg_hist`).
    pub required: bool,
}

impl Part {
    /// The SHA-256 (lowercase hex) of what [`Part::render`] gives for the schema `public`. A
    /// ledger step that installs the part records it, and a test fails when the SQL changes
    /// without a new step.
    pub fn digest(&self) -> String {
        let mut h = Sha256::new();
        h.update((self.render)("public").as_bytes());
        h.finalize().iter().map(|b| format!("{b:02x}")).collect()
    }
}

/// The record store: records, counters, short-lived values.
const BASE_SQL: &str = "CREATE TABLE IF NOT EXISTS {schema}.obop_record (
         seq   BIGINT GENERATED ALWAYS AS IDENTITY,
         model TEXT NOT NULL,
         id    TEXT NOT NULL,
         data  JSONB NOT NULL CHECK (jsonb_typeof(data) = 'object'),
         PRIMARY KEY (model, id)
     );
     CREATE INDEX IF NOT EXISTS obop_record_seq ON {schema}.obop_record (model, seq);
     CREATE INDEX IF NOT EXISTS obop_record_seq_all ON {schema}.obop_record (seq);
     CREATE TABLE IF NOT EXISTS {schema}.obop_counter (
         key        TEXT PRIMARY KEY,
         n          BIGINT NOT NULL,
         expires_ms BIGINT NOT NULL
     );
     CREATE TABLE IF NOT EXISTS {schema}.obop_kv (
         key        TEXT PRIMARY KEY,
         value      TEXT NOT NULL,
         expires_ms BIGINT NOT NULL
     );
     CREATE INDEX IF NOT EXISTS obop_kv_expiry ON {schema}.obop_kv (expires_ms);";

/// What the `base` part leaves behind, as the server resolves the names (search path).
const BASE_OBJECTS: [&str; 6] = [
    "obop_record",
    "obop_record_seq",
    "obop_record_seq_all",
    "obop_counter",
    "obop_kv",
    "obop_kv_expiry",
];

fn render_base(schema: &str) -> String {
    BASE_SQL.replace("{schema}", schema)
}

/// Are all `names` relations the server finds (through the search path)? An error when the
/// question could not be asked.
pub(crate) fn present(c: &mut Client, names: &[&str]) -> Result<bool> {
    let row = c
        .query_one(
            "SELECT bool_and(to_regclass(n) IS NOT NULL) FROM unnest($1::text[]) AS t(n)",
            &[&names],
        )
        .map_err(|e| describe(&e))?;
    // `names` is never empty, so the aggregate is a value.
    Ok(row.get::<_, Option<bool>>(0).unwrap_or(false))
}

fn base_present(c: &mut Client) -> Result<bool> {
    present(c, &BASE_OBJECTS)
}

/// The record store proper: everything but the optional parts.
const BASE: Part = Part {
    name: "base",
    render: render_base,
    check: base_present,
    required: true,
};

/// The byte store's table (`blob::DbBytes`): chunks of the files kept in the database.
const BLOB_SQL: &str = "CREATE TABLE IF NOT EXISTS {schema}.obop_blob_data (
         key  TEXT NOT NULL,
         seq  INTEGER NOT NULL,
         data BYTEA NOT NULL,
         PRIMARY KEY (key, seq)
     );";

const BLOB_OBJECTS: [&str; 1] = ["obop_blob_data"];

fn render_blob(schema: &str) -> String {
    BLOB_SQL.replace("{schema}", schema)
}

fn blob_present(c: &mut Client) -> Result<bool> {
    present(c, &BLOB_OBJECTS)
}

/// The table of the byte store. Not one of [`parts`]: a deployment that keeps its files in a
/// directory (`OBOP_BLOB_DIR`) never needs it, so the record store does not require it. The
/// byte store ensures it under the `Ddl` it was opened with, and [`install_all`] creates it.
pub static BLOB: Part = Part {
    name: "blob",
    render: render_blob,
    check: blob_present,
    required: true,
};

/// The parts of the record store, in the order they are created: `base`, then the change history.
pub fn parts() -> &'static [Part] {
    static PARTS: [Part; 2] = [BASE, crate::pg_hist::PART];
    &PARTS
}

/// The schema new objects are created in (the first of the search path that exists), quoted for
/// use in SQL.
fn current_schema(c: &mut Client) -> Result<String> {
    let name: Option<String> = c
        .query_one("SELECT quote_ident(current_schema())", &[])
        .map_err(|e| describe(&e))?
        .get(0);
    name.ok_or_else(|| {
        "the connection has no schema to create the obop tables in (its search_path names none \
         that exists)"
            .to_string()
    })
}

fn ddl_error(part: &str, err: &postgres::Error) -> String {
    let text = describe(err);
    if err.code() == Some(&SqlState::INSUFFICIENT_PRIVILEGE) {
        format!(
            "{text}; this role may not create the obop schema (part {part}): create it once as \
             a role that may (obop_storage::schema::owner_install) and connect as the serving \
             role afterwards"
        )
    } else {
        text
    }
}

/// Create (`Ddl::IfMissing`) or require (`Ddl::Never`) every part of the schema. Run it inside
/// the schema lock (`with_lock`): two processes that both find a part missing would otherwise
/// both create it. Under `Never` a part that is not [`Part::required`] may be missing.
pub fn ensure(c: &mut Client, ddl: Ddl) -> Result<()> {
    ensure_parts(c, parts(), ddl)
}

pub(crate) fn ensure_parts(c: &mut Client, parts: &[Part], ddl: Ddl) -> Result<()> {
    ensure_with(c, parts, ddl, false)
}

/// As [`ensure_parts`], but under `Never` every part named is required, whatever its flag: the
/// question "is this feature installed?" of the feature itself.
pub(crate) fn ensure_strict(c: &mut Client, parts: &[Part], ddl: Ddl) -> Result<()> {
    ensure_with(c, parts, ddl, true)
}

fn ensure_with(c: &mut Client, parts: &[Part], ddl: Ddl, strict: bool) -> Result<()> {
    let mut missing: Vec<&Part> = Vec::new();
    for p in parts {
        if !(p.check)(c)? {
            missing.push(p);
        }
    }
    if missing.is_empty() {
        return Ok(());
    }
    match ddl {
        Ddl::Never => {
            let needed: Vec<&str> = missing
                .iter()
                .filter(|p| strict || p.required)
                .map(|p| p.name)
                .collect();
            if needed.is_empty() {
                return Ok(());
            }
            Err(format!(
                "the database does not have the obop schema (missing: {}), and this process may not \
                 create it: create it first with a role that may create tables",
                needed.join(", ")
            ))
        }
        Ddl::IfMissing => {
            let schema = current_schema(c)?;
            for p in missing {
                c.batch_execute(&(p.render)(&schema))
                    .map_err(|e| ddl_error(p.name, &e))?;
            }
            Ok(())
        }
    }
}

/// Create every part, the byte store's [`BLOB`] included, whether or not it is there (the SQL is
/// idempotent). For the role that owns the objects; the caller holds the schema lock (see
/// [`owner_install`]).
pub fn install_all(c: &mut Client) -> Result<()> {
    let schema = current_schema(c)?;
    for p in parts().iter().chain(std::iter::once(&BLOB)) {
        c.batch_execute(&(p.render)(&schema))
            .map_err(|e| ddl_error(p.name, &e))?;
    }
    Ok(())
}

/// Run `f` in a transaction that holds the schema lock, committing when it returns `Ok`.
pub(crate) fn with_lock<T>(c: &mut Client, f: impl FnOnce(&mut Client) -> Result<T>) -> Result<T> {
    c.batch_execute("BEGIN").map_err(|e| describe(&e))?;
    let done = c
        .batch_execute(LOCK_SQL)
        .map_err(|e| describe(&e))
        .and_then(|()| f(c));
    match done {
        Ok(v) => {
            c.batch_execute("COMMIT").map_err(|e| describe(&e))?;
            Ok(v)
        }
        Err(err) => {
            let _ = c.batch_execute("ROLLBACK");
            Err(err)
        }
    }
}

/// Connect to `url` as the role that owns the objects and create every part of the schema.
pub fn owner_install(url: &str) -> Result<()> {
    let mut c = crate::tls::connect(url)?;
    with_lock(&mut c, install_all)
}

/// One thing an inspection found wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// A stable word: `schema_part_missing`.
    pub code: &'static str,
    /// What it is about (the part, later a table, a role, an index).
    pub object: String,
    pub detail: String,
    /// An error makes a check fail; a warning is only reported.
    pub error: bool,
}

/// Compare a live database with the parts of the record store and report what differs. Today
/// that is whether each part is there; roles, triggers, functions and indexes join as their parts
/// do. `Err` when the database could not be asked: that is not a finding about the schema.
pub fn inspect(c: &mut Client) -> Result<Vec<Finding>> {
    inspect_parts(c, parts())
}

pub(crate) fn inspect_parts(c: &mut Client, parts: &[Part]) -> Result<Vec<Finding>> {
    let mut found = Vec::new();
    for p in parts {
        if !(p.check)(c)? {
            found.push(Finding {
                code: "schema_part_missing",
                object: p.name.to_string(),
                detail: if p.required {
                    format!(
                        "part {} of the obop schema is not (completely) there",
                        p.name
                    )
                } else {
                    format!(
                        "part {} of the obop schema is not (completely) there; the process                          starts without it and the feature that needs it says so when it is used",
                        p.name
                    )
                },
                // An optional part is a warning: nothing is broken until something needs it.
                error: p.required,
            });
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The statements `Pg::connect` (now `connect_options`) ran before the schema moved here, to prove the move changed
    /// nothing but the qualification.
    const OLD_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS obop_record (
         seq   BIGINT GENERATED ALWAYS AS IDENTITY,
         model TEXT NOT NULL,
         id    TEXT NOT NULL,
         data  JSONB NOT NULL CHECK (jsonb_typeof(data) = 'object'),
         PRIMARY KEY (model, id)
     );
     CREATE INDEX IF NOT EXISTS obop_record_seq ON obop_record (model, seq);
     CREATE INDEX IF NOT EXISTS obop_record_seq_all ON obop_record (seq);
     CREATE TABLE IF NOT EXISTS obop_counter (
         key        TEXT PRIMARY KEY,
         n          BIGINT NOT NULL,
         expires_ms BIGINT NOT NULL
     );
     CREATE TABLE IF NOT EXISTS obop_kv (
         key        TEXT PRIMARY KEY,
         value      TEXT NOT NULL,
         expires_ms BIGINT NOT NULL
     );
     CREATE INDEX IF NOT EXISTS obop_kv_expiry ON obop_kv (expires_ms);";

    fn squeeze(s: &str) -> String {
        s.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn the_base_part_is_the_old_schema_with_the_schema_filled_in() {
        let base = &parts()[0];
        assert_eq!(base.name, "base");
        let sql = (base.render)("public");
        assert!(!sql.contains('{'), "a placeholder is left: {sql}");
        assert_eq!(squeeze(&sql.replace("public.", "")), squeeze(OLD_SCHEMA));
        // Another schema goes into every table reference and into nothing else.
        let other = (base.render)("\"my schema\"");
        assert_eq!(other.matches("\"my schema\".").count(), 6);
        assert!(!other.contains("public"));
    }

    /// The tables and indexes a part's SQL creates, by name, read from its statements: `CREATE
    /// TABLE IF NOT EXISTS schema.name (` and `CREATE INDEX IF NOT EXISTS name ON`. Panics on a
    /// statement it cannot read, so that a new kind of statement has to teach this test about it.
    fn created_by(sql: &str) -> Vec<String> {
        let mut out = Vec::new();
        for stmt in sql.split(';') {
            let words: Vec<&str> = stmt.split_whitespace().collect();
            if words.is_empty() {
                continue;
            }
            let is = |i: usize, w: &str| words.get(i) == Some(&w);
            if is(0, "CREATE") && is(1, "TABLE") && is(2, "IF") && is(3, "NOT") && is(4, "EXISTS") {
                // The index of a primary key is made with its table and is not listed apart.
                out.push(words[5].rsplit('.').next().unwrap().to_string());
            } else if is(0, "CREATE") && is(1, "INDEX") && is(2, "IF") && is(3, "NOT") {
                out.push(words[5].to_string());
            } else {
                panic!("a statement this test cannot read: {stmt}");
            }
        }
        out.sort();
        out
    }

    #[test]
    fn what_a_part_checks_is_exactly_what_it_creates() {
        // Both directions: an object the SQL creates and the check does not list would be
        // reported as complete when it is missing; one the check lists and the SQL does not
        // create could never be repaired.
        let sorted = |names: &[&str]| {
            let mut v: Vec<String> = names.iter().map(|s| (*s).to_string()).collect();
            v.sort();
            v
        };
        for (part, listed) in [(&parts()[0], &BASE_OBJECTS[..]), (&BLOB, &BLOB_OBJECTS[..])] {
            assert_eq!(
                sorted(listed),
                created_by(&(part.render)("public")),
                "part {}",
                part.name
            );
        }
        // And the reading sees a table or an index somebody adds to the SQL.
        let more =
            format!("{BASE_SQL} CREATE INDEX IF NOT EXISTS zz_new ON {{schema}}.obop_kv (key);");
        let read = created_by(&more.replace("{schema}", "public"));
        assert!(read.contains(&"zz_new".to_string()));
        assert_ne!(read, sorted(&BASE_OBJECTS));
    }

    #[test]
    fn the_blob_part_is_the_old_table_with_the_schema_filled_in() {
        const OLD: &str = "CREATE TABLE IF NOT EXISTS obop_blob_data (
                     key  TEXT NOT NULL,
                     seq  INTEGER NOT NULL,
                     data BYTEA NOT NULL,
                     PRIMARY KEY (key, seq)
                 );";
        let sql = (BLOB.render)("public");
        assert!(!sql.contains('{'));
        assert_eq!(squeeze(&sql.replace("public.", "")), squeeze(OLD));
        assert_eq!(BLOB.name, "blob");
        // It is not part of the record store: the store does not require it.
        assert!(parts().iter().all(|p| p.name != "blob"));
        assert_ne!(BLOB.digest(), parts()[0].digest());
    }

    #[test]
    fn the_digest_pins_the_sql_of_a_part() {
        let d = parts()[0].digest();
        assert_eq!(d.len(), 64);
        assert!(
            d.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase())
        );
        // The digest of the schema "public", whatever schema a process renders for. If this
        // fails, the SQL of the base part changed: existing databases do not get the change
        // unless a ledger step installs it, so add the step and update this value together.
        assert_eq!(
            d, "d440f7fca3230346aa1befb8598026cb38dcc9ecf3dfb403f7d5f2e5c96292f6",
            "digest of the base part"
        );
    }
}
