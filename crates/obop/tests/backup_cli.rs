//! The command line of sealed backups with the real binary (D73, OBR-DAT-046): a key in the
//! environment makes a backup format 2; without it a restore stops with exit status 3 and the code
//! `restore_key_required` and writes nothing; a planted canary is not in the file; a plain
//! backup still restores. No database server is needed.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const KEY_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const KEY_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Dir {
        let d = std::env::temp_dir().join(format!("obop-cli-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Dir(d)
    }
    fn p(&self, n: &str) -> PathBuf {
        self.0.join(n)
    }
    fn url(&self, n: &str) -> String {
        format!("sqlite://{}", self.p(n).display())
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn obop(env: &[(&str, &str)]) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_obop"));
    c.env_remove("OBOP_DATABASE_URL")
        .env_remove("OBOP_SECRET_KEY")
        .env_remove("OBOP_SECRET_KEY_PREVIOUS")
        .env_remove("OBOP_BACKUP_KEY")
        .env_remove("OBOP_BACKUP_KEY_PREVIOUS")
        .env_remove("OBOP_DEV_INSECURE_KEY")
        .env("OBOP_REQUIRE_MFA_FOR_ADMINS", "0")
        .envs(env.iter().copied());
    c
}

fn run(env: &[(&str, &str)], args: &[&str]) -> Output {
    obop(env).args(args).output().expect("run obop")
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

/// A database with one account.
fn seeded(dir: &Dir, name: &str) -> String {
    let url = dir.url(name);
    let mut child = obop(&[])
        .args(["admin", "create", "ada", "--all-groups", "--db", &url])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"a-twelve-plus-char-password\n")
        .unwrap();
    assert!(child.wait().unwrap().success());
    url
}

fn only_file(dir: &Path, ext: &str) -> PathBuf {
    let mut it = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.to_string_lossy().ends_with(ext));
    let f = it.next().unwrap_or_else(|| panic!("no *{ext} in {dir:?}"));
    assert!(it.next().is_none(), "more than one *{ext}");
    f
}

#[test]
fn a_key_in_the_environment_makes_the_backup_sealed_and_restore_needs_it() {
    let dir = Dir::new("sealed");
    let url = seeded(&dir, "src.db");
    let out = dir.p("backups");
    let o = run(
        &[("OBOP_BACKUP_KEY", KEY_A)],
        &["backup", "--db", &url, "--out", out.to_str().unwrap()],
    );
    assert!(o.status.success(), "{}", text(&o));
    let t = text(&o);
    assert!(t.contains("format 2") && t.contains("AUTHENTICATED"), "{t}");
    let file = only_file(&out, ".obbk");
    assert!(
        !t.contains(KEY_A)
            && !std::fs::read_to_string(file.with_extension("obbk.manifest.json"))
                .unwrap_or_default()
                .contains(KEY_A)
    );

    // Verify with the key: authenticated. Without it: exit 3 and the stable code.
    let v = run(
        &[("OBOP_BACKUP_KEY", KEY_A)],
        &[
            "backup",
            "verify",
            file.to_str().unwrap(),
            "--canary",
            "ada",
        ],
    );
    assert!(v.status.success(), "{}", text(&v));
    assert!(
        text(&v).contains("canary scan: 0 occurrence"),
        "{}",
        text(&v)
    );
    let v = run(&[], &["backup", "verify", file.to_str().unwrap()]);
    assert_eq!(v.status.code(), Some(3), "{}", text(&v));
    assert!(text(&v).contains("restore_key_required"), "{}", text(&v));
    let v = run(
        &[("OBOP_BACKUP_KEY", KEY_B)],
        &["backup", "verify", file.to_str().unwrap()],
    );
    assert_eq!(v.status.code(), Some(3), "{}", text(&v));
    // Structure only, no key: says it proved nothing about who wrote it.
    let v = run(
        &[],
        &[
            "backup",
            "verify",
            file.to_str().unwrap(),
            "--structure-only",
        ],
    );
    assert!(
        v.status.success() && text(&v).contains("NOT authenticated"),
        "{}",
        text(&v)
    );

    // Restore without the key: exit 3, and nothing is written (the database is not even made).
    let dst = dir.url("dst.db");
    let r = run(
        &[],
        &["restore", "--db", &dst, "--from", file.to_str().unwrap()],
    );
    assert_eq!(r.status.code(), Some(3), "{}", text(&r));
    assert!(text(&r).contains("restore_key_required"), "{}", text(&r));
    assert!(
        !Path::new(dir.p("dst.db").to_str().unwrap()).exists(),
        "a refused restore does not even make the target database"
    );
    // With the key it restores, and the account is there.
    let r = run(
        &[("OBOP_BACKUP_KEY", KEY_A)],
        &["restore", "--db", &dst, "--from", file.to_str().unwrap()],
    );
    assert!(r.status.success(), "{}", text(&r));
    let u = run(&[], &["admin", "users", "--db", &dst]);
    assert!(text(&u).contains("ada"), "{}", text(&u));
}

