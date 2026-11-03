//! The import map: `bulk_create` with a `source`, and `import_map.get` / `import_map.search`.
//! Every test runs on the memory and SQLite backends, and on PostgreSQL when
//! `OBOP_TEST_POSTGRES_URL` is set.

use crate::import_map::{self, READ_GROUP};
use crate::{CallError, Dispatcher, Principal, Response, Store, Surface, Value, Vault};
use obop_registry::{Access, FieldDef as F, FieldKind, ModelDef, Module, intern};
use obop_types::DataClass;

const U: &str = "im.user";
const ALL: &str = "im.all";

struct Rig {
    d: Dispatcher,
    store: Store,
    p: String,
}

fn m(pairs: Vec<(&str, Value)>) -> Value {
    Value::Map(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

fn t(s: &str) -> Value {
    Value::text(s)
}

fn get<'a>(v: &'a Value, k: &str) -> &'a Value {
    let Value::Map(x) = v else {
        panic!("not a map: {v:?}")
    };
    x.get(k).unwrap_or_else(|| panic!("no {k} in {v:?}"))
}

fn list(v: &Value) -> &Vec<Value> {
    let Value::List(l) = v else {
        panic!("not a list: {v:?}")
    };
    l
}

fn int(v: &Value) -> i64 {
    let Value::Int(n) = v else {
        panic!("not a number: {v:?}")
    };
    *n
}

fn text(v: &Value) -> String {
    let Value::Text(s) = v else {
        panic!("not text: {v:?}")
    };
    s.clone()
}

fn texts(v: &Value) -> Vec<Option<String>> {
    list(v)
        .iter()
        .map(|x| match x {
            Value::Text(s) => Some(s.clone()),
            _ => None,
        })
        .collect()
}

fn source(system: &str, table: &str, ids: &[&str]) -> Value {
    m(vec![
        ("system", t(system)),
        ("table", t(table)),
        ("ids", Value::List(ids.iter().map(|i| t(i)).collect())),
    ])
}

fn rows(names: &[&str]) -> Value {
    Value::List(names.iter().map(|n| m(vec![("name", t(n))])).collect())
}

fn module(p: &str) -> Module {
    let f = |name: &'static str| F::new(name, "d", DataClass::None, false, U);
    let model = |name: &str, fields, access| ModelDef {
        name: intern(&format!("{p}_{name}")),
        description: "d",
        read_group: U,
        write_group: U,
        access,
        fields,
        commands: vec![],
        ext: Default::default(),
    };
    Module {
        id: p.into(),
        name: "Imports".into(),
        models: vec![
            model(
                "party",
                vec![f("name").kind(FieldKind::Text).required()],
                None,
            ),
            // Rows are private to their owner (D58): an importer sees only their own.
            model(
                "note",
                vec![f("name").required(), f("owner")],
                Some(Access {
                    own: Some("owner"),
                    see_all: vec![ALL],
                    ..Access::default()
                }),
            ),
        ],
        ..Module::default()
    }
}

fn each_backend(f: impl Fn(&Rig)) {
    let mut urls = vec!["memory:".to_string(), "sqlite::memory:".to_string()];
    if let Ok(u) = std::env::var("OBOP_TEST_POSTGRES_URL") {
        urls.push(u);
    }
    for url in urls {
        let p = format!("im{}", &uuid::Uuid::now_v7().simple().to_string()[22..]);
        let store = Store::open(&url).unwrap();
        let d = Dispatcher::new();
        let module = module(&p);
        assert!(obop_registry::check_module(&module).is_empty());
        d.register_crud(&module, &store);
        let admin = Module {
            id: format!("{p}_admin"),
            name: "Admin".into(),
            models: vec![import_map::model_def()],
            ..Module::default()
        };
        d.register_import_map(&admin, &store);
        f(&Rig { d, store, p });
    }
}

impl Rig {
    fn model(&self, name: &str) -> String {
        format!("{}_{name}", self.p)
    }

