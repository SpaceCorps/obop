//! Installing with dependencies through the admin commands (D55): plans, atomic apply, agent
//! approval of a whole plan, upgrade conflicts, uninstall blocking, core mismatch, and the
//! metadata the module list carries.

use crate::installer::{Installer, Request, admin_module, register_admin};
use crate::manifest::Manifest;
use crate::marketplace::{Listing, Marketplace};
use obop_commands::{
    ADMIN_GROUP, CallError, Dispatcher, Principal, Response, Store, Surface, Value, Vault,
};
use std::collections::BTreeMap;
use std::sync::Arc;

/// A marketplace backed by a list of manifest texts.
struct Shelf(Vec<String>);

impl Marketplace for Shelf {
    fn list(&self) -> Result<Vec<Listing>, String> {
        self.0
            .iter()
            .map(|t| Manifest::parse(t).map(|m| Listing::from_manifest(&m)))
            .collect()
    }
    fn fetch(&self, id: &str) -> Result<String, String> {
        self.0
            .iter()
            .find(|t| Manifest::parse(t).is_ok_and(|m| m.module.id == id))
            .cloned()
            .ok_or_else(|| format!("no package {id}"))
    }
}

/// A valid package with one model `<id>_item` and the given dependencies.
fn pkg(id: &str, version: &str, core: &str, deps: &[(&str, &str)]) -> String {
    let mut t = format!(
        "[module]\nid = \"{id}\"\nversion = \"{version}\"\nname = \"{id}\"\ndescription = \"The {id} module\"\nsummary = \"Summary of {id}\"\nauthor = \"Tester\"\nlicense = \"MIT\"\nrequires_core = \"{core}\"\n"
    );
    if !deps.is_empty() {
        t.push_str("[module.dependencies]\n");
        for (d, r) in deps {
            t.push_str(&format!("{d} = \"{r}\"\n"));
        }
    }
    t.push_str(&format!(
        "[[model]]\nname = \"{id}_item\"\ndescription = \"An item\"\nread_group = \"{id}.user\"\nwrite_group = \"{id}.user\"\n[[model.field]]\nname = \"title\"\ndescription = \"Title\"\nclass = \"none\"\n"
    ));
    t
}

fn setup(shelf: Vec<String>, store: &Store) -> (Arc<Dispatcher>, Arc<Installer>) {
    let d = Arc::new(Dispatcher::new());
    // `platform` stands in for a built-in module: it versions with the core.
    let inst = Installer::new(
        store.clone(),
        &d,
        vec![admin_module()],
        Arc::new(Shelf(shelf)),
    );
    register_admin(&d, &inst);
    (d, inst)
}

fn admin() -> Principal {
    Principal::user("root", &[ADMIN_GROUP])
}

fn call(d: &Dispatcher, p: &Principal, id: &str, input: Value) -> Result<Response, CallError> {
    d.call(Surface::Rest, p, &mut Vault::new(), id, &input)
}

fn by_id(id: &str) -> Value {
    Value::map([("id", Value::text(id))])
}

fn get<'a>(v: &'a Value, k: &str) -> &'a Value {
    match v {
        Value::Map(m) => m.get(k).unwrap_or_else(|| panic!("no {k} in {v:?}")),
        _ => panic!("not a map: {v:?}"),
    }
}

fn text(v: &Value) -> &str {
    match v {
        Value::Text(t) => t,
        other => panic!("not text: {other:?}"),
    }
}

fn list(v: &Value) -> &[Value] {
    match v {
        Value::List(l) => l,
        other => panic!("not a list: {other:?}"),
    }
}

fn step_ids(v: &Value) -> Vec<String> {
    list(get(v, "steps"))
        .iter()
        .map(|s| format!("{}:{}", text(get(s, "id")), text(get(s, "action"))))
        .collect()
}

fn handler_error(r: Result<Response, CallError>) -> String {
    match r {
        Err(CallError::Handler(m)) => m,
        other => panic!("expected a handler error, got {other:?}"),
    }
}

