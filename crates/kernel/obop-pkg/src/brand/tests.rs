//! Tests of the model `instance_brand`, its three commands and the values-file reader
//! (docs/spec/branding.md, section 13: "Commands", "Only these commands", "Merge", "Audit",
//! "Parity" and the model's language fields). The dispatcher is the real one with the built-in
//! module `platform`, so what is asserted is what REST, MCP and the command line see; the wire
//! (REST routes, the MCP tool list, `obop import`) is asserted in the binary's tests.

use super::*;
use crate::installer::{Installer, admin_module, register_admin};
use crate::marketplace::Bundled;
use obop_commands::{CallError, Principal, Response, Store, Surface, Vault};
use obop_storage::testing::{Flaky, Mode};
use serde_json::{Value as J, json};

const MODEL: &str = brand::MODEL;

mod effects;
mod ids;
mod labels;
mod owned;
mod routes;

struct Rig {
    d: Arc<Dispatcher>,
    store: Store,
}

fn rig_over(store: Store) -> Rig {
    let d = Arc::new(Dispatcher::new());
    let inst = Installer::new(store.clone(), &d, vec![admin_module()], Arc::new(Bundled));
    register_admin(&d, &inst);
    Rig { d, store }
}

fn rig() -> Rig {
    rig_over(Store::new())
}

fn person() -> Principal {
    Principal::user("ada", &[ADMIN_GROUP])
}

fn agent() -> Principal {
    Principal::agent("ada/agent", &[ADMIN_GROUP], &[ADMIN_GROUP])
}

/// A token narrowed to groups, which holds the group of the commands: the plain id of its owner
/// and `scopes` set.
fn scoped() -> Principal {
    let mut p = Principal::user("ada", &[ADMIN_GROUP]);
    p.scopes = Some([ADMIN_GROUP.to_string()].into_iter().collect());
    p
}

/// A mobile app's access token: the person narrowed to the groups handed over, with its device.
fn mobile() -> Principal {
    let mut p = scoped();
    p.device = Some("device-1".into());
    p
}

fn text_input(pairs: &[(&str, &str)]) -> Value {
    Value::Map(
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), Value::text(v)))
            .collect(),
    )
}

impl Rig {
    fn call(&self, p: &Principal, id: &str, input: &Value) -> Result<Response, CallError> {
        self.d.call(Surface::Rest, p, &mut Vault::new(), id, input)
    }

    fn run(&self, id: &str, pairs: &[(&str, &str)]) -> Result<J, String> {
        match self.call(&person(), id, &text_input(pairs)) {
            Ok(r) => {
                assert!(r.denied.is_empty(), "withheld: {:?}", r.denied);
                Ok(r.value.to_json())
            }
            Err(CallError::Handler(m)) => Err(m),
            Err(e) => panic!("{id}: {e:?}"),
        }
    }

    fn configure(&self, pairs: &[(&str, &str)]) -> Result<J, String> {
        self.run("instance_brand.configure", pairs)
    }

    fn get(&self) -> J {
        self.run("instance_brand.get", &[]).unwrap()
    }

    fn row(&self) -> Option<obop_storage::Fields> {
        self.store.backend().get(MODEL, "instance").unwrap()
    }

    /// The events of the log, oldest first: `(kind, user, actor, detail)`.
    fn events(&self) -> Vec<(String, String, String, String)> {
        self.store
            .backend()
            .list("security_event")
            .unwrap()
            .into_iter()
            .map(|(_, f)| {
                let g = |k: &str| f[k].as_str().unwrap().to_string();
                (g("kind"), g("user"), g("actor"), g("detail"))
            })
            .collect()
    }
}

/// The text of a JSON text field of an answer, parsed.
fn parsed(answer: &J, key: &str) -> J {
    serde_json::from_str(
        answer[key]
            .as_str()
            .unwrap_or_else(|| panic!("{key} in {answer}")),
    )
    .unwrap()
}

fn handler_text(e: CallError) -> String {
    match e {
        CallError::Handler(m) => m,
        other => panic!("not a refusal by the command: {other:?}"),
    }
}

// ---- the model -----------------------------------------------------------------------------------

fn model() -> ModelDef {
    instance_brand_model()
}

#[test]
fn the_model_has_17_stored_and_8_answer_fields_all_public_with_data_class_none() {
    let m = model();
    let names: Vec<&str> = m.fields.iter().map(|f| f.name).collect();
    let mut stored = vec!["name".to_string(), "slug".to_string()];
    stored.extend(LANGUAGES.iter().map(|(c, _)| format!("title_{c}")));
    stored.extend(LANGUAGES.iter().map(|(c, _)| format!("message_{c}")));
    stored.push("logo_svg".into());
    assert_eq!(stored.len(), 17);
    assert_eq!(
        stored,
        brand::writable_fields(),
        "the stored fields are the writable fields of apply, in its order"
    );
    let answers = [
        "dry_run",
        "set",
        "v",
        "effective",
        "fallbacks",
        "changes",
        "refusals",
        "cleared",
    ];
    let mut want: Vec<&str> = stored.iter().map(String::as_str).collect();
    want.extend(answers);
    assert_eq!(names, want, "17 stored fields, then the 8 declared answers");
    assert_eq!(names.len(), 25);
    for f in &m.fields {
        assert_eq!(f.class, Some(DataClass::None), "{}", f.name);
        assert_eq!(f.permission_group, ANY_GROUP, "{}", f.name);
        assert!(!f.personal_data, "{}", f.name);
        assert!(!f.description.is_empty(), "{}", f.name);
    }
    // The kinds: the artwork is a long text, the bool inputs and answers are bools, the rest text.
    let kind = |n: &str| m.fields.iter().find(|f| f.name == n).unwrap().kind.clone();
    assert_eq!(kind("logo_svg"), FieldKind::LongText);
    for b in ["dry_run", "set", "cleared"] {
        assert_eq!(kind(b), FieldKind::Bool, "{b}");
    }
    for t in [
        "name",
        "slug",
        "v",
        "effective",
        "fallbacks",
        "changes",
        "refusals",
    ] {
        assert_eq!(kind(t), FieldKind::Text, "{t}");
    }
    assert!(
        !names.contains(&"hide_product_docs"),
        "the switch arrives with the back office slice"
    );
}

#[test]
fn the_model_declares_configure_and_clear_for_administrators_and_reads_for_any_group() {
    let m = model();
    assert_eq!(m.name, "instance_brand");
    assert_eq!(m.read_group, ANY_GROUP);
    assert_eq!(m.write_group, ADMIN_GROUP);
    let declared: Vec<(&str, &str, bool)> = m
        .commands
        .iter()
        .map(|c| (c.name, c.permission_group, c.needs_approval))
        .collect();
    assert_eq!(
        declared,
        [
            ("configure", ADMIN_GROUP, false),
            ("clear", ADMIN_GROUP, false)
        ],
        "no approval: an agent is refused outright, not given a plan"
    );
    // `get` is the generated command of the read group.
    let all = m.all_commands();
    let get = all.iter().find(|c| c.name == "get").unwrap();
    assert_eq!(get.permission_group, ANY_GROUP);
}

#[test]
fn the_language_fields_follow_the_languages_of_the_record() {
    let codes: Vec<&str> = LANGUAGES.iter().map(|(c, _)| *c).collect();
    assert_eq!(codes, obop_commands::brand::LANGS);
    let from_fields = |prefix: &str| -> Vec<String> {
        model()
            .fields
            .iter()
            .filter_map(|f| f.name.strip_prefix(prefix).map(String::from))
            .collect()
    };
    assert_eq!(from_fields("title_"), codes);
    assert_eq!(from_fields("message_"), codes);
}

