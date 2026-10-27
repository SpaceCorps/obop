//! A refusal by the database's own guards (D77): the typed error that crosses the places where
//! the storage contract has only a `String`.
//!
//! It is not [`crate::fault`]. A fault (D74) says the store could not do the work (busy, gone,
//! too slow) and the caller may try again; a refusal says the store would not do it because the
//! change breaks a rule the database enforces (a record that is frozen, a state change without
//! its action, a guard that cannot be replaced), and trying again changes nothing.
//!
//! The wire form is the human message, the separator [`SEP`] (ASCII record separator, which no
//! message contains) and a JSON object with the machine-readable parts:
//!
//! ```text
//! invoice 0192f: total cannot change while the record is posted<RS>{"code":"immutable_record",...}
//! ```
//!
//! A surface that only prints the error text shows [`human`]; one that knows refusals calls
//! [`parse`]. Neither the message nor the JSON carries a field value: names only.

use postgres::error::DbError;
use serde_json::{Value as J, json};

/// Between the human message and the JSON of a refusal.
pub const SEP: char = '\u{1e}';

/// SQLSTATE of a write the database refused because the record is frozen (or the table is
/// append-only).
pub const SQLSTATE_IMMUTABLE: &str = "OBG01";
/// SQLSTATE of a change of a guard the database refused to make.
pub const SQLSTATE_GUARD_CHANGE: &str = "OBG02";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefusalCode {
    /// The record is frozen, or the table is append-only; the change is not an allowed one.
    ImmutableRecord,
    /// The record changed while an action ran (a lost compare-and-set); nothing was written.
    StateConflict,
    /// A guard cannot be replaced or removed while records are frozen under it.
    GuardChangeRefused,
}

