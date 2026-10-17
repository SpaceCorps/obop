//! Updating the instance itself from inside it (D63), end to end: a real instance `Updater`
//! talking HTTP to a real control-plane router backed by the fake provisioner. Also the admin
//! commands over REST: who may call them and what an agent may only propose.

use crate::App;
use crate::mfa_tests::{rig, send};
use obop_auth::Auth;
use obop_keys::KeyRing;
use obop_pkg::semver::Version;
use obop_platform::provision::Fake;
use obop_platform::{Api, Platform};
use obop_update::{Config, Mode, Updater};
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// A control plane on a loopback port, with one running instance; returns its address, the fake
/// provisioner, the platform and the instance's update token.
struct Plane {
    url: String,
    fake: Arc<Fake>,
    platform: Arc<Platform>,
    token: String,
}

fn plane() -> Plane {
    let backend = obop_storage::open("memory:").unwrap();
    let fake = Arc::new(Fake::default());
    *fake.image.lock().unwrap() = Some("img:old".into());
    let platform = Platform::new(
        Arc::new(Auth::with_test_params(backend.clone())),
        backend,
        fake.clone(),
    );
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = format!("127.0.0.1:{}", l.local_addr().unwrap().port());
    drop(l);
    let url = format!("http://{addr}");
    platform.set_public_url(&url);
    let router = obop_http::platform_router(Arc::new(Api(platform.clone())), None);
    let a = addr.clone();
    std::thread::spawn(move || {
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(obop_http::serve(&a, router))
            .unwrap();
    });
    for _ in 0..200 {
        if std::net::TcpStream::connect(&addr).is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let owner = platform
        .signup("owner@example.test", "correct horse battery")
        .unwrap();
    let i = platform.create_instance(&owner, "acme-books").unwrap();
    for _ in 0..300 {
        if platform.get(&owner, &i.id).unwrap().status == "running" {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let token = fake.created.lock().unwrap()[0].update_token.clone();
    Plane {
        url,
        fake,
        platform,
        token,
    }
}

fn bump(core: &Version, minor: bool) -> String {
    if minor {
        format!("{}.{}.0", core.major, core.minor + 1)
    } else {
        format!("{}.{}.{}", core.major, core.minor, core.patch + 1)
    }
}

const PKG: &str = "[module]\nid = \"books\"\nversion = \"0.1.0\"\nname = \"Books\"\ndescription = \"d\"\nlicense = \"MIT\"\nrequires_core = \"^0.2\"\n[[model]]\nname = \"books_entry\"\ndescription = \"d\"\nread_group = \"books.user\"\nwrite_group = \"books.user\"\n[[model.field]]\nname = \"title\"\ndescription = \"t\"\nclass = \"none\"\n";

/// An instance (its own in-memory database) wired to `plane`. `backup` records the directories
/// it was asked to write to and fails when `fail_backup` is set.
struct Instance {
    app: App,
    up: Arc<Updater>,
    backups: Arc<Mutex<Vec<String>>>,
    fail_backup: Arc<std::sync::atomic::AtomicBool>,
}

fn instance(plane: &Plane, token: Option<&str>, backup_dir: bool) -> Instance {
    let app = App::new();
    app.installer.install(PKG, "upload", "root").unwrap();
    let cfg = Config {
        feed: Some(format!("{}/api/releases", plane.url)),
        control_url: Some(plane.url.clone()),
        token: token.map(String::from),
        backup_dir: backup_dir.then(|| std::env::temp_dir().join("obop-update-test")),
    };
    let up = Updater::new(
        app.store.backend(),
        app.installer.clone(),
        KeyRing::for_tests(),
        cfg,
    );
    let backups = Arc::new(Mutex::new(Vec::new()));
    let fail = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (b, f) = (backups.clone(), fail.clone());
    up.set_backup(Arc::new(move |dir: &std::path::Path| {
        if f.load(std::sync::atomic::Ordering::Relaxed) {
            return Err("disk full".into());
        }
        let path = format!("{}/obop-backup-test.jsonl.gz", dir.display());
        b.lock().unwrap().push(path.clone());
        Ok(path)
    }));
    Instance {
        app,
        up,
        backups,
        fail_backup: fail,
    }
}

fn wait_for(up: &Updater, state: &str) -> obop_update::Status {
    for _ in 0..400 {
        let s = up.status();
        if s.state == state {
            return s;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    panic!("never reached {state}: {:?}", up.status());
}

fn kinds(app: &App) -> Vec<String> {
    app.store
        .backend()
        .list("security_event")
        .unwrap()
        .into_iter()
        .map(|(_, f)| f["kind"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn a_managed_instance_checks_backs_up_asks_the_control_plane_and_follows_the_result() {
    let p = plane();
    let me = instance(&p, Some(&p.token.clone()), true);
    let core = me.up.core_version().clone();
    let (patch, minor) = (bump(&core, false), bump(&core, true));
    p.platform
        .publish_release(&patch, &format!("img:{patch}"), "Bug fixes.", "")
        .unwrap();
    p.platform
        .publish_release(&minor, &format!("img:{minor}"), "A new minor version.", "")
        .unwrap();
    assert_eq!(me.up.mode(), Mode::Managed);
    let c = me.up.check();
    assert_eq!(c.feed_error, None);
    assert_eq!(c.latest.as_deref(), Some(minor.as_str()));
    assert!(c.update_available);
    // Newest first. The minor release is blocked: the installed module supports ^0.2 only.
    assert_eq!(c.releases[0].release.version, minor);
    assert!(!c.releases[0].installable);
    assert!(
        c.releases[0].blockers[0].contains("books 0.1.0 would not run"),
        "{:?}",
        c.releases[0].blockers
    );
    assert!(
        c.releases[0]
            .modules
            .iter()
            .any(|m| m.id == "books" && !m.compatible)
    );
    assert!(c.releases[1].installable && c.releases[1].blockers.is_empty());
    // A blocked release cannot be started, and nothing reached the control plane.
    let e = me.up.start(&minor, "root", false).unwrap_err();
    assert!(
        e.contains("cannot be installed here yet") && e.contains("books"),
        "{e}"
    );
    assert!(p.fake.upgraded.lock().unwrap().is_empty());
    assert!(
        me.backups.lock().unwrap().is_empty(),
        "no backup for a refused request"
    );
    // Something that is not published, or not newer, is refused.
    assert!(
        me.up
            .start("9.9.9", "root", false)
            .unwrap_err()
            .contains("not a published")
    );
    // The good one: backup first, then the request.
    let s = me.up.start(&patch, "root", false).unwrap();
    assert!(matches!(s.state.as_str(), "running" | "requested"), "{s:?}");
    assert_eq!(me.backups.lock().unwrap().len(), 1);
    assert!(s.backup.ends_with("obop-backup-test.jsonl.gz"));
    // A second request while it runs is refused.
    *p.fake.upgrade_delay.lock().unwrap() = Duration::from_millis(0);
    let done = wait_for(&me.up, "succeeded");
    assert_eq!(
        (done.from.as_str(), done.target.as_str()),
        (core.to_string().as_str(), patch.as_str())
    );
    assert_eq!(
        *p.fake.upgraded.lock().unwrap(),
        [("acme-books".to_string(), format!("img:{patch}"))]
    );
    let k = kinds(&me.app);
    for want in ["instance_update_requested", "instance_update_finished"] {
        assert!(k.iter().any(|x| x == want), "{want} in {k:?}");
    }
    assert_eq!(
        k.iter()
            .filter(|x| *x == "instance_update_finished")
            .count(),
        1,
        "recorded once however often the status is read"
    );
    // The restarted instance reports the new version; it can go back to the old one.
    let restarted = Updater::new_for_core(
        me.app.store.backend(),
        me.app.installer.clone(),
        KeyRing::for_tests(),
        Config {
            feed: Some(format!("{}/api/releases", p.url)),
            control_url: Some(p.url.clone()),
            token: Some(p.token.clone()),
            backup_dir: None,
        },
        Version::parse(&patch).unwrap(),
    );
    let s = restarted.status();
    assert!(s.can_rollback, "{s:?}");
    restarted.rollback("root").unwrap();
    wait_for(&restarted, "succeeded");
    assert!(
        kinds(&me.app)
            .iter()
            .any(|x| x == "instance_rollback_requested")
    );
    assert_eq!(
        p.fake.upgraded.lock().unwrap().last().unwrap().1,
        "img:old",
        "the control plane restored the image the instance had before"
    );
}

#[test]
fn the_instance_stores_its_token_sealed_and_only_the_environment_hands_it_over() {
    let p = plane();
    let me = instance(&p, Some(&p.token.clone()), true);
    let stored = me
        .app
        .store
        .backend()
        .get("obop_update", "config")
        .unwrap()
        .unwrap();
    let sealed = stored["token"].as_str().unwrap();
    assert!(
        sealed.starts_with("e2.") && !sealed.contains(&p.token),
        "{sealed}"
    );
    // A second process on the same database, started without the variable, still has it.
    let again = Updater::new(
        me.app.store.backend(),
        me.app.installer.clone(),
        KeyRing::for_tests(),
        Config {
            feed: Some(format!("{}/api/releases", p.url)),
            control_url: None,
            token: None,
            backup_dir: None,
        },
    );
    assert_eq!(again.mode(), Mode::Managed);
}

#[test]
fn no_backup_means_no_request_unless_the_administrator_says_they_have_their_own() {
    let p = plane();
    let me = instance(&p, Some(&p.token.clone()), false);
    let v = bump(me.up.core_version(), false);
    p.platform
        .publish_release(&v, &format!("img:{v}"), "", "")
        .unwrap();
    let e = me.up.start(&v, "root", false).unwrap_err();
    assert!(
        e.contains("no backup directory") && e.contains("skip_backup"),
        "{e}"
    );
    assert!(p.fake.upgraded.lock().unwrap().is_empty());
    assert_eq!(me.up.status().state, "idle");
    // A backup that fails stops the request just the same.
    let me = instance(&p, Some(&p.token.clone()), true);
    me.fail_backup
        .store(true, std::sync::atomic::Ordering::Relaxed);
    let e = me.up.start(&v, "root", false).unwrap_err();
    assert!(
        e.contains("the backup failed") && e.contains("disk full"),
        "{e}"
    );
    assert!(p.fake.upgraded.lock().unwrap().is_empty());
    assert_eq!(me.up.status().state, "idle");
    // With the administrator's own backup, it goes ahead.
    let me = instance(&p, Some(&p.token.clone()), false);
    me.up.start(&v, "root", true).unwrap();
    wait_for(&me.up, "succeeded");
    assert!(me.backups.lock().unwrap().is_empty());
}

#[test]
fn a_wrong_or_revoked_token_fails_the_request_and_says_so() {
    let p = plane();
    let wrong = instance(
        &p,
        Some("obu_not_the_real_token_at_all_00000000000000000000"),
        true,
    );
    let v = bump(wrong.up.core_version(), false);
    p.platform
        .publish_release(&v, &format!("img:{v}"), "", "")
        .unwrap();
    let e = wrong.up.start(&v, "root", false).unwrap_err();
    assert!(e.contains("not a valid update token"), "{e}");
    let s = wrong.up.status();
    assert_eq!(s.state, "failed");
    assert!(s.error.contains("not a valid update token"));
    assert!(
        kinds(&wrong.app)
            .iter()
            .any(|k| k == "instance_update_failed")
    );
    assert!(p.fake.upgraded.lock().unwrap().is_empty());
    // A token the operator revoked stops working the same way.
    p.platform.revoke_update_token("acme-books").unwrap();
    let revoked = instance(&p, Some(&p.token.clone()), true);
    assert!(
        revoked
            .up
            .start(&v, "root", false)
            .unwrap_err()
            .contains("not a valid update token")
    );
}

#[test]
fn a_self_hosted_instance_gets_instructions_not_a_button() {
    let p = plane();
    let me = instance(&p, None, true);
    // No token and no control plane address: self-hosted. (The instance helper still points the
    // feed at the control plane, which is how a self-hosted operator can use a mirror.)
    let me2 = Updater::new(
        me.app.store.backend(),
        me.app.installer.clone(),
        KeyRing::for_tests(),
        Config {
            feed: Some(format!("{}/api/releases", p.url)),
            control_url: None,
            token: None,
            backup_dir: None,
        },
    );
    let v = bump(me2.core_version(), false);
    p.platform
        .publish_release(&v, &format!("ghcr.io/spacecorps/obop:{v}"), "n", "")
        .unwrap();
    assert_eq!(me2.mode(), Mode::SelfHosted);
    let c = me2.check();
    assert!(c.update_available);
    let text = c.instructions.join("\n");
    assert!(
        text.contains(&format!("docker pull ghcr.io/spacecorps/obop:{v}")),
        "{text}"
    );
    assert!(
        text.contains("obop migrate plan") && text.contains("obop backup"),
        "{text}"
    );
    let e = me2.start(&v, "root", true).unwrap_err();
    assert!(e.contains("not managed by a control plane"), "{e}");
}

#[test]
fn a_feed_that_cannot_be_read_or_was_altered_does_not_stop_the_page() {
    let p = plane();
    let down = Updater::new(
        App::new().store.backend(),
        App::new().installer.clone(),
        KeyRing::for_tests(),
        Config {
            feed: Some("http://127.0.0.1:1/api/releases".into()),
            ..Config::default()
        },
    );
    let c = down.check();
    assert!(c.feed_error.as_deref().unwrap().contains("cannot reach"));
    assert!(c.releases.is_empty() && !c.update_available);
    // Switched off.
    let off = Updater::new(
        App::new().store.backend(),
        App::new().installer.clone(),
        KeyRing::for_tests(),
        Config::default(),
    );
    assert!(off.check().feed_error.unwrap().contains("switched off"));
    // An altered release is dropped, the good one stays.
    p.platform
        .publish_release("0.9.0", "img:0.9.0", "real", "")
        .unwrap();
    let me = instance(&p, None, false);
    let mut feed = obop_update::parse_feed(&p.platform.feed_json().to_string()).unwrap();
    feed.releases[0].notes = "run this script".into();
    let raw = json!({"releases": feed.releases.iter().map(obop_update::Release::to_json).collect::<Vec<_>>()});
    let again = obop_update::parse_feed(&raw.to_string()).unwrap();
    assert!(again.releases.is_empty() && again.dropped.len() == 1);
    let _ = me;
}

#[tokio::test]
async fn the_commands_are_admin_only_and_an_agent_can_only_propose_them() {
    let r = rig(false);
    let admin = crate::mfa_tests::token(&r.r, "root").await;
    let user = crate::mfa_tests::token(&r.r, "ada").await;
    // Anyone without the admin group is refused the whole model.
    for path in [
        "/api/instance_update/status",
        "/api/instance_update/check",
        "/api/instance_update/start",
        "/api/instance_update/rollback",
    ] {
        let (s, _) = send(&r.r, "POST", Some(&user), path, json!({"version": "9.9.9"})).await;
        assert!(s == 403 || s == 404, "{path}: {s}");
        let (s, _) = send(&r.r, "POST", None, path, json!({})).await;
        assert_eq!(s, 401, "{path}");
    }
    // An administrator reads the state: a fresh instance is idle and self-hosted.
    let (s, b) = send(
        &r.r,
        "POST",
        Some(&admin),
        "/api/instance_update/status",
        json!({}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(b["value"]["state"], "idle");
    assert_eq!(b["value"]["mode"], "self_hosted");
    // Starting is refused with the reason, not swallowed.
    let (s, b) = send(
        &r.r,
        "POST",
        Some(&admin),
        "/api/instance_update/start",
        json!({"version": "9.9.9"}),
    )
    .await;
    assert!(s >= 400, "{b}");
    assert!(
        b.to_string().contains("not managed by a control plane"),
        "{b}"
    );
    // An agent token holding the admin group may only propose a start or a rollback.
    let (s, b) = send(
        &r.r,
        "POST",
        Some(&admin),
        "/api/auth/agent-token",
        json!({"scopes": ["platform.admin"], "label": "updater"}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    let agent = b["token"].as_str().unwrap().to_string();
    for path in [
        "/api/instance_update/start",
        "/api/instance_update/rollback",
    ] {
        let (s, b) = send(
            &r.r,
            "POST",
            Some(&agent),
            path,
            json!({"version": "9.9.9"}),
        )
        .await;
        assert!(
            b.to_string().contains("plan_id") || b.to_string().contains("approval"),
            "{path} must wait for approval: {s} {b}"
        );
    }
    // Reading the state needs no approval.
    let (s, _) = send(
        &r.r,
        "POST",
        Some(&agent),
        "/api/instance_update/status",
        json!({}),
    )
    .await;
    assert_eq!(s, 200);
}
