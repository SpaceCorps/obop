//! `brand::apply`: the one function that writes the brand row, for `configure` and `clear` on every
//! surface (docs/spec/branding.md, section 3). The dispatcher calls it for both commands. The
//! command line (`obop admin brand`) is not built yet; it will call it too, so that there is one
//! rule:
//!
//! - It takes the cross-process lock `brand.write` itself and does the read, the merge and the write
//!   under it, so two changes at once, from this process or another, cannot lose each other. It does
//!   not rely on the dispatcher's transaction lock: the command line will have no dispatcher, and
//!   `Backend::lock` is not re-entrant.
//! - `configure` merges by supplied field. A field that is not supplied keeps its stored value, also
//!   one this binary does not know (a newer binary's field survives). Each field has an explicit
//!   clear value: the empty text (the artwork too); `name` cannot be cleared, only replaced. The
//!   first call needs a `name`. Every supplied value is checked with the whole character policy (the
//!   artwork by the strict SVG reader, and what is stored is the canonical text it printed, never
//!   the input). Nothing is stored unless every supplied value passes.
//! - It is idempotent by construction: the same values twice change nothing and write no event. That
//!   holds for an artwork stored in a valid form that is not canonical too: it is the same logo, with
//!   the same version `v`, and sending it again writes nothing.
//! - The row and one event per changed field are written in one atomic batch
//!   (`Backend::apply_checked`), so the log has the entry exactly when the change exists.
//! - `dry_run` does every check, writes nothing, logs nothing and reports what would happen.
//! - When a reader is given (the dispatcher passes its own; the command line, once it exists,
//!   passes none) its cell is updated after the batch has committed and never before.
//!
//! **The audit events.** Kind `brand_configured` for `configure` and `brand_cleared` for `clear`;
//! `user` is `(instance)`; the actor is the principal's id (the command line will pass `operator`).
//! The detail is one line that names the field and gives the old and the new value:
//!
//! ```text
//! name: "obop" -> "MOHV"            a text field: both values as quoted text, "" when unset
//! title_sv: "" -> "Logga in"
//! logo_svg: none -> v=0123456789abcdef   the artwork is never written: its version `v` instead,
//!                                        `none` when there is no artwork, `unreadable` when the
//!                                        stored one fails the strict reader
//! name: "MOHV" -> removed           clear: one event per non-empty stored field, with its old value
//! ```
//!
//! A quoted value is the JSON text of the value when it passes the safety subset of the policy; a
//! value written around the command that does not (a control, format or bidirectional character, a
//! space other than U+0020, or too long) is written with every non-ASCII character as `\u{...}`
//! and cut at 300 characters, so a stored value cannot forge or reorder a line of the log. Every
//! field is public by design (data class `none`), so no value is secret.

use super::reader::Reader;
use super::row::{accent_of, read_row, text_of};
use super::{AUDIT_USER, LOCK_KEY, MODEL, ROW_ID, SETTING_ID, SETTING_MODEL, writable_fields};
use crate::audit;
use obop_brand::policy::{self, Kind};
use obop_brand::{logo, slug, svg};
use obop_storage::{Backend, Fields, Op};
use serde_json::{Value as J, json};
use std::collections::BTreeMap;

/// What to do to the row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Merge these fields, `(field name, text)`, over the stored row. A later pair with the same
    /// name wins.
    Configure(Vec<(String, String)>),
    /// Delete the row.
    Clear,
}

/// A value that was refused and the rule it broke.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refused {
    pub field: String,
    pub rule: String,
}

/// A field that changed (or would, with `dry_run`). For the artwork `old` and `new` are the
/// versions of the logo (`v`), empty when there is none, and `unreadable` for a stored artwork the
/// strict reader refuses. For any other field they are the values as stored: the answer carries a
/// value that was written around the command whole, where the log line is cut (`LOG_CUT`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Changed {
    pub field: String,
    pub old: String,
    pub new: String,
}

