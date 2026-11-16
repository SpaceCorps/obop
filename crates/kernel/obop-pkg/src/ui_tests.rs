//! The D95 keys in packages: every key is read into the registry, a key that cannot work is
//! refused with the field or model it is on, the `set` effect, `plural` in the label tables
//! (English included), and every bundled package still passing the gate.

use crate::manifest::Manifest;
use crate::{Bundled, Marketplace};
use obop_registry::ui::{DefaultLit, DefaultSpec, FilterValue};
use obop_registry::{EffectKind, Target, check_module};

const HEAD: &str = r#"[module]
id = "ui"
version = "0.1.0"
name = "UI"
description = "d"
license = "MIT"
requires_core = "^0.2.2"
home = "ui_task"
launcher = false
"#;

const PROJECT: &str = r#"
[[model]]
name = "ui_project"
description = "d"
read_group = "ui.user"
write_group = "ui.user"

[[model.field]]
name = "name"
description = "d"
class = "none"

[[model.field]]
name = "active"
description = "d"
class = "none"
kind = "bool"

[[model.field]]
name = "rate"
description = "d"
class = "none"
kind = "money"
"#;

const TASK: &str = r#"
[[model]]
name = "ui_task"
description = "d"
read_group = "ui.user"
write_group = "ui.user"

[model.ui]
title = "{title} | Task {project_id.name}"
subtitle = "{project_id.name}"
list = ["title", "project_id", "due", "rate", "state"]
card = ["title", "rate"]
card_sum = "rate"
quick_create = ["title", "project_id"]
order = "due desc, title asc"
default_view = "kanban"
menu = "main"
menu_section = "Work"
menu_order = 2
calendar = { start = "due", color = "project_id" }
report = { rows = "project_id", cols = "state", measure = "rate" }
public_url = "/tasks/{title}"

[[model.ui.section]]
label = "Money"
fields = ["rate"]
collapsed = true
show_in = ["done"]

[[model.ui.filter]]
label = "Due soon"
where = "due <= today+7 && state != done"

[[model.child]]
model = "ui_task_note"
via = "task_id"
shape = "list"

[[model.field]]
name = "title"
description = "d"
class = "none"
required = true

[[model.field]]
name = "kind"
description = "d"
class = "none"
default = "chore"

[[model.field]]
name = "due"
description = "d"
class = "none"
kind = "date"
default = "today+7"
[model.field.ui]
warn_before_days = 3

[[model.field]]
name = "owner"
description = "d"
class = "none"
default = "$user"
readonly = true

[[model.field]]
name = "project_id"
description = "d"
class = "none"
kind = "ref"
to = "ui_project"
ref_filter = "active = true"

[[model.field]]
name = "rate"
description = "d"
class = "none"
kind = "money"
fill_from = "project_id.rate"
[model.field.ui]
aggregate = "none"
currency_from = "project_id.name"

[[model.field]]
name = "reason"
description = "d"
class = "none"
kind = "longtext"
[model.field.ui]
display = "markdown"

[[model.field]]
name = "done_on"
description = "d"
class = "none"
kind = "date"

[[model.field]]
name = "state"
description = "d"
class = "none"
kind = "select"
options = ["new", "done", "dropped"]
[model.field.ui]
tones = { dropped = "neutral" }

[[model.action]]
name = "finish"
description = "Finish"
field = "state"
from = "new"
to = "done"
[model.action.ui]
primary = true
done = "Finished"
not_by = "owner"
[[model.action.effect]]
kind = "set"
set = { done_on = "$today" }
when = "!is_set(done_on)"

[[model.action]]
name = "drop"
description = "Drop"
field = "state"
from = "new"
to = "dropped"
[model.action.ui]
danger = true
confirm = "Drop this task?"
group = "close"
needs = ["reason"]

[[model]]
name = "ui_task_note"
description = "d"
read_group = "ui.user"
write_group = "ui.user"

[[model.field]]
name = "task_id"
description = "d"
class = "none"
kind = "ref"
to = "ui_task"
"#;