/// c needs b, b needs a: a chain, plus `platform` (built-in) as a dependency of a.
fn chain() -> Vec<String> {
    vec![
        pkg("a", "0.2.0", "^0.2", &[("platform", "^0.2")]),
        pkg("b", "0.2.0", "^0.2", &[("a", ">=0.2, <1")]),
        pkg("c", "0.2.0", "^0.2", &[("b", "^0.2"), ("platform", "^0.2")]),
    ]
}

fn row<'a>(rows: &'a Value, id: &str) -> &'a Value {
    list(rows)
        .iter()
        .find(|r| text(get(r, "id")) == id)
        .unwrap_or_else(|| panic!("no row {id}"))
}

#[test]
fn installing_one_module_installs_its_dependencies_in_order_in_one_operation() {
    let (d, _) = setup(chain(), &Store::new());
    let r = call(&d, &admin(), "marketplace_listing.install", by_id("c")).unwrap();
    assert_eq!(text(get(&r.value, "id")), "c");
    assert_eq!(step_ids(&r.value), ["a:install", "b:install", "c:install"]);
    for m in ["a_item", "b_item", "c_item"] {
        assert!(d.has_model(m), "{m}");
    }
    // The module list says who needs whom and what is satisfied.
    let l = call(&d, &admin(), "module.list", Value::Map(Default::default()))
        .unwrap()
        .value;
    let c = row(&l, "c");
    let deps = list(get(c, "dependencies"));
    let b = deps.iter().find(|x| text(get(x, "id")) == "b").unwrap();
    assert_eq!(text(get(b, "kind")), "package");
    assert_eq!(text(get(b, "installed_version")), "0.2.0");
    assert_eq!(get(b, "satisfied"), &Value::Bool(true));
    let p = deps
        .iter()
        .find(|x| text(get(x, "id")) == "platform")
        .unwrap();
    assert_eq!(text(get(p, "kind")), "built-in");
    assert_eq!(
        text(get(p, "installed_version")),
        obop_types::CORE_VERSION,
        "a built-in is as old as the core"
    );
    assert_eq!(
        list(get(row(&l, "a"), "dependents"))
            .iter()
            .map(text)
            .collect::<Vec<_>>(),
        ["b"]
    );
    assert_eq!(text(get(c, "source")), "marketplace");
    assert!(matches!(get(c, "installed_at"), Value::Int(t) if *t > 0));
    assert_eq!(text(get(c, "installed_by")), "root");
    assert_eq!(get(c, "compatible"), &Value::Bool(true));
    assert_eq!(text(get(c, "license")), "MIT");
    assert_eq!(text(get(c, "license_kind")), "spdx");
    assert_eq!(
        text(get(c, "license_url")),
        "https://spdx.org/licenses/MIT.html"
    );
    // Built-in modules carry the same fields.
    let platform = row(&l, "platform");
    assert_eq!(text(get(platform, "version")), obop_types::CORE_VERSION);
    assert_eq!(text(get(platform, "source")), "builtin");
    assert_eq!(text(get(platform, "license")), "Apache-2.0");
    assert!(!list(get(platform, "keywords")).is_empty());
}

#[test]
fn dependencies_cannot_be_skipped() {
    let (d, _) = setup(chain(), &Store::new());
    let e = handler_error(call(
        &d,
        &admin(),
        "marketplace_listing.install",
        Value::map([
            ("id", Value::text("c")),
            ("without_dependencies", Value::Bool(true)),
        ]),
    ));
    assert!(e.contains("cannot be skipped"), "{e}");
    assert!(!d.has_model("c_item"));
    // false is the default and is fine.
    call(
        &d,
        &admin(),
        "marketplace_listing.install",
        Value::map([
            ("id", Value::text("a")),
            ("without_dependencies", Value::Bool(false)),
        ]),
    )
    .unwrap();
}