/// What a call did (or, with `dry_run`, would do).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Applied {
    pub dry_run: bool,
    /// The fields that changed. For `clear`, the non-empty stored fields that were removed (`new`
    /// is empty).
    pub changes: Vec<Changed>,
    /// With `dry_run` only: what a real call would refuse. A real call returns the refusals as a
    /// [`Failure`].
    pub refusals: Vec<Refused>,
    /// `configure`: the version of the logo after the change. Empty after a `clear`.
    pub v: String,
    /// `clear`: a row was (with `dry_run`: would be) removed.
    pub cleared: bool,
}

/// Why a real call changed nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    /// Values were refused; the whole call stored nothing.
    Refused(Vec<Refused>),
    /// The store failed: its text, unchanged (a D74 code word stays at the start).
    Storage(String),
}

impl std::fmt::Display for Failure {
    /// A refusal lists every refusal, one per line as `field: rule`; a storage failure is the
    /// store's text.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Failure::Refused(list) => {
                for (i, r) in list.iter().enumerate() {
                    if i > 0 {
                        f.write_str("\n")?;
                    }
                    write!(f, "{}: {}", r.field, r.rule)?;
                }
                Ok(())
            }
            Failure::Storage(e) => f.write_str(e),
        }
    }
}

/// The `changes` of an answer: a JSON list of `{"field", "old", "new"}`.
pub fn changes_json(list: &[Changed]) -> String {
    J::Array(
        list.iter()
            .map(|c| json!({"field": c.field, "old": c.old, "new": c.new}))
            .collect(),
    )
    .to_string()
}

/// The `refusals` of an answer: a JSON list of `{"field", "rule"}`.
pub fn refusals_json(list: &[Refused]) -> String {
    J::Array(
        list.iter()
            .map(|r| json!({"field": r.field, "rule": r.rule}))
            .collect(),
    )
    .to_string()
}

/// How the row is to change.
#[derive(Clone)]
enum Write {
    Nothing,
    Put(Fields),
    Delete,
}

#[derive(Clone)]
struct Plan {
    write: Write,
    changes: Vec<Changed>,
    cleared: bool,
    /// The row after the change, for `v` and for the reader's cell.
    after: Option<Fields>,
}

const ARTWORK: &str = "logo_svg";
const UNREADABLE_ARTWORK: &str = "unreadable";
/// A value of the log is cut here (see the module documentation).
const LOG_CUT: usize = 300;

fn kind_of(field: &str) -> Kind {
    if field.starts_with("title_") {
        Kind::Title
    } else if field.starts_with("message_") {
        Kind::Message
    } else {
        Kind::Name
    }
}

/// An unknown input key as it may be repeated in a refusal: ASCII letters, digits, `_` and `-`,
/// cut at 40 characters. The one place that decides what of a caller's key a message repeats, so a
/// key cannot shape a line of the log or of a terminal.
pub fn show_key(k: &str) -> String {
    let mut out: String = k
        .chars()
        .take(40)
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '?'
            }
        })
        .collect();
    if k.chars().nth(40).is_some() {
        out.push_str("...");
    }
    out
}

/// The version of a stored artwork, as the logo route would serve it: the hash of the printer's
/// output for its parse. Empty for no artwork, `unreadable` when the reader refuses it.
fn artwork_v(stored: &str) -> String {
    if stored.is_empty() {
        return String::new();
    }
    match svg::parse_str(stored) {
        Ok(parsed) => logo::version(parsed.print().as_bytes()),
        Err(_) => UNREADABLE_ARTWORK.to_string(),
    }
}

/// The whole policy for one supplied value: the text to store (empty clears the field), or the
/// rules it broke.
fn validate(field: &str, value: &str) -> Result<String, Vec<String>> {
    let one = |r: obop_brand::Refusal| vec![r.message];
    match field {
        "name" => policy::clean(Kind::Name, value).map_err(one),
        "slug" => {
            if value.is_empty() {
                Ok(String::new())
            } else {
                slug::check(value).map(|()| value.to_string()).map_err(one)
            }
        }
        ARTWORK => {
            if value.is_empty() {
                Ok(String::new())
            } else {
                svg::canonical(value.as_bytes())
                    .map_err(|rs| rs.into_iter().map(|r| r.message).collect())
            }
        }
        f if f.starts_with("title_") => policy::clean(Kind::Title, value).map_err(one),
        f if f.starts_with("message_") => policy::clean(Kind::Message, value).map_err(one),
        _ => Err(vec!["is not a field of the brand".to_string()]),
    }
}

