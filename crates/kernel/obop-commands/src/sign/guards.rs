//! The generic writes of `sign_request`: a field's group restricts reading only, so the fields
//! that belong to the signer door (the evidence and the held copy) are protected here, and a
//! signed request is changed or deleted by an administrator only. An administrator can still
//! change any record: this is a rule against mistakes and against staff without the role, not
//! against someone with the database.

use super::*;
use crate::Value;

pub(super) fn install(d: &Dispatcher, svc: &Arc<SignService>) {
    let s = svc.clone();
    d.around(
        "sign_request.create",
        Box::new(move |p, input, inner| {
            if !is_admin(p) {
                s.refuse_door_fields(input, None)?;
            }
            inner(input)
        }),
    );
    let s = svc.clone();
    d.around(
        "sign_request.update",
        Box::new(move |p, input, inner| {
            if !is_admin(p) {
                s.check_update(input)?;
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
    d.around(
        "sign_request.bulk_create",
        Box::new(|p, input, inner| {
            if !is_admin(p) {
                refuse_in_rows(input)?;
            }
            inner(input)
        }),
    );
}

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
    /// Refuse a value in a door-owned key. `current` is the stored record of an update, whose
    /// unchanged values may be sent back (a form saves the whole record).
    pub(super) fn refuse_door_fields(
        &self,
        input: &Value,
        current: Option<&Fields>,
    ) -> Result<(), String> {
        let Value::Map(m) = input else {
            return Ok(());
        };
        for k in DOOR_OWNED {
            if let Some(v) = m.get(k)
                && !same(current.and_then(|c| c.get(k)), v)
            {
                return Err(door_message(k));
            }
        }
        Ok(())
    }

    fn check_update(&self, input: &Value) -> Result<(), String> {
        let Value::Map(m) = input else {
            return Ok(());
        };
        let Ok(id) = id_of(input) else {
            return Ok(());
        };
        let Some(cur) = self.get("sign_request", &id)? else {
            return Ok(());
        };
        self.refuse_door_fields(input, Some(&cur))?;
        if text(&cur, "state") == "signed"
            && m.iter().any(|(k, v)| !is_meta(k) && !same(cur.get(k), v))
        {
            return Err(SIGNED.into());
        }
        Ok(())
    }
}

/// `bulk_create` takes `rows` (a list of objects) or `csv` (text with a header line). A row that
/// names a door-owned column is refused.
fn refuse_in_rows(input: &Value) -> Result<(), String> {
    let Value::Map(m) = input else {
        return Ok(());
    };
    if let Some(Value::List(rows)) = m.get("rows") {
        for r in rows {
            if let Value::Map(r) = r {
                for k in DOOR_OWNED {
                    if r.get(k).is_some_and(|v| !same(None, v)) {
                        return Err(door_message(k));
                    }
                }
            }
        }
    }
    if let Some(Value::Text(csv)) = m.get("csv") {
        let header = csv.lines().next().unwrap_or("");
        for col in header.split([',', ';', '\t']) {
            let col = col.trim().trim_matches('"').trim_start_matches('\u{feff}');
            if let Some(k) = DOOR_OWNED.iter().find(|k| **k == col) {
                return Err(door_message(k));
            }
        }
    }
    Ok(())
}