#[test]
fn an_agent_proposes_the_whole_plan_and_one_approval_runs_it_once() {
    let (d, _) = setup(chain(), &Store::new());
    let agent = Principal::agent("bot", &[ADMIN_GROUP], &[ADMIN_GROUP]);
    let Err(CallError::ApprovalRequired { plan_id }) =
        call(&d, &agent, "marketplace_listing.install", by_id("c"))
    else {
        panic!("an agent only proposes")
    };
    for m in ["a_item", "b_item", "c_item"] {
        assert!(!d.has_model(m), "{m} installed before approval");
    }
    // The agent may read the plan freely, with no approval.
    let plan = call(&d, &agent, "module_plan.preview", by_id("c"))
        .unwrap()
        .value;
    assert_eq!(step_ids(&plan), ["a:install", "b:install", "c:install"]);
    let r = d.approve(&admin(), &mut Vault::new(), &plan_id).unwrap();
    assert_eq!(step_ids(&r.value), ["a:install", "b:install", "c:install"]);
    for m in ["a_item", "b_item", "c_item"] {
        assert!(d.has_model(m), "{m}");
    }
    assert!(
        matches!(
            d.approve(&admin(), &mut Vault::new(), &plan_id),
            Err(CallError::UnknownPlan(_))
        ),
        "the plan runs once"
    );
}

#[test]
fn a_step_that_fails_undoes_the_steps_before_it() {
    let store = Store::new();
    let (d, inst) = setup(chain(), &store);
    let (mut plan, _) = inst.plan(Request::Market("c")).unwrap();
    assert!(plan.ok());
    // Break the last step after planning: it refers to a model nobody provides.
    let broken = pkg("c", "0.2.0", "^0.2", &[("b", "^0.2")]).replace(
        "name = \"title\"",
        "name = \"ghost\"\nkind = \"ref\"\nto = \"nowhere\"\ndescription = \"x\"\nclass = \"none\"\n[[model.field]]\nname = \"title\"",
    );
    let last = plan.steps.last_mut().unwrap();
    assert_eq!(last.id, "c");
    last.manifest = Manifest::parse(&broken).unwrap();
    let e = inst
        .apply_plan(&plan, "c", "marketplace", "root")
        .unwrap_err();
    assert!(e.contains("rolled back") && e.contains("nowhere"), "{e}");
    for m in ["a_item", "b_item", "c_item"] {
        assert!(!d.has_model(m), "{m} must not remain");
    }
    assert!(
        inst.list().iter().all(|p| p.source == "builtin"),
        "nothing is recorded"
    );
    // The same plan, unbroken, still installs.
    let (plan, _) = inst.plan(Request::Market("c")).unwrap();
    inst.apply_plan(&plan, "c", "marketplace", "root").unwrap();
    assert!(d.has_model("c_item"));
}

#[test]
fn a_failed_upgrade_step_restores_the_old_version() {
    let store = Store::new();
    let shelf = [
        pkg("a", "0.2.0", "^0.2", &[]),
        pkg("a", "0.3.0", "^0.2", &[]),
    ];
    // The shelf offers a 0.2.0 first; installing it, then upgrading with a broken manifest.
    let (d, inst) = setup(shelf[..1].to_vec(), &store);
    inst.install(&shelf[0], "upload", "root").unwrap();
    let (mut plan, _) = inst.plan(Request::Manifest(&shelf[1])).unwrap();
    assert_eq!(plan.steps[0].action.as_str(), "upgrade");
    let broken = shelf[1].replace(
        "name = \"title\"",
        "name = \"ghost\"\nkind = \"ref\"\nto = \"nowhere\"\ndescription = \"x\"\nclass = \"none\"\n[[model.field]]\nname = \"title\"",
    );
    plan.steps[0].manifest = Manifest::parse(&broken).unwrap();
    assert!(inst.apply_plan(&plan, "a", "upload", "root").is_err());
    assert!(d.has_model("a_item"), "the old version is still registered");
    let rows = inst.module_rows();
    assert_eq!(text(get(row(&rows, "a"), "version")), "0.2.0");
}

