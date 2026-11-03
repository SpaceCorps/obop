//! `obop modules describe`: a read-only description of a package's models and fields, built from
//! the same registry metadata the install gate uses (no database).
//!
//! The JSON shape is versioned (`schema_version`, currently 1). Within version 1 keys are only
//! ever added, never renamed or removed; a breaking change raises the version. Shape:
//!
//! ```text
//! { "schema_version": 1,
//!   "packages": [ { "id", "name", "version", "requires_core", "description",
//!                   "dependencies": [ { "id", "requires" } ],
//!                   "models": [ { "name", "description", "read_group", "write_group",
//!                                 "state_field": string|null,
//!                                 "actions": [ { "name", "permission_group", "needs_approval",
//!                                                "state_field", "from", "to" } ],
//!                                 "fields": [ { "name", "kind", "description", "data_class",
//!                                               "permission_group", "personal_data", "required",
//!                                               "unique", "unique_with", "options", "to",
//!                                               "precision", "scale", "round",
//!                                               "limits": null | { "min_length", "max_length", "min", "max" },
//!                                               "computed": string|null,
//!                                               "assigned": null | { "kind": "sequence", "format",
//!                                                    "assign_on", "gapless", "reset" } } ] } ] } ] }
//! ```
//!
//! Absent facts are `null` (or `[]` for lists), never omitted, so a consumer can rely on the keys.
//! `personal_data` is the registry's flag on the field; `data_class` is the declared class.

use obop_registry::{FieldDef, FieldKind, ModelDef, Module};
use serde::Serialize;

/// The version of the JSON shape.
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Serialize)]
pub struct Description {
    pub schema_version: u32,
    pub packages: Vec<PackageD>,
}

#[derive(Debug, Serialize)]
pub struct PackageD {
    pub id: String,
    pub name: String,
    pub version: String,
    pub requires_core: String,
    pub description: String,
    pub dependencies: Vec<DependencyD>,
    pub models: Vec<ModelD>,
}

#[derive(Debug, Serialize)]
pub struct DependencyD {
    pub id: String,
    pub requires: String,
}

#[derive(Debug, Serialize)]
pub struct ModelD {
    pub name: String,
    pub description: String,
    pub read_group: String,
    pub write_group: String,
    pub state_field: Option<String>,
    pub actions: Vec<ActionD>,
    pub fields: Vec<FieldD>,
}

#[derive(Debug, Serialize)]
pub struct ActionD {
    pub name: String,
    pub permission_group: String,
    pub needs_approval: bool,
    pub state_field: String,
    pub from: Option<String>,
    pub to: String,
}