#[test]
fn the_model_has_56_label_slots_two_for_the_model_two_per_field_and_two_per_action() {
    let module = admin_module();
    let slots: Vec<_> = obop_registry::template::slots(&module)
        .into_iter()
        .filter(|s| s.model == MODEL)
        .collect();
    assert_eq!(slots.len(), 2 + 25 * 2 + 2 * 2);
    assert_eq!(slots.len(), 56);
}

#[test]
fn the_descriptions_name_the_input_keys_and_say_nothing_of_pages() {
    let m = model();
    let configure = m.commands.iter().find(|c| c.name == "configure").unwrap();
    for key in [
        "name",
        "slug",
        "title_<lang>",
        "message_<lang>",
        "logo_svg",
        "dry_run",
        "changes",
        "refusals",
    ] {
        assert!(configure.description.contains(key), "configure: {key}");
    }
    let clear = m.commands.iter().find(|c| c.name == "clear").unwrap();
    for key in ["dry_run", "cleared"] {
        assert!(clear.description.contains(key), "clear: {key}");
    }
    // What the record holds, not which pages show it: no page is built yet to show it.
    let mut texts = vec![m.description.to_string()];
    texts.extend(m.fields.iter().map(|f| f.description.to_string()));
    texts.extend(m.commands.iter().map(|c| c.description.to_string()));
    for t in texts {
        let low = t.to_lowercase();
        for word in [
            "oauth",
            "page",
            "signer",
            "back office",
            "screen",
            "favicon",
            "tab icon",
            "consent",
        ] {
            assert!(!low.contains(word), "{word:?} in {t:?}");
        }
    }
}

// ---- who may run what -----------------------------------------------------------------------------

#[test]
fn an_agent_a_scoped_token_and_a_mobile_token_are_refused_for_configure_and_clear() {
    let r = rig();
    for (who, p) in [
        ("agent", agent()),
        ("scoped", scoped()),
        ("mobile", mobile()),
    ] {
        assert!(
            p.allows(ADMIN_GROUP),
            "{who} holds the group of the command"
        );
        for (id, input) in [
            ("instance_brand.configure", text_input(&[("name", "MOHV")])),
            ("instance_brand.clear", text_input(&[])),
            (
                "instance_brand.configure",
                text_input(&[("name", "MOHV"), ("dry_run", "true")]),
            ),
        ] {
            let e = r.call(&p, id, &input).unwrap_err();
            // A refusal outright: not a plan for a person to approve.
            assert!(
                !matches!(e, CallError::ApprovalRequired { .. }),
                "{who} {id}: {e:?}"
            );
            let m = handler_text(e);
            assert!(m.contains("by a person"), "{who} {id}: {m}");
            // It says why (the branding is public) and not that pages show it: the wording is the
            // one the owner reads when an agent is turned away.
            assert!(m.contains("the branding is public"), "{who} {id}: {m}");
            assert!(!m.contains("pages"), "{who} {id}: {m}");
        }
    }
    assert!(r.row().is_none(), "nothing was stored");
    assert!(r.events().is_empty(), "nothing was logged");
}

#[test]
fn a_person_with_platform_admin_may_configure_and_clear() {
    let r = rig();
    let p = person();
    assert!(p.is_person());
    let out = r
        .call(
            &p,
            "instance_brand.configure",
            &text_input(&[("name", "MOHV")]),
        )
        .unwrap();
    assert!(r.row().is_some());
    assert!(out.value.to_json()["v"].as_str().is_some());
    let out = r
        .call(&p, "instance_brand.clear", &text_input(&[]))
        .unwrap();
    assert_eq!(out.value.to_json()["cleared"], json!(true));
    assert!(r.row().is_none());
}

#[test]
fn a_person_without_platform_admin_is_refused_by_the_permission() {
    let r = rig();
    let bob = Principal::user("bob", &["base.user"]);
    for id in ["instance_brand.configure", "instance_brand.clear"] {
        let e = r.call(&bob, id, &text_input(&[("name", "x")])).unwrap_err();
        assert!(matches!(e, CallError::Forbidden(_)), "{id}: {e:?}");
    }
    // An agent that is not scoped to the group is refused by the same permission, before the guard.
    let narrow = Principal::agent("bob/agent", &["base.user"], &["base.user"]);
    let e = r
        .call(&narrow, "instance_brand.clear", &text_input(&[]))
        .unwrap_err();
    assert!(matches!(e, CallError::Forbidden(_)), "{e:?}");
    assert!(r.row().is_none());
}

#[test]
fn get_is_open_to_any_signed_in_principal_an_agent_token_included() {
    let r = rig();
    r.configure(&[("name", "MOHV")]).unwrap();
    let nobody = Principal::user("x", &[]);
    let someone = Principal::user("bob", &["base.user"]);
    for (who, p) in [
        ("person", person()),
        ("agent", agent()),
        ("scoped", scoped()),
        ("mobile", mobile()),
        ("no groups", nobody),
        ("non-admin", someone),
        (
            "agent of a non-admin",
            Principal::agent("bob/agent", &["base.user"], &["base.user"]),
        ),
    ] {
        let out = r
            .call(&p, "instance_brand.get", &Value::map([]))
            .unwrap_or_else(|e| panic!("{who}: {e:?}"));
        assert!(out.denied.is_empty(), "{who}: {:?}", out.denied);
        assert_eq!(out.value.to_json()["name"], json!("MOHV"), "{who}");
    }
    // A session that is limited to the second-factor enrolment holds nothing, not even `*`.
    let mut limited = person();
    limited.mfa_limited = true;
    assert!(matches!(
        r.call(&limited, "instance_brand.get", &Value::map([])),
        Err(CallError::Forbidden(_))
    ));
}

// ---- only these commands --------------------------------------------------------------------------

#[test]
fn only_get_configure_and_clear_exist_and_the_model_is_in_no_listing() {
    let r = rig();
    // The generated commands are not bound: they are unknown, whoever asks.
    for c in [
        "create",
        "update",
        "delete",
        "bulk_create",
        "list",
        "search",
        "history",
    ] {
        let id = format!("instance_brand.{c}");
        for p in [person(), agent(), scoped(), mobile()] {
            let e = r
                .call(&p, &id, &text_input(&[("name", "x"), ("id", "instance")]))
                .unwrap_err();
            assert!(matches!(e, CallError::UnknownCommand(_)), "{id}: {e:?}");
        }
    }
    let ids = |p: &Principal| -> Vec<String> {
        let mut v: Vec<String> =
            r.d.catalog(Surface::Rest, p)
                .into_iter()
                .filter(|c| c.id.starts_with("instance_brand."))
                .map(|c| c.id)
                .collect();
        v.sort();
        v
    };
    assert_eq!(
        ids(&person()),
        [
            "instance_brand.clear",
            "instance_brand.configure",
            "instance_brand.get"
        ]
    );
    // Any other signed-in principal gains `get` only.
    assert_eq!(
        ids(&Principal::user("bob", &["base.user"])),
        ["instance_brand.get"]
    );
    assert_eq!(ids(&Principal::user("x", &[])), ["instance_brand.get"]);
    // No `list`, so the model is in no listing of models, on any surface.
    for surface in [Surface::Rest, Surface::Mcp, Surface::Cli] {
        for p in [person(), agent(), Principal::user("x", &[])] {
            let listed =
                r.d.meta(surface, &p)
                    .into_iter()
                    .flat_map(|m| m.models)
                    .any(|m| m.name == MODEL);
            assert!(!listed, "{surface:?}");
        }
    }
}

// ---- get -----------------------------------------------------------------------------------------

