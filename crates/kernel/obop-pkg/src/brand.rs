//! The model `instance_brand` of the Customize module and its three commands (docs/spec/branding.md,
//! PROPOSED, sections 2 and 3; decision D94 is reserved, no ruling number is assigned): the name,
//! the slug, the logo, a title and a message per language of one instance, in one row with the id
//! `instance`. No row means no brand.
//!
//! **Hand-registered, like `sso_provider`.** A command exists on the REST, MCP and CLI surfaces only
//! where a handler is bound, so the model has `get`, `configure` and `clear` and nothing else: the
//! generated `create`, `update`, `delete`, `bulk_create` (and with it `obop import`), `list` and
//! `search` are not bound and answer as unknown commands, and, because the model has no `list`,
//! `/api/meta` and the authenticated OpenAPI document do not list it.
//!
//! **Who.** `get` is open to any signed-in principal (every field has the group `*`). `configure` and
//! `clear` need `platform.admin` and a person: one `Dispatcher::around` wrapper on each refuses unless
//! `Principal::is_person()`, which also stops a scoped token, an agent token (as a call and as an
//! approved proposal) and a mobile app's token, which keeps its owner's plain id and has scopes set.
//! The commands declare no approval, so an agent is refused outright, not given a plan.
//!
//! **One rule.** Both writing commands call `obop_commands::brand::apply`, which merges by supplied
//! field, validates with the whole character policy, takes the lock `brand.write` and writes the row
//! with its audit events in one atomic batch. (The command line, `obop admin brand`, calls it too,
//! as the operator and with no reader.) This module only turns a call into its arguments and its
//! result into the declared answer fields: the input keys are checked by name here (a key that is
//! not a writable field or `dry_run` is refused, and so is a value that is not text), because a
//! hand-registered command has the generic object schema on REST and MCP. The keys the dispatcher
//! adds itself (`_caller` and the actor-prefixed ones) pass; no other key that starts with `_`
//! does.
//!
//! **The answers are declared fields.** A key of an answer that is not a declared field of the model
//! is withheld on every surface, so the keys of the three answers (`dry_run`, `set`, `v`,
//! `effective`, `fallbacks`, `changes`, `refusals`, `cleared`) are fields of the model; the registry
//! has no object or list kind, so a structured part is a text field that holds JSON.
//!
//! The descriptions say what the record holds, not which pages show it.

use obop_commands::brand::{self, Action, Applied, Reader, fallbacks_json, show_key};
use obop_commands::{ADMIN_GROUP, ANY_GROUP, Around, Dispatcher, Value};
use obop_registry::{CommandDef, FieldDef, FieldKind, ModelDef, Module, intern};
use obop_storage::Backend;
use obop_types::DataClass;
use std::collections::BTreeMap;
use std::sync::Arc;

/// The languages of the record, with the name each is given in the English texts of the model, in
/// the order of the stored fields. A test ties the codes to the list of the OAuth pages
/// (`obop_oauth::page::LANGS`), so an eighth language is a deliberate change of the model and of the
/// 56 label cells of each language.
const LANGUAGES: [(&str, &str); 7] = [
    ("en", "English"),
    ("de", "German"),
    ("fr", "French"),
    ("pl", "Polish"),
    ("fi", "Finnish"),
    ("sv", "Swedish"),
    ("es", "Spanish"),
];

const PUBLIC: &str = "Public by design, so it holds no personal data";

const CONFIGURE: &str = "Set the instance branding. Give only the fields to change (the others keep their value); the first call needs name. Fields: name, slug, title_<lang>, message_<lang>, logo_svg (SVG text). An empty text clears a field, except name. dry_run (true or false) writes nothing. Answers changes and v (with dry_run also refusals). Only a person with platform.admin may run it";

const CLEAR: &str = "Remove the instance branding: the one record is deleted, so no brand is set. dry_run (true or false) checks and removes nothing. Answers cleared: true when a brand was (with dry_run, would be) removed, false with none, and nothing is written then. Only a person with platform.admin may run it";

