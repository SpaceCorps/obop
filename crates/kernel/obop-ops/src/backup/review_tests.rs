//! Tests written for the review of D73: no silent downgrade to a plain backup, hostile
//! manifests, how a backup is published and cleaned up, and what a restore does with uploads.

use super::tests::{Dir, all, fields, seed};
use super::*;
use crate::testkit::Flaky;
use obop_storage::blob::{BlobBytes, Catalog, MemoryBytes, NewBlob};

const KA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const KC: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

fn keys(k: &str) -> Keys {
    Keys::from_ring(KeyRing::from_secrets(k, &[]).unwrap())
}

fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

#[test]
fn a_plain_pair_is_refused_when_the_instance_seals_its_backups_and_a_forged_one_restores_nothing() {
    let dir = Dir::new();
    // The real data, sealed.
    let real = dir.sqlite("real.db");
    seed(&real);
    let good = create_sealed(&real, None, 2, &dir.path("good"), &keys(KA), &mut |_| {}).unwrap();
    // What someone who can write the backup directory makes without any key: a plain pair of
    // other data, with correct checksums.
    let evil = dir.sqlite("evil.db");
    evil.insert(
        "auth_user",
        "mallory",
        &fields(json!({"groups": ["platform.admin"]})),
    )
    .unwrap();
    let forged = create(&evil, 2, &dir.path("forged"), &mut |_| {}).unwrap();
    assert_eq!(forged.manifest.format, FORMAT);

    // Nothing accepts it with the key configured: verify, restore, and (so) the gates, the
    // rollback and the drill, which all go through `verify_with`.
    let e = verify_with(&forged.data_path, &keys(KA)).unwrap_err();
    assert_eq!(error_code(&e), Some(NOT_SEALED), "{e}");
    assert!(e.contains("--allow-plain"), "{e}");
    let target = dir.sqlite("target.db");
    let e = restore_sealed(
        &target,
        None,
        2,
        &forged.data_path,
        RestoreOptions::default(),
        &keys(KA),
        &mut |_| {},
    )
    .unwrap_err();
    assert_eq!(error_code(&e), Some(NOT_SEALED), "{e}");
    assert_eq!(target.total().unwrap(), 0, "nothing was restored");
    let e = restore_sealed(
        &real,
        None,
        2,
        &forged.data_path,
        RestoreOptions {
            force: true,
            dry_run: false,
        },
        &keys(KA),
        &mut |_| {},
    )
    .unwrap_err();
    assert_eq!(error_code(&e), Some(NOT_SEALED), "{e}");
    assert!(
        real.get("auth_user", "ada").unwrap().is_some(),
        "not replaced"
    );

    // The sealed pair is as before.
    assert!(
        verify_with(&good.data_path, &keys(KA))
            .unwrap()
            .authenticated
    );
    // An operator who vouches for an old plain backup can still use it, and it is still never
    // called authenticated; an instance with no key at all behaves as it always did.
    let ok = verify_with(&forged.data_path, &keys(KA).allowing_plain(true)).unwrap();
    assert!(!ok.authenticated);
    let r = restore_sealed(
        &target,
        None,
        2,
        &forged.data_path,
        RestoreOptions::default(),
        &keys(KA).allowing_plain(true),
        &mut |_| {},
    )
    .unwrap();
    assert!(!r.authenticated);
    assert!(
        !verify_with(&forged.data_path, &Keys::none())
            .unwrap()
            .authenticated
    );
    // The structure-only check has no key and is not a restore: it is not refused.
    assert!(verify_structure(&good.data_path).is_ok());
}

