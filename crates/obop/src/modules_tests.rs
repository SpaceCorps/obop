//! D55 end to end: the metadata of every bundled package, dependency resolution over the real
//! catalogue, the command line, and the same plan over HTTP with an agent proposing it.

use crate::app::App;
use crate::{modules_check_files_with, modules_plan, modules_tree};
use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use obop_auth::Auth;
use obop_commands::{ADMIN_GROUP, Principal, Surface, Value, Vault};
use obop_pkg::installer::Request as PlanRequest;
use obop_pkg::semver::{Req, Version};
use obop_pkg::{Bundled, Manifest, Marketplace};
use obop_registry::{MATURITIES, MODULE_CATEGORIES};
use serde_json::{Value as J, json};
use std::collections::BTreeSet;
use std::sync::Arc;
use tower::ServiceExt;

fn bundled() -> Vec<Manifest> {
    Bundled
        .list()
        .unwrap()
        .iter()
        .map(|l| Manifest::parse(&Bundled.fetch(&l.id).unwrap()).unwrap())
        .collect()
}

fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn the_core_version_is_the_workspace_version() {
    assert_eq!(obop_types::CORE_VERSION, env!("CARGO_PKG_VERSION"));
    assert_eq!(obop_types::CORE_VERSION, "0.2.0");
    let v = Version::parse(obop_types::CORE_VERSION).unwrap();
    // The compatibility policy: while below 1.0 a minor bump is breaking, so ^0.2 is 0.2.x.
    assert!(Req::parse("^0.2").unwrap().matches(&v));
    assert!(!Req::parse("^0.2").unwrap().matches(&Version::new(0, 3, 0)));
}

#[test]
fn every_bundled_package_declares_its_metadata() {
    let all = bundled();
    assert_eq!(all.len(), 22);
    let builtin: BTreeSet<String> = App::new().modules.iter().map(|m| m.id.clone()).collect();
    let ids: BTreeSet<String> = all.iter().map(|m| m.module.id.clone()).collect();
    for m in &all {
        let x = &m.module;
        let id = &x.id;
        assert_eq!(
            x.license, "AGPL-3.0-or-later",
            "{id}: D1 first-party licence"
        );
        assert!(obop_pkg::spdx::check(&x.license).is_ok(), "{id}");
        assert_eq!(x.requires_core, "^0.2", "{id}");
        assert_eq!(x.author, "SpaceCorps", "{id}");
        assert!(
            !x.summary.is_empty() && x.summary.chars().count() <= 140,
            "{id}"
        );
        assert_eq!(x.homepage, "https://github.com/SpaceCorps/obop", "{id}");
        assert_eq!(x.repository, "https://github.com/SpaceCorps/obop", "{id}");
        assert!(!x.categories.is_empty(), "{id}");
        assert!(
            x.categories
                .iter()
                .all(|c| MODULE_CATEGORIES.contains(&c.as_str()))
        );
        assert!((2..=10).contains(&x.keywords.len()), "{id}");
        assert!(MATURITIES.contains(&x.maturity.as_str()), "{id}");
        assert!(m.catalog_problems().is_empty(), "{id}");
        assert!(
            x.depends.is_empty(),
            "{id}: bundled packages use the structured table"
        );
        assert!(
            m.semver() >= Version::new(0, 2, 0),
            "{id}: every package moved up a minor"
        );
        // Every dependency exists and its range is satisfied by what exists.
        for r in m.requirements() {
            if builtin.contains(&r.id) {
                assert!(
                    r.req
                        .matches(&Version::parse(obop_types::CORE_VERSION).unwrap()),
                    "{id} needs {} {}, but built-ins version with the core",
                    r.id,
                    r.req
                );
            } else {
                assert!(ids.contains(&r.id), "{id} needs unknown {}", r.id);
                let dep = all.iter().find(|p| p.module.id == r.id).unwrap();
                assert!(
                    r.req.matches(&dep.semver()),
                    "{id} needs {} {}",
                    r.id,
                    r.req
                );
            }
        }
    }
    // The listing carries the same metadata, so a remote index can show cards unfetched.
    let listing = Bundled.list().unwrap();
    let ts = listing.iter().find(|l| l.id == "timesheets").unwrap();
    assert_eq!(ts.license, "AGPL-3.0-or-later");
    assert_eq!(ts.dependencies["projects"], ">=0.3, <1");
    assert_eq!(ts.requires_core, "^0.2");
}