#[test]
fn upgrading_past_what_a_dependent_allows_is_blocked_and_explained() {
    let shelf = vec![
        pkg("projects", "0.2.0", "^0.2", &[]),
        pkg("timesheets", "0.2.0", "^0.2", &[("projects", ">=0.2, <1")]),
    ];
    let (d, inst) = setup(shelf, &Store::new());
    call(
        &d,
        &admin(),
        "marketplace_listing.install",
        by_id("timesheets"),
    )
    .unwrap();
    let v2 = pkg("projects", "2.0.0", "^0.2", &[]);
    let e = handler_error(call(
        &d,
        &admin(),
        "module.install",
        Value::map([("manifest", Value::text(&v2))]),
    ));
    assert!(
        e.contains("would break timesheets, which needs projects >=0.2, <1"),
        "{e}"
    );
    let plan = call(
        &d,
        &admin(),
        "module_plan.preview",
        Value::map([("manifest", Value::text(&v2))]),
    )
    .unwrap()
    .value;
    assert_eq!(get(&plan, "ok"), &Value::Bool(false));
    let b = &list(get(&plan, "blockers"))[0];
    assert_eq!(text(get(b, "kind")), "conflict");
    assert_eq!(text(get(b, "module")), "projects");
    // Nothing changed.
    assert_eq!(
        text(get(row(&inst.module_rows(), "projects"), "version")),
        "0.2.0"
    );
    // An upgrade inside the range goes through.
    let v09 = pkg("projects", "0.9.0", "^0.2", &[]);
    let r = call(
        &d,
        &admin(),
        "module.install",
        Value::map([("manifest", Value::text(&v09))]),
    )
    .unwrap();
    assert_eq!(step_ids(&r.value), ["projects:upgrade"]);
}

#[test]
fn uninstall_is_blocked_while_dependents_exist_and_lists_them_transitively() {
    let (d, inst) = setup(chain(), &Store::new());
    call(&d, &admin(), "marketplace_listing.install", by_id("c")).unwrap();
    let e = handler_error(call(&d, &admin(), "module.uninstall", by_id("a")));
    assert!(
        e.contains("a is required by b") && e.contains("through them, c"),
        "{e}"
    );
    let rows = inst.module_rows();
    let a = row(&rows, "a");
    assert_eq!(get(a, "uninstallable"), &Value::Bool(false));
    assert_eq!(list(get(a, "uninstall_blockers")).len(), 1);
    assert_eq!(get(row(&rows, "c"), "uninstallable"), &Value::Bool(true));
    // Removing from the top works, one by one.
    for id in ["c", "b", "a"] {
        call(&d, &admin(), "module.uninstall", by_id(id)).unwrap();
        assert!(!d.has_model(&format!("{id}_item")));
    }
    let e = handler_error(call(&d, &admin(), "module.uninstall", by_id("platform")));
    assert!(e.contains("built-in"), "{e}");
}

#[test]
fn a_package_list_the_store_refused_does_not_let_a_package_be_removed_from_under_its_dependents() {
    use obop_storage::testing::{Flaky, Mode};
    // The list of the installed packages was read as "none installed" by the command line (no
    // request scope, so no guard), and the check "does another package need it" found nobody.
    let flaky = Flaky::new(obop_storage::open("memory:").unwrap());
    let (d, inst) = setup(chain(), &Store::over(flaky.clone()));
    call(&d, &admin(), "marketplace_listing.install", by_id("c")).unwrap();
    flaky.refuse_model("obop_package", Mode::Reads);
    let done = inst.uninstall("a");
    flaky.allow_models();
    assert!(
        matches!(&done, Err(m) if m.starts_with("storage_busy")),
        "{done:?}"
    );
    assert!(d.has_model("a_item"), "a is still installed");
    // Healthy again, it is the dependents that stop it.
    assert!(inst.uninstall("a").unwrap_err().contains("required by b"));
}

#[test]
fn a_core_mismatch_refuses_the_install_with_both_versions() {
    let (d, _) = setup(vec![pkg("future", "1.0.0", ">=9, <10", &[])], &Store::new());
    let e = handler_error(call(
        &d,
        &admin(),
        "marketplace_listing.install",
        by_id("future"),
    ));
    assert_eq!(
        e,
        format!(
            "future 1.0.0 needs core >=9, <10, you run {}",
            obop_types::CORE_VERSION
        )
    );
    let e = handler_error(call(
        &d,
        &admin(),
        "module.install",
        Value::map([(
            "manifest",
            Value::text(&pkg("up", "0.1.0", ">=9, <10", &[])),
        )]),
    ));
    assert!(e.contains("needs core >=9"), "{e}");
    // The listing says so before anyone clicks.
    let l = call(
        &d,
        &admin(),
        "marketplace_listing.list",
        Value::Map(Default::default()),
    )
    .unwrap()
    .value;
    let f = row(&l, "future");
    assert_eq!(get(f, "compatible"), &Value::Bool(false));
    assert_eq!(get(f, "installable"), &Value::Bool(false));
    assert_eq!(text(get(&list(get(f, "blockers"))[0], "kind")), "core");
}