#[test]
fn get_with_no_row_answers_the_defaults_and_set_false_which_is_not_an_error() {
    let r = rig();
    let g = r.get();
    assert_eq!(g["set"], json!(false));
    assert_eq!(g["v"], json!(""));
    assert_eq!(g["name"], json!(""), "stored fields are as stored: none");
    assert_eq!(parsed(&g, "fallbacks"), json!([]));
    let e = parsed(&g, "effective");
    assert_eq!(e["name"], json!({"value": "obop", "source": "default"}));
    assert_eq!(e["logo"], json!({"value": "none", "source": "default"}));
    // The answer holds the 16 stored text fields (not the artwork), set, v, effective, fallbacks.
    let mut keys: Vec<&str> = g.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort_unstable();
    let mut want: Vec<String> = brand::writable_fields()
        .into_iter()
        .filter(|f| f != "logo_svg")
        .collect();
    want.extend(["set", "v", "effective", "fallbacks"].map(String::from));
    want.sort();
    assert_eq!(keys, want.iter().map(String::as_str).collect::<Vec<_>>());
    assert!(r.events().is_empty(), "a read writes nothing");
}

#[test]
fn get_shows_the_stored_fields_as_stored_and_never_the_artwork() {
    let r = rig();
    let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 8 8\"><rect width=\"8\" height=\"8\" fill=\"#112233\"/></svg>";
    r.configure(&[
        ("name", "MOHV"),
        ("slug", "mo-hv"),
        ("title_sv", "Logga in"),
        ("message_en", "Welcome"),
        ("logo_svg", svg),
    ])
    .unwrap();
    let g = r.get();
    assert_eq!(g["set"], json!(true));
    assert_eq!(g["name"], json!("MOHV"));
    assert_eq!(g["slug"], json!("mo-hv"));
    assert_eq!(g["title_sv"], json!("Logga in"));
    assert_eq!(g["title_de"], json!(""));
    assert_eq!(g["message_en"], json!("Welcome"));
    assert!(
        g.get("logo_svg").is_none(),
        "the artwork is served, not returned"
    );
    assert!(!g.to_string().contains("<svg"), "no artwork in the answer");
    assert_eq!(g["v"].as_str().unwrap().len(), 16);
    let e = parsed(&g, "effective");
    assert_eq!(e["logo"], json!({"value": "artwork", "source": "record"}));
    assert_eq!(e["slug"], json!({"value": "mo-hv", "source": "record"}));
    assert_eq!(e["title_de"], json!({"value": "MOHV", "source": "record"}));
}

#[test]
fn get_lists_the_fields_that_failed_the_read_check() {
    let r = rig();
    // A row written around the command.
    let mut f = obop_storage::Fields::new();
    f.insert("name".into(), json!("<script>alert(1)</script>"));
    f.insert("title_de".into(), json!("Guten Tag"));
    f.insert("logo_svg".into(), json!("<svg><script>x</script></svg>"));
    r.store
        .backend()
        .apply_checked(&[obop_storage::Op::Put {
            model: MODEL.into(),
            id: "instance".into(),
            fields: f,
        }])
        .unwrap();
    let g = r.get();
    assert_eq!(g["set"], json!(true));
    // The stored text is returned as stored (it is the operator's own data and `fallbacks` says why
    // it is not used); the effective values are the defaults.
    assert_eq!(g["title_de"], json!("Guten Tag"));
    let fb = parsed(&g, "fallbacks");
    let fields: Vec<&str> = fb
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["field"].as_str().unwrap())
        .collect();
    assert_eq!(fields, ["name", "logo_svg"]);
    for x in fb.as_array().unwrap() {
        assert!(x["reason"].as_str().unwrap().len() > 3, "{x}");
    }
    let e = parsed(&g, "effective");
    assert_eq!(e["name"], json!({"value": "obop", "source": "default"}));
}

#[test]
fn get_returns_a_stored_text_that_failed_the_read_check_escaped_so_a_terminal_cannot_be_shaped() {
    let r = rig();
    // A row written around the command: a title with a bidirectional override and a C1 control
    // (the start of an escape sequence for some terminals), a slug with a backslash, and a title
    // that is fine.
    let mut f = obop_storage::Fields::new();
    f.insert("name".into(), json!("MOHV"));
    f.insert("title_de".into(), json!("a\u{202E}b\u{9B}31m"));
    f.insert("title_sv".into(), json!("Logga in"));
    f.insert("slug".into(), json!("a\\b"));
    r.store
        .backend()
        .apply_checked(&[obop_storage::Op::Put {
            model: MODEL.into(),
            id: "instance".into(),
            fields: f,
        }])
        .unwrap();
    let g = r.get();
    assert_eq!(g["title_de"], json!("a\\u{202E}b\\u{009B}31m"));
    // A slug that breaks its pattern but holds nothing a terminal acts on is as stored.
    assert_eq!(g["slug"], json!("a\\b"));
    let fb = parsed(&g, "fallbacks");
    let fields: Vec<&str> = fb
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["field"].as_str().unwrap())
        .collect();
    assert_eq!(fields, ["slug", "title_de"]);
    // A text that works comes back as stored.
    assert_eq!(g["name"], json!("MOHV"));
    assert_eq!(g["title_sv"], json!("Logga in"));
    // What `obop call` prints holds no character that a terminal acts on.
    let printed = serde_json::to_string_pretty(&g).unwrap();
    assert!(
        printed.chars().all(|c| !c.is_control() || c == '\n'),
        "{printed}"
    );
    assert!(!printed.contains('\u{202E}'), "{printed}");
}

#[test]
fn get_under_a_failing_store_is_the_error_and_never_set_false() {
    let (flaky, backend) = Flaky::memory();
    let r = rig_over(Store::over(backend));
    r.configure(&[("name", "MOHV")]).unwrap();
    flaky.set(Mode::Reads);
    let e = r
        .call(&person(), "instance_brand.get", &Value::map([]))
        .unwrap_err();
    let m = handler_text(e);
    assert!(
        m.starts_with("storage_busy"),
        "the store's text is the error, a D74 code word first: {m}"
    );
    flaky.set(Mode::Healthy);
    assert_eq!(
        r.get()["set"],
        json!(true),
        "and it reads again when the store answers"
    );
}

#[test]
fn get_reads_the_store_every_time_so_a_change_made_elsewhere_shows_at_once() {
    let r = rig();
    assert_eq!(r.get()["set"], json!(false));
    // Another process (or the command line): no reader, so nothing of this process's cell moves.
    let backend = r.store.backend();
    brand::apply(
        &*backend,
        None,
        "operator",
        &Action::Configure(vec![("name".into(), "Elsewhere".into())]),
        false,
    )
    .unwrap();
    let g = r.get();
    assert_eq!(g["set"], json!(true));
    assert_eq!(g["name"], json!("Elsewhere"));
}

// ---- configure -------------------------------------------------------------------------------------

#[test]
fn the_first_call_needs_a_name_and_a_refusal_stores_nothing_of_the_call() {
    let r = rig();
    let m = r.configure(&[("title_sv", "Logga in")]).unwrap_err();
    assert!(m.starts_with("name: "), "{m}");
    assert!(m.contains("required"), "{m}");
    assert!(r.row().is_none());
    assert!(r.events().is_empty());
}

#[test]
fn a_refusal_lists_every_refusal_one_per_line_and_stores_nothing() {
    let r = rig();
    r.configure(&[("name", "MOHV"), ("title_de", "Hallo")])
        .unwrap();
    let before = r.row();
    let events = r.events().len();
    let m = r
        .configure(&[
            ("name", "Foo: Bar"),
            ("title_de", "<b>"),
            ("title_sv", "Fine"),
            ("slug", "9x"),
        ])
        .unwrap_err();
    let lines: Vec<&str> = m.lines().collect();
    assert_eq!(lines.len(), 3, "{m}");
    for (line, field) in lines.iter().zip(["name", "slug", "title_de"]) {
        assert!(line.starts_with(&format!("{field}: ")), "{line}");
    }
    assert_eq!(r.row(), before, "the valid title_sv was not stored either");
    assert_eq!(r.events().len(), events);
}