#[test]
fn installing_timesheets_alone_pulls_projects_and_a_dependency_on_base_is_the_core() {
    let app = App::new();
    let (plan, root) = app
        .installer
        .plan(PlanRequest::Market("timesheets"))
        .unwrap();
    assert!(plan.ok(), "{:?}", plan.blockers);
    assert_eq!(root, "timesheets");
    let steps: Vec<(&str, &str)> = plan
        .steps
        .iter()
        .map(|s| (s.id.as_str(), s.action.as_str()))
        .collect();
    assert_eq!(steps, [("projects", "install"), ("timesheets", "install")]);
    // field_service needs base and product (built in) and hr (a package).
    let (plan, _) = app
        .installer
        .plan(PlanRequest::Market("field_service"))
        .unwrap();
    assert_eq!(
        plan.steps.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
        ["hr", "field_service"]
    );
    let o = app
        .installer
        .install_from_marketplace_outcome("timesheets", "test")
        .unwrap();
    assert_eq!(o.applied.len(), 2);
    assert!(
        app.dispatcher.has_model("projects_task") && app.dispatcher.has_model("timesheets_entry")
    );
}

#[test]
fn all_22_packages_install_with_their_dependencies_and_the_module_list_shows_every_edge() {
    let app = App::new();
    let admin = Principal::user("root", &[ADMIN_GROUP]);
    let call = |id: &str, input: Value| {
        app.dispatcher
            .call(Surface::Rest, &admin, &mut Vault::new(), id, &input)
    };
    let list = |v: &Value| match v {
        Value::List(l) => l.clone(),
        _ => panic!(),
    };
    for l in Bundled.list().unwrap() {
        let have = list(
            &call("module.list", Value::Map(Default::default()))
                .unwrap()
                .value,
        );
        let installed = have
            .iter()
            .any(|r| matches!(r, Value::Map(m) if m["id"] == Value::text(&l.id)));
        if !installed {
            call(
                "marketplace_listing.install",
                Value::map([("id", Value::text(&l.id))]),
            )
            .unwrap_or_else(|e| panic!("{}: {e:?}", l.id));
        }
    }
    let rows = list(
        &call("module.list", Value::Map(Default::default()))
            .unwrap()
            .value,
    );
    // 7 built-in (6 modules and the admin module) plus 22.
    assert_eq!(rows.len(), app.modules.len() + 22);
    for r in &rows {
        let Value::Map(m) = r else { panic!() };
        let id = format!("{:?}", m["id"]);
        for k in [
            "version",
            "license",
            "requires_core",
            "compatible",
            "dependencies",
            "dependents",
            "summary",
            "author",
            "maturity",
            "categories",
            "keywords",
            "contents",
            "source",
        ] {
            assert!(m.contains_key(k), "{id} lacks {k}");
        }
        assert_eq!(m["compatible"], Value::Bool(true), "{id}");
        let Value::List(deps) = &m["dependencies"] else {
            panic!()
        };
        for d in deps {
            let Value::Map(d) = d else { panic!() };
            assert_eq!(d["satisfied"], Value::Bool(true), "{id} -> {:?}", d["id"]);
        }
    }
    let by = |id: &str| -> std::collections::BTreeMap<String, Value> {
        rows.iter()
            .find_map(|r| match r {
                Value::Map(m) if m["id"] == Value::text(id) => Some(m.clone()),
                _ => None,
            })
            .unwrap()
    };
    // projects is needed by timesheets; base by many; a built-in has the same fields.
    assert_eq!(
        by("projects")["dependents"],
        Value::List(vec![Value::text("timesheets")])
    );
    assert_eq!(by("base")["version"], Value::text("0.2.0"));
    assert_eq!(by("base")["license"], Value::text("AGPL-3.0-or-later"));
    let Value::List(base_dependents) = &by("base")["dependents"] else {
        panic!()
    };
    assert!(base_dependents.len() >= 6, "{base_dependents:?}");
    // Remove in reverse dependency order, each removal allowed only once nothing needs it.
    assert!(
        call(
            "module.uninstall",
            Value::map([("id", Value::text("projects"))])
        )
        .is_err()
    );
    call(
        "module.uninstall",
        Value::map([("id", Value::text("timesheets"))]),
    )
    .unwrap();
    call(
        "module.uninstall",
        Value::map([("id", Value::text("projects"))]),
    )
    .unwrap();
}

