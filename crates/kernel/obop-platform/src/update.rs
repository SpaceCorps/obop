//! The control plane's half of updating an instance from inside it (D63).
//!
//! Releases are published here by an operator (`obop platform release publish`) and read by
//! instances at `GET /api/releases`. An instance holds a per-instance *update token*, minted
//! when the instance is created: 256 random bits, shown to the instance once, kept here only as a
//! SHA-256 hash with an expiry. The token is good for exactly three things, each for its own
//! instance: read the state of its update, ask to be moved to a published release, ask to go
//! back to the image it had before. It cannot name an image (only a published version), cannot
//! reach any other instance or any account API, and every use is rate limited and written to
//! the audit log. The upgrade itself is the existing rolling-upgrade path
//! (`Provisioner::upgrade`: redeploy, wait for the health check); when the new version does not
//! come up the control plane puts the previous image back before it reports the failure.

use super::{Instance, Platform, TABLE, now};
use obop_storage::{Fields, Op};
use obop_update::Release;
use serde_json::{Value as J, json};
use sha2::{Digest, Sha256};
use std::sync::Arc;

const RELEASES: &str = "platform_release";
pub(crate) const TOKENS: &str = "platform_update_token";
/// How long a minted token works. A token can be re-minted by the operator at any time.
pub const TOKEN_LIFETIME_SECS: u64 = 365 * 24 * 3600;
/// Update or rollback requests per instance per hour.
pub(crate) const START_PER_HOUR: u64 = 12;
/// Reads (status polling) per instance per hour.
const READS_PER_HOUR: u64 = 3600;
/// Failed token checks per minute, for the whole control plane.
const BAD_PER_MINUTE: u64 = 120;

pub const TOKEN_PREFIX: &str = "obu_";

/// The hash a token is stored under.
pub fn hash_token(token: &str) -> String {
    obop_keys::hex(&Sha256::digest(token.as_bytes()))
}

/// A new token: `obu_` and 64 hex digits.
pub fn new_token() -> String {
    format!(
        "{TOKEN_PREFIX}{}",
        obop_keys::hex(&obop_keys::random::<32>())
    )
}

/// What an update token authenticates.
#[derive(Debug, Clone)]
pub struct Caller {
    pub instance: String,
    pub slug: String,
}

impl Platform {
    /// The address instances are told to reach the control plane at.
    pub fn set_public_url(&self, url: &str) {
        *self.public_url.lock().unwrap_or_else(|e| e.into_inner()) =
            url.trim_end_matches('/').to_string();
    }