impl RefusalCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ImmutableRecord => "immutable_record",
            Self::StateConflict => "state_conflict",
            Self::GuardChangeRefused => "guard_change_refused",
        }
    }

    pub fn from_code(s: &str) -> Option<Self> {
        [
            Self::ImmutableRecord,
            Self::StateConflict,
            Self::GuardChangeRefused,
        ]
        .into_iter()
        .find(|c| c.as_str() == s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub code: RefusalCode,
    pub model: String,
    pub record: String,
    /// Which rule refused: `immutable_when`, `transition`, `lock_date`, `children_of`,
    /// `no_delete_when`, `append_only`, `identity`, `truncate`, `initial`.
    pub rule: String,
    /// The fields the change touched that the rule does not allow (names only).
    pub fields: Vec<String>,
    /// Actions that would allow the change.
    pub actions: Vec<String>,
    /// Fields that may still be changed on this record.
    pub mutable: Vec<String>,
    /// The human message. Never contains a field value.
    pub message: String,
}

impl Refusal {
    pub fn new(code: RefusalCode, model: &str, record: &str, rule: &str, message: &str) -> Self {
        Self {
            code,
            model: model.to_string(),
            record: record.to_string(),
            rule: rule.to_string(),
            fields: Vec::new(),
            actions: Vec::new(),
            mutable: Vec::new(),
            message: message.to_string(),
        }
    }

    /// The error text to return through a `String`-typed boundary: see the module doc.
    pub fn to_error(&self) -> String {
        to_error(self)
    }
}

/// The wire form of `r`. A separator inside the message is replaced by a space, so the first
/// separator of an error text is always the one that starts the JSON.
pub fn to_error(r: &Refusal) -> String {
    let message = r.message.replace(SEP, " ");
    let body = json!({
        "code": r.code.as_str(),
        "model": r.model,
        "record": r.record,
        "rule": r.rule,
        "fields": r.fields,
        "actions": r.actions,
        "mutable": r.mutable,
    });
    format!("{message}{SEP}{body}")
}

fn names(v: &J, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(J::as_array)
        .map(|a| a.iter().filter_map(J::as_str).map(str::to_string).collect())
        .unwrap_or_default()
}

fn text(v: &J, key: &str) -> String {
    v.get(key)
        .and_then(J::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Recognise a refusal in an error text made by [`to_error`]. Text without the separator, with
/// JSON that is not an object, or with a code this version does not know, is not a refusal.
pub fn parse(error: &str) -> Option<Refusal> {
    let (message, body) = error.split_once(SEP)?;
    let v: J = serde_json::from_str(body).ok()?;
    if !v.is_object() {
        return None;
    }
    Some(Refusal {
        code: RefusalCode::from_code(v.get("code")?.as_str()?)?,
        model: text(&v, "model"),
        record: text(&v, "record"),
        rule: text(&v, "rule"),
        fields: names(&v, "fields"),
        actions: names(&v, "actions"),
        mutable: names(&v, "mutable"),
        message: message.to_string(),
    })
}

/// The part of an error text a person reads: the message of a refusal without its JSON, any
/// other text unchanged.
pub fn human(error: &str) -> &str {
    match parse(error) {
        Some(_) => error.split_once(SEP).map_or(error, |(m, _)| m),
        None => error,
    }
}

/// A server error raised by one of the guard functions (`OBG01`, `OBG02`), as a refusal. The
/// guards put names, never values, into the DETAIL: `{"model","id","rule","fields":[..],
/// "detail"}`. Any other error is not a refusal. `state_conflict` has no database origin: it is
/// made where a compare-and-set is lost.
pub fn from_db(e: &DbError) -> Option<Refusal> {
    let code = match e.code().code() {
        SQLSTATE_IMMUTABLE => RefusalCode::ImmutableRecord,
        SQLSTATE_GUARD_CHANGE => RefusalCode::GuardChangeRefused,
        _ => return None,
    };
    Some(from_detail(code, e.message(), e.detail()))
}

/// The part of [`from_db`] that does not need a server error: the message and DETAIL of one.
pub(crate) fn from_detail(code: RefusalCode, message: &str, detail: Option<&str>) -> Refusal {
    let d: J = detail
        .and_then(|t| serde_json::from_str(t).ok())
        .unwrap_or(J::Null);
    let (model, record, rule) = (text(&d, "model"), text(&d, "id"), text(&d, "rule"));
    let why = match text(&d, "detail") {
        s if s.is_empty() => message.to_string(),
        s => s,
    };
    let shown = if model.is_empty() {
        why
    } else {
        format!("{model} {record}: {why}")
    };
    Refusal {
        code,
        model,
        record,
        rule,
        fields: names(&d, "fields"),
        actions: Vec::new(),
        mutable: Vec::new(),
        message: shown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Refusal {
        Refusal {
            code: RefusalCode::ImmutableRecord,
            model: "invoice".into(),
            record: "0192f".into(),
            rule: "immutable_when".into(),
            fields: vec!["total".into()],
            actions: vec!["reopen".into()],
            mutable: vec!["note".into()],
            message: "invoice 0192f: total cannot change while the record is posted".into(),
        }
    }

    #[test]
    fn a_refusal_survives_a_string_boundary() {
        let r = sample();
        let wire = to_error(&r);
        assert_eq!(wire, r.to_error());
        assert!(wire.starts_with("invoice 0192f: total cannot change"));
        assert_eq!(wire.matches(SEP).count(), 1);
        assert_eq!(parse(&wire), Some(r));
    }

    #[test]
    fn every_code_has_one_stable_word() {
        for (c, w) in [
            (RefusalCode::ImmutableRecord, "immutable_record"),
            (RefusalCode::StateConflict, "state_conflict"),
            (RefusalCode::GuardChangeRefused, "guard_change_refused"),
        ] {
            assert_eq!(c.as_str(), w);
            assert_eq!(RefusalCode::from_code(w), Some(c));
            let r = Refusal::new(c, "m", "1", "rule", "no");
            assert_eq!(parse(&r.to_error()).unwrap().code, c);
        }
        assert_eq!(RefusalCode::from_code("storage_busy"), None);
    }

    #[test]
    fn a_separator_inside_the_message_cannot_forge_the_json() {
        let mut r = sample();
        r.message = format!("evil{SEP}{{\"code\":\"state_conflict\"}}");
        let wire = to_error(&r);
        assert_eq!(wire.matches(SEP).count(), 1);
        let back = parse(&wire).unwrap();
        assert_eq!(back.code, RefusalCode::ImmutableRecord);
        assert_eq!(back.model, "invoice");
    }

    #[test]
    fn text_that_is_not_a_refusal_is_not_parsed_as_one() {
        for t in [
            "",
            "plain failure",
            "storage_busy: all database connections are in use",
            &format!("msg{SEP}not json"),
            &format!("msg{SEP}[]"),
            &format!("msg{SEP}{{}}"),
            &format!("msg{SEP}{{\"code\":\"something_else\"}}"),
            &format!("msg{SEP}{{\"code\":7}}"),
        ] {
            assert_eq!(parse(t), None, "{t:?}");
            assert_eq!(human(t), t, "{t:?}");
        }
    }

    #[test]
    fn a_refusal_wrapped_by_a_caller_still_parses_and_prints_without_its_json() {
        let wire = format!("update failed: {}", to_error(&sample()));
        assert_eq!(parse(&wire).unwrap().rule, "immutable_when");
        assert_eq!(
            human(&wire),
            "update failed: invoice 0192f: total cannot change while the record is posted"
        );
        assert!(!human(&to_error(&sample())).contains('{'));
    }

    #[test]
    fn the_json_names_fields_and_never_values() {
        let wire = to_error(&sample());
        let v: J = serde_json::from_str(wire.split_once(SEP).unwrap().1).unwrap();
        assert_eq!(v["fields"], json!(["total"]));
        assert_eq!(v["mutable"], json!(["note"]));
        assert_eq!(v["actions"], json!(["reopen"]));
    }

    #[test]
    fn a_missing_optional_part_reads_as_empty() {
        let wire = format!("m{SEP}{{\"code\":\"state_conflict\",\"model\":\"o\"}}");
        let r = parse(&wire).unwrap();
        assert_eq!(r.code, RefusalCode::StateConflict);
        assert_eq!((r.record.as_str(), r.rule.as_str()), ("", ""));
        assert!(r.fields.is_empty() && r.actions.is_empty() && r.mutable.is_empty());
    }

    #[test]
    fn the_detail_of_a_guard_error_becomes_a_refusal_with_names_only() {
        let detail = r#"{"model":"invoice","id":"0192f","rule":"immutable_when","fields":["total"],"detail":"total cannot change while the state is posted"}"#;
        let r = from_detail(
            RefusalCode::ImmutableRecord,
            "immutable_record",
            Some(detail),
        );
        assert_eq!(
            (r.model.as_str(), r.record.as_str(), r.rule.as_str()),
            ("invoice", "0192f", "immutable_when")
        );
        assert_eq!(r.fields, ["total"]);
        assert_eq!(
            r.message,
            "invoice 0192f: total cannot change while the state is posted"
        );
        // A detail that is missing or not JSON still gives a refusal, with the server's message.
        for d in [None, Some("not json")] {
            let r = from_detail(RefusalCode::GuardChangeRefused, "guard_change_refused", d);
            assert_eq!(r.message, "guard_change_refused");
            assert!(r.model.is_empty() && r.fields.is_empty());
        }
    }
}
