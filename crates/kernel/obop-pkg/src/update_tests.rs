//! Updating, rolling back and migrating packages from inside the instance (D63): additive and
//! breaking changes, the declarative data steps, atomicity, two processes at once, what an agent
//! may do, and the dependents' ranges in both directions.

use crate::installer::{Installer, Request, admin_module, register_admin};
use crate::manifest::Manifest;
use crate::marketplace::{Listing, Marketplace};
use obop_commands::{
    ADMIN_GROUP, CallError, Dispatcher, Principal, Response, Store, Surface, Value, Vault,
};
use serde_json::{Value as J, json};
use std::sync::{Arc, Mutex};

/// A marketplace whose shelf the test changes between calls.
#[derive(Default, Clone)]
pub(crate) struct Shelf(Arc<Mutex<Vec<String>>>);

impl Shelf {
    pub(crate) fn offer(&self, text: String) {
        let id = Manifest::parse(&text).unwrap().module.id;
        let mut s = self.0.lock().unwrap();
        s.retain(|t| Manifest::parse(t).unwrap().module.id != id);
        s.push(text);
    }
}

impl Marketplace for Shelf {
    fn list(&self) -> Result<Vec<Listing>, String> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .map(|t| Manifest::parse(t).map(|m| Listing::from_manifest(&m)))
            .collect()
    }
    fn fetch(&self, id: &str) -> Result<String, String> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .find(|t| Manifest::parse(t).is_ok_and(|m| m.module.id == id))
            .cloned()
            .ok_or_else(|| format!("no package {id}"))
    }
}

/// Package `id` at `version`: model `<id>_item` with a `title` field, plus `fields` (TOML field
/// tables), `migrations` (TOML), release notes and dependencies.
pub(crate) fn pkg(
    id: &str,
    version: &str,
    fields: &str,
    migrations: &str,
    notes: &str,
    deps: &str,
) -> String {
    format!(
        "[module]\nid = \"{id}\"\nversion = \"{version}\"\nname = \"{id}\"\ndescription = \"The {id} module\"\nlicense = \"MIT\"\nrequires_core = \"^0.2\"\nchangelog = {notes:?}\n{deps}\n[[model]]\nname = \"{id}_item\"\ndescription = \"An item\"\nread_group = \"{id}.user\"\nwrite_group = \"{id}.user\"\n[[model.field]]\nname = \"title\"\ndescription = \"Title\"\nclass = \"none\"\n{fields}\n{migrations}"
    )
}

pub(crate) fn field(name: &str, extra: &str) -> String {
    format!(
        "[[model.field]]\nname = \"{name}\"\ndescription = \"{name}\"\nclass = \"none\"\n{extra}\n"
    )
}

pub(crate) fn setup(store: &Store, shelf: &Shelf) -> (Arc<Dispatcher>, Arc<Installer>) {
    let d = Arc::new(Dispatcher::new());
    let inst = Installer::new(
        store.clone(),
        &d,
        vec![admin_module()],
        Arc::new(shelf.clone()),
    );
    register_admin(&d, &inst);
    (d, inst)
}

pub(crate) fn admin() -> Principal {
    Principal::user("root", &[ADMIN_GROUP])
}

pub(crate) fn call(
    d: &Dispatcher,
    p: &Principal,
    id: &str,
    input: Value,
) -> Result<Response, CallError> {
    d.call(Surface::Rest, p, &mut Vault::new(), id, &input)
}

pub(crate) fn by_id(id: &str) -> Value {
    Value::map([("id", Value::text(id))])
}

pub(crate) fn get<'a>(v: &'a Value, k: &str) -> &'a Value {
    match v {
        Value::Map(m) => m.get(k).unwrap_or_else(|| panic!("no {k} in {v:?}")),
        _ => panic!("not a map: {v:?}"),
    }
}

pub(crate) fn text(v: &Value) -> &str {
    match v {
        Value::Text(t) => t,
        other => panic!("not text: {other:?}"),
    }
}

pub(crate) fn list(v: &Value) -> &[Value] {
    match v {
        Value::List(l) => l,
        other => panic!("not a list: {other:?}"),
    }
}

pub(crate) fn err(r: Result<Response, CallError>) -> String {
    match r {
        Err(CallError::Handler(m)) => m,
        other => panic!("expected a handler error, got {other:?}"),
    }
}

pub(crate) fn rec(store: &Store, model: &str, id: &str, pairs: &[(&str, J)]) {
    store
        .backend()
        .insert(
            model,
            id,
            &pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        )
        .unwrap();
}

pub(crate) fn stored(store: &Store, model: &str, id: &str) -> serde_json::Map<String, J> {
    store.backend().get(model, id).unwrap().unwrap()
}