#[test]
fn dry_run_answers_the_changes_and_the_typed_refusals_and_writes_nothing() {
    let r = rig();
    let out = r
        .configure(&[
            ("name", "MOHV"),
            ("title_sv", "Logga in"),
            ("dry_run", "true"),
        ])
        .unwrap();
    assert_eq!(out["dry_run"], json!(true));
    let changes = parsed(&out, "changes");
    assert_eq!(
        changes,
        json!([
            {"field": "name", "old": "", "new": "MOHV"},
            {"field": "title_sv", "old": "", "new": "Logga in"},
        ])
    );
    assert_eq!(parsed(&out, "refusals"), json!([]));
    assert_eq!(
        out["v"].as_str().unwrap().len(),
        16,
        "the version it would have"
    );
    assert!(r.row().is_none(), "nothing was written");
    assert!(r.events().is_empty(), "nothing was logged");
    // The refusals of a real call, as the typed list.
    let out = r
        .configure(&[
            ("name", "Foo: Bar"),
            ("title_de", "<b>"),
            ("dry_run", "true"),
        ])
        .unwrap();
    assert_eq!(out["dry_run"], json!(true));
    let refusals = parsed(&out, "refusals");
    let fields: Vec<&str> = refusals
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["field"].as_str().unwrap())
        .collect();
    assert_eq!(fields, ["name", "title_de"]);
    assert!(refusals[0]["rule"].as_str().unwrap().len() > 3);
    assert_eq!(parsed(&out, "changes"), json!([]));
    // And the real call refuses with the same fields and rules, one per line.
    let m = r
        .configure(&[("name", "Foo: Bar"), ("title_de", "<b>")])
        .unwrap_err();
    let lines: Vec<String> = refusals
        .as_array()
        .unwrap()
        .iter()
        .map(|x| {
            format!(
                "{}: {}",
                x["field"].as_str().unwrap(),
                x["rule"].as_str().unwrap()
            )
        })
        .collect();
    assert_eq!(m, lines.join("\n"));
}

#[test]
fn a_real_call_answers_without_dry_run_or_refusals() {
    let r = rig();
    let out = r.configure(&[("name", "MOHV")]).unwrap();
    let mut keys: Vec<&str> = out
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["changes", "v"]);
    assert_eq!(
        parsed(&out, "changes"),
        json!([{"field": "name", "old": "", "new": "MOHV"}])
    );
}

#[test]
fn a_bool_input_is_the_json_bool_or_the_text_true_or_false() {
    let r = rig();
    // The text, as `obop call` sends it.
    for text in ["true", "false"] {
        let out = r.configure(&[("name", "MOHV"), ("dry_run", text)]).unwrap();
        assert_eq!(out.get("dry_run").is_some(), text == "true", "{text}");
    }
    r.run("instance_brand.clear", &[]).unwrap();
    // The JSON booleans.
    for (b, writes) in [(true, false), (false, true)] {
        let input = Value::map([("name", Value::text("MOHV")), ("dry_run", Value::Bool(b))]);
        let out = r
            .call(&person(), "instance_brand.configure", &input)
            .unwrap();
        assert_eq!(out.value.to_json().get("dry_run").is_some(), b);
        assert_eq!(r.row().is_some(), writes, "dry_run {b}");
        r.run("instance_brand.clear", &[]).unwrap();
    }
    // Absent and null are false.
    let input = Value::map([("name", Value::text("MOHV")), ("dry_run", Value::Null)]);
    r.call(&person(), "instance_brand.configure", &input)
        .unwrap();
    assert!(r.row().is_some());
    // Anything else is refused by name, and nothing is stored.
    r.run("instance_brand.clear", &[]).unwrap();
    for bad in ["yes", "TRUE", "1", "", "True"] {
        let m = r
            .configure(&[("name", "MOHV"), ("dry_run", bad)])
            .unwrap_err();
        assert!(m.contains("dry_run must be true or false"), "{bad}: {m}");
        assert!(r.row().is_none());
        let m = r
            .run("instance_brand.clear", &[("dry_run", bad)])
            .unwrap_err();
        assert!(m.contains("dry_run must be true or false"), "{bad}: {m}");
    }
    for bad in [Value::Int(1), Value::List(vec![])] {
        let input = Value::map([("name", Value::text("MOHV")), ("dry_run", bad)]);
        let e = r
            .call(&person(), "instance_brand.configure", &input)
            .unwrap_err();
        assert!(handler_text(e).contains("dry_run must be true or false"));
    }
}

#[test]
fn an_input_key_that_is_not_a_writable_field_or_dry_run_is_refused_by_name() {
    let r = rig();
    for key in [
        "v",
        "id",
        "set",
        "effective",
        "fallbacks",
        "changes",
        "refusals",
        "cleared",
        "bogus",
        "hide_product_docs",
        "logo",
        "title_xx",
        "message_",
        // Only the two keys the dispatcher itself adds pass: not every key that starts with `_`.
        "_foo",
        "_caller2",
        "_Actor",
        "__caller",
        "caller",
    ] {
        let m = r.configure(&[("name", "MOHV"), (key, "x")]).unwrap_err();
        assert!(
            m.contains(&format!("the key {key} is not accepted")),
            "{key}: {m}"
        );
        assert!(m.starts_with("instance_brand.configure: "), "{m}");
        assert!(r.row().is_none(), "{key}: nothing is stored");
        // `clear` takes `dry_run` and nothing else.
        let m = r.run("instance_brand.clear", &[(key, "x")]).unwrap_err();
        assert!(
            m.contains(&format!("the key {key} is not accepted")),
            "{key}: {m}"
        );
        assert!(m.starts_with("instance_brand.clear: "), "{m}");
    }
    // `clear` refuses a writable field of configure.
    let m = r.run("instance_brand.clear", &[("name", "x")]).unwrap_err();
    assert!(m.contains("the key name is not accepted"), "{m}");
    // Every unknown key is named, not only the first.
    let m = r
        .configure(&[("name", "MOHV"), ("v", "1"), ("id", "x"), ("zzz", "1")])
        .unwrap_err();
    assert_eq!(m.lines().count(), 3, "{m}");
    for k in ["id", "v", "zzz"] {
        assert!(m.contains(&format!("the key {k} ")), "{k}: {m}");
    }
}

#[test]
fn a_hostile_input_key_is_repeated_only_as_far_as_it_is_safe_and_cannot_pick_a_row() {
    let r = rig();
    // A key that looks like the code of an error row, or like a forged line, or is very long.
    for key in [
        "denied",
        "storage_busy",
        "precondition_failed",
        "x\nname: forged",
        "x\u{1b}[2J",
        "\u{202e}rtl",
        "k\u{0000}k",
    ] {
        let m = r.configure(&[("name", "MOHV"), (key, "x")]).unwrap_err();
        assert!(m.starts_with("instance_brand.configure: "), "{m:?}");
        assert_eq!(m.lines().count(), 1, "{m:?}");
        assert!(m.chars().all(|c| !c.is_control()), "{m:?}");
        assert!(!m.contains('\u{202e}'), "{m:?}");
    }
    let long = "k".repeat(500);
    let m = r
        .configure(&[("name", "MOHV"), (long.as_str(), "x")])
        .unwrap_err();
    assert!(m.len() < 200, "{}", m.len());
    assert!(m.contains("..."), "{m}");
}

