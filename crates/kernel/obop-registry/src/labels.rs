//! Declarative translations of module metadata (D52): the display names and descriptions of a
//! module, its models, fields, select options and actions, per language.
//!
//! The same document shape serves a package (`[labels.<lang>...]` tables inside or next to its
//! `obop.toml`) and the built-in modules (one file per language). English stays the source
//! (the manifest's own `description` fields; the client humanises technical names for labels);
//! a language with no entry falls back to it. Translations never change what a name *is*: the
//! technical identifiers stay English everywhere (commands, MCP, CLI, stored data).

use crate::Module;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

/// Language code to text.
pub type Texts = BTreeMap<String, String>;

pub const MAX_LABEL_CHARS: usize = 80;
pub const MAX_DESCRIPTION_CHARS: usize = 400;
/// The same limit as the manifest's own one-line `summary`.
pub const MAX_SUMMARY_CHARS: usize = 140;

/// A name and a description in each language: the entry for a module, model, field or action.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entry {
    /// The display name per language (the "label").
    pub labels: Texts,
    pub descriptions: Texts,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FieldTr {
    pub entry: Entry,
    /// Option value -> language -> label.
    pub options: BTreeMap<String, Texts>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModelTr {
    pub entry: Entry,
    pub fields: BTreeMap<String, FieldTr>,
    pub actions: BTreeMap<String, Entry>,
}

/// All translations of one module.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Translations {
    pub module: Entry,
    /// The one-line card text of the module per language (`summary` under `[labels.<lang>.module]`).
    pub summaries: Texts,
    pub models: BTreeMap<String, ModelTr>,
}

// --- the document shape (TOML) -------------------------------------------------------------

/// `[labels.<lang>]`
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LangLabels {
    pub module: Option<NameDesc>,
    #[serde(default)]
    pub model: BTreeMap<String, ModelLabels>,
}

/// `name`, `description` and `summary`: a module.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NameDesc {
    pub name: Option<String>,
    pub description: Option<String>,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ModelLabels {
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub field: BTreeMap<String, FieldLabels>,
    #[serde(default)]
    pub action: BTreeMap<String, ActionLabels>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FieldLabels {
    pub label: Option<String>,
    pub description: Option<String>,
    /// Select option value -> label.
    #[serde(default)]
    pub options: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ActionLabels {
    pub label: Option<String>,
    pub description: Option<String>,
}

/// A stand-alone labels document: only `[labels.<lang>]` tables.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LabelFile {
    #[serde(default)]
    pub labels: BTreeMap<String, LangLabels>,
}

/// Whether `code` is an accepted language tag: two or three lowercase letters, optionally a
/// region (`de`, `fil`, `pt-BR`).
pub fn lang_ok(code: &str) -> bool {
    let (lang, region) = match code.split_once('-') {
        Some((l, r)) => (l, Some(r)),
        None => (code, None),
    };
    (2..=3).contains(&lang.len())
        && lang.bytes().all(|b| b.is_ascii_lowercase())
        && region.is_none_or(|r| r.len() == 2 && r.bytes().all(|b| b.is_ascii_uppercase()))
}

/// The value rules: non-empty, trimmed, one line, no control characters, no placeholders or
/// markup (`{`, `}`, `<`, `>`), at most `max` characters.
pub fn text_ok(s: &str, max: usize) -> Result<(), &'static str> {
    if s.trim().is_empty() {
        Err("is empty")
    } else if s != s.trim() {
        Err("has leading or trailing whitespace")
    } else if s.chars().any(char::is_control) {
        Err("contains a control character or line break")
    } else if s.chars().any(|c| matches!(c, '{' | '}' | '<' | '>')) {
        Err("contains a placeholder or markup character ({ } < >); labels are plain text")
    } else if s.chars().count() > max {
        Err("is too long")
    } else {
        Ok(())
    }
}

/// What a label document may refer to: the models of a package with their fields (and the
/// options of a select) and actions.
#[derive(Debug, Clone, Default)]
pub struct Shape {
    pub models: BTreeMap<String, ModelShape>,
}

#[derive(Debug, Clone, Default)]
pub struct ModelShape {
    /// Field name -> options (`Some` for a select).
    pub fields: BTreeMap<String, Option<BTreeSet<String>>>,
    pub actions: BTreeSet<String>,
}

