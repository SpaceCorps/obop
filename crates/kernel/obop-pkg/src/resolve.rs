//! Dependency resolution (D55): what an install or upgrade would have to do, as a plan, without
//! doing it. Pure logic over manifests; the installer supplies the world (what is installed, what
//! the marketplace offers) and applies the steps.
//!
//! Rules:
//! - A dependency on a built-in module is checked against the CORE version (built-ins version
//!   with the core). A dependency on a package is checked against the installed version.
//! - A missing dependency that the marketplace offers is pulled in, transitively, in topological
//!   order (dependencies before dependents). Cycles are blockers that name the cycle.
//! - An installed dependency whose version does not satisfy the requirement is upgraded when the
//!   marketplace offers a version that satisfies it, and blocks the plan otherwise.
//! - An install or upgrade that would leave an installed dependent with a requirement it no
//!   longer meets (upgrading `projects` to 2.0 while `timesheets` needs `<2`) is a blocker.
//! - Downgrades are refused; so is installing the version that is already installed.

use crate::manifest::Manifest;
use crate::semver::{Req, Version};
use std::collections::{BTreeMap, BTreeSet};

/// Everything a resolution needs to know about the instance.
pub struct World<'a> {
    /// The running core version.
    pub core: Version,
    /// Built-in module id to the models it declares.
    pub builtin: BTreeMap<String, Vec<String>>,
    /// Installed packages by id.
    pub installed: BTreeMap<String, Manifest>,
    /// The marketplace: the manifest offered for an id, `Ok(None)` if it offers none.
    pub market: &'a dyn Fn(&str) -> Result<Option<Manifest>, String>,
    /// Whether a model is registered right now, whoever registered it.
    pub registered: &'a dyn Fn(&str) -> bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepAction {
    Install,
    Upgrade,
    /// Already installed at a version that satisfies everything: nothing to do.
    Skip,
}

impl StepAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Install => "install",
            Self::Upgrade => "upgrade",
            Self::Skip => "skip",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Step {
    pub id: String,
    pub name: String,
    pub version: String,
    pub action: StepAction,
    /// The installed version an upgrade replaces.
    pub from_version: Option<String>,
    pub license: String,
    /// Why the step is in the plan, in a sentence.
    pub reason: String,
    /// Whether this is the module the caller asked for.
    pub requested: bool,
    /// `upload` for the root of an upload, else `marketplace`.
    pub source: &'static str,
    /// The manifest to install (unused for skips).
    pub manifest: Manifest,
}

/// What stops a plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blocker {
    /// The module the problem is about.
    pub module: String,
    /// `core`, `missing`, `version`, `conflict`, `cycle`, `downgrade`, `installed`, `builtin`,
    /// `refs`, `unavailable`, `dependents` or `data`.
    pub kind: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct Plan {
    pub steps: Vec<Step>,
    pub blockers: Vec<Blocker>,
    pub warnings: Vec<String>,
    /// What each upgrade step does to the records already stored (D63), by module id. Filled in
    /// by the installer, which can count records; the resolver itself is pure.
    pub data: BTreeMap<String, crate::migrate::DataPlan>,
}

impl Plan {
    pub fn ok(&self) -> bool {
        self.blockers.is_empty()
    }

    /// The steps that change something.
    pub fn work(&self) -> impl Iterator<Item = &Step> {
        self.steps.iter().filter(|s| s.action != StepAction::Skip)
    }
}

struct Choice {
    version: Version,
    action: StepAction,
    manifest: Manifest,
    from: Option<Version>,
    reason: String,
    source: &'static str,
}

struct Run<'a, 'w> {
    w: &'a World<'w>,
    chosen: BTreeMap<String, Choice>,
    /// Requirements put on each module so far: (who needs it, what).
    needs: BTreeMap<String, Vec<(String, Req)>>,
    blockers: Vec<Blocker>,
    cache: BTreeMap<String, Option<Manifest>>,
    stack: Vec<String>,
}

impl Run<'_, '_> {
    fn block(&mut self, module: &str, kind: &'static str, message: String) {
        let b = Blocker {
            module: module.into(),
            kind,
            message,
        };
        if !self.blockers.contains(&b) {
            self.blockers.push(b);
        }
    }

