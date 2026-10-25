use super::*;
use obop_connectors::secretbox::PSEUDONYM_KEY_MODEL;
use obop_keys::legacy;
use obop_storage::Fields;
use serde_json::json;

const OLD: &str = "0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a";
const NEW: &str = "0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b";

fn fields(v: J) -> Fields {
    v.as_object().unwrap().clone()
}

fn old_ring() -> KeyRing {
    KeyRing::from_secrets(OLD, &[]).unwrap()
}

fn rotated_ring() -> KeyRing {
    KeyRing::from_secrets(NEW, &[OLD]).unwrap()
}

/// A database as an instance on the old key left it.
fn instance() -> Arc<dyn Backend> {
    let b = obop_storage::open("memory:").unwrap();
    let old = old_ring();
    let sb = SecretBox::from_ring_with_store(old.clone(), b.clone()).unwrap();
    b.insert(
        "auth_user",
        "ada",
        &fields(json!({
            "password_hash": "$argon2id$x",
            "totp_secret": old.seal("totp:ada", b"seed").unwrap(),
        })),
    )
    .unwrap();
    b.insert(
        "connection",
        "c1",
        &fields(json!({
            "credentials": old.seal("connection-credentials", b"{\"api_key\":\"k\"}").unwrap(),
            "oauth_tokens": "",
        })),
    )
    .unwrap();
    b.insert("connection", "c2", &fields(json!({"credentials": ""})))
        .unwrap();
    // The pseudonym key as the connectors would have derived it, from the old key.
    let _ = sb.pseudonym_key("pseudonym:c1");
    b.kv_put(
        "plan:p1",
        &old.seal("plan:p1", b"{\"x\":1}").unwrap(),
        Some(60_000),
    )
    .unwrap();
    b
}

#[test]
fn rotation_reseals_everything_and_pseudonyms_do_not_change() {
    let b = instance();
    let before = SecretBox::from_ring_with_store(old_ring(), b.clone()).unwrap();
    let psn_before = obop_connectors::egress::pseudonym(&before, "c1", &json!("ada@example.test"));

    let mut lines = Vec::new();
    let r = rotate(&b, &rotated_ring(), Options::default(), &mut |l| {
        lines.push(l.to_string())
    })
    .unwrap();
    assert!(r.resealed >= 4 && r.complete(), "{r:?}");
    // The old key is no longer needed to read any of it.
    let only_new = KeyRing::from_secrets(NEW, &[]).unwrap();
    let rec = b.get("auth_user", "ada").unwrap().unwrap();
    assert_eq!(
        only_new
            .open("totp:ada", rec["totp_secret"].as_str().unwrap())
            .unwrap(),
        b"seed"
    );
    let rec = b.get("connection", "c1").unwrap().unwrap();
    assert_eq!(
        only_new
            .open(
                "connection-credentials",
                rec["credentials"].as_str().unwrap()
            )
            .unwrap(),
        b"{\"api_key\":\"k\"}"
    );
    // Pseudonyms already given to receivers are the same under the new ring.
    let after = SecretBox::from_ring_with_store(only_new.clone(), b.clone()).unwrap();
    assert_eq!(
        obop_connectors::egress::pseudonym(&after, "c1", &json!("ada@example.test")),
        psn_before
    );
    // A connection that never pseudonymised anything got its key stored too, anchored on the
    // previous key, so its first pseudonyms after the rotation match the old ones.
    assert!(
        b.get(PSEUDONYM_KEY_MODEL, "pseudonym:c2")
            .unwrap()
            .is_some()
    );
    // Reports and progress lines carry no key and no secret.
    let everything = format!("{r:?} {lines:?}");
    for secret in [OLD, NEW, "seed", "api_key"] {
        assert!(!everything.contains(secret), "{secret} leaked");
    }
    // Run again: nothing to do.
    let again = rotate(&b, &rotated_ring(), Options::default(), &mut |_| {}).unwrap();
    assert_eq!((again.to_reseal, again.resealed), (0, 0));
}

#[test]
fn rotation_refuses_without_the_old_key_and_touches_nothing() {
    let b = instance();
    let snapshot = b.get("auth_user", "ada").unwrap().unwrap();
    // The operator forgot OBOP_SECRET_KEY_PREVIOUS.
    let forgot = KeyRing::from_secrets(NEW, &[]).unwrap();
    let e = rotate(&b, &forgot, Options::default(), &mut |_| {}).unwrap_err();
    assert!(
        e.contains("OBOP_SECRET_KEY_PREVIOUS") && e.contains("nothing was changed"),
        "{e}"
    );
    assert!(!e.contains(OLD) && !e.contains(NEW));
    assert_eq!(b.get("auth_user", "ada").unwrap().unwrap(), snapshot);
    // No key at all.
    assert!(rotate(&b, &KeyRing::default(), Options::default(), &mut |_| {}).is_err());
}

