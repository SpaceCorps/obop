//! The generic writes of `sign_request`: a field's group restricts reading only, so the fields
//! that belong to the signer door (the evidence and the held copy) are protected here, and a
//! signed request is changed or deleted by an administrator only. An administrator can still
//! change any record: this is a rule against mistakes and against staff without the role, not
//! against someone with the database.

use super::*;
use crate::Value;
use obop_masking::mode_for;
use obop_types::{DataClass, MaskMode};

pub(super) fn install(d: &Dispatcher, svc: &Arc<SignService>) {
    let s = svc.clone();
    d.around(
        "sign_request.create",
        Box::new(move |p, input, inner| {
            if !is_admin(p) {
                s.refuse_door_fields(p, input, None)?;
            }
            inner(input)
        }),
    );
    let s = svc.clone();
    d.around(
        "sign_request.update",
        Box::new(move |p, input, inner| {
            if !is_admin(p) {
                s.check_update(p, input)?;
            }
            inner(input)
        }),
    );
    let s = svc.clone();
    d.around(
        "sign_request.delete",
        Box::new(move |p, input, inner| {
            if !is_admin(p)
                && let Ok(id) = id_of(input)
                && let Some(cur) = s.get("sign_request", &id)?
                && text(&cur, "state") == "signed"
            {
                return Err(SIGNED.into());
            }
            inner(input)
        }),
    );
    let s = svc.clone();
    d.around(
        "sign_request.bulk_create",
        Box::new(move |p, input, inner| {
            if !is_admin(p) {
                s.refuse_in_rows(p, input)?;
            }
            inner(input)
        }),
    );
}

const DOCUMENT_FIXED: &str =
    "sign_request.document_id cannot be changed after the request was sent; make a new request";

const SIGNED: &str =
    "sign_request: a signed request cannot be changed or deleted; ask an administrator";

fn door_message(k: &str) -> String {
    format!(
        "sign_request.{k} is written by the signing process (send_document and the signer's page); it cannot be set by hand"
    )
}

/// Whether two values are the same field value: absent, null and an empty text are alike.
fn same(stored: Option<&J>, given: &Value) -> bool {
    let empty = |v: Option<&J>| matches!(v, None | Some(J::Null)) || v == Some(&J::from(""));
    match given {
        Value::Null => empty(stored),
        Value::Text(t) if t.is_empty() => empty(stored),
        v => stored == Some(&v.to_json()),
    }
}

fn is_meta(k: &str) -> bool {
    k == "id" || k == "_caller" || k.starts_with(crate::query::ACTOR_PREFIX)
}

impl SignService {
    /// Whether `p` may read the door-owned field `key` of a request **in clear**: the group the
    /// package gives the field (narrowed by the token's scopes) and a mask mode of `Allow`, the
    /// rule `query_guard` applies to a filter. An agent that is shown a mask token or nothing is
    /// not a reader, or a right guess would be told from a wrong one. A field the registry does
    /// not list is not readable (the check fails closed).
    fn reads(&self, p: &Principal, key: &str) -> bool {
        self.models
            .read()
            .unwrap()
            .get("sign_request")
            .and_then(|m| m.def.fields.iter().find(|f| f.name == key))
            .is_some_and(|f| {
                p.allows(f.permission_group)
                    && mode_for(
                        p.kind,
                        f.class.unwrap_or(DataClass::Secret),
                        f.personal_data,
                    ) == MaskMode::Allow
            })
    }

    /// Refuse a value in a door-owned key. `current` is the stored record of an update, whose
    /// unchanged values a reader of the field may send back (a form saves the whole record).
    ///
    /// A caller who cannot read a field is refused for the key **whatever the value**: with the
    /// pass-through, a guess that equals the stored evidence would be answered 200 and a wrong one
    /// 400, which tells a person who may not read it whether it is that value (an equality oracle
    /// on the signer's address, name or hash).
    pub(super) fn refuse_door_fields(
        &self,
        p: &Principal,
        input: &Value,
        current: Option<&Fields>,
    ) -> Result<(), String> {
        let Value::Map(m) = input else {
            return Ok(());
        };
        for k in DOOR_OWNED {
            if let Some(v) = m.get(k)
                && (!self.reads(p, k) || !same(current.and_then(|c| c.get(k)), v))
            {
                return Err(door_message(k));
            }
        }
        Ok(())
    }

    fn check_update(&self, p: &Principal, input: &Value) -> Result<(), String> {
        let Value::Map(m) = input else {
            return Ok(());
        };
        let Ok(id) = id_of(input) else {
            return Ok(());
        };
        let Some(cur) = self.get("sign_request", &id)? else {
            return Ok(());
        };
        self.refuse_door_fields(p, input, Some(&cur))?;
        if text(&cur, "state") == "signed"
            && m.iter().any(|(k, v)| !is_meta(k) && !same(cur.get(k), v))
        {
            return Err(SIGNED.into());
        }
        // Once a request has left draft, staff cannot swap, clear or add its Document: the
        // signer's page and the held copy are bound to it, and a request that no longer names a
        // Document would look like one that never had one (and could be signed by hand). This
        // holds whether the request was sent with `send_document` or, with a template, with `send`
        // (which holds no hash and no copy): the stored Document decides, not the hash.
        if !matches!(text(&cur, "state"), "" | "draft")
            && let Some(v) = m.get("document_id")
            && !same(cur.get("document_id"), v)
        {
            return Err(DOCUMENT_FIXED.into());
        }
        Ok(())
    }
}

impl SignService {
    /// `bulk_create` takes `rows` (a list of objects) or `csv` (the text of a CSV file). A row or
    /// a column that names a door-owned field is refused; in a row, an empty value is let through
    /// only for a reader of the field (as in `create`).
    fn refuse_in_rows(&self, p: &Principal, input: &Value) -> Result<(), String> {
        let Value::Map(m) = input else {
            return Ok(());
        };
        if let Some(Value::List(rows)) = m.get("rows") {
            for r in rows {
                if let Value::Map(r) = r {
                    for k in DOOR_OWNED {
                        if r.get(k)
                            .is_some_and(|v| !self.reads(p, k) || !same(None, v))
                        {
                            return Err(door_message(k));
                        }
                    }
                }
            }
        }
        refuse_csv_columns(m)
    }
}

fn refuse_csv_columns(m: &std::collections::BTreeMap<String, Value>) -> Result<(), String> {
    if let Some(Value::Text(csv)) = m.get("csv") {
        // The header as the importer reads it, not as a line of text: the importer skips blank
        // records, ends a record at a lone CR, and trims a cell after unquoting it. A file that
        // cannot be parsed is refused here with the importer's own words.
        let table = crate::csv::parse(csv.as_bytes(), crate::csv::MAX_ROWS)?;
        for col in &table.headers {
            if let Some(k) = DOOR_OWNED.iter().find(|k| **k == col.as_str()) {
                return Err(door_message(k));
            }
        }
    }
    Ok(())
}
