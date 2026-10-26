//! `bulk_create` (D70): many records of one model in one call, each row checked exactly as
//! `create` checks a record, written in one storage transaction.
//!
//! - *Rows* come as a list of objects (field name to value, JSON types) or as `csv` text (a
//!   header of field names, cells converted by the field's kind). At most `MAX_BULK_ROWS` rows.
//! - *Checks per row* are those of `create`: the governed fields start at their first option,
//!   computed and numbered fields cannot be set, kinds, required fields, references the caller
//!   may see, sequences, unique values (also among the rows of the same call), the row-level
//!   rule that a creator may not make a record they could not then see. Image and file fields
//!   are not accepted: a blob is uploaded and claimed by the caller, one record at a time.
//! - *Modes.* `all_or_nothing` (default): any bad row writes nothing. `partial`: the good rows
//!   are written and the bad ones reported with their row number.
//! - *Dry run* does all the checking, including numbering, and discards the unit.
//! - *Idempotency.* With an `idempotency_key` a retried call returns the first call's answer
//!   instead of creating the rows again. The answer is stored in the same transaction as the
//!   rows (a record in `bulk_idempotency`, valid 24 hours), so it exists exactly when the rows
//!   do. A key used for a different request is refused.
//! - *Rate limit.* A person may start `OBOP_BULK_PER_MINUTE` calls a minute (default 12, `0`
//!   for no limit), dry runs included; the counter is shared by all server processes.

use super::access::{Gate, creator_blocked};
use super::crud::{check_fields, default_governed, own_default, system_error, to_fields};
use super::effects::lookup_fn;
use super::seq;
use super::unit::{Mark, R, Unit};
use super::{Actor, Engine};
use crate::Value;
use crate::csv;
use crate::guards::create_guard;
use crate::kinds::{self, Path};
use crate::store::declared_fields;
use obop_registry::{FieldKind, MAX_BULK_ROWS, ModelDef};
use obop_storage::{Fields, Op};
use serde_json::Value as J;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Table of the stored answers of calls that carried an idempotency key.
pub const IDEMPOTENCY_TABLE: &str = "bulk_idempotency";
/// How long a retried call gets the first answer.
const IDEMPOTENCY_SECS: u64 = 24 * 3600;
const MAX_KEY_CHARS: usize = 128;
/// Errors listed in an answer; `failed` always counts them all.
const MAX_LISTED_ERRORS: usize = 100;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    AllOrNothing,
    Partial,
}

impl Mode {
    fn name(self) -> &'static str {
        match self {
            Mode::AllOrNothing => "all_or_nothing",
            Mode::Partial => "partial",
        }
    }
}

/// One row as it arrived: its place in the input and either the fields or why it is unusable.
struct Incoming {
    /// 1 based position among the rows.
    row: usize,
    /// The line of the CSV file the row starts on.
    line: Option<usize>,
    fields: Result<BTreeMap<String, Value>, String>,
}

fn per_minute() -> u64 {
    std::env::var("OBOP_BULK_PER_MINUTE")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(12)
}

fn text<'a>(m: &'a BTreeMap<String, Value>, k: &str) -> Option<&'a str> {
    match m.get(k) {
        Some(Value::Text(t)) => Some(t),
        _ => None,
    }
}

/// One cell of a CSV file as the field's kind wants it. `None`: the cell is empty, the field is
/// left out (a default or "required" decides).
fn cell(def: &ModelDef, name: &str, raw: &str) -> Result<Option<Value>, String> {
    let f = def
        .fields
        .iter()
        .find(|f| f.name == name)
        .ok_or_else(|| format!("unknown field {}.{name}", def.name))?;
    let t = raw.trim();
    if t.is_empty() {
        // Spaces are not "nothing" in a decimal or datetime column: the strict parser refuses
        // them, as it does everywhere else (D72).
        if !raw.is_empty() && matches!(f.kind, FieldKind::Decimal { .. } | FieldKind::Datetime) {
            return Ok(Some(Value::Text(raw.to_string())));
        }
        return Ok(None);
    }
    let bad = |what: &str| format!("{}.{name} must be {what}, not {t:?}", def.name);
    Ok(Some(match &f.kind {
        FieldKind::Text | FieldKind::LongText => Value::Text(raw.to_string()),
        FieldKind::Number => Value::Int(t.parse().map_err(|_| bad("a whole number"))?),
        FieldKind::Money => Value::Int(t.parse().map_err(|_| {
            bad("a whole number of minor currency units such as cents, without a decimal point")
        })?),
        FieldKind::Bool => match t.to_ascii_lowercase().as_str() {
            "true" | "yes" | "1" => Value::Bool(true),
            "false" | "no" | "0" => Value::Bool(false),
            _ => return Err(bad("true or false")),
        },
        FieldKind::Date | FieldKind::Select(_) | FieldKind::Ref(_) => Value::Text(t.to_string()),
        // A decimal or an instant stays text, exactly as the file has it (spaces included, so
        // they are refused as everywhere else); `create_row` checks it, and rounds it when the
        // field declares a mode, with the very code every other write uses (D72).
        FieldKind::Decimal { .. } | FieldKind::Datetime => Value::Text(raw.to_string()),
        FieldKind::Image(_) | FieldKind::File(_) => {
            return Err(blob_error(def, name));
        }
    }))
}