#[test]
fn a_previous_backup_key_alone_opens_older_backups_and_never_seals() {
    let dir = Dir::new();
    let src = dir.sqlite("src.db");
    seed(&src);
    let old = create_sealed(&src, None, 2, &dir.path("old"), &keys(KA), &mut |_| {}).unwrap();
    let server = KeyRing::from_secrets(KC, &[]).unwrap();
    // The separate backup key was retired: only the old one is kept, as a previous key.
    let k = Keys::from_vars(None, Some(KA), &server).unwrap();
    assert!(k.can_seal(), "the server key seals new backups");
    assert!(verify_with(&old.data_path, &k).unwrap().authenticated);
    // No server key at all: the old key opens, and no new backup can be sealed with it.
    let k = Keys::from_vars(None, Some(KA), &KeyRing::default()).unwrap();
    assert!(!k.can_seal() && k.from() == KeysFrom::Nowhere);
    assert!(verify_with(&old.data_path, &k).unwrap().authenticated);
    let c = create_sealed(&src, None, 2, &dir.path("new"), &k, &mut |_| {}).unwrap();
    assert_eq!(c.manifest.format, FORMAT, "written plain, as with no key");
    // Garbage in the variable is still an error that names it.
    let e = Keys::from_vars(None, Some("short"), &server).err().unwrap();
    assert!(e.contains("OBOP_BACKUP_KEY_PREVIOUS"), "{e}");
}

#[test]
fn a_hostile_chunk_size_in_the_manifest_is_an_error_not_a_panic_or_an_allocation() {
    let dir = Dir::new();
    let src = dir.sqlite("src.db");
    seed(&src);
    let c = create_sealed(&src, None, 2, &dir.path("b.obbk"), &keys(KA), &mut |_| {}).unwrap();
    for hostile in [u64::MAX, u64::from(u32::MAX), 0, 15, 4 * 1024 * 1024 + 1] {
        let mut j: J =
            serde_json::from_str(&std::fs::read_to_string(&c.manifest_path).unwrap()).unwrap();
        j["encryption"]["chunk_size"] = J::from(hostile);
        std::fs::write(&c.manifest_path, j.to_string()).unwrap();
        for r in [
            verify_structure(&c.data_path).map(|_| ()),
            verify_with(&c.data_path, &keys(KA)).map(|_| ()),
            restore_sealed(
                &dir.sqlite("t.db"),
                None,
                2,
                &c.data_path,
                RestoreOptions::default(),
                &keys(KA),
                &mut |_| {},
            )
            .map(|_| ()),
        ] {
            let e = r.unwrap_err();
            assert!(e.contains("out of range"), "{hostile}: {e}");
        }
    }
}

#[test]
fn a_sealed_backup_is_not_given_a_gz_name() {
    let dir = Dir::new();
    let src = dir.sqlite("src.db");
    seed(&src);
    let e = create_sealed(
        &src,
        None,
        2,
        &dir.path("nightly.jsonl.gz"),
        &keys(KA),
        &mut |_| {},
    )
    .unwrap_err();
    assert!(e.contains(".obbk") && e.contains("--plain"), "{e}");
    assert!(
        !dir.path("nightly.jsonl.gz").exists(),
        "nothing was written under the misleading name"
    );
    // A plain backup keeps its .gz name; a sealed one may be named .obbk or given a directory.
    assert!(
        create_sealed(
            &src,
            None,
            2,
            &dir.path("plain.jsonl.gz"),
            &Keys::none(),
            &mut |_| {}
        )
        .is_ok()
    );
    assert!(create_sealed(&src, None, 2, &dir.path("n.obbk"), &keys(KA), &mut |_| {}).is_ok());
    let c = create_sealed(&src, None, 2, &dir.path("dir"), &keys(KA), &mut |_| {}).unwrap();
    assert!(c.data_path.to_string_lossy().ends_with(".obbk"));
}

#[test]
fn a_backup_appears_whole_manifest_and_data_and_a_failed_run_leaves_nothing() {
    let dir = Dir::new();
    let inner = dir.sqlite("src.db");
    seed(&inner);
    let flaky = Flaky::new(inner);
    let src: Arc<dyn Backend> = flaky.clone();
    let out = dir.path("bk");
    // A good run leaves the data file and the manifest and nothing else.
    let c = create_sealed(&src, None, 2, &out.join("a.obbk"), &keys(KA), &mut |_| {}).unwrap();
    assert_eq!(names(&out), ["a.obbk", "a.obbk.manifest.json"]);
    assert!(verify_with(&c.data_path, &keys(KA)).unwrap().authenticated);
    // The database goes away half way through the export: no part file, no manifest, no data.
    Flaky::set(&flaky.fail_export, true);
    let e = create_sealed(&src, None, 2, &out.join("b.obbk"), &keys(KA), &mut |_| {}).unwrap_err();
    assert!(e.contains("went away"), "{e}");
    assert_eq!(names(&out), ["a.obbk", "a.obbk.manifest.json"]);
    // Neither does a plain backup.
    let e = create(&src, 2, &out.join("c.jsonl.gz"), &mut |_| {}).unwrap_err();
    assert!(e.contains("went away"), "{e}");
    assert_eq!(names(&out), ["a.obbk", "a.obbk.manifest.json"]);
    Flaky::set(&flaky.fail_export, false);
    create_sealed(&src, None, 2, &out.join("d.obbk"), &keys(KA), &mut |_| {}).unwrap();
    assert_eq!(names(&out).len(), 4);
}