#[test]
fn a_cycle_is_named_in_the_error() {
    let shelf = vec![
        pkg("p", "0.1.0", "^0.2", &[("q", "^0.1")]),
        pkg("q", "0.1.0", "^0.2", &[("p", "^0.1")]),
    ];
    let (d, _) = setup(shelf, &Store::new());
    let e = handler_error(call(
        &d,
        &admin(),
        "marketplace_listing.install",
        by_id("p"),
    ));
    assert!(e.contains("dependency cycle: p -> q -> p"), "{e}");
    assert!(!d.has_model("p_item") && !d.has_model("q_item"));
}

#[test]
fn preview_changes_nothing_and_describes_every_step() {
    let (d, _) = setup(chain(), &Store::new());
    let plan = call(&d, &admin(), "module_plan.preview", by_id("c"))
        .unwrap()
        .value;
    assert_eq!(get(&plan, "ok"), &Value::Bool(true));
    assert_eq!(text(get(&plan, "core_version")), obop_types::CORE_VERSION);
    let steps = list(get(&plan, "steps"));
    let first = &steps[0];
    assert_eq!(text(get(first, "id")), "a");
    assert_eq!(text(get(first, "license")), "MIT");
    assert!(text(get(first, "reason")).contains("needed by b"));
    assert_eq!(get(&steps[2], "requested"), &Value::Bool(true));
    assert!(!d.has_model("a_item"), "a preview installs nothing");
    // Parse errors are reported, not swallowed.
    let e = handler_error(call(
        &d,
        &admin(),
        "module_plan.preview",
        Value::map([("manifest", Value::text("[module]\nid = 3"))]),
    ));
    assert!(e.contains("invalid obop.toml"), "{e}");
    let e = handler_error(call(
        &d,
        &admin(),
        "module_plan.preview",
        Value::Map(Default::default()),
    ));
    assert!(e.contains("either id"), "{e}");
}

#[test]
fn the_marketplace_listing_reports_installability_and_updates() {
    let shelf = vec![
        pkg("a", "0.2.0", "^0.2", &[]),
        pkg("b", "0.2.0", "^0.2", &[("a", "^0.2"), ("ghost", "^0.1")]),
    ];
    let store = Store::new();
    let (d, inst) = setup(shelf.clone(), &store);
    let l = inst.listing_rows().unwrap();
    assert_eq!(get(row(&l, "a"), "installable"), &Value::Bool(true));
    let b = row(&l, "b");
    assert_eq!(get(b, "installable"), &Value::Bool(false));
    assert_eq!(text(get(&list(get(b, "blockers"))[0], "kind")), "missing");
    let ghost = list(get(b, "dependencies"))
        .iter()
        .find(|x| text(get(x, "id")) == "ghost")
        .unwrap();
    assert_eq!(text(get(ghost, "kind")), "missing");
    // Install a, then the marketplace offers 0.3.0: an update is available.
    call(&d, &admin(), "marketplace_listing.install", by_id("a")).unwrap();
    let newer = vec![pkg("a", "0.3.0", "^0.2", &[]), shelf[1].clone()];
    let (_d2, inst2) = setup(newer, &store);
    assert!(inst2.load_installed().is_empty());
    let l = inst2.listing_rows().unwrap();
    let a = row(&l, "a");
    assert_eq!(get(a, "installed"), &Value::Bool(true));
    assert_eq!(text(get(a, "installed_version")), "0.2.0");
    assert_eq!(text(get(a, "update_available")), "0.3.0");
    let m = inst2.module_rows();
    assert_eq!(text(get(row(&m, "a"), "update_available")), "0.3.0");
    assert_eq!(get(row(&m, "platform"), "update_available"), &Value::Null);
}