#[test]
fn the_server_key_seals_when_there_is_no_backup_key_and_no_key_at_all_means_a_plain_backup() {
    let dir = Dir::new("server");
    let url = seeded(&dir, "src.db");
    // The server key seals: format 2.
    let out = dir.p("a");
    let o = run(
        &[("OBOP_SECRET_KEY", KEY_A)],
        &["backup", "--db", &url, "--out", out.to_str().unwrap()],
    );
    assert!(o.status.success(), "{}", text(&o));
    only_file(&out, ".obbk");
    // No key: a plain backup, with the warning; --encrypted refuses; --plain is explicit.
    let out = dir.p("b");
    let o = run(
        &[],
        &["backup", "--db", &url, "--out", out.to_str().unwrap()],
    );
    assert!(o.status.success(), "{}", text(&o));
    assert!(text(&o).contains("NOT encrypted"), "{}", text(&o));
    let plain = only_file(&out, ".gz");
    let o = run(
        &[],
        &[
            "backup",
            "--db",
            &url,
            "--out",
            dir.p("c").to_str().unwrap(),
            "--encrypted",
        ],
    );
    assert!(
        !o.status.success() && text(&o).contains("--encrypted needs a key"),
        "{}",
        text(&o)
    );
    // A key is configured but --plain asks for format 1.
    let out = dir.p("d");
    let o = run(
        &[("OBOP_BACKUP_KEY", KEY_A)],
        &[
            "backup",
            "--db",
            &url,
            "--out",
            out.to_str().unwrap(),
            "--plain",
        ],
    );
    assert!(
        o.status.success() && !text(&o).contains("AUTHENTICATED"),
        "{}",
        text(&o)
    );
    only_file(&out, ".gz");
    // A format 1 backup restores when no key is configured (it always did) ...
    let dst0 = dir.url("p0.db");
    let r = run(
        &[],
        &["restore", "--db", &dst0, "--from", plain.to_str().unwrap()],
    );
    assert!(r.status.success(), "{}", text(&r));
    assert!(text(&r).contains("NOT authenticated"), "{}", text(&r));
    // ... but where a key is configured it is refused (exit 4): a plain pair can be made by
    // anyone who can write the directory. --allow-plain says the operator vouches for it.
    let dst = dir.url("p.db");
    let r = run(
        &[("OBOP_BACKUP_KEY", KEY_B)],
        &["restore", "--db", &dst, "--from", plain.to_str().unwrap()],
    );
    assert_eq!(r.status.code(), Some(4), "{}", text(&r));
    assert!(text(&r).contains("backup_not_sealed"), "{}", text(&r));
    assert!(!dir.p("p.db").exists(), "nothing was made");
    let v = run(
        &[("OBOP_BACKUP_KEY", KEY_B)],
        &["backup", "verify", plain.to_str().unwrap()],
    );
    assert_eq!(v.status.code(), Some(4), "{}", text(&v));
    let r = run(
        &[("OBOP_BACKUP_KEY", KEY_B)],
        &[
            "restore",
            "--db",
            &dst,
            "--from",
            plain.to_str().unwrap(),
            "--allow-plain",
        ],
    );
    assert!(r.status.success(), "{}", text(&r));
    assert!(text(&r).contains("NOT authenticated"), "{}", text(&r));
    // A key that is only the development key does not seal.
    let out = dir.p("e");
    let o = run(
        &[("OBOP_DEV_INSECURE_KEY", "1")],
        &["backup", "--db", &url, "--out", out.to_str().unwrap()],
    );
    assert!(o.status.success(), "{}", text(&o));
    only_file(&out, ".gz");
}