#[test]
fn part_files_of_a_killed_run_are_swept_by_the_next_backup_and_other_files_are_not() {
    let dir = Dir::new();
    let src = dir.sqlite("src.db");
    seed(&src);
    let out = dir.path("bk");
    std::fs::create_dir_all(&out).unwrap();
    let stale = out.join("obop-backup-20260101T000000Z.obbk.99-0.part");
    let fresh = out.join("obop-backup-20260102T000000Z.obbk.99-1.part");
    let other = out.join("somebody-elses.part");
    for p in [&stale, &fresh, &other] {
        std::fs::write(p, b"x").unwrap();
    }
    let long_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(8 * 3600);
    for p in [&stale, &other] {
        std::fs::OpenOptions::new()
            .write(true)
            .open(p)
            .unwrap()
            .set_modified(long_ago)
            .unwrap();
    }
    create_sealed(&src, None, 2, &out.join("n.obbk"), &keys(KA), &mut |_| {}).unwrap();
    assert!(!stale.exists(), "idle for hours: a run that died");
    assert!(
        fresh.exists(),
        "written to a moment ago: maybe still running"
    );
    assert!(other.exists(), "not a backup's part file");
}

#[test]
fn a_name_is_never_overwritten_even_when_two_runs_race_for_it() {
    let dir = Dir::new();
    let src = dir.sqlite("src.db");
    seed(&src);
    let out = dir.path("bk");
    let target = out.join("same.obbk");
    for round in 0..6 {
        let t = out.join(format!("same-{round}.obbk"));
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let results: Vec<Result<Created, String>> = std::thread::scope(|s| {
            (0..2)
                .map(|_| {
                    let (src, t, barrier) = (src.clone(), t.clone(), barrier.clone());
                    s.spawn(move || {
                        barrier.wait();
                        create_sealed(&src, None, 2, &t, &keys(KA), &mut |_| {})
                    })
                })
                .collect::<Vec<_>>()
                .into_iter()
                .map(|h| h.join().unwrap())
                .collect()
        });
        let won = results.iter().filter(|r| r.is_ok()).count();
        assert_eq!(won, 1, "round {round}: exactly one run publishes the name");
        let lost = results.iter().find_map(|r| r.as_ref().err()).unwrap();
        assert!(lost.contains("already exists"), "round {round}: {lost}");
        // The winner's pair is whole and opens; nothing of the loser is left.
        assert!(verify_with(&t, &keys(KA)).unwrap().authenticated);
        assert_eq!(
            names(&out)
                .iter()
                .filter(|n| n.starts_with(&format!("same-{round}.")))
                .count(),
            2,
            "{:?}",
            names(&out)
        );
    }
    assert!(!target.exists());
    // A manifest left behind with no data file also blocks the name: it is not silently reused.
    let orphan = out.join("o.obbk");
    std::fs::write(manifest_path_for(&orphan), b"{}").unwrap();
    let e = create_sealed(&src, None, 2, &orphan, &keys(KA), &mut |_| {}).unwrap_err();
    assert!(e.contains("already exists"), "{e}");
}

