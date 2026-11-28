//! The `obop` binary: one call into the library (D98, C1). Everything, the command line, the start-up
//! and the linked modules, is in `lib.rs`, so that another crate can build the same server.

fn main() -> std::process::ExitCode {
    obop::run()
}