    pub fn public_url(&self) -> String {
        self.public_url
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Record a new token for the instance `id`, replacing (revoking) any earlier one. The hash
    /// goes in the token table under its own key, so authentication is one lookup.
    pub(crate) fn store_update_token(&self, id: &str, token: &str) -> Result<(), String> {
        let old = self.backend.get(TABLE, id)?.and_then(|f| {
            f.get("update_token_hash")
                .and_then(J::as_str)
                .map(String::from)
        });
        let hash = hash_token(token);
        let mut t = Fields::new();
        t.insert("instance".into(), J::from(id));
        t.insert("expires_at".into(), J::from(now() + TOKEN_LIFETIME_SECS));
        t.insert("created_at".into(), J::from(now()));
        let mut patch = Fields::new();
        patch.insert("update_token_hash".into(), J::from(hash.as_str()));
        patch.insert(
            "update_token_expires".into(),
            J::from(now() + TOKEN_LIFETIME_SECS),
        );
        self.backend
            .apply_checked(&[
                Op::Put {
                    model: TOKENS.into(),
                    id: hash.clone(),
                    fields: t,
                },
                Op::Merge {
                    model: TABLE.into(),
                    id: id.into(),
                    patch,
                },
            ])
            .map_err(|e| e.to_string())?;
        // The earlier token stops working at once.
        if let Some(o) = old.filter(|o| *o != hash) {
            let _ = self.backend.delete(TOKENS, &o);
        }
        Ok(())
    }

    /// Mint a fresh token for an instance (the operator's way to rotate, or to hand a token to
    /// an instance that was registered by hand). The token is returned once.
    pub fn mint_update_token(&self, slug: &str) -> Result<String, String> {
        let id = self.id_of(slug)?;
        let token = new_token();
        self.store_update_token(&id, &token)?;
        self.audit(
            slug,
            "update_token_minted",
            "operator",
            "a new update token was issued",
        );
        Ok(token)
    }

    /// Stop a token from working before it expires.
    pub fn revoke_update_token(&self, slug: &str) -> Result<(), String> {
        let id = self.id_of(slug)?;
        if let Some(h) = self.backend.get(TABLE, &id)?.and_then(|f| {
            f.get("update_token_hash")
                .and_then(J::as_str)
                .map(String::from)
        }) {
            self.backend.delete(TOKENS, &h)?;
        }
        self.audit(
            slug,
            "update_token_revoked",
            "operator",
            "the update token was revoked",
        );
        Ok(())
    }

    fn id_of(&self, slug: &str) -> Result<String, String> {
        self.backend
            .list(TABLE)?
            .into_iter()
            .find(|(_, f)| {
                f.get("slug").and_then(J::as_str) == Some(slug)
                    && f.get("status").and_then(J::as_str) != Some("deleted")
            })
            .map(|(id, _)| id)
            .ok_or_else(|| format!("no instance {slug}"))
    }

    /// Register an instance that this control plane did not create (one started by hand, or a
    /// test), so it can hold an update token. `token` is stored hashed; when none is given a
    /// fresh one is minted. Returns the token.
    pub fn register_instance(
        &self,
        slug: &str,
        url: &str,
        external_id: &str,
        image: &str,
        token: Option<&str>,
    ) -> Result<String, String> {
        if super::slugify(slug).as_deref() != Some(slug) {
            return Err("the slug must be 3 to 30 lowercase letters, digits or hyphens".into());
        }
        let _names = self.backend.lock("platform.instances")?;
        if self.id_of(slug).is_ok() {
            return Err(format!("the name {slug} is taken"));
        }
        let token = match token {
            Some(t) if t.len() >= 32 && t.starts_with(TOKEN_PREFIX) => t.to_string(),
            Some(_) => {
                return Err(format!(
                    "a token starts with {TOKEN_PREFIX} and has at least 32 characters"
                ));
            }
            None => new_token(),
        };
        let id = uuid::Uuid::now_v7().to_string();
        let mut f = Fields::new();
        for (k, v) in [
            ("owner", "operator"),
            ("name", slug),
            ("slug", slug),
            ("status", "running"),
            ("url", url),
            ("error", ""),
            ("external_id", external_id),
            ("image", image),
            ("registered", "true"),
        ] {
            f.insert(k.into(), J::from(v));
        }
        f.insert("created_at".into(), J::from(now()));
        self.backend.insert(TABLE, &id, &f)?;
        self.store_update_token(&id, &token)?;
        self.audit(slug, "instance_registered", "operator", url);
        Ok(token)
    }

    // ---- releases ----------------------------------------------------------------------

    /// Publish (or replace) a release.
    pub fn publish_release(
        &self,
        version: &str,
        image: &str,
        notes: &str,
        min_from: &str,
    ) -> Result<Release, String> {
        let v = obop_pkg::semver::Version::parse(version)
            .map_err(|_| "the version must look like 0.2.1".to_string())?;
        if v.is_prerelease() {
            return Err("the core has no pre-releases (D55)".into());
        }
        if image.is_empty() || image.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(
                "the image must be a reference such as ghcr.io/spacecorps/obop:0.2.1".into(),
            );
        }
        if !min_from.is_empty() && obop_pkg::semver::Version::parse(min_from).is_err() {
            return Err("min_from must look like 0.2.0".into());
        }
        if notes.chars().count() > 8000 {
            return Err("the notes are longer than 8000 characters".into());
        }
        let r = Release::new(version, image, notes, now(), min_from);
        let mut f = Fields::new();
        for (k, v) in [
            ("version", r.version.as_str()),
            ("image", r.image.as_str()),
            ("notes", r.notes.as_str()),
            ("min_from", r.min_from.as_str()),
        ] {
            f.insert(k.into(), J::from(v));
        }
        f.insert("published_at".into(), J::from(r.published_at));
        f.insert("withdrawn".into(), J::from(false));
        self.backend
            .apply_checked(&[Op::Put {
                model: RELEASES.into(),
                id: version.into(),
                fields: f,
            }])
            .map_err(|e| e.to_string())?;
        self.audit(version, "release_published", "operator", image);
        Ok(r)
    }

    /// Stop offering a release (instances that already run it are not touched).
    pub fn withdraw_release(&self, version: &str) -> Result<(), String> {
        let mut p = Fields::new();
        p.insert("withdrawn".into(), J::from(true));
        if !self.backend.update(RELEASES, version, &p)? {
            return Err(format!("no release {version}"));
        }
        self.audit(version, "release_withdrawn", "operator", "");
        Ok(())
    }

    /// The published releases, newest first.
    pub fn releases(&self) -> Vec<Release> {
        let mut v: Vec<Release> = self
            .backend
            .list(RELEASES)
            .unwrap_or_default()
            .into_iter()
            .filter(|(_, f)| f.get("withdrawn").and_then(J::as_bool) != Some(true))
            .filter_map(|(_, f)| release_of(&f))
            .collect();
        v.sort_by_key(|r| std::cmp::Reverse(r.semver()));
        v
    }

