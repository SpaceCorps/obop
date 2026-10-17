//! Package files on disk (`obop modules check|install FILE`, `obop modules labels`): a package is
//! one document, but an author may keep the translations in separate files (D52).
//!
//! For `PATH/<id>.toml` the following are appended when they exist, in this order:
//! `PATH/<id>.labels.toml` and every `PATH/labels/<lang>/<id>.toml` (the layout of the bundled
//! catalog). All are plain TOML, so the combined document is their concatenation.
//! A file that holds only `[labels...]` tables (`<id>.labels.toml` or `labels/<lang>/<id>.toml`)
//! is checked on its own against the package (or built-in module) it translates.

use crate::manifest::Manifest;
use obop_registry::Module;
use obop_registry::labels::{Shape, Translations, check_labels};
use std::path::{Path, PathBuf};

/// What a path names.
pub enum Source {
    /// A package: the manifest text with its sibling labels appended.
    Package(String),
    /// A labels-only document for package or module `stem`; `manifest` is the package text
    /// (English only) when one sits next to it.
    Labels {
        stem: String,
        text: String,
        manifest: Option<String>,
        /// English texts that may stay identical in translation (`labels/identical.json` and
        /// `labels/<lang>/identical.json`, when the file sits in the bundled layout).
        allow: Vec<String>,
    },
}

/// The allow-list for `lang` in a `labels/` directory.
pub fn allow_list(labels_dir: &Path, lang: &str) -> Vec<String> {
    let mut v = Vec::new();
    for f in [
        labels_dir.join("identical.json"),
        labels_dir.join(lang).join("identical.json"),
    ] {
        if let Ok(t) = std::fs::read_to_string(f)
            && let Ok(list) = serde_json::from_str::<Vec<String>>(&t)
        {
            v.extend(list);
        }
    }
    v
}

fn is_labels_file(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    name.ends_with(".labels.toml")
        || path
            .parent()
            .and_then(Path::parent)
            .and_then(Path::file_name)
            .is_some_and(|d| d == "labels")
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// The paths of the sibling labels documents of `dir/<stem>.toml`.
fn siblings(dir: &Path, stem: &str) -> Vec<PathBuf> {
    let mut v = Vec::new();
    let one = dir.join(format!("{stem}.labels.toml"));
    if one.is_file() {
        v.push(one);
    }
    if let Ok(rd) = std::fs::read_dir(dir.join("labels")) {
        let mut langs: Vec<PathBuf> = rd.filter_map(|e| e.ok()).map(|e| e.path()).collect();
        langs.sort();
        for l in langs {
            let f = l.join(format!("{stem}.toml"));
            if f.is_file() {
                v.push(f);
            }
        }
    }
    v
}

/// Read what `path` names.
pub fn read_source(path: &Path) -> Result<Source, String> {
    let text = read(path)?;
    let dir = path.parent().unwrap_or(Path::new("."));
    if !is_labels_file(path) {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let mut all = text;
        for s in siblings(dir, stem) {
            all = Manifest::combine(&all, &read(&s)?);
        }
        return Ok(Source::Package(all));
    }
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let (stem, base) = match name.strip_suffix(".labels.toml") {
        Some(stem) => (stem.to_string(), dir.to_path_buf()),
        None => (
            name.trim_end_matches(".toml").to_string(),
            dir.parent()
                .and_then(Path::parent)
                .unwrap_or(Path::new("."))
                .to_path_buf(),
        ),
    };
    let manifest = std::fs::read_to_string(base.join(format!("{stem}.toml"))).ok();
    let lang = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let allow = if name.ends_with(".labels.toml") {
        Vec::new()
    } else {
        allow_list(base.join("labels").as_path(), lang)
    };
    Ok(Source::Labels {
        stem,
        text,
        manifest,
        allow,
    })
}

/// The install text for `path`: the manifest with its labels.
pub fn read_package(path: &Path) -> Result<String, String> {
    match read_source(path)? {
        Source::Package(t) => Ok(t),
        Source::Labels { .. } => Err(format!(
            "{} holds only labels; install the package file it translates",
            path.display()
        )),
    }
}

/// Problems with a labels-only document against the package or built-in module it translates.
///
/// With `complete`, every language in the document must also translate every text (none missing,
/// none identical to English unless in `allow`).
pub fn check_labels_source(
    stem: &str,
    text: &str,
    manifest: Option<&str>,
    allow: &[String],
    complete: bool,
    builtin: &[Module],
) -> Vec<String> {
    let labels = match Translations::parse_labels(text) {
        Ok(l) => l,
        Err(e) => return vec![e],
    };
    let module = if let Some(m) = manifest {
        match Manifest::parse(m).and_then(|m| m.to_module()) {
            Ok(m) => m,
            Err(e) => return vec![format!("the package {stem}.toml: {e}")],
        }
    } else if let Some(b) = builtin.iter().find(|b| b.id == stem) {
        b.clone()
    } else {
        return vec![format!(
            "cannot find the package {stem}.toml next to the labels, nor a built-in module {stem}"
        )];
    };
    if let Err(e) = check_labels(&labels, &Shape::of(&module)) {
        return vec![e];
    }
    if !complete {
        return Vec::new();
    }
    let t = Translations::from_labels(&labels);
    labels
        .keys()
        .flat_map(|lang| obop_registry::template::incomplete(&module, &t, lang, allow))
        .collect()
}

/// A label template (`obop modules labels`) for a built-in module, a bundled package, or a
/// package file.
pub fn template_for(target: &str, lang: &str, builtin: &[Module]) -> Result<String, String> {
    if !obop_registry::labels::lang_ok(lang) {
        return Err(format!("{lang:?} is not a language code (de, pl, pt-BR)"));
    }
    let module = if let Some(b) = builtin.iter().find(|b| b.id == target) {
        b.clone()
    } else {
        use crate::marketplace::{Bundled, Marketplace};
        let text = if Path::new(target).is_file() {
            read_package(Path::new(target))?
        } else {
            Bundled.fetch(target).map_err(|_| {
                format!("{target} is neither a built-in module, a bundled package nor a file")
            })?
        };
        Manifest::parse(&text)?.to_module()?
    };
    Ok(obop_registry::template::template(&module, lang))
}