/// Registry metadata of the model: 17 stored fields (`name`, `slug`, `title_<lang>` and
/// `message_<lang>` for seven languages, `logo_svg`) and the 8 declared fields of the answers. Every
/// field has the data class `none` (explicitly not personal data) and the group `*`: who may write is
/// decided by the commands, and the stored values are public by design.
pub fn instance_brand_model() -> ModelDef {
    let f = |name: &'static str, description: &str| {
        FieldDef::new(name, intern(description), DataClass::None, false, ANY_GROUP)
    };
    let mut fields = vec![
        f(
            "name",
            &format!(
                "The product name, 1 to 40 characters (letters, digits, spaces and - . , & ' + _ ( ) !). Required: a brand without a name is not stored. {PUBLIC}"
            ),
        ),
        f(
            "slug",
            &format!(
                "A short identifier: a-z, 0-9 and -, starting with a letter, not ending with -, at most 30 characters. Empty means derived from the name. {PUBLIC}"
            ),
        ),
    ];
    for (code, language) in LANGUAGES {
        let rest = if code == "en" {
            "Empty means the name. It is also the title of every language that has none of its own"
        } else {
            "Empty means the English title, then the name"
        };
        fields.push(f(
            intern(&format!("title_{code}")),
            &format!(
                "The title in {language}: the line beside the logo, at most 60 characters. {rest}. {PUBLIC}"
            ),
        ));
    }
    for (code, language) in LANGUAGES {
        let rest = if code == "en" {
            "Empty means none. It is also the message of every language that has none of its own"
        } else {
            "Empty means the English message, then none"
        };
        fields.push(f(
            intern(&format!("message_{code}")),
            &format!(
                "The message in {language}: one plain text of at most 240 characters, with no links or markup. {rest}. {PUBLIC}"
            ),
        ));
    }
    fields.push(
        f(
            "logo_svg",
            "Input of configure: the text of an SVG file, at most 16 KiB, a strict subset (shapes and paths in plain colours; no script, text, image or link). What is stored is the canonical text the server re-wrote, never the input; get does not return it. Empty clears it, and the logo is then drawn from the name",
        )
        .kind(FieldKind::LongText),
    );
    // The declared fields of the answers (and the input `dry_run`).
    fields.extend([
        f(
            "dry_run",
            "Input of configure and clear: true or false. With true everything is checked and nothing is written or logged; the answer says what would happen, and echoes dry_run",
        )
        .kind(FieldKind::Bool),
        f(
            "set",
            "Answer of get: whether a brand is set, that is whether the record exists",
        )
        .kind(FieldKind::Bool),
        f(
            "v",
            "Answer of get and configure: the version of the logo (16 hex digits), which changes when the logo does; empty when no brand is set",
        ),
        f(
            "effective",
            "Answer of get: a JSON object with, for each value in force (name, slug, logo, tile_accent, authenticator_issuer, title_<lang> and message_<lang>), its value and the source that won: record, env, theme or default",
        ),
        f(
            "fallbacks",
            "Answer of get: a JSON list of the stored fields that failed the read check and fell back to a default, each as field and reason",
        ),
        f(
            "changes",
            "Answer of configure: a JSON list of the fields that changed (with dry_run, that would), each as field, old and new; the artwork as its old and new version",
        ),
        f(
            "refusals",
            "Answer of configure with dry_run: a JSON list of what a real call would refuse, each as field and rule",
        ),
        f(
            "cleared",
            "Answer of clear: whether a brand was removed (with dry_run, whether it would be)",
        )
        .kind(FieldKind::Bool),
    ]);
    ModelDef {
        name: brand::MODEL,
        description: "The instance branding: the name, the slug, the logo and a title and a message per language. One record, public by design, changed only by a person with platform.admin and never by an agent",
        read_group: ANY_GROUP,
        write_group: ADMIN_GROUP,
        access: None,
        fields,
        // `get` is the generated command of the read group, bound below with its own text.
        commands: vec![
            CommandDef::new("configure", CONFIGURE, ADMIN_GROUP),
            CommandDef::new("clear", CLEAR, ADMIN_GROUP),
        ],
        ext: Default::default(),
    }
}

type Input = BTreeMap<String, Value>;

fn object(v: &Value) -> Result<&Input, String> {
    match v {
        Value::Map(m) => Ok(m),
        _ => Err("expected an object".into()),
    }
}

/// A key the dispatcher puts into the input itself; a client cannot send it.
fn dispatcher_key(k: &str) -> bool {
    k == "_caller" || k.starts_with(obop_commands::query::ACTOR_PREFIX)
}

/// A bool input: the JSON booleans and the texts `true` and `false` (`obop call` types an input from
/// `/api/meta`, this model is not in it, and so it sends text). Absent or null is false.
fn flag(x: &Input, key: &str) -> Result<bool, String> {
    match x.get(key) {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(b)) => Ok(*b),
        Some(Value::Text(t)) if t == "true" => Ok(true),
        Some(Value::Text(t)) if t == "false" => Ok(false),
        Some(_) => Err(format!("instance_brand: {key} must be true or false")),
    }
}

