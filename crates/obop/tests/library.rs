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

/// A crate outside this one can name the entry point and build the stock app, whose first six
/// modules are the linked first-party ones in the order `app.rs` registers them.
#[test]
fn another_crate_can_link_the_library_and_build_the_stock_app() {
    let _entry: fn() -> std::process::ExitCode = obop::run;
    let app = obop::app::App::new();
    let ids: Vec<&str> = app.modules.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(
        ids[..6],
        ["base", "product", "crm", "sales", "invoicing", "mileage"],
        "{ids:?}"
    );
    let groups = app.all_groups();
    assert!(groups.contains(&"base.user".to_string()), "{groups:?}");
    assert!(
        groups.windows(2).all(|w| w[0] < w[1]),
        "sorted and without duplicates: {groups:?}"
    );
}

/// The library's public surface is the entry point and the composition root and nothing else:
/// every other module stays private until C2 decides, for a customer's crates, what else is an
/// API (D98 Decision 3), and a crate-root item is public only by a line that starts in column 0.
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
    // No re-export hides another item behind a private module.
    assert!(
        !lib.lines().any(|l| l.starts_with("pub use ")),
        "a re-export"
    );
}
