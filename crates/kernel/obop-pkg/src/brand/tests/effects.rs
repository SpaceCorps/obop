//! An action effect of a package cannot write the branding row (docs/spec/branding.md, sections 3
//! and 10, item 4: "only a person who holds platform.admin can write the record, through one
//! wrapper"). An effect writes a record the way the generated `create` and `update` do, for the
//! caller; the model has neither, so the guard of `configure` and `clear`, the character policy,
//! the lock and the audit event would all be skipped by an effect that wrote the row. The install
//! gate refuses such a package, and the engine refuses the same write when an effect runs (a
//! package stored before the gate, or a module registered around it).

use super::*;
use obop_registry::{Effect, FieldDef};

/// A package with one action whose single effect is `effect` (the lines of a
/// `[[model.action.effect]]` table).
fn probe(effect: &str) -> String {
    format!(
        r#"
[module]
id = "probe"
version = "0.1.0"
name = "Probe"
description = "A probe"
requires_core = "^0.2"

[[model]]
name = "probe_note"
description = "A note"
read_group = "probe.user"
write_group = "probe.user"

  [[model.field]]
  name = "title"
  description = "The title"
  class = "none"

  [[model.field]]
  name = "brand_id"
  description = "The branding record"
  class = "none"
  kind = "ref"
  to = "instance_brand"

  [[model.field]]
  name = "event_id"
  description = "A record of the log"
  class = "none"
  kind = "ref"
  to = "security_event"

  [[model.field]]
  name = "state"
  description = "new or done"
  class = "none"
  kind = "select"
  options = ["new", "done"]

  [[model.action]]
  name = "apply"
  description = "Apply the note"
  field = "state"
  from = "new"
  to = "done"

    [[model.action.effect]]
    {effect}
"#
    )
}

fn installer() -> (Arc<Dispatcher>, Arc<Installer>) {
    let d = Arc::new(Dispatcher::new());
    let inst = Installer::new(Store::new(), &d, vec![admin_module()], Arc::new(Bundled));
    register_admin(&d, &inst);
    (d, inst)
}

#[test]
fn a_package_whose_action_effect_writes_the_brand_row_is_refused_at_install() {
    for effect in [
        // Change the name: the probe of the review.
        r#"kind = "update"
    model = "instance_brand"
    ref = "brand_id"
    set = { name = "$self.title" }"#,
        // Make the row, when there is none.
        r#"kind = "create"
    model = "instance_brand"
    map = { name = "$self.title" }"#,
    ] {
        let (_d, inst) = installer();
        let e = inst
            .install(&probe(effect), "upload", "ada")
            .expect_err(effect);
        assert!(e.contains("writes instance_brand"), "{e}");
        assert!(e.contains("changed by its own commands only"), "{e}");
        assert!(
            inst.installed_manifests().is_empty(),
            "nothing was installed"
        );
    }
}

#[test]
fn the_other_models_with_hand_registered_commands_are_refused_the_same_way() {
    let (_d, inst) = installer();
    let e = inst
        .install(
            &probe(
                r#"kind = "update"
    model = "security_event"
    ref = "event_id"
    set = { detail = "$self.title" }"#,
            ),
            "upload",
            "ada",
        )
        .expect_err("an effect on the log");
    assert!(e.contains("writes security_event"), "{e}");
}

#[test]
fn an_effect_on_a_model_of_the_package_itself_is_not_in_the_way() {
    // The gate is about the model written: the same package with an effect on a model it declares
    // installs, with its refs to the branding row and to the log still in place.
    let (_d, inst) = installer();
    inst.install(
        &probe(
            r#"kind = "create"
    model = "probe_note"
    map = { title = "$self.title" }"#,
        ),
        "upload",
        "ada",
    )
    .unwrap();
}

const FORCED_GROUP: &str = "forced.user";

fn forced_field(name: &'static str) -> FieldDef {
    FieldDef::new(name, "d", DataClass::None, false, FORCED_GROUP)
}