fn plan_configure(backend: &dyn Backend, supplied: &[(String, String)]) -> Result<Plan, Failure> {
    let old: Option<Fields> = backend.get(MODEL, ROW_ID).map_err(Failure::Storage)?;
    let fields = writable_fields();
    let given: BTreeMap<&str, &str> = supplied
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let mut refusals = Vec::new();
    for key in given.keys() {
        if !fields.iter().any(|f| f == key) {
            refusals.push(Refused {
                field: show_key(key),
                rule: "is not a field of the brand; the fields are name, slug, title_<lang>, message_<lang> and logo_svg".into(),
            });
        }
    }
    let mut row = old.clone().unwrap_or_default();
    let mut changes = Vec::new();
    for field in &fields {
        let Some(&value) = given.get(field.as_str()) else {
            continue;
        };
        match validate(field, value) {
            Err(rules) => refusals.extend(rules.into_iter().map(|rule| Refused {
                field: field.clone(),
                rule,
            })),
            Ok(stored) => {
                let before = old
                    .as_ref()
                    .and_then(|r| text_of(r, field).ok())
                    .unwrap_or("");
                if before != stored {
                    let (o, n) = if field == ARTWORK {
                        (artwork_v(before), artwork_v(&stored))
                    } else {
                        (before.to_string(), stored.clone())
                    };
                    // A stored artwork that is valid but not in canonical form, sent again: what is
                    // served and its version are the same as before, so there is nothing to record
                    // (the row is canonical again if the call writes it for another field).
                    if o != n {
                        changes.push(Changed {
                            field: field.clone(),
                            old: o,
                            new: n,
                        });
                    }
                }
                if stored.is_empty() {
                    row.remove(field);
                } else {
                    row.insert(field.clone(), J::String(stored));
                }
            }
        }
    }
    let has_name = text_of(&row, "name").is_ok_and(|n| !n.is_empty());
    if !has_name && !refusals.iter().any(|r| r.field == "name") {
        refusals.push(Refused {
            field: "name".into(),
            rule: "is required: a brand needs a name".into(),
        });
    }
    if !refusals.is_empty() {
        return Err(Failure::Refused(refusals));
    }
    if changes.is_empty() && old.is_some() {
        return Ok(Plan {
            write: Write::Nothing,
            changes,
            cleared: false,
            after: old,
        });
    }
    Ok(Plan {
        write: Write::Put(row.clone()),
        changes,
        cleared: false,
        after: Some(row),
    })
}

fn plan_clear(backend: &dyn Backend) -> Result<Plan, Failure> {
    let Some(row) = backend.get(MODEL, ROW_ID).map_err(Failure::Storage)? else {
        return Ok(Plan {
            write: Write::Nothing,
            changes: Vec::new(),
            cleared: false,
            after: None,
        });
    };
    let mut changes = Vec::new();
    for field in writable_fields() {
        let Ok(text) = text_of(&row, &field) else {
            continue;
        };
        if text.is_empty() {
            continue;
        }
        let old = if field == ARTWORK {
            artwork_v(text)
        } else {
            text.to_string()
        };
        changes.push(Changed {
            field,
            old,
            new: String::new(),
        });
    }
    Ok(Plan {
        write: Write::Delete,
        changes,
        cleared: true,
        after: None,
    })
}