#[derive(Debug, Serialize)]
pub struct LimitsD {
    pub min_length: Option<u32>,
    pub max_length: Option<u32>,
    pub min: Option<String>,
    pub max: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AssignedD {
    pub kind: &'static str,
    pub format: String,
    pub assign_on: Option<String>,
    pub gapless: bool,
    pub reset: &'static str,
}

#[derive(Debug, Serialize)]
pub struct FieldD {
    pub name: String,
    pub kind: &'static str,
    pub description: String,
    pub data_class: Option<&'static str>,
    pub permission_group: String,
    pub personal_data: bool,
    pub required: bool,
    pub unique: bool,
    pub unique_with: Vec<String>,
    pub options: Vec<String>,
    pub to: Option<String>,
    pub precision: Option<u8>,
    pub scale: Option<u8>,
    pub round: Option<&'static str>,
    pub limits: Option<LimitsD>,
    pub computed: Option<String>,
    pub assigned: Option<AssignedD>,
}

/// Describe parsed modules.
pub fn describe(modules: &[Module]) -> Description {
    Description {
        schema_version: SCHEMA_VERSION,
        packages: modules.iter().map(package).collect(),
    }
}

fn package(m: &Module) -> PackageD {
    PackageD {
        id: m.id.clone(),
        name: m.name.clone(),
        version: m.meta.version.clone(),
        requires_core: m.meta.requires_core.clone(),
        description: m.description.clone(),
        dependencies: m
            .meta
            .dependencies
            .iter()
            .map(|(id, req)| DependencyD {
                id: id.clone(),
                requires: req.clone(),
            })
            .collect(),
        models: m.models.iter().map(model).collect(),
    }
}

fn model(m: &ModelDef) -> ModelD {
    let mut actions = Vec::new();
    for c in &m.commands {
        if let Some(t) = &c.transition {
            actions.push(ActionD {
                name: c.name.to_string(),
                permission_group: c.permission_group.to_string(),
                needs_approval: c.needs_approval,
                state_field: t.field.to_string(),
                from: t.from.map(str::to_string),
                to: t.to.to_string(),
            });
        }
    }
    let state_field = actions.first().map(|a| a.state_field.clone());
    ModelD {
        name: m.name.to_string(),
        description: m.description.to_string(),
        read_group: m.read_group.to_string(),
        write_group: m.write_group.to_string(),
        state_field,
        actions,
        fields: m.fields.iter().map(field).collect(),
    }
}

fn field(f: &FieldDef) -> FieldD {
    let (precision, scale, round) = match &f.kind {
        FieldKind::Decimal {
            precision,
            scale,
            round,
        } => (Some(*precision), Some(*scale), round.map(|r| r.name())),
        _ => (None, None, None),
    };
    FieldD {
        name: f.name.to_string(),
        kind: f.kind.name(),
        description: f.description.to_string(),
        data_class: f.class.map(|c| c.as_str()),
        permission_group: f.permission_group.to_string(),
        personal_data: f.personal_data,
        required: f.required,
        unique: f.unique.is_some(),
        unique_with: f
            .unique
            .as_ref()
            .map(|u| u.with.iter().map(|w| w.to_string()).collect())
            .unwrap_or_default(),
        options: match &f.kind {
            FieldKind::Select(o) => o.iter().map(|s| s.to_string()).collect(),
            _ => Vec::new(),
        },
        to: match &f.kind {
            FieldKind::Ref(t) => Some(t.to_string()),
            _ => None,
        },
        precision,
        scale,
        round,
        limits: f
            .limits
            .as_ref()
            .filter(|l| !l.is_empty())
            .map(|l| LimitsD {
                min_length: l.min_length,
                max_length: l.max_length,
                min: l.min.map(|b| b.to_string()),
                max: l.max.map(|b| b.to_string()),
            }),
        computed: f.computed.as_ref().map(|c| c.source.clone()),
        assigned: f.sequence.as_ref().map(|s| AssignedD {
            kind: "sequence",
            format: s.format.source().to_string(),
            assign_on: s.assign_on.map(str::to_string),
            gapless: s.gapless,
            reset: s.reset.name(),
        }),
    }
}

/// The JSON text (pretty, trailing newline).
pub fn to_json(d: &Description) -> String {
    let mut s = serde_json::to_string_pretty(d).expect("a description serialises");
    s.push('\n');
    s
}

/// A readable indented text.
pub fn to_text(d: &Description) -> String {
    let mut o = String::new();
    for p in &d.packages {
        o.push_str(&format!(
            "package {} {} (requires core {})\n",
            p.id, p.version, p.requires_core
        ));
        if !p.name.is_empty() {
            o.push_str(&format!("  name: {}\n", p.name));
        }
        if !p.description.is_empty() {
            o.push_str(&format!("  {}\n", p.description));
        }
        for dep in &p.dependencies {
            o.push_str(&format!("  depends on {} {}\n", dep.id, dep.requires));
        }
        for m in &p.models {
            o.push_str(&format!("\n  model {}\n", m.name));
            o.push_str(&format!("    {}\n", m.description));
            o.push_str(&format!(
                "    read: {}   write: {}\n",
                m.read_group, m.write_group
            ));
            if let Some(sf) = &m.state_field {
                o.push_str(&format!("    governed state field: {sf}\n"));
            }
            for a in &m.actions {
                o.push_str(&format!(
                    "    action {}: {} -> {}  group {}{}\n",
                    a.name,
                    a.from.as_deref().unwrap_or("any"),
                    a.to,
                    a.permission_group,
                    if a.needs_approval {
                        "  (needs approval)"
                    } else {
                        ""
                    }
                ));
            }
            let w = m.fields.iter().map(|f| f.name.len()).max().unwrap_or(4);
            o.push_str("    fields:\n");
            for f in &m.fields {
                let mut kind = f.kind.to_string();
                if let Some(t) = &f.to {
                    kind = format!("ref -> {t}");
                }
                if let (Some(p), Some(s)) = (f.precision, f.scale) {
                    kind = format!("decimal({p},{s})");
                    if let Some(r) = f.round {
                        kind.push_str(&format!(" round {r}"));
                    }
                }
                let mut notes: Vec<String> = vec![format!("class {}", f.data_class.unwrap_or("?"))];
                if f.personal_data {
                    notes.push("personal data".into());
                }
                if f.required {
                    notes.push("required".into());
                }
                if f.unique {
                    if f.unique_with.is_empty() {
                        notes.push("unique".into());
                    } else {
                        notes.push(format!("unique with {}", f.unique_with.join(",")));
                    }
                }
                if !f.options.is_empty() {
                    notes.push(format!("options {}", f.options.join("|")));
                }
                if let Some(l) = &f.limits {
                    if let Some(v) = l.min_length {
                        notes.push(format!("min_length {v}"));
                    }
                    if let Some(v) = l.max_length {
                        notes.push(format!("max_length {v}"));
                    }
                    if let Some(v) = &l.min {
                        notes.push(format!("min {v}"));
                    }
                    if let Some(v) = &l.max {
                        notes.push(format!("max {v}"));
                    }
                }
                if let Some(c) = &f.computed {
                    notes.push(format!("computed {c}"));
                }
                if let Some(a) = &f.assigned {
                    notes.push(format!(
                        "assigned by the engine: {} {}{}{}",
                        a.kind,
                        a.format,
                        a.assign_on
                            .as_deref()
                            .map(|x| format!(" on {x}"))
                            .unwrap_or_default(),
                        if a.gapless { " gapless" } else { "" }
                    ));
                }
                o.push_str(&format!(
                    "      {:<w$}  {:<22} {}\n",
                    f.name,
                    kind,
                    notes.join("; "),
                    w = w
                ));
            }
        }
        o.push('\n');
    }
    o
}

/// Describe package documents (the text of an `obop.toml`, labels already combined), given as
/// `(name, text)`. The first error names the document.
pub fn describe_texts(docs: &[(String, String)]) -> Result<Description, String> {
    let mut modules = Vec::new();
    for (name, text) in docs {
        let manifest = crate::Manifest::parse(text).map_err(|e| format!("{name}: {e}"))?;
        modules.push(manifest.to_module().map_err(|e| format!("{name}: {e}"))?);
    }
    Ok(describe(&modules))
}
