//! `decimal` and `datetime` values at the command boundary (D72).
//!
//! Every write path (`create`, `update`, the effects of an action, `bulk_create`, the public
//! site) calls [`canonicalise`] before it checks and stores a record: a decimal or an instant
//! that is valid is rewritten to its one canonical text (`12.3` at scale 4 becomes `12.3000`, an
//! instant with an offset becomes UTC with six fractional digits), and one that is not valid is
//! refused with an error that names the field. The checks of the stores ([`check_canonical`])
//! then accept only canonical text, so a path that forgot to canonicalise fails loudly instead
//! of storing a second spelling of a value, and stored strings of one field compare as the
//! numbers and instants they stand for wherever strings are compared.

use crate::Value;
use obop_registry::{FieldKind, ModelDef};
use obop_storage::Fields;
use obop_values::{Decimal, Instant};
use serde_json::Value as J;
use std::collections::BTreeMap;

/// Where a value comes from. Only an import rounds extra fractional digits, and only when the
/// field declares a rounding mode.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Path {
    /// `create`, `update`, effects: too many fractional digits are an error.
    Write,
    /// `bulk_create` and `obop import`: the field's `round`, when it has one.
    Import,
}

/// The canonical text of `s` for a decimal or datetime kind; `None` for any other kind. The
/// error is the reason, to follow "must be ...".
fn canonical_text(kind: &FieldKind, s: &str, path: Path) -> Option<Result<String, String>> {
    match kind {
        FieldKind::Decimal {
            precision,
            scale,
            round,
        } => {
            let mode = if path == Path::Import { *round } else { None };
            Some(
                Decimal::parse_for(s, *precision, *scale, mode)
                    .map(|d| d.to_string())
                    .map_err(|e| e.to_string()),
            )
        }
        FieldKind::Datetime => Some(
            Instant::parse(s)
                .map(Instant::canonical)
                .map_err(|e| e.to_string()),
        ),
        _ => None,
    }
}

/// `model.field must be <what a value is>` with the reason after a semicolon.
pub(crate) fn wrong(model: &str, name: &str, kind: &FieldKind, why: Option<&str>) -> String {
    match why {
        Some(w) => format!("{model}.{name} must be {}; {w}", kind.describe()),
        None => format!("{model}.{name} must be {}", kind.describe()),
    }
}

fn not_a_string(kind: &FieldKind) -> &'static str {
    match kind {
        FieldKind::Datetime => "a JSON number is not an instant; send an RFC 3339 string",
        _ => "a JSON number is never accepted for a decimal, send it as a string",
    }
}

/// Rewrite the decimal and datetime values of `fields` to canonical text; see the module notes.
pub(crate) fn canonicalise(def: &ModelDef, fields: &mut Fields, path: Path) -> Result<(), String> {
    for f in &def.fields {
        if !matches!(f.kind, FieldKind::Decimal { .. } | FieldKind::Datetime) {
            continue;
        }
        match fields.get_mut(f.name) {
            None | Some(J::Null) => {}
            Some(J::String(s)) => match canonical_text(&f.kind, s, path) {
                Some(Ok(c)) => *s = c,
                Some(Err(e)) => return Err(wrong(def.name, f.name, &f.kind, Some(&e))),
                None => {}
            },
            Some(J::Number(_)) => {
                return Err(wrong(
                    def.name,
                    f.name,
                    &f.kind,
                    Some(not_a_string(&f.kind)),
                ));
            }
            Some(_) => return Err(wrong(def.name, f.name, &f.kind, None)),
        }
    }
    Ok(())
}

/// `canonicalise` for a record held as `Value`s (the public site).
pub(crate) fn canonicalise_values(
    def: &ModelDef,
    fields: &mut BTreeMap<String, Value>,
    path: Path,
) -> Result<(), String> {
    let mut j: Fields = fields
        .iter()
        .filter(|(k, _)| {
            def.fields.iter().any(|f| {
                f.name == k.as_str()
                    && matches!(f.kind, FieldKind::Decimal { .. } | FieldKind::Datetime)
            })
        })
        .map(|(k, v)| (k.clone(), v.to_json()))
        .collect();
    canonicalise(def, &mut j, path)?;
    for (k, v) in j {
        fields.insert(k, Value::from_json(&v));
    }
    Ok(())
}

