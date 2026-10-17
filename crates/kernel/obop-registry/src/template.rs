//! The translatable slots of a module, label templates and completeness checks (D52).

use crate::Module;
use crate::labels::{Entry, Translations};

/// What kind of text a slot holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotKind {
    ModuleName,
    ModuleDescription,
    ModelName,
    ModelDescription,
    FieldLabel,
    FieldDescription,
    OptionLabel,
    ActionLabel,
    ActionDescription,
}

/// One thing a translator fills in, with its English source text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    pub kind: SlotKind,
    pub model: String,
    /// Field or action name; empty for the module and model slots.
    pub item: String,
    /// The option value, for `OptionLabel`.
    pub option: String,
    /// What the interface shows without a translation.
    pub english: String,
}

/// "expected_revenue_minor" -> "Expected revenue": the English label the client derives from a
/// technical name (kept in step with `label` in `web/apps/backoffice/src/format.ts`).
pub fn humanise(name: &str) -> String {
    let s = name.strip_suffix("_minor").unwrap_or(name);
    let s = s.strip_suffix("_id").unwrap_or(s);
    capitalise(&s.replace('_', " "))
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

/// Every translatable text of `module`, in document order.
pub fn slots(module: &Module) -> Vec<Slot> {
    let mut v = Vec::new();
    let mut push = |kind, model: &str, item: &str, option: &str, english: &str| {
        v.push(Slot {
            kind,
            model: model.into(),
            item: item.into(),
            option: option.into(),
            english: english.into(),
        })
    };
    push(SlotKind::ModuleName, "", "", "", &module.name);
    if !module.description.is_empty() {
        push(SlotKind::ModuleDescription, "", "", "", &module.description);
    }
    for m in &module.models {
        let own = m
            .name
            .strip_prefix(&format!("{}_", module.id))
            .unwrap_or(m.name);
        push(SlotKind::ModelName, m.name, "", "", &humanise(own));
        push(SlotKind::ModelDescription, m.name, "", "", m.description);
        for f in &m.fields {
            push(SlotKind::FieldLabel, m.name, f.name, "", &humanise(f.name));
            push(
                SlotKind::FieldDescription,
                m.name,
                f.name,
                "",
                f.description,
            );
            for o in f.kind.options() {
                push(
                    SlotKind::OptionLabel,
                    m.name,
                    f.name,
                    o,
                    &capitalise(&o.replace('_', " ")),
                );
            }
        }
        for c in &m.commands {
            push(SlotKind::ActionLabel, m.name, c.name, "", &humanise(c.name));
            push(
                SlotKind::ActionDescription,
                m.name,
                c.name,
                "",
                c.description,
            );
        }
    }
    v
}

impl Slot {
    /// The text in `t` for `lang`, if any.
    pub fn get<'a>(&self, t: &'a Translations, lang: &str) -> Option<&'a str> {
        let pick =
            |e: &'a Entry, labels: bool| if labels { &e.labels } else { &e.descriptions }.get(lang);
        let model = t.models.get(&self.model);
        let field = model.and_then(|m| m.fields.get(&self.item));
        let action = model.and_then(|m| m.actions.get(&self.item));
        match self.kind {
            SlotKind::ModuleName => pick(&t.module, true),
            SlotKind::ModuleDescription => pick(&t.module, false),
            SlotKind::ModelName => model.and_then(|m| pick(&m.entry, true)),
            SlotKind::ModelDescription => model.and_then(|m| pick(&m.entry, false)),
            SlotKind::FieldLabel => field.and_then(|f| pick(&f.entry, true)),
            SlotKind::FieldDescription => field.and_then(|f| pick(&f.entry, false)),
            SlotKind::OptionLabel => field
                .and_then(|f| f.options.get(&self.option))
                .and_then(|o| o.get(lang)),
            SlotKind::ActionLabel => action.and_then(|a| pick(a, true)),
            SlotKind::ActionDescription => action.and_then(|a| pick(a, false)),
        }
        .map(String::as_str)
    }

    /// A human path for messages: `fleet_vehicle.fuel.option.petrol`.
    pub fn path(&self) -> String {
        let k = match self.kind {
            SlotKind::ModuleName => "module.name",
            SlotKind::ModuleDescription => "module.description",
            SlotKind::ModelName => "name",
            SlotKind::ModelDescription => "description",
            SlotKind::FieldLabel => "label",
            SlotKind::FieldDescription => "description",
            SlotKind::OptionLabel => "option",
            SlotKind::ActionLabel => "label",
            SlotKind::ActionDescription => "description",
        };
        let mut parts: Vec<&str> = [&self.model, &self.item, &self.option]
            .into_iter()
            .filter(|x| !x.is_empty())
            .map(String::as_str)
            .collect();
        parts.push(k);
        parts.join(".")
    }
}

