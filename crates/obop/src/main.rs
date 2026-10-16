use obop_commands::{Dispatcher, Principal, Surface, Value, names};
use obop_masking::Vault;
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
        ["call", rest @ ..] => call(rest),
        _ => {
            eprintln!(
                "usage: obop [--version | modules check | call [--agent] <model> <command> [key=value...]]"
            );
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

/// Demo of the one dispatch path over fake in-memory data (no database yet).
fn call(args: &[&str]) -> ExitCode {
    let (agent, args) = match args {
        ["--agent", rest @ ..] => (true, rest),
        rest => (false, rest),
    };
    let [model, command, kv @ ..] = args else {
        eprintln!("usage: obop call [--agent] <model> <command> [key=value...]");
        return ExitCode::from(2);
    };
    let Some(id) = names::from_cli(&[model, command]) else {
        return ExitCode::from(2);
    };
    let input = Value::Map(
        kv.iter()
            .filter_map(|p| p.split_once('='))
            .map(|(k, v)| (k.to_string(), Value::text(v)))
            .collect(),
    );
    let principal = if agent {
        Principal::agent("cli-agent", &["base.user"], &["base.user"])
    } else {
        Principal::user("cli-user", &["base.user", "privacy.officer"])
    };
    let mut d = Dispatcher::new();
    base::register(&mut d, base::demo_store());
    match d.call(Surface::Cli, &principal, &mut Vault::new(), &id, &input) {
        Ok(r) => {
            println!("{:#?}", r.value);
            if !r.denied.is_empty() {
                println!("denied: {}", r.denied.join(", "));
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e:?}");
            ExitCode::FAILURE
        }
    }
}
