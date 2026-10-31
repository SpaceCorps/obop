//! `obop secrets rotate` (B3): find every sealed value in the database and re-seal what the
//! current key does not already protect.
//!
//! The inventory below is the one list of where the server keeps sealed material: second-factor
//! secrets, connector credentials and OAuth tokens, the client secret of the Microsoft sign-in, the
//! stored pseudonym keys, the tokens of the mask vaults, the payloads of queued deliveries, and the short-lived sealed values (approval
//! plans, REST vaults, MCP sessions). A test in `obop-ops` checks it against the code that seals.
//!
//! The run is in two passes. The first only reads: it opens every value with the key ring and
//! counts what each key still protects; a value no key of the ring opens stops the run before
//! anything is written (unless told to skip those). The second re-seals in batches, each batch
//! one transaction of compare-and-set swaps, so a value changed by the running server in the
//! meantime is left alone and picked up by the next run. Stopping at any point, for any reason,
//! leaves a database that is entirely readable with the same ring; running again finishes.
//! Nothing in a report, a log line or an error carries a key, a plaintext or a sealed value.

use crate::{KeyError, KeyRing, legacy, sealed_key_id};
use obop_storage::{Backend, Swap};
use serde_json::Value as J;
use std::collections::BTreeMap;
use std::sync::Arc;

/// How the purpose (the associated data) of a sealed value is derived.
#[derive(Debug, Clone, Copy)]
pub enum Purpose {
    /// The same for every record of the model.
    Fixed(&'static str),
    /// A prefix and the record id.
    WithId(&'static str),
    /// `totp:<record id>`; the first format bound the user name as associated data.
    Totp,
    /// `vault:<the record's connection>`.
    VaultConnection,
}

#[derive(Debug, Clone, Copy)]
pub struct SealedField {
    pub model: &'static str,
    pub field: &'static str,
    pub purpose: Purpose,
}

/// Records with sealed text fields.
pub const FIELDS: &[SealedField] = &[
    SealedField {
        model: "auth_user",
        field: "totp_secret",
        purpose: Purpose::Totp,
    },
    SealedField {
        model: "auth_user",
        field: "totp_pending",
        purpose: Purpose::Totp,
    },
    SealedField {
        model: "connection",
        field: "credentials",
        purpose: Purpose::Fixed("connection-credentials"),
    },
    SealedField {
        model: "connection",
        field: "oauth_tokens",
        purpose: Purpose::Fixed("connection-oauth-tokens"),
    },
    SealedField {
        model: "provider_policy",
        field: "oauth_client_secret",
        purpose: Purpose::Fixed("provider-oauth-client"),
    },
    SealedField {
        model: "connector_vault",
        field: "value",
        purpose: Purpose::VaultConnection,
    },
    SealedField {
        model: "connector_oauth_state",
        field: "verifier",
        purpose: Purpose::WithId("oauth-state:"),
    },
    SealedField {
        model: "connector_pseudonym_key",
        field: "key",
        purpose: Purpose::WithId("pseudonym-key:"),
    },
    SealedField {
        model: "connector_job",
        field: "body",
        purpose: Purpose::WithId("queue-job:"),
    },
    SealedField {
        model: "connector_dead",
        field: "body",
        purpose: Purpose::WithId("queue-job:"),
    },
    // The instance's update token for the control plane (D63).
    SealedField {
        model: "obop_update",
        field: "token",
        purpose: Purpose::Fixed("update-token"),
    },
    // The client secret of the Microsoft sign-in (D91).
    SealedField {
        model: "auth_sso_provider",
        field: "client_secret",
        purpose: Purpose::Fixed("sso-client-secret"),
    },
];

/// Short-lived values whose purpose is their own key.
pub const KV_PREFIXES: &[&str] = &["plan:", "vault:", "mcp:s:"];

#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// Read and report only.
    pub dry_run: bool,
    /// Values per transaction.
    pub batch: usize,
    /// Re-seal what can be opened and leave the rest, instead of refusing to start.
    pub skip_unopenable: bool,
    /// Stop after this many batches (the run can be resumed): for operators who want to
    /// rotate gently, and for tests.
    pub max_batches: Option<usize>,
    /// Touch only values in an older format (a migration), not values another key sealed (a
    /// rotation).
    pub only_legacy: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            dry_run: false,
            batch: 200,
            skip_unopenable: false,
            max_batches: None,
            only_legacy: false,
        }
    }
}