#[test]
fn a_canary_scan_of_a_plain_backup_fails_and_a_sealed_one_cannot_be_given_a_gz_name() {
    let dir = Dir::new("canary");
    let url = seeded(&dir, "src.db");
    let out = dir.p("plain");
    let o = run(
        &[],
        &["backup", "--db", &url, "--out", out.to_str().unwrap()],
    );
    assert!(o.status.success(), "{}", text(&o));
    let plain = only_file(&out, ".gz");
    // `gunzip` gives anyone the records: the scan says so and fails, though a scan of the file as
    // stored finds nothing.
    let v = run(
        &[],
        &[
            "backup",
            "verify",
            plain.to_str().unwrap(),
            "--canary",
            "ada",
        ],
    );
    assert!(!v.status.success(), "{}", text(&v));
    assert!(
        text(&v).contains("after decompression") && text(&v).contains("not encrypted"),
        "{}",
        text(&v)
    );
    // A sealed backup is not given a gz name: that would be a file gzip cannot read.
    let o = run(
        &[("OBOP_BACKUP_KEY", KEY_A)],
        &[
            "backup",
            "--db",
            &url,
            "--out",
            dir.p("nightly.jsonl.gz").to_str().unwrap(),
        ],
    );
    assert!(!o.status.success(), "{}", text(&o));
    assert!(
        text(&o).contains(".obbk") && text(&o).contains("--plain"),
        "{}",
        text(&o)
    );
    assert!(!dir.p("nightly.jsonl.gz").exists());
}

#[test]
fn a_prune_in_a_shell_without_the_sink_removes_nothing_the_sink_does_not_hold_and_the_dry_run_agrees()
 {
    let dir = Dir::new("prune");
    let url = seeded(&dir, "src.db");
    let (bk, off) = (dir.p("bk"), dir.p("off"));
    let job = |extra: &[&str]| {
        let mut a = vec![
            "backup",
            "run",
            "--db",
            &url,
            "--out",
            bk.to_str().unwrap(),
            "--plain",
            "--keep-monthly",
            "0",
            "--max-age-days",
            "0",
        ];
        a.extend_from_slice(extra);
        run(&[], &a)
    };
    // A is copied off the host. B's run is given a sink that cannot take a copy (a file where a
    // directory should be), so A, now past retention (one backup is kept per day), has no
    // verified copy in every configured sink and stays, while the catalogue records a copy of it.
    let broken = dir.p("not-a-directory");
    std::fs::write(&broken, b"x").unwrap();
    let o = job(&["--keep-daily", "5", "--copy-to", off.to_str().unwrap()]);
    assert!(o.status.success(), "{}", text(&o));
    std::thread::sleep(std::time::Duration::from_millis(1200));
    let o = job(&["--keep-daily", "5", "--copy-to", broken.to_str().unwrap()]);
    assert!(
        !o.status.success() && text(&o).contains("COPY FAILED"),
        "{}",
        text(&o)
    );
    let listed = text(&run(&[], &["backup", "list", "--db", &url]));
    assert_eq!(listed.matches("format 1").count(), 2, "{listed}");
    let args = [
        "backup",
        "prune",
        "--db",
        &url,
        "--keep-daily",
        "1",
        "--keep-monthly",
        "0",
        "--max-age-days",
        "0",
    ];
    // A shell that was not given the sink: the dry run and the real run agree, and remove
    // nothing, because the catalogue records a copy off the host that this shell cannot see.
    let mut dry = args.to_vec();
    dry.push("--dry-run");
    let d = run(&[], &dry);
    assert!(
        text(&d).contains("would remove 0") && text(&d).contains("no sink"),
        "{}",
        text(&d)
    );
    let r = run(&[], &args);
    assert!(text(&r).contains("0 backup(s) removed"), "{}", text(&r));
    // With the operator's word the dry run lists what a real run would remove.
    let mut dry = args.to_vec();
    dry.extend(["--no-sinks", "--dry-run"]);
    let d = run(&[], &dry);
    assert!(text(&d).contains("would remove"), "{}", text(&d));
    assert!(!text(&d).contains("would remove 0"), "{}", text(&d));
    // The timer and `backup run` are the operator's standing configuration: a run with no sink
    // given retains by that, and does not wait for the copy an earlier run recorded.
    std::thread::sleep(std::time::Duration::from_millis(1200));
    let o = job(&["--keep-daily", "1"]);
    assert!(o.status.success(), "{}", text(&o));
    assert!(
        text(&o).contains("no sink is configured here"),
        "{}",
        text(&o)
    );
    assert!(text(&o).contains("removed obop-backup-"), "{}", text(&o));
    let listed = text(&run(&[], &["backup", "list", "--db", &url]));
    assert_eq!(listed.matches("format 1").count(), 1, "{listed}");
    // A copy into the very directory the backup is in is not an off-host copy.
    let o = run(
        &[],
        &[
            "backup",
            "run",
            "--db",
            &url,
            "--out",
            dir.p("same").to_str().unwrap(),
            "--copy-to",
            dir.p("same").to_str().unwrap(),
            "--plain",
        ],
    );
    assert!(!o.status.success(), "{}", text(&o));
    assert!(
        text(&o).contains("is the directory the backup is in"),
        "{}",
        text(&o)
    );
}
