//! `obop admin brand status|configure|clear|check` (docs/spec/branding.md, PROPOSED, section 3): the
//! command line of the instance branding. It acts on the database as the operator, as `obop site
//! set` and `obop admin sso configure` do: a person with the host, not a token.
//!
//! - `configure` and `clear` call `obop_commands::brand::apply`, the one function that writes the
//!   row for every surface, with the actor `operator` and **no reader**: a change made at a terminal
//!   reaches a running server within the reader's two seconds, as a change made by another process
//!   does. It takes the same lock, merges by supplied field and writes the audit events in the
//!   same atomic batch.
//! - `status` is the command line of `get`: the uncached read of the row and the theme setting, so a
//!   store that cannot be read is an error and never "no brand".
//! - `check FILE.svg` runs the strict SVG reader on a file and needs no database.
//!
//! **The flags.** `--title LANG=TEXT` and `--message LANG=TEXT` are repeatable, and the binary's
//! global options of the same names take one value each for `status incident`, so `main` hands the
//! argument list to [`take_flags`] first. Only when the first two words are `admin brand` does it
//! pull out the flags of this area (`--title`, `--message`, `--name`, `--slug`, `--logo-file`,
//! `--clear-logo`); every other command line goes on untouched. `--db`, `--dry-run` and `--from`
//! stay the global options and arrive as arguments of [`run`].
//!
//! **What a line may hold.** The lines about `--from FILE` name the field or the table and the
//! rule, never a value of the file. The refusals of `configure` come from `apply`, whose reasons
//! repeat only what the rules made safe to repeat. A change is shown as the security log holds it
//! (`brand::change_line`), so a stored value that is not safe to print is escaped.

use super::{Exit, Opts, fail, open_app_with};
use obop_brand::{logo, svg};
use obop_commands::brand::{self, Action, Failure, Fresh, LANGS, Reader, change_line};
use serde_json::Value as J;
use std::process::ExitCode;

/// The actor of the events a change made at a terminal writes to the security log.
const OPERATOR: &str = "operator";

const USAGE: &str = "usage:
  obop admin brand status
  obop admin brand configure [--name N] [--slug S] [--title LANG=TEXT]... [--message LANG=TEXT]...
                             [--logo-file FILE.svg | --clear-logo] [--from FILE.toml] [--dry-run]
  obop admin brand clear [--dry-run]
  obop admin brand check FILE.svg
(--db URL as for every command; LANG is one of en, de, fr, pl, fi, sv, es)";

/// The flags of the area `admin brand` that the global options do not have, or have with another
/// meaning.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Flags {
    pub name: Option<String>,
    pub slug: Option<String>,
    /// `(language, text)` in the order given; at most one per language.
    pub titles: Vec<(String, String)>,
    /// `(language, text)` in the order given; at most one per language.
    pub messages: Vec<(String, String)>,
    pub logo_file: Option<String>,
    pub clear_logo: bool,
}

impl Flags {
    fn is_empty(&self) -> bool {
        *self == Flags::default()
    }
}

/// Where the words `admin brand` stand: at the start of the command line, past any leading global
/// options (`--db URL`, `--token T`, and flags without a value). `None` for every other command.
fn area_at(args: &[&str]) -> Option<usize> {
    let mut i = 0;
    while let Some(a) = args.get(i) {
        if !a.starts_with("--") {
            break;
        }
        i += if matches!(*a, "--db" | "--token") {
            2
        } else {
            1
        };
    }
    (args.get(i) == Some(&"admin") && args.get(i + 1) == Some(&"brand")).then_some(i)
}

/// `LANG=TEXT` of `--title` and `--message`. The text is everything after the first `=`, and may be
/// empty (it clears the field). A refusal names the flag and never repeats the argument.
fn lang_text(flag: &str, pair: Option<&str>) -> Result<(String, String), String> {
    let Some(pair) = pair else {
        return Err(format!(
            "{flag} needs LANG=TEXT, for example {flag} sv=Logga in"
        ));
    };
    let Some((lang, text)) = pair.split_once('=') else {
        return Err(format!(
            "{flag} needs LANG=TEXT, for example {flag} sv=Logga in"
        ));
    };
    if !LANGS.contains(&lang) {
        return Err(format!(
            "{flag}: the language must be one of {}",
            LANGS.join(", ")
        ));
    }
    Ok((lang.to_string(), text.to_string()))
}

