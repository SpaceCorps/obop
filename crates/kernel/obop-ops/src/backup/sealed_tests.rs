//! Backup format 2: authenticated, encrypted backups (OBR-DAT-046, D73).

use super::tests::{Dir, PgScratch, all, fields, seed};
use super::*;
use obop_keys::key_id;

const KA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const KB: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const CANARY: &str = "CANARY-7f3a91-zorp-the-unforgettable";

fn ring(k: &str) -> KeyRing {
    KeyRing::from_secrets(k, &[]).unwrap()
}

fn keys(k: &str) -> Keys {
    Keys::from_ring(ring(k))
}

fn sealed(src: &Arc<dyn Backend>, dir: &Dir, name: &str, k: &Keys) -> Created {
    create_sealed(src, None, 2, &dir.path(name), k, &mut |_| {}).unwrap()
}

/// Enough incompressible records for several chunks.
fn noisy(b: &Arc<dyn Backend>, n: usize) {
    let mut x: u64 = 0x2545_F491_4F6C_DD1D;
    let rows: Vec<Row> = (0..n)
        .map(|i| {
            let pad: String = (0..40)
                .map(|_| {
                    x ^= x << 13;
                    x ^= x >> 7;
                    x ^= x << 17;
                    format!("{x:016x}")
                })
                .collect();
            (
                "item".to_string(),
                format!("{i:06}"),
                fields(json!({"n": i, "pad": pad})),
            )
        })
        .collect();
    for chunk in rows.chunks(500) {
        b.insert_many(chunk).unwrap();
    }
}

fn manifest_json(c: &Created) -> J {
    serde_json::from_str(&std::fs::read_to_string(&c.manifest_path).unwrap()).unwrap()
}

fn write_manifest(c: &Created, j: &J) {
    std::fs::write(&c.manifest_path, j.to_string()).unwrap();
}

/// What a holder of the key does after changing the data file: make the manifest tell the truth
/// about it again (size, checksum, chain, MAC). The data file itself stays as it is.
fn resign(c: &Created, k: &Keys) {
    let mut j = manifest_json(c);
    let (sha, bytes) = file_sha256(&c.data_path).unwrap();
    j["data_sha256"] = J::from(sha);
    j["data_bytes"] = J::from(bytes);
    let f = File::open(&c.data_path).unwrap();
    let (chunks, head) = stream::scan(BufReader::new(f), stream::CHUNK).unwrap();
    j["encryption"]["chunks"] = J::from(chunks);
    j["encryption"]["chain_head"] = J::from(head);
    let key = j["encryption"]["key_id"].as_str().unwrap().to_string();
    let tag = k
        .ring
        .mac(&key, MAC_PURPOSE, &Manifest::mac_input(&j))
        .unwrap();
    j["mac"] = J::from(hex(&tag));
    write_manifest(c, &j);
}

fn flip(path: &Path, at: usize, bit: u8) {
    let mut b = std::fs::read(path).unwrap();
    b[at] ^= 1 << bit;
    std::fs::write(path, b).unwrap();
}