    /// `GET /api/releases`.
    pub fn feed_json(&self) -> J {
        json!({
            "generated_at": now(),
            "releases": self.releases().iter().map(Release::to_json).collect::<Vec<_>>(),
        })
    }

    fn release(&self, version: &str) -> Option<Release> {
        self.backend
            .get(RELEASES, version)
            .ok()
            .flatten()
            .filter(|f| f.get("withdrawn").and_then(J::as_bool) != Some(true))
            .and_then(|f| release_of(&f))
    }

    // ---- the token's three powers ------------------------------------------------------

    /// Who an update token belongs to, or why it is not accepted. Wrong, expired and revoked
    /// tokens all look the same from outside; failures are counted and limited.
    pub fn authenticate_update(&self, token: &str) -> Result<Caller, String> {
        let bad = || {
            // Guessing a 256-bit token is hopeless; this stops a flood anyway.
            let n = self
                .backend
                .counter_add("platform.update.bad", 60_000, false)
                .unwrap_or(0);
            if n > BAD_PER_MINUTE {
                "too many requests".to_string()
            } else {
                "not a valid update token".to_string()
            }
        };
        if !token.starts_with(TOKEN_PREFIX) || token.len() > 200 {
            return Err(bad());
        }
        let hash = hash_token(token);
        let Some(row) = self.backend.get(TOKENS, &hash).ok().flatten() else {
            return Err(bad());
        };
        if row.get("expires_at").and_then(J::as_u64).unwrap_or(0) <= now() {
            return Err(bad());
        }
        let id = row.get("instance").and_then(J::as_str).unwrap_or("");
        let Some(f) = self.backend.get(TABLE, id).ok().flatten() else {
            return Err(bad());
        };
        // The record must still point at this very token (a re-mint replaces it).
        if f.get("update_token_hash").and_then(J::as_str) != Some(hash.as_str())
            || f.get("status").and_then(J::as_str) == Some("deleted")
        {
            return Err(bad());
        }
        Ok(Caller {
            instance: id.to_string(),
            slug: f.get("slug").and_then(J::as_str).unwrap_or("").to_string(),
        })
    }

    fn limit(&self, c: &Caller, what: &str, per_hour: u64) -> Result<(), String> {
        let n = self
            .backend
            .counter_add(
                &format!("platform.update.{what}.{}", c.instance),
                3_600_000,
                false,
            )
            .map_err(|e| e.to_string())?;
        if n > per_hour {
            return Err("too many requests; try again later".into());
        }
        Ok(())
    }

    fn audit(&self, subject: &str, kind: &str, actor: &str, detail: &str) {
        obop_commands::audit::record_event(&*self.backend, now(), subject, kind, actor, detail);
    }

    /// The state of the instance's update: what the page polls.
    pub fn update_status(&self, token: &str) -> Result<J, String> {
        let c = self.authenticate_update(token)?;
        self.limit(&c, "read", READS_PER_HOUR)?;
        let f = self
            .backend
            .get(TABLE, &c.instance)?
            .ok_or("no such instance")?;
        let s = |k: &str| f.get(k).and_then(J::as_str).unwrap_or("").to_string();
        let state = if s("update_state").is_empty() {
            "idle".to_string()
        } else {
            s("update_state")
        };
        Ok(json!({
            "state": state,
            "phase": if state == "running" { s("phase") } else { String::new() },
            "target": s("update_target"),
            "from": s("update_from"),
            "image": s("image"),
            "error": s("update_error"),
            "can_rollback": !s("update_prev_image").is_empty(),
            "updated_at": f.get("update_at").and_then(J::as_u64).unwrap_or(0),
        }))
    }

    /// Move the instance to the published release `version`. Returns at once; the work runs in
    /// the background and `update_status` follows it.
    pub fn update_start(
        self: &Arc<Self>,
        token: &str,
        version: &str,
        from_version: &str,
    ) -> Result<J, String> {
        let c = self.authenticate_update(token)?;
        self.limit(&c, "start", START_PER_HOUR)?;
        let Some(release) = self.release(version) else {
            self.audit(
                &c.slug,
                "instance_update_refused",
                "instance",
                &format!("{version} is not published"),
            );
            return Err(format!("{version} is not a published release"));
        };
        self.begin_upgrade(&c, &release.image, version, from_version, false)
    }

