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
//! repeat only what `obop_brand::refusal` makes safe to repeat: a fixed sentence, the code point of a
//! character that broke a rule, or the name of an element or attribute of an artwork; never the
//! text of a value. A change is shown as the security log holds it (`brand::change_line`), so a
//! stored value that is not safe to print is escaped.
//!
//! **The exit status.** 0 when the command did what it was asked; 2 for a command line that is
//! spelled wrong (a word or flag the area does not have, a flag that needs a value, a field given
//! twice, nothing to set), as for the usage text of any command; 1 for a value, a file or a store
//! that was refused or could not be read; 75 when the store itself refused the call (the D74 code
//! words `storage_busy`, `storage_unavailable` and `storage_timeout`, "try again later"), with the
//! line `obop call` prints for it. A store that cannot be read is never "no brand".
//!
//! **What a command says.** `status`, `configure` and `clear` work over a store and return a
//! [`Said`], the text for the two streams and the status, which [`Said::print`] writes; a test
//! reads the [`Said`] and needs no process. The functions that open the database and read the
//! files are the thin part around them.

use super::{Exit, Opts, fail, open_app_with};
use obop_brand::{logo, svg};
use obop_commands::brand::{self, Action, Failure, Fresh, LANGS, Reader, change_line};
use obop_commands::errors;
use obop_storage::Backend;
use serde_json::Value as J;
use std::process::ExitCode;
use std::sync::Arc;

/// The actor of the events a change made at a terminal writes to the security log.
const OPERATOR: &str = "operator";

pub(crate) const USAGE: &str = "usage:
  obop admin brand status
  obop admin brand configure [--name N] [--slug S] [--title LANG=TEXT]... [--message LANG=TEXT]...
                             [--logo-file FILE.svg | --clear-logo] [--from FILE.toml] [--dry-run]
  obop admin brand clear [--dry-run]
  obop admin brand check FILE.svg
(--db URL as for every command; LANG is one of en, de, fr, pl, fi, sv, es)";

/// What a command of the area prints on each stream and the status it ends with, kept apart from
/// the printing so that a test reads all three. Every line ends with a newline.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Said {
    pub out: String,
    pub err: String,
    pub code: u8,
}

impl Said {
    fn out(text: String) -> Said {
        Said {
            out: text,
            ..Said::default()
        }
    }

    fn failed(code: u8, err: String) -> Said {
        Said {
            err,
            code,
            ..Said::default()
        }
    }

    /// Writes both streams and ends with the status.
    fn print(self) -> Exit {
        print!("{}", self.out);
        eprint!("{}", self.err);
        match self.code {
            0 => Ok(()),
            code => Err(ExitCode::from(code)),
        }
    }
}

/// The status of a command line that is spelled wrong, as for the usage text of any command.
pub const EXIT_MISUSE: u8 = 2;

/// A command line that is spelled wrong: the line, and status [`EXIT_MISUSE`].
pub fn misuse(msg: &str) -> ExitCode {
    eprintln!("{msg}");
    ExitCode::from(EXIT_MISUSE)
}

/// The line and the status of a store that failed: for a refusal of the store (the D74 code word
/// at the start of its text) the line `obop call` prints and status 75, "try again later"; for any
/// other text `error: ` and the text, status 1. The text is the store's own, which never names a
/// host, a user or a statement.
pub fn storage_said(text: &str) -> Said {
    let (line, code) = match errors::of_storage_text(text) {
        Some(c) => (format!("error: {}", c.cli), c.exit),
        None => (format!("error: {text}"), 1),
    };
    Said::failed(code, format!("{line}\n"))
}

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

/// The global options that take a value: the ones `main` takes with `take_opt`, before it looks
/// at the command. Here to find the words of the area past any of them; a test reads `main.rs` and
/// holds this list to the `take_opt` calls in it, so a global option added there is added here.
pub const VALUE_OPTIONS: [&str; 31] = [
    "--db",
    "--token",
    "--scopes",
    "--groups",
    "--label",
    "--addr",
    "--static",
    "--provisioner",
    "--instance-static",
    "--title",
    "--impact",
    "--status",
    "--components",
    "--message",
    "--admin-prefix",
    "--out",
    "--note",
    "--mode",
    "--key",
    "--source-system",
    "--source-table",
    "--source-ids",
    "--on-existing",
    "--model",
    "--source-id",
    "--from",
    "--documents",
    "--account-types",
    "--tenant",
    "--client-id",
    "--year",
];