impl Shape {
    pub fn of(module: &Module) -> Self {
        Self {
            models: module
                .models
                .iter()
                .map(|m| {
                    (
                        m.name.to_string(),
                        ModelShape {
                            fields: m
                                .fields
                                .iter()
                                .map(|f| {
                                    let o = f.kind.options();
                                    (
                                        f.name.to_string(),
                                        (!o.is_empty())
                                            .then(|| o.iter().map(|x| x.to_string()).collect()),
                                    )
                                })
                                .collect(),
                            actions: m.commands.iter().map(|c| c.name.to_string()).collect(),
                        },
                    )
                })
                .collect(),
        }
    }
}

fn val(at: String, v: &Option<String>, max: usize) -> Result<(), String> {
    if let Some(v) = v {
        text_ok(v, max).map_err(|e| format!("labels.{at}: value {e}"))?;
    }
    Ok(())
}

/// Check a set of `[labels.<lang>]` tables: language codes, values, and that every key refers to
/// something in `shape` (a typo is an error, never silently ignored).
pub fn check_labels(labels: &BTreeMap<String, LangLabels>, shape: &Shape) -> Result<(), String> {
    for (lang, l) in labels {
        if !lang_ok(lang) {
            return Err(format!(
                "labels.{lang}: not a language code (use two or three lowercase letters, optionally a region: de, pl, pt-BR)"
            ));
        }
        if let Some(m) = &l.module {
            val(format!("{lang}.module.name"), &m.name, MAX_LABEL_CHARS)?;
            val(
                format!("{lang}.module.description"),
                &m.description,
                MAX_DESCRIPTION_CHARS,
            )?;
            val(
                format!("{lang}.module.summary"),
                &m.summary,
                MAX_SUMMARY_CHARS,
            )?;
        }
        for (model, ml) in &l.model {
            let at = format!("{lang}.model.{model}");
            let Some(ms) = shape.models.get(model) else {
                return Err(format!("labels.{at}: the package has no model {model:?}"));
            };
            val(format!("{at}.name"), &ml.name, MAX_LABEL_CHARS)?;
            val(
                format!("{at}.description"),
                &ml.description,
                MAX_DESCRIPTION_CHARS,
            )?;
            for (field, fl) in &ml.field {
                let at = format!("{at}.field.{field}");
                let Some(opts) = ms.fields.get(field) else {
                    return Err(format!("labels.{at}: {model} has no field {field:?}"));
                };
                val(format!("{at}.label"), &fl.label, MAX_LABEL_CHARS)?;
                val(
                    format!("{at}.description"),
                    &fl.description,
                    MAX_DESCRIPTION_CHARS,
                )?;
                for (o, text) in &fl.options {
                    let Some(opts) = opts else {
                        return Err(format!(
                            "labels.{at}.options: {field} is not a select field"
                        ));
                    };
                    if !opts.contains(o) {
                        return Err(format!(
                            "labels.{at}.options.{o}: {field} has no option {o:?}"
                        ));
                    }
                    val(
                        format!("{at}.options.{o}"),
                        &Some(text.clone()),
                        MAX_LABEL_CHARS,
                    )?;
                }
            }
            for (action, al) in &ml.action {
                let at = format!("{at}.action.{action}");
                if !ms.actions.contains(action) {
                    return Err(format!("labels.{at}: {model} has no action {action:?}"));
                }
                val(format!("{at}.label"), &al.label, MAX_LABEL_CHARS)?;
                val(
                    format!("{at}.description"),
                    &al.description,
                    MAX_DESCRIPTION_CHARS,
                )?;
            }
        }
    }
    Ok(())
}

