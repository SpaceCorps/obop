//! The CLI prints a failed call as it always did (the guide documents the line), now that the
//! text comes from `obop_commands::errors::classify`. Real binary, no database server.

use std::process::Command;

fn obop() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_obop"));
    c.env("OBOP_DEV_INSECURE_KEY", "1")
        .env("OBOP_REQUIRE_MFA_FOR_ADMINS", "0")
        .env_remove("OBOP_DATABASE_URL")
        .env_remove("OBOP_SECRET_KEY")
        .env_remove("OBOP_SECRET_KEY_PREVIOUS");
    c
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
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.lines().any(|l| l == "error: Handler(\"no lead nope\")"),
        "{err}"
    );
}