    fn run(&self, who: &Principal, id: &str, input: Value) -> Result<Response, CallError> {
        self.d
            .call(Surface::Rest, who, &mut Vault::new(), id, &input)
    }

    fn ok(&self, who: &Principal, id: &str, input: Value) -> Value {
        match self.run(who, id, input) {
            Ok(r) => r.value,
            Err(e) => panic!("{id} failed: {e:?}"),
        }
    }

    fn err(&self, who: &Principal, id: &str, input: Value) -> String {
        match self.run(who, id, input) {
            Err(CallError::Handler(m)) => m,
            other => panic!("{id}: expected a handler error, got {other:?}"),
        }
    }

    /// `bulk_create` of `model` with a source.
    fn import(&self, who: &Principal, model: &str, mut input: Vec<(&str, Value)>) -> Value {
        input.sort_by_key(|(k, _)| *k);
        self.ok(who, &format!("{}.bulk_create", self.model(model)), m(input))
    }

    fn count(&self, model: &str) -> usize {
        self.store.backend().list(&self.model(model)).unwrap().len()
    }

    fn search(&self, who: &Principal, model: Option<&str>, table: &str, id: &str) -> Value {
        let mut i = vec![
            ("source_system", t("legacy")),
            ("source_table", t(table)),
            ("source_id", t(id)),
        ];
        if let Some(model) = model {
            i.push(("model", t(&self.model(model))));
        }
        self.ok(who, "import_map.search", m(i))
    }

    fn keys_of(&self, who: &Principal, model: &str, id: &str) -> Vec<Value> {
        list(&self.ok(
            who,
            "import_map.get",
            m(vec![("model", t(&self.model(model))), ("id", t(id))]),
        ))
        .clone()
    }
}

fn importer() -> Principal {
    Principal::user("imp", &[U])
}

fn reader() -> Principal {
    Principal::user("rdr", &[U, READ_GROUP])
}

#[test]
fn a_source_key_is_recorded_found_both_ways_and_survives_an_update() {
    each_backend(|r| {
        let imp = importer();
        let a = r.import(
            &imp,
            "party",
            vec![
                ("rows", rows(&["Ada", "Bob", "Cy"])),
                (
                    "source",
                    source("legacy", "customers", &["A-1", "B-2", "C-3"]),
                ),
            ],
        );
        assert_eq!(int(get(&a, "created")), 3, "{a:?}");
        assert_eq!(int(get(&a, "already_imported")), 0);
        let ids = texts(get(&a, "ids"));

        // A source key to its record, with and without the model; the table is part of the key.
        for who in [reader(), Principal::user("root", &["platform.admin"])] {
            let found = r.search(&who, Some("party"), "customers", "B-2");
            assert_eq!(int(get(&found, "total")), 1);
            let row = &list(get(&found, "rows"))[0];
            assert_eq!(Some(text(get(row, "record"))), ids[1]);
            assert_eq!(text(get(row, "model")), r.model("party"));
            assert_eq!(text(get(row, "source_system")), "legacy");
            assert_eq!(text(get(row, "imported_by")), "imp");
            assert!(text(get(row, "imported_at")).ends_with('Z'));
            let found = r.search(&who, None, "customers", "B-2");
            assert_eq!(int(get(&found, "total")), 1);
            assert_eq!(
                int(get(&r.search(&who, None, "suppliers", "B-2"), "total")),
                0
            );
        }

        // A record to its keys.
        let keys = r.keys_of(&reader(), "party", ids[0].as_ref().unwrap());
        assert_eq!(keys.len(), 1);
        assert_eq!(text(get(&keys[0], "source_id")), "A-1");

        // An update of the record changes nothing in the map.
        r.ok(
            &imp,
            &format!("{}.update", r.model("party")),
            m(vec![
                ("id", t(ids[0].as_ref().unwrap())),
                ("name", t("Ada L.")),
            ]),
        );
        let keys = r.keys_of(&reader(), "party", ids[0].as_ref().unwrap());
        assert_eq!(keys.len(), 1);
        assert_eq!(text(get(&keys[0], "source_table")), "customers");

        // A record with no key, and a key that is not there, give nothing.
        let plain = r.ok(
            &imp,
            &format!("{}.create", r.model("party")),
            m(vec![("name", t("Plain"))]),
        );
        let plain = text(get(&plain, "id"));
        assert!(r.keys_of(&reader(), "party", &plain).is_empty());
        assert_eq!(
            int(get(
                &r.search(&reader(), None, "customers", "nope"),
                "total"
            )),
            0
        );
    });
}

