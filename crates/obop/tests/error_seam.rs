//! The CLI prints a failed call as it always did (the guide documents the line), now that the
//! text comes from `obop_commands::errors`, and says what the store did to it when the store
//! refused: the real binary, a SQLite file, no database server.
//!
//! A refusal is made to order: the test build of the store (`obop-storage` with `test-support`,
//! which the dev-dependencies of this crate switch on; a release build has none of it) reads
//! `OBOP_TEST_FAULTS=apply:SKIP[:N]` and refuses atomic batches `SKIP+1` to `SKIP+N` of the
//! process as `storage_busy`, behind the request guard, as the PostgreSQL pool would.

use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

/// A directory of its own, removed when it goes out of scope.
struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Dir {
        let d = std::env::temp_dir().join(format!("obop-seam-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Dir(d)
    }
    fn url(&self) -> String {
        format!("sqlite://{}", self.0.join("seam.db").display())
    }
    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn obop() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_obop"));
    c.env("OBOP_DEV_INSECURE_KEY", "1")
        .env("OBOP_REQUIRE_MFA_FOR_ADMINS", "0")
        .env_remove("OBOP_DATABASE_URL")
        .env_remove("OBOP_SECRET_KEY")
        .env_remove("OBOP_SECRET_KEY_PREVIOUS")
        .env_remove("OBOP_TEST_FAULTS");
    c
}

/// `obop call MODEL COMMAND ARGS...` against the database, with the faults of `faults` if given.
fn call(db: &Dir, faults: Option<&str>, args: &[&str]) -> Output {
    let mut c = obop();
    if let Some(f) = faults {
        c.env("OBOP_TEST_FAULTS", f);
    }
    c.arg("call")
        .args(args)
        .args(["--dev-auth", "--db", &db.url()])
        .output()
        .expect("run obop")
}

fn json(o: &Output) -> serde_json::Value {
    serde_json::from_slice(&o.stdout).unwrap_or_else(|e| {
        panic!(
            "not JSON ({e}): {}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        )
    })
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

fn error_line(o: &Output) -> String {
    stderr(o)
        .lines()
        .find(|l| l.starts_with("error: "))
        .unwrap_or_else(|| panic!("no error line: {}", stderr(o)))
        .to_string()
}

fn new_lead(db: &Dir, title: &str) -> String {
    let o = call(db, None, &["lead", "create", &format!("title={title}")]);
    assert!(o.status.success(), "{}", stderr(&o));
    json(&o)["id"].as_str().unwrap().to_string()
}

fn stage(db: &Dir, id: &str) -> String {
    let o = call(db, None, &["lead", "get", &format!("id={id}")]);
    assert!(o.status.success(), "{}", stderr(&o));
    json(&o)["stage"].as_str().unwrap().to_string()
}

#[test]
fn a_failed_call_prints_the_error_line_the_cli_guide_documents() {
    let out = obop()
        .args([
            "call",
            "lead",
            "get",
            "id=nope",
            "--dev-auth",
            "--db",
            "memory:",
        ])
        .output()
        .expect("run obop");
    assert!(!out.status.success());
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.lines().any(|l| l == "error: Handler(\"no lead nope\")"),
        "{err}"
    );
}

/// A call whose store refuses exits 75 and says what the refusal left: nothing changed when the
/// first batch was refused, part of it applied when the second was (the stage moves with the first
/// batch of `lead update stage=won`), nothing refused at all when the batch is past the last one.
#[test]
fn a_refused_call_of_the_binary_exits_75_and_says_what_was_changed() {
    let db = Dir::new("call");
    // A first, healthy run makes the database and its demo data.
    let first = call(&db, None, &["lead", "list", "limit=1"]);
    assert!(first.status.success(), "{}", stderr(&first));

    // The first batch refused: nothing was changed.
    let id = new_lead(&db, "Refused at once");
    let o = call(
        &db,
        Some("apply:0"),
        &["lead", "update", &format!("id={id}"), "stage=won"],
    );
    assert_eq!(o.status.code(), Some(75), "{}", stderr(&o));
    let line = error_line(&o);
    assert!(
        line.starts_with("error: storage_busy: ")
            && line.ends_with("(nothing was changed; try again in 1 s)"),
        "{line}"
    );
    assert_ne!(stage(&db, &id), "won");

    // The second batch refused: the first committed, and the line says so.
    let id = new_lead(&db, "Refused after the first batch");
    let o = call(
        &db,
        Some("apply:1"),
        &[
            "lead",
            "update",
            &format!("id={id}"),
            "stage=won",
            "contact_name=Zed",
        ],
    );
    assert_eq!(o.status.code(), Some(75), "{}", stderr(&o));
    let line = error_line(&o);
    assert!(
        line.starts_with("error: storage_busy: ")
            && line
                .ends_with("(part of it was applied: do not repeat it blindly; try again in 1 s)"),
        "{line}"
    );
    assert_eq!(stage(&db, &id), "won", "it said part of it was applied");

    // A refusal beyond the last batch of the call touches nothing: it succeeds.
    let id = new_lead(&db, "Never refused");
    let o = call(
        &db,
        Some("apply:5"),
        &["lead", "update", &format!("id={id}"), "stage=won"],
    );
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    assert_eq!(stage(&db, &id), "won");

    // A call of its own that fails is still the documented line and status 1, with a refusal armed
    // that the call never reaches: it is validated before any batch, and the armed refusal of the
    // process is for a batch that does not come. Exit 75 here would be the armed fault (or a text)
    // being taken for the call's failure.
    let id = new_lead(&db, "Not a stage");
    let o = call(
        &db,
        Some("apply:0"),
        &["lead", "update", &format!("id={id}"), "stage=not-a-stage"],
    );
    assert_eq!(o.status.code(), Some(1), "{}", stderr(&o));
    let line = error_line(&o);
    assert!(
        line.starts_with("error: Handler(\"") && !line.contains("storage_"),
        "{line}"
    );
    assert_ne!(stage(&db, &id), "not-a-stage");
    // That a text which begins with the word of a storage fault (or any other code of the table)
    // is not read as one is tested where a command that echoes its input first can be called: with
    // a token that may uninstall a module, in `error_seam_tests` of the binary's own crate
    // (`input_that_begins_with_a_code_word_cannot_forge_a_fault_or_a_row`). No command a dev token
    // may call puts the caller's text first, so there is nothing to ask the binary here.
}

/// `obop import` goes through the same seam: a refused batch is exit 75 and the line, not "the
/// rows are invalid" and not exit 1, and nothing is created; the same file imports when the store
/// does not refuse.
#[test]
fn a_refused_import_of_the_binary_exits_75_and_creates_nothing() {
    let db = Dir::new("import");
    let first = call(&db, None, &["lead", "list", "limit=1"]);
    assert!(first.status.success(), "{}", stderr(&first));
    let csv = db.file("leads.csv");
    std::fs::write(&csv, "title\nImported one\nImported two\n").unwrap();
    let import = |faults: Option<&str>| {
        let mut c = obop();
        if let Some(f) = faults {
            c.env("OBOP_TEST_FAULTS", f);
        }
        c.args(["import", "lead"])
            .arg(&csv)
            .args(["--dev-auth", "--db", &db.url()])
            .output()
            .expect("run obop")
    };
    let listed = || {
        let o = call(&db, None, &["lead", "list", "limit=500"]);
        String::from_utf8_lossy(&o.stdout).to_string()
    };

    let o = import(Some("apply:0"));
    assert_eq!(o.status.code(), Some(75), "{}", stderr(&o));
    let line = error_line(&o);
    assert!(
        line.starts_with("error: storage_busy: ")
            && line.ends_with("(nothing was changed; try again in 1 s)"),
        "{line}"
    );
    assert!(
        !listed().contains("Imported one"),
        "a refused import created a record"
    );

    let o = import(None);
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    let after = listed();
    assert!(after.contains("Imported one") && after.contains("Imported two"));
}

/// The stdio MCP server: one line in, one tool error out, `partial` when the first batch of the
/// call had committed. The server takes the line from standard input, as a client sends it.
#[test]
fn a_refused_tool_call_of_the_binary_says_what_was_applied() {
    use std::io::Write;
    let db = Dir::new("stdio");
    let first = call(&db, None, &["lead", "list", "limit=1"]);
    assert!(first.status.success(), "{}", stderr(&first));
    let tool_call = |faults: &str, id: &str| -> serde_json::Value {
        let mut child = obop()
            .env("OBOP_TEST_FAULTS", faults)
            .args(["mcp", "--dev-auth", "--db", &db.url()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("run obop mcp");
        let line = serde_json::json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{
            "name": "lead__update",
            "arguments": {"id": id, "stage": "won", "contact_name": "Zed"}}});
        {
            let mut stdin = child.stdin.take().unwrap();
            writeln!(stdin, "{line}").unwrap();
        }
        let out = child.wait_with_output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        serde_json::from_str(
            text.lines()
                .next()
                .unwrap_or_else(|| panic!("no answer: {}", String::from_utf8_lossy(&out.stderr))),
        )
        .unwrap()
    };

    let id = new_lead(&db, "Stdio refused at once");
    let reply = tool_call("apply:0", &id);
    assert_eq!(reply["result"]["isError"], true, "{reply}");
    let b = &reply["result"]["structuredContent"];
    assert_eq!(
        (b["code"].as_str(), b["applied"].as_str()),
        (Some("storage_busy"), Some("none"))
    );
    assert_eq!(b["retry_after"], 1);
    assert_ne!(stage(&db, &id), "won");

    let id = new_lead(&db, "Stdio refused after the first batch");
    let reply = tool_call("apply:1", &id);
    assert_eq!(reply["result"]["isError"], true, "{reply}");
    let b = &reply["result"]["structuredContent"];
    assert_eq!(
        (b["code"].as_str(), b["applied"].as_str()),
        (Some("storage_busy"), Some("partial")),
        "{reply}"
    );
    assert_eq!(stage(&db, &id), "won", "it said part of it was applied");
}
