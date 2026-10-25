//! The migration ledger and the backup gate around `module.upgrade` and `module.rollback`
//! (OBR-DAT-051, OBR-DAT-053, D73).

use crate::installer::{GateReceipt, GateRequest, MigrationGate};
use crate::update_tests::{
    Shelf, admin, by_id, call, err, events, field, pkg, rec, setup, stored, version_of,
};
use obop_commands::Store;
use obop_migrate::{Entry, EntryStatus, ledger};
use serde_json::json;
use std::sync::{Arc, Mutex};

fn rows(store: &Store) -> Vec<Entry> {
    ledger::read_all(&*store.backend()).unwrap()
}

/// A gate that records what it was asked and answers as told.
#[derive(Default)]
struct TestGate {
    asked: Mutex<Vec<GateRequest>>,
    refuse: Mutex<Option<String>>,
}

impl MigrationGate for TestGate {
    fn before(&self, what: &GateRequest) -> Result<GateReceipt, String> {
        self.asked.lock().unwrap().push(what.clone());
        match self.refuse.lock().unwrap().clone() {
            Some(why) => Err(why),
            None => Ok(GateReceipt {
                backup_id: Some("obop-backup-gate.jsonl.gz".into()),
            }),
        }
    }
}

fn renaming_shelf(shelf: &Shelf) {
    shelf.offer(pkg(
        "upd",
        "0.2.0",
        &format!("{}{}", field("registration", ""), field("tier", "required = true")),
        "[[migration]]\nfrom = \"<0.2.0\"\nop = \"rename_field\"\nmodel = \"upd_item\"\nfield = \"plate\"\nto = \"registration\"\n[[migration]]\nfrom = \"<0.2.0\"\nop = \"set_default\"\nmodel = \"upd_item\"\nfield = \"tier\"\nvalue = \"standard\"\n",
        "Plate becomes registration.",
        "",
    ));
}

fn installed_with_a_record(
    store: &Store,
    shelf: &Shelf,
) -> (Arc<obop_commands::Dispatcher>, Arc<crate::Installer>) {
    let (d, inst) = setup(store, shelf);
    inst.install(
        &pkg("upd", "0.1.0", &field("plate", ""), "", "", ""),
        "upload",
        "root",
    )
    .unwrap();
    rec(
        store,
        "upd_item",
        "1",
        &[("title", json!("a")), ("plate", json!("AB-1"))],
    );
    (d, inst)
}

#[test]
fn an_upgrade_with_data_steps_writes_one_ledger_row_per_step_in_the_same_batch_and_so_does_the_rollback()
 {
    let store = Store::new();
    let shelf = Shelf::default();
    let (d, inst) = installed_with_a_record(&store, &shelf);
    renaming_shelf(&shelf);
    assert!(rows(&store).is_empty(), "nothing before the upgrade");
    call(&d, &admin(), "module.upgrade", by_id("upd")).unwrap();
    let all = rows(&store);
    assert_eq!(all.len(), 2, "{all:?}");
    for (i, r) in all.iter().enumerate() {
        assert_eq!(
            (r.module.as_str(), r.kind.as_str(), r.by.as_str()),
            ("upd", "package", "root")
        );
        assert_eq!(r.step, format!("0.1.0->0.2.0#{}", i + 1));
        assert_eq!(r.status, EntryStatus::Applied);
        assert_eq!(r.checksum.len(), 64);
        assert!(r.id.starts_with("pkg/upd/0.1.0-0.2.0/"), "{}", r.id);
        assert_eq!(r.backup_id, None);
    }
    assert!(
        all[0]
            .name
            .contains("rename upd_item.plate to registration")
    );
    assert_eq!(all[0].class, ledger::Class::Backfill);
    assert_ne!(all[0].checksum, all[1].checksum);
    // The way back is a package step too, marked as a rollback.
    call(&d, &admin(), "module.rollback", by_id("upd")).unwrap();
    let all = rows(&store);
    assert_eq!(all.len(), 3, "the inverse of the rename: {all:?}");
    let back = all.last().unwrap();
    assert!(back.id.contains("0.2.0-0.1.0/rollback-"), "{}", back.id);
    assert!(back.note.contains("rollback"));
    assert_eq!(version_of(&inst, "upd"), "0.1.0");
}

