//! `obop secrets rotate` and `obop secrets status` (B3, D59).
//!
//! Rotation changes which key seals new values and re-seals the old ones, without a maintenance
//! window. The operator's steps (docs/DEPLOY.md):
//!
//! 1. Make a new key. Set `OBOP_SECRET_KEY` to it and `OBOP_SECRET_KEY_PREVIOUS` to the old one
//!    (a comma-separated list when there have been several), restart or deploy. The servers now
//!    seal with the new key and still open everything the old one sealed.
//! 2. Run `obop secrets rotate --db URL` with the same environment, any time, while serving.
//! 3. When the report says nothing is left under an older key, remove it from
//!    `OBOP_SECRET_KEY_PREVIOUS` at the next deploy.
//!
//! Before re-sealing, the stored pseudonym key of every connection is made to exist (see
//! `obop_connectors::secretbox`), so the pseudonyms already handed to receivers do not change.

use obop_connectors::SecretBox;
use obop_keys::reseal::{self, Options, Report};
use obop_keys::{KeyRing, key_id};
use obop_storage::Backend;
use serde_json::Value as J;
use std::sync::Arc;

/// Rotate: make sure the pseudonym keys are stored, then re-seal everything. `progress` gets a
/// line per batch.
pub fn rotate(
    backend: &Arc<dyn Backend>,
    ring: &KeyRing,
    opts: Options,
    progress: &mut dyn FnMut(&str),
) -> Result<Report, String> {
    if ring.current().is_none() {
        return Err("no current key: set OBOP_SECRET_KEY".into());
    }
    if !opts.dry_run {
        materialise_pseudonym_keys(backend, ring)?;
    }
    reseal::run(backend, ring, opts, progress)
}

/// Store the pseudonym key of every connection that has none, derived from the key its other
/// secrets were sealed with (else the most recent previous key, else the current one).
fn materialise_pseudonym_keys(backend: &Arc<dyn Backend>, ring: &KeyRing) -> Result<usize, String> {
    let sb = SecretBox::from_ring_with_store(ring.clone(), backend.clone())?;
    let mut n = 0;
    for (id, rec) in backend.list(obop_connectors::model::CONNECTION)? {
        let anchor = ["credentials", "oauth_tokens"]
            .iter()
            .filter_map(|f| rec.get(*f).and_then(J::as_str))
            .filter_map(obop_keys::sealed_key_id)
            .find_map(|kid| ring.find(&kid))
            .or_else(|| ring.previous().first());
        for name in [format!("pseudonym:{id}"), format!("pseudonym:{id}:vault")] {
            let existed = backend
                .get(obop_connectors::secretbox::PSEUDONYM_KEY_MODEL, &name)?
                .is_some();
            sb.materialise_pseudonym_key(&name, anchor)?;
            if !existed {
                n += 1;
            }
        }
    }
    Ok(n)
}

/// What `obop secrets status` shows: the keys of the ring by id, and which key protects how
/// many stored values (a read-only pass, the first half of a dry run).
pub fn status(backend: &Arc<dyn Backend>, ring: &KeyRing) -> Result<String, String> {
    let mut out = String::new();
    match ring.current() {
        Some(c) => out.push_str(&format!("current key: {} ({:?})\n", c.id(), c.source())),
        None => out.push_str("current key: none (set OBOP_SECRET_KEY)\n"),
    }
    for k in ring.previous() {
        out.push_str(&format!("previous key: {}\n", k.id()));
    }
    if ring.current().is_some() {
        let r = reseal::run(
            backend,
            ring,
            Options {
                dry_run: true,
                skip_unopenable: true,
                ..Options::default()
            },
            &mut |_| {},
        )?;
        out.push_str(&format!("sealed values: {}\n", r.scanned));
        for (k, n) in &r.by_key_before {
            let note = if *k
                == ring
                    .current()
                    .map(|c| c.id().to_string())
                    .unwrap_or_default()
            {
                " (current)"
            } else {
                ""
            };
            out.push_str(&format!("  sealed with {k}: {n}{note}\n"));
        }
        for p in &r.unopenable {
            out.push_str(&format!("  cannot be opened: {} ({})\n", p.place, p.why));
        }
        if r.to_reseal == 0 && r.unopenable.is_empty() {
            out.push_str("nothing to re-seal: every sealed value is under the current key\n");
        } else if r.to_reseal > 0 {
            out.push_str(&format!(
                "{} value(s) are under another key or an older format: run `obop secrets rotate`\n",
                r.to_reseal
            ));
        }
    }
    Ok(out)
}

/// The id of a key text, for documentation and tests.
pub fn id_of(raw: &str) -> String {
    key_id(raw)
}

#[cfg(test)]
mod tests;