#[test]
fn importing_the_same_key_again_reports_already_imported_and_creates_nothing() {
    each_backend(|r| {
        let imp = importer();
        let call = |names: &[&str], ids: &[&str], extra: Vec<(&str, Value)>| {
            let mut i = vec![
                ("rows", rows(names)),
                ("source", source("legacy", "customers", ids)),
            ];
            i.extend(extra);
            r.import(&imp, "party", i)
        };
        let first = call(&["Ada", "Bob"], &["A-1", "B-2"], vec![]);
        let ids = texts(get(&first, "ids"));
        assert_eq!(r.count("party"), 2);

        // The whole batch again, and a batch with one new row among two known ones.
        let again = call(&["Ada", "Bob"], &["A-1", "B-2"], vec![]);
        assert_eq!(
            (
                int(get(&again, "created")),
                int(get(&again, "already_imported"))
            ),
            (0, 2),
            "{again:?}"
        );
        assert_eq!(get(&again, "ok"), &Value::Bool(true));
        assert_eq!(texts(get(&again, "ids")), ids);
        let listed = list(get(&again, "already_imported_rows"));
        assert_eq!(text(get(&listed[1], "source_id")), "B-2");
        assert_eq!(Some(text(get(&listed[1], "id"))), ids[1]);
        let mixed = call(&["Ada", "Cy", "Bob"], &["A-1", "C-3", "B-2"], vec![]);
        assert_eq!(
            (
                int(get(&mixed, "created")),
                int(get(&mixed, "already_imported"))
            ),
            (1, 2)
        );
        assert_eq!(r.count("party"), 3);
        assert_eq!(texts(get(&mixed, "ids"))[0], ids[0]);

        // A dry run says the same and writes nothing.
        let dry = call(
            &["Dee", "Ada"],
            &["D-4", "A-1"],
            vec![("dry_run", Value::Bool(true))],
        );
        assert_eq!(
            (int(get(&dry, "valid")), int(get(&dry, "already_imported"))),
            (1, 1)
        );
        assert_eq!(r.count("party"), 3);
        assert_eq!(
            int(get(&r.search(&reader(), None, "customers", "D-4"), "total")),
            0
        );

        // `on_existing: error` makes it a bad row with the code, in both modes.
        let e = r.err(
            &imp,
            &format!("{}.bulk_create", r.model("party")),
            m(vec![
                ("rows", rows(&["Ada"])),
                ("source", source("legacy", "customers", &["A-1"])),
                ("on_existing", t("error")),
            ]),
        );
        assert!(e.contains("already_imported") && e.contains("A-1"), "{e}");
        let part = call(
            &["Ada", "Eve"],
            &["A-1", "E-5"],
            vec![("mode", t("partial")), ("on_existing", t("error"))],
        );
        assert_eq!(int(get(&part, "created")), 1);
        let bad = &list(get(&part, "errors"))[0];
        assert_eq!(text(get(bad, "code")), "already_imported");
        assert_eq!(int(get(bad, "row")), 1);
        assert!(text(get(bad, "error")).contains(ids[0].as_ref().unwrap()));
        assert_eq!(r.count("party"), 4);

        // A key twice in one call is refused with its own code.
        let e = r.err(
            &imp,
            &format!("{}.bulk_create", r.model("party")),
            m(vec![
                ("rows", rows(&["X", "Y"])),
                ("source", source("legacy", "customers", &["X-1", "X-1"])),
            ]),
        );
        assert!(
            e.contains("duplicate_source_key") && e.contains("row 1"),
            "{e}"
        );
        assert_eq!(r.count("party"), 4);
    });
}

