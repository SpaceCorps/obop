//! The import map over REST, and through a backup and a restore: `bulk_create` with a `source`
//! writes the key of every created record, `import_map.search` and `import_map.get` read it, a
//! re-run reports `already_imported`, and the entries are part of a backup.

use crate::app::App;
use crate::mfa_tests::{PW, post, rig, token};
use obop_commands::{Principal, Store, Surface, Value, Vault};
use obop_ops::backup::{RestoreOptions, create, restore};
use obop_storage::Backend;
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn rest_a_source_key_is_recorded_found_and_not_imported_twice() {
    let rig = rig(false);
    let r = &rig.r;
    let g = |gs: &[&str]| gs.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    rig.auth
        .create_user("imp", PW, &g(&["base.user", "api.import_map_read"]))
        .unwrap();
    let imp = token(r, "imp").await;
    let ada = token(r, "ada").await; // base.user only

    let call = json!({
        "rows": [{"name": "Ada"}, {"name": "Bob"}],
        "source": {"system": "legacy", "table": "customers", "ids": ["A-1", "B-2"]},
    });
    let (s, b) = post(r, Some(&imp), "/api/party/bulk_create", call.clone()).await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(b["value"]["created"], 2);
    let ids = b["value"]["ids"].clone();

    // The key to the record.
    let q = json!({"source_system": "legacy", "source_table": "customers", "source_id": "B-2"});
    let (s, b) = post(r, Some(&imp), "/api/import_map/search", q.clone()).await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(b["value"]["total"], 1);
    assert_eq!(b["value"]["rows"][0]["record"], ids[1]);
    assert_eq!(b["value"]["rows"][0]["model"], "party");

    // The record to its keys.
    let (s, b) = post(
        r,
        Some(&imp),
        "/api/import_map/get",
        json!({"model": "party", "id": ids[0]}),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(b["value"][0]["source_id"], "A-1");

    // Without the group the commands are not there.
    let (s, _) = post(r, Some(&ada), "/api/import_map/search", q).await;
    assert_eq!(s, 404);

    // The same call again: nothing is created, the ids are the first ones, the answer says so.
    let (s, b) = post(r, Some(&imp), "/api/party/bulk_create", call.clone()).await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(
        (
            b["value"]["created"].clone(),
            b["value"]["already_imported"].clone(),
            b["value"]["ids"].clone()
        ),
        (json!(0), json!(2), ids)
    );
    // With `on_existing: error` it is a 400 whose text carries the code.
    let mut strict = call;
    strict["on_existing"] = json!("error");
    let (s, b) = post(r, Some(&imp), "/api/party/bulk_create", strict).await;
    assert_eq!(s, 400, "{b}");
    assert!(b["error"].as_str().unwrap().contains("already_imported"));
}

fn open(url: &str) -> App {
    App::with_store(Store::open(url).unwrap())
}

fn call(app: &App, id: &str, input: serde_json::Value) -> serde_json::Value {
    let root = Principal::user("root", &["platform.admin"]);
    app.dispatcher
        .call(
            Surface::Rest,
            &root,
            &mut Vault::new(),
            id,
            &Value::from_json(&input),
        )
        .unwrap_or_else(|e| panic!("{id}: {e:?}"))
        .value
        .to_json()
}

fn entries(b: &Arc<dyn Backend>) -> Vec<(String, serde_json::Value)> {
    let mut v: Vec<_> = b
        .list("import_map")
        .unwrap()
        .into_iter()
        .map(|(id, f)| (id, serde_json::Value::Object(f)))
        .collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

/// The entries are in a backup and come back with it, on SQLite and in memory, and the restored
/// database answers the lookups and refuses to import the same keys again.
#[test]
fn the_import_map_is_part_of_a_backup_and_a_restore() {
    // A directory of this run's own, removed when the test ends (after the stores are closed).
    let _guard = obop_storage::testing::TempPath::dir("obop-importmap");
    let dir = _guard.path().to_path_buf();
    let url = |n: &str| format!("sqlite://{}", dir.join(n).display());

    let src = open(&url("src.db"));
    let made = call(
        &src,
        "party.bulk_create",
        json!({
            "rows": [{"name": "Ada"}, {"name": "Bob"}, {"name": "Cy"}],
            "source": {"system": "legacy", "table": "customers", "ids": ["A-1", "B-2", "C-3"]},
        }),
    );
    assert_eq!(made["created"], 3, "{made}");
    let ids = made["ids"].clone();
    let before = entries(&src.store.backend());
    assert_eq!(before.len(), 3);

    let out = dir.join("b.jsonl.gz");
    let created = create(&src.store.backend(), 2, &out, &mut |_| {}).unwrap();
    assert!(
        created.manifest.models["import_map"] == 3,
        "{:?}",
        created.manifest.models
    );

    // SQLite file, then memory.
    let dst_url = url("dst.db");
    let dst = obop_storage::open(&dst_url).unwrap();
    restore(
        &dst,
        2,
        &created.data_path,
        RestoreOptions::default(),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(entries(&dst), before);
    let mem = obop_storage::open("memory:").unwrap();
    restore(
        &mem,
        2,
        &created.data_path,
        RestoreOptions::default(),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(entries(&mem), before);
    drop(dst);

    // The restored instance finds the keys and does not import them again.
    let back = open(&dst_url);
    let found = call(
        &back,
        "import_map.search",
        json!({"source_system": "legacy", "source_table": "customers", "source_id": "C-3"}),
    );
    assert_eq!(found["rows"][0]["record"], ids[2]);
    let again = call(
        &back,
        "party.bulk_create",
        json!({
            "rows": [{"name": "Ada"}, {"name": "Dee"}],
            "source": {"system": "legacy", "table": "customers", "ids": ["A-1", "D-4"]},
        }),
    );
    assert_eq!(
        (
            again["created"].clone(),
            again["already_imported"].clone(),
            again["ids"][0].clone()
        ),
        (json!(1), json!(1), ids[0].clone()),
        "{again}"
    );
    drop(back);
    drop(src);
}
