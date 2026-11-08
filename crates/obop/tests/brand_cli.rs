//! `obop admin brand ...` through the real binary (docs/spec/branding.md, section 3 and section 13,
//! "Merge", "Audit" and "Commands"): the instance branding is shown, changed and removed from the
//! command line as the operator, the rules that guard it hold there as they do for an
//! administrator at the screen, and the lines about a values file name the field and the rule and
//! never a value. The parsing of the arguments and the text of `status` are also unit tested in
//! `src/brand_cmd_tests.rs`; the dispatcher side of the same commands is in
//! `src/brand_commands_tests.rs` and the crate `obop-commands`.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};

const KEY: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// A small artwork the strict reader accepts: the "M" of the spec's placeholder, in plain shapes.
const LOGO: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect x="0" y="0" width="64" height="64" rx="12" fill="#495670"/><path d="M 16 48 L 16 16 L 32 36 L 48 16 L 48 48" fill="none" stroke="#ffffff" stroke-width="6"/></svg>"##;

/// A second artwork, to tell a swap from a first set.
const LOGO_TWO: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><circle cx="32" cy="32" r="30" fill="#0f766e"/></svg>"##;

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Dir {
        let d = std::env::temp_dir().join(format!("obop-brand-cli-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Dir(d)
    }
    fn url(&self) -> String {
        format!("sqlite://{}", self.0.join("brand.sqlite").display())
    }
    fn file(&self, name: &str, text: &str) -> String {
        let p = self.0.join(name);
        std::fs::write(&p, text).unwrap();
        p.display().to_string()
    }
    fn bytes(&self, name: &str, bytes: &[u8]) -> String {
        let p = self.0.join(name);
        std::fs::write(&p, bytes).unwrap();
        p.display().to_string()
    }
    /// A database with one user, as `CLAUDE.md` says to start.
    fn started(tag: &str) -> Dir {
        let dir = Dir::new(tag);
        ok(obop(
            &dir.url(),
            &["admin", "create", "ada", "--all-groups"],
            Some("a long enough password 1\n"),
        ));
        dir
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The binary with the settings of a test in its environment and `--db URL` last. `cwd` is where
/// it runs: a command that needs no database must not make one there.
fn run(url: Option<&str>, args: &[&str], stdin: Option<&str>, cwd: Option<&PathBuf>) -> Output {
    run_with(url, args, stdin, cwd, &[])
}

/// As [`run`], with more settings in the environment of the process.
fn run_with(
    url: Option<&str>,
    args: &[&str],
    stdin: Option<&str>,
    cwd: Option<&PathBuf>,
    env: &[(&str, &str)],
) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_obop"));
    c.env_clear();
    for keep in ["SystemRoot", "SYSTEMROOT", "PATH", "TEMP", "TMP", "HOME"] {
        if let Ok(v) = std::env::var(keep) {
            c.env(keep, v);
        }
    }
    c.env("OBOP_SECRET_KEY", KEY)
        .env("OBOP_REQUIRE_MFA_FOR_ADMINS", "0")
        .envs(env.iter().copied())
        .args(args);
    if let Some(url) = url {
        c.args(["--db", url]);
    }
    if let Some(cwd) = cwd {
        c.current_dir(cwd);
    }
    c.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = c.spawn().unwrap();
    let mut pipe = child.stdin.take().unwrap();
    if let Some(s) = stdin {
        pipe.write_all(s.as_bytes()).unwrap();
    }
    drop(pipe);
    child.wait_with_output().unwrap()
}

fn obop(url: &str, args: &[&str], stdin: Option<&str>) -> Output {
    run(Some(url), args, stdin, None)
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}

fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

fn text(o: &Output) -> String {
    format!("{}{}", out(o), err(o))
}

/// The output of a command that succeeded.
fn ok(o: Output) -> String {
    assert!(o.status.success(), "{}", text(&o));
    text(&o)
}

/// The stderr of a command that failed with this exit status.
fn refused(o: Output, status: i32) -> String {
    assert_eq!(o.status.code(), Some(status), "{}", text(&o));
    err(&o)
}

fn status(url: &str) -> String {
    out(&{
        let o = obop(url, &["admin", "brand", "status"], None);
        assert!(o.status.success(), "{}", text(&o));
        o
    })
}

/// The events of the brand in the security log as `(kind, user, actor, detail)`, in the order of
/// kind and detail. The log is ordered by the second and a random suffix, so two events of one
/// second have no order a test may rely on.
fn events(url: &str) -> Vec<(String, String, String, String)> {
    let backend = obop_storage::open(url).unwrap();
    let mut all: Vec<_> = backend
        .list("security_event")
        .unwrap()
        .into_iter()
        .map(|(_, f)| {
            let get = |k: &str| f.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
            (get("kind"), get("user"), get("actor"), get("detail"))
        })
        .filter(|(kind, ..)| kind.starts_with("brand_"))
        .collect();
    all.sort();
    all
}

/// The details of the events of one kind.
fn details(url: &str, kind: &str) -> Vec<String> {
    events(url)
        .into_iter()
        .filter(|e| e.0 == kind)
        .map(|e| e.3)
        .collect()
}

/// The `v` that `check`, `status` or a change prints after `label`.
fn version_after(output: &str, label: &str) -> String {
    let rest = output
        .split(label)
        .nth(1)
        .unwrap_or_else(|| panic!("no {label:?} in {output}"));
    let v: String = rest
        .trim_start()
        .chars()
        .take_while(char::is_ascii_hexdigit)
        .collect();
    assert_eq!(v.len(), 16, "{output}");
    v
}

// ---- the help ------------------------------------------------------------------------------------

#[test]
fn the_help_lists_the_area() {
    let o = run(None, &["--help"], None, None);
    let help = refused(o, 2);
    for line in [
        "admin brand status",
        "admin brand configure [--name N] [--slug S] [--title LANG=TEXT]... [--message LANG=TEXT]...",
        "[--logo-file FILE.svg | --clear-logo] [--from FILE.toml] [--dry-run]",
        "admin brand clear [--dry-run]",
        "admin brand check FILE.svg",
    ] {
        assert!(help.contains(line), "{line} is not in\n{help}");
    }
}

// ---- the local smoke: create, configure, status, check, clear --------------------------------------

#[test]
fn the_smoke_configure_status_check_and_clear_as_the_operator() {
    let dir = Dir::started("smoke");
    let url = dir.url();

    // No brand: the defaults, and the answer says so.
    let s = status(&url);
    assert!(
        s.starts_with("brand: not set (no record; every value is the default)\n"),
        "{s}"
    );
    assert!(s.contains("name: \"obop\" (default)\n"), "{s}");
    assert!(s.contains("logo: none (default)\n"), "{s}");
    assert!(events(&url).is_empty());

    // The first call needs a name.
    let r = refused(
        obop(
            &url,
            &["admin", "brand", "configure", "--slug", "mohv"],
            None,
        ),
        1,
    );
    assert!(r.contains("error: name: is required"), "{r}");
    assert!(r.contains("nothing was stored"), "{r}");
    assert!(status(&url).starts_with("brand: not set"));

    // Configure.
    let c = ok(obop(
        &url,
        &["admin", "brand", "configure", "--name", "MOHV"],
        None,
    ));
    assert!(c.contains("changed:\n  name: \"\" -> \"MOHV\"\n"), "{c}");
    assert!(c.contains("recorded in the security log"), "{c}");
    let first_v = version_after(&c, "logo version:");
    let s = status(&url);
    assert!(s.starts_with("brand: set\n"), "{s}");
    assert!(s.contains("name: \"MOHV\" (record)\n"), "{s}");
    assert!(s.contains("slug: \"mohv\" (record)\n"), "{s}");
    assert!(s.contains("logo: tile (record)\n"), "{s}");
    assert_eq!(version_after(&s, "logo version:"), first_v);
    assert!(s.contains("fallbacks: none\n"), "{s}");

    // The same values again change nothing and write no event.
    let again = ok(obop(
        &url,
        &["admin", "brand", "configure", "--name", "MOHV"],
        None,
    ));
    assert!(again.contains("nothing changed"), "{again}");
    assert_eq!(events(&url).len(), 1);
    assert_eq!(
        events(&url)[0],
        (
            "brand_configured".into(),
            "(instance)".into(),
            "operator".into(),
            "name: \"\" -> \"MOHV\"".into()
        )
    );

    // `check` says what `v` the file would have; a change with the file gives the same `v`.
    let logo = dir.file("logo.svg", LOGO);
    let checked = ok(obop(&url, &["admin", "brand", "check", &logo], None));
    assert!(
        checked.starts_with(&format!("ok    {logo}  v=")),
        "{checked}"
    );
    let artwork_v = version_after(&checked, "v=");
    assert_ne!(artwork_v, first_v);
    let c = ok(obop(
        &url,
        &["admin", "brand", "configure", "--logo-file", &logo],
        None,
    ));
    assert!(
        c.contains(&format!("  logo_svg: none -> v={artwork_v}\n")),
        "{c}"
    );
    assert_eq!(version_after(&c, "logo version:"), artwork_v);
    let s = status(&url);
    assert!(s.contains("logo: artwork (record)\n"), "{s}");
    assert_eq!(version_after(&s, "logo version:"), artwork_v);
    // The one-flag swap left the name alone, and the artwork itself is in no line.
    assert!(s.contains("name: \"MOHV\" (record)\n"), "{s}");
    assert!(!s.contains("<svg") && !c.contains("<svg"));
    assert_eq!(
        details(&url, "brand_configured"),
        [
            format!("logo_svg: none -> v={artwork_v}"),
            "name: \"\" -> \"MOHV\"".to_string()
        ]
    );

    // `--clear-logo` goes back to the generated tile, and the version is the tile's again.
    let c = ok(obop(
        &url,
        &["admin", "brand", "configure", "--clear-logo"],
        None,
    ));
    assert!(
        c.contains(&format!("  logo_svg: v={artwork_v} -> none\n")),
        "{c}"
    );
    assert_eq!(version_after(&c, "logo version:"), first_v);

    // Clear: one event per non-empty field with its old value, then nothing to clear.
    ok(obop(
        &url,
        &["admin", "brand", "configure", "--logo-file", &logo],
        None,
    ));
    let configured_before = details(&url, "brand_configured");
    assert_eq!(configured_before.len(), 4);
    assert!(details(&url, "brand_cleared").is_empty());
    let cleared = ok(obop(&url, &["admin", "brand", "clear"], None));
    assert!(cleared.contains("brand removed:\n"), "{cleared}");
    assert!(
        cleared.contains("recorded in the security log"),
        "a real clear says it was logged: {cleared}"
    );
    assert!(
        cleared.contains("  name: \"MOHV\" -> removed\n"),
        "{cleared}"
    );
    assert!(
        cleared.contains(&format!("  logo_svg: v={artwork_v} -> removed\n")),
        "{cleared}"
    );
    let after = events(&url);
    // One event per non-empty field, with its old value; the configure events are as they were.
    assert_eq!(
        details(&url, "brand_cleared"),
        [
            format!("logo_svg: v={artwork_v} -> removed"),
            "name: \"MOHV\" -> removed".to_string()
        ]
    );
    assert_eq!(details(&url, "brand_configured"), configured_before);
    for e in &after {
        assert_eq!((e.1.as_str(), e.2.as_str()), ("(instance)", "operator"));
    }
    assert!(status(&url).starts_with("brand: not set"));
    let none = ok(obop(&url, &["admin", "brand", "clear"], None));
    assert!(
        none.contains("no brand is set; nothing to remove"),
        "{none}"
    );
    assert_eq!(
        events(&url).len(),
        after.len(),
        "a clear with no row writes no event"
    );
}

#[test]
fn what_the_operator_set_is_what_get_returns_and_an_agent_token_cannot_change_it() {
    let dir = Dir::started("get");
    let url = dir.url();
    // An agent of the administrator who holds `platform.admin` itself: the group does not stop it,
    // only the rule that these commands are for a person. A second agent with a narrower scope is
    // refused for the lack of the group, which is another refusal.
    let issue = |scopes: &str| {
        out(&obop(
            &url,
            &["agent-token", "ada", "--scopes", scopes],
            None,
        ))
        .trim()
        .to_string()
    };
    let token = issue("platform.admin");
    let narrow = issue("base.user");
    assert!(token.starts_with("obop_"), "an agent token was issued");
    assert!(narrow.starts_with("obop_"), "an agent token was issued");
    ok(obop(
        &url,
        &[
            "admin",
            "brand",
            "configure",
            "--name",
            "MOHV",
            "--title",
            "sv=Logga in",
        ],
        None,
    ));
    // `get` is open to any signed-in principal, an agent token included, and shows the same values.
    let get = obop(
        &url,
        &["call", "--token", &token, "instance_brand", "get"],
        None,
    );
    assert!(get.status.success(), "{}", text(&get));
    let got: serde_json::Value = serde_json::from_slice(&get.stdout).unwrap();
    assert_eq!(got["set"], true);
    assert_eq!(got["name"], "MOHV");
    assert_eq!(got["title_sv"], "Logga in");
    let s = status(&url);
    assert_eq!(
        version_after(&s, "logo version:"),
        got["v"].as_str().unwrap()
    );
    let effective: serde_json::Value =
        serde_json::from_str(got["effective"].as_str().unwrap()).unwrap();
    assert_eq!(effective["title_sv"]["value"], "Logga in");
    assert_eq!(effective["name"]["source"], "record");
    // The commands that write refuse the agent token that holds the group, by the rule that they are
    // for a person; and the row is as the operator left it.
    for command in ["configure", "clear"] {
        let args = if command == "configure" {
            vec!["name=Other"]
        } else {
            vec![]
        };
        let mut call = vec!["call", "--token", &token, "instance_brand", command];
        call.extend(args);
        let refused_call = obop(&url, &call, None);
        assert!(!refused_call.status.success(), "{}", text(&refused_call));
        assert!(
            err(&refused_call).contains("by a person with platform.admin"),
            "{command}: {}",
            text(&refused_call)
        );
    }
    // The narrower agent is refused too, but not by that rule: the call is as hidden as a command
    // that does not exist. So the refusal above is the rule of the wrapper and not a mistake in the
    // way the call is spelled.
    let hidden = obop(
        &url,
        &[
            "call",
            "--token",
            &narrow,
            "instance_brand",
            "configure",
            "name=Other",
        ],
        None,
    );
    assert!(!hidden.status.success(), "{}", text(&hidden));
    assert!(
        !err(&hidden).contains("by a person with platform.admin"),
        "{}",
        text(&hidden)
    );
    assert!(status(&url).contains("name: \"MOHV\" (record)\n"));
    assert_eq!(events(&url).len(), 2);
}

// ---- the repeatable flags -------------------------------------------------------------------------------

#[test]
fn titles_and_messages_repeat_and_an_empty_text_clears_only_that_field() {
    let dir = Dir::started("texts");
    let url = dir.url();
    let c = ok(obop(
        &url,
        &[
            "admin",
            "brand",
            "configure",
            "--name",
            "MOHV",
            "--title",
            "sv=Logga in",
            "--title",
            "en=Sign in",
            "--message",
            "fi=Tervetuloa",
            "--message",
            "sv=Välkommen",
        ],
        None,
    ));
    for line in [
        "  title_en: \"\" -> \"Sign in\"\n",
        "  title_sv: \"\" -> \"Logga in\"\n",
        "  message_fi: \"\" -> \"Tervetuloa\"\n",
        "  message_sv: \"\" -> \"Välkommen\"\n",
    ] {
        assert!(c.contains(line), "{line} is not in\n{c}");
    }
    let s = status(&url);
    for line in [
        "title_en: \"Sign in\" (record)\n",
        "title_sv: \"Logga in\" (record)\n",
        "message_fi: \"Tervetuloa\" (record)\n",
        "message_sv: \"Välkommen\" (record)\n",
    ] {
        assert!(s.contains(line), "{line} is not in\n{s}");
    }
    assert_eq!(events(&url).len(), 5);

    // `--title sv=` clears the Swedish title and nothing else.
    let c = ok(obop(
        &url,
        &["admin", "brand", "configure", "--title", "sv="],
        None,
    ));
    assert!(c.contains("  title_sv: \"Logga in\" -> \"\"\n"), "{c}");
    let s = status(&url);
    assert!(!s.contains("title_sv:"), "{s}");
    for kept in [
        "title_en: \"Sign in\"",
        "message_fi: ",
        "message_sv: ",
        "name: \"MOHV\"",
    ] {
        assert!(s.contains(kept), "{kept} is not in\n{s}");
    }
}

#[test]
fn status_incident_keeps_its_global_title_and_message() {
    let dir = Dir::new("incident");
    let url = dir.url();
    let c = ok(obop(
        &url,
        &[
            "status",
            "incident",
            "create",
            "--title",
            "Database slow",
            "--message",
            "We are looking into it",
        ],
        None,
    ));
    assert!(c.contains("incident 1 created"), "{c}");
    let l = ok(obop(&url, &["status", "incident", "list"], None));
    assert!(l.contains("Database slow"), "{l}");
    // The flags in front of nothing else are not the area's, whatever order they come in.
    let r = refused(
        obop(
            &url,
            &["--title", "x", "admin", "brand", "configure", "--name", "A"],
            None,
        ),
        2,
    );
    assert!(
        r.contains("give --title and --message after `admin brand`"),
        "{r}"
    );
}

#[test]
fn a_title_or_message_that_reached_the_global_options_is_refused_not_dropped() {
    let dir = Dir::started("order");
    let url = dir.url();
    // `--title` or `--message` before the words is the global option of `status incident`; the area
    // would drop it. Both are refused, with the flag first or after `--db URL`, and the answer is a
    // usage error that repeats no value.
    for flag in ["--title", "--message"] {
        for args in [
            vec![flag, "sv=Hej", "admin", "brand", "status"],
            vec!["--db", &url, flag, "sv=Hej", "admin", "brand", "status"],
        ] {
            let with_db = args.first() == Some(&"--db");
            let o = run(if with_db { None } else { Some(&url) }, &args, None, None);
            let r = refused(o, 2);
            assert!(
                r.contains("give --title and --message after `admin brand`, as LANG=TEXT"),
                "{args:?}: {r}"
            );
            assert!(!r.contains("Hej"), "{args:?}: {r}");
        }
    }
    // With `--db URL` first the words are still found, and the flags are the area's.
    let c = ok(run(
        None,
        &[
            "--db",
            &url,
            "admin",
            "brand",
            "configure",
            "--name",
            "A",
            "--title",
            "sv=Hej",
        ],
        None,
        None,
    ));
    assert!(c.contains("  title_sv: \"\" -> \"Hej\"\n"), "{c}");
    // `--year N`, a global option of the SIE commands, is skipped with its value like the rest: the
    // `2026` is not read as the command word, so the area is found and its `--title` is the area's.
    // (The failure was the usage error above, status 2, for a call that is spelled right.)
    let c = ok(obop(
        &url,
        &[
            "--year",
            "2026",
            "admin",
            "brand",
            "configure",
            "--title",
            "sv=X",
            "--dry-run",
        ],
        None,
    ));
    assert!(c.contains("  title_sv: \"Hej\" -> \"X\"\n"), "{c}");
    assert!(!c.contains("give --title and --message"), "{c}");
}

// ---- --dry-run --------------------------------------------------------------------------------------------

#[test]
fn a_dry_run_checks_and_reports_and_writes_nothing() {
    let dir = Dir::started("dry");
    let url = dir.url();
    let logo = dir.file("logo.svg", LOGO);
    let d = ok(obop(
        &url,
        &[
            "admin",
            "brand",
            "configure",
            "--name",
            "MOHV",
            "--logo-file",
            &logo,
            "--dry-run",
        ],
        None,
    ));
    assert!(
        d.contains("dry run, nothing was written; a real call would change:\n"),
        "{d}"
    );
    assert!(d.contains("  name: \"\" -> \"MOHV\"\n"), "{d}");
    assert!(!d.contains("recorded in the security log"), "{d}");
    let v = version_after(&d, "logo version:");
    // The version of a dry run is the one `check` prints and the real call gives.
    let checked = ok(obop(&url, &["admin", "brand", "check", &logo], None));
    assert_eq!(version_after(&checked, "v="), v);
    assert!(status(&url).starts_with("brand: not set"));
    assert!(events(&url).is_empty());

    // The same call for real, then the dry runs of a second configure and of a clear.
    let real = ok(obop(
        &url,
        &[
            "admin",
            "brand",
            "configure",
            "--name",
            "MOHV",
            "--logo-file",
            &logo,
        ],
        None,
    ));
    assert_eq!(version_after(&real, "logo version:"), v);
    let n = events(&url).len();
    let d = ok(obop(
        &url,
        &["admin", "brand", "configure", "--name", "MOHV", "--dry-run"],
        None,
    ));
    assert!(d.contains("dry run: nothing would change"), "{d}");
    let d = ok(obop(&url, &["admin", "brand", "clear", "--dry-run"], None));
    assert!(
        d.contains("dry run, nothing was written; a real call would remove the brand:\n"),
        "{d}"
    );
    assert!(d.contains("  name: \"MOHV\" -> removed\n"), "{d}");
    assert!(
        !d.contains("recorded in the security log"),
        "a dry run of clear logs nothing and does not say so: {d}"
    );
    assert!(status(&url).starts_with("brand: set"));
    assert_eq!(events(&url).len(), n);

    // A dry run that finds a refusal fails, and says what a real call would be refused for.
    let r = refused(
        obop(
            &url,
            &[
                "admin",
                "brand",
                "configure",
                "--name",
                "REFUSEDNAME<b",
                "--dry-run",
            ],
            None,
        ),
        1,
    );
    assert!(r.contains("error: name: "), "{r}");
    assert!(
        r.contains("dry run: a real call would be refused; nothing was written"),
        "{r}"
    );
    assert!(!r.contains("REFUSEDNAME"), "{r}");
}

// ---- refusals ---------------------------------------------------------------------------------------------------

#[test]
fn a_refused_field_stores_nothing_of_the_call_and_every_refusal_is_listed() {
    let dir = Dir::started("refused");
    let url = dir.url();
    let r = refused(
        obop(
            &url,
            &[
                "admin",
                "brand",
                "configure",
                "--name",
                "MOHV",
                "--slug",
                "9x",
                "--title",
                "sv=https://x.example",
                "--message",
                "en=Fine",
            ],
            None,
        ),
        1,
    );
    assert!(r.contains("error: slug: "), "{r}");
    assert!(r.contains("error: title_sv: "), "{r}");
    assert!(
        !r.contains("error: name: ") && !r.contains("error: message_en: "),
        "{r}"
    );
    assert!(r.contains("nothing was stored"), "{r}");
    assert!(
        !r.contains("x.example"),
        "a refusal does not repeat the value: {r}"
    );
    assert!(status(&url).starts_with("brand: not set"));
    assert!(events(&url).is_empty());

    // The dry run reports the same refusals as the real call.
    let lines = |s: &str| -> Vec<String> {
        s.lines()
            .filter(|l| l.starts_with("error: "))
            .map(str::to_string)
            .collect()
    };
    let dry = refused(
        obop(
            &url,
            &[
                "admin",
                "brand",
                "configure",
                "--name",
                "MOHV",
                "--slug",
                "9x",
                "--title",
                "sv=https://x.example",
                "--message",
                "en=Fine",
                "--dry-run",
            ],
            None,
        ),
        1,
    );
    assert_eq!(lines(&dry), lines(&r));
    assert!(!lines(&r).is_empty());
    assert!(
        dry.contains("dry run: a real call would be refused"),
        "{dry}"
    );
    assert!(events(&url).is_empty());
}

#[test]
fn five_processes_at_once_on_different_fields_all_survive() {
    let dir = Dir::started("race");
    let url = dir.url();
    ok(obop(
        &url,
        &["admin", "brand", "configure", "--name", "MOHV"],
        None,
    ));
    // The command line takes the lock of `apply` itself: five operators at once, one language each.
    let handles: Vec<_> = ["sv", "fi", "de", "fr", "pl"]
        .into_iter()
        .map(|lang| {
            let url = url.clone();
            std::thread::spawn(move || {
                let t = format!("{lang}=Title {lang}");
                let o = obop(&url, &["admin", "brand", "configure", "--title", &t], None);
                (lang, o.status.success(), text(&o))
            })
        })
        .collect();
    for h in handles {
        let (lang, success, said) = h.join().unwrap();
        assert!(success, "{lang}: {said}");
    }
    let s = status(&url);
    for lang in ["sv", "fi", "de", "fr", "pl"] {
        assert!(
            s.contains(&format!("title_{lang}: \"Title {lang}\" (record)\n")),
            "{lang} was lost:\n{s}"
        );
    }
    assert!(s.contains("name: \"MOHV\" (record)\n"), "{s}");
    assert_eq!(details(&url, "brand_configured").len(), 6);
}

#[test]
fn a_call_with_nothing_to_set_and_a_misused_area_are_usage_errors_with_one_status() {
    let dir = Dir::started("usage");
    let url = dir.url();
    let r = refused(obop(&url, &["admin", "brand", "configure"], None), 2);
    assert!(r.contains("nothing to set"), "{r}");
    for args in [
        vec!["admin", "brand"],
        vec!["admin", "brand", "frobnicate"],
        vec!["admin", "brand", "check"],
        vec!["admin", "brand", "status", "extra"],
    ] {
        let r = refused(obop(&url, &args, None), 2);
        assert!(r.contains("obop admin brand status"), "{args:?}: {r}");
    }
    // Every command line that is spelled wrong ends with status 2, and a verb never takes an option
    // it does not have: a flag it would drop is refused, not ignored.
    for (args, needle) in [
        (
            vec!["admin", "brand", "status", "--name", "A"],
            "`admin brand status` takes no options",
        ),
        (
            vec!["admin", "brand", "status", "--dry-run"],
            "takes no options",
        ),
        (
            vec!["admin", "brand", "status", "--from", "x.toml"],
            "`admin brand status` takes no options",
        ),
        (
            vec!["admin", "brand", "check", "x.svg", "--name", "A"],
            "takes no options",
        ),
        (
            vec!["admin", "brand", "check", "x.svg", "--from", "x.toml"],
            "`admin brand check` takes no options",
        ),
        (
            vec!["admin", "brand", "check", "x.svg", "--dry-run"],
            "`admin brand check` takes no options",
        ),
        (
            vec!["admin", "brand", "clear", "--name", "A"],
            "takes only --dry-run",
        ),
        (
            vec!["admin", "brand", "clear", "--from", "x.toml"],
            "takes only --dry-run",
        ),
        (
            vec!["admin", "brand", "configure", "--name", "A", "--name", "B"],
            "--name is given twice",
        ),
        (
            vec!["admin", "brand", "configure", "--title", "sv"],
            "--title needs LANG=TEXT",
        ),
        (
            vec!["admin", "brand", "configure", "--title", "xx=A"],
            "the language must be one of",
        ),
        (
            vec![
                "admin",
                "brand",
                "configure",
                "--logo-file",
                "a.svg",
                "--clear-logo",
            ],
            "give --logo-file or --clear-logo, not both",
        ),
    ] {
        let o = obop(&url, &args, None);
        assert_eq!(o.status.code(), Some(2), "{args:?}: {}", text(&o));
        assert!(err(&o).contains(needle), "{args:?}: {}", err(&o));
    }
    // A flag that takes a value and is the last word has none (`--db URL` goes in front here, or
    // it would be taken for the value). `--from` is a global option, and without its file the call
    // would go on as if no file had been meant and write the fields that were given.
    for (flag, said) in [
        ("--logo-file", "error: --logo-file needs a value"),
        ("--from", "error: --from needs a file"),
    ] {
        let o = run(
            None,
            &[
                "--db",
                &url,
                "admin",
                "brand",
                "configure",
                "--name",
                "NEVERSET",
                flag,
            ],
            None,
            None,
        );
        assert_eq!(refused(o, 2).trim(), said);
    }
    assert!(status(&url).starts_with("brand: not set"));
    assert!(events(&url).is_empty());
}

// ---- --from FILE ---------------------------------------------------------------------------------------------------

#[test]
fn from_reads_the_record_table_and_ignores_the_phone_table() {
    let dir = Dir::started("from");
    let url = dir.url();
    let values = dir.file(
        "mohv.toml",
        &format!(
            "[record]\nname = \"MOHV\"\ntitle_sv = \"Logga in\"\nmessage_en = \"Welcome\"\nlogo_svg = '''{LOGO}'''\n\n[phone]\napplication_id = \"se.mohv.app\"\nanything = 5\n"
        ),
    );
    let c = ok(obop(
        &url,
        &["admin", "brand", "configure", "--from", &values],
        None,
    ));
    assert!(c.contains("  name: \"\" -> \"MOHV\"\n"), "{c}");
    assert!(c.contains("  title_sv: \"\" -> \"Logga in\"\n"), "{c}");
    assert!(c.contains("  message_en: \"\" -> \"Welcome\"\n"), "{c}");
    let artwork_v = version_after(&c, "logo version:");
    let s = status(&url);
    assert!(s.contains("logo: artwork (record)\n"), "{s}");
    assert_eq!(version_after(&s, "logo version:"), artwork_v);
    assert!(!s.contains("se.mohv.app"), "{s}");

    // A flag for a field the file does not set is added to the call.
    let c = ok(obop(
        &url,
        &[
            "admin",
            "brand",
            "configure",
            "--from",
            &values,
            "--title",
            "fi=Kirjaudu",
            "--dry-run",
        ],
        None,
    ));
    assert!(c.contains("  title_fi: \"\" -> \"Kirjaudu\"\n"), "{c}");
    assert!(c.contains("dry run"), "{c}");
}

#[test]
fn from_refuses_what_is_not_a_field_by_name_and_prints_no_value() {
    let dir = Dir::started("fromrefused");
    let url = dir.url();
    for (what, file, needle) in [
        (
            "an answer field",
            "[record]\nname = \"VALUEONE\"\nv = \"VALUETWO\"\n",
            "the key v is not a field of the brand",
        ),
        (
            "dry_run",
            "[record]\nname = \"VALUEONE\"\ndry_run = \"VALUETWO\"\n",
            "the key dry_run is not a field of the brand",
        ),
        (
            "an unknown key",
            "[record]\nname = \"VALUEONE\"\ncolour = \"VALUETWO\"\n",
            "the key colour is not a field of the brand",
        ),
        (
            "another table",
            "[record]\nname = \"VALUEONE\"\n[extra]\nname = \"VALUETWO\"\n",
            "[extra] is not read",
        ),
        (
            "a key outside a table",
            "name = \"VALUETWO\"\n[record]\nname = \"VALUEONE\"\n",
            "the key name is outside a table",
        ),
        (
            "a value that is not text",
            "[record]\nname = 5\n",
            "[record] name must be text",
        ),
        (
            "no record table",
            "[phone]\napplication_id = \"VALUETWO\"\n",
            "the file has no [record] table",
        ),
        (
            "text that is not TOML",
            "[record]\nname = \"VALUETWO\n",
            "the file is not valid TOML",
        ),
    ] {
        let f = dir.file("bad.toml", file);
        let r = refused(
            obop(&url, &["admin", "brand", "configure", "--from", &f], None),
            1,
        );
        assert!(r.contains(needle), "{what}: {r}");
        assert!(
            r.contains("bad.toml"),
            "{what}: the line names the file: {r}"
        );
        for value in ["VALUEONE", "VALUETWO"] {
            assert!(!r.contains(value), "{what}: a value is in the line:\n{r}");
        }
        assert!(status(&url).starts_with("brand: not set"), "{what}");
    }
    // Every declared field of the answers, and `id`, is a key the file may not set.
    for key in [
        "dry_run",
        "set",
        "v",
        "effective",
        "fallbacks",
        "changes",
        "refusals",
        "cleared",
        "id",
    ] {
        let f = dir.file(
            "answer.toml",
            &format!("[record]\nname = \"VALUEONE\"\n{key} = \"VALUETWO\"\n"),
        );
        let r = refused(
            obop(&url, &["admin", "brand", "configure", "--from", &f], None),
            1,
        );
        assert!(
            r.contains(&format!("the key {key} is not a field of the brand")),
            "{key}: {r}"
        );
        assert!(
            !r.contains("VALUEONE") && !r.contains("VALUETWO"),
            "{key}: {r}"
        );
    }
    assert!(status(&url).starts_with("brand: not set"));
    assert!(events(&url).is_empty());

    // A value the rules refuse: the field and the rule, never the text of the value. A rule is a
    // fixed sentence (the host-name rule has `shop.example` in it as its example, so the value
    // below is another host name), and a character that broke one is named by its code point.
    let f = dir.file(
        "policy.toml",
        "[record]\nname = \"POLICYVALUE<b\"\ntitle_de = \"https://x.example\"\nmessage_en = \"visit demo.invalid today\"\n",
    );
    let r = refused(
        obop(&url, &["admin", "brand", "configure", "--from", &f], None),
        1,
    );
    assert!(
        r.contains("error: name: ")
            && r.contains("error: title_de: ")
            && r.contains("error: message_en: "),
        "{r}"
    );
    assert!(r.contains("host name (shop.example)"), "the host rule: {r}");
    for value in ["POLICYVALUE", "x.example", "demo.invalid", "visit", "today"] {
        assert!(!r.contains(value), "{value} is in\n{r}");
    }

    // A file that does not exist, and a field given by the file and by a flag.
    let missing = dir.0.join("missing.toml").display().to_string();
    let r = refused(
        obop(
            &url,
            &["admin", "brand", "configure", "--from", &missing],
            None,
        ),
        1,
    );
    assert!(r.contains("cannot read the values file"), "{r}");
    let good = dir.file("good.toml", "[record]\nname = \"MOHV\"\n");
    let r = refused(
        obop(
            &url,
            &[
                "admin",
                "brand",
                "configure",
                "--from",
                &good,
                "--name",
                "Other",
            ],
            None,
        ),
        2,
    );
    assert!(
        r.contains("name is given twice: by the values file and by a flag"),
        "{r}"
    );
    assert!(!r.contains("Other") && !r.contains("MOHV"), "{r}");
    let r = refused(
        obop(
            &url,
            &[
                "admin",
                "brand",
                "configure",
                "--from",
                &good,
                "--dry-run",
                "--clear-logo",
                "--logo-file",
                &good,
            ],
            None,
        ),
        2,
    );
    assert!(r.contains("not both"), "{r}");
    assert!(status(&url).starts_with("brand: not set"));
}

// ---- the artwork: --logo-file and check ----------------------------------------------------------------------------

#[test]
fn check_needs_no_database_and_prints_the_version_or_every_reason() {
    let dir = Dir::new("check");
    let logo = dir.file("logo.svg", LOGO);
    // No --db, no environment, and a working directory of its own: it makes no database there.
    let work = dir.0.join("work");
    std::fs::create_dir_all(&work).unwrap();
    let o = run(None, &["admin", "brand", "check", &logo], None, Some(&work));
    assert!(o.status.success(), "{}", text(&o));
    assert!(
        out(&o).starts_with(&format!("ok    {logo}  v=")),
        "{}",
        out(&o)
    );
    assert_eq!(err(&o), "", "check opens no database and says nothing else");
    assert_eq!(
        std::fs::read_dir(&work).unwrap().count(),
        0,
        "no database file was made"
    );

    // Every reason of a file that cannot be used, one line each, with the file in front.
    let hostile = dir.file(
        "hostile.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" onload="x()"><script>x</script><defs/><path d="M 0 0" opacity="0.5"/></svg>"##,
    );
    let o = run(
        None,
        &["admin", "brand", "check", &hostile],
        None,
        Some(&work),
    );
    let r = refused(o, 1);
    assert!(r.lines().count() >= 3, "every reason has a line: {r}");
    for line in r.lines() {
        assert!(line.starts_with(&format!("{hostile}: ")), "{line}");
    }
    for word in ["<script>", "<defs>", "onload", "opacity"] {
        assert!(r.contains(word), "{word} is not named in\n{r}");
    }
    assert_eq!(std::fs::read_dir(&work).unwrap().count(), 0);

    // A byte order mark, UTF-16 and a file that is not there have a reason of their own.
    let mut bom = vec![0xEF, 0xBB, 0xBF];
    bom.extend_from_slice(LOGO.as_bytes());
    let bom = dir.bytes("bom.svg", &bom);
    let r = refused(
        run(None, &["admin", "brand", "check", &bom], None, Some(&work)),
        1,
    );
    assert!(r.contains("byte order mark"), "{r}");
    let missing = dir.0.join("missing.svg").display().to_string();
    let r = refused(
        run(
            None,
            &["admin", "brand", "check", &missing],
            None,
            Some(&work),
        ),
        1,
    );
    assert!(r.contains("cannot read"), "{r}");
}

#[test]
fn a_logo_file_is_refused_with_the_reasons_of_the_reader_and_a_swap_keeps_the_rest() {
    let dir = Dir::started("logo");
    let url = dir.url();
    let hostile = dir.file(
        "hostile.svg",
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 1 1\"><script>x</script></svg>",
    );
    let r = refused(
        obop(
            &url,
            &[
                "admin",
                "brand",
                "configure",
                "--name",
                "MOHV",
                "--logo-file",
                &hostile,
            ],
            None,
        ),
        1,
    );
    assert!(
        r.contains("error: logo_svg: ") && r.contains("<script>"),
        "{r}"
    );
    assert!(status(&url).starts_with("brand: not set"));

    // The same reasons as `check`, and as a dry run.
    let checked = refused(obop(&url, &["admin", "brand", "check", &hostile], None), 1);
    let reasons = |s: &str, prefix: &str| -> Vec<String> {
        s.lines()
            .filter_map(|l| l.strip_prefix(prefix))
            .map(str::to_string)
            .collect()
    };
    assert_eq!(
        reasons(&checked, &format!("{hostile}: ")),
        reasons(&r, "error: logo_svg: ")
    );
    assert!(!reasons(&r, "error: logo_svg: ").is_empty());
    let dry = refused(
        obop(
            &url,
            &[
                "admin",
                "brand",
                "configure",
                "--name",
                "MOHV",
                "--logo-file",
                &hostile,
                "--dry-run",
            ],
            None,
        ),
        1,
    );
    assert_eq!(
        reasons(&dry, "error: logo_svg: "),
        reasons(&r, "error: logo_svg: ")
    );
    assert!(
        dry.contains("dry run: a real call would be refused; nothing was written"),
        "{dry}"
    );

    // A byte order mark is UTF-8, so it reaches the reader, which refuses it as `check` does. The
    // file is not cleaned up on the way in: the same artwork is refused on every surface.
    let mut with_mark = vec![0xEF, 0xBB, 0xBF];
    with_mark.extend_from_slice(LOGO.as_bytes());
    let marked = dir.bytes("marked.svg", &with_mark);
    let r = refused(
        obop(
            &url,
            &[
                "admin",
                "brand",
                "configure",
                "--name",
                "MOHV",
                "--logo-file",
                &marked,
            ],
            None,
        ),
        1,
    );
    assert!(
        r.contains("error: logo_svg: the file starts with a byte order mark"),
        "{r}"
    );
    assert!(status(&url).starts_with("brand: not set"));
    assert!(events(&url).is_empty());

    // UTF-16 and bytes that are not text: named by the reader, not by a decoding error.
    let utf16: Vec<u8> = std::iter::once(0xFEFFu16)
        .chain(LOGO.encode_utf16())
        .flat_map(u16::to_le_bytes)
        .collect();
    let f = dir.bytes("utf16.svg", &utf16);
    let r = refused(
        obop(
            &url,
            &[
                "admin",
                "brand",
                "configure",
                "--name",
                "MOHV",
                "--logo-file",
                &f,
            ],
            None,
        ),
        1,
    );
    assert!(
        r.contains("error: logo_svg: ") && r.contains("UTF-16"),
        "{r}"
    );
    let f = dir.bytes("binary.svg", &[0x3C, 0x73, 0xFF, 0xFE, 0x00]);
    let r = refused(
        obop(
            &url,
            &[
                "admin",
                "brand",
                "configure",
                "--name",
                "MOHV",
                "--logo-file",
                &f,
            ],
            None,
        ),
        1,
    );
    assert!(r.contains("error: logo_svg: "), "{r}");
    assert!(status(&url).starts_with("brand: not set"));

    // Set a name, titles and a message, then swap the artwork twice: only the artwork changes.
    ok(obop(
        &url,
        &[
            "admin",
            "brand",
            "configure",
            "--name",
            "MOHV",
            "--title",
            "sv=Logga in",
            "--message",
            "en=Hello",
        ],
        None,
    ));
    let one = dir.file("one.svg", LOGO);
    let two = dir.file("two.svg", LOGO_TWO);
    let a = ok(obop(
        &url,
        &["admin", "brand", "configure", "--logo-file", &one],
        None,
    ));
    let b = ok(obop(
        &url,
        &["admin", "brand", "configure", "--logo-file", &two],
        None,
    ));
    let (va, vb) = (
        version_after(&a, "logo version:"),
        version_after(&b, "logo version:"),
    );
    assert_ne!(va, vb);
    assert!(
        b.contains(&format!("  logo_svg: v={va} -> v={vb}\n")),
        "{b}"
    );
    assert_eq!(b.lines().filter(|l| l.starts_with("  ")).count(), 1, "{b}");
    let s = status(&url);
    for kept in [
        "name: \"MOHV\"",
        "title_sv: \"Logga in\"",
        "message_en: \"Hello\"",
    ] {
        assert!(s.contains(kept), "{kept} is not in\n{s}");
    }
}

// ---- the version of an artwork is that of the text the reader writes ------------------------------------------------

/// What a design tool exports: an extra space, a newline between the elements and at the end, and
/// numbers as `32.000` and `.5`. The reader accepts it and writes it back in its own form, so the
/// bytes of this file are not the bytes the logo route serves.
const LOGO_EXPORT: &str = "<svg  xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 64 64\">\n <circle cx=\"32\" cy=\"32.000\" r=\".5\" fill=\"#0f766e\" />\n</svg>\n";

#[test]
fn the_version_is_that_of_the_text_the_reader_writes_not_of_the_bytes_of_the_file() {
    let dir = Dir::started("canon");
    let url = dir.url();
    let file = dir.file("export.svg", LOGO_EXPORT);
    let canonical = obop_brand::svg::parse(LOGO_EXPORT.as_bytes())
        .expect("the export is an artwork the reader accepts")
        .print()
        .to_string();
    assert_ne!(
        canonical, LOGO_EXPORT,
        "the sample must not be in canonical form"
    );
    let v = obop_brand::logo::version(canonical.as_bytes()).to_string();
    let of_the_file = obop_brand::logo::version(LOGO_EXPORT.as_bytes()).to_string();
    assert_ne!(
        v, of_the_file,
        "the two hashes must differ for this to tell"
    );

    // `check`, a dry run, the real call and `status` all say the one version.
    let checked = ok(obop(&url, &["admin", "brand", "check", &file], None));
    assert_eq!(version_after(&checked, "v="), v, "{checked}");
    let call = |dry: bool| {
        let mut args = vec![
            "admin",
            "brand",
            "configure",
            "--name",
            "MOHV",
            "--logo-file",
            &file,
        ];
        if dry {
            args.push("--dry-run");
        }
        ok(obop(&url, &args, None))
    };
    let dry = call(true);
    assert_eq!(version_after(&dry, "logo version:"), v, "{dry}");
    assert!(
        dry.contains(&format!("  logo_svg: none -> v={v}\n")),
        "{dry}"
    );
    let real = call(false);
    assert_eq!(version_after(&real, "logo version:"), v, "{real}");
    assert!(
        real.contains(&format!("  logo_svg: none -> v={v}\n")),
        "{real}"
    );
    assert_eq!(version_after(&status(&url), "logo version:"), v);
    assert_eq!(
        details(&url, "brand_configured"),
        [
            format!("logo_svg: none -> v={v}"),
            "name: \"\" -> \"MOHV\"".to_string()
        ]
    );
}

// ---- no demo data -----------------------------------------------------------------------------------------------------

#[test]
fn the_commands_leave_a_new_sqlite_database_free_of_demo_data() {
    // The rollout of an instance starts with `configure` on a database that is new (spec, section
    // 14). Another command opens a new SQLite database with a demo company in it; these must not.
    let demo = |url: &str| -> Vec<(&'static str, usize)> {
        let backend = obop_storage::open(url).unwrap();
        ["party", "product", "lead"]
            .into_iter()
            .map(|model| (model, backend.list(model).unwrap().len()))
            .filter(|(_, rows)| *rows > 0)
            .collect()
    };
    let control = Dir::new("seeded");
    ok(obop(
        &control.url(),
        &["call", "--dev-auth", "party", "list", "limit=1"],
        None,
    ));
    assert!(
        !demo(&control.url()).is_empty(),
        "the control: `call` seeds a new database, so this test can see demo data"
    );

    let dir = Dir::new("nodemo");
    let url = dir.url();
    let steps: [&[&str]; 4] = [
        &["admin", "brand", "status"],
        &["admin", "brand", "configure", "--name", "MOHV"],
        &["admin", "brand", "clear", "--dry-run"],
        &["admin", "brand", "clear"],
    ];
    for args in steps {
        ok(obop(&url, args, None));
        assert!(
            demo(&url).is_empty(),
            "{args:?} left demo data in the database"
        );
    }
    assert_eq!(events(&url).len(), 2, "the commands did run");
}

// ---- a store that refuses ---------------------------------------------------------------------------------------------

#[test]
fn a_write_the_store_refuses_is_status_75_with_the_line_of_obop_call_and_stores_nothing() {
    // The test build of the store refuses the first atomic batch of the process as `storage_busy`,
    // as the PostgreSQL pool would (`OBOP_TEST_FAULTS`, see `tests/error_seam.rs`).
    let faults = [("OBOP_TEST_FAULTS", "apply:0")];
    let with_faults = |url: &str, args: &[&str]| run_with(Some(url), args, None, None, &faults);
    let line_of = |r: &str| {
        r.lines()
            .find(|l| l.starts_with("error: "))
            .unwrap_or_else(|| panic!("no error line in {r}"))
            .to_string()
    };
    let dir = Dir::started("busy");
    let url = dir.url();

    // `configure`: the refusal is the line `obop call` prints and the status "try again later".
    let configure = ["admin", "brand", "configure", "--name", "MOHV"];
    let line = line_of(&refused(with_faults(&url, &configure), 75));
    assert!(
        line.starts_with("error: storage_busy: ")
            && line.ends_with("(nothing was changed; try again in 1 s)"),
        "{line}"
    );
    assert!(status(&url).starts_with("brand: not set"));
    assert!(events(&url).is_empty());
    // A dry run writes no batch, so the fault that waits for one does not touch it.
    let mut dry = configure.to_vec();
    dry.push("--dry-run");
    let said = ok(with_faults(&url, &dry));
    assert!(said.contains("dry run, nothing was written"), "{said}");

    // The same call without the fault writes.
    ok(obop(&url, &configure, None));
    assert_eq!(events(&url).len(), 1);

    // `clear` the same way: the row and the events stay.
    let line = line_of(&refused(
        with_faults(&url, &["admin", "brand", "clear"]),
        75,
    ));
    assert!(
        line.starts_with("error: storage_busy: ")
            && line.ends_with("(nothing was changed; try again in 1 s)"),
        "{line}"
    );
    assert!(status(&url).contains("name: \"MOHV\" (record)\n"));
    assert_eq!(events(&url).len(), 1);
    ok(obop(&url, &["admin", "brand", "clear"], None));
    assert!(status(&url).starts_with("brand: not set"));
}

// ---- a stored value that is not safe to print ---------------------------------------------------------------------------

#[test]
fn a_stored_value_that_is_not_safe_to_print_is_escaped_where_configure_and_clear_show_it() {
    let dir = Dir::started("escaped");
    let url = dir.url();
    // A row written around the command: an escape sequence and a bidirectional override.
    let plant = || {
        let backend = obop_storage::open(&url).unwrap();
        let mut row = obop_storage::Fields::new();
        row.insert("name".into(), serde_json::json!("A\u{1b}[31mB\u{202e}C"));
        backend
            .apply_checked(&[obop_storage::Op::Put {
                model: "instance_brand".into(),
                id: "instance".into(),
                fields: row,
            }])
            .unwrap();
    };
    let escaped = "A\\u{001B}[31mB\\u{202E}C";
    let safe = |said: &str| {
        assert!(
            !said.chars().any(|c| c == '\u{1b}' || c == '\u{202e}'),
            "a raw character reached the terminal: {said:?}"
        );
        assert!(said.contains(escaped), "{said}");
    };

    plant();
    let dry = ok(obop(
        &url,
        &[
            "admin",
            "brand",
            "configure",
            "--name",
            "Clean",
            "--dry-run",
        ],
        None,
    ));
    safe(&dry);
    let real = ok(obop(
        &url,
        &["admin", "brand", "configure", "--name", "Clean"],
        None,
    ));
    safe(&real);
    for detail in details(&url, "brand_configured") {
        safe(&detail);
    }

    plant();
    let cleared = ok(obop(&url, &["admin", "brand", "clear"], None));
    safe(&cleared);
    let logged = details(&url, "brand_cleared");
    assert_eq!(logged.len(), 1, "{logged:?}");
    safe(&logged[0]);
}

/// `clear` says "recorded in the security log" only when it wrote an event. A row written around the
/// command with no key at all is deleted with no event (`apply`), and the line says that nothing was
/// recorded; a row with a key still says it was.
#[test]
fn clear_says_it_recorded_the_removal_only_when_an_event_was_written() {
    let dir = Dir::started("emptyrow");
    let url = dir.url();
    let plant = |fields: obop_storage::Fields| {
        let backend = obop_storage::open(&url).unwrap();
        backend
            .apply_checked(&[obop_storage::Op::Put {
                model: "instance_brand".into(),
                id: "instance".into(),
                fields,
            }])
            .unwrap();
    };

    plant(obop_storage::Fields::new());
    // The dry run says what the real call will do: remove the row and record nothing.
    let dry = ok(obop(&url, &["admin", "brand", "clear", "--dry-run"], None));
    assert!(
        dry.contains(
            "a real call would remove the row, which holds no key, and record nothing in the security log"
        ),
        "{dry}"
    );
    assert!(!dry.contains("would remove the brand:"), "{dry}");
    let still = obop_storage::open(&url).unwrap();
    assert!(
        still.get("instance_brand", "instance").unwrap().is_some(),
        "a dry run removed the row"
    );
    drop(still);
    let said = ok(obop(&url, &["admin", "brand", "clear"], None));
    assert!(said.contains("brand removed"), "{said}");
    assert!(
        said.contains("nothing was recorded in the security log: the row held no key"),
        "{said}"
    );
    assert!(!said.contains("\nrecorded in the security log"), "{said}");
    assert!(
        details(&url, "brand_cleared").is_empty(),
        "an event was written for a row with no key"
    );
    assert!(status(&url).contains("brand: not set"), "the row is gone");

    let mut row = obop_storage::Fields::new();
    row.insert("name".into(), serde_json::json!("MOHV"));
    plant(row);
    let said = ok(obop(&url, &["admin", "brand", "clear"], None));
    assert!(said.contains("\nrecorded in the security log"), "{said}");
    assert!(!said.contains("nothing was recorded"), "{said}");
    assert_eq!(details(&url, "brand_cleared").len(), 1);
}

// ---- the lines the reader writes name this command ------------------------------------------------------------------

#[test]
fn the_command_the_stderr_hint_names_exists_and_lists_the_field() {
    let dir = Dir::started("hint");
    let url = dir.url();
    // A row written around the command: the reader falls back for the title and says so once.
    {
        let backend = obop_storage::open(&url).unwrap();
        let mut row = obop_storage::Fields::new();
        row.insert("name".into(), serde_json::json!("Acme"));
        row.insert("title_de".into(), serde_json::json!("a<b"));
        backend
            .apply_checked(&[obop_storage::Op::Put {
                model: "instance_brand".into(),
                id: "instance".into(),
                fields: row,
            }])
            .unwrap();
    }
    let lines = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = lines.clone();
    let reader = obop_commands::brand::Reader::new(obop_storage::open(&url).unwrap())
        .with_sink(Arc::new(move |l| sink.lock().unwrap().push(l.to_string())));
    reader.current();
    let line = lines.lock().unwrap().first().cloned().expect("a line");
    // "... (obop admin brand status lists it)": the words in the parentheses, without "lists it".
    let named = line
        .rsplit_once('(')
        .and_then(|(_, tail)| tail.strip_suffix(" lists it)"))
        .unwrap_or_else(|| panic!("no command in {line:?}"));
    let words: Vec<&str> = named.split_whitespace().collect();
    assert_eq!(words[0], "obop", "{named}");
    let o = obop(&url, &words[1..], None);
    let shown = ok(o);
    assert!(shown.contains("brand: set\n"), "{shown}");
    assert!(
        shown.contains("fallback: title_de is not used, the default is shown (unsafe_character)\n"),
        "{shown}"
    );
    assert!(!shown.contains("a<b"), "{shown}");
}

// ---- the authenticator issuer (S1e) -------------------------------------------------------------------

/// `status` and `get` say what an enrolment would write into an authenticator and which source won:
/// the variable `OBOP_MFA_ISSUER` of the process that asks, else the stored name, else `obop`. The
/// process of a test has an environment of its own (`run_with` clears it), so the variable is set or
/// unset here and never leaks in from the machine that runs the test.
#[test]
fn status_and_get_name_the_authenticator_issuer_and_the_source_that_won() {
    let dir = Dir::started("issuer");
    let url = dir.url();
    let s = status(&url);
    assert!(
        s.contains("authenticator_issuer: \"obop\" (default)\n"),
        "{s}"
    );
    ok(obop(
        &url,
        &["admin", "brand", "configure", "--name", "MOHV"],
        None,
    ));
    let s = status(&url);
    assert!(
        s.contains("authenticator_issuer: \"MOHV\" (record)\n"),
        "{s}"
    );
    // The variable wins, also when it says `obop` and the record says another name; the name is
    // the record's all the same.
    for variable in ["Acme", "obop"] {
        let o = run_with(
            Some(&url),
            &["admin", "brand", "status"],
            None,
            None,
            &[("OBOP_MFA_ISSUER", variable)],
        );
        let s = ok(o);
        assert!(
            s.contains(&format!("authenticator_issuer: \"{variable}\" (env)\n")),
            "{s}"
        );
        assert!(s.contains("name: \"MOHV\" (record)\n"), "{s}");
    }
    // A variable that is only white space is no value.
    let s = ok(run_with(
        Some(&url),
        &["admin", "brand", "status"],
        None,
        None,
        &[("OBOP_MFA_ISSUER", "  ")],
    ));
    assert!(
        s.contains("authenticator_issuer: \"MOHV\" (record)\n"),
        "{s}"
    );
    // `get` through the command line (the REST and MCP answer is the same function).
    let token = out(&obop(
        &url,
        &["agent-token", "ada", "--scopes", "platform.admin"],
        None,
    ))
    .trim()
    .to_string();
    let get = run_with(
        Some(&url),
        &["call", "--token", &token, "instance_brand", "get"],
        None,
        None,
        &[("OBOP_MFA_ISSUER", "Acme")],
    );
    assert!(get.status.success(), "{}", text(&get));
    let got: serde_json::Value = serde_json::from_slice(&get.stdout).unwrap();
    let effective: serde_json::Value =
        serde_json::from_str(got["effective"].as_str().unwrap()).unwrap();
    assert_eq!(
        effective["authenticator_issuer"],
        serde_json::json!({"value": "Acme", "source": "env"})
    );
    assert_eq!(
        effective["name"],
        serde_json::json!({"value": "MOHV", "source": "record"})
    );
}

/// The repair the answer `brand_unavailable` points to: a stored name that cannot be shown is listed
/// by `status` (which prints nothing of its value), and `configure` at the terminal, which needs no
/// session, replaces it. Until then `authenticator_issuer` has no value (`null`, source
/// `unreadable`: the enrolment is refused) while `name` shows the default, and `fallbacks` says why.
#[test]
fn a_stored_name_that_cannot_be_shown_is_listed_by_status_and_repaired_at_the_terminal() {
    let dir = Dir::started("repair");
    let url = dir.url();
    {
        let backend = obop_storage::open(&url).unwrap();
        let mut row = obop_storage::Fields::new();
        row.insert("name".into(), serde_json::json!("a<b"));
        backend
            .apply_checked(&[obop_storage::Op::Put {
                model: "instance_brand".into(),
                id: "instance".into(),
                fields: row,
            }])
            .unwrap();
    }
    let s = status(&url);
    assert!(
        s.contains("fallback: name is not used, the default is shown (unsafe_character)\n"),
        "{s}"
    );
    assert!(!s.contains("a<b"), "{s}");
    // No issuer is in force: an enrolment is refused for as long as this is so, and `status` says
    // so instead of the default that the enrolment would not write.
    assert!(
        s.contains("authenticator_issuer: null (unreadable)\n"),
        "{s}"
    );
    assert!(s.contains("name: \"obop\" (default)\n"), "{s}");
    ok(obop(
        &url,
        &["admin", "brand", "configure", "--name", "MOHV"],
        None,
    ));
    let s = status(&url);
    assert!(s.contains("fallbacks: none\n"), "{s}");
    assert!(
        s.contains("authenticator_issuer: \"MOHV\" (record)\n"),
        "{s}"
    );
}

/// A name that the display accepts and `configure` refuses (a colon: it would make a link) can only
/// be in the row if it was written around the command. It shows as the name, `fallbacks` lists
/// nothing, and the enrolment is refused all the same: `status` says so with the issuer `null`.
#[test]
fn a_stored_name_with_a_colon_has_no_issuer_in_force_and_is_repaired_at_the_terminal() {
    let dir = Dir::started("colon");
    let url = dir.url();
    {
        let backend = obop_storage::open(&url).unwrap();
        let mut row = obop_storage::Fields::new();
        row.insert("name".into(), serde_json::json!("a:b"));
        backend
            .apply_checked(&[obop_storage::Op::Put {
                model: "instance_brand".into(),
                id: "instance".into(),
                fields: row,
            }])
            .unwrap();
    }
    let s = status(&url);
    assert!(s.contains("name: \"a:b\" (record)\n"), "{s}");
    assert!(s.contains("fallbacks: none\n"), "{s}");
    assert!(
        s.contains("authenticator_issuer: null (unreadable)\n"),
        "{s}"
    );
    ok(obop(
        &url,
        &["admin", "brand", "configure", "--name", "MOHV"],
        None,
    ));
    let s = status(&url);
    assert!(
        s.contains("authenticator_issuer: \"MOHV\" (record)\n"),
        "{s}"
    );
}