impl Translations {
    /// Build the runtime form from checked tables.
    pub fn from_labels(labels: &BTreeMap<String, LangLabels>) -> Self {
        let mut t = Self::default();
        let put = |m: &mut Texts, lang: &str, v: &Option<String>| {
            if let Some(v) = v {
                m.insert(lang.to_string(), v.clone());
            }
        };
        for (lang, l) in labels {
            if let Some(m) = &l.module {
                put(&mut t.module.labels, lang, &m.name);
                put(&mut t.module.descriptions, lang, &m.description);
                put(&mut t.summaries, lang, &m.summary);
            }
            for (model, ml) in &l.model {
                let mt = t.models.entry(model.clone()).or_default();
                put(&mut mt.entry.labels, lang, &ml.name);
                put(&mut mt.entry.descriptions, lang, &ml.description);
                for (field, fl) in &ml.field {
                    let ft = mt.fields.entry(field.clone()).or_default();
                    put(&mut ft.entry.labels, lang, &fl.label);
                    put(&mut ft.entry.descriptions, lang, &fl.description);
                    for (o, text) in &fl.options {
                        ft.options
                            .entry(o.clone())
                            .or_default()
                            .insert(lang.clone(), text.clone());
                    }
                }
                for (action, al) in &ml.action {
                    let at = mt.actions.entry(action.clone()).or_default();
                    put(&mut at.labels, lang, &al.label);
                    put(&mut at.descriptions, lang, &al.description);
                }
            }
        }
        t
    }

    /// Parse a stand-alone label document (or several concatenated); values and language codes
    /// are checked later by `check_labels`.
    pub fn parse_labels(text: &str) -> Result<BTreeMap<String, LangLabels>, String> {
        let f: LabelFile = toml::from_str(text).map_err(|e| format!("invalid labels: {e}"))?;
        Ok(f.labels)
    }

    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

impl Module {
    /// Attach translations from label documents (the files of a built-in module, one per
    /// language, each `[labels.<lang>...]`). The documents are compiled in, so a document that
    /// does not parse or names something the module lacks is a programming error and panics;
    /// the tests that build every module catch it before release.
    pub fn translated(mut self, docs: &[&str]) -> Self {
        let mut all: BTreeMap<String, LangLabels> = BTreeMap::new();
        for d in docs {
            let parsed = Translations::parse_labels(d)
                .unwrap_or_else(|e| panic!("labels of {}: {e}", self.id));
            for (lang, l) in parsed {
                all.entry(lang).or_default().merge(l);
            }
        }
        if let Err(e) = check_labels(&all, &Shape::of(&self)) {
            panic!("labels of {}: {e}", self.id);
        }
        self.translations = std::sync::Arc::new(Translations::from_labels(&all));
        self
    }
}

impl LangLabels {
    fn merge(&mut self, o: LangLabels) {
        if o.module.is_some() {
            self.module = o.module;
        }
        for (k, v) in o.model {
            self.model.insert(k, v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_codes() {
        for ok in ["de", "pl", "fil", "pt-BR", "en-GB"] {
            assert!(lang_ok(ok), "{ok}");
        }
        for bad in [
            "", "d", "DE", "deu1", "de-de", "de_DE", "de-", "english", "zh-Hans",
        ] {
            assert!(!lang_ok(bad), "{bad}");
        }
    }

    #[test]
    fn a_module_summary_is_translated_and_checked() {
        let shape = Shape::default();
        let parse = |s: &str| Translations::parse_labels(s).unwrap();
        let ok =
            parse("[labels.de.module]\nname = \"Fuhrpark\"\nsummary = \"Fahrzeuge und Fahrer.\"");
        assert!(check_labels(&ok, &shape).is_ok());
        assert_eq!(
            Translations::from_labels(&ok).summaries["de"],
            "Fahrzeuge und Fahrer."
        );
        let long = format!("[labels.de.module]\nsummary = \"{}\"", "x".repeat(141));
        assert!(
            check_labels(&parse(&long), &shape)
                .unwrap_err()
                .contains("module.summary")
        );
        let markup = parse("[labels.de.module]\nsummary = \"<b>x</b>\"");
        assert!(check_labels(&markup, &shape).is_err());
    }

    #[test]
    fn value_rules() {
        assert!(text_ok("Fahrzeug", 80).is_ok());
        assert!(text_ok("Kilometerstand (km)", 80).is_ok());
        assert!(text_ok("", 80).is_err());
        assert!(text_ok("  ", 80).is_err());
        assert!(text_ok(" x", 80).is_err());
        assert!(text_ok("a\nb", 80).is_err());
        assert!(text_ok("a\u{7}b", 80).is_err());
        assert!(text_ok("Hallo {name}", 80).is_err());
        assert!(text_ok("<b>x</b>", 80).is_err());
        assert!(text_ok(&"x".repeat(81), 80).is_err());
    }
}
