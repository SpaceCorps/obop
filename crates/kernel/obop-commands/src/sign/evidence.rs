//! The reader of the `sign_signed` entry (docs/ISSUES.md O3, O10, O19): `sign_request.get` of a
//! signed request carries `evidence_check`, which compares the evidence stored on the request
//! now with the digest the door wrote to the security log at signing, and, for an entry written
//! from `sign` 0.3.0 on, the title, message and requested name the signer read with the wording
//! digest of that entry.
//!
//! What it shows is narrow, and docs/SIGN.md says so: an edit made through the commands (an
//! administrator's `update` of a signed request) shows as `differs`, because no command writes
//! the security log. The digests are plain SHA-256 and the entry is a row of the same database,
//! so someone who can write the database can rewrite both and the check still says `matches`.
//! It is not a proof of tamper-evidence. It is worked out when `get` is answered and never
//! stored; `list` and `search` leave it empty (the lookup of the entry reads the security log).

use super::*;
use crate::Value;

/// The field `get` fills.
pub(crate) const FIELD: &str = "evidence_check";

/// The digest of what the signer read of the request: the title, the message and the requested
/// name, as stored.
pub(super) fn wording_digest(req: &Fields) -> String {
    super::door::evidence_digest(&[
        ("message", text(req, "message")),
        ("signer_name", text(req, "signer_name")),
        ("title", text(req, "title")),
    ])
}

/// The value of one `key {value};` part of an entry's detail, read up to the next `;`. Only the
/// parts the door writes before the address are read: what follows them is what a client sent.
fn part<'a>(detail: &'a str, key: &str) -> Option<&'a str> {
    let head = detail.split("; address ").next().unwrap_or(detail);
    head.split("; ")
        .find_map(|p| p.strip_prefix(key).and_then(|r| r.strip_prefix(' ')))
        .map(|v| v.trim_end_matches(';'))
}

pub(super) fn install(d: &Dispatcher, svc: &Arc<SignService>) {
    let s = svc.clone();
    d.around(
        "sign_request.get",
        Box::new(move |_, input, inner| {
            let mut out = inner(input)?;
            if let Value::Map(m) = &mut out
                && let Some(Value::Text(id)) = m.get("id").cloned()
                && let Some(check) = s.evidence_check(&id)?
            {
                m.insert(FIELD.into(), Value::Text(check.into()));
            }
            Ok(out)
        }),
    );
}

impl SignService {
    /// `matches`, `differs` or `no_entry` for a request that was signed at the door (it holds
    /// `signed_at`); `None` for any other.
    pub(crate) fn evidence_check(&self, rid: &str) -> Result<Option<&'static str>, String> {
        let Some(req) = self.get("sign_request", rid)? else {
            return Ok(None);
        };
        if text(&req, "state") != "signed" || text(&req, "signed_at").is_empty() {
            return Ok(None);
        }
        let entries = self.store.backend.find(
            crate::audit::EVENT_TABLE,
            "user",
            &J::from(format!("sign_request:{rid}")),
        )?;
        let Some(detail) = entries
            .iter()
            .filter(|(_, e)| text(e, "kind") == "sign_signed")
            .map(|(_, e)| text(e, "detail").to_string())
            .next()
        else {
            return Ok(Some("no_entry"));
        };
        let (Some(id12), Some(digest)) = (part(&detail, "link"), part(&detail, "digest")) else {
            return Ok(Some("differs"));
        };
        // The link that signed: the used link of this request whose id the entry begins.
        let link = self
            .store
            .backend
            .find("sign_link", "request_id", &J::from(rid))?
            .into_iter()
            .find(|(id, l)| id.starts_with(id12) && !text(l, "used_at").is_empty());
        let Some((link_id, _)) = link else {
            return Ok(Some("differs"));
        };
        if !token::ct_eq(&digest_of_request(&link_id, rid, &req), digest) {
            return Ok(Some("differs"));
        }
        if let Some(wording) = part(&detail, "wording")
            && !token::ct_eq(&wording_digest(&req), wording)
        {
            return Ok(Some("differs"));
        }
        Ok(Some("matches"))
    }
}

#[cfg(test)]
mod tests {
    use super::part;

    #[test]
    fn the_parts_are_read_from_the_doors_own_head_of_the_detail() {
        let d = "request r1; link abcdefabcdef; document 00ff; consent v1-draft/en/a; digest d1; wording w1; address 1.2.3.4; peer p; forwarded digest evil; wording evil";
        assert_eq!(part(d, "link"), Some("abcdefabcdef"));
        assert_eq!(part(d, "digest"), Some("d1"));
        assert_eq!(part(d, "wording"), Some("w1"));
        let old = "request r1; link abc; document 00ff; consent c; digest d1; address 1.2.3.4; peer p; forwarded wording evil";
        assert_eq!(part(old, "wording"), None, "an entry of 0.2.x has none");
    }
}