fn blob_error(def: &ModelDef, name: &str) -> String {
    format!(
        "{}.{name} is an image or file; upload it and attach it with create or update, bulk_create does not take those",
        def.name
    )
}

/// The header of a CSV file must name fields a caller may set.
fn check_header(def: &ModelDef, headers: &[String]) -> R<()> {
    for h in headers {
        if h == "id" {
            return Err(
                "the column \"id\" cannot be imported: ids are assigned by the system, and references are written as the id of the record they point to".into(),
            );
        }
        let Some(f) = def.fields.iter().find(|f| f.name == h) else {
            return Err(format!(
                "unknown column {h:?}: the columns must be field names of {} ({})",
                def.name,
                def.fields
                    .iter()
                    .filter(|f| f.computed.is_none() && f.sequence.is_none())
                    .map(|f| f.name)
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        };
        if def.is_system_field(h) {
            return Err(system_error(def, h));
        }
        if matches!(f.kind, FieldKind::Image(_) | FieldKind::File(_)) {
            return Err(blob_error(def, h));
        }
    }
    Ok(())
}

/// The input as rows: a list of objects, or the converted lines of the CSV text.
fn incoming(def: &ModelDef, m: &BTreeMap<String, Value>) -> R<(Vec<Incoming>, bool)> {
    match (m.get("rows"), m.get("csv")) {
        (Some(_), Some(_)) => Err("give rows or csv, not both".into()),
        (None | Some(Value::Null), None | Some(Value::Null)) => {
            Err("give rows (a list of objects) or csv (the text of a CSV file)".into())
        }
        (Some(Value::List(rows)), _) => {
            if rows.len() > MAX_BULK_ROWS {
                return Err(format!(
                    "at most {MAX_BULK_ROWS} rows per call (got {}); split the work into several calls",
                    rows.len()
                ));
            }
            Ok((
                rows.iter()
                    .enumerate()
                    .map(|(i, r)| Incoming {
                        row: i + 1,
                        line: None,
                        fields: match r {
                            Value::Map(f) => Ok(f.clone()),
                            _ => Err("a row must be an object of field names and values".into()),
                        },
                    })
                    .collect(),
                false,
            ))
        }
        (Some(_), _) => Err("rows must be a list of objects".into()),
        (_, Some(Value::Text(t))) => {
            let table = csv::parse(t.as_bytes(), MAX_BULK_ROWS)?;
            check_header(def, &table.headers)?;
            Ok((
                table
                    .rows
                    .iter()
                    .zip(&table.lines)
                    .enumerate()
                    .map(|(i, (cells, line))| {
                        let mut f = BTreeMap::new();
                        let mut err = None;
                        for (h, c) in table.headers.iter().zip(cells) {
                            match cell(def, h, c) {
                                Ok(Some(v)) => {
                                    f.insert(h.clone(), v);
                                }
                                Ok(None) => {}
                                Err(e) => {
                                    err = Some(e);
                                    break;
                                }
                            }
                        }
                        Incoming {
                            row: i + 1,
                            line: Some(*line),
                            fields: err.map_or(Ok(f), Err),
                        }
                    })
                    .collect(),
                true,
            ))
        }
        (_, Some(_)) => Err("csv must be the text of a CSV file".into()),
    }
}

fn fingerprint(model: &str, mode: Mode, m: &BTreeMap<String, Value>) -> String {
    let payload = match (m.get("rows"), m.get("csv")) {
        (Some(rows), _) if !matches!(rows, Value::Null) => format!("rows\n{}", rows.to_json()),
        (_, Some(Value::Text(t))) => format!("csv\n{t}"),
        _ => String::new(),
    };
    let mut h = Sha256::new();
    h.update(format!("{model}\n{}\n{payload}", mode.name()).as_bytes());
    hex(&h.finalize())
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn idempotency_id(model: &str, actor: &Actor, key: &str) -> String {
    let mut h = Sha256::new();
    h.update(format!("{model}\0{}\0{key}", actor.id).as_bytes());
    hex(&h.finalize())
}

/// One row, checked and added to `unit` exactly as `create` does it.
fn create_row(
    eng: &Engine,
    def: &Arc<ModelDef>,
    look: &dyn Fn(&str) -> Option<Arc<ModelDef>>,
    actor: &Actor,
    unit: &mut Unit<'_>,
    row: &BTreeMap<String, Value>,
) -> R<String> {
    if row.contains_key("id") {
        return Err("id is assigned by the system and cannot be set".into());
    }
    for f in &def.fields {
        if matches!(f.kind, FieldKind::Image(_) | FieldKind::File(_))
            && !matches!(row.get(f.name), None | Some(Value::Null))
        {
            return Err(blob_error(def, f.name));
        }
    }
    let mut row = Value::Map(row.clone());
    // Governed fields start at their first option, as the guard of `create` decides.
    if let Some(g) = create_guard(def) {
        let nobody = crate::Principal::user("", &[]);
        if let Some(v) = g(&nobody, &row)?.input {
            row = v;
        }
    }
    let given = declared_fields(def, &row)?;
    if let Some(k) = given.keys().find(|k| def.is_system_field(k)) {
        return Err(system_error(def, k));
    }
    let mut f = to_fields(&given);
    // An import rounds a decimal that has too many fractional digits when its field declares
    // how (`round`); a create never does.
    kinds::canonicalise(def, &mut f, Path::Import)?;
    default_governed(def, &mut f);
    own_default(def, &mut f, actor);
    check_fields(unit, look, actor, def, &f, true)?;
    seq::assign(unit, def, &mut f, None, eng.now())?;
    if let Some(why) = creator_blocked(&Gate::new(unit, look, actor), def, &f) {
        return Err(why);
    }
    let id = uuid::Uuid::now_v7().to_string();
    unit.insert(def.name, &id, f)?;
    seq::check_unique_keys(unit, look, &[(def.name.to_string(), id.clone())])?;
    Ok(id)
}

fn now_secs(eng: &Engine) -> u64 {
    eng.now().max(0) as u64
}

pub(crate) fn bulk_create(eng: &Engine, def: &Arc<ModelDef>, input: &Value) -> R<Value> {
    let actor = Actor::from_input(input);
    let Value::Map(m) = input else {
        return Err("expected an object".into());
    };
    for k in m.keys() {
        let known = matches!(
            k.as_str(),
            "rows" | "csv" | "mode" | "dry_run" | "idempotency_key"
        );
        if !known && !k.starts_with('_') {
            return Err(format!(
                "unknown input {k:?}: bulk_create takes rows or csv, mode, dry_run and idempotency_key"
            ));
        }
    }
    let mode = match text(m, "mode") {
        None | Some("all_or_nothing") => Mode::AllOrNothing,
        Some("partial") => Mode::Partial,
        Some(other) => {
            return Err(format!(
                "mode must be all_or_nothing or partial, not {other:?}"
            ));
        }
    };
    let dry_run = match m.get("dry_run") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(_) => return Err("dry_run must be true or false".into()),
    };
    let key = match m.get("idempotency_key") {
        None | Some(Value::Null) => None,
        Some(Value::Text(k))
            if !k.is_empty()
                && k.chars().count() <= MAX_KEY_CHARS
                && k.chars().all(|c| c.is_ascii_graphic()) =>
        {
            Some(k.clone())
        }
        Some(_) => {
            return Err(format!(
                "idempotency_key must be 1 to {MAX_KEY_CHARS} visible ASCII characters"
            ));
        }
    };
    let (rows, from_csv) = incoming(def, m)?;

    // A person may start only so many calls a minute, whatever their size.
    let limit = per_minute();
    if limit > 0 {
        let n = eng
            .backend()
            .counter_add(&format!("bulk:{}", actor.id), 60_000, false)?;
        if n > limit {
            return Err(format!(
                "too many bulk imports: at most {limit} a minute; wait a minute and retry"
            ));
        }
    }

    let fp = fingerprint(def.name, mode, m);
    let idem = key
        .as_ref()
        .filter(|_| !dry_run)
        .map(|k| idempotency_id(def.name, &actor, k));
    if let Some(id) = &idem
        && let Some(stored) = eng.backend().get(IDEMPOTENCY_TABLE, id)?
        && stored.get("expires_at").and_then(J::as_u64).unwrap_or(0) > now_secs(eng)
    {
        if stored.get("fingerprint").and_then(J::as_str) != Some(fp.as_str()) {
            return Err(
                "this idempotency_key was already used for a different request; use a new key for different rows".into(),
            );
        }
        let mut first: J = stored
            .get("result")
            .and_then(J::as_str)
            .and_then(|s| serde_json::from_str(s).ok())
            .ok_or("the stored answer for this idempotency_key cannot be read")?;
        first["replayed"] = J::Bool(true);
        return Ok(Value::from_json(&first));
    }

    let look = lookup_fn(eng);
    let mut unit = Unit::with_cap(eng.backend(), MAX_BULK_ROWS * 4);
    let mut ids: Vec<Option<String>> = Vec::with_capacity(rows.len());
    let mut errors: Vec<(usize, Option<usize>, String)> = Vec::new();
    let mut failed = 0usize;
    for r in &rows {
        let mark: Mark = unit.mark();
        let done = match &r.fields {
            Ok(f) => create_row(eng, def, &look, &actor, &mut unit, f),
            Err(e) => Err(e.clone()),
        };
        match done {
            Ok(id) => ids.push(Some(id)),
            Err(e) => {
                unit.rewind(mark);
                ids.push(None);
                failed += 1;
                if errors.len() < MAX_LISTED_ERRORS {
                    errors.push((r.row, r.line, e));
                }
            }
        }
    }
    let total = rows.len();
    let created = total - failed;
    let ok = failed == 0;

    if !ok && mode == Mode::AllOrNothing && !dry_run {
        let first: Vec<String> = errors
            .iter()
            .take(5)
            .map(|(row, line, e)| match line {
                Some(l) => format!("row {row} (line {l}): {e}"),
                None => format!("row {row}: {e}"),
            })
            .collect();
        return Err(format!(
            "{failed} of {total} row(s) are invalid, so nothing was created. {}{}",
            first.join("; "),
            if failed > first.len() {
                format!(
                    "; and {} more (use dry_run for the full list)",
                    failed - first.len()
                )
            } else {
                String::new()
            }
        ));
    }

    let write = !dry_run && !(mode == Mode::AllOrNothing && !ok);
    let to_value = |replayed: bool| {
        let errs = errors
            .iter()
            .map(|(row, line, e)| {
                let mut o = BTreeMap::new();
                o.insert("row".to_string(), Value::Int(*row as i64));
                if let Some(l) = line {
                    o.insert("line".to_string(), Value::Int(*l as i64));
                }
                o.insert("error".to_string(), Value::text(e));
                Value::Map(o)
            })
            .collect();
        let mut o = BTreeMap::new();
        o.insert("ok".to_string(), Value::Bool(ok));
        o.insert("dry_run".to_string(), Value::Bool(dry_run));
        o.insert("mode".to_string(), Value::text(mode.name()));
        o.insert(
            "source".to_string(),
            Value::text(if from_csv { "csv" } else { "rows" }),
        );
        o.insert("total".to_string(), Value::Int(total as i64));
        o.insert(
            if dry_run { "valid" } else { "created" }.to_string(),
            Value::Int(created as i64),
        );
        o.insert("failed".to_string(), Value::Int(failed as i64));
        o.insert("errors".to_string(), Value::List(errs));
        o.insert(
            "errors_truncated".to_string(),
            Value::Bool(failed > errors.len()),
        );
        if !dry_run {
            o.insert(
                "ids".to_string(),
                Value::List(
                    ids.iter()
                        .map(|i| i.as_deref().map_or(Value::Null, Value::text))
                        .collect(),
                ),
            );
        }
        o.insert("replayed".to_string(), Value::Bool(replayed));
        Value::Map(o)
    };
    let answer = to_value(false);

    if write {
        if let Some(id) = &idem {
            let mut rec = Fields::new();
            rec.insert("fingerprint".into(), J::from(fp));
            rec.insert("result".into(), J::from(answer.to_json().to_string()));
            rec.insert(
                "expires_at".into(),
                J::from(now_secs(eng) + IDEMPOTENCY_SECS),
            );
            unit.push_op(Op::Put {
                model: IDEMPOTENCY_TABLE.into(),
                id: id.clone(),
                fields: rec,
            });
        }
        let touched = unit.touched();
        unit.commit_checked()
            .map_err(|e| seq::explain_db_error(e, &touched, &look))?;
        purge_expired(eng);
    }
    Ok(answer)
}

/// Forget stored answers that expired; at most once every ten minutes across all processes.
fn purge_expired(eng: &Engine) {
    let first = eng
        .backend()
        .counter_add("bulk.purge", 600_000, false)
        .is_ok_and(|n| n == 1);
    if !first {
        return;
    }
    let now = now_secs(eng);
    for (id, rec) in eng.backend().list(IDEMPOTENCY_TABLE).unwrap_or_default() {
        if rec.get("expires_at").and_then(J::as_u64).unwrap_or(0) <= now {
            let _ = eng.backend().delete(IDEMPOTENCY_TABLE, &id);
        }
    }
}
