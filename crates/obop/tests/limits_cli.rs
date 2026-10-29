//! `obop modules check` through the real binary on packages with field limits (D90): a good file
//! passes, and a bad one fails with the file, the model and field, and the key, so an author
//! knows where to look.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Dir {
        let d = std::env::temp_dir().join(format!("obop-limits-cli-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Dir(d)
    }
    fn file(&self, name: &str, text: &str) -> PathBuf {
        let p = self.0.join(name);
        std::fs::write(&p, text).unwrap();
        p
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn package(core: &str, field: &str) -> String {
    format!(
        r#"[module]
id = "lim"
version = "0.1.0"
name = "Lim"
description = "Limits"
license = "MIT"
requires_core = "{core}"

[[model]]
name = "lim_note"
description = "A note"
read_group = "base.user"
write_group = "base.user"

  [[model.field]]
  name = "target"
  description = "The field under test"
  class = "none"
{field}
"#
    )
}

fn check(files: &[&Path]) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_obop"));
    c.env_clear().args(["modules", "check"]).args(files);
    for keep in ["SystemRoot", "SYSTEMROOT", "PATH"] {
        if let Ok(v) = std::env::var(keep) {
            c.env(keep, v);
        }
    }
    c.output().unwrap()
}

fn said(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

#[test]
fn a_package_with_limits_passes_and_the_bundled_ones_still_do() {
    let dir = Dir::new("good");
    let good = dir.file(
        "good.toml",
        &package(
            "^0.2.1",
            "  max_length = 120\n  min_length = 2\n\n  [[model.field]]\n  name = \"qty\"\n  description = \"Quantity\"\n  class = \"none\"\n  kind = \"number\"\n  min = 0\n  max = 99",
        ),
    );
    let o = check(&[&good]);
    assert!(o.status.success(), "{}", said(&o));
    assert!(said(&o).contains("ok"), "{}", said(&o));
    // The shipped catalogue is untouched by the default length.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fleet = root.join("packages/fleet.toml");
    let o = check(&[&fleet]);
    assert!(o.status.success(), "{}", said(&o));
}

#[test]
fn a_bad_limit_fails_the_run_and_names_the_file_the_field_and_the_key() {
    let dir = Dir::new("bad");
    for (keys, want) in [
        ("  max_length = 0", "`max_length` must be at least 1"),
        (
            "  max_length = 10001",
            "above the largest a text field may declare",
        ),
        (
            "  min_length = 5\n  max_length = 4",
            "`min_length` 5 is above `max_length` 4",
        ),
        (
            "  kind = \"number\"\n  max_length = 4",
            "only valid for kind \"text\" or \"longtext\"",
        ),
        ("  kind = \"number\"\n  min = 0.5", "must be a whole number"),
        (
            "  kind = \"decimal\"\n  precision = 10\n  scale = 2\n  max = 1.5",
            "a TOML float is not exact",
        ),
        (
            "  kind = \"decimal\"\n  precision = 10\n  scale = 2\n  max = \"1.234\"",
            "is not a decimal that fits decimal(10, 2)",
        ),
        (
            "  kind = \"number\"\n  min = 5\n  max = 4",
            "`min` 5 is above `max` 4",
        ),
        ("  maxlength = 4", "unknown field"),
    ] {
        let file = dir.file("bad.toml", &package("^0.2.1", keys));
        let o = check(&[&file]);
        assert!(!o.status.success(), "{keys}: {}", said(&o));
        let out = said(&o);
        assert!(
            out.contains("FAIL") && out.contains("bad.toml"),
            "{keys}: {out}"
        );
        assert!(out.contains(want), "{keys}: wanted {want:?} in {out}");
        // The field is named (not for a key the parser does not know, which names its position).
        if !keys.contains("maxlength") {
            assert!(out.contains("lim_note.target"), "{keys}: {out}");
        }
    }
    // One bad file fails the run even next to a good one.
    let good = dir.file("good.toml", &package("^0.2.1", "  max_length = 10"));
    let bad = dir.file("bad.toml", &package("^0.2.1", "  max_length = 0"));
    let o = check(&[&good, &bad]);
    assert!(!o.status.success());
    let out = said(&o);
    assert!(out.contains("ok") && out.contains("FAIL"), "{out}");
}