    fn offered(&mut self, id: &str) -> Result<Option<Manifest>, String> {
        if let Some(c) = self.cache.get(id) {
            return Ok(c.clone());
        }
        let r = (self.w.market)(id)?;
        self.cache.insert(id.into(), r.clone());
        Ok(r)
    }

    /// Check the manifest against the core and look at its own declarations.
    fn core_ok(&mut self, m: &Manifest) -> bool {
        let req = m.core_req();
        if req.matches(&self.w.core) {
            return true;
        }
        self.block(
            &m.module.id,
            "core",
            format!(
                "{} {} needs core {}, you run {}",
                m.module.id, m.module.version, req, self.w.core
            ),
        );
        false
    }

    /// Put `m` into the plan as the choice for its id and resolve what it needs.
    fn adopt(&mut self, m: Manifest, reason: String, source: &'static str) {
        let id = m.module.id.clone();
        let from = self.w.installed.get(&id).map(Manifest::semver);
        let action = if from.is_some() {
            StepAction::Upgrade
        } else {
            StepAction::Install
        };
        self.chosen.insert(
            id.clone(),
            Choice {
                version: m.semver(),
                action,
                manifest: m.clone(),
                from,
                reason,
                source,
            },
        );
        if !self.core_ok(&m) {
            return;
        }
        self.stack.push(id.clone());
        for r in m.requirements() {
            self.need(&r.id, &r.req, &id);
        }
        self.stack.pop();
    }

    /// `by` needs module `id` at a version satisfying `req`.
    fn need(&mut self, id: &str, req: &Req, by: &str) {
        if let Some(pos) = self.stack.iter().position(|s| s == id) {
            let mut chain: Vec<&str> = self.stack[pos..].iter().map(String::as_str).collect();
            chain.push(id);
            self.block(
                id,
                "cycle",
                format!("dependency cycle: {}", chain.join(" -> ")),
            );
            return;
        }
        self.needs
            .entry(id.into())
            .or_default()
            .push((by.into(), req.clone()));
        if self.w.builtin.contains_key(id) {
            if !req.matches(&self.w.core) {
                self.block(
                    by,
                    "core",
                    format!(
                        "{by} needs {id} {req}, a built-in module that versions with the core; you run core {}",
                        self.w.core
                    ),
                );
            }
            return;
        }
        if let Some(c) = self.chosen.get(id) {
            if req.matches(&c.version) {
                return;
            }
            if c.action == StepAction::Skip {
                self.upgrade_to_satisfy(id, by);
            } else {
                let v = c.version.clone();
                self.block(
                    id,
                    "conflict",
                    format!(
                        "{by} needs {id} {req}, but this plan puts {id} {v} in place ({})",
                        self.who_needs(id)
                    ),
                );
            }
            return;
        }
        if let Some(have) = self.w.installed.get(id) {
            let v = have.semver();
            if req.matches(&v) {
                self.chosen.insert(
                    id.into(),
                    Choice {
                        version: v.clone(),
                        action: StepAction::Skip,
                        manifest: have.clone(),
                        from: Some(v.clone()),
                        reason: format!("{id} {v} is installed and satisfies {req}"),
                        source: "marketplace",
                    },
                );
            } else {
                self.chosen.insert(
                    id.into(),
                    Choice {
                        version: v.clone(),
                        action: StepAction::Skip,
                        manifest: have.clone(),
                        from: Some(v),
                        reason: String::new(),
                        source: "marketplace",
                    },
                );
                self.upgrade_to_satisfy(id, by);
            }
            return;
        }
        match self.offered(id) {
            Err(e) => self.block(
                id,
                "unavailable",
                format!("{by} needs {id} {req}, and the marketplace could not be read: {e}"),
            ),
            Ok(None) => self.block(
                id,
                "missing",
                format!(
                    "{by} needs {id} {req}, which is not installed and not offered by the marketplace"
                ),
            ),
            Ok(Some(m)) => {
                if !req.matches(&m.semver()) {
                    self.block(
                        id,
                        "version",
                        format!(
                            "{by} needs {id} {req}, but the marketplace offers {id} {}",
                            m.module.version
                        ),
                    );
                    return;
                }
                let reason = format!("needed by {by} ({id} {req})");
                self.adopt(m, reason, "marketplace");
            }
        }
    }