/// A value that cannot be re-sealed, by place and reason (never by content).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub place: String,
    pub why: String,
}

#[derive(Debug, Default, Clone)]
pub struct Report {
    /// Sealed values found.
    pub scanned: usize,
    /// Already sealed with the current key.
    pub current: usize,
    /// Sealed by another key or in an older format: the work.
    pub to_reseal: usize,
    pub resealed: usize,
    /// Changed by the running server between reading and swapping; the next run takes them.
    pub changed_meanwhile: usize,
    /// Values per sealing key before the run (`older format` for values that name none).
    pub by_key_before: BTreeMap<String, usize>,
    pub unopenable: Vec<Problem>,
    /// Values per model, for the report.
    pub by_model: BTreeMap<String, usize>,
    pub batches: usize,
    /// The run stopped early (`max_batches`); run again to finish.
    pub stopped_early: bool,
}

impl Report {
    /// True when nothing is left that a run could do.
    pub fn complete(&self) -> bool {
        !self.stopped_early && self.changed_meanwhile == 0 && self.unopenable.is_empty()
    }
}

fn purpose_of(rule: Purpose, id: &str, rec: &obop_storage::Fields) -> (String, Option<String>) {
    match rule {
        Purpose::Fixed(p) => (p.to_string(), None),
        Purpose::WithId(prefix) => (format!("{prefix}{id}"), None),
        Purpose::Totp => (format!("totp:{id}"), Some(id.to_string())),
        Purpose::VaultConnection => (
            format!(
                "vault:{}",
                rec.get("connection").and_then(J::as_str).unwrap_or("")
            ),
            None,
        ),
    }
}

fn label(sealed: &str) -> String {
    sealed_key_id(sealed).unwrap_or_else(|| "older format".to_string())
}

fn why(e: &KeyError) -> String {
    match e {
        KeyError::UnknownKey(id) => format!("sealed with key {id}, which is not in the key ring"),
        KeyError::CannotOpen => "no key of the key ring opens it".into(),
        KeyError::Malformed => "malformed".into(),
        KeyError::NotSealed => "not a sealed value".into(),
        KeyError::NoKey => "no key".into(),
    }
}

/// One value found by a scan.
struct Found {
    place: String,
    sealed: String,
    purpose: String,
    legacy_user: Option<String>,
    target: Target,
}

enum Target {
    Field {
        model: String,
        id: String,
        field: String,
    },
    Kv {
        key: String,
    },
}

fn scan(
    backend: &dyn Backend,
    mut visit: impl FnMut(Found) -> Result<(), String>,
) -> Result<(), String> {
    for sf in FIELDS {
        for (id, rec) in backend.list(sf.model)? {
            let Some(v) = rec
                .get(sf.field)
                .and_then(J::as_str)
                .filter(|v| !v.is_empty())
            else {
                continue;
            };
            let (purpose, legacy_user) = purpose_of(sf.purpose, &id, &rec);
            visit(Found {
                place: format!("{}/{}.{}", sf.model, id, sf.field),
                sealed: v.to_string(),
                purpose,
                legacy_user,
                target: Target::Field {
                    model: sf.model.to_string(),
                    id,
                    field: sf.field.to_string(),
                },
            })?;
        }
    }
    for prefix in KV_PREFIXES {
        for (key, v) in backend.kv_scan(prefix)? {
            visit(Found {
                place: format!("short-lived/{key}"),
                sealed: v,
                purpose: key.clone(),
                legacy_user: None,
                target: Target::Kv { key },
            })?;
        }
    }
    Ok(())
}

/// Whether any sealed value in the database is still in one of the older formats.
pub fn has_legacy(backend: &dyn Backend) -> Result<bool, String> {
    let mut found = false;
    scan(backend, |f| {
        found |= legacy::is_legacy(&f.sealed);
        Ok(())
    })?;
    Ok(found)
}