#[test]
fn the_keys_the_dispatcher_adds_itself_pass_and_a_client_cannot_send_them() {
    let r = rig();
    let backend = r.store.backend();
    let reader = r.d.brand();
    // What `call` hands to the handler: `_caller` and, for a handler that asks for it, the actor.
    let input = Value::map([
        ("name", Value::text("MOHV")),
        ("_caller", Value::text("ada")),
        ("_actor", Value::text("ada")),
        ("_actor_groups", Value::text("platform.admin")),
    ]);
    configure(&*backend, &reader, &input).unwrap();
    assert_eq!(r.events()[0].2, "ada");
    // Sent by a client they are dropped before the handler sees them: the call still succeeds, and
    // the actor is the real one.
    let input = Value::map([
        ("name", Value::text("MOHV2")),
        ("_caller", Value::text("someone-else")),
        ("_actor_groups", Value::text("x")),
    ]);
    r.call(&person(), "instance_brand.configure", &input)
        .unwrap();
    let last = r.events().pop().unwrap();
    assert_eq!(last.2, "ada", "the caller is set by the dispatcher");
}

#[test]
fn a_value_that_is_not_text_is_refused_by_name() {
    let r = rig();
    for bad in [
        Value::Int(5),
        Value::Bool(true),
        Value::Null,
        Value::List(vec![]),
    ] {
        let input = Value::map([("name", Value::text("MOHV")), ("title_de", bad)]);
        let m = handler_text(
            r.call(&person(), "instance_brand.configure", &input)
                .unwrap_err(),
        );
        assert!(m.contains("title_de must be text"), "{m}");
        assert!(r.row().is_none());
    }
}

#[test]
fn the_actor_of_each_event_is_the_principal_and_the_user_is_the_instance() {
    let r = rig();
    r.configure(&[("name", "MOHV"), ("title_sv", "Logga in")])
        .unwrap();
    let events = r.events();
    assert_eq!(events.len(), 2);
    for (kind, user, actor, _) in &events {
        assert_eq!(kind, "brand_configured");
        assert_eq!(user, "(instance)");
        assert_eq!(actor, "ada");
    }
    r.run("instance_brand.clear", &[]).unwrap();
    let events = r.events();
    assert_eq!(
        events.len(),
        4,
        "one brand_cleared event per non-empty stored field"
    );
    for (kind, user, actor, detail) in &events[2..] {
        assert_eq!(kind, "brand_cleared");
        assert_eq!(user, "(instance)");
        assert_eq!(actor, "ada");
        assert!(detail.ends_with("-> removed"), "{detail}");
    }
}

#[test]
fn the_same_values_twice_change_nothing_and_write_no_event() {
    let r = rig();
    r.configure(&[("name", "MOHV"), ("title_sv", "Logga in")])
        .unwrap();
    let (row, events) = (r.row(), r.events());
    let out = r
        .configure(&[("name", "MOHV"), ("title_sv", "Logga in")])
        .unwrap();
    assert_eq!(parsed(&out, "changes"), json!([]));
    assert_eq!(r.row(), row);
    assert_eq!(r.events(), events);
}

#[test]
fn the_one_flag_logo_swap_keeps_the_rest_and_an_empty_text_clears_only_its_field() {
    let r = rig();
    let svg = |fill: &str| {
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 8 8\"><rect width=\"8\" height=\"8\" fill=\"{fill}\"/></svg>"
        )
    };
    r.configure(&[
        ("name", "MOHV"),
        ("title_sv", "Logga in"),
        ("title_de", "Anmelden"),
        ("message_en", "Welcome"),
    ])
    .unwrap();
    let v0 = r.get()["v"].as_str().unwrap().to_string();
    // The artwork alone: everything else stays.
    let out = r.configure(&[("logo_svg", &svg("#112233"))]).unwrap();
    let v1 = out["v"].as_str().unwrap().to_string();
    assert_ne!(v0, v1);
    let g = r.get();
    assert_eq!(g["name"], json!("MOHV"));
    assert_eq!(g["title_sv"], json!("Logga in"));
    assert_eq!(g["title_de"], json!("Anmelden"));
    assert_eq!(g["message_en"], json!("Welcome"));
    assert_eq!(g["v"], json!(v1));
    let changes = parsed(&out, "changes");
    assert_eq!(changes.as_array().unwrap().len(), 1);
    assert_eq!(changes[0]["field"], json!("logo_svg"));
    assert_eq!(
        changes[0]["old"],
        json!(""),
        "the artwork as old and new version: none was stored before (the tile is no artwork)"
    );
    assert_eq!(changes[0]["new"], json!(v1));
    // A second swap names both versions.
    let out = r.configure(&[("logo_svg", &svg("#445566"))]).unwrap();
    let v2 = out["v"].as_str().unwrap().to_string();
    let changes = parsed(&out, "changes");
    assert_eq!(changes[0]["old"], json!(v1));
    assert_eq!(changes[0]["new"], json!(v2));
    // The audit detail holds the version and never the artwork.
    let last = r.events().pop().unwrap();
    assert!(last.3.starts_with("logo_svg: "), "{}", last.3);
    assert!(!last.3.contains("<svg"), "{}", last.3);
    // An empty text clears only that field: the Swedish title goes, the German one stays.
    r.configure(&[("title_sv", "")]).unwrap();
    let g = r.get();
    assert_eq!(g["title_sv"], json!(""));
    assert_eq!(g["title_de"], json!("Anmelden"));
    // The empty artwork returns to the tile.
    let out = r.configure(&[("logo_svg", "")]).unwrap();
    assert_eq!(out["v"], json!(v0), "the tile of the same name and theme");
    // The name cannot be cleared, only replaced.
    let m = r.configure(&[("name", "")]).unwrap_err();
    assert!(m.starts_with("name: "), "{m}");
    assert_eq!(r.get()["name"], json!("MOHV"));
}

// ---- clear -----------------------------------------------------------------------------------------

#[test]
fn clear_with_no_row_answers_cleared_false_and_writes_no_event() {
    let r = rig();
    assert_eq!(
        r.run("instance_brand.clear", &[]).unwrap(),
        json!({"cleared": false})
    );
    assert!(r.events().is_empty());
    let out = r
        .run("instance_brand.clear", &[("dry_run", "true")])
        .unwrap();
    assert_eq!(out, json!({"cleared": false, "dry_run": true}));
}

#[test]
fn clear_removes_the_row_and_with_dry_run_removes_nothing() {
    let r = rig();
    r.configure(&[("name", "MOHV"), ("title_sv", "Logga in")])
        .unwrap();
    let events = r.events().len();
    let out = r
        .run("instance_brand.clear", &[("dry_run", "true")])
        .unwrap();
    assert_eq!(out, json!({"cleared": true, "dry_run": true}));
    assert!(r.row().is_some(), "a dry run removes nothing");
    assert_eq!(r.events().len(), events, "and logs nothing");
    let out = r.run("instance_brand.clear", &[]).unwrap();
    assert_eq!(out, json!({"cleared": true}));
    assert!(r.row().is_none());
    assert_eq!(r.events().len(), events + 2);
    let g = r.get();
    assert_eq!(g["set"], json!(false), "the stock look is back");
    assert_eq!(g["name"], json!(""));
}

// ---- the answers are declared, so every surface shows the same keys ------------------------------

#[test]
fn every_key_of_every_answer_is_a_declared_field_so_shape_withholds_none() {
    let r = rig();
    let declared: Vec<&str> = model().fields.iter().map(|f| f.name).collect();
    let answers = [
        r.configure(&[("name", "MOHV"), ("dry_run", "true")])
            .unwrap(),
        r.configure(&[("name", "MOHV")]).unwrap(),
        r.get(),
        r.run("instance_brand.clear", &[("dry_run", "true")])
            .unwrap(),
        r.run("instance_brand.clear", &[]).unwrap(),
    ];
    for a in answers {
        for k in a.as_object().unwrap().keys() {
            assert!(
                declared.contains(&k.as_str()),
                "{k} is not a declared field"
            );
        }
    }
    // The answers of an agent's `get` are the same keys: the fields are class none and group `*`.
    let g_person = r.get();
    let g_agent = r
        .call(&agent(), "instance_brand.get", &Value::map([]))
        .unwrap();
    assert!(g_agent.denied.is_empty());
    assert_eq!(g_agent.value.to_json(), g_person);
}

