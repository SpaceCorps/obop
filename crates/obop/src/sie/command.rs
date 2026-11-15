//! The typed commands `sie_exchange.export` and `sie_exchange.import` (REST, MCP and the
//! generated CLI surface), the same functions as `obop sie export|import`.
//!
//! The file travels as base64 in the answer (`content_base64`) and in the input, because no
//! command of the platform returns raw bytes. The export needs `accounting.user`; the import
//! needs `accounting.manager`. An agent may run the import as a dry run (counts, nothing is
//! written) and is refused a real run in words: a plan that a person approves runs while the
//! dispatcher holds its transaction lock, and the import makes many writes of its own through the
//! commands, which would wait for that lock for ever. So the person runs the import (REST as
//! themselves, or `obop sie import`), and posting (`--post`) is the CLI's, as the real post
//! action is a person's own.

use super::{EXPORT_GROUP, Env, IMPORT_GROUP, export, import};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use obop_commands::{Actor, Dispatcher, Principal, Store, Value};
use obop_registry::{CommandDef, FieldDef, FieldKind, ModelDef, Module};
use obop_types::DataClass;
use std::sync::{Arc, Weak};

const MODEL: &str = "sie_exchange";

pub fn module() -> Module {
    let f =
        |name, description| FieldDef::new(name, description, DataClass::None, false, EXPORT_GROUP);
    Module {
        id: "sie".into(),
        name: "SIE".into(),
        description: "SIE 4 export and import of the accounting books (the Swedish interchange format)".into(),
        models: vec![ModelDef {
            name: MODEL,
            description: "SIE 4 files: `export` builds the file of the posted moves of a year; `import` reads a file into accounts, journals and draft moves (series and number kept in `reference`). The file is base64 text of the CP437 bytes. Both need the accounting package. The record layout is not checked against the official SIE specification",
            read_group: EXPORT_GROUP,
            write_group: IMPORT_GROUP,
            access: None,
            fields: vec![
                f("year", "Input for export: the calendar year the fiscal year starts in, for example 2026").kind(FieldKind::Number),
                f("content_base64", "Input for import: the SIE file (CP437 bytes) as base64. Answer of export: the file"),
                f("dry_run", "Input for import: only count what would happen and write nothing").kind(FieldKind::Bool),
                f("default_account_types", "Input for import: accept the built-in table of account types for new accounts (UNVERIFIED defaults)").kind(FieldKind::Bool),
                f("account_types", "Input for import: lines `number,type` that give or override the type of single new accounts"),
                f("filename", "Answer of export: a suggested file name"),
                f("encoding", "Answer of export: the character set of the file (cp437)"),
                f("bytes", "Answer of export: the size of the file").kind(FieldKind::Number),
                f("sha256", "Answer of export: the SHA-256 of the file"),
                f("from", "Answer of export: first day of the exported year"),
                f("to", "Answer of export: last day of the exported year"),
                f("accounts", "Answer of export: how many accounts are in the file").kind(FieldKind::Number),
                f("vouchers", "Answer of export: how many vouchers are in the file").kind(FieldKind::Number),
                f("notes", "Answer: remarks (a list)"),
                f("ok", "Answer of import: whether the run completed and the totals match").kind(FieldKind::Bool),
                f("report", "Answer of import: the report as text"),
                f("problems", "Answer of import: what refused the run (a list)"),
                f("counts", "Answer of import: new and existing accounts, journals, vouchers and lines (an object)"),
            ],
            commands: vec![
                CommandDef::new(
                    "export",
                    "Build a SIE 4 file of the posted moves of one year (give year). Returns content_base64 (CP437 bytes), filename, sha256 and counts",
                    EXPORT_GROUP,
                ),
                CommandDef::new(
                    "import",
                    "Import a SIE 4 file (give content_base64): creates missing accounts (types need default_account_types or account_types), one journal per series and draft moves with the series and number in reference. Idempotent; checks the per account totals against the file. dry_run counts only. An agent may only run it with dry_run; a person runs it for real. Posting is `obop sie import --post`",
                    IMPORT_GROUP,
                ),
            ],
            ext: Default::default(),
        }],
        ..Module::default()
    }
}