/// Re-seal everything the current key does not protect. `progress` gets one line per batch.
pub fn run(
    backend: &Arc<dyn Backend>,
    ring: &KeyRing,
    opts: Options,
    progress: &mut dyn FnMut(&str),
) -> Result<Report, String> {
    let current = ring
        .current()
        .ok_or("no current key: set OBOP_SECRET_KEY")?
        .id()
        .to_string();
    let mut report = Report::default();

    // Pass one: read only.
    scan(&**backend, |f| {
        if !(crate::is_sealed(&f.sealed) || legacy::is_legacy(&f.sealed)) {
            return Ok(()); // not sealed material (an empty or revoked field)
        }
        if opts.only_legacy && !legacy::is_legacy(&f.sealed) {
            return Ok(());
        }
        report.scanned += 1;
        *report.by_key_before.entry(label(&f.sealed)).or_default() += 1;
        match legacy::open_any(ring, &f.purpose, f.legacy_user.as_deref(), &f.sealed) {
            Ok(_) => {
                if sealed_key_id(&f.sealed).as_deref() == Some(current.as_str()) {
                    report.current += 1;
                } else {
                    report.to_reseal += 1;
                    let model = f.place.split('/').next().unwrap_or("").to_string();
                    *report.by_model.entry(model).or_default() += 1;
                }
            }
            Err(e) => report.unopenable.push(Problem {
                place: f.place,
                why: why(&e),
            }),
        }
        Ok(())
    })?;
    if !report.unopenable.is_empty() && !opts.skip_unopenable {
        return Err(format!(
            "{} sealed value(s) cannot be opened with the key ring, so nothing was changed. \
             Put the missing key in OBOP_SECRET_KEY_PREVIOUS, or accept the loss of those values \
             with --skip-unopenable. First: {} ({})",
            report.unopenable.len(),
            report.unopenable[0].place,
            report.unopenable[0].why
        ));
    }
    if opts.dry_run || report.to_reseal == 0 {
        return Ok(report);
    }

    // Pass two: re-seal in transactions of at most `batch` swaps.
    let batch = opts.batch.max(1);
    let mut swaps: Vec<Swap> = Vec::new();
    let mut kv_swaps: Vec<(String, String, String)> = Vec::new();
    let mut flush = |swaps: &mut Vec<Swap>,
                     kv: &mut Vec<(String, String, String)>,
                     report: &mut Report|
     -> Result<(), String> {
        if swaps.is_empty() && kv.is_empty() {
            return Ok(());
        }
        let n = backend.swap_fields(swaps)?;
        let mut done = n;
        let mut missed = swaps.len() - n;
        for (k, old, new) in kv.iter() {
            if backend.kv_swap(k, old, new)? {
                done += 1;
            } else {
                missed += 1;
            }
        }
        report.resealed += done;
        report.changed_meanwhile += missed;
        report.batches += 1;
        progress(&format!(
            "batch {}: {} re-sealed, {} changed meanwhile",
            report.batches, done, missed
        ));
        swaps.clear();
        kv.clear();
        Ok(())
    };
    let mut stop = false;
    scan(&**backend, |f| {
        if stop || !(crate::is_sealed(&f.sealed) || legacy::is_legacy(&f.sealed)) {
            return Ok(());
        }
        if opts.only_legacy && !legacy::is_legacy(&f.sealed) {
            return Ok(());
        }
        if sealed_key_id(&f.sealed).as_deref() == Some(current.as_str()) {
            return Ok(());
        }
        let Ok(plain) = legacy::open_any(ring, &f.purpose, f.legacy_user.as_deref(), &f.sealed)
        else {
            return Ok(()); // skipped on request (or changed since the first pass)
        };
        let new = ring.seal(&f.purpose, &plain).map_err(|e| e.to_string())?;
        match f.target {
            Target::Field { model, id, field } => swaps.push(Swap {
                model,
                id,
                field,
                old: f.sealed,
                new,
            }),
            Target::Kv { key } => kv_swaps.push((key, f.sealed, new)),
        }
        if swaps.len() + kv_swaps.len() >= batch {
            flush(&mut swaps, &mut kv_swaps, &mut report)?;
            if opts.max_batches.is_some_and(|m| report.batches >= m) {
                stop = true;
                report.stopped_early = true;
            }
        }
        Ok(())
    })?;
    if !stop {
        flush(&mut swaps, &mut kv_swaps, &mut report)?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use obop_storage::Fields;

    const OLD: &str = "oooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooo";
    const NEW: &str = "nnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnn";
    const OLD_HEX: &str = "0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a";

    fn put(b: &Arc<dyn Backend>, model: &str, id: &str, field: &str, sealed: String) {
        let mut f = Fields::new();
        f.insert(field.into(), J::from(sealed));
        b.insert(model, id, &f).unwrap();
    }

    fn seeded(n_users: usize) -> (Arc<dyn Backend>, KeyRing) {
        let b = obop_storage::open("memory:").unwrap();
        let old = KeyRing::from_secrets(OLD, &[]).unwrap();
        for i in 0..n_users {
            let id = format!("user{i}");
            put(
                &b,
                "auth_user",
                &id,
                "totp_secret",
                old.seal(&format!("totp:{id}"), b"seed").unwrap(),
            );
        }
        put(
            &b,
            "connection",
            "c1",
            "credentials",
            old.seal("connection-credentials", b"{\"k\":1}").unwrap(),
        );
        put(&b, "connection", "c2", "credentials", String::new());
        let mut f = Fields::new();
        f.insert("connection".into(), J::from("c1"));
        f.insert(
            "value".into(),
            J::from(old.seal("vault:c1", b"Ada").unwrap()),
        );
        b.insert("connector_vault", "v1", &f).unwrap();
        b.kv_put(
            "plan:p1",
            &old.seal("plan:p1", b"{}").unwrap(),
            Some(60_000),
        )
        .unwrap();
        (b, KeyRing::from_secrets(NEW, &[OLD]).unwrap())
    }

    #[test]
    fn a_dry_run_reports_and_changes_nothing() {
        let (b, ring) = seeded(3);
        let before = b.get("auth_user", "user0").unwrap().unwrap();
        let r = run(
            &b,
            &ring,
            Options {
                dry_run: true,
                ..Options::default()
            },
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(
            (r.scanned, r.current, r.to_reseal, r.resealed),
            (6, 0, 6, 0)
        );
        assert_eq!(r.by_key_before[&crate::key_id(OLD)], 6);
        assert_eq!(b.get("auth_user", "user0").unwrap().unwrap(), before);
    }

    #[test]
    fn a_rotation_reseals_every_value_in_batches_and_is_idempotent() {
        let (b, ring) = seeded(5);
        let mut lines = Vec::new();
        let r = run(
            &b,
            &ring,
            Options {
                batch: 2,
                ..Options::default()
            },
            &mut |l| lines.push(l.to_string()),
        )
        .unwrap();
        assert_eq!((r.to_reseal, r.resealed, r.changed_meanwhile), (8, 8, 0));
        assert!(r.batches >= 4 && lines.len() == r.batches, "{lines:?}");
        assert!(r.complete());
        // Everything is now under the new key, and the old key is no longer needed.
        let only_new = KeyRing::from_secrets(NEW, &[]).unwrap();
        let rec = b.get("auth_user", "user3").unwrap().unwrap();
        assert_eq!(
            only_new
                .open("totp:user3", rec["totp_secret"].as_str().unwrap())
                .unwrap(),
            b"seed"
        );
        assert_eq!(
            only_new
                .open("plan:p1", &b.kv_get("plan:p1").unwrap().unwrap())
                .unwrap(),
            b"{}"
        );
        let again = run(&b, &ring, Options::default(), &mut |_| {}).unwrap();
        assert_eq!(
            (
                again.scanned,
                again.current,
                again.to_reseal,
                again.resealed
            ),
            (8, 8, 0, 0)
        );
        // No report line names a key or a value.
        assert!(!lines.join(" ").contains(OLD) && !format!("{r:?}").contains(NEW));
    }

    #[test]
    fn first_format_values_are_upgraded_with_the_same_key() {
        let b = obop_storage::open("memory:").unwrap();
        let legacy_totp =
            legacy::seal_totp_v1(&legacy::parse_key(OLD_HEX).unwrap(), "ada", b"seed");
        put(&b, "auth_user", "ada", "totp_secret", legacy_totp);
        put(
            &b,
            "connection",
            "c1",
            "credentials",
            legacy::seal_box_v1(OLD_HEX, "connection-credentials", b"x"),
        );
        let ring = KeyRing::from_secrets(OLD_HEX, &[]).unwrap();
        let r = run(&b, &ring, Options::default(), &mut |_| {}).unwrap();
        assert_eq!((r.scanned, r.resealed), (2, 2));
        assert_eq!(r.by_key_before["older format"], 2);
        let rec = b.get("connection", "c1").unwrap().unwrap();
        assert!(rec["credentials"].as_str().unwrap().starts_with("e2."));
    }

    #[test]
    fn a_value_no_key_opens_stops_the_run_before_any_write_unless_skipped() {
        let (b, _) = seeded(2);
        let stranger = KeyRing::from_secrets(&"s".repeat(64), &[]).unwrap();
        put(
            &b,
            "auth_user",
            "lost",
            "totp_secret",
            stranger.seal("totp:lost", b"seed").unwrap(),
        );
        let ring = KeyRing::from_secrets(NEW, &[OLD]).unwrap();
        let before = b.get("auth_user", "user0").unwrap().unwrap();
        let e = run(&b, &ring, Options::default(), &mut |_| {}).unwrap_err();
        assert!(e.contains("auth_user/lost.totp_secret") && e.contains("--skip-unopenable"));
        assert!(!e.contains(NEW) && !e.contains(OLD));
        assert_eq!(
            b.get("auth_user", "user0").unwrap().unwrap(),
            before,
            "nothing was written"
        );
        let r = run(
            &b,
            &ring,
            Options {
                skip_unopenable: true,
                ..Options::default()
            },
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(r.unopenable.len(), 1);
        assert_eq!(r.resealed, 5);
        assert!(!r.complete(), "the lost value is still reported");
    }

    #[test]
    fn a_stopped_run_leaves_everything_readable_and_the_next_run_finishes() {
        let (b, ring) = seeded(6);
        let r = run(
            &b,
            &ring,
            Options {
                batch: 2,
                max_batches: Some(1),
                ..Options::default()
            },
            &mut |_| {},
        )
        .unwrap();
        assert!(r.stopped_early && r.resealed == 2);
        // Half old, half new: all of it opens with the ring.
        for i in 0..6 {
            let id = format!("user{i}");
            let rec = b.get("auth_user", &id).unwrap().unwrap();
            ring.open(&format!("totp:{id}"), rec["totp_secret"].as_str().unwrap())
                .unwrap();
        }
        let rest = run(
            &b,
            &ring,
            Options {
                batch: 2,
                ..Options::default()
            },
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(rest.resealed, r.to_reseal - 2);
        assert!(rest.complete());
        let done = run(&b, &ring, Options::default(), &mut |_| {}).unwrap();
        assert_eq!(done.to_reseal, 0);
    }

    #[test]
    fn a_tampered_value_is_reported_not_resealed() {
        let (b, ring) = seeded(1);
        let rec = b.get("auth_user", "user0").unwrap().unwrap();
        let mut s = rec["totp_secret"].as_str().unwrap().to_string();
        // Change one hex digit inside the ciphertext to a different digit (the reader accepts
        // either case, so a case change alone would not alter the bytes).
        let at = s.len() - 8;
        let flipped = if s.as_bytes()[at] == b'0' { '1' } else { '0' };
        s.replace_range(at..=at, &flipped.to_string());
        let mut p = Fields::new();
        p.insert("totp_secret".into(), J::from(s));
        b.update("auth_user", "user0", &p).unwrap();
        let e = run(&b, &ring, Options::default(), &mut |_| {}).unwrap_err();
        assert!(e.contains("auth_user/user0.totp_secret"));
    }

    #[test]
    fn it_refuses_without_a_current_key() {
        let b = obop_storage::open("memory:").unwrap();
        let e = run(&b, &KeyRing::default(), Options::default(), &mut |_| {}).unwrap_err();
        assert!(e.contains("OBOP_SECRET_KEY"));
    }
}