#[test]
fn an_additive_upgrade_has_no_data_steps_and_so_no_rows() {
    let store = Store::new();
    let shelf = Shelf::default();
    let (d, _inst) = setup(&store, &shelf);
    _inst
        .install(&pkg("upd", "0.1.0", "", "", "", ""), "upload", "root")
        .unwrap();
    shelf.offer(pkg("upd", "0.2.0", &field("note", ""), "", "", ""));
    call(&d, &admin(), "module.upgrade", by_id("upd")).unwrap();
    assert!(rows(&store).is_empty());
}

#[test]
fn the_gate_runs_only_before_updates_that_rewrite_records_and_its_backup_lands_in_the_ledger() {
    let store = Store::new();
    let shelf = Shelf::default();
    let (d, inst) = installed_with_a_record(&store, &shelf);
    let gate = Arc::new(TestGate::default());
    inst.set_gate(gate.clone());
    // An additive update does not ask the gate.
    shelf.offer(pkg("upd", "0.1.1", &field("plate", ""), "", "", ""));
    call(&d, &admin(), "module.upgrade", by_id("upd")).unwrap();
    assert!(gate.asked.lock().unwrap().is_empty());
    // An update with data steps does, once, with the steps in words.
    renaming_shelf(&shelf);
    call(&d, &admin(), "module.upgrade", by_id("upd")).unwrap();
    {
        let asked = gate.asked.lock().unwrap();
        assert_eq!(asked.len(), 1);
        assert_eq!(asked[0].command, "module.upgrade");
        assert_eq!(asked[0].by, "root");
        assert_eq!(asked[0].modules, ["upd 0.1.1->0.2.0"]);
        assert!(
            asked[0]
                .steps
                .iter()
                .any(|s| s.contains("rename upd_item.plate")),
            "{:?}",
            asked[0].steps
        );
    }
    let all = rows(&store);
    assert_eq!(all.len(), 2);
    assert!(
        all.iter()
            .all(|r| r.backup_id.as_deref() == Some("obop-backup-gate.jsonl.gz"))
    );
    assert!(
        events(&store)
            .iter()
            .any(|(k, _, d)| k == "module_update" && d.contains("backup obop-backup-gate")),
        "the audit entry names the backup: {:?}",
        events(&store)
    );
    // The rollback of a rename rewrites records, so it asks too.
    call(&d, &admin(), "module.rollback", by_id("upd")).unwrap();
    let asked = gate.asked.lock().unwrap();
    assert_eq!(asked.len(), 2);
    assert_eq!(asked[1].command, "module.rollback");
}

#[test]
fn a_gate_that_refuses_stops_the_upgrade_and_the_rollback_with_nothing_changed() {
    let store = Store::new();
    let shelf = Shelf::default();
    let (d, inst) = installed_with_a_record(&store, &shelf);
    let gate = Arc::new(TestGate::default());
    inst.set_gate(gate.clone());
    renaming_shelf(&shelf);
    *gate.refuse.lock().unwrap() = Some("no verified backup could be taken".into());
    let e = err(call(&d, &admin(), "module.upgrade", by_id("upd")));
    assert!(
        e.contains("no verified backup could be taken") && e.contains("not changed"),
        "{e}"
    );
    assert_eq!(version_of(&inst, "upd"), "0.1.0");
    assert_eq!(stored(&store, "upd_item", "1")["plate"], "AB-1");
    assert!(rows(&store).is_empty());
    // Let it through once, then refuse the rollback.
    *gate.refuse.lock().unwrap() = None;
    call(&d, &admin(), "module.upgrade", by_id("upd")).unwrap();
    assert_eq!(stored(&store, "upd_item", "1")["registration"], "AB-1");
    *gate.refuse.lock().unwrap() = Some("disk is full".into());
    let e = err(call(&d, &admin(), "module.rollback", by_id("upd")));
    assert!(e.contains("disk is full"), "{e}");
    assert_eq!(version_of(&inst, "upd"), "0.2.0");
    assert_eq!(stored(&store, "upd_item", "1")["registration"], "AB-1");
}
