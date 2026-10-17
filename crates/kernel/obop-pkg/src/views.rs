//! The rows the admin commands return (`module.list`, `module.get`, `marketplace_listing.list`,
//! `module_plan.preview`): catalogue metadata plus what is computed against this instance, such
//! as compatibility with the running core, how each dependency stands, and who depends on whom.

use crate::manifest::Manifest;
use crate::marketplace::Listing;
use crate::resolve::{Blocker, Plan};
use crate::semver::{Req, Version};
use crate::spdx::{self, LicenseKind};
use obop_commands::Value;
use obop_registry::{GENERATED_COMMANDS, Module, ModuleMeta};
use std::collections::BTreeMap;

/// What a view is computed against.
pub struct Ctx<'a> {
    pub core: &'a Version,
    /// Built-in modules by id.
    pub builtin: &'a [Module],
    pub installed: &'a BTreeMap<String, Manifest>,
    /// What the marketplace offers, by id.
    pub offered: &'a BTreeMap<String, Listing>,
}

pub fn text(v: &str) -> Value {
    Value::text(v)
}

fn texts(v: &[String]) -> Value {
    Value::List(v.iter().map(|x| text(x)).collect())
}

fn opt_text(v: &str) -> Value {
    if v.is_empty() { Value::Null } else { text(v) }
}

impl Ctx<'_> {
    fn name_of(&self, id: &str) -> String {
        self.builtin
            .iter()
            .find(|m| m.id == id)
            .map(|m| m.name.clone())
            .or_else(|| self.installed.get(id).map(|m| m.module.name.clone()))
            .or_else(|| self.offered.get(id).map(|l| l.name.clone()))
            .unwrap_or_else(|| id.to_string())
    }

    /// How one dependency stands today. The interface derives the chip from these facts.
    fn dependency(&self, id: &str, requirement: &str) -> Value {
        let req = Req::parse(requirement).unwrap_or_else(|_| Req::any());
        let builtin = self.builtin.iter().any(|m| m.id == id);
        let installed = if builtin {
            Some(self.core.clone())
        } else {
            self.installed.get(id).map(Manifest::semver)
        };
        let available = self
            .offered
            .get(id)
            .and_then(|l| Version::parse(&l.version).ok());
        Value::map([
            ("id", text(id)),
            ("name", text(&self.name_of(id))),
            ("requirement", text(requirement)),
            (
                "kind",
                text(if builtin {
                    "built-in"
                } else if installed.is_some() {
                    "package"
                } else {
                    "missing"
                }),
            ),
            (
                "installed_version",
                installed
                    .as_ref()
                    .map_or(Value::Null, |v| text(&v.to_string())),
            ),
            (
                "satisfied",
                Value::Bool(installed.as_ref().is_some_and(|v| req.matches(v))),
            ),
            (
                "available_version",
                available
                    .as_ref()
                    .map_or(Value::Null, |v| text(&v.to_string())),
            ),
            (
                "available_satisfies",
                Value::Bool(available.as_ref().is_some_and(|v| req.matches(v))),
            ),
            ("optional", Value::Bool(false)),
        ])
    }

    /// Ids of the installed packages that name `id` as a dependency.
    fn dependents(&self, id: &str) -> Vec<String> {
        self.installed
            .iter()
            .filter(|(pid, m)| pid.as_str() != id && m.dependency_ids().iter().any(|d| d == id))
            .map(|(pid, _)| pid.clone())
            .collect()
    }
}

fn license_kind(license: &str) -> &'static str {
    if license.is_empty() {
        return "none";
    }
    match spdx::check(license) {
        Ok(LicenseKind::Spdx) => "spdx",
        Ok(LicenseKind::Custom) => "custom",
        Ok(LicenseKind::Proprietary) => "proprietary",
        Err(_) => "custom",
    }
}

/// Insert the catalogue fields shared by installed modules and marketplace listings.
pub fn put_meta(
    m: &mut BTreeMap<String, Value>,
    meta: &ModuleMeta,
    id: &str,
    version: &str,
    ctx: &Ctx<'_>,
) {
    let mut put = |k: &str, v: Value| {
        m.insert(k.to_string(), v);
    };
    put("summary", text(&meta.summary));
    put("author", text(&meta.author));
    put("authors", texts(&meta.authors));
    put("license", text(&meta.license));
    put("license_kind", text(license_kind(&meta.license)));
    put(
        "license_url",
        spdx::url_for(&meta.license).map_or(Value::Null, |u| text(&u)),
    );
    put("homepage", opt_text(&meta.homepage));
    put("repository", opt_text(&meta.repository));
    put("documentation", opt_text(&meta.documentation));
    put("support", opt_text(&meta.support));
    put("keywords", texts(&meta.keywords));
    put("categories", texts(&meta.categories));
    put("icon", text(&meta.icon));
    put("maturity", text(&meta.maturity));
    let req = if meta.requires_core.is_empty() {
        "*"
    } else {
        &meta.requires_core
    };
    put("requires_core", text(req));
    put(
        "compatible",
        Value::Bool(Req::parse(req).is_ok_and(|r| r.matches(ctx.core))),
    );
    put(
        "dependencies",
        Value::List(
            meta.dependencies
                .iter()
                .map(|(d, r)| ctx.dependency(d, r))
                .collect(),
        ),
    );
    put("dependents", texts(&ctx.dependents(id)));
    let newer = ctx.offered.get(id).and_then(|l| {
        let offered = Version::parse(&l.version).ok()?;
        let have = Version::parse(version).ok()?;
        (offered > have).then(|| l.version.clone())
    });
    put("update_available", newer.map_or(Value::Null, |v| text(&v)));
}