fn forced_model(name: &'static str, fields: Vec<FieldDef>, commands: Vec<CommandDef>) -> ModelDef {
    ModelDef {
        name,
        description: "d",
        read_group: FORCED_GROUP,
        write_group: FORCED_GROUP,
        access: None,
        fields,
        commands,
        ext: Default::default(),
    }
}

fn forced_wrap(id: &str, models: Vec<ModelDef>) -> Module {
    Module {
        id: id.into(),
        name: id.into(),
        models,
        network_destinations: vec![],
        description: String::new(),
        translations: Default::default(),
        meta: Default::default(),
    }
}

/// A module registered around the install gate (as a package stored by an older version would be):
/// a note with a ref to the branding row and to an ordinary record, and two actions, one whose
/// effect writes the branding row and one whose effect writes the ordinary record.
fn forced_module() -> Module {
    const G: &str = FORCED_GROUP;
    let f = forced_field;
    forced_wrap(
        "forced",
        vec![
            forced_model(
                "forced_note",
                vec![
                    f("title"),
                    f("brand_id").reference(MODEL),
                    f("target_id").reference("forced_target"),
                    f("state").select(&["new", "done", "copied"]),
                ],
                vec![
                    CommandDef::new("apply", "Rename the brand", G)
                        .transition("state", Some("new"), "done")
                        .effect(Effect::update_ref(
                            MODEL,
                            "brand_id",
                            &[("name", "$self.title")],
                        )),
                    CommandDef::new("copy", "Copy the title", G)
                        .transition("state", Some("new"), "copied")
                        .effect(Effect::update_ref(
                            "forced_target",
                            "target_id",
                            &[("title", "$self.title")],
                        )),
                ],
            ),
            forced_model("forced_target", vec![f("title")], vec![]),
        ],
    )
}

/// A module registered around the install gate and around the registry's checks (an adjust of a
/// text field is not a valid effect): two actions whose effects make the branding row, one as a
/// `create` effect and one as the missing record of an `adjust`.
fn forced_makers() -> Module {
    const G: &str = FORCED_GROUP;
    forced_wrap(
        "forced_makers",
        vec![forced_model(
            "forced_maker",
            vec![
                forced_field("title"),
                forced_field("state").select(&["new", "made", "tallied"]),
            ],
            vec![
                CommandDef::new("make", "Make the brand", G)
                    .transition("state", Some("new"), "made")
                    .effect(Effect::create(MODEL, &[("name", "$self.title")])),
                CommandDef::new("tally", "Count on the brand", G)
                    .transition("state", Some("new"), "tallied")
                    .effect(Effect::adjust_match(
                        MODEL,
                        &[("name", "$self.title")],
                        "name",
                        false,
                        "1",
                        true,
                    )),
            ],
        )],
    )
}

#[test]
fn a_forced_effect_cannot_write_the_brand_row_for_anyone() {
    let rig = rig();
    rig.configure(&[("name", "MOHV")]).unwrap();
    let module = forced_module();
    assert!(
        obop_registry::check_module_in(&module, &|n| rig.d.model_def(n).map(Arc::new)).is_empty(),
        "the module is valid as far as the registry goes: only the engine stands in the way"
    );
    rig.d.register_crud(&module, &rig.store);
    for c in module.models.iter().flat_map(|m| m.commands.iter()) {
        rig.d
            .register_transition(&module, "forced_note", c.name, &rig.store);
    }
    let before_row = rig.row().unwrap();
    let before_events = rig.events();
    for who in [
        // The agent of the review: a token narrowed to platform.admin and the group of the probe.
        Principal::agent(
            "ada/agent",
            &[ADMIN_GROUP, "forced.user"],
            &[ADMIN_GROUP, "forced.user"],
        ),
        // A person with platform.admin does not get around the wrapper of `configure` either.
        Principal::user("ada", &[ADMIN_GROUP, "forced.user"]),
    ] {
        let created = rig
            .d
            .call(
                Surface::Rest,
                &who,
                &mut Vault::new(),
                "forced_note.create",
                &text_input(&[
                    ("title", "Evil Name"),
                    ("brand_id", "instance"),
                    ("state", "new"),
                ]),
            )
            .unwrap_or_else(|e| panic!("{e:?}"));
        let id = created.value.to_json()["id"].as_str().unwrap().to_string();
        let e = rig
            .d
            .call(
                Surface::Rest,
                &who,
                &mut Vault::new(),
                "forced_note.apply",
                &text_input(&[("id", &id)]),
            )
            .expect_err("the effect writes the branding row");
        let m = handler_text(e);
        assert!(
            m.contains("instance_brand is changed by its own commands only"),
            "{m}"
        );
        assert_eq!(rig.row().unwrap(), before_row, "the row is as it was");
        assert_eq!(rig.events(), before_events, "and no event was written");
    }
}

