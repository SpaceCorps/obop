use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] | ["--version"] => {
            println!("{} {}", obop_types::NAME, env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        ["modules", "check"] => modules_check(),
        _ => {
            eprintln!("usage: obop [--version | modules check]");
            ExitCode::from(2)
        }
    }
}

/// The D41 install gate over every linked first-party module.
fn modules_check() -> ExitCode {
    let modules = [base::module()];
    let mut failed = false;
    for m in &modules {
        let violations = obop_registry::check_module(m);
        if violations.is_empty() {
            println!("ok    {}", m.id);
        } else {
            failed = true;
            for v in violations {
                println!("FAIL  {}: {}", v.path, v.problem);
            }
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