/// Pulls the flags of the area out of a command line, when the command is `admin brand`. Returns
/// them (`None` for any other command) and the rest of the arguments, in their order. A flag that
/// takes a value and has none, a `--title` or `--message` that is not `LANG=TEXT` or names a language
/// the record does not have, and a flag given twice are refused.
pub fn take_flags(args: Vec<&str>) -> Result<(Option<Flags>, Vec<&str>), String> {
    let Some(at) = area_at(&args) else {
        return Ok((None, args));
    };
    let mut flags = Flags::default();
    let mut rest = Vec::with_capacity(args.len());
    let mut it = args.into_iter().enumerate();
    while let Some((i, a)) = it.next() {
        if i < at + 2 {
            rest.push(a);
            continue;
        }
        let mut value = |flag: &str, what: &str| {
            it.next()
                .map(|(_, v)| v)
                .ok_or_else(|| format!("{flag} needs {what}"))
        };
        match a {
            "--name" | "--slug" | "--logo-file" => {
                let v = value(a, "a value")?.to_string();
                let slot = match a {
                    "--name" => &mut flags.name,
                    "--slug" => &mut flags.slug,
                    _ => &mut flags.logo_file,
                };
                if slot.replace(v).is_some() {
                    return Err(format!("{a} is given twice"));
                }
            }
            "--title" | "--message" => {
                let (lang, text) = lang_text(a, value(a, "LANG=TEXT").ok())?;
                let list = if a == "--title" {
                    &mut flags.titles
                } else {
                    &mut flags.messages
                };
                if list.iter().any(|(l, _)| *l == lang) {
                    return Err(format!("{a} {lang} is given twice"));
                }
                list.push((lang, text));
            }
            "--clear-logo" => flags.clear_logo = true,
            other => rest.push(other),
        }
    }
    Ok((Some(flags), rest))
}

/// `obop admin brand ...`: `words` are the words after `admin brand`, `from` the global `--from`.
pub fn run(o: &Opts, words: &[&str], flags: Flags, from: Option<&str>, dry_run: bool) -> Exit {
    let takes_no_options = |verb: &str| {
        if flags.is_empty() && from.is_none() && !dry_run {
            Ok(())
        } else {
            Err(fail(&format!(
                "error: `admin brand {verb}` takes no options\n{USAGE}"
            )))
        }
    };
    match words {
        ["status"] => {
            takes_no_options("status")?;
            status(o)
        }
        ["check", file] => {
            takes_no_options("check")?;
            check(file)
        }
        ["clear"] => {
            if !flags.is_empty() || from.is_some() {
                return Err(fail(&format!(
                    "error: `admin brand clear` takes only --dry-run\n{USAGE}"
                )));
            }
            clear(o, dry_run)
        }
        ["configure"] => configure(o, &flags, from, dry_run),
        _ => {
            eprintln!("{USAGE}");
            Err(ExitCode::from(2))
        }
    }
}

/// The fields of one `configure` call, from the values file and the flags. A field given by both is
/// refused, by its name.
fn supplied(flags: &Flags, from: Option<&str>) -> Result<Vec<(String, String)>, ExitCode> {
    let mut out: Vec<(String, String)> = Vec::new();
    if let Some(path) = from {
        let text = std::fs::read_to_string(path)
            .map_err(|e| fail(&format!("error: cannot read the values file {path}: {e}")))?;
        match obop_pkg::brand::read_values_file(&text) {
            Ok(pairs) => out = pairs,
            Err(problems) => {
                for p in problems {
                    eprintln!("error: {path}: {p}");
                }
                return Err(ExitCode::FAILURE);
            }
        }
    }
    let mut add = |field: String, text: String| {
        if out.iter().any(|(f, _)| *f == field) {
            return Err(fail(&format!(
                "error: {field} is given twice: by the values file and by a flag"
            )));
        }
        out.push((field, text));
        Ok(())
    };
    if let Some(n) = &flags.name {
        add("name".into(), n.clone())?;
    }
    if let Some(s) = &flags.slug {
        add("slug".into(), s.clone())?;
    }
    for (lang, text) in &flags.titles {
        add(format!("title_{lang}"), text.clone())?;
    }
    for (lang, text) in &flags.messages {
        add(format!("message_{lang}"), text.clone())?;
    }
    if flags.clear_logo && flags.logo_file.is_some() {
        return Err(fail("error: give --logo-file or --clear-logo, not both"));
    }
    if let Some(path) = &flags.logo_file {
        add("logo_svg".into(), read_logo(path)?)?;
    }
    if flags.clear_logo {
        add("logo_svg".into(), String::new())?;
    }
    Ok(out)
}

/// The text of an artwork file. A file that is not UTF-8 is refused here with the reasons the SVG
/// reader gives (a UTF-16 file, bytes that are not text), because a call carries text; a byte order
/// mark is UTF-8 and goes on, to be refused by the reader with its own reason.
fn read_logo(path: &str) -> Result<String, ExitCode> {
    let bytes = std::fs::read(path)
        .map_err(|e| fail(&format!("error: cannot read the logo file {path}: {e}")))?;
    String::from_utf8(bytes).map_err(|e| {
        let reasons = svg::parse(e.as_bytes()).err().unwrap_or_default();
        if reasons.is_empty() {
            eprintln!("error: logo_svg: the file is not valid UTF-8");
        }
        for r in reasons {
            eprintln!("error: logo_svg: {r}");
        }
        ExitCode::FAILURE
    })
}