/// A value for the log (see the module documentation).
fn quoted(field: &str, v: &str) -> String {
    if field == ARTWORK {
        return match v {
            "" => "none".to_string(),
            UNREADABLE_ARTWORK => UNREADABLE_ARTWORK.to_string(),
            v => format!("v={v}"),
        };
    }
    let cut: String = v.chars().take(LOG_CUT).collect();
    let more = v.chars().nth(LOG_CUT).is_some();
    let mut out = if policy::safe(kind_of(field), &cut).is_ok() {
        J::String(cut).to_string()
    } else {
        let mut s = String::from("\"");
        for c in cut.chars() {
            match c {
                '"' => s.push_str("\\\""),
                '\\' => s.push_str("\\\\"),
                c if c.is_ascii_graphic() || c == ' ' => s.push(c),
                c => s.push_str(&format!("\\u{{{:04X}}}", c as u32)),
            }
        }
        s.push('"');
        s
    };
    if more {
        out.push_str("...");
    }
    out
}

fn detail(field: &str, old: &str, new: Option<&str>) -> String {
    match new {
        Some(n) => format!("{field}: {} -> {}", quoted(field, old), quoted(field, n)),
        None => format!("{field}: {} -> removed", quoted(field, old)),
    }
}

fn event(now: u64, cleared: bool, actor: &str, detail: &str) -> Op {
    if cleared {
        audit::event_op(now, AUDIT_USER, "brand_cleared", actor, detail)
    } else {
        audit::event_op(now, AUDIT_USER, "brand_configured", actor, detail)
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Writes (or, with `dry_run`, only checks) one change of the brand. See the module documentation.
/// `actor` is the principal's id (the command line will pass `operator`); `reader` is the
/// dispatcher's reader, or none (what the command line will pass).
pub fn apply(
    backend: &dyn Backend,
    reader: Option<&Reader>,
    actor: &str,
    action: &Action,
    dry_run: bool,
) -> Result<Applied, Failure> {
    let plan = |backend: &dyn Backend| match action {
        Action::Configure(supplied) => plan_configure(backend, supplied),
        Action::Clear => plan_clear(backend),
    };
    if dry_run {
        return match plan(backend).and_then(|p| answer(backend, p, true)) {
            Err(Failure::Refused(refusals)) => Ok(Applied {
                dry_run: true,
                changes: Vec::new(),
                refusals,
                v: String::new(),
                cleared: false,
            }),
            other => other,
        };
    }
    let _lock = backend.lock(LOCK_KEY).map_err(Failure::Storage)?;
    let plan = plan(backend)?;
    // Everything that can still fail without a write is done before the batch, so a change that
    // committed is never reported as a failure.
    let applied = answer(backend, plan.clone(), false)?;
    let ops = match &plan.write {
        Write::Nothing => Vec::new(),
        Write::Put(row) => vec![Op::Put {
            model: MODEL.into(),
            id: ROW_ID.into(),
            fields: row.clone(),
        }],
        Write::Delete => vec![Op::Delete {
            model: MODEL.into(),
            id: ROW_ID.into(),
        }],
    };
    if !ops.is_empty() {
        let at = now_secs();
        let mut ops = ops;
        for c in &plan.changes {
            let line = if plan.cleared {
                detail(&c.field, &c.old, None)
            } else {
                detail(&c.field, &c.old, Some(&c.new))
            };
            ops.push(event(at, plan.cleared, actor, &line));
        }
        backend
            .apply_checked(&ops)
            .map_err(|e| Failure::Storage(e.to_string()))?;
        if let Some(r) = reader {
            r.applied(plan.after.as_ref());
        }
    }
    Ok(applied)
}

/// The answer for a plan: with the version of the logo the row shows after the change.
fn answer(backend: &dyn Backend, plan: Plan, dry_run: bool) -> Result<Applied, Failure> {
    let v = match (&plan.after, plan.cleared) {
        (Some(row), false) => {
            let setting = backend
                .get(SETTING_MODEL, SETTING_ID)
                .map_err(Failure::Storage)?;
            read_row(Some(row), accent_of(setting.as_ref()))
                .v()
                .to_string()
        }
        _ => String::new(),
    };
    Ok(Applied {
        dry_run,
        changes: plan.changes,
        refusals: Vec::new(),
        v,
        cleared: plan.cleared,
    })
}
