use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

fn unique(tag: &str) -> String {
    static N: AtomicUsize = AtomicUsize::new(0);
    format!(
        "{tag}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    )
}

/// A scratch directory that goes away with the test.
struct Dir(PathBuf);

impl Dir {
    fn new() -> Dir {
        let d = std::env::temp_dir().join(unique("obop-ops"));
        std::fs::create_dir_all(&d).unwrap();
        Dir(d)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
    fn sqlite(&self, name: &str) -> Arc<dyn Backend> {
        obop_storage::open(&format!("sqlite://{}", self.path(name).display())).unwrap()
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fields(v: J) -> Fields {
    v.as_object().unwrap().clone()
}

/// Records with the awkward things a backup must keep: sealed secrets, unicode, big numbers,
/// nulls, nested values, an empty object.
fn seed(b: &Arc<dyn Backend>) {
    b.insert(
        "auth_user",
        "ada",
        &fields(json!({
            "password_hash": "$argon2id$v=19$m=19456,t=2,p=1$abc$def",
            "groups": ["base.user", "sales.user"],
            "totp_secret": "e2.deadbeef.000102030405060708090a0b.aabbccddeeff00112233445566778899",
        })),
    )
    .unwrap();
    b.insert(
        "party",
        "p1",
        &fields(json!({
            "name": "Zoë \"Ünï\" O'Brien \u{1F600}", "balance": 9_007_199_254_740_993_i64,
            "note": null, "tags": [], "nested": {"a": [1, 2, {"b": null}]},
        })),
    )
    .unwrap();
    b.insert("party", "p2", &Fields::new()).unwrap();
    b.insert(
        "sale",
        "s1",
        &fields(json!({"total": 12345, "party": "p1"})),
    )
    .unwrap();
}

fn all(b: &Arc<dyn Backend>) -> Vec<Row> {
    let mut v = Vec::new();
    b.export(100, &mut |batch| {
        v.extend_from_slice(batch);
        Ok(())
    })
    .unwrap();
    v
}

fn make(src: &Arc<dyn Backend>, dir: &Dir, name: &str) -> Created {
    create(src, 2, &dir.path(name), &mut |_| {}).unwrap()
}

fn gunzip(p: &Path) -> String {
    let mut s = String::new();
    flate2::read::GzDecoder::new(File::open(p).unwrap())
        .read_to_string(&mut s)
        .unwrap();
    s
}

/// Rewrite the data file with `text` and make the manifest agree about the file as stored, so
/// only the checks of the content itself can catch what was changed.
fn rewrite(c: &Created, text: &str) {
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(text.as_bytes()).unwrap();
    let bytes = gz.finish().unwrap();
    std::fs::write(&c.data_path, &bytes).unwrap();
    let mut j: J =
        serde_json::from_str(&std::fs::read_to_string(&c.manifest_path).unwrap()).unwrap();
    j["data_sha256"] = J::from(hex(&Sha256::digest(&bytes)));
    j["data_bytes"] = J::from(bytes.len());
    std::fs::write(&c.manifest_path, j.to_string()).unwrap();
}

fn edit_manifest(c: &Created, f: impl FnOnce(&mut J)) {
    let mut j: J =
        serde_json::from_str(&std::fs::read_to_string(&c.manifest_path).unwrap()).unwrap();
    f(&mut j);
    std::fs::write(&c.manifest_path, j.to_string()).unwrap();
}

#[test]
fn a_backup_restores_to_an_identical_database_on_sqlite() {
    let dir = Dir::new();
    let src = dir.sqlite("src.db");
    seed(&src);
    src.kv_put("session:x", "short-lived", Some(60_000))
        .unwrap();
    let c = make(&src, &dir, "b.jsonl.gz");
    assert_eq!(c.manifest.records, 4);
    assert_eq!(c.manifest.models["party"], 2);
    assert_eq!(c.manifest.schema_version, 2);
    assert_eq!(c.manifest.source, "sqlite");
    let v = verify(&c.data_path).unwrap();
    assert_eq!(v.manifest.records, 4);
    // Verifying the manifest path works too.
    verify(&c.manifest_path).unwrap();

    let dst = dir.sqlite("dst.db");
    let mut seen = 0;
    let r = restore(&dst, 2, &c.data_path, RestoreOptions::default(), &mut |n| {
        seen = n
    })
    .unwrap();
    assert_eq!(
        (r.records, r.target_records, r.needs_migration),
        (4, 0, false)
    );
    assert_eq!(seen, 4);
    assert_eq!(all(&dst), all(&src), "same records, same order, same bytes");
    assert_eq!(
        dst.kv_get("session:x").unwrap(),
        None,
        "short-lived values are not backed up"
    );
    // The sealed value is exactly as stored, and the key is nowhere in the file.
    let text = gunzip(&c.data_path);
    assert!(text.contains("e2.deadbeef.000102030405060708090a0b."));
    assert_eq!(
        dst.get("party", "p1").unwrap().unwrap()["balance"],
        9_007_199_254_740_993_i64
    );
}

#[test]
fn memory_to_sqlite_and_back_keeps_everything() {
    let dir = Dir::new();
    let mem = obop_storage::open("memory:").unwrap();
    seed(&mem);
    let c = make(&mem, &dir, "m.jsonl.gz");
    assert_eq!(c.manifest.source, "memory");
    let file = dir.sqlite("f.db");
    restore(
        &file,
        2,
        &c.data_path,
        RestoreOptions::default(),
        &mut |_| {},
    )
    .unwrap();
    let back = make(&file, &dir, "back.jsonl.gz");
    let mem2 = obop_storage::open("memory:").unwrap();
    restore(
        &mem2,
        2,
        &back.data_path,
        RestoreOptions::default(),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(all(&mem2), all(&mem));
}

#[test]
fn a_directory_gets_a_timestamped_file_and_nothing_is_overwritten() {
    let dir = Dir::new();
    let src = dir.sqlite("src.db");
    seed(&src);
    let out = dir.path("backups");
    std::fs::create_dir_all(&out).unwrap();
    let c = create(&src, 2, &out, &mut |_| {}).unwrap();
    let name = c
        .data_path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    assert!(
        name.starts_with("obop-backup-") && name.ends_with(".jsonl.gz"),
        "{name}"
    );
    assert!(c.manifest_path.exists());
    let again = create(&src, 2, &c.data_path, &mut |_| {}).unwrap_err();
    assert!(again.contains("never overwritten"));
}

#[test]
fn damage_and_edits_are_detected() {
    let dir = Dir::new();
    let src = dir.sqlite("src.db");
    seed(&src);

    // A flipped byte in the file as stored.
    let c = make(&src, &dir, "flip.jsonl.gz");
    let mut bytes = std::fs::read(&c.data_path).unwrap();
    let mid = bytes.len() / 2;
    bytes[mid] ^= 0xff;
    std::fs::write(&c.data_path, &bytes).unwrap();
    assert!(verify(&c.data_path).unwrap_err().contains("checksum"));

    // A truncated file.
    let c = make(&src, &dir, "cut.jsonl.gz");
    let bytes = std::fs::read(&c.data_path).unwrap();
    std::fs::write(&c.data_path, &bytes[..bytes.len() - 10]).unwrap();
    assert!(verify(&c.data_path).unwrap_err().contains("bytes"));

    // A manifest edited to hide a missing record.
    let c = make(&src, &dir, "count.jsonl.gz");
    edit_manifest(&c, |j| j["records"] = J::from(3));
    assert!(verify(&c.data_path).unwrap_err().contains("records"));

    // A record changed inside a file whose stored checksum was made to match.
    let c = make(&src, &dir, "edit.jsonl.gz");
    let text = gunzip(&c.data_path).replace("12345", "99999");
    rewrite(&c, &text);
    assert!(
        verify(&c.data_path)
            .unwrap_err()
            .contains("do not match their checksum")
    );

    // A record dropped, the end marker kept.
    let c = make(&src, &dir, "drop.jsonl.gz");
    let text: Vec<String> = gunzip(&c.data_path).lines().map(String::from).collect();
    let kept: Vec<_> = text
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != 2)
        .map(|(_, l)| l.clone())
        .collect();
    rewrite(&c, &(kept.join("\n") + "\n"));
    assert!(verify(&c.data_path).unwrap_err().contains("records"));

    // The end marker removed: an incomplete backup.
    let c = make(&src, &dir, "noend.jsonl.gz");
    let text: Vec<String> = gunzip(&c.data_path).lines().map(String::from).collect();
    rewrite(&c, &(text[..text.len() - 1].join("\n") + "\n"));
    assert!(verify(&c.data_path).unwrap_err().contains("incomplete"));

    // A header that disagrees with the manifest.
    let c = make(&src, &dir, "head.jsonl.gz");
    let text = gunzip(&c.data_path).replacen("\"schema_version\":2", "\"schema_version\":1", 1);
    rewrite(&c, &text);
    assert!(verify(&c.data_path).unwrap_err().contains("header"));

    // Not a backup at all.
    std::fs::write(dir.path("junk.manifest.json"), "{}").unwrap();
    std::fs::write(dir.path("junk"), "x").unwrap();
    assert!(verify(&dir.path("junk")).is_err());
    assert!(verify(&dir.path("missing")).is_err());
}

#[test]
fn a_restore_into_a_database_with_data_is_refused_unless_forced() {
    let dir = Dir::new();
    let src = dir.sqlite("src.db");
    seed(&src);
    let c = make(&src, &dir, "b.jsonl.gz");
    let dst = dir.sqlite("dst.db");
    dst.insert("party", "other", &fields(json!({"name": "keep me"})))
        .unwrap();
    dst.kv_put("session:old", "x", None).unwrap();

    let e = restore(
        &dst,
        2,
        &c.data_path,
        RestoreOptions::default(),
        &mut |_| {},
    )
    .unwrap_err();
    assert!(e.contains("not empty") && e.contains("--force"), "{e}");
    assert!(dst.get("party", "other").unwrap().is_some(), "untouched");

    // A dry run says what would happen and writes nothing, even when forced.
    let r = restore(
        &dst,
        2,
        &c.data_path,
        RestoreOptions {
            force: true,
            dry_run: true,
        },
        &mut |_| {},
    )
    .unwrap();
    assert!(r.dry_run && r.records == 4 && r.target_records == 1);
    assert!(dst.get("party", "other").unwrap().is_some());

    // Forced, the target is replaced as a whole.
    restore(
        &dst,
        2,
        &c.data_path,
        RestoreOptions {
            force: true,
            dry_run: false,
        },
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(all(&dst), all(&src));
    assert!(dst.get("party", "other").unwrap().is_none());
    assert_eq!(dst.kv_get("session:old").unwrap(), None);
}

#[test]
fn versions_that_this_build_cannot_read_are_refused_before_anything_is_written() {
    let dir = Dir::new();
    let src = dir.sqlite("src.db");
    seed(&src);
    let dst = dir.sqlite("dst.db");

    // A format from the future.
    let c = make(&src, &dir, "f2.jsonl.gz");
    edit_manifest(&c, |j| j["format"] = J::from(2));
    let e = verify(&c.data_path).unwrap_err();
    assert!(e.contains("format 2") && e.contains("not supported"), "{e}");
    assert!(
        restore(
            &dst,
            2,
            &c.data_path,
            RestoreOptions::default(),
            &mut |_| {}
        )
        .is_err()
    );

    // A schema newer than the binary: refused, with the advice.
    let c = create(&src, 9, &dir.path("s9.jsonl.gz"), &mut |_| {}).unwrap();
    let e = restore(
        &dst,
        2,
        &c.data_path,
        RestoreOptions::default(),
        &mut |_| {},
    )
    .unwrap_err();
    assert!(
        e.contains("schema version 9") && e.contains("newer than this build"),
        "{e}"
    );
    assert_eq!(dst.total().unwrap(), 0);

    // An older schema restores and says it needs a migration.
    let c = create(&src, 1, &dir.path("s1.jsonl.gz"), &mut |_| {}).unwrap();
    let r = restore(
        &dst,
        2,
        &c.data_path,
        RestoreOptions::default(),
        &mut |_| {},
    )
    .unwrap();
    assert!(r.needs_migration);
}

#[test]
fn a_large_database_is_streamed_in_batches() {
    let dir = Dir::new();
    let src = dir.sqlite("big.db");
    let rows: Vec<Row> = (0..5000)
        .map(|i| {
            (
                "item".to_string(),
                format!("{i:06}"),
                fields(json!({"n": i, "pad": "x".repeat(300)})),
            )
        })
        .collect();
    for chunk in rows.chunks(1000) {
        src.insert_many(chunk).unwrap();
    }
    let mut ticks = Vec::new();
    let c = create(&src, 2, &dir.path("big.jsonl.gz"), &mut |n| ticks.push(n)).unwrap();
    assert_eq!(c.manifest.records, 5000);
    assert_eq!(ticks, [1000, 2000, 3000, 4000, 5000], "one tick per batch");
    assert!(c.manifest.data_bytes < 5000 * 300, "compressed");
    let dst = dir.sqlite("big2.db");
    restore(
        &dst,
        2,
        &c.data_path,
        RestoreOptions::default(),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(dst.count("item").unwrap(), 5000);
    assert_eq!(dst.get("item", "004999").unwrap().unwrap()["n"], 4999);
}

/// A scratch database on the PostgreSQL server of `OBOP_TEST_POSTGRES_URL`, dropped afterwards.
struct PgScratch {
    admin: String,
    name: String,
    url: String,
}

impl PgScratch {
    fn new(base: &str) -> PgScratch {
        let name = unique("obop_ops_t").replace('-', "_");
        let mut c = postgres::Client::connect(base, postgres::NoTls).unwrap();
        c.batch_execute(&format!("CREATE DATABASE {name}")).unwrap();
        let (head, _) = base.rsplit_once('/').unwrap();
        PgScratch {
            admin: base.to_string(),
            url: format!("{head}/{name}"),
            name,
        }
    }
}

impl Drop for PgScratch {
    fn drop(&mut self) {
        if let Ok(mut c) = postgres::Client::connect(&self.admin, postgres::NoTls) {
            let _ = c.batch_execute(&format!(
                "DROP DATABASE IF EXISTS {} WITH (FORCE)",
                self.name
            ));
        }
    }
}

#[test]
fn postgres_round_trips_and_moves_data_to_and_from_sqlite() {
    let Ok(base) = std::env::var("OBOP_TEST_POSTGRES_URL") else {
        eprintln!("skipped: set OBOP_TEST_POSTGRES_URL to run against PostgreSQL");
        return;
    };
    let dir = Dir::new();
    let (a, b) = (PgScratch::new(&base), PgScratch::new(&base));
    let pg_a = obop_storage::open(&a.url).unwrap();
    seed(&pg_a);
    pg_a.kv_put("session:x", "v", None).unwrap();
    let c = make(&pg_a, &dir, "pg.jsonl.gz");
    assert_eq!(c.manifest.source, "postgres");
    verify(&c.data_path).unwrap();
    // PostgreSQL to PostgreSQL.
    let pg_b = obop_storage::open(&b.url).unwrap();
    restore(
        &pg_b,
        2,
        &c.data_path,
        RestoreOptions::default(),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(all(&pg_b), all(&pg_a));
    // Refusal and force on PostgreSQL.
    assert!(
        restore(
            &pg_b,
            2,
            &c.data_path,
            RestoreOptions::default(),
            &mut |_| {}
        )
        .unwrap_err()
        .contains("not empty")
    );
    pg_b.insert("party", "extra", &Fields::new()).unwrap();
    restore(
        &pg_b,
        2,
        &c.data_path,
        RestoreOptions {
            force: true,
            dry_run: false,
        },
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(all(&pg_b), all(&pg_a));
    // PostgreSQL to SQLite and back: nothing changes on the way (JSONB reorders object keys, so
    // compare parsed values, which is what a restore must preserve).
    let lite = dir.sqlite("lite.db");
    restore(
        &lite,
        2,
        &c.data_path,
        RestoreOptions::default(),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(all(&lite), all(&pg_a));
    let back = make(&lite, &dir, "lite.jsonl.gz");
    let pg_c = PgScratch::new(&base);
    let pg_c_db = obop_storage::open(&pg_c.url).unwrap();
    restore(
        &pg_c_db,
        2,
        &back.data_path,
        RestoreOptions::default(),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(all(&pg_c_db), all(&pg_a));
    assert_eq!(
        pg_c_db.get("party", "p1").unwrap().unwrap()["balance"],
        9_007_199_254_740_993_i64
    );
}

#[test]
fn uploaded_files_travel_with_the_backup_and_come_back_intact() {
    use obop_storage::blob::{Catalog, NewBlob};
    let dir = Dir::new();
    let src = dir.sqlite("src.db");
    seed(&src);
    let blobs = Catalog::memory(src.clone());
    let png: Vec<u8> = (0..=255u8).cycle().take(5000).collect();
    let info = blobs
        .put(
            &png,
            NewBlob {
                content_type: "image/png".into(),
                uploader: "ada".into(),
                ..NewBlob::default()
            },
        )
        .unwrap();
    let c = create_with(&src, Some(&blobs), 2, &dir.path("b.jsonl.gz"), &mut |_| {}).unwrap();
    assert_eq!(c.manifest.blobs, 1);
    assert_eq!(verify(&c.data_path).unwrap().manifest.blobs, 1);

    // Into a database whose byte store is empty.
    let dst = dir.sqlite("dst.db");
    let fresh = Catalog::memory(dst.clone());
    let r = restore_with(
        &dst,
        Some(&fresh),
        2,
        &c.data_path,
        RestoreOptions::default(),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(r.records, c.manifest.records);
    let back = fresh.info(&info.id).unwrap().expect("the blob record");
    assert_eq!(back.pending, info.pending, "what holds it is kept");
    assert_eq!(fresh.read_all(&info.id, 1 << 20).unwrap().unwrap(), png);

    // A damaged blob line is caught by verify before anything is written.
    let text = gunzip(&c.data_path);
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    let at = lines.iter().position(|l| l.contains("\"blob\"")).unwrap();
    lines[at] = lines[at].replace("\"x\":\"", "\"x\":\"AA");
    rewrite(&c, &(lines.join("\n") + "\n"));
    assert!(verify(&c.data_path).is_err());
}