/// A stored or supplied value must already be in canonical form (see the module notes).
pub(crate) fn check_canonical(
    model: &str,
    name: &str,
    kind: &FieldKind,
    s: &str,
) -> Result<(), String> {
    match canonical_text(kind, s, Path::Write) {
        Some(Ok(c)) if c == s => Ok(()),
        Some(Ok(c)) => Err(wrong(
            model,
            name,
            kind,
            Some(&format!("write it in canonical form, {c}")),
        )),
        Some(Err(e)) => Err(wrong(model, name, kind, Some(&e))),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use obop_registry::FieldDef;
    use obop_types::DataClass;

    fn def() -> ModelDef {
        let f = |n: &'static str| FieldDef::new(n, "d", DataClass::None, false, "t.user");
        ModelDef {
            name: "t_row",
            description: "d",
            read_group: "t.user",
            write_group: "t.user",
            fields: vec![
                f("price").decimal(8, 2),
                f("rated").decimal_rounded(8, 2, obop_registry::Round::HalfEven),
                f("at").datetime(),
                f("note"),
            ],
            commands: vec![],
            access: None,
        }
    }

    fn fields(v: serde_json::Value) -> Fields {
        v.as_object().unwrap().clone().into_iter().collect()
    }

    #[test]
    fn valid_values_become_canonical_and_others_are_refused_by_field() {
        let d = def();
        let mut f = fields(serde_json::json!({
            "price": "12.3", "at": "2026-03-29T03:30:00+02:00", "note": "x", "rated": null
        }));
        canonicalise(&d, &mut f, Path::Write).unwrap();
        assert_eq!(f["price"], "12.30");
        assert_eq!(f["at"], "2026-03-29T01:30:00.000000Z");
        assert_eq!(f["rated"], J::Null);
        for (bad, field, want) in [
            (
                serde_json::json!({"price": 12.5}),
                "price",
                "never accepted",
            ),
            (serde_json::json!({"price": 12}), "price", "never accepted"),
            (serde_json::json!({"price": "1e3"}), "price", "exponent"),
            (serde_json::json!({"price": "1,5"}), "price", "comma"),
            (serde_json::json!({"price": " 1.5"}), "price", "spaces"),
            (serde_json::json!({"price": "NaN"}), "price", "NaN"),
            (serde_json::json!({"price": "0x10"}), "price", "hexadecimal"),
            (
                serde_json::json!({"price": "1.234"}),
                "price",
                "3 fractional digits",
            ),
            (
                serde_json::json!({"price": "1000000.00"}),
                "price",
                "too many digits",
            ),
            (serde_json::json!({"price": true}), "price", "decimal"),
            (
                serde_json::json!({"at": "2026-03-29T01:30:00"}),
                "at",
                "no offset",
            ),
            (
                serde_json::json!({"at": "2026-02-30T10:00:00Z"}),
                "at",
                "calendar date",
            ),
            (
                serde_json::json!({"at": 1_700_000_000}),
                "at",
                "RFC 3339 string",
            ),
            (serde_json::json!({"at": ["x"]}), "at", "RFC 3339"),
        ] {
            let mut f = fields(bad);
            let e = canonicalise(&d, &mut f, Path::Write).unwrap_err();
            assert!(
                e.starts_with(&format!("t_row.{field} must be ")) && e.contains(want),
                "{e}"
            );
        }
    }

    #[test]
    fn only_an_import_rounds_and_only_with_a_mode() {
        let d = def();
        let mut f = fields(serde_json::json!({"rated": "1.005", "price": "1.00"}));
        assert!(canonicalise(&d, &mut f, Path::Write).is_err());
        canonicalise(&d, &mut f, Path::Import).unwrap();
        assert_eq!(
            f["rated"], "1.00",
            "half_even: the tie goes to the even digit"
        );
        let mut f = fields(serde_json::json!({"price": "1.005"}));
        let e = canonicalise(&d, &mut f, Path::Import).unwrap_err();
        assert!(
            e.contains("3 fractional digits"),
            "no mode, no rounding: {e}"
        );
    }

    #[test]
    fn the_stores_accept_only_canonical_text() {
        let k = FieldKind::decimal(8, 2, None).unwrap();
        assert!(check_canonical("m", "f", &k, "12.30").is_ok());
        assert!(check_canonical("m", "f", &k, "-0.05").is_ok());
        let e = check_canonical("m", "f", &k, "12.3").unwrap_err();
        assert!(e.contains("canonical form, 12.30"), "{e}");
        assert!(check_canonical("m", "f", &k, "-0.00").is_err());
        assert!(
            check_canonical(
                "m",
                "f",
                &FieldKind::Datetime,
                "2026-03-29T01:30:00.000000Z"
            )
            .is_ok()
        );
        assert!(check_canonical("m", "f", &FieldKind::Datetime, "2026-03-29T01:30:00Z").is_err());
    }
}
