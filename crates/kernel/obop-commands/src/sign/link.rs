//! The commands of `sign_link`. `create` is a wrapper that does not call the generated handler:
//! it mints the secret, so nothing but the server chooses what a link is. `update` takes one
//! thing, the stamp of `revoked_at`; `delete` and `bulk_create` are refused (an administrator may
//! delete). There is no listing of tokens anywhere: the record id is a hash.
//!
//! **Observers.** `Dispatcher::run` hands the raw answer of every command to the registered
//! observers before it is shaped, and that answer of `create` holds the link. The two observers of
//! the connectors runtime read only the `id` of the answer (`obop-connectors` `runtime.rs`); the
//! link exists in memory in the answer and nowhere else. Proxy access logs will contain the path
//! of a signer's request, which holds the token (docs/SIGN.md says so).

use super::*;
use crate::Value;
use obop_storage::Op;

/// Links one person may make in an hour.
const MINT_PER_HOUR: u64 = 20;

pub(super) fn install(d: &Dispatcher, svc: &Arc<SignService>) {
    let s = svc.clone();
    d.around(
        "sign_link.create",
        Box::new(move |p, input, _| s.create_link(p, input)),
    );
    let s = svc.clone();
    d.around(
        "sign_link.update",
        Box::new(move |p, input, inner| s.update_link(p, input, inner)),
    );
    d.around(
        "sign_link.delete",
        Box::new(|p, input, inner| {
            if !is_admin(p) {
                return Err(
                    "sign_link: a link is kept as a record of what was issued; revoke it instead (set revoked_at)"
                        .into(),
                );
            }
            inner(input)
        }),
    );
    d.around(
        "sign_link.bulk_create",
        Box::new(|_, _, _| {
            Err("sign_link: links are made one at a time by sign_link.create".into())
        }),
    );
}

impl SignService {
    fn create_link(&self, p: &Principal, input: &Value) -> Result<Value, String> {
        if !p.is_person() {
            return Err(
                "sign_link: a signing link is a secret; ask a person to create it".to_string(),
            );
        }
        if !p.allows("sign.manager") {
            return Err("sign_link: creating a signing link needs the sign.manager group".into());
        }
        let Value::Map(m) = input else {
            return Err("sign_link: expected an object".into());
        };
        for (k, v) in m {
            // An empty `id` is what a form sends for a new record; a link's id is a hash of its
            // secret and is never chosen by the caller.
            let blank = matches!(v, Value::Null) || *v == Value::Text(String::new());
            if !matches!(k.as_str(), "request_id" | "expires_at")
                && !(k == "id" && blank)
                && !k.starts_with(crate::query::ACTOR_PREFIX)
                && k != "_caller"
            {
                return Err(format!(
                    "sign_link.{k} cannot be given: the server sets it (give request_id, and optionally expires_at)"
                ));
            }
        }
        let rid = match m.get("request_id") {
            Some(Value::Text(t)) if !t.is_empty() => t.clone(),
            _ => return Err("sign_link.request_id is required".into()),
        };
        if let Err(wait) = obop_storage::limit::hit(
            &*self.store.backend,
            &format!(
                "rl:sign:mint:{}",
                token::short_hash("obop-sign-mint:", &p.id)
            ),
            MINT_PER_HOUR,
            3_600_000,
        ) {
            return Err(format!(
                "sign_link: too many links were made lately; try again in {wait} seconds"
            ));
        }
        let req = self
            .get("sign_request", &rid)?
            .ok_or_else(|| format!("there is no sign_request {rid}"))?;
        if text(&req, "state") != "sent" {
            return Err(format!(
                "sign_link: sign_request {rid} is {}; a link can be made for a sent request",
                text(&req, "state")
            ));
        }
        let (doc_id, hash, file) = (
            text(&req, "document_id"),
            text(&req, "document_hash"),
            text(&req, "document_file"),
        );
        if doc_id.is_empty() || hash.is_empty() || file != hash {
            return Err(format!(
                "sign_link: sign_request {rid} was not sent with send_document, so it holds no copy of a document"
            ));
        }
        let doc = self.visible_document(p, input, doc_id)?;
        if text(&doc, "attachment") != hash {
            return Err(
                "sign_link: the document changed since the request was sent; send a new request"
                    .into(),
            );
        }
        self.checked_pdf(hash)?;
        // When the link stops working: 14 days, or less when asked, and never after the last day
        // of the request.
        let now = self.now_micros();
        let mut end = now + LINK_DAYS * MICROS_PER_DAY;
        let on = text(&req, "expires_on");
        if !on.is_empty() {
            match end_of_day_micros(on) {
                Some(day_end) if day_end < now => {
                    return Err(format!("sign_link: sign_request {rid} has expired ({on})"));
                }
                Some(day_end) => end = end.min(day_end),
                None => {}
            }
        }
        if let Some(Value::Text(asked)) = m.get("expires_at").filter(|v| **v != Value::Null) {
            let asked = instant_micros(asked)
                .ok_or("sign_link.expires_at must be a date and time (UTC)".to_string())?;
            if asked <= now {
                return Err("sign_link.expires_at must be in the future".into());
            }
            end = end.min(asked);
        }
        let tok = token::mint()?;
        let id = token::link_id(&rid, &tok);
        let expires_at = micros_text(end);
        let mut f = Fields::new();
        f.insert("request_id".into(), J::from(rid.clone()));
        f.insert("expires_at".into(), J::from(expires_at.clone()));
        let id12: String = id.chars().take(12).collect();
        let ops = [
            Op::Insert {
                model: "sign_link".into(),
                id: id.clone(),
                fields: f,
            },
            crate::audit::event_op(
                u64::try_from(now / MICROS_PER_SECOND).unwrap_or(0),
                &format!("sign_request:{rid}"),
                "sign_link_issued",
                &p.id,
                &format!(
                    "link {id12}; expires {expires_at}; document {}",
                    hash.chars().take(16).collect::<String>()
                ),
            ),
        ];
        self.store
            .backend
            .apply_checked(&ops)
            .map_err(|e| format!("could not make the link: {e}"))?;
        let path = format!("/sign/{rid}/{tok}");
        let link = match self.public_url() {
            Some(base) => format!("{base}{path}"),
            None => path,
        };
        Ok(Value::map([
            ("id", Value::Text(id)),
            ("request_id", Value::Text(rid)),
            ("expires_at", Value::Text(expires_at)),
            ("link", Value::Text(link)),
        ]))
    }

