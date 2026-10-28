//! `obop openapi` through the real binary: the bytes do not depend on the environment, the working
//! directory, the user, the time zone or the address the instance is configured with. The unit
//! tests in `src/openapi_tests.rs` look at the document in process; this one runs the program in
//! two very different surroundings and compares what it wrote.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A scratch directory, removed when the value is dropped (bind it to a name).
struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Dir {
        let d = std::env::temp_dir().join(format!("obop-openapi-{tag}-{}", std::process::id()));
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

/// Words that no part of the document may contain: they only occur in the surroundings below.
const MARKERS: [&str; 6] = [
    "openapi-marker-user",
    "openapi-marker-home",
    "openapi-marker-cwd",
    "openapi-marker-tmp",
    "openapi-marker.example",
    "Asia/Tokyo",
];

/// `obop openapi` in `cwd` with nothing from the caller's environment but what the operating
/// system needs to start a program.
fn command(cwd: &Path, args: &[&str]) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_obop"));
    c.env_clear().current_dir(cwd).arg("openapi").args(args);
    for keep in ["SystemRoot", "SYSTEMROOT", "PATH"] {
        if let Ok(v) = std::env::var(keep) {
            c.env(keep, v);
        }
    }
    c
}

fn bare(cwd: &Path, args: &[&str]) -> Output {
    command(cwd, args).output().unwrap()
}

/// The same program in a surrounding with a user name, a home, a temp directory, a time zone, a
/// language, a public address and the instance settings all set to something else.
fn busy(cwd: &Path, args: &[&str]) -> Output {
    let mut c = command(cwd, args);
    let home = cwd.join("openapi-marker-home");
    let tmp = cwd.join("openapi-marker-tmp");
    for (k, v) in [
        ("USER", "openapi-marker-user"),
        ("USERNAME", "openapi-marker-user"),
        ("LOGNAME", "openapi-marker-user"),
        ("TZ", "Asia/Tokyo"),
        ("LANG", "fi_FI.UTF-8"),
        ("LC_ALL", "fi_FI.UTF-8"),
        ("OBOP_ZONE", "Asia/Tokyo"),
        ("OBOP_PUBLIC_URL", "https://openapi-marker.example"),
        ("OBOP_DEFAULT_LANGUAGE", "sv"),
        ("OBOP_DEMO_PAYMENTS", "1"),
        ("OBOP_BULK_PER_MINUTE", "3"),
        ("OBOP_ALLOWED_ORIGINS", "https://openapi-marker.example"),
    ] {
        c.env(k, v);
    }
    for (k, p) in [
        ("HOME", &home),
        ("USERPROFILE", &home),
        ("TMPDIR", &tmp),
        ("TEMP", &tmp),
        ("TMP", &tmp),
    ] {
        c.env(k, p);
    }
    c.output().unwrap()
}

fn ok(o: &Output) -> &[u8] {
    assert!(
        o.status.success(),
        "obop openapi failed: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    &o.stdout
}

#[test]
fn the_bytes_do_not_depend_on_the_environment_or_the_working_directory() {
    let quiet = Dir::new("cwd-a");
    let loud = Dir::new("openapi-marker-cwd");
    std::fs::create_dir_all(loud.0.join("openapi-marker-home")).unwrap();
    std::fs::create_dir_all(loud.0.join("openapi-marker-tmp")).unwrap();
    let a = bare(&quiet.0, &[]);
    let b = busy(&loud.0, &[]);
    let (a, b) = (ok(&a), ok(&b));
    assert!(a.len() > 100_000, "{} bytes", a.len());
    assert_eq!(a, b, "the document changed with the surroundings");
    let text = std::str::from_utf8(a).unwrap();
    for marker in MARKERS {
        assert!(!text.contains(marker), "{marker} reached the document");
    }
    assert!(text.ends_with("}\n") && !text.contains('\r'));
}

#[test]
fn out_writes_the_same_bytes_as_standard_output_and_leaves_standard_output_empty() {
    let work = Dir::new("out");
    let file = work.0.join("openapi.json");
    let written = busy(&work.0, &["--out", file.to_str().unwrap()]);
    assert!(ok(&written).is_empty(), "nothing is printed with --out");
    let printed = bare(&work.0, &[]);
    assert_eq!(std::fs::read(&file).unwrap(), ok(&printed));
}

#[test]
fn a_path_that_cannot_be_written_fails_with_a_message_and_a_status() {
    let work = Dir::new("bad");
    let nowhere = work.0.join("missing").join("openapi.json");
    let o = bare(&work.0, &["--out", nowhere.to_str().unwrap()]);
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("cannot write"));
}
