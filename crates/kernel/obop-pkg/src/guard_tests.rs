//! `[model.guard]` (D77) in packages: the long and the short forms are read into the registry,
//! the short forms are resolved against the model's actions, and a key that cannot work is
//! refused by the gate with the model and the key.

use crate::manifest::Manifest;
use obop_registry::check_module;

const HEAD: &str = r#"[module]
id = "gd"
version = "0.1.0"
name = "Guarded"
description = "d"
license = "MIT"
requires_core = "^0.2.2"
"#;

fn order(guard: &str) -> String {
    format!(
        r#"
[[model]]
name = "gd_period"
description = "d"
read_group = "gd.user"
write_group = "gd.user"

[[model.field]]
name = "start_on"
description = "d"
class = "none"
kind = "date"

[[model.field]]
name = "end_on"
description = "d"
class = "none"
kind = "date"

[[model.field]]
name = "state"
description = "d"
class = "none"
kind = "select"
options = ["open", "closed"]

[[model]]
name = "gd_order"
description = "d"
read_group = "gd.user"
write_group = "gd.user"

[[model.field]]
name = "note"
description = "d"
class = "none"

[[model.field]]
name = "reason"
description = "d"
class = "none"

[[model.field]]
name = "date"
description = "d"
class = "none"
kind = "date"

[[model.field]]
name = "state"
description = "d"
class = "none"
kind = "select"
options = ["draft", "posted", "cancelled"]

[[model.action]]
name = "post"
description = "d"
field = "state"
from = "draft"
to = "posted"

[[model.action]]
name = "cancel"
description = "d"
field = "state"
from = "posted"
to = "cancelled"

{guard}

[[model]]
name = "gd_order_line"
description = "d"
read_group = "gd.user"
write_group = "gd.user"

[[model.field]]
name = "order_id"
description = "d"
class = "none"
kind = "ref"
to = "gd_order"

[model.guard]
children_of = "order_id"
"#
    )
}

fn module(guard: &str) -> Result<obop_registry::Module, String> {
    Manifest::parse(&format!("{HEAD}{}", order(guard)))?.to_module()
}

#[test]
fn the_short_and_the_long_forms_read_the_same() {
    let short = module(
        r#"[model.guard]
immutable_when = ["posted", "cancelled"]
mutable_fields = ["note"]
exit_to = ["cancelled"]
no_delete_when = ["posted"]
lock_date = { field = "date", closed_by = "gd_period" }
message = "A posted order cannot be changed."
"#,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let long = module(
        r#"[model.guard]
immutable_when = { field = "state", values = ["posted", "cancelled"] }
mutable_fields = ["note"]
exit_to = [{ to = "cancelled", action = "cancel" }]
no_delete_when = { field = "state", values = ["posted"] }
lock_date = { field = "date", closed_by = "gd_period" }
message = "A posted order cannot be changed."
"#,
    )
    .unwrap();
    let g = |m: &obop_registry::Module, name: &str| {
        m.models
            .iter()
            .find(|x| x.name == name)
            .and_then(|x| x.guard().cloned())
    };
    assert_eq!(g(&short, "gd_order"), g(&long, "gd_order"));
    let guard = g(&short, "gd_order").unwrap();
    assert_eq!(guard.exit_to[0].action, "cancel");
    assert_eq!(guard.lock_date.unwrap().closed_by, Some("gd_period"));
    assert_eq!(guard.message, Some("A posted order cannot be changed."));
    assert_eq!(
        g(&short, "gd_order_line").unwrap().children_of,
        Some("order_id")
    );
    assert!(
        check_module(&short).is_empty(),
        "{:?}",
        check_module(&short)
    );
}

#[test]
fn a_guard_that_cannot_work_is_refused() {
    // Read errors: no such action, an unknown key.
    let e = module("[model.guard]\nimmutable_when = [\"posted\"]\nexit_to = [\"draft\"]\n")
        .unwrap_err();
    assert!(
        e.contains("gd_order.guard.exit_to") && e.contains("no action"),
        "{e}"
    );
    let e = Manifest::parse(&format!(
        "{HEAD}{}",
        order("[model.guard]\nfrozen = true\n")
    ))
    .err()
    .unwrap_or_default();
    assert!(e.contains("frozen"), "{e}");
    // Gate errors: the first option, a computed or missing field, both lock sources.
    for (guard, want) in [
        (
            "[model.guard]\nimmutable_when = [\"draft\"]\n",
            "born frozen",
        ),
        (
            "[model.guard]\nimmutable_when = [\"posted\"]\nexit_to = [\"cancelled\"]\nmutable_fields = [\"nope\"]\n",
            "has no field nope",
        ),
        (
            "[model.guard]\nlock_date = { field = \"date\", setting = \"a.b\", closed_by = \"gd_period\" }\n",
            "exactly one",
        ),
        (
            "[model.guard]\nimmutable_when = [\"posted\"]\n",
            "list it in exit_to",
        ),
        (
            "[model.guard]\nmessage = \"Only a message\"\n",
            "at least one rule",
        ),
    ] {
        let e = module(guard).unwrap_err();
        assert!(
            e.contains("gd.gd_order.guard") && e.contains(want),
            "{guard}: wanted {want:?}, got {e}"
        );
    }
}