#[test]
fn catalogue_rules_apply_to_marketplace_packages_and_warnings_to_uploads() {
    // A marketplace package without a licence or requires_core is refused...
    let bare = pkg("bare", "0.1.0", "^0.2", &[]).replace("license = \"MIT\"\n", "");
    let (d, inst) = setup(vec![bare.clone()], &Store::new());
    let e = inst.install(&bare, "marketplace", "root").unwrap_err();
    assert!(e.contains("license is required"), "{e}");
    let e = handler_error(call(
        &d,
        &admin(),
        "marketplace_listing.install",
        by_id("bare"),
    ));
    assert!(e.contains("license is required"), "{e}");
    // ... an upload is allowed, with warnings that the plan reports.
    let upload = pkg("up", "0.1.0", "^0.2", &[])
        .replace("license = \"MIT\"\n", "license = \"proprietary\"\n")
        .replace("requires_core = \"^0.2\"\n", "requires_core = \">=0.2\"\n");
    let plan = call(
        &d,
        &admin(),
        "module_plan.preview",
        Value::map([("manifest", Value::text(&upload))]),
    )
    .unwrap()
    .value;
    assert_eq!(get(&plan, "ok"), &Value::Bool(true));
    let w: Vec<&str> = list(get(&plan, "warnings")).iter().map(text).collect();
    assert!(
        w.iter()
            .any(|x| x.contains("requires_core = \">=0.2\" has no upper bound")),
        "{w:?}"
    );
    assert!(w.iter().any(|x| x.contains("proprietary")), "{w:?}");
    let r = call(
        &d,
        &admin(),
        "module.install",
        Value::map([("manifest", Value::text(&upload))]),
    )
    .unwrap();
    assert_eq!(list(get(&r.value, "warnings")).len(), 2);
    let rows = inst.module_rows();
    let up = row(&rows, "up");
    assert_eq!(text(get(up, "license_kind")), "proprietary");
    assert_eq!(text(get(up, "requires_core")), ">=0.2");
    assert_eq!(text(get(up, "source")), "upload");
}

#[test]
fn the_tree_and_the_plan_print_for_the_command_line() {
    let (_d, inst) = setup(chain(), &Store::new());
    let t = inst.tree(Some("c")).unwrap();
    assert!(t.starts_with("c 0.2.0"), "{t}");
    assert!(t.contains("marketplace, not installed"), "{t}");
    assert!(t.contains("├── b ^0.2"), "{t}");
    assert!(t.contains("platform ^0.2  [built in, core"), "{t}");
    let (text, ok) = inst.plan_text(Request::Market("c")).unwrap();
    assert!(ok && text.contains("1. install  a 0.2.0"), "{text}");
    assert!(inst.tree(Some("nope")).is_err());
    inst.install_from_marketplace("c", "root").unwrap();
    let t = inst.tree(None).unwrap();
    assert!(
        t.contains("a 0.2.0") && t.contains("[installed 0.2.0]"),
        "{t}"
    );
}

#[test]
fn old_and_new_marketplace_indexes_both_parse() {
    // The first format: five fields.
    let old = r#"[{"id":"fleet","name":"Fleet","version":"0.1.0","description":"d","author":"x"}]"#;
    let l: Vec<Listing> = serde_json::from_str(old).unwrap();
    assert_eq!(l[0].license, "");
    assert!(l[0].dependencies.is_empty());
    // It parses, but it names no supported core versions, which a listing must (D63).
    assert!(
        l[0].stub()
            .unwrap_err()
            .contains("requires_core is required")
    );
    // The current format carries the metadata, so cards need no manifest fetch.
    let new = r#"[{"id":"projects","name":"Projects","version":"0.3.0","description":"d","author":"SpaceCorps",
        "summary":"s","license":"AGPL-3.0-or-later","requires_core":"^0.2","categories":["services"],
        "keywords":["projects"],"maturity":"beta","dependencies":{"base":"^0.2","hr":">=0.1, <1"},"homepage":"https://example.com"}]"#;
    let l: Vec<Listing> = serde_json::from_str(new).unwrap();
    assert_eq!(l[0].dependencies["hr"], ">=0.1, <1");
    let stub = l[0].stub().unwrap();
    assert_eq!(stub.requirements().len(), 2);
    // A malformed entry is rejected by the stub.
    let bad = Listing {
        requires_core: "nonsense".into(),
        ..l[0].clone()
    };
    assert!(bad.stub().is_err());
    // The listing of a manifest round-trips through JSON.
    let m = Manifest::parse(&pkg("z", "1.2.3", "^0.2", &[("a", "^1")])).unwrap();
    let again: Listing =
        serde_json::from_str(&serde_json::to_string(&Listing::from_manifest(&m)).unwrap()).unwrap();
    assert_eq!(again.dependencies["a"], "^1");
    assert_eq!(again.version, "1.2.3");
}