/// Where the words `admin brand` stand: at the start of the command line, past any leading global
/// options (`--db URL` and every other option of [`VALUE_OPTIONS`] with its value, and flags
/// without a value). `None` for every other command.
fn area_at(args: &[&str]) -> Option<usize> {
    let mut i = 0;
    while let Some(a) = args.get(i) {
        if !a.starts_with("--") {
            break;
        }
        i += if VALUE_OPTIONS.contains(a) { 2 } else { 1 };
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
/// takes a value and has none (`--from` included, which stays a global option), a `--title` or
/// `--message` that is not `LANG=TEXT` or names a language the record does not have, and a flag
/// given twice are refused.
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
            // `--from` stays a global option, which takes the next argument as its file. With none
            // it would yield no file, and the call would go on as if no file had been meant.
            "--from" => {
                rest.push(a);
                rest.push(value(a, "a file")?);
            }
            other => rest.push(other),
        }
    }
    Ok((Some(flags), rest))
}

/// The database for a command of the area. It seeds no demo data: `open_app` fills a new SQLite
/// database with fake parties, products and orders, and the first thing an operator does to a new
/// instance is `configure` (spec, section 14, the rollout), so the branding must not be the first
/// write to a database that then holds a demo company.
fn open(o: &Opts) -> Result<super::App, ExitCode> {
    open_app_with(o, false)
}