#[test]
fn the_check_command_applies_the_catalogue_rules_and_reports_warnings() {
    let pk = root().join("packages");
    let mut files: Vec<String> = std::fs::read_dir(&pk)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    files.sort();
    assert_eq!(files.len(), 22);
    let refs: Vec<&str> = files.iter().map(String::as_str).collect();
    assert!(
        modules_check_files_with(&refs, false, true).is_ok(),
        "--catalog accepts every shipped package"
    );
    // Without a licence, or with a core range that has no upper bound, a package passes `check`
    // (with warnings) but not `check --catalog`. No core range at all is refused everywhere.
    let dir = std::env::temp_dir().join(format!("obop-catalog-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let text = std::fs::read_to_string(pk.join("fleet.toml")).unwrap();
    let bare = text
        .replace("license = \"AGPL-3.0-or-later\"\n", "")
        .replace("requires_core = \"^0.2\"\n", "requires_core = \">=0.2\"\n");
    let f = dir.join("fleet.toml");
    std::fs::write(&f, bare).unwrap();
    let fs = f.to_string_lossy().to_string();
    assert!(modules_check_files_with(&[&fs], false, false).is_ok());
    assert!(modules_check_files_with(&[&fs], false, true).is_err());
    let builtin = App::new().modules;
    let r = obop_pkg::check_package(&std::fs::read_to_string(&f).unwrap(), &builtin, false);
    assert!(r.errors.is_empty() && r.warnings.len() == 2, "{r:?}");
    let r = obop_pkg::check_package(&std::fs::read_to_string(&f).unwrap(), &builtin, true);
    assert!(
        r.errors.iter().any(|e| e.contains("license is required")),
        "{r:?}"
    );
    assert!(
        r.errors.iter().any(|e| e.contains("has no upper bound")),
        "{r:?}"
    );
    let no_range = text.replace("requires_core = \"^0.2\"\n", "");
    for catalog in [false, true] {
        let r = obop_pkg::check_package(&no_range, &builtin, catalog);
        assert!(
            r.errors
                .iter()
                .any(|e| e.contains("requires_core is required")),
            "{r:?}"
        );
    }
    // Ranges that nothing satisfies, an unknown dependency, a cycle-free core mismatch.
    let timesheets = std::fs::read_to_string(pk.join("timesheets.toml")).unwrap();
    for (broken, want) in [
        (
            timesheets.replace("projects = \">=0.3, <1\"", "projects = \">=5, <6\""),
            "needs projects >=5",
        ),
        (
            timesheets.replace("projects = \">=0.3, <1\"", "ghost = \"^1\""),
            "needs module ghost",
        ),
        (
            timesheets.replace(
                "[module.dependencies]",
                "[module.dependencies]\nbase = \">=0.9\"",
            ),
            "built-in module that versions with the core",
        ),
        (
            timesheets.replace("requires_core = \"^0.2\"", "requires_core = \">=9, <10\""),
            "needs core >=9",
        ),
        (
            timesheets.replace("license = \"AGPL-3.0-or-later\"", "license = \"Apache\""),
            "not a known SPDX",
        ),
        (
            timesheets.replace(
                "[module.dependencies]",
                "depends = [\"projects\"]\n[module.dependencies]",
            ),
            "depends = [...] is no longer accepted",
        ),
        (
            timesheets.replace("projects = \">=0.3, <1\"", "projects = \"\""),
            "the version range is empty",
        ),
        (
            timesheets.replace("projects = \">=0.3, <1\"", "projects = \">=2, <1\""),
            "matches no version",
        ),
    ] {
        let r = obop_pkg::check_package(&broken, &builtin, true);
        assert!(
            r.errors.iter().any(|e| e.contains(want)),
            "{want}: {:?}",
            r.errors
        );
    }
    // An upload with a core mismatch is a warning for `check` (the author may prepare for a
    // later core) and an error at install time.
    let r = obop_pkg::check_package(
        &timesheets.replace("requires_core = \"^0.2\"", "requires_core = \">=9, <10\""),
        &builtin,
        false,
    );
    assert!(
        r.errors.is_empty() && r.warnings.iter().any(|w| w.contains("needs core >=9")),
        "{r:?}"
    );
}

#[test]
fn the_tree_list_and_plan_commands_run() {
    let o = crate::Opts {
        db: Some("memory:".into()),
        token: None,
        demo: false,
        dev_auth: false,
        public_site: false,
        trust_proxy: false,
        admin_prefix: "/admin".into(),
    };
    assert!(crate::modules_list(&o).is_ok());
    assert!(modules_tree(&o, Some("timesheets")).is_ok());
    assert!(modules_tree(&o, None).is_ok());
    assert!(modules_tree(&o, Some("nope")).is_err());
    assert!(modules_plan(&o, "timesheets").is_ok());
    assert!(modules_plan(&o, "nope").is_err());
    let app = App::new();
    let t = app.installer.tree(Some("timesheets")).unwrap();
    assert!(t.contains("timesheets 0.3.0"), "{t}");
    assert!(
        t.contains("projects >=0.3, <1  [not installed, marketplace offers 0.4.0]"),
        "{t}"
    );
    let t = app.installer.tree(Some("field_service")).unwrap();
    assert!(t.contains("base ^0.2  [built in, core 0.2.0]"), "{t}");
    assert!(
        t.contains("hr >=0.2, <1  [not installed, marketplace offers 0.3.0]"),
        "{t}"
    );
}

async fn post(r: &axum::Router, token: Option<&str>, path: &str, body: J) -> (u16, J) {
    let mut req = Request::post(path).header("content-type", "application/json");
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    let resp = r
        .clone()
        .oneshot(req.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = resp.status().as_u16();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(J::Null))
}

async fn get(r: &axum::Router, token: &str, path: &str) -> J {
    let resp = r
        .clone()
        .oneshot(
            Request::get(path)
                .header("host", "inst.test")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

#[tokio::test]
async fn the_core_version_is_served_and_an_agent_proposes_a_plan_that_one_approval_runs() {
    let app = App::new();
    app.seed_demo();
    let auth = Arc::new(Auth::with_test_params(app.store.backend()));
    auth.create_user("ada", "correct horse battery", &app.all_groups())
        .unwrap();
    let r = obop_http::router(
        obop_http::HttpState::new(app.dispatcher.clone(), auth.clone()),
        None,
    );
    let login = post(
        &r,
        None,
        "/api/auth/login",
        json!({"username": "ada", "password": "correct horse battery"}),
    )
    .await;
    let human = login.1["token"].as_str().unwrap().to_string();
    // The one core version, everywhere it is reported.
    let meta = get(&r, &human, "/api/meta").await;
    assert_eq!(meta["core_version"], "0.2.0");
    let api = get(&r, &human, "/api/openapi.json").await;
    assert_eq!(api["info"]["version"], "0.2.0");
    // The plan over HTTP, read-only.
    let (s, plan) = post(
        &r,
        Some(&human),
        "/api/module_plan/preview",
        json!({"id": "timesheets"}),
    )
    .await;
    assert_eq!(s, 200, "{plan}");
    let v = &plan["value"];
    assert_eq!(v["ok"], true);
    assert_eq!(v["core_version"], "0.2.0");
    assert_eq!(v["steps"][0]["id"], "projects");
    assert_eq!(v["steps"][0]["license"], "AGPL-3.0-or-later");
    assert_eq!(v["steps"][1]["id"], "timesheets");
    assert!(!app.dispatcher.has_model("projects_task"));
    // An agent scoped to the admin group proposes; nothing runs; one approval runs the plan.
    let (s, tok) = post(
        &r,
        Some(&human),
        "/api/auth/agent-token",
        json!({"scopes": [ADMIN_GROUP], "label": "installer"}),
    )
    .await;
    assert_eq!(s, 200, "{tok}");
    let agent = tok["token"].as_str().unwrap().to_string();
    let (s, p) = post(
        &r,
        Some(&agent),
        "/api/marketplace_listing/install",
        json!({"id": "timesheets"}),
    )
    .await;
    assert_eq!(s, 202, "{p}");
    assert!(
        !app.dispatcher.has_model("projects_task") && !app.dispatcher.has_model("timesheets_entry")
    );
    let plan_id = p["plan_id"].as_str().unwrap().to_string();
    let (s, done) = post(
        &r,
        Some(&human),
        &format!("/api/approve/{plan_id}"),
        json!({}),
    )
    .await;
    assert_eq!(s, 200, "{done}");
    assert_eq!(done["value"]["steps"].as_array().unwrap().len(), 2);
    assert!(
        app.dispatcher.has_model("projects_task") && app.dispatcher.has_model("timesheets_entry")
    );
    // The listing now says timesheets is installed, and uninstalling projects is refused.
    let (_, l) = post(&r, Some(&human), "/api/marketplace_listing/list", json!({})).await;
    let rows = l["value"].as_array().unwrap();
    let ts = rows.iter().find(|x| x["id"] == "timesheets").unwrap();
    assert_eq!(ts["installed"], true);
    assert_eq!(ts["dependencies"][0]["id"], "projects");
    let (s, e) = post(
        &r,
        Some(&human),
        "/api/module/uninstall",
        json!({"id": "projects"}),
    )
    .await;
    assert_eq!(s, 400, "{e}");
    assert!(
        e["error"]
            .as_str()
            .unwrap()
            .contains("projects is required by timesheets"),
        "{e}"
    );
    // An agent may read the preview without an approval.
    let (s, _) = post(
        &r,
        Some(&agent),
        "/api/module_plan/preview",
        json!({"id": "hr"}),
    )
    .await;
    assert_eq!(s, 200);
}