#[test]
fn an_upload_that_rotted_on_disk_is_left_out_and_named_and_verify_checks_every_file() {
    let dir = Dir::new();
    let src = dir.sqlite("src.db");
    seed(&src);
    let bytes = Arc::new(MemoryBytes::default());
    let blobs = Catalog::new(src.clone(), bytes.clone());
    let put = |seed: u8| {
        blobs
            .put(
                &vec![seed; 3000],
                NewBlob {
                    content_type: "image/png".into(),
                    uploader: "ada".into(),
                    ..NewBlob::default()
                },
            )
            .unwrap()
    };
    let (good, rotten, also_good) = (put(1), put(2), put(3));
    // One byte of the middle upload changes on disk.
    bytes.put(&rotten.id, &vec![9u8; 3000]).unwrap();
    let c = create_sealed(
        &src,
        Some(&blobs),
        2,
        &dir.path("b.obbk"),
        &keys(KA),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(c.manifest.blobs, 2, "the damaged file is not in the backup");
    assert_eq!(c.manifest.blobs_damaged, std::slice::from_ref(&rotten.id));
    let j: J = serde_json::from_str(&std::fs::read_to_string(&c.manifest_path).unwrap()).unwrap();
    assert_eq!(
        j["blobs_damaged"][0],
        rotten.id.as_str(),
        "named in the manifest"
    );
    // It verifies, and it restores: both files come back, in one go, not just the first.
    verify_with(&c.data_path, &keys(KA)).unwrap();
    let dst = dir.sqlite("dst.db");
    let fresh = Catalog::memory(dst.clone());
    restore_sealed(
        &dst,
        Some(&fresh),
        2,
        &c.data_path,
        RestoreOptions::default(),
        &keys(KA),
        &mut |_| {},
    )
    .unwrap();
    for id in [&good.id, &also_good.id] {
        assert_eq!(fresh.read_all(id, 1 << 20).unwrap().unwrap().len(), 3000);
    }

    // A backup file that carries a file that does not hash to its id is not a backup.
    let plain_dir = Dir::new();
    let src2 = dir.sqlite("src2.db");
    seed(&src2);
    let clean = Catalog::new(src2.clone(), Arc::new(MemoryBytes::default()));
    let info = clean
        .put(
            &vec![5u8; 3000],
            NewBlob {
                content_type: "image/png".into(),
                uploader: "ada".into(),
                ..NewBlob::default()
            },
        )
        .unwrap();
    let p = create_with(
        &src2,
        Some(&clean),
        2,
        &plain_dir.path("p.jsonl.gz"),
        &mut |_| {},
    )
    .unwrap();
    let text = super::tests::gunzip(&p.data_path);
    let swapped = text.replace(
        &base64::engine::general_purpose::STANDARD.encode(vec![5u8; 3000]),
        &base64::engine::general_purpose::STANDARD.encode(vec![6u8; 3000]),
    );
    assert_ne!(text, swapped);
    super::tests::rewrite(&p, &swapped);
    let e = verify(&p.data_path).unwrap_err();
    assert!(
        e.contains(&info.id) && e.contains("does not hash to its id"),
        "{e}"
    );
    // So nothing is replaced by a restore of it.
    let victim = dir.sqlite("victim.db");
    seed(&victim);
    let before = all(&victim);
    assert!(
        restore_with(
            &victim,
            Some(&clean),
            2,
            &p.data_path,
            RestoreOptions {
                force: true,
                dry_run: false
            },
            &mut |_| {}
        )
        .is_err()
    );
    assert_eq!(all(&victim), before, "the target is as it was");
}

#[test]
fn a_restore_can_carry_the_callers_rows_in_its_own_transaction_and_reports_authentication() {
    let dir = Dir::new();
    let src = dir.sqlite("src.db");
    seed(&src);
    let c = create_sealed(&src, None, 2, &dir.path("b.obbk"), &keys(KA), &mut |_| {}).unwrap();
    let target = dir.sqlite("t.db");
    target.insert("old", "o1", &Fields::new()).unwrap();
    let extra = vec![(
        "obop_migration".to_string(),
        "core/rollback-1".to_string(),
        fields(json!({"note": "because"})),
    )];
    let r = restore_with_rows(
        &target,
        None,
        2,
        &c.data_path,
        RestoreOptions {
            force: true,
            dry_run: false,
        },
        &keys(KA),
        extra,
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(r.records, c.manifest.records, "extra rows are not counted");
    assert!(r.authenticated);
    assert!(target.get("old", "o1").unwrap().is_none(), "replaced");
    assert_eq!(
        target
            .get("obop_migration", "core/rollback-1")
            .unwrap()
            .unwrap()["note"],
        "because"
    );
    assert_eq!(target.total().unwrap(), c.manifest.records + 1);
    // If the transaction fails, the extra rows are not there either.
    let flaky_inner = dir.sqlite("f.db");
    flaky_inner.insert("old", "o1", &Fields::new()).unwrap();
    let e = restore_with_rows(
        &flaky_inner,
        None,
        2,
        &c.data_path,
        RestoreOptions {
            force: true,
            dry_run: false,
        },
        &keys(KC),
        vec![("x".into(), "y".into(), Fields::new())],
        &mut |_| {},
    )
    .unwrap_err();
    assert_eq!(error_code(&e), Some(KEY_REQUIRED), "{e}");
    assert!(flaky_inner.get("x", "y").unwrap().is_none());
    assert!(flaky_inner.get("old", "o1").unwrap().is_some());
}

#[test]
fn files_failing_to_write_after_the_records_say_so_and_can_be_retried() {
    struct Refusing;
    impl BlobBytes for Refusing {
        fn put(&self, _: &str, _: &[u8]) -> obop_storage::Result<()> {
            Err("no space left on device".into())
        }
        fn open(&self, _: &str) -> obop_storage::Result<Option<Box<dyn std::io::Read + Send>>> {
            Ok(None)
        }
        fn delete(&self, _: &str) -> obop_storage::Result<bool> {
            Ok(false)
        }
        fn keys(&self) -> obop_storage::Result<Vec<String>> {
            Ok(Vec::new())
        }
    }
    let dir = Dir::new();
    let src = dir.sqlite("src.db");
    seed(&src);
    let blobs = Catalog::new(src.clone(), Arc::new(MemoryBytes::default()));
    let info = blobs
        .put(
            &vec![4u8; 2000],
            NewBlob {
                content_type: "image/png".into(),
                uploader: "ada".into(),
                ..NewBlob::default()
            },
        )
        .unwrap();
    let c = create_sealed(
        &src,
        Some(&blobs),
        2,
        &dir.path("b.obbk"),
        &keys(KA),
        &mut |_| {},
    )
    .unwrap();
    let dst = dir.sqlite("dst.db");
    let full = Catalog::new(dst.clone(), Arc::new(Refusing));
    let e = restore_sealed(
        &dst,
        Some(&full),
        2,
        &c.data_path,
        RestoreOptions::default(),
        &keys(KA),
        &mut |_| {},
    )
    .unwrap_err();
    assert_eq!(error_code(&e), Some(FILES_FAILED), "{e}");
    assert!(
        e.contains("records are restored") && e.contains("--force") && e.contains("no space"),
        "{e}"
    );
    assert_eq!(
        dst.total().unwrap(),
        c.manifest.records,
        "the records are in"
    );
    // Running it again with a store that works writes the files.
    let fresh = Catalog::memory(dst.clone());
    restore_sealed(
        &dst,
        Some(&fresh),
        2,
        &c.data_path,
        RestoreOptions {
            force: true,
            dry_run: false,
        },
        &keys(KA),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(
        fresh.read_all(&info.id, 1 << 20).unwrap().unwrap().len(),
        2000
    );
}

#[test]
fn a_canary_in_a_plain_backup_is_found_after_decompression_and_a_sealed_one_holds_none() {
    let dir = Dir::new();
    let src = dir.sqlite("src.db");
    seed(&src);
    src.insert(
        "party",
        "canary",
        &fields(json!({"name": "CANARY-zorp-7f3a91-unforgettable"})),
    )
    .unwrap();
    let needle = b"CANARY-zorp-7f3a91-unforgettable";
    let plain = create(&src, 2, &dir.path("p.jsonl.gz"), &mut |_| {}).unwrap();
    // As stored, gzip hides it from a byte scan; that is the false comfort the review found.
    assert_eq!(count_in_file(&plain.data_path, needle).unwrap(), 0);
    let (hits, sealed) = count_in_backup(&plain.data_path, needle).unwrap();
    assert!(hits >= 1 && !sealed, "{hits} {sealed}");
    let s = create_sealed(&src, None, 2, &dir.path("s.obbk"), &keys(KA), &mut |_| {}).unwrap();
    let (hits, sealed) = count_in_backup(&s.manifest_path, needle).unwrap();
    assert_eq!((hits, sealed), (0, true));
}