#[test]
fn the_same_wiring_writes_an_ordinary_record() {
    let rig = rig();
    let module = forced_module();
    rig.d.register_crud(&module, &rig.store);
    for c in module.models.iter().flat_map(|m| m.commands.iter()) {
        rig.d
            .register_transition(&module, "forced_note", c.name, &rig.store);
    }
    let who = Principal::user("ada", &[ADMIN_GROUP, "forced.user"]);
    let call = |id: &str, pairs: &[(&str, &str)]| {
        rig.d
            .call(
                Surface::Rest,
                &who,
                &mut Vault::new(),
                id,
                &text_input(pairs),
            )
            .unwrap_or_else(|e| panic!("{id}: {e:?}"))
            .value
            .to_json()
    };
    let target = call("forced_target.create", &[("title", "old")])["id"]
        .as_str()
        .unwrap()
        .to_string();
    let note = call(
        "forced_note.create",
        &[("title", "new"), ("target_id", &target), ("state", "new")],
    )["id"]
        .as_str()
        .unwrap()
        .to_string();
    call("forced_note.copy", &[("id", &note)]);
    assert_eq!(
        call("forced_target.get", &[("id", &target)])["title"],
        "new"
    );
}

#[test]
fn a_forced_create_or_adjust_effect_cannot_make_the_brand_row_for_anyone() {
    // With a row and without one: the effect that makes a record must not make this one, whether
    // it is a `create` effect or the missing record of an `adjust` (`create_missing`).
    for with_row in [true, false] {
        let rig = rig();
        if with_row {
            rig.configure(&[("name", "MOHV")]).unwrap();
        }
        let module = forced_makers();
        rig.d.register_crud(&module, &rig.store);
        for c in module.models.iter().flat_map(|m| m.commands.iter()) {
            rig.d
                .register_transition(&module, "forced_maker", c.name, &rig.store);
        }
        let rows = || rig.store.backend().list(MODEL).unwrap();
        let (before_row, before_rows, before_events) = (rig.row(), rows(), rig.events());
        for who in [
            Principal::agent(
                "ada/agent",
                &[ADMIN_GROUP, FORCED_GROUP],
                &[ADMIN_GROUP, FORCED_GROUP],
            ),
            Principal::user("ada", &[ADMIN_GROUP, FORCED_GROUP]),
        ] {
            for action in ["make", "tally"] {
                let call = |id: &str, pairs: &[(&str, &str)]| {
                    rig.d.call(
                        Surface::Rest,
                        &who,
                        &mut Vault::new(),
                        id,
                        &text_input(pairs),
                    )
                };
                let created = call("forced_maker.create", &[("title", "Evil Name")])
                    .unwrap_or_else(|e| panic!("{e:?}"));
                let id = created.value.to_json()["id"].as_str().unwrap().to_string();
                let e = call(&format!("forced_maker.{action}"), &[("id", &id)])
                    .expect_err("the effect makes the branding row");
                let m = handler_text(e);
                assert!(
                    m.contains("instance_brand is changed by its own commands only"),
                    "{action}: {m}"
                );
                assert_eq!(rig.row(), before_row, "{action}: the row is as it was");
                assert_eq!(rows(), before_rows, "{action}: and no other row was made");
                assert_eq!(rig.events(), before_events, "{action}: and no event");
            }
        }
    }
}