    /// The only change a link takes: `revoked_at`, stamped with the server's clock whatever the
    /// caller wrote, once, on a link that was not used. An administrator may change any field.
    fn update_link(
        &self,
        p: &Principal,
        input: &Value,
        inner: &dyn Fn(&Value) -> Result<Value, String>,
    ) -> Result<Value, String> {
        if is_admin(p) {
            return inner(input);
        }
        let Value::Map(m) = input else {
            return inner(input);
        };
        let id = id_of(input)?;
        let Some(cur) = self.get("sign_link", &id)? else {
            return inner(input);
        };
        let mut out = m.clone();
        let mut revoking = false;
        for (k, v) in m {
            if k == "id" || k == "_caller" || k.starts_with(crate::query::ACTOR_PREFIX) {
                continue;
            }
            if k == "revoked_at" {
                if *v == Value::Null || *v == Value::Text(String::new()) {
                    if text(&cur, "revoked_at").is_empty() {
                        out.remove("revoked_at");
                        continue;
                    }
                    return Err("sign_link: a revoked link cannot be made to work again".into());
                }
                if !text(&cur, "revoked_at").is_empty() {
                    // Echoing the stored value back is not a change.
                    if *v == Value::Text(text(&cur, "revoked_at").to_string()) {
                        continue;
                    }
                    return Err("sign_link: the link is already revoked".into());
                }
                if !text(&cur, "used_at").is_empty() {
                    return Err("sign_link: the link was already used to sign".into());
                }
                revoking = true;
                continue;
            }
            // Any other field may only be sent back as it is stored.
            let same = match (cur.get(k), v) {
                (None | Some(J::Null), Value::Null) => true,
                (Some(c), v) => *c == v.to_json(),
                (None, _) => false,
            };
            if !same {
                return Err(format!(
                    "sign_link.{k} cannot be changed; to stop a link, set revoked_at"
                ));
            }
            out.remove(k);
        }
        if revoking {
            out.insert("revoked_at".into(), Value::Text(self.now_text()));
        } else {
            out.remove("revoked_at");
        }
        inner(&Value::Map(out))
    }
}