    fn who_needs(&self, id: &str) -> String {
        self.needs
            .get(id)
            .map(|v| {
                v.iter()
                    .map(|(by, r)| format!("{by} needs {r}"))
                    .collect::<Vec<_>>()
                    .join("; ")
            })
            .unwrap_or_default()
    }

    /// The installed `id` is too old (or too new) for what `by` asked: take the marketplace's
    /// version if it satisfies everything asked of `id` so far.
    fn upgrade_to_satisfy(&mut self, id: &str, by: &str) {
        let have = self
            .chosen
            .get(id)
            .map(|c| c.version.clone())
            .unwrap_or_else(|| Version::new(0, 0, 0));
        let asked = self.who_needs(id);
        let reqs: Vec<Req> = self
            .needs
            .get(id)
            .map(|v| v.iter().map(|(_, r)| r.clone()).collect())
            .unwrap_or_default();
        let cand = match self.offered(id) {
            Ok(Some(m)) => m,
            Ok(None) => {
                self.block(
                    id,
                    "version",
                    format!(
                        "{by} needs a different {id} than the installed {have} ({asked}), and the marketplace does not offer {id}"
                    ),
                );
                return;
            }
            Err(e) => {
                self.block(
                    id,
                    "unavailable",
                    format!("{id} {have} is installed but {asked}; the marketplace could not be read: {e}"),
                );
                return;
            }
        };
        let v = cand.semver();
        if v <= have {
            self.block(
                id,
                "version",
                format!(
                    "{by} needs {id} that satisfies {asked}, but {id} {have} is installed and the marketplace offers {}",
                    cand.module.version
                ),
            );
            return;
        }
        if !reqs.iter().all(|r| r.matches(&v)) {
            self.block(
                id,
                "version",
                format!(
                    "{id} {have} is installed; the marketplace offers {v}, which does not satisfy: {asked}"
                ),
            );
            return;
        }
        let reason =
            format!("{id} {have} is installed but {by} needs a different version ({asked})");
        self.adopt(cand, reason, "marketplace");
    }
}

/// Plan the installation (or upgrade) of `root`, an uploaded manifest or one the marketplace
/// offers. `root_source` is `upload` or `marketplace`.
pub fn plan_install(world: &World<'_>, root: &Manifest, root_source: &'static str) -> Plan {
    let mut run = Run {
        w: world,
        chosen: BTreeMap::new(),
        needs: BTreeMap::new(),
        blockers: Vec::new(),
        cache: BTreeMap::new(),
        stack: Vec::new(),
    };
    let id = root.module.id.clone();
    let rv = root.semver();
    if world.builtin.contains_key(&id) {
        run.block(
            &id,
            "builtin",
            format!("{id} is a built-in module and is part of the core"),
        );
        return Plan {
            blockers: run.blockers,
            ..Plan::default()
        };
    }
    if let Some(old) = world.installed.get(&id) {
        let ov = old.semver();
        if rv == ov {
            run.block(&id, "installed", format!("{id} {ov} is already installed"));
        } else if rv < ov {
            run.block(
                &id,
                "downgrade",
                format!(
                    "{id} {ov} is installed; installing {rv} would be a downgrade, which is refused"
                ),
            );
        }
    }
    let mut warnings: Vec<String> = root
        .warnings()
        .into_iter()
        .map(|w| format!("{id}: {w}"))
        .collect();
    run.adopt(root.clone(), "requested".into(), root_source);

    // Nothing may be left behind that needs a version this plan takes away.
    let changing: Vec<String> = run
        .chosen
        .iter()
        .filter(|(_, c)| c.action != StepAction::Skip)
        .map(|(id, _)| id.clone())
        .collect();
    for cid in &changing {
        let v = run.chosen[cid].version.clone();
        for (other, m) in &world.installed {
            if run
                .chosen
                .get(other)
                .is_some_and(|c| c.action != StepAction::Skip)
            {
                continue; // replaced in this plan: its new manifest was resolved above
            }
            for r in m.requirements() {
                if &r.id == cid && !r.req.matches(&v) {
                    let kind = if run.chosen[cid].action == StepAction::Upgrade {
                        "upgrading"
                    } else {
                        "installing"
                    };
                    run.block(
                        cid,
                        "conflict",
                        format!(
                            "{kind} {cid} to {v} would break {other}, which needs {cid} {}",
                            r.req
                        ),
                    );
                }
            }
        }
    }

    let order = topological(&run.chosen);
    // References to models must be satisfied by what is there or comes earlier in the plan.
    let mut provided: BTreeSet<String> = world.builtin.values().flatten().cloned().collect();
    for m in world.installed.values() {
        provided.extend(m.model.iter().map(|x| x.name.clone()));
    }
    let mut steps = Vec::new();
    for sid in order {
        let c = &run.chosen[&sid];
        if c.action != StepAction::Skip {
            for (at, to) in c.manifest.external_refs() {
                if !provided.contains(&to) && !(world.registered)(&to) {
                    run.blockers.push(Blocker {
                        module: sid.clone(),
                        kind: "refs",
                        message: format!(
                            "{at} refers to model {to}, which no dependency of {sid} provides; add the module that declares it to the dependencies"
                        ),
                    });
                }
            }
            provided.extend(c.manifest.model.iter().map(|x| x.name.clone()));
            if sid != id {
                warnings.extend(
                    c.manifest
                        .warnings()
                        .into_iter()
                        .map(|w| format!("{sid}: {w}")),
                );
            }
        }
        let c = &run.chosen[&sid];
        steps.push(Step {
            id: sid.clone(),
            name: c.manifest.module.name.clone(),
            version: c.version.to_string(),
            action: c.action,
            from_version: c.from.as_ref().map(Version::to_string),
            license: c.manifest.module.license.clone(),
            reason: c.reason.clone(),
            requested: sid == id,
            source: if sid == id { root_source } else { c.source },
            manifest: c.manifest.clone(),
        });
    }
    Plan {
        steps,
        blockers: run.blockers,
        warnings,
        data: BTreeMap::new(),
    }
}

