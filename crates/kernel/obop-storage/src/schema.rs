//! The PostgreSQL schema as parts (K13): the DDL text lives here and nowhere else, so that what a
//! process may do at start-up (create what is missing, or only check), what an owner installs and
//! what an inspection compares against are one definition.
//!
//! A [`Part`] is one group of objects: how to create it (`render`, idempotent SQL with a
//! `{schema}` placeholder), how to tell that it is there (`check`, a catalogue lookup that issues
//! no DDL) and a digest of the SQL it renders (see [`Part::digest`]), which a migration step
//! pins so that edited SQL cannot reach existing databases unannounced. Today there is one part,
//! `base`, the tables of the record store; the record history, the guards and the query support
//! of waves D75 to D78 each add a part.
//!
//! Who calls what:
//!
//! - `Pg::connect` calls [`ensure`] inside the `obop-schema` advisory lock, so that processes
//!   that start together do not trip over each other: with [`Ddl::IfMissing`] a part that is not
//!   there is created, with [`Ddl::Never`] a part that is not there is an error and no DDL is
//!   issued (a serving role that may not create tables).
//! - [`owner_install`] / [`install_all`] create every part unconditionally, as the role that owns
//!   the objects (tests, the control plane, a migration step).
//! - [`inspect`] compares a live database with the parts and reports what is wrong.

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
    /// connecting.
    pub check: fn(&mut Client) -> bool,
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

/// Are all `names` relations the server finds (through the search path)?
fn present(c: &mut Client, names: &[&str]) -> bool {
    c.query_one(
        "SELECT bool_and(to_regclass(n) IS NOT NULL) FROM unnest($1::text[]) AS t(n)",
        &[&names],
    )
    .ok()
    .and_then(|r| r.get::<_, Option<bool>>(0))
    .unwrap_or(false)
}

fn base_present(c: &mut Client) -> bool {
    present(c, &BASE_OBJECTS)
}

/// The parts of the schema, in the order they are created.
pub fn parts() -> &'static [Part] {
    static PARTS: [Part; 1] = [Part {
        name: "base",
        render: render_base,
        check: base_present,
    }];
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
/// both create it.
pub fn ensure(c: &mut Client, ddl: Ddl) -> Result<()> {
    ensure_parts(c, parts(), ddl)
}

pub(crate) fn ensure_parts(c: &mut Client, parts: &[Part], ddl: Ddl) -> Result<()> {
    let missing: Vec<&Part> = parts.iter().filter(|p| !(p.check)(c)).collect();
    if missing.is_empty() {
        return Ok(());
    }
    match ddl {
        Ddl::Never => Err(format!(
            "the database does not have the obop schema (missing: {}), and this process may not \
             create it (OBOP_PRODUCTION): create it first with a role that may create tables",
            missing
                .iter()
                .map(|p| p.name)
                .collect::<Vec<_>>()
                .join(", ")
        )),
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

/// Create every part, whether or not it is there (the SQL is idempotent). For the role that owns
/// the objects; the caller holds the schema lock (see [`owner_install`]).
pub fn install_all(c: &mut Client) -> Result<()> {
    let schema = current_schema(c)?;
    for p in parts() {
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

/// Compare a live database with the parts and report what differs. Today that is whether each
/// part is there; roles, triggers, functions and indexes join as their parts do.
pub fn inspect(c: &mut Client) -> Vec<Finding> {
    inspect_parts(c, parts())
}

pub(crate) fn inspect_parts(c: &mut Client, parts: &[Part]) -> Vec<Finding> {
    parts
        .iter()
        .filter(|p| !(p.check)(c))
        .map(|p| Finding {
            code: "schema_part_missing",
            object: p.name.to_string(),
            detail: format!(
                "part {} of the obop schema is not (completely) there",
                p.name
            ),
            error: true,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The statements `Pg::connect` ran before the schema moved here, to prove the move changed
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

    #[test]
    fn what_the_base_part_checks_is_what_it_creates() {
        let sql = (parts()[0].render)("public");
        for name in BASE_OBJECTS {
            assert!(
                sql.contains(&format!("EXISTS {name} "))
                    || sql.contains(&format!("EXISTS public.{name} "))
                    || sql.contains(&format!("INDEX IF NOT EXISTS {name} ")),
                "{name} is checked but not created"
            );
        }
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
