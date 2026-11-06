//! `brand::apply`: the one function that writes the brand row, for `configure` and `clear` on every
//! surface (docs/spec/branding.md, section 3). The dispatcher calls it for both commands, and so
//! does the command line (`obop admin brand`, which acts on the database as the operator), so that
//! there is one rule:
//!
//! - It takes the cross-process lock `brand.write` itself and does the read, the merge and the write
//!   under it, so two changes at once, from this process or another, cannot lose each other. It does
//!   not rely on the dispatcher's transaction lock: the command line has no dispatcher, and
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
//! - When a reader is given (the dispatcher passes its own; the command line passes none) its cell
//!   is updated after the batch has committed and never before.
//!
//! **The audit events.** Kind `brand_configured` for `configure` and `brand_cleared` for `clear`;
//! `user` is `(instance)`; the actor is the principal's id (the command line passes `operator`).
//! The detail is one line that names the field and gives the old and the new value ([`change_line`]
//! is that line, for a caller that shows a change the way the log holds it):
//!
//! ```text
//! name: "obop" -> "MOHV"            a text field: both values as quoted text, "" when unset
//! title_sv: "" -> "Logga in"
//! logo_svg: none -> v=0123456789abcdef   the artwork is never written: its version `v` instead,
//!                                        `none` when there is no artwork, `unreadable` when the
//!                                        stored one fails the strict reader
//! name: "MOHV" -> removed           clear: one event per key of the row, a text field with its
//!                                   old value
//! name: <not text> -> removed       a field whose stored value is not text: no value
//! hide_product_docs: <unknown> -> removed   a key this binary does not know: no value
//! ```
//!
//! `clear` accounts for every key the row holds: a row of only unknown keys writes one event per
//! key, and a row with no key at all is deleted with no event. A key is written as [`show_key`]
//! shows it. At most 50 unknown keys are written one by one (in the order of their names); the
//! rest are one event, `and N more keys: <unknown> -> removed`, so that a row written around the
//! command cannot make the batch, and the lock it holds, as long as it likes.
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
///
/// `withheld` is set only for a key that `clear` removed and whose value is not written down: a
/// value that is not text, or a key this binary does not know (a newer binary's field, a switch of
/// a later slice). `old` is then the marker (`<not text>` or `<unknown>`), `field` is the key as
/// [`show_key`] shows it, and the log line is `key: <marker> -> removed`, with no value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Changed {
    pub field: String,
    pub old: String,
    pub new: String,
    pub withheld: bool,
}

