//! The update token and the upgrade it can ask for (D63): who it authenticates, what it cannot
//! do, rate limits, replays, rollback and the audit trail.

use crate::provision::Fake;
use crate::update::{START_PER_HOUR, TOKENS, hash_token};
use crate::{Platform, TABLE, now};
use obop_auth::Auth;
use obop_update::Release;
use serde_json::Value as J;
use std::sync::Arc;

fn platform() -> (Arc<Platform>, Arc<Fake>) {
    let backend = obop_storage::open("memory:").unwrap();
    let fake = Arc::new(Fake::default());
    let p = Platform::new(
        Arc::new(Auth::with_test_params(backend.clone())),
        backend,
        fake.clone(),
    );
    p.set_public_url("https://control.example.test/");
    (p, fake)
}

/// A running instance called `slug` on image `img:old`: its owner's session and its update token.
fn running(p: &Arc<Platform>, fake: &Fake, slug: &str) -> (String, String) {
    *fake.image.lock().unwrap() = Some("img:old".into());
    let tok = p
        .signup(&format!("{slug}@example.test"), "correct horse battery")
        .unwrap();
    let i = p.create_instance(&tok, slug).unwrap();
    for _ in 0..300 {
        if p.get(&tok, &i.id).unwrap().status == "running" {
            let token = fake
                .created
                .lock()
                .unwrap()
                .iter()
                .find(|s| s.slug == slug)
                .unwrap()
                .update_token
                .clone();
            return (tok, token);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("never running");
}

fn settle(p: &Platform, token: &str) -> J {
    for _ in 0..300 {
        let s = p.update_status(token).unwrap();
        if s["state"] != "running" {
            return s;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("the update never finished");
}

#[test]
fn creating_an_instance_mints_its_token_once_and_keeps_only_a_hash() {
    let (p, fake) = platform();
    let (_, token) = running(&p, &fake, "alpha-one");
    assert!(token.starts_with("obu_") && token.len() == 4 + 64);
    let spec = fake.created.lock().unwrap()[0].clone();
    assert_eq!(spec.control_url, "https://control.example.test");
    let hash = hash_token(&token);
    assert!(p.backend.get(TOKENS, &hash).unwrap().is_some());
    for model in [TABLE, TOKENS] {
        for (_, f) in p.backend.list(model).unwrap() {
            assert!(
                !serde_json::to_string(&f).unwrap().contains(&token),
                "{model} holds the token"
            );
        }
    }
    assert_eq!(p.authenticate_update(&token).unwrap().slug, "alpha-one");
}

#[test]
fn wrong_expired_revoked_replaced_and_foreign_tokens_are_all_refused_alike() {
    let (p, fake) = platform();
    let (session, token) = running(&p, &fake, "alpha-one");
    let (_, other) = running(&p, &fake, "alpha-two");
    let refused = |t: &str| p.authenticate_update(t).unwrap_err();
    let same = refused("obu_wrong");
    assert_eq!(same, "not a valid update token");
    assert_eq!(refused(""), same);
    assert_eq!(refused(&token[..20]), same);
    assert_eq!(refused(&format!("{token}0")), same);
    assert_eq!(refused(&token.replace("obu_", "obp_")), same);
    // A user's session is not an update token, and an update token is not a session.
    assert_eq!(refused(&session), same);
    assert!(
        p.list(&token).is_err(),
        "an update token opens no account API"
    );
    assert!(p.create_instance(&token, "stolen-one").is_err());
    // Expired.
    let mut row = p.backend.get(TOKENS, &hash_token(&token)).unwrap().unwrap();
    row.insert("expires_at".into(), J::from(now() - 1));
    p.backend.update(TOKENS, &hash_token(&token), &row).unwrap();
    assert_eq!(refused(&token), same);
    // Replaced: a new token ends the old one at once; the other instance is untouched.
    let fresh = p.mint_update_token("alpha-one").unwrap();
    assert_eq!(refused(&token), same);
    assert!(p.authenticate_update(&fresh).is_ok());
    assert!(p.authenticate_update(&other).is_ok());
    // Revoked.
    p.revoke_update_token("alpha-one").unwrap();
    assert_eq!(refused(&fresh), same);
    assert_eq!(p.authenticate_update(&other).unwrap().slug, "alpha-two");
}

#[test]
fn a_token_can_only_ask_for_a_published_version_never_an_image() {
    let (p, fake) = platform();
    let (_, token) = running(&p, &fake, "alpha-one");
    for v in ["0.2.1", "img:evil", "ghcr.io/evil/obop:1", ""] {
        let e = p.update_start(&token, v, "0.2.0").unwrap_err();
        assert!(e.contains("not a published release"), "{v}: {e}");
    }
    assert!(fake.upgraded.lock().unwrap().is_empty());
    p.publish_release("0.2.1", "img:0.2.1", "fixes", "")
        .unwrap();
    p.publish_release("0.3.0", "img:0.3.0", "new", "0.2.0")
        .unwrap();
    p.withdraw_release("0.3.0").unwrap();
    assert!(
        p.update_start(&token, "0.3.0", "0.2.0").is_err(),
        "withdrawn"
    );
    assert!(p.publish_release("0.4.0-rc.1", "img", "", "").is_err());
    assert!(p.publish_release("0.4.0", "bad image", "", "").is_err());
    let r = p.update_start(&token, "0.2.1", "0.2.0").unwrap();
    assert_eq!(r["state"], "running");
    let s = settle(&p, &token);
    assert_eq!(s["state"], "succeeded");
    assert_eq!(s["image"], "img:0.2.1");
    assert_eq!(
        *fake.upgraded.lock().unwrap(),
        [("alpha-one".to_string(), "img:0.2.1".to_string())]
    );
}

#[test]
fn a_replayed_or_concurrent_request_does_not_upgrade_twice() {
    let (p, fake) = platform();
    let (_, token) = running(&p, &fake, "alpha-one");
    p.publish_release("0.2.1", "img:0.2.1", "", "").unwrap();
    *fake.upgrade_delay.lock().unwrap() = std::time::Duration::from_millis(300);
    p.update_start(&token, "0.2.1", "0.2.0").unwrap();
    let e = p.update_start(&token, "0.2.1", "0.2.0").unwrap_err();
    assert!(e.contains("already running"), "{e}");
    assert_eq!(settle(&p, &token)["state"], "succeeded");
    let e = p.update_start(&token, "0.2.1", "0.2.0").unwrap_err();
    assert!(e.contains("already runs this release"), "{e}");
    assert_eq!(fake.upgraded.lock().unwrap().len(), 1);
}

#[test]
fn requests_are_rate_limited_per_instance() {
    let (p, fake) = platform();
    let (_, a) = running(&p, &fake, "alpha-one");
    let (_, b) = running(&p, &fake, "alpha-two");
    p.publish_release("0.2.1", "img:0.2.1", "", "").unwrap();
    let mut refused = 0;
    for _ in 0..(START_PER_HOUR + 3) {
        if p.update_start(&a, "9.9.9", "0.2.0")
            .unwrap_err()
            .contains("too many requests")
        {
            refused += 1;
        }
    }
    assert_eq!(refused, 3);
    assert!(
        p.update_start(&b, "0.2.1", "0.2.0").is_ok(),
        "its own allowance"
    );
}

#[test]
fn a_version_that_does_not_come_up_is_rolled_back_before_the_failure_is_reported() {
    let (p, fake) = platform();
    let (_, token) = running(&p, &fake, "alpha-one");
    p.publish_release("0.2.1", "img:0.2.1", "", "").unwrap();
    *fake.upgrade_fail_image.lock().unwrap() = Some("img:0.2.1".into());
    p.update_start(&token, "0.2.1", "0.2.0").unwrap();
    let s = settle(&p, &token);
    assert_eq!(s["state"], "rolled_back");
    assert!(
        s["error"]
            .as_str()
            .unwrap()
            .contains("back on the previous version")
    );
    assert_eq!(
        s["image"], "img:old",
        "the record still names the working image"
    );
    assert_eq!(
        *fake.upgraded.lock().unwrap(),
        [("alpha-one".to_string(), "img:old".to_string())],
        "the old image was put back"
    );
    // When even the rollback fails, the state says an operator must look.
    *fake.upgrade_fail_image.lock().unwrap() = None;
    *fake.upgrade_fail_for.lock().unwrap() = Some("alpha-one".into());
    p.update_start(&token, "0.2.1", "0.2.0").unwrap();
    let s = settle(&p, &token);
    assert_eq!(s["state"], "failed");
    assert!(s["error"].as_str().unwrap().contains("operator"));
}

#[test]
fn rollback_goes_to_the_previous_image_and_every_step_is_audited() {
    let (p, fake) = platform();
    let (_, token) = running(&p, &fake, "alpha-one");
    assert!(
        p.update_rollback(&token)
            .unwrap_err()
            .contains("no earlier image")
    );
    p.publish_release("0.2.1", "img:0.2.1", "", "").unwrap();
    p.update_start(&token, "0.2.1", "0.2.0").unwrap();
    assert_eq!(settle(&p, &token)["state"], "succeeded");
    assert_eq!(p.update_status(&token).unwrap()["can_rollback"], true);
    p.update_rollback(&token).unwrap();
    let s = settle(&p, &token);
    assert_eq!(
        (s["state"].as_str(), s["image"].as_str()),
        (Some("succeeded"), Some("img:old"))
    );
    assert_eq!(
        s["target"], "0.2.0",
        "the rollback targets the version it came from"
    );
    assert_eq!(s["can_rollback"], false, "no loop");
    let kinds: Vec<String> = p
        .backend
        .list("security_event")
        .unwrap()
        .into_iter()
        .map(|(_, f)| f["kind"].as_str().unwrap().to_string())
        .collect();
    for want in [
        "release_published",
        "instance_update_started",
        "instance_update_succeeded",
        "instance_rollback_started",
    ] {
        assert!(kinds.iter().any(|k| k == want), "{want} in {kinds:?}");
    }
}

#[test]
fn the_feed_is_sorted_and_carries_a_digest_per_release() {
    let (p, _) = platform();
    p.publish_release("0.2.1", "img:0.2.1", "a", "").unwrap();
    p.publish_release("0.10.0", "img:0.10.0", "b", "0.2.0")
        .unwrap();
    let feed = obop_update::parse_feed(&p.feed_json().to_string()).unwrap();
    let v: Vec<_> = feed.releases.iter().map(|r| r.version.as_str()).collect();
    assert_eq!(v, ["0.10.0", "0.2.1"]);
    assert!(feed.dropped.is_empty() && feed.releases.iter().all(Release::intact));
}