    /// Put the instance back on the image it had before its last update.
    pub fn update_rollback(self: &Arc<Self>, token: &str) -> Result<J, String> {
        let c = self.authenticate_update(token)?;
        self.limit(&c, "start", START_PER_HOUR)?;
        let f = self
            .backend
            .get(TABLE, &c.instance)?
            .ok_or("no such instance")?;
        let prev = f
            .get("update_prev_image")
            .and_then(J::as_str)
            .unwrap_or("")
            .to_string();
        if prev.is_empty() {
            return Err("this instance has no earlier image to go back to".into());
        }
        let target = f
            .get("update_from")
            .and_then(J::as_str)
            .unwrap_or("")
            .to_string();
        let from = f
            .get("update_target")
            .and_then(J::as_str)
            .unwrap_or("")
            .to_string();
        self.begin_upgrade(&c, &prev, &target, &from, true)
    }

    fn begin_upgrade(
        self: &Arc<Self>,
        c: &Caller,
        image: &str,
        target: &str,
        from: &str,
        is_rollback: bool,
    ) -> Result<J, String> {
        // One request at a time per instance, across control-plane processes.
        let _guard = self
            .backend
            .lock(&format!("platform.update.{}", c.instance))?;
        let f = self
            .backend
            .get(TABLE, &c.instance)?
            .ok_or("no such instance")?;
        let i = Instance::from_fields(&c.instance, &f);
        if i.status != "running" {
            return Err(format!("the instance is {}, not running", i.status));
        }
        if f.get("update_state").and_then(J::as_str) == Some("running") {
            return Err("an update is already running for this instance".into());
        }
        if i.image == image {
            return Err("the instance already runs this release".into());
        }
        let ext = f
            .get("external_id")
            .and_then(J::as_str)
            .unwrap_or("")
            .to_string();
        let prev_image = i.image.clone();
        let mut patch = Fields::new();
        for (k, v) in [
            ("update_state", "running"),
            ("update_target", target),
            ("update_from", from),
            ("update_error", ""),
            ("phase", "Queued"),
        ] {
            patch.insert(k.into(), J::from(v));
        }
        // A rollback leaves no further way back (it would be a loop); an update remembers the
        // image it replaces.
        patch.insert(
            "update_prev_image".into(),
            J::from(if is_rollback { "" } else { prev_image.as_str() }),
        );
        patch.insert("update_at".into(), J::from(now()));
        self.backend.update(TABLE, &c.instance, &patch)?;
        self.audit(
            &c.slug,
            if is_rollback {
                "instance_rollback_started"
            } else {
                "instance_update_started"
            },
            "instance",
            &format!("{from} -> {target} ({image})"),
        );
        let answer = json!({"state": "running", "target": target});
        let me = self.clone();
        let (id, slug, image, target) = (
            c.instance.clone(),
            c.slug.clone(),
            image.to_string(),
            target.to_string(),
        );
        std::thread::spawn(move || {
            let phase = |ph: &str| me.set_phase(&id, ph);
            let mut done = Fields::new();
            done.insert("phase".into(), J::from(""));
            done.insert("update_at".into(), J::from(now()));
            match me.provisioner.upgrade(&slug, &ext, &image, &phase) {
                Ok(()) => {
                    done.insert("image".into(), J::from(image.as_str()));
                    done.insert("update_state".into(), J::from("succeeded"));
                    me.audit(&slug, "instance_update_succeeded", "control-plane", &image);
                }
                Err(e) => {
                    // The new version did not come up: put the old one back before saying so.
                    let back = (!prev_image.is_empty())
                        .then(|| me.provisioner.upgrade(&slug, &ext, &prev_image, &phase));
                    match back {
                        Some(Ok(())) => {
                            done.insert("update_state".into(), J::from("rolled_back"));
                            done.insert(
                                "update_error".into(),
                                J::from(format!(
                                    "{target} did not start ({e}); the instance is back on the previous version"
                                )),
                            );
                            me.audit(
                                &slug,
                                "instance_update_rolled_back",
                                "control-plane",
                                &format!("{image}: {e}"),
                            );
                        }
                        other => {
                            done.insert("update_state".into(), J::from("failed"));
                            let tail = match other {
                                Some(Err(e2)) => format!(
                                    "; putting the previous version back also failed ({e2})"
                                ),
                                _ => String::new(),
                            };
                            done.insert(
                                "update_error".into(),
                                J::from(format!("{target} did not start ({e}){tail}; an operator must look at it")),
                            );
                            me.audit(
                                &slug,
                                "instance_update_failed",
                                "control-plane",
                                &format!("{image}: {e}{tail}"),
                            );
                        }
                    }
                }
            }
            let _ = me.backend.update(TABLE, &id, &done);
        });
        Ok(answer)
    }
}

fn release_of(f: &Fields) -> Option<Release> {
    let t = |k: &str| f.get(k).and_then(J::as_str).unwrap_or("").to_string();
    Some(Release::new(
        &t("version"),
        &t("image"),
        &t("notes"),
        f.get("published_at").and_then(J::as_u64).unwrap_or(0),
        &t("min_from"),
    ))
}