/// What a call did (or, with `dry_run`, would do).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Applied {
    pub dry_run: bool,
    /// The fields that changed. For `clear`, every key of the removed row (`new` is empty; see
    /// [`Changed::withheld`] for the keys whose value is not written down).
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
/// What the log says in place of the value of a key that `clear` removed and does not write down
/// (see [`Changed::withheld`]): a value that is not text, and a key this binary does not know.
const LOG_NOT_TEXT: &str = "<not text>";
const LOG_UNKNOWN: &str = "<unknown>";
/// The most keys this binary does not know that `clear` accounts for one by one; the rest are one
/// event with their number.
const LOG_MAX_UNKNOWN: usize = 50;

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
/// key cannot shape a line of the log or of a terminal. Every other character is shown as `?`, so
/// two keys that differ only in such characters (a line break, a space or a question mark between
/// `a` and `b`) are shown the same: the log accounts for each key of a cleared row, and does not
/// tell those apart.
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
                // A stored value that is not text (a row written around the command) reads as the
                // word `not_text` for the log, and any supplied value, an empty one included,
                // replaces it: the empty text is how a field is cleared, and it must repair this
                // as well.
                let (before, was_text) = match old.as_ref().map_or(Ok(""), |r| text_of(r, field)) {
                    Ok(text) => (text, true),
                    Err(word) => (word, false),
                };
                if !was_text || before != stored {
                    let (o, n) = if field == ARTWORK {
                        (artwork_v(before), artwork_v(&stored))
                    } else {
                        (before.to_string(), stored.clone())
                    };
                    // A stored artwork that is valid but not in canonical form, sent again: what is
                    // served and its version are the same as before, so there is nothing to record
                    // (the row is canonical again if the call writes it for another field). A value
                    // that was not text is always a change, even when the text sent is the very
                    // word the log reads it as: the row is written, so the change is logged.
                    if !was_text || o != n {
                        changes.push(Changed {
                            field: field.clone(),
                            old: o,
                            new: n,
                            withheld: false,
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
    // One change for every key the row holds, so the log accounts for all that the delete removes:
    // a text field with its old value (an empty or null one with the empty text); a field whose
    // value is not text, and a key this binary does not know, as a marker with no value (what such
    // a value is cannot be written safely into a line of the log, and a later field's value must
    // not be logged before its slice says how). A row with no key at all is deleted with no event.
    let known = writable_fields();
    let mut changes = Vec::new();
    for field in &known {
        if !row.contains_key(field) {
            continue;
        }
        changes.push(match text_of(&row, field) {
            Ok(text) => Changed {
                field: field.clone(),
                old: if field == ARTWORK {
                    artwork_v(text)
                } else {
                    text.to_string()
                },
                new: String::new(),
                withheld: false,
            },
            Err(_) => Changed {
                field: field.clone(),
                old: LOG_NOT_TEXT.to_string(),
                new: String::new(),
                withheld: true,
            },
        });
    }
    let mut others: Vec<&String> = row.keys().filter(|k| !known.contains(k)).collect();
    others.sort();
    // The keys this binary knows are a handful; a row written around the command can hold any
    // number of others, and each would be an event of the batch that is written under the lock. So
    // the first LOG_MAX_UNKNOWN (in the order of their names) are accounted for one by one and the
    // rest by one event with their number. Its field has spaces, which `show_key` never writes, so
    // it cannot be taken for the key of a row.
    let rest = others.len().saturating_sub(LOG_MAX_UNKNOWN);
    for key in others.into_iter().take(LOG_MAX_UNKNOWN) {
        changes.push(Changed {
            field: show_key(key),
            old: LOG_UNKNOWN.to_string(),
            new: String::new(),
            withheld: true,
        });
    }
    if rest > 0 {
        changes.push(Changed {
            field: format!("and {rest} more {}", if rest == 1 { "key" } else { "keys" }),
            old: LOG_UNKNOWN.to_string(),
            new: String::new(),
            withheld: true,
        });
    }
    Ok(Plan {
        write: Write::Delete,
        changes,
        cleared: true,
        after: None,
    })
}

/// `v` with every character that is not an ASCII graphic character or a space as `\u{...}`, the
/// backslash as `\\` and, with `quotes`, the double quote as `\"`.
fn escaped(v: &str, quotes: bool) -> String {
    let mut s = String::new();
    for c in v.chars() {
        match c {
            '"' if quotes => s.push_str("\\\""),
            '\\' => s.push_str("\\\\"),
            c if c.is_ascii_graphic() || c == ' ' => s.push(c),
            c => s.push_str(&format!("\\u{{{:04X}}}", c as u32)),
        }
    }
    s
}

/// A stored text as it may be shown where nothing escapes it, such as the answer of `get` that
/// `obop call` prints to a terminal: the text itself when it passes the safety subset of the policy
/// (so the stored text of every field that passes the read check comes back exactly as stored), else
/// ASCII only, with every other character as `\u{...}` and the backslash doubled, so that a value
/// written around the command cannot move a cursor, reorder a line or recolour a terminal.
pub fn printable(field: &str, v: &str) -> String {
    if field != ARTWORK && policy::safe(kind_of(field), v).is_ok() {
        return v.to_string();
    }
    escaped(v, false)
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
        format!("\"{}\"", escaped(&cut, true))
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

/// One change as the security log holds it (the module documentation has the format):
/// `field: old -> new`, or `field: old -> removed` for a field a `clear` removed (`cleared`). The
/// artwork is its version, never its text, and a value that is not safe to print is escaped, so a
/// terminal that shows the line cannot be shaped by a stored value.
pub fn change_line(change: &Changed, cleared: bool) -> String {
    if change.withheld {
        return format!("{}: {} -> removed", change.field, change.old);
    }
    if cleared {
        detail(&change.field, &change.old, None)
    } else {
        detail(&change.field, &change.old, Some(&change.new))
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
/// `actor` is the principal's id (the command line passes `operator`); `reader` is the
/// dispatcher's reader, or none (what the command line passes).
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
            ops.push(event(
                at,
                plan.cleared,
                actor,
                &change_line(c, plan.cleared),
            ));
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