#[test]
fn the_three_surfaces_give_the_same_answers() {
    // REST, MCP and the command line all end in `Dispatcher::call`, and the surface only decides
    // whether the command is listed: every command is on all three.
    let r = rig();
    r.configure(&[("name", "MOHV"), ("title_sv", "Logga in")])
        .unwrap();
    for id in [
        "instance_brand.get",
        "instance_brand.configure",
        "instance_brand.clear",
    ] {
        let all: Vec<bool> = [Surface::Rest, Surface::Mcp, Surface::Cli]
            .into_iter()
            .map(|s| r.d.catalog(s, &person()).iter().any(|c| c.id == id))
            .collect();
        assert_eq!(all, [true, true, true], "{id}");
    }
    let answers: Vec<J> = [Surface::Rest, Surface::Mcp, Surface::Cli]
        .into_iter()
        .map(|s| {
            r.d.call(
                s,
                &person(),
                &mut Vault::new(),
                "instance_brand.get",
                &Value::map([]),
            )
            .unwrap()
            .value
            .to_json()
        })
        .collect();
    assert_eq!(answers[0], answers[1]);
    assert_eq!(answers[1], answers[2]);
}

// ---- the values file ---------------------------------------------------------------------------------

fn file_err(text: &str) -> Vec<String> {
    read_values_file(text).unwrap_err()
}

#[test]
fn a_values_file_gives_the_keys_of_the_record_table_in_the_order_of_the_fields() {
    let got = read_values_file(
        "[record]\nmessage_en = \"Hello\"\ntitle_sv = \"Logga in\"\nname = \"MOHV\"\nslug = \"mohv\"\nlogo_svg = \"<svg/>\"\n",
    )
    .unwrap();
    assert_eq!(
        got,
        [
            ("name".to_string(), "MOHV".to_string()),
            ("slug".to_string(), "mohv".to_string()),
            ("title_sv".to_string(), "Logga in".to_string()),
            ("message_en".to_string(), "Hello".to_string()),
            ("logo_svg".to_string(), "<svg/>".to_string()),
        ]
    );
    // An empty text is a value: it clears the field.
    let got = read_values_file("[record]\nname = \"MOHV\"\ntitle_sv = \"\"\n").unwrap();
    assert_eq!(got[1], ("title_sv".to_string(), String::new()));
    // An inline table is the same table.
    let got = read_values_file("record = { name = \"MOHV\" }\n").unwrap();
    assert_eq!(got, [("name".to_string(), "MOHV".to_string())]);
}

#[test]
fn a_phone_table_is_ignored_whatever_it_holds() {
    let got = read_values_file(
        "[record]\nname = \"MOHV\"\n\n[phone]\napplication_id = \"se.mohv.app\"\nname = \"Phone\"\nv = 3\ndry_run = true\n[phone.nested]\nx = 1\n",
    )
    .unwrap();
    assert_eq!(
        got,
        [("name".to_string(), "MOHV".to_string())],
        "the name is stated once"
    );
    // Even a `[phone]` table with keys that `[record]` would refuse.
    assert!(read_values_file("[record]\nname = \"MOHV\"\n[phone]\nset = [1, 2]\n").is_ok());
}

#[test]
fn a_values_file_refuses_every_key_that_is_not_a_writable_field_the_answer_fields_included() {
    for key in [
        "dry_run",
        "set",
        "v",
        "effective",
        "fallbacks",
        "changes",
        "refusals",
        "cleared",
        "id",
        "bogus",
        "hide_product_docs",
        "title_xx",
    ] {
        let reasons = file_err(&format!("[record]\nname = \"MOHV\"\n{key} = \"x\"\n"));
        assert_eq!(reasons.len(), 1, "{key}: {reasons:?}");
        assert!(reasons[0].starts_with("[record] the key "), "{reasons:?}");
        assert!(
            reasons[0].contains(&format!("the key {key} ")),
            "{key}: {reasons:?}"
        );
        assert!(
            reasons[0].contains("not a field of the brand"),
            "{reasons:?}"
        );
    }
}

#[test]
fn a_values_file_refuses_another_table_a_key_outside_a_table_and_a_value_that_is_not_text() {
    let r = file_err("[record]\nname = \"MOHV\"\n[extra]\nx = 1\n");
    assert_eq!(r.len(), 1);
    assert!(r[0].contains("[extra] is not read"), "{r:?}");
    let r = file_err("name = \"MOHV\"\n[record]\nname = \"MOHV\"\n");
    assert_eq!(r.len(), 1);
    assert!(r[0].contains("the key name is outside a table"), "{r:?}");
    let r = file_err("[record]\nname = 5\n");
    assert_eq!(r, ["[record] name must be text (a string)"]);
    for value in ["true", "[\"a\"]", "{ a = \"b\" }", "1.5", "2026-11-06"] {
        let r = file_err(&format!("[record]\nname = \"MOHV\"\ntitle_en = {value}\n"));
        assert_eq!(r, ["[record] title_en must be text (a string)"], "{value}");
    }
    let r = file_err("record = 5\n");
    assert!(r[0].contains("[record] must be a table"), "{r:?}");
    let r = file_err("[record]\nname = \"MOHV\"\nphone = 1\n");
    assert!(
        r[0].contains("the key phone is not a field of the brand"),
        "{r:?}"
    );
    let r = file_err("phone = 3\n[record]\nname = \"MOHV\"\n");
    assert!(r[0].contains("[phone] must be a table"), "{r:?}");
}

#[test]
fn a_values_file_without_a_record_table_is_refused() {
    let r = file_err("");
    assert_eq!(r, ["the file has no [record] table"]);
    let r = file_err("[phone]\napplication_id = \"se.mohv.app\"\n");
    assert_eq!(r, ["the file has no [record] table"]);
}

#[test]
fn a_values_file_lists_every_reason_at_once() {
    let r = file_err(
        "stray = 1\n[record]\nv = \"1\"\nname = 5\nbogus = \"x\"\n[other]\na = 1\n[third]\nb = 2\n",
    );
    assert_eq!(r.len(), 6, "{r:?}");
}

#[test]
fn a_values_file_reason_never_holds_a_value_and_a_hostile_key_is_cut_to_what_is_safe() {
    const SECRET: &str = "VALUE-THAT-MUST-NOT-APPEAR";
    // A refused key with a value, a refused table with a value, a value of the wrong type.
    let files = [
        format!("[record]\nname = \"{SECRET}\"\nv = \"{SECRET}\"\n"),
        format!("[record]\nname = \"{SECRET}\"\n[other]\nx = \"{SECRET}\"\n"),
        format!("[record]\nname = \"{SECRET}\"\ntitle_en = [\"{SECRET}\"]\n"),
        format!("stray = \"{SECRET}\"\n[record]\nname = \"{SECRET}\"\n"),
        // A TOML error on the line that holds the value.
        format!("[record]\nname = \"{SECRET}\"\ntitle_en = {SECRET}\n"),
        format!("[record]\nname = \"{SECRET}\n"),
        format!("[record]\nname = \"{SECRET}\"\nname = \"{SECRET}\"\n"),
    ];
    for f in &files {
        let reasons = file_err(f);
        assert!(!reasons.is_empty(), "{f}");
        for line in &reasons {
            assert!(
                !line.contains(SECRET),
                "a reason holds a value: {line:?} for {f:?}"
            );
        }
    }
    // A hostile key: a line break, an escape sequence and a right-to-left override.
    let r = file_err(
        "[record]\nname = \"MOHV\"\n\"x\\ny: forged\" = \"1\"\n\"a\\u001b[2J\" = \"1\"\n\"\\u202ertl\" = \"1\"\n",
    );
    assert_eq!(r.len(), 3, "{r:?}");
    for line in &r {
        assert_eq!(line.lines().count(), 1, "{line:?}");
        assert!(
            line.chars().all(|c| !c.is_control() && c != '\u{202e}'),
            "{line:?}"
        );
    }
    // A very long key is cut.
    let long = "k".repeat(500);
    let r = file_err(&format!("[record]\nname = \"MOHV\"\n{long} = \"1\"\n"));
    assert!(r[0].len() < 250 && r[0].contains("..."), "{}", r[0].len());
}