/// The input keys that are not accepted, each by name (as much of it as is safe to repeat).
fn unknown_keys(x: &Input, command: &str, accepted: &dyn Fn(&str) -> bool) -> Vec<String> {
    x.keys()
        .filter(|k| !dispatcher_key(k) && !accepted(k))
        .map(|k| {
            format!(
                "instance_brand.{command}: the key {} is not accepted",
                show_key(k)
            )
        })
        .collect()
}

fn caller(x: &Input) -> Result<&str, String> {
    match x.get("_caller") {
        Some(Value::Text(t)) if !t.is_empty() => Ok(t),
        _ => Err("instance_brand: the caller is not known".into()),
    }
}

fn lines(problems: Vec<String>) -> String {
    problems.join("\n")
}

/// `get`: the stored fields as stored (the artwork is served by the logo route, not returned; a
/// text that failed the read check comes back escaped, `brand::printable`, because `obop call`
/// prints the answer to a terminal), whether a brand is set, the version of the logo, the values in
/// force with their sources and the fields that fell back. It reads the row and the theme itself, every time, through the reader's
/// uncached read: a store that cannot be read is the error, never `set: false`.
fn get(reader: &Reader) -> Result<Value, String> {
    let fresh = reader.fresh()?;
    let shown = fresh.shown();
    let mut m = BTreeMap::new();
    for field in brand::writable_fields() {
        if field != "logo_svg" {
            let value = Value::text(&brand::printable(&field, fresh.stored_text(&field)));
            m.insert(field, value);
        }
    }
    m.insert("set".into(), Value::Bool(fresh.set()));
    m.insert("v".into(), Value::text(shown.v()));
    m.insert("effective".into(), Value::Text(fresh.effective_json()));
    m.insert(
        "fallbacks".into(),
        Value::Text(fallbacks_json(shown.fallbacks())),
    );
    Ok(Value::Map(m))
}

fn applied(a: &Applied, with_changes: bool) -> Value {
    let mut m = BTreeMap::new();
    if with_changes {
        m.insert(
            "changes".to_string(),
            Value::Text(brand::changes_json(&a.changes)),
        );
        m.insert("v".to_string(), Value::text(&a.v));
    } else {
        m.insert("cleared".to_string(), Value::Bool(a.cleared));
    }
    if a.dry_run {
        m.insert("dry_run".to_string(), Value::Bool(true));
        if with_changes {
            m.insert(
                "refusals".to_string(),
                Value::Text(brand::refusals_json(&a.refusals)),
            );
        }
    }
    Value::Map(m)
}

fn configure(backend: &dyn Backend, reader: &Reader, input: &Value) -> Result<Value, String> {
    let x = object(input)?;
    let writable = brand::writable_fields();
    let mut problems = unknown_keys(x, "configure", &|k| {
        k == "dry_run" || writable.iter().any(|f| f == k)
    });
    let mut supplied = Vec::new();
    for field in &writable {
        match x.get(field) {
            None => {}
            Some(Value::Text(t)) => supplied.push((field.clone(), t.clone())),
            Some(_) => problems.push(format!(
                "instance_brand.configure: {field} must be text; an empty text clears it"
            )),
        }
    }
    let dry_run = match flag(x, "dry_run") {
        Ok(b) => b,
        Err(e) => {
            problems.push(e);
            false
        }
    };
    if !problems.is_empty() {
        return Err(lines(problems));
    }
    let actor = caller(x)?;
    brand::apply(
        backend,
        Some(reader),
        actor,
        &Action::Configure(supplied),
        dry_run,
    )
    .map(|a| applied(&a, true))
    .map_err(|f| f.to_string())
}

fn clear(backend: &dyn Backend, reader: &Reader, input: &Value) -> Result<Value, String> {
    let x = object(input)?;
    let mut problems = unknown_keys(x, "clear", &|k| k == "dry_run");
    let dry_run = match flag(x, "dry_run") {
        Ok(b) => b,
        Err(e) => {
            problems.push(e);
            false
        }
    };
    if !problems.is_empty() {
        return Err(lines(problems));
    }
    let actor = caller(x)?;
    brand::apply(backend, Some(reader), actor, &Action::Clear, dry_run)
        .map(|a| applied(&a, false))
        .map_err(|f| f.to_string())
}

/// The one guard of `configure` and `clear`: it refuses unless the caller is a person. It runs when
/// the command really runs, after the permission check and after any approval, and an approved plan
/// runs as the principal that proposed it, so an agent's approved proposal is refused too.
fn person_only() -> Box<Around> {
    Box::new(|p, input, inner| {
        if !p.is_person() {
            return Err("instance_brand: the branding is public, so it is changed by a person with platform.admin, not by an agent, a scoped token or a phone app".into());
        }
        inner(input)
    })
}