#[test]
fn a_sealed_backup_round_trips_and_is_authenticated_by_its_key() {
    let dir = Dir::new();
    let src = dir.sqlite("src.db");
    seed(&src);
    let c = sealed(&src, &dir, "b.obbk", &keys(KA));
    let m = &c.manifest;
    assert_eq!((m.format, m.records, m.sealed()), (2, 4, true));
    let e = m.encryption.as_ref().unwrap();
    assert_eq!(e.key_id, key_id(KA));
    assert_eq!(e.cipher, "chacha20poly1305-stream-v1");
    assert!(m.mac.as_deref().is_some_and(|t| t.len() == 64));
    assert!(m.backup_id.as_deref().is_some_and(|t| t.len() == 32));
    assert!(c.data_path.to_string_lossy().ends_with(".obbk"));
    let v = verify_with(&c.data_path, &keys(KA)).unwrap();
    assert!(v.authenticated);
    assert_eq!(v.manifest.records, 4);
    // The manifest path works too, and the same data restores to an identical database.
    verify_with(&c.manifest_path, &keys(KA)).unwrap();
    let dst = dir.sqlite("dst.db");
    let r = restore_sealed(
        &dst,
        None,
        2,
        &c.data_path,
        RestoreOptions::default(),
        &keys(KA),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!((r.records, r.needs_migration), (4, false));
    assert_eq!(all(&dst), all(&src));
    // A directory gets a timestamped sealed file.
    let out = dir.path("backups");
    std::fs::create_dir_all(&out).unwrap();
    let c2 = create_sealed(&src, None, 2, &out, &keys(KA), &mut |_| {}).unwrap();
    let name = c2
        .data_path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    assert!(
        name.starts_with("obop-backup-") && name.ends_with(".obbk"),
        "{name}"
    );
    // Two backups of the same data share nothing: the data key and nonces are fresh.
    let c3 = sealed(&src, &dir, "again.obbk", &keys(KA));
    assert_ne!(
        std::fs::read(&c.data_path).unwrap(),
        std::fs::read(&c3.data_path).unwrap()
    );
    assert_ne!(c.manifest.backup_id, c3.manifest.backup_id);
}

#[test]
fn restoring_without_the_key_is_refused_with_a_stable_code_and_writes_nothing() {
    let dir = Dir::new();
    let src = dir.sqlite("src.db");
    seed(&src);
    let c = sealed(&src, &dir, "b.obbk", &keys(KA));
    let dst = dir.sqlite("dst.db");
    for k in [Keys::none(), keys(KB)] {
        let e = restore_sealed(
            &dst,
            None,
            2,
            &c.data_path,
            RestoreOptions::default(),
            &k,
            &mut |_| {},
        )
        .unwrap_err();
        assert!(e.starts_with("restore_key_required:"), "{e}");
        assert_eq!(error_code(&e), Some(KEY_REQUIRED));
        // The message names the key by id and never by content.
        assert!(
            e.contains(&key_id(KA)) && !e.contains(KA) && !e.contains(KB),
            "{e}"
        );
        assert_eq!(dst.total().unwrap(), 0, "no record written");
    }
    assert!(verify(&c.data_path).unwrap_err().starts_with(KEY_REQUIRED));
    assert_eq!(error_code("some other error"), None);
}

#[test]
fn a_planted_canary_is_absent_from_a_sealed_backup_and_found_in_what_it_restores() {
    let dir = Dir::new();
    let src = dir.sqlite("src.db");
    seed(&src);
    src.insert("party", "canary", &fields(json!({"name": CANARY})))
        .unwrap();
    let c = sealed(&src, &dir, "b.obbk", &keys(KA));
    for f in [&c.data_path, &c.manifest_path] {
        assert_eq!(count_in_file(f, CANARY.as_bytes()).unwrap(), 0, "{f:?}");
        assert_eq!(count_in_file(f, b"Zo\xc3\xab").unwrap(), 0);
    }
    // The scanner finds a string that is there (this one is a model name in the manifest).
    assert!(count_in_file(&c.manifest_path, b"party").unwrap() > 0);
    assert!(count_in_file(&c.manifest_path, b"").is_err());
    // And it is in the database restored from the file: the test is not vacuous.
    let dst = dir.sqlite("dst.db");
    restore_sealed(
        &dst,
        None,
        2,
        &c.data_path,
        RestoreOptions::default(),
        &keys(KA),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(dst.get("party", "canary").unwrap().unwrap()["name"], CANARY);
    // A string spanning the buffer boundary of the scanner is found too.
    let big = dir.path("big.bin");
    let mut bytes = vec![b'x'; (1 << 16) - 5];
    bytes.extend_from_slice(CANARY.as_bytes());
    bytes.extend_from_slice(&[b'y'; 100]);
    std::fs::write(&big, bytes).unwrap();
    assert_eq!(count_in_file(&big, CANARY.as_bytes()).unwrap(), 1);
}

#[test]
fn one_hundred_single_bit_flips_in_the_data_file_are_each_detected() {
    let dir = Dir::new();
    let src = dir.sqlite("src.db");
    noisy(&src, 400);
    let c = sealed(&src, &dir, "b.obbk", &keys(KA));
    assert!(
        c.manifest.encryption.as_ref().unwrap().chunks > 1,
        "several chunks"
    );
    let len = std::fs::metadata(&c.data_path).unwrap().len() as usize;
    verify_with(&c.data_path, &keys(KA)).unwrap();
    let mut x: u64 = 88_172_645_463_325_252;
    let mut reached_the_cipher = 0;
    for i in 0..100 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        let (at, bit) = ((x % len as u64) as usize, (i % 8) as u8);
        flip(&c.data_path, at, bit);
        // The checksum in the manifest catches it at once.
        assert!(
            verify_with(&c.data_path, &keys(KA)).is_err(),
            "flip {i} at {at}"
        );
        flip(&c.data_path, at, bit);
        // A holder of the key who makes the manifest agree with the damaged file still cannot
        // make the damaged chunk open: the cipher refuses it.
        if i % 10 == 0 {
            flip(&c.data_path, at, bit);
            let good = std::fs::read(&c.manifest_path).unwrap();
            if stream::scan(
                BufReader::new(File::open(&c.data_path).unwrap()),
                stream::CHUNK,
            )
            .is_ok()
            {
                resign(&c, &keys(KA));
                let e = verify_with(&c.data_path, &keys(KA)).unwrap_err();
                // The cipher's own refusal, not some later parse error: a chunk that does not
                // authenticate, or a header or wrapped key that was changed.
                assert!(
                    e.contains("does not authenticate")
                        || e.contains("wrapped data key does not open")
                        || e.contains("disagrees with the manifest")
                        || e.contains("sealed header"),
                    "flip {i} at {at}: {e}"
                );
                reached_the_cipher += 1;
            }
            flip(&c.data_path, at, bit);
            std::fs::write(&c.manifest_path, good).unwrap();
        }
    }
    // The guard above must not hide the check: most of the ten flips reach the cipher.
    assert!(
        reached_the_cipher >= 8,
        "only {reached_the_cipher} of 10 flips reached the cipher"
    );
    verify_with(&c.data_path, &keys(KA)).unwrap();
}

#[test]
fn every_field_of_the_manifest_is_authenticated() {
    let dir = Dir::new();
    let src = dir.sqlite("src.db");
    seed(&src);
    let c = sealed(&src, &dir, "b.obbk", &keys(KA));
    let good = manifest_json(&c);
    let mut tried = 0;
    let paths: Vec<(String, Option<String>)> = good
        .as_object()
        .unwrap()
        .iter()
        .flat_map(|(k, v)| match v.as_object() {
            Some(o) if k != "models" => o.keys().map(|s| (k.clone(), Some(s.clone()))).collect(),
            _ => vec![(k.clone(), None)],
        })
        .collect();
    for (k, sub) in paths {
        if k == "mac" {
            continue;
        }
        let mut j = good.clone();
        let target = match &sub {
            Some(s) => &mut j[&k][s],
            None => &mut j[&k],
        };
        *target = match target.clone() {
            J::String(s) => J::from(format!("{s}x")),
            J::Number(n) => J::from(n.as_u64().unwrap_or(0) + 1),
            J::Object(mut o) => {
                o.insert("extra".into(), J::from(1));
                J::Object(o)
            }
            J::Array(mut a) => {
                a.push(J::from("extra"));
                J::Array(a)
            }
            other => other,
        };
        write_manifest(&c, &j);
        assert!(
            verify_with(&c.data_path, &keys(KA)).is_err(),
            "{k}.{sub:?} could be changed"
        );
        tried += 1;
    }
    assert!(tried >= 15, "{tried}");
    // The MAC itself.
    let mut j = good.clone();
    j["mac"] = J::from("0".repeat(64));
    write_manifest(&c, &j);
    assert!(
        verify_with(&c.data_path, &keys(KA))
            .unwrap_err()
            .contains("authenticate")
    );
    // Whitespace is not content.
    std::fs::write(
        &c.manifest_path,
        serde_json::to_string_pretty(&good).unwrap(),
    )
    .unwrap();
    verify_with(&c.data_path, &keys(KA)).unwrap();
}

#[test]
fn a_forged_pair_is_rejected_without_the_key() {
    let dir = Dir::new();
    let src = dir.sqlite("src.db");
    noisy(&src, 300);
    let c = sealed(&src, &dir, "b.obbk", &keys(KA));
    // The attacker rewrites BOTH files: damages the data, then recomputes every checksum, the
    // sizes and the chain in the manifest. Only the MAC needs the key, and cannot be made.
    let len = std::fs::metadata(&c.data_path).unwrap().len() as usize;
    flip(&c.data_path, len - 40, 3);
    let mut j = manifest_json(&c);
    let (sha, bytes) = file_sha256(&c.data_path).unwrap();
    j["data_sha256"] = J::from(sha);
    j["data_bytes"] = J::from(bytes);
    let (chunks, head) = stream::scan(
        BufReader::new(File::open(&c.data_path).unwrap()),
        stream::CHUNK,
    )
    .unwrap();
    j["encryption"]["chunks"] = J::from(chunks);
    j["encryption"]["chain_head"] = J::from(head);
    write_manifest(&c, &j);
    let e = verify_with(&c.data_path, &keys(KA)).unwrap_err();
    assert!(e.contains("does not authenticate"), "{e}");
    // The structure check passes (it is only a structure check) and says it proved nothing.
    let v = verify_structure(&c.data_path).unwrap();
    assert!(!v.authenticated);
    // A whole sealed backup made by someone else, with their own key: valid in itself, refused
    // by the holder of KA because the key is not theirs.
    let other = dir.sqlite("other.db");
    seed(&other);
    let theirs = sealed(&other, &dir, "theirs.obbk", &keys(KB));
    let e = verify_with(&theirs.data_path, &keys(KA)).unwrap_err();
    assert!(e.starts_with(KEY_REQUIRED), "{e}");
    // Their data file under our manifest: the header disagrees with the manifest.
    let ours = sealed(&src, &dir, "ours.obbk", &keys(KA));
    std::fs::copy(&theirs.data_path, &ours.data_path).unwrap();
    assert!(verify_with(&ours.data_path, &keys(KA)).is_err());
}

#[test]
fn truncated_and_extended_sealed_backups_are_detected_even_when_the_manifest_agrees() {
    let dir = Dir::new();
    let src = dir.sqlite("src.db");
    noisy(&src, 400);
    let c = sealed(&src, &dir, "b.obbk", &keys(KA));
    let bytes = std::fs::read(&c.data_path).unwrap();
    let manifest = std::fs::read(&c.manifest_path).unwrap();
    // The end of the first chunk: 8 magic + 4 + header, then 4 + (64 KiB + 16).
    let header_len = u32::from_be_bytes(bytes[8..12].try_into().unwrap()) as usize;
    let first_end = 12 + header_len + 4 + stream::CHUNK + 16;
    assert!(bytes.len() > first_end + 20, "more than one chunk");
    // Cut exactly at a chunk boundary and make the manifest tell the truth about the shorter
    // file: the cipher still knows the new last chunk was not the last.
    std::fs::write(&c.data_path, &bytes[..first_end]).unwrap();
    resign(&c, &keys(KA));
    let e = verify_with(&c.data_path, &keys(KA)).unwrap_err();
    assert!(e.contains("truncated") || e.contains("authenticate"), "{e}");
    // Cut inside a chunk: no longer even a valid framing.
    std::fs::write(&c.data_path, &bytes[..first_end + 30]).unwrap();
    assert!(verify_with(&c.data_path, &keys(KA)).is_err());
    // Bytes appended after the last chunk, the manifest made to agree: the old last chunk now
    // claims to be the last while something follows it.
    let mut more = bytes.clone();
    more.extend_from_slice(&[0, 0, 0, 20]);
    more.extend_from_slice(&[7u8; 20]);
    std::fs::write(&c.data_path, &more).unwrap();
    assert!(
        verify_with(&c.data_path, &keys(KA)).is_err(),
        "the manifest disagrees"
    );
    // The same with the manifest made to agree with the longer file: only the cipher can still
    // tell, and it says the file was extended.
    resign(&c, &keys(KA));
    let e = verify_with(&c.data_path, &keys(KA)).unwrap_err();
    assert!(e.contains("extended"), "{e}");
    std::fs::write(&c.manifest_path, &manifest).unwrap();
    std::fs::write(&c.data_path, &bytes).unwrap();
    verify_with(&c.data_path, &keys(KA)).unwrap();
}

#[test]
fn a_backup_sealed_with_an_old_key_opens_while_that_key_is_kept_and_names_it_when_it_is_not() {
    let dir = Dir::new();
    let src = dir.sqlite("src.db");
    seed(&src);
    let c = sealed(&src, &dir, "old.obbk", &keys(KA));
    // After a rotation B is current and A is previous: the old backup still opens, a new one
    // is sealed with B.
    let rotated = Keys::from_ring(KeyRing::from_secrets(KB, &[KA]).unwrap());
    verify_with(&c.data_path, &rotated).unwrap();
    let fresh = sealed(&src, &dir, "new.obbk", &rotated);
    assert_eq!(fresh.manifest.encryption.unwrap().key_id, key_id(KB));
    // Without A the old backup says which key it needs.
    let e = verify_with(&c.data_path, &keys(KB)).unwrap_err();
    assert!(
        e.starts_with(KEY_REQUIRED) && e.contains(&key_id(KA)),
        "{e}"
    );
}

#[test]
fn keys_come_from_the_backup_key_first_then_the_server_key_but_never_a_generated_one() {
    let server = ring(KA);
    // No backup key: the server key seals.
    let k = Keys::from_vars(None, None, &server).unwrap();
    assert_eq!(
        (k.can_seal(), k.from(), k.current_id()),
        (true, KeysFrom::ServerKey, Some(key_id(KA)))
    );
    assert_eq!(k.describe(), "the server key (OBOP_SECRET_KEY)");
    // A backup key wins; the server key stays in the ring to open older backups.
    let k = Keys::from_vars(Some(KB), Some(""), &server).unwrap();
    assert_eq!(
        (k.from(), k.current_id()),
        (KeysFrom::BackupKey, Some(key_id(KB)))
    );
    assert!(k.ring.find(&key_id(KA)).is_some());
    // Previous backup keys are kept for opening.
    let k = Keys::from_vars(Some(KB), Some(&format!(" {KA} ")), &KeyRing::default()).unwrap();
    assert!(k.ring.find(&key_id(KA)).is_some());
    // Nothing configured, or only a key generated into the database: no sealing, because that
    // key is inside every backup and could not open one after a total loss.
    let b = obop_storage::open("memory:").unwrap();
    let generated = KeyRing::for_backend(&b).unwrap();
    assert!(!generated.is_empty());
    for s in [KeyRing::default(), generated] {
        let k = Keys::from_vars(None, None, &s).unwrap();
        assert!(
            !k.can_seal() && k.from() == KeysFrom::Nowhere,
            "{}",
            k.describe()
        );
    }
    // A too-short key is refused with the name of the variable, not the key.
    let e = Keys::from_vars(Some("short"), None, &server).err().unwrap();
    assert!(
        e.contains("OBOP_BACKUP_KEY") && !e.contains("short key text"),
        "{e}"
    );
    // Without keys a backup is format 1, as before.
    let dir = Dir::new();
    let src = dir.sqlite("src.db");
    seed(&src);
    let c = create_sealed(&src, None, 2, &dir.path("p"), &Keys::none(), &mut |_| {}).unwrap();
    assert_eq!(c.manifest.format, 1);
    assert!(c.manifest.encryption.is_none());
}

#[test]
fn format_one_backups_still_restore_and_are_never_called_authenticated() {
    let dir = Dir::new();
    let src = dir.sqlite("src.db");
    seed(&src);
    let c = make_plain(&src, &dir);
    assert_eq!(c.manifest.format, 1);
    // Without a key configured a plain backup is checked and restored as it always was.
    let v = verify_with(&c.data_path, &Keys::none()).unwrap();
    assert!(!v.authenticated);
    let dst = dir.sqlite("dst.db");
    // With a key configured it is refused (it could be a forgery: see review_tests) unless the
    // operator vouches for it, and even then it is never called authenticated.
    let vouched = keys(KA).allowing_plain(true);
    let v = verify_with(&c.data_path, &vouched).unwrap();
    assert!(!v.authenticated);
    restore_sealed(
        &dst,
        None,
        2,
        &c.data_path,
        RestoreOptions::default(),
        &vouched,
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(all(&dst), all(&src));
    // And a plain backup's file is the same bytes as before this change (gzip of JSON lines).
    assert!(
        std::fs::read(&c.data_path)
            .unwrap()
            .starts_with(&[0x1f, 0x8b])
    );
}

fn make_plain(src: &Arc<dyn Backend>, dir: &Dir) -> Created {
    create(src, 2, &dir.path("plain.jsonl.gz"), &mut |_| {}).unwrap()
}

#[test]
fn a_sealed_backup_without_its_key_can_still_be_checked_for_integrity() {
    let dir = Dir::new();
    let src = dir.sqlite("src.db");
    noisy(&src, 300);
    let c = sealed(&src, &dir, "b.obbk", &keys(KA));
    let v = verify_structure(&c.data_path).unwrap();
    assert!(!v.authenticated && v.manifest.sealed());
    flip(&c.data_path, 100, 1);
    assert!(verify_structure(&c.data_path).is_err());
}

#[test]
fn a_large_sealed_backup_is_streamed_in_chunks_and_restores() {
    let dir = Dir::new();
    let src = dir.sqlite("big.db");
    noisy(&src, 3000);
    let mut ticks = Vec::new();
    let c = create_sealed(&src, None, 2, &dir.path("big.obbk"), &keys(KA), &mut |n| {
        ticks.push(n)
    })
    .unwrap();
    assert_eq!(ticks, [1000, 2000, 3000]);
    let chunks = c.manifest.encryption.as_ref().unwrap().chunks;
    assert!(chunks > 10, "{chunks} chunks of at most 64 KiB");
    let dst = dir.sqlite("big2.db");
    restore_sealed(
        &dst,
        None,
        2,
        &c.data_path,
        RestoreOptions::default(),
        &keys(KA),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(dst.count("item").unwrap(), 3000);
    assert_eq!(all(&dst), all(&src));
}

#[test]
fn uploaded_files_travel_inside_the_sealed_stream() {
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
    let c = create_sealed(
        &src,
        Some(&blobs),
        2,
        &dir.path("b.obbk"),
        &keys(KA),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(c.manifest.blobs, 1);
    assert_eq!(count_in_file(&c.data_path, &png[..64]).unwrap(), 0);
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
    assert_eq!(fresh.read_all(&info.id, 1 << 20).unwrap().unwrap(), png);
}

#[test]
fn times_round_trip_through_rfc3339() {
    for t in [0, 951_782_400, 1_800_000_000, 1_792_000_000, 4_102_444_800] {
        assert_eq!(parse_rfc3339(&rfc3339(t)), Some(t), "{t}");
    }
    assert_eq!(parse_rfc3339("2027-01-15T08:00:00Z"), Some(1_800_000_000));
    for bad in [
        "",
        "2027-01-15",
        "2027-13-15T08:00:00Z",
        "2027-01-15 08:00:00Z",
        "x",
    ] {
        assert_eq!(parse_rfc3339(bad), None, "{bad}");
    }
}

#[test]
fn a_sealed_backup_moves_between_postgres_and_sqlite_when_a_server_is_given() {
    let Ok(base) = std::env::var("OBOP_TEST_POSTGRES_URL") else {
        eprintln!("skipped: set OBOP_TEST_POSTGRES_URL to run against PostgreSQL");
        return;
    };
    let dir = Dir::new();
    let (a, b) = (PgScratch::new(&base), PgScratch::new(&base));
    let pg_a = obop_storage::open(&a.url).unwrap();
    seed(&pg_a);
    noisy(&pg_a, 200);
    let c = sealed(&pg_a, &dir, "pg.obbk", &keys(KA));
    assert_eq!(c.manifest.source, "postgres");
    assert!(verify_with(&c.data_path, &keys(KA)).unwrap().authenticated);
    let pg_b = obop_storage::open(&b.url).unwrap();
    restore_sealed(
        &pg_b,
        None,
        2,
        &c.data_path,
        RestoreOptions::default(),
        &keys(KA),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(all(&pg_b), all(&pg_a));
    let lite = dir.sqlite("lite.db");
    restore_sealed(
        &lite,
        None,
        2,
        &c.data_path,
        RestoreOptions::default(),
        &keys(KA),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(all(&lite), all(&pg_a));
}