#[test]
fn a_values_file_that_is_not_toml_is_refused_with_the_line_and_without_the_text() {
    let r = file_err("[record\nname = \"MOHV\"\n");
    assert_eq!(r.len(), 1);
    assert!(
        r[0].starts_with("the file is not valid TOML (line 1)"),
        "{r:?}"
    );
    let r = file_err("[record]\nname = \"MOHV\"\nname = \"again\"\n");
    assert!(r[0].contains("line 3"), "{r:?}");
    assert!(!r[0].contains("again"), "{r:?}");
    assert!(r[0].chars().all(|c| !c.is_control()));
    // A very long message is cut.
    assert!(r[0].len() < 200);
    // Text with a byte order mark is read as TOML reads it, and never panics.
    let _ = read_values_file("\u{feff}[record]\nname = \"MOHV\"\n");
}

#[test]
fn a_values_file_is_total_over_truncated_and_odd_text() {
    let seed = "[record]\nname = \"MOHV\"\ntitle_sv = \"Logga in\"\nlogo_svg = \"<svg/>\"\n[phone]\napplication_id = \"se.mohv.app\"\n";
    for n in 0..=seed.len() {
        if seed.is_char_boundary(n) {
            let _ = read_values_file(&seed[..n]);
        }
    }
    for odd in [
        "\0",
        "[[record]]\n",
        "[record]\n[record]\n",
        "[record.name]\nx = 1\n",
    ] {
        let _ = read_values_file(odd);
    }
}

// ---- the reader of the dispatcher follows a command ----------------------------------------------
//
// `configure` and `clear` hand the dispatcher's own reader to `brand::apply` (`Some(reader)`), and
// that is what makes a change made through a command show at once in the process that made it, and
// what stops a refill that began before the change from putting the replaced brand back. Passing
// `None` instead leaves every test that calls `brand::apply` itself green, so these two tests pin
// the wiring (docs/spec/branding.md, section 13, "Cache": "a `configure` in the same process shows
// at once"; "a refill that read the old row before a `configure` and finishes after it stores
// nothing").

#[test]
fn a_configure_and_a_clear_through_the_dispatcher_show_in_its_reader_at_once() {
    let r = rig();
    let reader = r.d.brand();
    // The cell is filled first: its value is good for two seconds, so a change that did not reach
    // it would not show.
    let before = reader.current();
    assert_eq!(before.brand().name(), "obop");
    assert!(!before.is_set());

    r.configure(&[("name", "First")]).unwrap();
    let shown = reader.current();
    assert_eq!(shown.brand().name(), "First");
    assert!(shown.is_set());

    // Another change merges over the first one and shows at once too.
    r.configure(&[("title_sv", "Logga in"), ("name", "Second")])
        .unwrap();
    let shown = reader.current();
    assert_eq!(shown.brand().name(), "Second");
    assert_eq!(shown.brand().title("sv"), "Logga in");

    // A dry run writes nothing and moves nothing.
    r.configure(&[("name", "Third"), ("dry_run", "true")])
        .unwrap();
    assert_eq!(reader.current().brand().name(), "Second");
    r.run("instance_brand.clear", &[("dry_run", "true")])
        .unwrap();
    assert_eq!(reader.current().brand().name(), "Second");

    // A clear shows the default at once.
    r.run("instance_brand.clear", &[]).unwrap();
    let shown = reader.current();
    assert_eq!(shown.brand().name(), "obop");
    assert!(!shown.is_set());
    assert_eq!(shown.brand().title("sv"), "obop");
}

/// Runs `id` as the person, from inside the read of a refill (`Flaky::after_the_get_of`): the
/// refill has read the old row and has not stored yet, which is what a slow store does to it.
fn during_the_refill(flaky: &Flaky, r: &Rig, id: &'static str, pairs: &[(&str, &str)]) {
    let d = r.d.clone();
    let input = text_input(pairs);
    flaky.after_the_get_of(
        MODEL,
        0,
        Box::new(move |_| {
            d.call(Surface::Rest, &person(), &mut Vault::new(), id, &input)
                .unwrap();
        }),
    );
}

#[test]
fn a_refill_that_read_the_row_before_a_command_changed_it_does_not_put_the_old_brand_back() {
    // Each case has its own dispatcher, so its cell is empty and the next `current()` is a refill.
    let rig = || {
        let (flaky, backend) = Flaky::memory();
        let r = rig_over(Store::over(backend));
        (flaky, r)
    };

    // The refill reads "no row", a `configure` commits inside it, the refill ends.
    let (flaky, r) = rig();
    let reader = r.d.brand();
    during_the_refill(
        &flaky,
        &r,
        "instance_brand.configure",
        &[("name", "Fresh"), ("title_de", "Willkommen")],
    );
    let _ = reader.current();
    assert!(r.row().is_some(), "the command ran inside the refill");
    let shown = reader.current();
    assert_eq!(
        shown.brand().name(),
        "Fresh",
        "the refill stored nothing: the default it read is not kept"
    );
    assert_eq!(shown.brand().title("de"), "Willkommen");
    assert!(shown.is_set());

    // The refill reads a row, a `configure` replaces it inside the refill, the refill ends.
    let (flaky, r) = rig();
    let reader = r.d.brand();
    r.configure(&[("name", "One")]).unwrap();
    during_the_refill(&flaky, &r, "instance_brand.configure", &[("name", "Two")]);
    let _ = reader.current();
    assert_eq!(reader.current().brand().name(), "Two");

    // The refill reads a row, a `clear` removes it inside the refill, the refill ends.
    let (flaky, r) = rig();
    let reader = r.d.brand();
    r.configure(&[("name", "One"), ("title_sv", "Logga in")])
        .unwrap();
    during_the_refill(&flaky, &r, "instance_brand.clear", &[]);
    let _ = reader.current();
    assert!(r.row().is_none(), "the command ran inside the refill");
    let shown = reader.current();
    assert_eq!(
        shown.brand().name(),
        "obop",
        "the refill did not put the cleared brand back"
    );
    assert!(!shown.is_set());
    assert_eq!(shown.brand().title("sv"), "obop");
}

// ---- the input keys, the refusal lines and the defensive rules of the handlers ---------------------

#[test]
fn only_the_caller_and_the_actor_prefix_are_keys_of_the_dispatcher() {
    assert!(dispatcher_key("_caller"));
    assert!(dispatcher_key("_actor"));
    assert!(dispatcher_key("_actor_groups"));
    for key in [
        "_foo", "_caller2", "_Caller", "caller", "actor", "__actor", "_", "",
    ] {
        assert!(!dispatcher_key(key), "{key:?}");
    }
}