/// Bind the handlers and the guard. Called by `register_admin`, which has created the reader of the
/// dispatcher just before; `m` is the built-in module that holds the model.
pub(crate) fn register(d: &Dispatcher, backend: &Arc<dyn Backend>, m: &Module) {
    let reader = d.brand();
    let r = reader.clone();
    d.register(m, brand::MODEL, "get", Box::new(move |_| get(&r)));
    let (b, r) = (backend.clone(), reader.clone());
    d.register(
        m,
        brand::MODEL,
        "configure",
        Box::new(move |v| configure(&*b, &r, v)),
    );
    let (b, r) = (backend.clone(), reader);
    d.register(
        m,
        brand::MODEL,
        "clear",
        Box::new(move |v| clear(&*b, &r, v)),
    );
    for command in ["configure", "clear"] {
        d.around(&format!("{}.{command}", brand::MODEL), person_only());
    }
}

/// The fields of the `[record]` table of a values file, in the order of the writable fields of the
/// record, or every reason the file is refused. This is the reader of the `--from FILE` of `obop
/// admin brand configure` (spec, section 3; `crates/obop/src/brand_cmd.rs` calls it). One file
/// holds the record's values and, in a `[phone]` table that only the generator of the phone app
/// reads, the phone-only values, so the name is stated once.
///
/// It reads `[record]`, whose keys are the writable fields of the record (`name`, `slug`,
/// `title_<lang>`, `message_<lang>`, `logo_svg`) and whose values are text; it ignores a `[phone]`
/// table whatever it holds; it refuses any other table, any key outside a table, any key of
/// `[record]` that is not a writable field (the declared fields of the answers included: `dry_run`,
/// `set`, `v`, `effective`, `fallbacks`, `changes`, `refusals`, `cleared`) and any value that is not
/// text. Each reason names the key or the table and the rule. **No reason holds a value** of the
/// file: a key is repeated only as `show_key` allows, and the message of the TOML parser (which can
/// quote the text) is cut to what is safe to print.
pub fn read_values_file(text: &str) -> Result<Vec<(String, String)>, Vec<String>> {
    let doc: toml::Table = match toml::from_str(text) {
        Ok(t) => t,
        Err(e) => return Err(vec![parse_reason(text, &e)]),
    };
    let mut problems = Vec::new();
    let mut record = None;
    for (key, value) in &doc {
        match (key.as_str(), value) {
            ("record", toml::Value::Table(t)) => record = Some(t),
            ("phone", toml::Value::Table(_)) => {}
            ("record" | "phone", _) => problems.push(format!(
                "[{key}] must be a table; the file has a value of that name outside a table"
            )),
            (other, toml::Value::Table(_)) => problems.push(format!(
                "[{}] is not read: a values file has the tables [record] and [phone] only",
                show_key(other)
            )),
            (other, _) => problems.push(format!(
                "the key {} is outside a table: a values file has the tables [record] and [phone] only",
                show_key(other)
            )),
        }
    }
    let writable = brand::writable_fields();
    let mut fields = BTreeMap::new();
    match record {
        None if problems.is_empty() => problems.push("the file has no [record] table".to_string()),
        None => {}
        Some(table) => {
            for (key, value) in table {
                if !writable.iter().any(|f| f == key) {
                    problems.push(format!(
                        "[record] the key {} is not a field of the brand; the fields are name, slug, title_<lang>, message_<lang> and logo_svg",
                        show_key(key)
                    ));
                    continue;
                }
                match value {
                    toml::Value::String(s) => {
                        fields.insert(key.as_str(), s.clone());
                    }
                    _ => problems.push(format!("[record] {key} must be text (a string)")),
                }
            }
        }
    }
    if !problems.is_empty() {
        return Err(problems);
    }
    Ok(writable
        .iter()
        .filter_map(|f| fields.remove(f.as_str()).map(|v| (f.clone(), v)))
        .collect())
}

/// What is wrong with a document that does not parse: the line and the parser's own message cut
/// to ASCII graphic characters, never the line itself.
fn parse_reason(text: &str, e: &toml::de::Error) -> String {
    let line = e.span().map(|s| {
        text.as_bytes()[..s.start.min(text.len())]
            .iter()
            .filter(|b| **b == b'\n')
            .count()
            + 1
    });
    let message: String = e
        .message()
        .chars()
        .map(|c| {
            if c.is_ascii_graphic() || c == ' ' {
                c
            } else {
                '?'
            }
        })
        .take(120)
        .collect();
    match line {
        Some(n) => format!("the file is not valid TOML (line {n}): {message}"),
        None => format!("the file is not valid TOML: {message}"),
    }
}

#[cfg(test)]
mod tests;