/// Dependencies before dependents; ties in id order. Edges only between plan members that change
/// something (an installed skip has nothing to wait for).
fn topological(chosen: &BTreeMap<String, Choice>) -> Vec<String> {
    let mut out = Vec::new();
    let mut done = BTreeSet::new();
    fn visit(
        id: &str,
        chosen: &BTreeMap<String, Choice>,
        done: &mut BTreeSet<String>,
        out: &mut Vec<String>,
    ) {
        if !done.insert(id.to_string()) {
            return;
        }
        if let Some(c) = chosen.get(id)
            && c.action != StepAction::Skip
        {
            for d in c.manifest.dependency_ids() {
                if chosen.contains_key(&d) {
                    visit(&d, chosen, done, out);
                }
            }
        }
        out.push(id.to_string());
    }
    for id in chosen.keys() {
        visit(id, chosen, &mut done, &mut out);
    }
    out
}

/// An installed package that needs another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependent {
    pub id: String,
    /// Whether it names the module itself (else it needs it through another dependent).
    pub direct: bool,
    /// The requirement it states, for a direct dependent.
    pub requirement: Option<String>,
    /// The chain from the dependent down to the module, `a -> b -> id`.
    pub via: Vec<String>,
}

/// The installed packages that need `id`, directly and through others, nearest first.
pub fn dependents(installed: &BTreeMap<String, Manifest>, id: &str) -> Vec<Dependent> {
    let mut out: Vec<Dependent> = Vec::new();
    let mut frontier: Vec<(String, Vec<String>)> = vec![(id.to_string(), vec![id.to_string()])];
    let mut seen: BTreeSet<String> = BTreeSet::from([id.to_string()]);
    while !frontier.is_empty() {
        let mut next = Vec::new();
        for (target, path) in &frontier {
            for (pid, m) in installed {
                if seen.contains(pid) {
                    continue;
                }
                if let Some(r) = m.requirements().into_iter().find(|r| &r.id == target) {
                    seen.insert(pid.clone());
                    let mut via = vec![pid.clone()];
                    via.extend(path.iter().cloned());
                    out.push(Dependent {
                        id: pid.clone(),
                        direct: target == id,
                        requirement: (target == id).then(|| r.req.to_string()),
                        via: via.clone(),
                    });
                    next.push((pid.clone(), via));
                }
            }
        }
        frontier = next;
    }
    out
}