fn principal_of(input: &Value) -> Principal {
    let a = Actor::from_input(input);
    let groups: Vec<&str> = a.groups.iter().map(String::as_str).collect();
    Principal::user(&a.caller, &groups)
}

fn field<'a>(input: &'a Value, k: &str) -> Option<&'a Value> {
    match input {
        Value::Map(m) => m.get(k),
        _ => None,
    }
}

fn flag(input: &Value, k: &str) -> bool {
    matches!(field(input, k), Some(Value::Bool(true)))
}

fn texts(v: &[String]) -> Value {
    Value::List(v.iter().map(|s| Value::text(s)).collect())
}

pub fn register(d: &Arc<Dispatcher>, store: &Store) {
    let m = module();
    let (weak, st) = (Arc::downgrade(d), store.clone());
    d.register_with_actor(
        &m,
        MODEL,
        "export",
        Box::new(move |input| {
            let year = match field(input, "year") {
                Some(Value::Int(y)) if (1000..=9999).contains(y) => *y as u16,
                _ => return Err("year is required: a four digit calendar year".into()),
            };
            with_env(&weak, &st, |env| {
                let done = export::export(env, &principal_of(input), year)?;
                Ok(Value::map([
                    ("filename", Value::text(&done.filename)),
                    ("encoding", Value::text("cp437")),
                    ("content_base64", Value::text(&STANDARD.encode(&done.bytes))),
                    ("bytes", Value::Int(done.bytes.len() as i64)),
                    ("sha256", Value::text(&done.sha256)),
                    ("from", Value::text(&done.from)),
                    ("to", Value::text(&done.to)),
                    ("accounts", Value::Int(done.accounts as i64)),
                    ("vouchers", Value::Int(done.vouchers as i64)),
                    ("notes", texts(&done.notes)),
                ]))
            })
        }),
    );
    let (weak, st) = (Arc::downgrade(d), store.clone());
    d.register_with_actor(
        &m,
        MODEL,
        "import",
        Box::new(move |input| {
            let Some(Value::Text(b64)) = field(input, "content_base64") else {
                return Err("content_base64 is required: the SIE file as base64".into());
            };
            let bytes = STANDARD
                .decode(b64.trim())
                .map_err(|e| format!("content_base64 is not base64: {e}"))?;
            if Actor::from_input(input).agent && !flag(input, "dry_run") {
                return Err("an agent may only run the SIE import as a dry run (dry_run=true); ask the person to run it, or to run `obop sie import`".into());
            }
            let opts = import::Options {
                dry_run: flag(input, "dry_run"),
                post: false,
                default_account_types: flag(input, "default_account_types"),
                account_types: match field(input, "account_types") {
                    Some(Value::Text(t)) => Some(t.clone()),
                    _ => None,
                },
                wait_on_rate_limit: false,
            };
            with_env(&weak, &st, |env| {
                let (ok, rep) = match import::import(env, &principal_of(input), &bytes, &opts) {
                    Ok(r) => (true, r),
                    Err(r) => (false, r),
                };
                let c = &rep.counts;
                Ok(Value::map([
                    ("ok", Value::Bool(ok)),
                    ("dry_run", Value::Bool(opts.dry_run)),
                    ("report", Value::text(&rep.to_string())),
                    ("problems", texts(&rep.problems)),
                    (
                        "counts",
                        Value::map([
                            ("accounts_new", Value::Int(c.accounts_new as i64)),
                            ("accounts_matched", Value::Int(c.accounts_matched as i64)),
                            ("journals_new", Value::Int(c.journals_new as i64)),
                            ("vouchers_new", Value::Int(c.vouchers_new as i64)),
                            ("vouchers_already", Value::Int(c.vouchers_already as i64)),
                            ("lines_new", Value::Int(c.lines_new as i64)),
                        ]),
                    ),
                ]))
            })
        }),
    );
}

fn with_env(
    d: &Weak<Dispatcher>,
    store: &Store,
    f: impl FnOnce(Env<'_>) -> Result<Value, String>,
) -> Result<Value, String> {
    let d = d.upgrade().ok_or("the dispatcher is gone")?;
    f(Env { d: &d, store })
}