pub(crate) fn events(store: &Store) -> Vec<(String, String, String)> {
    store
        .backend()
        .list("security_event")
        .unwrap()
        .into_iter()
        .map(|(_, f)| {
            (
                f["kind"].as_str().unwrap().to_string(),
                f["actor"].as_str().unwrap().to_string(),
                f["detail"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

pub(crate) fn version_of(inst: &Installer, id: &str) -> String {
    inst.list()
        .into_iter()
        .find(|p| p.id == id)
        .unwrap()
        .version
}

#[test]
fn an_additive_update_keeps_every_record_and_is_audited() {
    let store = Store::new();
    let shelf = Shelf::default();
    let (d, inst) = setup(&store, &shelf);
    inst.install(&pkg("upd", "0.1.0", "", "", "", ""), "upload", "root")
        .unwrap();
    rec(&store, "upd_item", "1", &[("title", json!("first"))]);
    shelf.offer(pkg(
        "upd",
        "0.2.0",
        &field("note", ""),
        "",
        "Adds a note field.",
        "",
    ));
    // The list says an update is on offer, with its notes and that it fits.
    let rows = inst.module_rows();
    let row = list(&rows)
        .iter()
        .find(|r| text(get(r, "id")) == "upd")
        .unwrap();
    assert_eq!(text(get(row, "update_available")), "0.2.0");
    assert_eq!(text(get(row, "update_changelog")), "Adds a note field.");
    assert_eq!(get(row, "update_compatible"), &Value::Bool(true));
    assert_eq!(list(get(row, "update_changes")).len(), 1);
    let r = call(&d, &admin(), "module.upgrade", by_id("upd"))
        .unwrap()
        .value;
    assert_eq!(text(get(&r, "version")), "0.2.0");
    assert_eq!(version_of(&inst, "upd"), "0.2.0");
    assert_eq!(stored(&store, "upd_item", "1")["title"], "first");
    assert!(
        events(&store).iter().any(|(k, who, d)| k == "module_update"
            && who == "root"
            && d.contains("0.1.0 -> 0.2.0")),
        "{:?}",
        events(&store)
    );
    // Nothing newer is on offer now.
    assert!(err(call(&d, &admin(), "module.upgrade", by_id("upd"))).contains("up to date"));
    // And a package that is not installed, or a built-in module, says so.
    assert!(err(call(&d, &admin(), "module.upgrade", by_id("ghost"))).contains("not installed"));
    assert!(err(call(&d, &admin(), "module.upgrade", by_id("platform"))).contains("built-in"));
}

#[test]
fn a_breaking_update_without_a_step_is_refused_and_changes_nothing() {
    let store = Store::new();
    let shelf = Shelf::default();
    let (d, inst) = setup(&store, &shelf);
    inst.install(
        &pkg("upd", "0.1.0", &field("legacy", ""), "", "", ""),
        "upload",
        "root",
    )
    .unwrap();
    rec(
        &store,
        "upd_item",
        "1",
        &[("title", json!("a")), ("legacy", json!("x"))],
    );
    shelf.offer(pkg(
        "upd",
        "0.2.0",
        &field("code", "required = true"),
        "",
        "",
        "",
    ));
    let plan = call(&d, &admin(), "module_plan.preview", by_id("upd"))
        .unwrap()
        .value;
    assert_eq!(get(&plan, "ok"), &Value::Bool(false));
    let blockers = list(get(&plan, "blockers"));
    assert!(blockers.iter().all(|b| text(get(b, "kind")) == "data"));
    let all: Vec<&str> = blockers.iter().map(|b| text(get(b, "message"))).collect();
    assert!(
        all.iter()
            .any(|m| m.contains("field legacy is removed") && m.contains("drop_field")),
        "{all:?}"
    );
    assert!(
        all.iter()
            .any(|m| m.contains("no value for code") && m.contains("set_default")),
        "{all:?}"
    );
    let step = &list(get(&plan, "steps"))[0];
    let changes = list(get(step, "changes"));
    assert!(
        changes
            .iter()
            .any(|c| text(get(c, "kind")) == "field_removed")
    );
    assert!(err(call(&d, &admin(), "module.upgrade", by_id("upd"))).contains("legacy is removed"));
    assert_eq!(version_of(&inst, "upd"), "0.1.0");
    assert_eq!(stored(&store, "upd_item", "1")["legacy"], "x");
    // Retyping is refused whatever the author declares: there is no conversion.
    shelf.offer(pkg(
        "upd",
        "0.3.0",
        &field("legacy", "kind = \"number\""),
        "",
        "",
        "",
    ));
    assert!(
        err(call(&d, &admin(), "module.upgrade", by_id("upd"))).contains("cannot be converted")
    );
}

/// D72: a package update that changes the digits of a decimal must say how the stored values
/// are rewritten; otherwise old records keep an old spelling next to new ones.
#[test]
fn a_reinstall_over_the_records_of_an_earlier_install_must_find_them_in_the_new_shape() {
    let store = Store::new();
    let shelf = Shelf::default();
    let (d, inst) = setup(&store, &shelf);
    let dec = |scale: u8| {
        field(
            "price",
            &format!("kind = \"decimal\"\nprecision = 10\nscale = {scale}"),
        )
    };
    inst.install(
        &pkg("again", "0.1.0", &dec(2), "", "", ""),
        "upload",
        "root",
    )
    .unwrap();
    rec(
        &store,
        "again_item",
        "1",
        &[("title", json!("a")), ("price", json!("12.50"))],
    );
    // An uninstall keeps the data, and the old shape is gone with the package record.
    call(&d, &admin(), "module.uninstall", by_id("again")).unwrap();
    let e = inst
        .install(
            &pkg("again", "0.2.0", &dec(4), "", "", ""),
            "upload",
            "root",
        )
        .unwrap_err();
    assert!(
        e.contains("again_item 1") && e.contains("12.50") && e.contains("decimal(10, 4)"),
        "{e}"
    );
    assert!(e.contains("reformat_field"), "{e}");
    assert!(
        inst.list().iter().all(|p| p.id != "again"),
        "nothing was installed"
    );
    // The same shape comes back fine, and then the update with a step is the way to change it.
    inst.install(
        &pkg("again", "0.1.0", &dec(2), "", "", ""),
        "upload",
        "root",
    )
    .unwrap();
    shelf.offer(pkg(
        "again",
        "0.2.0",
        &dec(4),
        "[[migration]]\nfrom = \"<0.2.0\"\nop = \"reformat_field\"\nmodel = \"again_item\"\nfield = \"price\"\n",
        "",
        "",
    ));
    call(&d, &admin(), "module.upgrade", by_id("again")).unwrap();
    assert_eq!(stored(&store, "again_item", "1")["price"], "12.5000");
}

/// A count the store would not give is not "no records": the check of what is stored, and the
/// plan of an upgrade, must not be made with a zero that only means the store was busy.
#[test]
fn a_count_the_store_would_not_give_is_never_read_as_no_records() {
    use obop_storage::testing::{Flaky, Mode};
    let flaky = Flaky::new(obop_storage::open("memory:").unwrap());
    let store = Store::over(obop_storage::guarded(flaky.clone()));
    let shelf = Shelf::default();
    let (d, inst) = setup(&store, &shelf);
    let dec = |scale: u8| {
        field(
            "price",
            &format!("kind = \"decimal\"\nprecision = 10\nscale = {scale}"),
        )
    };
    inst.install(&pkg("cnt", "0.1.0", &dec(2), "", "", ""), "upload", "root")
        .unwrap();
    rec(
        &store,
        "cnt_item",
        "1",
        &[("title", json!("a")), ("price", json!("12.50"))],
    );
    // The check of stored shapes on a reinstall: refused, not skipped.
    let new = Manifest::parse(&pkg("cnt", "0.2.0", &dec(4), "", "", "")).unwrap();
    flaky.refuse_model("cnt_item", Mode::Reads);
    let e = crate::migrate::check_stored_shapes(&*store.backend(), &new).unwrap_err();
    assert!(e.contains("storage_busy"), "{e}");
    flaky.allow_models();
    let e = crate::migrate::check_stored_shapes(&*store.backend(), &new).unwrap_err();
    assert!(
        e.contains("12.50"),
        "healthy, the stored value is what is found: {e}"
    );
    // An upgrade that changes the digits of a decimal and says nothing about the stored values is
    // blocked when there are records. With the count refused it was not (no records, no blocker),
    // and the records stayed in the old spelling next to new ones.
    shelf.offer(pkg("cnt", "0.2.0", &dec(4), "", "", ""));
    flaky.refuse_model("cnt_item", Mode::Reads);
    let e = err(call(&d, &admin(), "module.upgrade", by_id("cnt")));
    flaky.allow_models();
    assert!(e.contains("could not be counted"), "{e}");
    assert_eq!(version_of(&inst, "cnt"), "0.1.0", "nothing was upgraded");
    let e = err(call(&d, &admin(), "module.upgrade", by_id("cnt")));
    assert!(
        e.contains("reformat_field"),
        "healthy: the usual blocker: {e}"
    );
    // With the step the upgrade does it.
    shelf.offer(pkg(
        "cnt",
        "0.2.0",
        &dec(4),
        "[[migration]]\nfrom = \"<0.2.0\"\nop = \"reformat_field\"\nmodel = \"cnt_item\"\nfield = \"price\"\n",
        "",
        "",
    ));
    call(&d, &admin(), "module.upgrade", by_id("cnt")).unwrap();
    assert_eq!(stored(&store, "cnt_item", "1")["price"], "12.5000");
}

#[test]
fn the_settings_a_change_is_compared_with_are_never_nothing_because_the_store_would_not_give_them()
{
    use obop_storage::testing::{Flaky, Mode};
    let flaky = Flaky::new(obop_storage::open("memory:").unwrap());
    let store = Store::over(obop_storage::guarded(flaky.clone()));
    let (_d, inst) = setup(&store, &Shelf::default());
    inst.set_settings(r#"{"security": {"require_mfa_for_all": true}}"#)
        .unwrap();
    // The audit line of a security change says old and new: with the old one unreadable the change
    // is refused, not recorded as a change from nothing.
    flaky.refuse_model("obop_setting", Mode::Reads);
    let e = inst
        .set_settings(r#"{"security": {"require_mfa_for_all": false}}"#)
        .unwrap_err();
    flaky.allow_models();
    assert!(e.starts_with("storage_busy"), "{e}");
    assert!(inst.settings().contains("true"), "unchanged");
    assert_eq!(inst.settings(), inst.try_settings().unwrap());
}

#[test]
fn changing_the_digits_of_a_decimal_needs_a_rewrite_step_and_the_step_does_it() {
    let store = Store::new();
    let shelf = Shelf::default();
    let (d, inst) = setup(&store, &shelf);
    let dec = |scale: u8, extra: &str| {
        field(
            "price",
            &format!(
                "kind = \"decimal\"
precision = 10
scale = {scale}
unique = true
{extra}"
            ),
        )
    };
    inst.install(
        &pkg("upd", "0.1.0", &dec(2, ""), "", "", ""),
        "upload",
        "root",
    )
    .unwrap();
    rec(
        &store,
        "upd_item",
        "1",
        &[("title", json!("a")), ("price", json!("12.50"))],
    );
    rec(
        &store,
        "upd_item",
        "2",
        &[("title", json!("b")), ("price", json!("3.00"))],
    );
    // Without a step: refused in the plan and by the upgrade, nothing changes.
    shelf.offer(pkg("upd", "0.2.0", &dec(4, ""), "", "", ""));
    let plan = call(&d, &admin(), "module_plan.preview", by_id("upd"))
        .unwrap()
        .value;
    assert_eq!(get(&plan, "ok"), &Value::Bool(false));
    let all: Vec<&str> = list(get(&plan, "blockers"))
        .iter()
        .map(|b| text(get(b, "message")))
        .collect();
    assert!(
        all.iter().any(|m| m.contains("decimal(10, 2) to decimal(10, 4)") && m.contains("reformat_field")),
        "{all:?}"
    );
    assert!(err(call(&d, &admin(), "module.upgrade", by_id("upd"))).contains("reformat_field"));
    assert_eq!(version_of(&inst, "upd"), "0.1.0");
    assert_eq!(stored(&store, "upd_item", "1")["price"], "12.50");
    // With the step: every stored value is rewritten in the new shape, in one batch.
    shelf.offer(pkg(
        "upd",
        "0.3.0",
        &dec(4, ""),
        "[[migration]]
from = \"<0.3.0\"
op = \"reformat_field\"
model = \"upd_item\"
field = \"price\"
",
        "",
        "",
    ));
    let plan = call(&d, &admin(), "module_plan.preview", by_id("upd"))
        .unwrap()
        .value;
    assert_eq!(get(&plan, "ok"), &Value::Bool(true), "{plan:?}");
    call(&d, &admin(), "module.upgrade", by_id("upd")).unwrap();
    assert_eq!(stored(&store, "upd_item", "1")["price"], "12.5000");
    assert_eq!(stored(&store, "upd_item", "2")["price"], "3.0000");
    // One spelling per value again: the command layer takes the same number and a duplicate
    // is a duplicate.
    let made = call(
        &d,
        &admin(),
        "upd_item.create",
        Value::map([("title", Value::text("c")), ("price", Value::text("7.5"))]),
    )
    .unwrap()
    .value;
    assert_eq!(text(get(&made, "price")), "7.5000");
    assert!(
        err(call(
            &d,
            &admin(),
            "upd_item.create",
            Value::map([
                ("title", Value::text("dup")),
                ("price", Value::text("12.5"))
            ]),
        ))
        .contains("must be unique")
    );
    // An aggregate over values that predate a narrowing is an error, not a quietly shortened sum.
    store
        .backend()
        .update(
            "upd_item",
            "2",
            &[("price".to_string(), json!("3.123456"))]
                .into_iter()
                .collect(),
        )
        .unwrap();
    let e = err(call(
        &d,
        &admin(),
        "upd_item.search",
        Value::map([(
            "aggregate",
            Value::map([(
                "measures",
                Value::List(vec![Value::map([
                    ("op", Value::text("sum")),
                    ("field", Value::text("price")),
                ])]),
            )]),
        )]),
    ));
    assert!(e.contains("more than the 4 fractional digits"), "{e}");
}

#[test]
fn rename_and_set_default_run_in_one_batch_and_rename_rolls_back() {
    let store = Store::new();
    let shelf = Shelf::default();
    let (d, inst) = setup(&store, &shelf);
    inst.install(
        &pkg("upd", "0.1.0", &field("plate", ""), "", "", ""),
        "upload",
        "root",
    )
    .unwrap();
    rec(
        &store,
        "upd_item",
        "1",
        &[("title", json!("a")), ("plate", json!("AB-1"))],
    );
    rec(
        &store,
        "upd_item",
        "2",
        &[("title", json!("b")), ("plate", json!("CD-2"))],
    );
    rec(&store, "upd_item", "3", &[("title", json!("c"))]);
    shelf.offer(pkg(
        "upd",
        "0.2.0",
        &format!("{}{}", field("registration", ""), field("tier", "required = true")),
        "[[migration]]\nfrom = \"<0.2.0\"\nop = \"rename_field\"\nmodel = \"upd_item\"\nfield = \"plate\"\nto = \"registration\"\n[[migration]]\nfrom = \"<0.2.0\"\nop = \"set_default\"\nmodel = \"upd_item\"\nfield = \"tier\"\nvalue = \"standard\"\n",
        "Plate becomes registration.",
        "",
    ));
    let preview = call(&d, &admin(), "module_plan.preview", by_id("upd"))
        .unwrap()
        .value;
    assert_eq!(get(&preview, "ok"), &Value::Bool(true));
    let step = &list(get(&preview, "steps"))[0];
    assert_eq!(list(get(step, "migrations")).len(), 2);
    call(&d, &admin(), "module.upgrade", by_id("upd")).unwrap();
    let a = stored(&store, "upd_item", "1");
    assert_eq!(a["registration"], "AB-1");
    assert!(a["plate"].is_null());
    assert_eq!(a["tier"], "standard");
    assert_eq!(stored(&store, "upd_item", "3")["tier"], "standard");
    assert!(!stored(&store, "upd_item", "3").contains_key("registration"));
    assert!(
        events(&store)
            .iter()
            .any(|(k, _, d)| k == "module_update"
                && d.contains("rename upd_item.plate to registration")),
        "the audit entry names the data steps"
    );
    // The row offers the way back, and taking it reverses the rename.
    let rows = inst.module_rows();
    let row = list(&rows)
        .iter()
        .find(|r| text(get(r, "id")) == "upd")
        .unwrap();
    assert_eq!(get(row, "rollback_available"), &Value::Bool(true));
    assert_eq!(text(get(row, "rollback_version")), "0.1.0");
    let back = call(&d, &admin(), "module.rollback", by_id("upd"))
        .unwrap()
        .value;
    assert_eq!(text(get(&back, "version")), "0.1.0");
    let a = stored(&store, "upd_item", "1");
    assert_eq!(a["plate"], "AB-1");
    assert!(a["registration"].is_null());
    assert_eq!(version_of(&inst, "upd"), "0.1.0");
    assert!(events(&store).iter().any(|(k, ..)| k == "module_rollback"));
    // One level only: nothing further to go back to, and updating again works.
    assert!(
        err(call(&d, &admin(), "module.rollback", by_id("upd"))).contains("no earlier version")
    );
    call(&d, &admin(), "module.upgrade", by_id("upd")).unwrap();
    assert_eq!(stored(&store, "upd_item", "2")["registration"], "CD-2");
}

#[test]
fn dropping_a_field_needs_a_declared_step_and_cannot_be_rolled_back() {
    let store = Store::new();
    let shelf = Shelf::default();
    let (d, inst) = setup(&store, &shelf);
    inst.install(
        &pkg("upd", "0.1.0", &field("legacy", ""), "", "", ""),
        "upload",
        "root",
    )
    .unwrap();
    rec(
        &store,
        "upd_item",
        "1",
        &[("title", json!("a")), ("legacy", json!("keep me?"))],
    );
    shelf.offer(pkg(
        "upd",
        "0.2.0",
        "",
        "[[migration]]\nfrom = \"<0.2.0\"\nop = \"drop_field\"\nmodel = \"upd_item\"\nfield = \"legacy\"\nconfirm = true\n",
        "",
        "",
    ));
    let plan = call(&d, &admin(), "module_plan.preview", by_id("upd"))
        .unwrap()
        .value;
    let warnings: Vec<&str> = list(get(&plan, "warnings")).iter().map(text).collect();
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("deletes the values of upd_item.legacy")),
        "{warnings:?}"
    );
    call(&d, &admin(), "module.upgrade", by_id("upd")).unwrap();
    assert!(stored(&store, "upd_item", "1")["legacy"].is_null());
    let rows = inst.module_rows();
    let row = list(&rows)
        .iter()
        .find(|r| text(get(r, "id")) == "upd")
        .unwrap();
    assert_eq!(get(row, "rollback_available"), &Value::Bool(false));
    let e = err(call(&d, &admin(), "module.rollback", by_id("upd")));
    assert!(
        e.contains("deleted data") && e.contains("restore the backup"),
        "{e}"
    );
    assert_eq!(version_of(&inst, "upd"), "0.2.0");
}

#[test]
fn a_failing_step_in_a_plan_leaves_the_data_and_the_version_alone() {
    let store = Store::new();
    let shelf = Shelf::default();
    let (d, inst) = setup(&store, &shelf);
    inst.install(
        &pkg("upd", "0.1.0", &field("plate", ""), "", "", ""),
        "upload",
        "root",
    )
    .unwrap();
    rec(
        &store,
        "upd_item",
        "1",
        &[("title", json!("a")), ("plate", json!("AB-1"))],
    );
    shelf.offer(pkg(
        "upd",
        "0.2.0",
        &field("registration", ""),
        "[[migration]]\nfrom = \"<0.2.0\"\nop = \"rename_field\"\nmodel = \"upd_item\"\nfield = \"plate\"\nto = \"registration\"\n",
        "",
        "",
    ));
    let (mut plan, _) = inst.plan(Request::Market("upd")).unwrap();
    assert!(plan.ok());
    // A second step, planned, that fails when it is applied: its model name is taken by a model
    // registered outside the installer.
    let other = Manifest::parse(&pkg("other", "0.1.0", "", "", "", "")).unwrap();
    let mut step = plan.steps[0].clone();
    step.id = "other".into();
    step.name = "other".into();
    step.version = "0.1.0".into();
    step.action = crate::resolve::StepAction::Install;
    step.from_version = None;
    step.requested = false;
    step.manifest = other.clone();
    plan.steps.push(step);
    let squatter = other.to_module().unwrap();
    d.register_crud(&squatter, &store);
    let e = inst
        .apply_plan(&plan, "upd", "marketplace", "root")
        .unwrap_err();
    assert!(
        e.contains("model other_item already exists") && e.contains("rolled back"),
        "{e}"
    );
    assert_eq!(
        version_of(&inst, "upd"),
        "0.1.0",
        "the first step was undone"
    );
    let a = stored(&store, "upd_item", "1");
    assert_eq!(a["plate"], "AB-1", "the rename never happened");
    assert!(!a.contains_key("registration"));
    assert!(d.has_model("upd_item"));
    assert!(!events(&store).iter().any(|(k, ..)| k == "module_update"));
}

#[test]
fn two_processes_updating_at_once_apply_the_update_exactly_once() {
    let store = Store::new();
    let shelf = Shelf::default();
    let (_d1, one) = setup(&store, &shelf);
    let (_d2, two) = setup(&store, &shelf);
    one.install(
        &pkg("upd", "0.1.0", &field("plate", ""), "", "", ""),
        "upload",
        "root",
    )
    .unwrap();
    for n in 0..30 {
        rec(
            &store,
            "upd_item",
            &n.to_string(),
            &[("title", json!("t")), ("plate", json!(format!("P-{n}")))],
        );
    }
    shelf.offer(pkg(
        "upd",
        "0.2.0",
        &field("registration", ""),
        "[[migration]]\nfrom = \"<0.2.0\"\nop = \"rename_field\"\nmodel = \"upd_item\"\nfield = \"plate\"\nto = \"registration\"\n",
        "",
        "",
    ));
    let results: Vec<Result<_, String>> = std::thread::scope(|s| {
        let a = s.spawn(|| one.update_outcome("upd", "alice"));
        let b = s.spawn(|| two.update_outcome("upd", "bob"));
        vec![a.join().unwrap(), b.join().unwrap()]
    });
    assert_eq!(
        results.iter().filter(|r| r.is_ok()).count(),
        1,
        "{:?}",
        results.iter().map(|r| r.as_ref().err()).collect::<Vec<_>>()
    );
    let loser = results.iter().find_map(|r| r.as_ref().err()).unwrap();
    assert!(
        loser.contains("up to date") || loser.contains("not an upgrade"),
        "{loser}"
    );
    for n in 0..30 {
        let r = stored(&store, "upd_item", &n.to_string());
        assert_eq!(r["registration"], format!("P-{n}"), "record {n}");
        assert!(r["plate"].is_null());
    }
    assert_eq!(
        events(&store)
            .iter()
            .filter(|(k, ..)| k == "module_update")
            .count(),
        1,
        "audited once"
    );
}

#[test]
fn agents_only_propose_updates_rollbacks_and_update_all_and_non_admins_cannot_see_them() {
    let store = Store::new();
    let shelf = Shelf::default();
    let (d, inst) = setup(&store, &shelf);
    inst.install(&pkg("upd", "0.1.0", "", "", "", ""), "upload", "root")
        .unwrap();
    shelf.offer(pkg("upd", "0.2.0", "", "", "", ""));
    let agent = Principal::agent("bot", &[ADMIN_GROUP], &[ADMIN_GROUP]);
    for (cmd, input) in [
        ("module.upgrade", by_id("upd")),
        ("module.upgrade_all", Value::Map(Default::default())),
        ("module.rollback", by_id("upd")),
    ] {
        assert!(
            matches!(
                call(&d, &agent, cmd, input),
                Err(CallError::ApprovalRequired { .. })
            ),
            "{cmd} needs an approval"
        );
    }
    assert_eq!(
        version_of(&inst, "upd"),
        "0.1.0",
        "proposing changed nothing"
    );
    // The agent proposes an update; an administrator approves it, once.
    let Err(CallError::ApprovalRequired { plan_id }) =
        call(&d, &agent, "module.upgrade", by_id("upd"))
    else {
        panic!()
    };
    d.approve(&admin(), &mut Vault::new(), &plan_id).unwrap();
    assert_eq!(version_of(&inst, "upd"), "0.2.0");
    assert!(matches!(
        d.approve(&admin(), &mut Vault::new(), &plan_id),
        Err(CallError::UnknownPlan(_))
    ));
    // Someone without the admin group is told nothing and can do nothing.
    let sam = Principal::user("sam", &["base.user"]);
    for cmd in ["module.upgrade", "module.upgrade_all", "module.rollback"] {
        assert!(call(&d, &sam, cmd, by_id("upd")).is_err(), "{cmd}");
    }
}

#[test]
fn update_all_takes_what_fits_and_reports_what_does_not() {
    let store = Store::new();
    let shelf = Shelf::default();
    let (d, inst) = setup(&store, &shelf);
    for id in ["aa", "bb", "cc"] {
        inst.install(
            &pkg(id, "0.1.0", &field("old", ""), "", "", ""),
            "upload",
            "root",
        )
        .unwrap();
    }
    rec(
        &store,
        "bb_item",
        "1",
        &[("title", json!("x")), ("old", json!("v"))],
    );
    shelf.offer(pkg("aa", "0.2.0", "", "", "", "")); // drops a field with records? none stored: fine
    shelf.offer(pkg("bb", "0.2.0", "", "", "", "")); // has a record with the removed field: blocked
    // cc is already current.
    let r = call(
        &d,
        &admin(),
        "module.upgrade_all",
        Value::Map(Default::default()),
    )
    .unwrap()
    .value;
    let updated: Vec<&str> = list(get(&r, "updated"))
        .iter()
        .map(|u| text(get(u, "id")))
        .collect();
    assert_eq!(updated, ["aa"]);
    let skipped = list(get(&r, "skipped"));
    assert_eq!(skipped.len(), 1);
    assert_eq!(text(get(&skipped[0], "id")), "bb");
    assert!(text(get(&skipped[0], "reason")).contains("old is removed"));
    assert_eq!(version_of(&inst, "aa"), "0.2.0");
    assert_eq!(version_of(&inst, "bb"), "0.1.0");
    assert_eq!(version_of(&inst, "cc"), "0.1.0");
    let ups = inst.updates();
    assert_eq!(ups.len(), 1);
    assert!(!ups[0].ok && ups[0].blockers[0].contains("old is removed"));
}

#[test]
fn dependents_are_protected_in_both_directions_and_the_plan_shows_it() {
    let store = Store::new();
    let shelf = Shelf::default();
    let (d, inst) = setup(&store, &shelf);
    inst.install(&pkg("base1", "1.0.0", "", "", "", ""), "upload", "root")
        .unwrap();
    let needs = pkg(
        "dep1",
        "0.1.0",
        "",
        "",
        "",
        "[module.dependencies]\nbase1 = \">=1.0, <2\"\n",
    );
    inst.install(&needs, "upload", "root").unwrap();
    // 1: an update of the dependency that leaves the dependent's range is blocked, and the
    // preview names the dependent and its range.
    shelf.offer(pkg("base1", "2.0.0", "", "", "", ""));
    let plan = call(&d, &admin(), "module_plan.preview", by_id("base1"))
        .unwrap()
        .value;
    assert_eq!(get(&plan, "ok"), &Value::Bool(false));
    let step = &list(get(&plan, "steps"))[0];
    let dep = &list(get(step, "dependents"))[0];
    assert_eq!(text(get(dep, "id")), "dep1");
    assert_eq!(text(get(dep, "requirement")), ">=1.0, <2");
    assert_eq!(get(dep, "satisfied"), &Value::Bool(false));
    let e = err(call(&d, &admin(), "module.upgrade", by_id("base1")));
    assert!(
        e.contains("would break dep1") && e.contains(">=1.0, <2"),
        "{e}"
    );
    // 2: a version inside the range is fine.
    shelf.offer(pkg("base1", "1.5.0", "", "", "", ""));
    call(&d, &admin(), "module.upgrade", by_id("base1")).unwrap();
    // 3: the dependent's update must itself fit what is installed.
    shelf.offer(pkg(
        "dep1",
        "0.2.0",
        "",
        "",
        "",
        "[module.dependencies]\nbase1 = \">=3, <4\"\n",
    ));
    let e = err(call(&d, &admin(), "module.upgrade", by_id("dep1")));
    assert!(e.contains("base1"), "{e}");
    // 4: rolling a dependency back is refused when it would leave a dependent's range, and the
    // module row says why.
    shelf.offer(pkg("base1", "1.9.0", "", "", "", ""));
    call(&d, &admin(), "module.upgrade", by_id("base1")).unwrap();
    inst.uninstall("dep1").unwrap();
    inst.install(
        &pkg(
            "dep1",
            "0.1.0",
            "",
            "",
            "",
            "[module.dependencies]\nbase1 = \">=1.8, <2\"\n",
        ),
        "upload",
        "root",
    )
    .unwrap();
    let e = err(call(&d, &admin(), "module.rollback", by_id("base1")));
    assert!(
        e.contains("would break dep1") && e.contains(">=1.8, <2"),
        "{e}"
    );
    // 5: uninstalling a dependency is still refused while a dependent needs it.
    assert!(
        inst.uninstall("base1")
            .unwrap_err()
            .contains("required by dep1")
    );
}

#[test]
fn a_core_update_checks_every_installed_module_against_the_new_core() {
    let store = Store::new();
    let shelf = Shelf::default();
    let (_d, inst) = setup(&store, &shelf);
    inst.install(&pkg("upd", "0.1.0", "", "", "", ""), "upload", "root")
        .unwrap();
    let narrow = pkg("pin", "0.1.0", "", "", "", "").replace("^0.2", ">=0.2, <0.3");
    inst.install(&narrow, "upload", "root").unwrap();
    let on = |v: &str| {
        inst.core_compat(&crate::semver::Version::parse(v).unwrap())
            .into_iter()
            .map(|c| (c.id, c.compatible))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        on("0.2.9"),
        [("pin".to_string(), true), ("upd".to_string(), true)]
    );
    assert_eq!(
        on("0.3.0"),
        [("pin".to_string(), false), ("upd".to_string(), false)]
    );
    let one = inst
        .core_compat(&crate::semver::Version::parse("0.3.0").unwrap())
        .remove(0);
    assert!(
        one.reasons[0].contains("supports core"),
        "{:?}",
        one.reasons
    );
}

#[test]
fn prereleases_wildcards_and_empty_ranges_in_dependencies() {
    use crate::semver::{Req, Version};
    let v = |s: &str| Version::parse(s).unwrap();
    // A release range does not match a pre-release of the next version unless it names one.
    let r = Req::parse(">=1.0.0, <2").unwrap();
    assert!(!r.matches(&v("1.5.0-beta.1")));
    assert!(r.matches(&v("1.5.0")));
    let r = Req::parse(">=1.5.0-beta.1, <2").unwrap();
    assert!(r.matches(&v("1.5.0-beta.2")));
    assert!(!r.matches(&v("1.6.0-beta.1")));
    // The wildcard is allowed to parse and is reported as unbounded; the empty range is refused.
    assert!(
        Req::parse("*").unwrap().matches(&v("9.9.9")) && !Req::parse("*").unwrap().is_bounded()
    );
    assert!(Req::parse("").is_err());
    assert!(!Req::parse(">=2, <1").unwrap().is_satisfiable());
    // A manifest that declares a dependency with an empty or impossible range is refused with
    // the dependency named.
    let m = |range: &str| {
        Manifest::parse(&pkg(
            "x1",
            "0.1.0",
            "",
            "",
            "",
            &format!("[module.dependencies]\nbase = \"{range}\"\n"),
        ))
    };
    assert!(
        m("")
            .unwrap_err()
            .contains("base: the version range is empty")
    );
    assert!(m(">=2, <1").unwrap_err().contains("matches no version"));
    assert!(m("^0.2").is_ok());
}