#[test]
fn a_remote_index_serves_listings_and_leaves_out_malformed_ones() {
    use std::io::{Read, Write};
    let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = server.local_addr().unwrap().port();
    let index = r#"[
      {"id":"fleet","name":"Fleet","version":"0.1.0","description":"d","author":"x"},
      {"id":"projects","name":"Projects","version":"0.3.0","description":"d","license":"MIT","requires_core":"^0.2","dependencies":{"base":"^0.2"}},
      {"id":"broken","name":"Broken","version":"1.0","description":"d"},
      {"id":"BadId","name":"Bad","version":"1.0.0","description":"d"}
    ]"#;
    let t = std::thread::spawn(move || {
        let (mut s, _) = server.accept().unwrap();
        let mut buf = [0u8; 2048];
        let _ = s.read(&mut buf).unwrap();
        let body = index.to_string();
        let _ = write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
    });
    let l = crate::marketplace::Http::new(&format!("http://127.0.0.1:{port}"))
        .list()
        .unwrap();
    t.join().unwrap();
    assert_eq!(
        l.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(),
        ["projects"],
        "a listing without a core range is left out"
    );
    assert_eq!(l[0].license, "MIT");
}

#[test]
fn a_float_set_default_is_refused_on_install_and_upgrade_but_not_in_an_installed_package() {
    // Closure check of PR 33 (D90, F19): the previous core let a float through `set_default` for a
    // number or money field, so a package that is installed already may carry one, and it has to
    // keep loading, upgrading and rolling back. Only what comes in now is refused.
    let with_step = |version: &str, value: &str| {
        format!(
            "{}[[model.field]]\nname = \"qty\"\ndescription = \"Qty\"\nclass = \"none\"\nkind = \"number\"\n[[migration]]\nfrom = \"<0.2.0\"\nop = \"set_default\"\nmodel = \"fl_item\"\nfield = \"qty\"\nvalue = {value}\n",
            pkg("fl", version, "^0.2", &[])
        )
    };
    let store = Store::new();
    let (d, inst) = setup(vec![], &store);
    // A fresh install with a float is refused, and so is the same through the admin command.
    let e = inst
        .install(&with_step("0.2.0", "0.0"), "upload", "root")
        .unwrap_err();
    assert!(e.contains("does not fit the number field qty"), "{e}");
    assert!(!d.has_model("fl_item"));
    inst.install(&with_step("0.2.0", "0"), "upload", "root")
        .unwrap();
    // The stored copy as the previous core wrote it: with a float.
    store
        .update(
            "obop_package",
            "fl",
            BTreeMap::from([(
                "manifest".to_string(),
                Value::text(&with_step("0.2.0", "0.0")),
            )]),
        )
        .unwrap();
    // A restart still loads it.
    let (d2, inst2) = setup(vec![], &store);
    assert!(inst2.load_installed().is_empty(), "the module must load");
    assert!(d2.has_model("fl_item"));
    // An upgrade that brings a float is refused, nothing changes ...
    let e = handler_error(call(
        &d2,
        &admin(),
        "module.install",
        Value::map([("manifest", Value::text(&with_step("0.3.0", "2.5")))]),
    ));
    assert!(e.contains("does not fit the number field qty"), "{e}");
    assert_eq!(
        text(get(row(&inst2.module_rows(), "fl"), "version")),
        "0.2.0"
    );
    // ... and one that does not goes through, although the installed copy carries a float.
    let r = call(
        &d2,
        &admin(),
        "module.install",
        Value::map([("manifest", Value::text(&with_step("0.3.0", "1")))]),
    )
    .unwrap();
    assert_eq!(step_ids(&r.value), ["fl:upgrade"]);
}