/// `obop admin brand ...`: `words` are the words after `admin brand`, `from` the global `--from`.
pub fn run(o: &Opts, words: &[&str], flags: Flags, from: Option<&str>, dry_run: bool) -> Exit {
    let takes_no_options = |verb: &str| {
        if flags.is_empty() && from.is_none() && !dry_run {
            Ok(())
        } else {
            Err(misuse(&format!(
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
                return Err(misuse(&format!(
                    "error: `admin brand clear` takes only --dry-run\n{USAGE}"
                )));
            }
            clear(o, dry_run)
        }
        ["configure"] => configure(o, &flags, from, dry_run),
        _ => {
            eprintln!("{USAGE}");
            Err(ExitCode::from(EXIT_MISUSE))
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
            return Err(misuse(&format!(
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
        return Err(misuse("error: give --logo-file or --clear-logo, not both"));
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
/// reader gives (a UTF-16 file, bytes that are not text; the reader has a reason for every input
/// that is not UTF-8), because a call carries text; a byte order mark is UTF-8 and goes on, to be
/// refused by the reader with its own reason.
fn read_logo(path: &str) -> Result<String, ExitCode> {
    let bytes = std::fs::read(path)
        .map_err(|e| fail(&format!("error: cannot read the logo file {path}: {e}")))?;
    String::from_utf8(bytes).map_err(|e| {
        for r in svg::parse(e.as_bytes()).err().unwrap_or_default() {
            eprintln!("error: logo_svg: {r}");
        }
        ExitCode::FAILURE
    })
}

fn configure(o: &Opts, flags: &Flags, from: Option<&str>, dry_run: bool) -> Exit {
    let fields = supplied(flags, from)?;
    if fields.is_empty() {
        return Err(misuse(&format!(
            "error: nothing to set: give a field flag or a --from file whose [record] table has fields\n{USAGE}"
        )));
    }
    let app = open(o)?;
    configure_over(&*app.store.backend(), fields, dry_run).print()
}

/// `configure` over a store, as the operator: what it prints and its status. A refusal is status 1
/// with every reason on a line of its own, a store that failed is [`storage_said`].
pub fn configure_over(backend: &dyn Backend, fields: Vec<(String, String)>, dry_run: bool) -> Said {
    let applied = match brand::apply(backend, None, OPERATOR, &Action::Configure(fields), dry_run) {
        Ok(applied) => applied,
        Err(f) => return failure_said(f),
    };
    if !applied.refusals.is_empty() {
        let mut err = String::new();
        for r in &applied.refusals {
            err.push_str(&format!("error: {}: {}\n", r.field, r.rule));
        }
        err.push_str("dry run: a real call would be refused; nothing was written\n");
        return Said::failed(1, err);
    }
    let mut out = String::new();
    if applied.changes.is_empty() {
        out.push_str(if dry_run {
            "dry run: nothing would change\n"
        } else {
            "nothing changed\n"
        });
    } else {
        out.push_str(if dry_run {
            "dry run, nothing was written; a real call would change:\n"
        } else {
            "changed:\n"
        });
        for c in &applied.changes {
            out.push_str(&format!("  {}\n", change_line(c, false)));
        }
        if !dry_run {
            out.push_str("recorded in the security log\n");
        }
    }
    if !applied.v.is_empty() {
        out.push_str(&format!("logo version: {}\n", applied.v));
    }
    Said::out(out)
}

fn clear(o: &Opts, dry_run: bool) -> Exit {
    let app = open(o)?;
    clear_over(&*app.store.backend(), dry_run).print()
}

/// `clear` over a store, as the operator: what it prints and its status.
pub fn clear_over(backend: &dyn Backend, dry_run: bool) -> Said {
    let applied = match brand::apply(backend, None, OPERATOR, &Action::Clear, dry_run) {
        Ok(applied) => applied,
        Err(f) => return failure_said(f),
    };
    if !applied.cleared {
        return Said::out("no brand is set; nothing to remove\n".to_string());
    }
    if dry_run && applied.changes.is_empty() {
        // The real call deletes a row with no key and writes no event, and the dry run says so.
        return Said::out(
            "dry run, nothing was written; a real call would remove the row, which holds no key, and record nothing in the security log\n"
                .to_string(),
        );
    }
    let mut out = String::from(if dry_run {
        "dry run, nothing was written; a real call would remove the brand:\n"
    } else {
        "brand removed:\n"
    });
    for c in &applied.changes {
        out.push_str(&format!("  {}\n", change_line(c, true)));
    }
    if !dry_run {
        // A row with no key at all is deleted with no event (`apply`), and the line says so.
        out.push_str(if applied.changes.is_empty() {
            "nothing was recorded in the security log: the row held no key\n"
        } else {
            "recorded in the security log\n"
        });
    }
    Said::out(out)
}

/// What a call that changed nothing says: every refusal on a line of its own, or the store's text.
fn failure_said(f: Failure) -> Said {
    match f {
        Failure::Refused(list) => {
            let mut err = String::new();
            for r in list {
                err.push_str(&format!("error: {}: {}\n", r.field, r.rule));
            }
            err.push_str("nothing was stored\n");
            Said::failed(1, err)
        }
        Failure::Storage(e) => storage_said(&e),
    }
}

fn status(o: &Opts) -> Exit {
    let app = open(o)?;
    status_over(app.store.backend()).print()
}

/// `status` over a store: the text of [`render_status`], or, for a store that cannot be read, its
/// error and no text at all. A failed read is never "no brand".
pub fn status_over(backend: Arc<dyn Backend>) -> Said {
    match Reader::new(backend).fresh() {
        Ok(fresh) => Said::out(render_status(&fresh)),
        Err(e) => storage_said(&e),
    }
}

/// What `status` prints: whether a brand is set, every value in force with the source that won
/// (the entries of the `effective` answer of `get`, so an entry a later slice adds shows here
/// without a change), the titles and messages that are stored (a language that shows another
/// language's title or the name through the fallback chain has no line of its own), and the stored
/// fields that fell back to a default. At a terminal the authenticator issuer is read from the
/// environment of the terminal's own process, which may not be the server's (spec, section 3).
pub fn render_status(fresh: &Fresh) -> String {
    render_status_with(fresh, brand::env_issuer())
}

/// As [`render_status`] with the value of `OBOP_MFA_ISSUER` given, so that a test does not depend on
/// the environment of the process.
pub fn render_status_with(fresh: &Fresh, issuer_variable: Option<String>) -> String {
    let shown = fresh.shown();
    let effective: J =
        serde_json::from_str(&fresh.effective_json_with(issuer_variable)).unwrap_or(J::Null);
    let mut out = String::new();
    // With no record the header says every value is the default, except what an explicit `site.*`
    // key of the shop sets (that entry has the source `site`) and what the variable `OBOP_MFA_ISSUER`
    // sets (the entry `authenticator_issuer` has the source `env`), and it names which of the two is
    // in force. With no record those are the only sources besides `default` and `theme`. The theme's
    // accent is not named: with a theme alone the header reads as it has since S1a, which the
    // no-record outputs hold, and the entry says `(theme)` on its own line.
    let sourced = |wanted: &str| {
        effective
            .as_object()
            .is_some_and(|entries| entries.values().any(|entry| entry["source"] == wanted))
    };
    let exceptions: Vec<&str> = [
        (sourced("site"), "a site.* key"),
        (sourced("env"), "OBOP_MFA_ISSUER"),
    ]
    .into_iter()
    .filter_map(|(in_force, name)| in_force.then_some(name))
    .collect();
    if fresh.set() {
        out.push_str("brand: set\n");
    } else if exceptions.is_empty() {
        out.push_str("brand: not set (no record; every value is the default)\n");
    } else {
        out.push_str(&format!(
            "brand: not set (no record; every value is the default, except what {} sets)\n",
            exceptions.join(" or ")
        ));
    }
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
    const LEADING: [&str; 4] = ["name", "slug", "logo", "tile_accent"];
    for key in LEADING {
        line(key, key == "logo");
    }
    let language = |key: &str| key.starts_with("title_") || key.starts_with("message_");
    for key in effective
        .as_object()
        .into_iter()
        .flat_map(|entries| entries.keys())
        .filter(|key| !LEADING.contains(&key.as_str()) && !language(key))
    {
        line(key, false);
    }
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
/// version of the logo it would have, or the reasons the file is refused (at most 20; the reasons
/// `configure --dry-run` gives for the artwork).
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
