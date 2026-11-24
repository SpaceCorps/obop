//! The e-invoice sample sets (testdata/e-invoices, docs/LOCALIZATION.md 1.6 and 4.3) are what they
//! say they are: every committed sample is listed in `provenance.json` with its SHA-256, every own
//! sample is marked fictitious and has an entry in `expected.json`, and the fetched sets (CEN and
//! OpenPeppol, which are not committed) match the hashes pinned in `fetch.txt`.
//!
//! The fetched files are read from `target/e-invoice-samples` (or `OBOP_EINVOICE_SAMPLES_DIR`),
//! where `deploy/fetch-einvoice-samples.sh` puts them. When they are missing the fetched-set test
//! says so and skips; with `OBOP_REQUIRE_EINVOICE_SAMPLES=1` (CI) a missing file fails it. A file
//! that is there with the wrong bytes always fails.
//!
//! These tests live here until the `edi` crate exists (issue #2 chooses its home); its readers will
//! use the same sets.

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn samples() -> PathBuf {
    repo().join("testdata/e-invoices")
}

fn provenance() -> Value {
    let text = std::fs::read_to_string(samples().join("provenance.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn sets(p: &Value) -> &Vec<Value> {
    p["sets"].as_array().expect("provenance.json: sets")
}

fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key]
        .as_str()
        .unwrap_or_else(|| panic!("provenance.json: {key} of {v}"))
}

fn sha256(path: &Path) -> String {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Every `.xml` under `dir`, as paths relative to `dir` with `/` separators.
fn xml_under(dir: &Path) -> BTreeSet<String> {
    fn walk(base: &Path, dir: &Path, out: &mut BTreeSet<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(base, &path, out);
            } else if path.extension().is_some_and(|e| e == "xml") {
                let rel = path.strip_prefix(base).unwrap();
                out.insert(
                    rel.components()
                        .map(|c| c.as_os_str().to_string_lossy())
                        .collect::<Vec<_>>()
                        .join("/"),
                );
            }
        }
    }
    let mut out = BTreeSet::new();
    walk(dir, dir, &mut out);
    out
}

#[test]
fn every_set_names_its_authority_licence_and_retrieval() {
    let p = provenance();
    let ids: Vec<&str> = sets(&p).iter().map(|s| text(s, "id")).collect();
    assert_eq!(ids, ["kosit", "own", "cen", "peppol"]);
    for s in sets(&p) {
        for key in ["name", "authority", "url", "licence", "retrieved", "mode"] {
            assert!(!text(s, key).is_empty(), "{key} of {s}");
        }
        let mode = text(s, "mode");
        match mode {
            // What is committed must be redistributable under the repository's own licence (OR-1).
            "vendored" | "own" => assert_eq!(text(s, "licence"), "Apache-2.0", "{s}"),
            "fetched" => assert_eq!(text(s, "files_list"), "fetch.txt", "{s}"),
            _ => panic!("unknown mode {mode}"),
        }
        if mode != "own" {
            // A third-party set is pinned to a tag and the commit it pointed at.
            assert!(!text(s, "tag").is_empty(), "{s}");
            let commit = text(s, "commit");
            assert!(
                commit.len() == 40 && commit.chars().all(|c| c.is_ascii_hexdigit()),
                "{s}"
            );
        }
    }
    // A vendored third-party set carries its licence text, and NOTICE names it.
    let kosit = &sets(&p)[0];
    assert!(samples().join(text(kosit, "licence_file")).is_file());
    let notice = std::fs::read_to_string(repo().join("NOTICE")).unwrap();
    assert!(notice.contains("testdata/e-invoices/kosit"), "NOTICE");
}

#[test]
fn every_committed_sample_is_listed_with_its_hash() {
    let p = provenance();
    let mut listed = BTreeSet::new();
    for s in sets(&p).iter().filter(|s| s["files"].is_array()) {
        let dir = text(s, "dir");
        for f in s["files"].as_array().unwrap() {
            let rel = format!("{dir}/{}", text(f, "path"));
            let got = sha256(&samples().join(&rel));
            assert_eq!(
                got,
                text(f, "sha256"),
                "{rel} changed: update provenance.json only if the change is meant"
            );
            listed.insert(rel);
        }
    }
    // Nothing committed that the manifest does not account for.
    let on_disk = xml_under(&samples());
    let unlisted: Vec<_> = on_disk.difference(&listed).collect();
    assert!(unlisted.is_empty(), "not in provenance.json: {unlisted:?}");
    assert_eq!(listed.len(), 61, "48 KoSIT cases and 13 own files");
}