const LABELS: &str = r#"
[labels.en.model.ui_task]
name = "Task"
plural = "Tasks"

[labels.de.model.ui_task]
name = "Aufgabe"
plural = "Aufgaben"
"#;

fn package() -> String {
    format!("{HEAD}{PROJECT}{TASK}{LABELS}")
}

/// The error of parsing, building and gating a package, or `None` when it is fine.
fn problem(text: &str) -> Option<String> {
    let m = match Manifest::parse(text) {
        Ok(m) => m,
        Err(e) => return Some(e),
    };
    match m.to_module() {
        Ok(module) => {
            let v = check_module(&module);
            (!v.is_empty()).then(|| format!("{v:?}"))
        }
        Err(e) => Some(e),
    }
}

#[test]
fn every_key_is_read_into_the_registry() {
    let text = package();
    assert_eq!(problem(&text), None);
    let module = Manifest::parse(&text).unwrap().to_module().unwrap();
    assert_eq!(module.meta.ui.home.as_deref(), Some("ui_task"));
    assert_eq!(module.meta.ui.launcher, Some(false));
    let task = module.models.iter().find(|m| m.name == "ui_task").unwrap();
    let ui = task.ui().unwrap();
    assert_eq!(
        ui.title.as_deref(),
        Some("{title} | Task {project_id.name}")
    );
    assert_eq!(ui.list.len(), 5);
    assert_eq!(ui.card_sum.as_deref(), Some("rate"));
    assert_eq!(ui.default_view.as_deref(), Some("kanban"));
    assert_eq!(ui.menu_order, Some(2));
    assert_eq!(ui.calendar.as_ref().unwrap().start, "due");
    assert_eq!(ui.report.as_ref().unwrap().measure.as_deref(), Some("rate"));
    assert_eq!(ui.sections[0].show_in, vec!["done".to_string()]);
    assert!(ui.sections[0].collapsed);
    assert_eq!(ui.filters[0].filter.clauses.len(), 2);
    assert_eq!(ui.filters[0].filter.clauses[0].value, FilterValue::Today(7));
    assert_eq!(ui.children[0].model, "ui_task_note");
    assert_eq!(ui.public_url.as_deref(), Some("/tasks/{title}"));
    let field = |n: &str| task.fields.iter().find(|f| f.name == n).unwrap();
    let h = |n: &str| field(n).hints.clone().unwrap();
    assert_eq!(
        h("kind").default,
        Some(DefaultSpec::Literal(DefaultLit::Text("chore".into())))
    );
    assert_eq!(h("due").default, Some(DefaultSpec::Today(7)));
    assert_eq!(h("due").ui.warn_before_days, Some(3));
    assert_eq!(h("owner").default, Some(DefaultSpec::User));
    assert!(h("owner").readonly);
    assert_eq!(
        h("rate").fill_from.as_ref().unwrap().written(),
        "project_id.rate"
    );
    assert_eq!(h("rate").ui.aggregate.as_deref(), Some("none"));
    assert_eq!(
        h("project_id").ref_filter.as_ref().unwrap().source,
        "active = true"
    );
    assert_eq!(h("reason").ui.display.as_deref(), Some("markdown"));
    assert_eq!(h("state").ui.tones["dropped"], "neutral");
    let action = |n: &str| task.commands.iter().find(|c| c.name == n).unwrap();
    let finish = action("finish");
    let fu = finish.ui.as_ref().unwrap();
    assert!(fu.primary && fu.not_by.as_deref() == Some("owner"));
    assert!(matches!(
        &finish.effects[0].kind,
        EffectKind::Update {
            target: Target::This,
            model: "ui_task",
            ..
        }
    ));
    assert_eq!(finish.effects[0].kind_name(), "set");
    let du = action("drop").ui.clone().unwrap();
    assert_eq!(du.needs, vec!["reason".to_string()]);
    assert_eq!(du.confirm.as_deref(), Some("Drop this task?"));
    let tr = &module.translations.models["ui_task"];
    assert_eq!(tr.plurals["en"], "Tasks");
    assert_eq!(tr.plurals["de"], "Aufgaben");
}