/// The message that stops an uninstall, or `None` when nothing depends on the module.
pub fn uninstall_blocker(installed: &BTreeMap<String, Manifest>, id: &str) -> Option<Blocker> {
    let deps = dependents(installed, id);
    if deps.is_empty() {
        return None;
    }
    let direct: Vec<&str> = deps
        .iter()
        .filter(|d| d.direct)
        .map(|d| d.id.as_str())
        .collect();
    let indirect: Vec<&str> = deps
        .iter()
        .filter(|d| !d.direct)
        .map(|d| d.id.as_str())
        .collect();
    let mut message = format!("{id} is required by {}", direct.join(", "));
    if !indirect.is_empty() {
        message.push_str(&format!(" (and, through them, {})", indirect.join(", ")));
    }
    message.push_str("; uninstall those first");
    Some(Blocker {
        module: id.into(),
        kind: "dependents",
        message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(id: &str, version: &str, core: &str, deps: &[(&str, &str)]) -> Manifest {
        let mut t = format!(
            "[module]\nid = \"{id}\"\nversion = \"{version}\"\nname = \"{id}\"\ndescription = \"d\"\nlicense = \"MIT\"\nrequires_core = \"{core}\"\n"
        );
        if !deps.is_empty() {
            t.push_str("[module.dependencies]\n");
            for (d, r) in deps {
                t.push_str(&format!("{d} = \"{r}\"\n"));
            }
        }
        Manifest::parse(&t).unwrap()
    }

    struct Fixture {
        installed: BTreeMap<String, Manifest>,
        market: BTreeMap<String, Manifest>,
        core: &'static str,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                installed: BTreeMap::new(),
                market: BTreeMap::new(),
                core: "0.2.0",
            }
        }
        fn have(mut self, x: Manifest) -> Self {
            self.installed.insert(x.module.id.clone(), x);
            self
        }
        fn offer(mut self, x: Manifest) -> Self {
            self.market.insert(x.module.id.clone(), x);
            self
        }
        fn plan(&self, root: &Manifest) -> Plan {
            let market = |id: &str| Ok(self.market.get(id).cloned());
            let w = World {
                core: Version::parse(self.core).unwrap(),
                builtin: BTreeMap::from([
                    ("base".to_string(), vec!["party".to_string()]),
                    ("product".to_string(), vec!["product".to_string()]),
                ]),
                installed: self.installed.clone(),
                market: &market,
                registered: &|_| false,
            };
            plan_install(&w, root, "marketplace")
        }
    }

    fn ids(p: &Plan) -> Vec<String> {
        p.steps
            .iter()
            .map(|s| format!("{}:{}", s.id, s.action.as_str()))
            .collect()
    }

    fn msgs(p: &Plan) -> String {
        p.blockers
            .iter()
            .map(|b| format!("[{}] {}", b.kind, b.message))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn dependencies_come_first_and_transitively() {
        let f = Fixture::new()
            .offer(m("projects", "0.2.0", "^0.2", &[("base", "*")]))
            .offer(m("hr", "0.2.0", "^0.2", &[("base", "*")]))
            .offer(m(
                "timesheets",
                "0.2.0",
                "^0.2",
                &[("projects", ">=0.2, <1"), ("hr", "^0.2"), ("base", "^0.2")],
            ));
        let root = f.market["timesheets"].clone();
        let p = f.plan(&root);
        assert!(p.ok(), "{}", msgs(&p));
        assert_eq!(
            ids(&p),
            ["hr:install", "projects:install", "timesheets:install"]
        );
        assert!(p.steps[2].requested && !p.steps[0].requested);
        assert!(p.steps[0].reason.contains("needed by timesheets"));
    }

    #[test]
    fn what_is_installed_and_satisfying_is_skipped() {
        let f = Fixture::new()
            .have(m("projects", "0.2.5", "^0.2", &[]))
            .offer(m("timesheets", "0.2.0", "^0.2", &[("projects", "^0.2")]));
        let p = f.plan(&f.market["timesheets"].clone());
        assert!(p.ok(), "{}", msgs(&p));
        assert_eq!(ids(&p), ["projects:skip", "timesheets:install"]);
        assert_eq!(p.work().count(), 1);
    }

    #[test]
    fn diamond_dependencies_are_resolved_once() {
        let f = Fixture::new()
            .offer(m("d", "0.1.0", "*", &[]))
            .offer(m("b", "0.1.0", "*", &[("d", "^0.1")]))
            .offer(m("c", "0.1.0", "*", &[("d", ">=0.1, <0.3")]))
            .offer(m("a", "0.1.0", "*", &[("b", "*"), ("c", "*")]));
        let p = f.plan(&f.market["a"].clone());
        assert!(p.ok(), "{}", msgs(&p));
        assert_eq!(p.steps.len(), 4);
        let pos = |id: &str| p.steps.iter().position(|s| s.id == id).unwrap();
        assert!(pos("d") < pos("b") && pos("d") < pos("c") && pos("b") < pos("a"));
        assert_eq!(p.steps.iter().filter(|s| s.id == "d").count(), 1);
    }

    #[test]
    fn two_dependents_that_cannot_share_a_version_conflict() {
        let f = Fixture::new()
            .offer(m("d", "0.2.0", "*", &[]))
            .offer(m("b", "0.1.0", "*", &[("d", "^0.2")]))
            .offer(m("c", "0.1.0", "*", &[("d", "^0.1")]))
            .offer(m("a", "0.1.0", "*", &[("b", "*"), ("c", "*")]));
        let p = f.plan(&f.market["a"].clone());
        assert!(!p.ok());
        assert!(msgs(&p).contains("c needs d ^0.1"), "{}", msgs(&p));
    }

    #[test]
    fn cycles_are_blockers_that_name_the_cycle() {
        let f = Fixture::new()
            .offer(m("b", "0.1.0", "*", &[("c", "*")]))
            .offer(m("c", "0.1.0", "*", &[("b", "*")]));
        let root = m("a", "0.1.0", "*", &[("b", "*")]);
        let p = f.plan(&root);
        assert!(!p.ok());
        let text = msgs(&p);
        assert!(
            text.contains("[cycle]") && text.contains("b -> c -> b"),
            "{text}"
        );
        let p = f.plan(&m("b", "0.1.0", "*", &[("c", "*")]));
        assert!(msgs(&p).contains("b -> c -> b"), "{}", msgs(&p));
    }

    #[test]
    fn built_in_dependencies_are_checked_against_the_core_version() {
        let f = Fixture::new();
        let ok = f.plan(&m("x", "0.1.0", "*", &[("base", "^0.2")]));
        assert!(ok.ok(), "{}", msgs(&ok));
        let bad = f.plan(&m("x", "0.1.0", "*", &[("base", ">=0.3")]));
        assert!(!bad.ok());
        assert!(
            msgs(&bad).contains("built-in module that versions with the core")
                && msgs(&bad).contains("0.2.0"),
            "{}",
            msgs(&bad)
        );
        // Built-ins are never steps.
        assert_eq!(ids(&ok), ["x:install"]);
    }

    #[test]
    fn a_core_mismatch_blocks_with_required_and_running_versions() {
        let f = Fixture::new();
        let p = f.plan(&m("x", "0.1.0", ">=9", &[]));
        assert!(!p.ok());
        assert_eq!(
            p.blockers[0].message,
            "x 0.1.0 needs core >=9, you run 0.2.0"
        );
        assert_eq!(p.blockers[0].kind, "core");
        // The same for a dependency pulled from the marketplace.
        let f = Fixture::new().offer(m("y", "0.1.0", ">=9", &[]));
        let p = f.plan(&m("x", "0.1.0", "*", &[("y", "*")]));
        assert!(msgs(&p).contains("y 0.1.0 needs core >=9"), "{}", msgs(&p));
    }

    #[test]
    fn missing_and_out_of_range_dependencies_say_so() {
        let f = Fixture::new().offer(m("y", "0.1.0", "*", &[]));
        let p = f.plan(&m("x", "0.1.0", "*", &[("ghost", "*")]));
        assert!(
            msgs(&p).contains("[missing] x needs ghost *"),
            "{}",
            msgs(&p)
        );
        let p = f.plan(&m("x", "0.1.0", "*", &[("y", ">=1")]));
        assert!(
            msgs(&p).contains("[version] x needs y >=1, but the marketplace offers y 0.1.0"),
            "{}",
            msgs(&p)
        );
    }

    #[test]
    fn an_installed_dependency_that_is_too_old_is_upgraded_when_possible() {
        let f = Fixture::new()
            .have(m("p", "0.1.0", "*", &[]))
            .offer(m("p", "0.3.0", "*", &[]));
        let p = f.plan(&m("x", "0.1.0", "*", &[("p", ">=0.2")]));
        assert!(p.ok(), "{}", msgs(&p));
        assert_eq!(ids(&p), ["p:upgrade", "x:install"]);
        assert_eq!(p.steps[0].from_version.as_deref(), Some("0.1.0"));
        // ... and blocks when the marketplace has nothing better.
        let f = Fixture::new().have(m("p", "0.1.0", "*", &[]));
        let p = f.plan(&m("x", "0.1.0", "*", &[("p", ">=0.2")]));
        assert!(!p.ok(), "{}", msgs(&p));
    }

    #[test]
    fn upgrading_a_module_that_an_installed_dependent_caps_is_a_conflict() {
        let ts = m("timesheets", "0.2.0", "*", &[("projects", ">=0.2, <1")]);
        let f = Fixture::new()
            .have(m("projects", "0.2.0", "*", &[]))
            .have(ts);
        let p = f.plan(&m("projects", "2.0.0", "*", &[]));
        assert!(!p.ok());
        assert_eq!(
            p.blockers[0].message,
            "upgrading projects to 2.0.0 would break timesheets, which needs projects >=0.2, <1"
        );
        // An upgrade inside the range is fine.
        let p = f.plan(&m("projects", "0.9.0", "*", &[]));
        assert!(p.ok(), "{}", msgs(&p));
        assert_eq!(ids(&p), ["projects:upgrade"]);
    }

    #[test]
    fn downgrades_and_reinstalls_and_built_ins_are_refused() {
        let f = Fixture::new().have(m("projects", "0.2.0", "*", &[]));
        let p = f.plan(&m("projects", "0.1.0", "*", &[]));
        assert_eq!(p.blockers[0].kind, "downgrade");
        let p = f.plan(&m("projects", "0.2.0", "*", &[]));
        assert_eq!(p.blockers[0].kind, "installed");
        let p = f.plan(&m("base", "0.2.0", "*", &[]));
        assert_eq!(p.blockers[0].kind, "builtin");
    }

    #[test]
    fn prerelease_upgrades_order_correctly() {
        let f = Fixture::new().have(m("p", "0.2.0-beta.1", "*", &[]));
        let p = f.plan(&m("p", "0.2.0", "*", &[]));
        assert!(p.ok(), "{}", msgs(&p));
        let p = f.plan(&m("p", "0.2.0-alpha", "*", &[]));
        assert_eq!(p.blockers[0].kind, "downgrade");
    }

    #[test]
    fn refs_to_models_nobody_provides_block_the_plan() {
        let text = "[module]\nid = \"x\"\nversion = \"0.1.0\"\nname = \"x\"\ndescription = \"d\"\nrequires_core = \"^0.2\"\n[[model]]\nname = \"x_a\"\ndescription = \"d\"\nread_group = \"x.u\"\nwrite_group = \"x.u\"\n[[model.field]]\nname = \"p\"\ndescription = \"d\"\nclass = \"none\"\nkind = \"ref\"\nto = \"project\"\n";
        let x = Manifest::parse(text).unwrap();
        let f = Fixture::new();
        let p = f.plan(&x);
        assert!(msgs(&p).contains("[refs]"), "{}", msgs(&p));
    }

    #[test]
    fn dependents_are_found_directly_and_transitively() {
        let mut inst = BTreeMap::new();
        for x in [
            m("projects", "0.2.0", "*", &[]),
            m("timesheets", "0.2.0", "*", &[("projects", "^0.2")]),
            m("billing", "0.2.0", "*", &[("timesheets", "*")]),
            m("other", "0.2.0", "*", &[]),
        ] {
            inst.insert(x.module.id.clone(), x);
        }
        let d = dependents(&inst, "projects");
        assert_eq!(
            d.iter()
                .map(|x| (x.id.as_str(), x.direct))
                .collect::<Vec<_>>(),
            [("timesheets", true), ("billing", false)]
        );
        assert_eq!(d[0].requirement.as_deref(), Some("^0.2"));
        assert_eq!(d[1].via, ["billing", "timesheets", "projects"]);
        let b = uninstall_blocker(&inst, "projects").unwrap();
        assert!(
            b.message.contains("required by timesheets")
                && b.message.contains("through them, billing"),
            "{}",
            b.message
        );
        assert!(uninstall_blocker(&inst, "other").is_none());
        assert!(uninstall_blocker(&inst, "billing").is_none());
    }
}