/// The slots of `module` that `t` leaves empty or identical to the English source in `lang`,
/// except texts in `allow` (proper nouns and international words that read the same).
pub fn incomplete(module: &Module, t: &Translations, lang: &str, allow: &[String]) -> Vec<String> {
    slots(module)
        .into_iter()
        .filter_map(|s| match s.get(t, lang) {
            None => Some(format!("{} ({lang}): missing", s.path())),
            Some(v) if v == s.english && !allow.iter().any(|a| a == v) => {
                Some(format!("{} ({lang}): identical to English {v:?}", s.path()))
            }
            _ => None,
        })
        .collect()
}

fn toml_str(s: &str) -> String {
    let mut o = String::from('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn toml_key(k: &str) -> String {
    if !k.is_empty()
        && k.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        k.to_string()
    } else {
        toml_str(k)
    }
}

/// A complete label document for `lang`, every value the English source: a translator replaces
/// the values and the file is done (`obop modules labels`).
pub fn template(module: &Module, lang: &str) -> String {
    let mut out = String::new();
    let mut last = String::new();
    for s in slots(module) {
        let model = toml_key(&s.model);
        let item = toml_key(&s.item);
        let (table, key) = match s.kind {
            SlotKind::ModuleName => (format!("labels.{lang}.module"), "name"),
            SlotKind::ModuleDescription => (format!("labels.{lang}.module"), "description"),
            SlotKind::ModelName => (format!("labels.{lang}.model.{model}"), "name"),
            SlotKind::ModelDescription => (format!("labels.{lang}.model.{model}"), "description"),
            SlotKind::FieldLabel => (format!("labels.{lang}.model.{model}.field.{item}"), "label"),
            SlotKind::FieldDescription => (
                format!("labels.{lang}.model.{model}.field.{item}"),
                "description",
            ),
            SlotKind::OptionLabel => (
                format!("labels.{lang}.model.{model}.field.{item}.options"),
                s.option.as_str(),
            ),
            SlotKind::ActionLabel => (
                format!("labels.{lang}.model.{model}.action.{item}"),
                "label",
            ),
            SlotKind::ActionDescription => (
                format!("labels.{lang}.model.{model}.action.{item}"),
                "description",
            ),
        };
        if table != last {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&format!("[{table}]\n"));
            last = table;
        }
        out.push_str(&format!("{} = {}\n", toml_key(key), toml_str(&s.english)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::labels::{Shape, Translations, check_labels};
    use crate::{CommandDef, FieldDef, ModelDef};
    use obop_types::DataClass;

    fn module() -> Module {
        Module {
            id: "shop".into(),
            name: "Shop".into(),
            description: "Sells things".into(),
            models: vec![ModelDef {
                name: "shop_item",
                description: "Something for sale",
                read_group: "shop.user",
                write_group: "shop.user",
                access: None,
                fields: vec![
                    FieldDef::new("title", "What it is", DataClass::None, false, "shop.user"),
                    FieldDef::new("state", "Where it is", DataClass::None, false, "shop.user")
                        .select(&["on_sale", "sold"]),
                ],
                commands: vec![
                    CommandDef::new("sell", "Mark as sold", "shop.user").transition(
                        "state",
                        Some("on_sale"),
                        "sold",
                    ),
                ],
            }],
            ..Default::default()
        }
    }

    #[test]
    fn the_template_parses_checks_and_is_all_english() {
        let m = module();
        let t = template(&m, "de");
        assert!(
            t.contains("[labels.de.model.shop_item.field.state.options]\non_sale = \"On sale\"")
        );
        let labels = Translations::parse_labels(&t).unwrap();
        check_labels(&labels, &Shape::of(&m)).unwrap();
        let tr = Translations::from_labels(&labels);
        let missing = incomplete(&m, &tr, "de", &[]);
        assert_eq!(
            missing.len(),
            slots(&m).len(),
            "every slot is still English"
        );
        assert!(incomplete(&m, &tr, "fr", &[])[0].contains("missing"));
        let allow = vec!["Shop".to_string()];
        assert_eq!(incomplete(&m, &tr, "de", &allow).len(), missing.len() - 1);
    }

    #[test]
    fn translated_attaches_checked_documents_and_panics_on_a_typo() {
        let doc = "[labels.de.model.shop_item]\nname = \"Artikel\"\n[labels.fi.module]\nname = \"Kauppa\"";
        let m = module().translated(&[doc]);
        assert_eq!(
            m.translations.models["shop_item"].entry.labels["de"],
            "Artikel"
        );
        assert_eq!(m.translations.module.labels["fi"], "Kauppa");
        let bad = std::panic::catch_unwind(|| {
            module().translated(&["[labels.de.model.shop_itemm]\nname = \"x\""])
        });
        assert!(bad.is_err());
    }

    #[test]
    fn humanise_matches_the_client() {
        assert_eq!(humanise("expected_revenue_minor"), "Expected revenue");
        assert_eq!(humanise("party_id"), "Party");
        assert_eq!(humanise("return_to_pool"), "Return to pool");
    }
}
