//! The backup that comes before a data migration is not made on a guess (D74): a schema version
//! the store would not give stops it, on the module gate and in the scheduled backup.

use crate::backup_cli::{GateSettings, ModuleGate, schema_for_backup};
use obop_ops::backup::Keys;
use obop_pkg::{GateRequest, MigrationGate};
use obop_storage::blob::{Catalog, open_bytes};
use obop_storage::testing::{Flaky, Mode};
use obop_storage::{Backend, Fields};
use std::path::PathBuf;
use std::sync::Arc;

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Dir {
        let d = std::env::temp_dir().join(format!("obop-bgate-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Dir(d)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn files_in(d: &Dir) -> usize {
    std::fs::read_dir(&d.0).map_or(0, |r| r.count())
}

#[test]
fn the_backup_before_a_module_upgrade_is_not_taken_when_the_schema_version_cannot_be_read() {
    let inner = obop_storage::open("memory:").unwrap();
    inner.insert("party", "p1", &Fields::new()).unwrap();
    let flaky = Flaky::new(inner.clone());
    let backend: Arc<dyn Backend> = flaky.clone();
    let blobs = Arc::new(Catalog::new(
        backend.clone(),
        open_bytes("memory:", None).unwrap(),
    ));
    let dir = Dir::new("module");
    let gate = ModuleGate::new(
        backend,
        blobs,
        GateSettings {
            strict: true,
            dir: Some(dir.0.clone()),
            keys: Keys::none(),
        },
    );
    let what = GateRequest {
        command: "module.upgrade",
        modules: vec!["crm 0.1.0->0.2.0".into()],
        steps: vec![],
        by: "root".into(),
    };
    // Labelled 0 it would be restored as never migrated: no backup is made.
    flaky.refuse_model(obop_migrate::META_MODEL, Mode::Reads);
    let refused = gate.before(&what);
    flaky.allow_models();
    let e = refused.unwrap_err();
    assert!(e.contains("the schema version cannot be read"), "{e}");
    assert_eq!(files_in(&dir), 0, "nothing was written");
    let receipt = gate.before(&what).unwrap();
    assert!(receipt.backup_id.is_some());
}

#[test]
fn a_scheduled_backup_waits_when_the_schema_version_cannot_be_read() {
    let inner = obop_storage::open("memory:").unwrap();
    let flaky = Flaky::new(inner);
    let backend: Arc<dyn Backend> = flaky.clone();
    flaky.refuse_model(obop_migrate::META_MODEL, Mode::Reads);
    let refused = schema_for_backup(&backend);
    flaky.allow_models();
    assert!(
        matches!(&refused, Err(e) if e.contains("cannot be read")),
        "{refused:?}"
    );
    assert_eq!(
        schema_for_backup(&backend),
        Ok(0),
        "a database never migrated"
    );
}