#[test]
fn a_key_that_cannot_work_is_refused_where_it_is() {
    let text = package();
    let cases: &[(&str, &str, &str)] = &[
        (
            "home = \"ui_task\"",
            "home = \"ui_nothing\"",
            "home must be",
        ),
        (
            "default_view = \"kanban\"",
            "default_view = \"board\"",
            "default_view must be one of",
        ),
        ("menu = \"main\"", "menu = \"side\"", "menu must be one of"),
        (
            "card_sum = \"rate\"",
            "card_sum = \"title\"",
            "card_sum must be",
        ),
        (
            "order = \"due desc, title asc\"",
            "order = \"due sideways\"",
            "order:",
        ),
        (
            "list = [\"title\", ",
            "list = [\"nope\", ",
            "list: ui_task has no field nope",
        ),
        (
            "show_in = [\"done\"]",
            "show_in = [\"gone\"]",
            "is not a state",
        ),
        (
            "\"due <= today+7 && state != done\"",
            "\"due <= today+7 && state != gone\"",
            "is not an option",
        ),
        (
            "title = \"{title} | Task {project_id.name}\"",
            "title = \"{nope}\"",
            "title: {nope}",
        ),
        (
            "default = \"today+7\"",
            "default = \"yesterday-ish\"",
            "does not fit",
        ),
        (
            "default = \"$user\"",
            "default = \"$boss\"",
            "is not a default",
        ),
        (
            "fill_from = \"project_id.rate\"",
            "fill_from = \"project_id.nope\"",
            "fill_from",
        ),
        (
            "ref_filter = \"active = true\"",
            "ref_filter = \"active = maybe\"",
            "true/false",
        ),
        (
            "warn_before_days = 3",
            "warn_before_days = 0",
            "warn_before_days must be",
        ),
        (
            "display = \"markdown\"",
            "display = \"fancy\"",
            "display must be one of",
        ),
        (
            "tones = { dropped = \"neutral\" }",
            "tones = { dropped = \"grey\" }",
            "tones:",
        ),
        ("needs = [\"reason\"]", "needs = [\"state\"]", "needs:"),
        ("not_by = \"owner\"", "not_by = \"due\"", "not_by must be"),
        (
            "via = \"task_id\"",
            "via = \"title\"",
            "is not a ref to ui_task",
        ),
        (
            "shape = \"list\"",
            "shape = \"grid\"",
            "shape must be one of",
        ),
        ("plural = \"Tasks\"", "plural = \"\"", "plural"),
        (
            "set = { done_on = \"$today\" }",
            "set = { state = \"done\" }",
            "governed",
        ),
        (
            "[model.action.ui]\nprimary = true",
            "[model.action.ui]\nprimary = true\ncolour = \"red\"",
            "unknown field",
        ),
    ];
    for (from, to, want) in cases {
        assert!(text.contains(from), "the fixture has no {from:?}");
        let bad = text.replacen(from, to, 1);
        let p = problem(&bad).unwrap_or_else(|| panic!("{to:?} was accepted"));
        assert!(p.contains(want), "{to:?}: {p}");
    }
}

#[test]
fn every_bundled_package_still_passes_the_gate() {
    let list = Bundled.list().unwrap();
    assert!(list.len() >= 24, "{}", list.len());
    for l in list {
        let text = Bundled.fetch(&l.id).unwrap();
        let module = Manifest::parse(&text)
            .and_then(|m| m.to_module())
            .unwrap_or_else(|e| panic!("{}: {e}", l.id));
        let v = check_module(&module);
        assert!(v.is_empty(), "{}: {v:?}", l.id);
    }
}

/// `obop modules check FILE` passes the package: a `set` effect names no model and a declared
/// child of the package is no dependency.
#[test]
fn modules_check_passes_a_package_with_every_key() {
    let problems = crate::check_manifest(&package(), &[]);
    assert!(problems.is_empty(), "{problems:?}");
}
