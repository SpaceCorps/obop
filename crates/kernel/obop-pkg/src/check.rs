//! `obop modules check FILE...`: everything an install checks that does not need a database.
//!
//! The same parser and the same D41 gate as an install, then the rules the installer applies
//! against what is registered: the id is not a built-in module, every dependency is a built-in
//! module or a bundled package, and every `ref` to a model outside the package points at a model
//! the package's dependencies (transitively) declare.

use crate::manifest::Manifest;
use crate::marketplace::{Bundled, Marketplace};
use crate::resolve::{self, World};
use crate::semver::Version;
use obop_registry::Module;
use std::collections::{BTreeMap, BTreeSet};

/// What `obop modules check` found in one package.
#[derive(Debug, Default, Clone)]
pub struct Report {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// The problems with the package text, empty if it would pass an install on a fresh instance
/// that has its dependencies. `builtin` are the linked first-party modules.
pub fn check_manifest(text: &str, builtin: &[Module]) -> Vec<String> {
    check_package(text, builtin, false).errors
}

/// Like `check_manifest`, with warnings. With `catalog` the package must also meet the rules
/// for the bundled catalogue and the marketplace: a licence, a core requirement that the running
/// core meets, and dependency ranges the bundled packages can satisfy (D55).
pub fn check_package(text: &str, builtin: &[Module], catalog: bool) -> Report {
    let manifest = match Manifest::parse(text) {
        Ok(m) => m,
        Err(e) => {
            return Report {
                errors: vec![e],
                warnings: vec![],
            };
        }
    };
    let mut errors = Vec::new();
    let mut warnings = manifest.warnings();
    if catalog {
        errors.extend(manifest.catalog_problems());
        // The catalogue says it: those two are required there, so they are not also warnings.
        warnings.retain(|w| {
            !w.starts_with("requires_core is not declared") && w != "no license is declared"
        });
    }
    if let Err(e) = manifest.to_module() {
        errors.push(e);
    }
    let id = &manifest.module.id;
    if builtin.iter().any(|b| &b.id == id) {
        errors.push(format!("{id} is a built-in module"));
    }
    let bundled: Vec<Manifest> = Bundled
        .list()
        .unwrap_or_default()
        .iter()
        .filter_map(|l| Bundled.fetch(&l.id).ok())
        .filter_map(|t| Manifest::parse(&t).ok())
        .collect();

    // Models available through the dependencies, transitively for bundled packages.
    let mut available: BTreeSet<String> = BTreeSet::new();
    let mut todo: Vec<String> = manifest.dependency_ids();
    let mut done = BTreeSet::new();
    while let Some(dep) = todo.pop() {
        if !done.insert(dep.clone()) {
            continue;
        }
        if &dep == id {
            errors.push(format!("{id} depends on itself"));
        } else if let Some(b) = builtin.iter().find(|b| b.id == dep) {
            available.extend(b.models.iter().map(|m| m.name.to_string()));
        } else if let Some(p) = bundled.iter().find(|p| p.module.id == dep) {
            available.extend(p.model.iter().map(|m| m.name.clone()));
            todo.extend(p.dependency_ids());
        } else {
            errors.push(format!(
                "{id} needs module {dep}, which is neither a built-in module nor a bundled package"
            ));
        }
    }
    for (at, to) in manifest.external_refs() {
        if available.contains(&to) {
            continue;
        }
        let owner = builtin
            .iter()
            .find(|b| b.models.iter().any(|m| m.name == to))
            .map(|b| b.id.clone())
            .or_else(|| {
                bundled
                    .iter()
                    .find(|p| p.model.iter().any(|m| m.name == to))
                    .map(|p| p.module.id.clone())
            });
        errors.push(match owner {
            Some(o) => format!(
                "{at} refers to model {to}, which module {o} declares; add {o} to depends"
            ),
            None => format!(
                "{at} refers to model {to}, which no built-in module, bundled package or dependency declares"
            ),
        });
    }
    // Versions: the core requirement, every range against what exists (built-ins version with
    // the core), cycles. Unknown ids and refs were reported above.
    let core = Version::parse(obop_types::CORE_VERSION).unwrap_or_else(|_| Version::new(0, 0, 0));
    let catalogue: BTreeMap<String, Manifest> = bundled
        .iter()
        .map(|p| (p.module.id.clone(), p.clone()))
        .collect();
    let market = |dep: &str| Ok(catalogue.get(dep).cloned());
    let world = World {
        core: core.clone(),
        builtin: builtin
            .iter()
            .map(|b| {
                (
                    b.id.clone(),
                    b.models.iter().map(|m| m.name.to_string()).collect(),
                )
            })
            .collect(),
        installed: BTreeMap::new(),
        market: &market,
        registered: &|_| true,
    };
    let own = !builtin.iter().any(|b| &b.id == id);
    if own {
        let plan = resolve::plan_install(&world, &manifest, "marketplace");
        for b in plan.blockers {
            if matches!(b.kind, "missing" | "refs" | "installed" | "builtin") {
                continue;
            }
            // A core mismatch of the package itself is only an error where the catalogue needs
            // it to run: an author may be preparing for a later core.
            if b.kind == "core" && b.module == *id && b.message.contains(" needs core ") && !catalog
            {
                warnings.push(b.message);
            } else {
                errors.push(b.message);
            }
        }
    }
    let mut seen = BTreeSet::new();
    errors.retain(|e| seen.insert(e.clone()));
    Report { errors, warnings }
}

#[cfg(test)]
mod tests {
    use super::*;
    use obop_registry::ModelDef;