#[test]
fn own_samples_are_marked_fictitious_and_have_expectations() {
    let own = samples().join("own");
    let expected: Value =
        serde_json::from_str(&std::fs::read_to_string(own.join("expected.json")).unwrap()).unwrap();
    let described: BTreeSet<String> = expected
        .as_array()
        .unwrap()
        .iter()
        .map(|e| text(e, "file").to_string())
        .collect();
    let files = xml_under(&own);
    assert_eq!(
        files, described,
        "own/*.xml and the entries of expected.json"
    );
    for f in &files {
        let body = std::fs::read_to_string(own.join(f)).unwrap();
        assert!(body.contains("FAKE TEST DATA"), "{f} is not marked as fake");
    }
    // Amounts are minor units, and a valid invoice adds up: lines to net, groups to VAT.
    for e in expected.as_array().unwrap() {
        let fields = &e["fields"];
        if fields.is_null() || !text(e, "expect").starts_with("ok") {
            continue;
        }
        let lines: i64 = fields["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["net_minor"].as_i64().unwrap())
            .sum();
        let net = fields["net_minor"].as_i64().unwrap();
        let vat = fields["vat_minor"].as_i64().unwrap();
        assert_eq!(lines, net, "{}", text(e, "file"));
        assert_eq!(
            net + vat,
            fields["gross_minor"].as_i64().unwrap(),
            "{}",
            text(e, "file")
        );
        if let Some(groups) = fields["vat_breakdown"].as_array() {
            let sum: i64 = groups
                .iter()
                .map(|g| g["vat_minor"].as_i64().unwrap())
                .sum();
            assert_eq!(sum, vat, "{}", text(e, "file"));
        }
    }
}

/// One line of `fetch.txt`.
struct Pinned {
    set: String,
    repo: String,
    commit: String,
    sha256: String,
    path: String,
}

fn fetch_list() -> Vec<Pinned> {
    let text = std::fs::read_to_string(samples().join("fetch.txt")).unwrap();
    text.lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|l| {
            // The path is the last field and may contain spaces.
            let mut it = l.splitn(5, ' ');
            let mut next = || {
                it.next()
                    .unwrap_or_else(|| panic!("fetch.txt: {l}"))
                    .to_string()
            };
            Pinned {
                set: next(),
                repo: next(),
                commit: next(),
                sha256: next(),
                path: next(),
            }
        })
        .collect()
}

#[test]
fn the_fetch_list_pins_the_fetched_sets_of_the_manifest() {
    let p = provenance();
    let fetched: BTreeMap<&str, &Value> = sets(&p)
        .iter()
        .filter(|s| text(s, "mode") == "fetched")
        .map(|s| (text(s, "id"), s))
        .collect();
    let list = fetch_list();
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for f in &list {
        let s = fetched.get(f.set.as_str()).unwrap_or_else(|| {
            panic!(
                "fetch.txt names set {} that provenance.json does not fetch",
                f.set
            )
        });
        assert_eq!(f.commit, text(s, "commit"), "{}", f.path);
        assert!(
            text(s, "url").ends_with(&format!("github.com/{}", f.repo)),
            "{}",
            f.path
        );
        assert!(f.sha256.len() == 64 && f.sha256.chars().all(|c| c.is_ascii_hexdigit()));
        *counts.entry(text(s, "id")).or_default() += 1;
    }
    assert_eq!(counts, BTreeMap::from([("cen", 32), ("peppol", 26)]));
}

#[test]
fn fetched_samples_match_their_pinned_hashes() {
    let dir = match std::env::var_os("OBOP_EINVOICE_SAMPLES_DIR") {
        Some(d) => repo().join(d),
        None => repo().join("target/e-invoice-samples"),
    };
    let required = std::env::var("OBOP_REQUIRE_EINVOICE_SAMPLES").is_ok_and(|v| v == "1");
    let mut missing = Vec::new();
    for f in fetch_list() {
        let path = dir.join(&f.set).join(&f.path);
        if !path.is_file() {
            missing.push(format!("{}/{}", f.set, f.path));
            continue;
        }
        assert_eq!(
            sha256(&path),
            f.sha256,
            "{} differs from the pinned file",
            path.display()
        );
    }
    if !missing.is_empty() {
        let msg = format!(
            "{} fetched e-invoice sample file(s) missing from {} (first: {}); run deploy/fetch-einvoice-samples.sh",
            missing.len(),
            dir.display(),
            missing[0]
        );
        assert!(!required, "{msg} (OBOP_REQUIRE_EINVOICE_SAMPLES=1)");
        eprintln!("skipped: {msg}");
    }
}