#[test]
fn the_problems_with_the_input_are_listed_one_per_line_before_anything_is_stored() {
    let r = rig();
    // Two unknown keys, a value that is not text and a bad `dry_run`, in one call.
    let input = Value::map([
        ("name", Value::text("MOHV")),
        ("zz1", Value::text("x")),
        ("zz2", Value::text("x")),
        ("title_de", Value::Int(5)),
        ("dry_run", Value::text("maybe")),
    ]);
    let m = handler_text(
        r.call(&person(), "instance_brand.configure", &input)
            .unwrap_err(),
    );
    assert_eq!(m.lines().count(), 4, "{m}");
    for line in m.lines() {
        assert!(line.starts_with("instance_brand"), "{line}");
    }
    assert!(m.contains("the key zz1 "), "{m}");
    assert!(m.contains("the key zz2 "), "{m}");
    assert!(m.contains("title_de must be text"), "{m}");
    assert!(m.contains("dry_run must be true or false"), "{m}");
    assert!(r.row().is_none() && r.events().is_empty());
    // And for `clear`.
    let input = Value::map([
        ("zz1", Value::text("x")),
        ("name", Value::text("x")),
        ("dry_run", Value::Int(1)),
    ]);
    let m = handler_text(
        r.call(&person(), "instance_brand.clear", &input)
            .unwrap_err(),
    );
    assert_eq!(m.lines().count(), 3, "{m}");
}

/// Every kind of character that a deny-list of controls would let through, in a key, repeated as
/// one `?`: through `configure`, through `clear` and through a values file (the full table is the
/// test of `show_key` in `obop-commands`).
#[test]
fn a_hostile_key_is_repeated_as_question_marks_by_configure_clear_and_a_values_file() {
    let r = rig();
    for c in [
        '\u{200b}',
        '\u{200f}',
        '\u{2028}',
        '\u{2029}',
        '\u{0085}',
        '\u{2066}',
        '\u{2069}',
        '\u{feff}',
        '\u{e0001}',
        '\u{e0041}',
        '\u{fe0f}',
        '\u{3164}',
        '\u{00a0}',
        '\u{00ad}',
        '\u{202e}',
        '\u{1b}',
        '\n',
        '<',
        '>',
        '"',
        '\'',
        '&',
        '/',
        '\\',
        ';',
        ' ',
        '.',
    ] {
        let key = format!("a{c}b");
        let want = "the key a?b is not accepted";
        let m = r
            .configure(&[("name", "MOHV"), (key.as_str(), "x")])
            .unwrap_err();
        assert!(m.ends_with(want), "U+{:04X}: {m:?}", c as u32);
        assert_eq!(m.lines().count(), 1, "U+{:04X}: {m:?}", c as u32);
        let m = r
            .run("instance_brand.clear", &[(key.as_str(), "x")])
            .unwrap_err();
        assert!(m.ends_with(want), "U+{:04X}: {m:?}", c as u32);
        // In a values file the key is written with TOML's escapes.
        let file = format!(
            "[record]\nname = \"MOHV\"\n\"a\\U{:08X}b\" = \"1\"\n",
            c as u32
        );
        let reasons = file_err(&file);
        assert_eq!(reasons.len(), 1, "U+{:04X}: {reasons:?}", c as u32);
        assert!(
            reasons[0].starts_with("[record] the key a?b is not a field of the brand"),
            "U+{:04X}: {:?}",
            c as u32,
            reasons[0]
        );
    }
}

#[test]
fn a_call_without_a_caller_is_refused_and_stores_nothing() {
    let r = rig();
    let backend = r.store.backend();
    let reader = r.d.brand();
    // The dispatcher always sets `_caller`; a handler called without it (or with an empty one)
    // would write an event with no actor, so it refuses.
    for caller in [None, Some(Value::text("")), Some(Value::Int(1))] {
        let with_caller = |mut m: Input| {
            if let Some(c) = &caller {
                m.insert("_caller".to_string(), c.clone());
            }
            Value::Map(m)
        };
        let named = Input::from([("name".to_string(), Value::text("MOHV"))]);
        let m = configure(&*backend, &reader, &with_caller(named)).unwrap_err();
        assert!(m.contains("the caller is not known"), "{caller:?}: {m}");
        let m = clear(&*backend, &reader, &with_caller(Input::new())).unwrap_err();
        assert!(m.contains("the caller is not known"), "{caller:?}: {m}");
        assert!(r.row().is_none() && r.events().is_empty(), "{caller:?}");
    }
}

#[test]
fn the_message_of_the_toml_parser_is_cut_to_one_clean_short_line() {
    use serde::de::Error as _;
    // A parser that quoted the file would put anything in its message: a line break, an escape
    // sequence, a right-to-left override, other scripts and any length.
    let nasty = format!(
        "bad\n\u{1b}[2J \u{202e} caf\u{e9} {} the-very-end",
        "x".repeat(500)
    );
    let e = toml::de::Error::custom(nasty);
    let reason = parse_reason("[record]\nname = \"MOHV\"\n", &e);
    assert!(
        reason.starts_with("the file is not valid TOML: bad"),
        "{reason}"
    );
    assert_eq!(reason.lines().count(), 1, "{reason:?}");
    assert!(
        reason.chars().all(|c| c.is_ascii_graphic() || c == ' '),
        "{reason:?}"
    );
    assert!(reason.len() < 200, "{}", reason.len());
    assert!(!reason.contains("the-very-end"), "cut: {reason}");
}

#[test]
fn get_returns_every_stored_text_exactly_as_it_is_stored() {
    let r = rig();
    // A row written around the command, with texts that trimming, normalising or collapsing white
    // space would change: surrounding and inner double spaces, a decomposed letter, a trailing
    // space on a message.
    let stored = [
        ("name", "  MOHV  Mileage  "),
        ("title_de", "Zu\u{308}rich  Anmelden "),
        ("title_sv", "Logga  in"),
        ("message_en", "Welcome  back "),
        ("slug", "mo-hv"),
    ];
    let mut f = obop_storage::Fields::new();
    for (k, v) in stored {
        f.insert(k.into(), json!(v));
    }
    r.store
        .backend()
        .apply_checked(&[obop_storage::Op::Put {
            model: MODEL.into(),
            id: "instance".into(),
            fields: f,
        }])
        .unwrap();
    let g = r.get();
    for (k, v) in stored {
        assert_eq!(g[k], json!(v), "{k}");
    }
}

/// The module row of `platform` that Customize lists is one of the differences S1a makes on
/// purpose for an administrator: the model joins the models of the module, with its 17 stored and 8
/// answer fields, and the field and data-class counters of the row grow by 25. Pinned here so that
/// a later change to the model's fields is a deliberate act.
#[test]
fn the_platform_module_row_lists_the_model_with_its_twenty_five_fields() {
    let r = rig();
    let row = r
        .call(
            &person(),
            "module.get",
            &Value::map([("id", Value::text("platform"))]),
        )
        .unwrap()
        .value
        .to_json();
    let listed: Vec<&str> = row["models"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| m.as_str())
        .collect();
    assert!(listed.contains(&MODEL), "{row}");
    let entry = row["contents"]["models"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["name"] == MODEL)
        .unwrap_or_else(|| panic!("no entry for the model: {row}"));
    assert_eq!(entry["fields"], 25, "17 stored and 8 answer fields");
    assert_eq!(model().fields.len(), 25);
    assert_eq!(
        (entry["read_group"].as_str(), entry["write_group"].as_str()),
        (Some("*"), Some("platform.admin"))
    );
    let actions: Vec<&str> = entry["actions"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|a| a["name"].as_str())
        .collect();
    assert_eq!(actions, ["configure", "clear"]);
    // The counter of the row is the sum over the models, so it grew by 25 with this one.
    let all: usize = admin_module().models.iter().map(|m| m.fields.len()).sum();
    assert_eq!(
        row["contents"]["fields"].as_u64(),
        Some(all as u64),
        "{row}"
    );
}