#[test]
fn a_deleted_record_no_longer_holds_its_key() {
    each_backend(|r| {
        let imp = importer();
        let go = || {
            r.import(
                &imp,
                "party",
                vec![
                    ("rows", rows(&["Ada"])),
                    ("source", source("legacy", "customers", &["A-1"])),
                ],
            )
        };
        let first = texts(get(&go(), "ids"))[0].clone().unwrap();
        r.ok(
            &imp,
            &format!("{}.delete", r.model("party")),
            m(vec![("id", t(&first))]),
        );
        // The entry is still stored but its record is gone: it is not shown, and the key is free.
        assert_eq!(
            int(get(&r.search(&reader(), None, "customers", "A-1"), "total")),
            0
        );
        let second = go();
        assert_eq!(int(get(&second, "created")), 1, "{second:?}");
        let new_id = texts(get(&second, "ids"))[0].clone().unwrap();
        assert_ne!(new_id, first);
        let found = r.search(&reader(), None, "customers", "A-1");
        assert_eq!(text(get(&list(get(&found, "rows"))[0], "record")), new_id);
        assert!(r.keys_of(&reader(), "party", &first).is_empty());
    });
}

#[test]
fn a_caller_never_learns_of_a_record_they_cannot_read() {
    each_backend(|r| {
        let imp = importer();
        let other = Principal::user("other", &[U]);
        let a = r.import(
            &imp,
            "note",
            vec![
                ("rows", rows(&["Private"])),
                ("source", source("legacy", "notes", &["N-1"])),
            ],
        );
        let id = texts(get(&a, "ids"))[0].clone().unwrap();

        // Another person with the group, and a reader whose group does not cover the model.
        let rdr = reader();
        let no_model_group = Principal::user("nm", &[READ_GROUP]);
        for who in [&rdr, &no_model_group] {
            assert_eq!(
                int(get(&r.search(who, Some("note"), "notes", "N-1"), "total")),
                0
            );
            assert_eq!(int(get(&r.search(who, None, "notes", "N-1"), "total")), 0);
            assert!(r.keys_of(who, "note", &id).is_empty());
        }
        // The owner's own record, a see-all holder and the administrator may.
        for who in [
            Principal::user("imp", &[U, READ_GROUP]),
            Principal::user("boss", &[U, ALL, READ_GROUP]),
            Principal::user("root", &["platform.admin"]),
        ] {
            assert_eq!(
                int(get(&r.search(&who, None, "notes", "N-1"), "total")),
                1,
                "{}",
                who.id
            );
            assert_eq!(r.keys_of(&who, "note", &id).len(), 1);
        }
        // Without the group of the map nothing is reachable at all.
        for c in ["import_map.get", "import_map.search"] {
            assert!(matches!(
                r.run(&importer(), c, m(vec![])),
                Err(CallError::Forbidden(_))
            ));
        }
        // Importing the same key as somebody else: a conflict, but not the record's id.
        let again = r.import(
            &other,
            "note",
            vec![
                ("rows", rows(&["Mine"])),
                ("source", source("legacy", "notes", &["N-1"])),
                ("mode", t("partial")),
            ],
        );
        assert_eq!(int(get(&again, "created")), 0);
        assert_eq!(int(get(&again, "failed")), 1);
        let bad = &list(get(&again, "errors"))[0];
        assert_eq!(text(get(bad, "code")), "already_imported");
        assert!(!text(get(bad, "error")).contains(&id), "{bad:?}");
        assert_eq!(r.count("note"), 1);
    });
}