    fn base() -> Module {
        Module {
            id: "base".into(),
            name: "Contacts".into(),
            models: vec![ModelDef {
                name: "party",
                description: "p",
                read_group: "base.user",
                write_group: "base.user",
                fields: vec![],
                commands: vec![],
            }],
            network_destinations: vec![],
            description: String::new(),
            translations: Default::default(),
            meta: Default::default(),
        }
    }

    const PKG: &str = r#"
[module]
id = "notes"
version = "0.1.0"
name = "Notes"
description = "Notes about contacts"
depends = ["base"]

[[model]]
name = "notes_note"
description = "A note"
read_group = "notes.user"
write_group = "notes.user"

  [[model.field]]
  name = "party_id"
  description = "Who the note is about"
  class = "none"
  kind = "ref"
  to = "party"
"#;

    #[test]
    fn a_good_package_has_no_errors() {
        assert!(check_manifest(PKG, &[base()]).is_empty());
    }

    #[test]
    fn parse_and_gate_errors_are_reported_as_an_install_would() {
        let e = check_manifest(&PKG.replace("0.1.0", "1"), &[base()]);
        assert_eq!(e, ["version must look like 1.2.3"]);
        let e = check_manifest(
            &PKG.replace("class = \"none\"", "class = \"bogus\""),
            &[base()],
        );
        assert!(e[0].contains("unknown data class"), "{e:?}");
    }

    #[test]
    fn dependencies_and_refs_are_checked_without_a_database() {
        let e = check_manifest(&PKG.replace("[\"base\"]", "[\"ghost\"]"), &[base()]);
        assert!(e.iter().any(|x| x.contains("needs module ghost")), "{e:?}");
        assert!(
            e.iter()
                .any(|x| x.contains("refers to model party") && x.contains("module base")),
            "{e:?}"
        );
        let e = check_manifest(
            &PKG.replace("to = \"party\"", "to = \"ghost_model\""),
            &[base()],
        );
        assert!(
            e[0].contains("no built-in module, bundled package or dependency"),
            "{e:?}"
        );
        let e = check_manifest(
            &PKG.replace("\"notes\"", "\"base\"")
                .replace("depends = [\"base\"]", "depends = []")
                .replace("notes_note", "base_note"),
            &[base()],
        );
        assert!(e.iter().any(|x| x.contains("built-in module")), "{e:?}");
    }

    #[test]
    fn refs_through_a_bundled_dependency_resolve() {
        let pkg = PKG
            .replace("depends = [\"base\"]", "depends = [\"accounting\"]")
            .replace("to = \"party\"", "to = \"accounting_move\"");
        assert!(check_manifest(&pkg, &[base()]).is_empty());
    }
}