fn configure(o: &Opts, flags: &Flags, from: Option<&str>, dry_run: bool) -> Exit {
    let fields = supplied(flags, from)?;
    if fields.is_empty() {
        return Err(fail(&format!(
            "error: nothing to set: give a field flag or a --from file whose [record] table has fields\n{USAGE}"
        )));
    }
    let app = open_app_with(o, false)?;
    let backend = app.store.backend();
    let applied = brand::apply(
        &*backend,
        None,
        OPERATOR,
        &Action::Configure(fields),
        dry_run,
    )
    .map_err(failure)?;
    if !applied.refusals.is_empty() {
        for r in &applied.refusals {
            eprintln!("error: {}: {}", r.field, r.rule);
        }
        eprintln!("dry run: a real call would be refused; nothing was written");
        return Err(ExitCode::FAILURE);
    }
    if applied.changes.is_empty() {
        println!(
            "{}",
            if dry_run {
                "dry run: nothing would change"
            } else {
                "nothing changed"
            }
        );
    } else {
        println!(
            "{}",
            if dry_run {
                "dry run, nothing was written; a real call would change:"
            } else {
                "changed:"
            }
        );
        for c in &applied.changes {
            println!("  {}", change_line(c, false));
        }
        if !dry_run {
            println!("recorded in the security log");
        }
    }
    if !applied.v.is_empty() {
        println!("logo version: {}", applied.v);
    }
    Ok(())
}

fn clear(o: &Opts, dry_run: bool) -> Exit {
    let app = open_app_with(o, false)?;
    let backend = app.store.backend();
    let applied =
        brand::apply(&*backend, None, OPERATOR, &Action::Clear, dry_run).map_err(failure)?;
    if !applied.cleared {
        println!("no brand is set; nothing to remove");
        return Ok(());
    }
    println!(
        "{}",
        if dry_run {
            "dry run, nothing was written; a real call would remove the brand:"
        } else {
            "brand removed:"
        }
    );
    for c in &applied.changes {
        println!("  {}", change_line(c, true));
    }
    if !dry_run {
        println!("recorded in the security log");
    }
    Ok(())
}

/// The exit of a call that changed nothing: every refusal on its own line, or the store's text.
fn failure(f: Failure) -> ExitCode {
    match f {
        Failure::Refused(list) => {
            for r in list {
                eprintln!("error: {}: {}", r.field, r.rule);
            }
            eprintln!("nothing was stored");
            ExitCode::FAILURE
        }
        Failure::Storage(e) => fail(&format!("error: {e}")),
    }
}

fn status(o: &Opts) -> Exit {
    let app = open_app_with(o, false)?;
    let fresh = Reader::new(app.store.backend())
        .fresh()
        .map_err(|e| fail(&format!("error: {e}")))?;
    print!("{}", render_status(&fresh));
    Ok(())
}

/// What `status` prints: whether a brand is set, the values in force with the source that won, the
/// titles and messages that are set, and the stored fields that fell back to a default.
pub fn render_status(fresh: &Fresh) -> String {
    let shown = fresh.shown();
    let effective: J = serde_json::from_str(&fresh.effective_json()).unwrap_or(J::Null);
    let mut out = String::new();
    out.push_str(if fresh.set() {
        "brand: set\n"
    } else {
        "brand: not set (no record; every value is the default)\n"
    });
    let mut line = |key: &str, raw: bool| {
        let entry = &effective[key];
        let Some(source) = entry["source"].as_str() else {
            return;
        };
        let value = match (&entry["value"], raw) {
            (J::String(s), true) => s.clone(),
            (other, _) => other.to_string(),
        };
        out.push_str(&format!("{key}: {value} ({source})\n"));
    };
    line("name", false);
    line("slug", false);
    line("logo", true);
    line("tile_accent", false);
    let fell_back = |field: &str| shown.fallbacks().iter().any(|f| f.field == field);
    for prefix in ["title", "message"] {
        for lang in LANGS {
            let field = format!("{prefix}_{lang}");
            if !fresh.stored_text(&field).is_empty() && !fell_back(&field) {
                line(&field, false);
            }
        }
    }
    if !shown.v().is_empty() {
        out.push_str(&format!("logo version: {}\n", shown.v()));
    }
    if shown.fallbacks().is_empty() {
        out.push_str("fallbacks: none\n");
    }
    for f in shown.fallbacks() {
        out.push_str(&format!(
            "fallback: {} is not used, the default is shown ({})\n",
            f.field, f.reason
        ));
    }
    out
}

/// `obop admin brand check FILE.svg`: the strict SVG reader on a file, with no database. Prints the
/// version of the logo it would have, or every reason the file is refused (the reasons `configure
/// --dry-run` gives for the artwork).
fn check(file: &str) -> Exit {
    let bytes =
        std::fs::read(file).map_err(|e| fail(&format!("error: cannot read {file}: {e}")))?;
    match svg::parse(&bytes) {
        Ok(parsed) => {
            println!(
                "ok    {file}  v={}",
                logo::version(parsed.print().as_bytes())
            );
            Ok(())
        }
        Err(refusals) => {
            for r in refusals {
                eprintln!("{file}: {r}");
            }
            Err(ExitCode::FAILURE)
        }
    }
}