/// What a module declares, for the Contents tab: models, field counts, actions, permission
/// groups, approval steps and data classes with a count of personal-data fields.
pub fn contents(module: &Module) -> Value {
    let mut classes: BTreeMap<String, i64> = BTreeMap::new();
    let mut personal = 0i64;
    let mut fields_total = 0i64;
    let mut approvals = Vec::new();
    let mut models = Vec::new();
    for m in &module.models {
        let mut pf = 0i64;
        for f in &m.fields {
            *classes
                .entry(f.class.map_or("unclassified", |c| c.as_str()).to_string())
                .or_default() += 1;
            if f.personal_data {
                pf += 1;
            }
        }
        personal += pf;
        fields_total += m.fields.len() as i64;
        let actions: Vec<_> = m
            .commands
            .iter()
            .filter(|c| !GENERATED_COMMANDS.contains(&c.name))
            .collect();
        for a in &actions {
            if a.needs_approval {
                approvals.push(Value::map([
                    ("model", text(m.name)),
                    ("action", text(a.name)),
                    ("group", text(a.permission_group)),
                ]));
            }
        }
        models.push(Value::map([
            ("name", text(m.name)),
            ("description", text(m.description)),
            ("fields", Value::Int(m.fields.len() as i64)),
            ("personal_fields", Value::Int(pf)),
            ("read_group", text(m.read_group)),
            ("write_group", text(m.write_group)),
            (
                "actions",
                Value::List(
                    actions
                        .iter()
                        .map(|a| {
                            Value::map([
                                ("name", text(a.name)),
                                ("description", text(a.description)),
                                ("approval", Value::Bool(a.needs_approval)),
                            ])
                        })
                        .collect(),
                ),
            ),
        ]));
    }
    Value::map([
        ("models", Value::List(models)),
        (
            "groups",
            Value::List(module.permission_groups().into_iter().map(text).collect()),
        ),
        ("approval_steps", Value::List(approvals)),
        (
            "data_classes",
            Value::Map(
                classes
                    .into_iter()
                    .map(|(k, v)| (k, Value::Int(v)))
                    .collect(),
            ),
        ),
        ("fields", Value::Int(fields_total)),
        ("personal_data_fields", Value::Int(personal)),
    ])
}

pub fn blockers_value(b: &[Blocker]) -> Value {
    Value::List(
        b.iter()
            .map(|x| {
                Value::map([
                    ("module", text(&x.module)),
                    ("kind", text(x.kind)),
                    ("message", text(&x.message)),
                ])
            })
            .collect(),
    )
}

/// The result of `module_plan.preview`.
pub fn plan_value(plan: &Plan, root: &str, ctx: &Ctx<'_>, manifest_warnings: &[String]) -> Value {
    let steps = plan
        .steps
        .iter()
        .map(|s| {
            let x = &s.manifest.module;
            Value::map([
                ("id", text(&s.id)),
                ("name", text(&s.name)),
                ("version", text(&s.version)),
                ("action", text(s.action.as_str())),
                (
                    "from_version",
                    s.from_version.as_deref().map_or(Value::Null, text),
                ),
                ("license", text(&s.license)),
                ("license_kind", text(license_kind(&s.license))),
                ("reason", text(&s.reason)),
                ("requested", Value::Bool(s.requested)),
                ("source", text(s.source)),
                (
                    "requires_core",
                    text(if x.requires_core.is_empty() {
                        "*"
                    } else {
                        &x.requires_core
                    }),
                ),
                ("author", text(&x.author)),
                ("summary", text(&x.summary)),
                (
                    "dependencies",
                    Value::List(
                        s.manifest
                            .requirements()
                            .iter()
                            .map(|r| ctx.dependency(&r.id, r.req.as_str()))
                            .collect(),
                    ),
                ),
            ])
        })
        .collect();
    let mut warnings = plan.warnings.clone();
    for w in manifest_warnings {
        if !warnings.contains(w) {
            warnings.push(w.clone());
        }
    }
    Value::map([
        ("ok", Value::Bool(plan.ok())),
        ("root", text(root)),
        ("core_version", text(&ctx.core.to_string())),
        ("steps", Value::List(steps)),
        ("blockers", blockers_value(&plan.blockers)),
        ("warnings", texts(&warnings)),
    ])
}