#[test]
fn a_call_without_a_source_answers_as_it_always_did() {
    each_backend(|r| {
        let a = r.import(&importer(), "party", vec![("rows", rows(&["Ada"]))]);
        let Value::Map(x) = &a else { panic!() };
        assert!(!x.contains_key("already_imported") && !x.contains_key("already_imported_rows"));
        assert_eq!(int(get(&a, "created")), 1);
        // No entry was written.
        assert!(
            r.store
                .backend()
                .list(import_map::MODEL)
                .unwrap()
                .is_empty()
        );
    });
}

#[test]
fn bad_source_input_is_refused_before_anything_is_written() {
    each_backend(|r| {
        let imp = importer();
        let id = format!("{}.bulk_create", r.model("party"));
        let cases: Vec<(Vec<(&str, Value)>, &str)> = vec![
            (
                vec![
                    ("rows", rows(&["A", "B"])),
                    ("source", source("s", "t", &["1"])),
                ],
                "has 1 entries but the call has 2 rows",
            ),
            (
                vec![("rows", rows(&["A"])), ("on_existing", t("skip"))],
                "on_existing needs a source",
            ),
            (
                vec![
                    ("rows", rows(&["A"])),
                    ("source", source("s", "t", &["1"])),
                    ("on_existing", t("replace")),
                ],
                "on_existing must be skip or error",
            ),
            (
                vec![
                    ("rows", rows(&["A"])),
                    (
                        "source",
                        m(vec![("system", t("s")), ("ids", Value::List(vec![t("1")]))]),
                    ),
                ],
                "source.table is required",
            ),
            (
                vec![("rows", rows(&["A"])), ("source", source("", "t", &["1"]))],
                "source.system must be",
            ),
        ];
        for (input, want) in cases {
            let e = r.err(&imp, &id, m(input));
            assert!(e.contains(want), "{e}");
        }
        assert_eq!(r.count("party"), 0);
    });
}

#[test]
fn an_idempotency_key_binds_the_source_too() {
    each_backend(|r| {
        let imp = importer();
        let id = format!("{}.bulk_create", r.model("party"));
        let call = |ids: &[&str]| {
            m(vec![
                ("rows", rows(&["Ada"])),
                ("idempotency_key", t("run-1")),
                ("source", source("legacy", "customers", ids)),
            ])
        };
        let first = r.ok(&imp, &id, call(&["A-1"]));
        let replay = r.ok(&imp, &id, call(&["A-1"]));
        assert_eq!(get(&replay, "replayed"), &Value::Bool(true));
        assert_eq!(get(&first, "ids"), get(&replay, "ids"));
        let e = r.err(&imp, &id, call(&["A-9"]));
        assert!(e.contains("different request"), "{e}");
    });
}

#[test]
fn rows_may_come_from_several_tables_and_keys_are_numbers_or_text() {
    each_backend(|r| {
        let imp = importer();
        let a = r.import(
            &imp,
            "party",
            vec![
                ("rows", rows(&["Ada", "Bob", "Cy"])),
                (
                    "source",
                    m(vec![
                        ("system", t("legacy")),
                        ("table", t("customers")),
                        ("ids", Value::List(vec![Value::Int(7), t("7"), Value::Null])),
                        (
                            "tables",
                            Value::List(vec![Value::Null, t("suppliers"), Value::Null]),
                        ),
                    ]),
                ),
            ],
        );
        assert_eq!(int(get(&a, "created")), 3, "{a:?}");
        // Number 7 in `customers` and text "7" in `suppliers` are two keys; the third has none.
        assert_eq!(
            int(get(&r.search(&reader(), None, "customers", "7"), "total")),
            1
        );
        assert_eq!(
            int(get(&r.search(&reader(), None, "suppliers", "7"), "total")),
            1
        );
        assert_eq!(r.store.backend().list(import_map::MODEL).unwrap().len(), 2);
    });
}