#[test]
fn a_dry_run_writes_nothing_not_even_pseudonym_keys() {
    let b = instance();
    let n_before = b.total().unwrap();
    let r = rotate(
        &b,
        &rotated_ring(),
        Options {
            dry_run: true,
            ..Options::default()
        },
        &mut |_| {},
    )
    .unwrap();
    assert!(r.to_reseal >= 4 && r.resealed == 0);
    assert_eq!(b.total().unwrap(), n_before);
}

#[test]
fn rotating_while_the_server_keeps_reading_and_writing_loses_nothing() {
    let b = instance();
    let ring = rotated_ring();
    let stop = std::sync::atomic::AtomicBool::new(false);
    let started = std::sync::atomic::AtomicBool::new(false);
    std::thread::scope(|s| {
        // A "server" that uses the new ring: opens the credentials all the time and keeps
        // storing new secrets with the current key.
        let server = s.spawn(|| {
            let sb = SecretBox::from_ring_with_store(ring.clone(), b.clone()).unwrap();
            let mut n = 0;
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                let rec = b.get("connection", "c1").unwrap().unwrap();
                let plain = sb.open(
                    "connection-credentials",
                    rec["credentials"].as_str().unwrap(),
                );
                assert_eq!(
                    plain.as_deref(),
                    Ok("{\"api_key\":\"k\"}"),
                    "unreadable during rotation"
                );
                let id = format!("w{n}");
                b.insert(
                    "connection",
                    &id,
                    &fields(json!({
                        "credentials": sb.seal("connection-credentials", "{}"),
                    })),
                )
                .unwrap();
                n += 1;
                started.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            n
        });
        // Rotate in small batches meanwhile, once the "server" is really running (without this
        // the three quick rotations could finish before the thread was scheduled).
        while !started.load(std::sync::atomic::Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        for _ in 0..3 {
            let r = rotate(
                &b,
                &ring,
                Options {
                    batch: 1,
                    ..Options::default()
                },
                &mut |_| {},
            )
            .unwrap();
            assert!(r.unopenable.is_empty());
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(server.join().unwrap() > 0);
    });
    // Everything the server wrote and everything rotated opens under the new key alone.
    let only_new = KeyRing::from_secrets(NEW, &[]).unwrap();
    for (id, rec) in b.list("connection").unwrap() {
        if let Some(c) = rec["credentials"].as_str().filter(|c| !c.is_empty()) {
            only_new
                .open("connection-credentials", c)
                .unwrap_or_else(|e| panic!("{id}: {e}"));
        }
    }
}

#[test]
fn first_format_values_are_upgraded_and_the_status_report_counts_by_key() {
    let b = obop_storage::open("memory:").unwrap();
    let master = legacy::parse_key(OLD).unwrap();
    b.insert(
        "auth_user",
        "ada",
        &fields(json!({
            "totp_secret": legacy::seal_totp_v1(&master, "ada", b"seed"),
        })),
    )
    .unwrap();
    let ring = rotated_ring();
    let s = status(&b, &ring).unwrap();
    assert!(s.contains(&format!("current key: {}", id_of(NEW))), "{s}");
    assert!(s.contains(&format!("previous key: {}", id_of(OLD))));
    assert!(
        s.contains("sealed with older format: 1") && s.contains("obop secrets rotate"),
        "{s}"
    );
    assert!(!s.contains(NEW) && !s.contains(OLD));
    rotate(&b, &ring, Options::default(), &mut |_| {}).unwrap();
    let s = status(&b, &ring).unwrap();
    assert!(s.contains("nothing to re-seal"), "{s}");
}

#[test]
fn the_inventory_names_every_place_the_server_seals_something() {
    // The places the sealing code uses, against the inventory the rotation walks. A new sealed
    // field that is not listed here would survive a rotation under the old key: this test is the
    // reminder to add it.
    use obop_keys::reseal::{FIELDS, KV_PREFIXES};
    let listed: Vec<(&str, &str)> = FIELDS.iter().map(|f| (f.model, f.field)).collect();
    for want in [
        ("auth_user", "totp_secret"),
        ("auth_user", "totp_pending"),
        (obop_connectors::model::CONNECTION, "credentials"),
        (obop_connectors::model::CONNECTION, "oauth_tokens"),
        (
            obop_connectors::model::PROVIDER_POLICY,
            "oauth_client_secret",
        ),
        (obop_connectors::model::VAULT, "value"),
        (obop_connectors::secretbox::PSEUDONYM_KEY_MODEL, "key"),
        ("connector_oauth_state", "verifier"),
        ("connector_job", "body"),
        ("connector_dead", "body"),
    ] {
        assert!(
            listed.contains(&want),
            "{want:?} missing from the rotation inventory"
        );
    }
    assert_eq!(KV_PREFIXES, ["plan:", "vault:", "mcp:s:"]);
    // The purposes the inventory uses are the ones the code seals with.
    assert_eq!(
        obop_connectors::runtime::PURPOSE_CREDENTIALS,
        "connection-credentials"
    );
    assert_eq!(
        obop_connectors::runtime::PURPOSE_OAUTH,
        "connection-oauth-tokens"
    );
    assert_eq!(
        obop_connectors::runtime::PURPOSE_CLIENT,
        "provider-oauth-client"
    );
}
