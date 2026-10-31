//! `obop admin sso ...` through the real binary (D91): the provider is configured, bound, shown and
//! removed from the command line, and the rules that guard it hold there as they do for an
//! administrator at the screen. The sign-in itself is in `src/sso_tests.rs`.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

const KEY: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const TENANT: &str = "11111111-2222-3333-4444-555555555555";
const OTHER: &str = "99999999-2222-3333-4444-555555555555";
const CLIENT: &str = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
const OID: &str = "0f0f0f0f-1111-2222-3333-444444444444";
const SECRET: &str = "cli-test-client-secret-5d2e";

struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Dir {
        let d = std::env::temp_dir().join(format!("obop-sso-cli-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Dir(d)
    }
    fn url(&self) -> String {
        format!("sqlite://{}", self.0.join("sso.sqlite").display())
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn obop(url: &str, args: &[&str], stdin: Option<&str>) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_obop"));
    c.env_clear();
    for keep in ["SystemRoot", "SYSTEMROOT", "PATH", "TEMP", "TMP", "HOME"] {
        if let Ok(v) = std::env::var(keep) {
            c.env(keep, v);
        }
    }
    c.env("OBOP_SECRET_KEY", KEY)
        .env("OBOP_REQUIRE_MFA_FOR_ADMINS", "0")
        .args(args)
        .args(["--db", url])
        .stdin(Stdio::piped())
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

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

fn ok(o: Output) -> String {
    assert!(o.status.success(), "{}", text(&o));
    text(&o)
}

fn refused(o: Output) -> String {
    assert!(!o.status.success(), "expected a refusal, got: {}", text(&o));
    text(&o)
}

#[test]
fn the_provider_is_configured_shown_kept_and_removed_from_the_command_line() {
    let dir = Dir::new("provider");
    let url = dir.url();
    ok(obop(
        &url,
        &["admin", "create", "ada", "--groups", "base.user"],
        Some("a long enough password 1\n"),
    ));
    assert!(
        ok(obop(&url, &["admin", "sso", "status"], None)).contains("provider: none configured")
    );

    // The input is checked.
    assert!(refused(obop(&url, &["admin", "sso", "configure"], None)).contains("--tenant"));
    assert!(
        refused(obop(
            &url,
            &["admin", "sso", "configure", "--tenant", TENANT],
            None
        ))
        .contains("--client-id")
    );
    let bad = refused(obop(
        &url,
        &[
            "admin",
            "sso",
            "configure",
            "--tenant",
            "common",
            "--client-id",
            CLIENT,
            "--secret-stdin",
        ],
        Some(&format!("{SECRET}\n")),
    ));
    assert!(bad.contains("multi-tenant"), "{bad}");
    assert!(
        refused(obop(
            &url,
            &[
                "admin",
                "sso",
                "configure",
                "--tenant",
                TENANT,
                "--client-id",
                CLIENT,
                "--enabled",
                "--disabled",
            ],
            None
        ))
        .contains("not both")
    );

    // The first configuration is on, and the secret is stored and never printed.
    let done = ok(obop(
        &url,
        &[
            "admin",
            "sso",
            "configure",
            "--tenant",
            TENANT,
            "--client-id",
            CLIENT,
            "--secret-stdin",
        ],
        Some(&format!("{SECRET}\n")),
    ));
    assert!(
        done.contains("enabled") && !done.contains("not enabled"),
        "{done}"
    );
    assert!(!done.contains(SECRET));
    let status = ok(obop(&url, &["admin", "sso", "status"], None));
    assert!(
        status.contains(TENANT)
            && status.contains("enabled")
            && status.contains("client secret stored"),
        "{status}"
    );
    assert!(!status.contains(SECRET));

    // Switched off on purpose, and a later configuration without the switch keeps it off.
    let off = ok(obop(
        &url,
        &[
            "admin",
            "sso",
            "configure",
            "--tenant",
            TENANT,
            "--client-id",
            CLIENT,
            "--disabled",
        ],
        None,
    ));
    assert!(off.contains("not enabled"), "{off}");
    let kept = ok(obop(
        &url,
        &[
            "admin",
            "sso",
            "configure",
            "--tenant",
            TENANT,
            "--client-id",
            CLIENT,
        ],
        None,
    ));
    assert!(kept.contains("not enabled"), "{kept}");
    let on = ok(obop(
        &url,
        &[
            "admin",
            "sso",
            "configure",
            "--tenant",
            TENANT,
            "--client-id",
            CLIENT,
            "--enabled",
        ],
        None,
    ));
    assert!(
        on.contains("enabled") && !on.contains("not enabled"),
        "{on}"
    );
}

#[test]
fn a_binding_pins_the_tenant_even_after_the_provider_was_removed() {
    let dir = Dir::new("pin");
    let url = dir.url();
    ok(obop(
        &url,
        &["admin", "create", "ada", "--groups", "base.user"],
        Some("a long enough password 1\n"),
    ));
    ok(obop(
        &url,
        &[
            "admin",
            "sso",
            "configure",
            "--tenant",
            TENANT,
            "--client-id",
            CLIENT,
            "--secret-stdin",
        ],
        Some(&format!("{SECRET}\n")),
    ));
    // Not bound to a malformed id; bound to a good one; shown.
    assert!(
        refused(obop(
            &url,
            &["admin", "sso", "bind", "ada", "not-a-guid"],
            None
        ))
        .contains("GUID")
    );
    ok(obop(&url, &["admin", "sso", "bind", "ada", OID], None));
    let status = ok(obop(&url, &["admin", "sso", "status"], None));
    assert!(status.contains(OID) && status.contains("bound"), "{status}");
    // Bound is not tried: Microsoft-only needs a sign-in on the binding first.
    let early = refused(obop(&url, &["admin", "sso", "only", "ada", "on"], None));
    assert!(early.contains("never signed in"), "{early}");
    // The tenant cannot change under a binding, with the provider there or removed.
    let change = |url: &str| {
        refused(obop(
            url,
            &[
                "admin",
                "sso",
                "configure",
                "--tenant",
                OTHER,
                "--client-id",
                CLIENT,
                "--secret-stdin",
            ],
            Some(&format!("{SECRET}\n")),
        ))
    };
    assert!(change(&url).contains("unbind"));
    ok(obop(&url, &["admin", "sso", "clear"], None));
    assert!(
        ok(obop(&url, &["admin", "sso", "status"], None)).contains("provider: none configured")
    );
    assert!(
        change(&url).contains("unbind"),
        "the pin holds without a provider"
    );
    // Unbound, the tenant can change.
    let gone = ok(obop(&url, &["admin", "sso", "unbind", "ada"], None));
    assert!(gone.contains("sessions_ended"), "{gone}");
    ok(obop(
        &url,
        &[
            "admin",
            "sso",
            "configure",
            "--tenant",
            OTHER,
            "--client-id",
            CLIENT,
            "--secret-stdin",
        ],
        Some(&format!("{SECRET}\n")),
    ));
    assert!(ok(obop(&url, &["admin", "sso", "status"], None)).contains(OTHER));
    // A verb nobody knows.
    assert!(refused(obop(&url, &["admin", "sso", "only", "ada", "maybe"], None)).contains("usage"));
}
