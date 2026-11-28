//! D98, C1: the `obop` crate is a library and the binary is one call into it. An integration test
//! is a separate crate that links the library the way any other crate would, which is what these
//! tests rely on; the command line itself is held by the other files of this directory, which
//! start the binary and check what it prints and returns.

use std::path::Path;

fn source(file: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join(file)).unwrap()
}

/// The code of a Rust file with its comments and blank lines left out, one line per line of code.
fn code(src: &str) -> Vec<String> {
    src.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("//"))
        .map(String::from)
        .collect()
}

/// The binary target holds nothing but the call: no module, no function besides `main`, no
/// logic. Everything the command line does is in the library, so that another crate can build the
/// same server.
#[test]
fn the_binary_is_one_call_into_the_library() {
    assert_eq!(
        code(&source("main.rs")),
        ["fn main() -> std::process::ExitCode {", "obop::run()", "}"]
    );
}

/// A crate outside this one can name the entry point and build the stock app, whose modules are
/// the linked ones in the order `app.rs` registers them; every permission group any of them
/// declares is in `all_groups`, sorted and without duplicates.
#[test]
fn another_crate_can_link_the_library_and_build_the_stock_app() {
    let _entry: fn() -> std::process::ExitCode = obop::run;
    let app = obop::app::App::new();
    let ids: Vec<&str> = app.modules.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "base",
            "product",
            "crm",
            "sales",
            "invoicing",
            "mileage",
            "connectors",
            "mail",
            "platform"
        ]
    );
    let groups = app.all_groups();
    assert!(groups.contains(&"base.user".to_string()), "{groups:?}");
    assert!(
        groups.windows(2).all(|w| w[0] < w[1]),
        "sorted and without duplicates: {groups:?}"
    );
    for m in &app.modules {
        for g in m.permission_groups() {
            assert!(groups.iter().any(|x| x == g), "{g} of {} is missing", m.id);
        }
    }
}

/// The library's public surface is the entry point and the composition root and nothing else:
/// every other module stays private until C2 decides, for a customer's crates, what else is an
/// API (D98 Decision 3). A crate-root item is public only by a line that starts in column 0, so
/// that is what is read; in `app.rs` the only item public outside the crate is `App` (the fixed
/// development principals `DevAuth` and `dev_principal` are `pub(crate)`).
#[test]
fn the_library_exports_only_run_and_app() {
    let lib = source("lib.rs");
    let exported: Vec<&str> = lib
        .lines()
        .filter(|l| {
            l.starts_with("pub ") || (l.starts_with("pub(") && !l.starts_with("pub(crate)"))
        })
        .collect();
    assert_eq!(exported, ["pub mod app;", "pub fn run() -> ExitCode {"]);
    // No re-export hides another item behind a private module, and no macro leaves the crate.
    assert!(
        !lib.lines().any(|l| l.starts_with("pub use ")),
        "a re-export"
    );
    assert!(!lib.contains("#[macro_export]"), "an exported macro");
    let app = source("app.rs");
    let exported: Vec<&str> = app
        .lines()
        .take_while(|l| !l.starts_with("#[cfg(test)]"))
        .filter(|l| l.starts_with("pub "))
        .collect();
    assert_eq!(exported, ["pub struct App {"]);
}
