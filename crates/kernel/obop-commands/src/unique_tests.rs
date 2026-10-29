//! The uniqueness check (D57) after the keyed lookup (ask 11.4): the same answers as before, the
//! same messages, and a cost that does not grow with the model.
//!
//! * `the_check_gives_the_answers_of_the_rule` runs random sequences of creates, updates, deletes
//!   and bulk creates against a plain model of the rule written out again in the test, on every
//!   backend, and compares every outcome and the stored state. It runs on two models: one whose
//!   fields are all text, and one whose first fields are a text and a whole number and whose
//!   scopes are a boolean, a whole number and a text. It uses only the public surface, so it also
//!   runs against the code before the change: that is how "no change of results" was checked.
//! * `the_messages_are_exactly_these` pins the text of the refusals.
//! * `the_check_reads_the_same_few_rows_in_a_model_of_any_size` counts what the check reads
//!   (through a store that tallies its reads), which statement the backend ran for it, and on
//!   SQLite the steps of its virtual machine: not time. A check that went back to `find` or to a
//!   scan fails it.
//! * the race tests: two commands that insert one key at once give one success and one refusal
//!   that says "must be unique", never a storage fault.

use crate::unique_bench::{Bench, U, created, f, m, t};
use crate::{CallError, Dispatcher, Principal, Response, Store, Surface, Value, Vault};
use obop_registry::{FieldDef as F, ModelDef, Module, intern};
use obop_storage::testing::Flaky;
use std::sync::Arc;

// ---- a model with three rules --------------------------------------------------------------

/// `item`: `code` unique; `slot` unique within `scope`; `tag` unique within `scope` and `region`.
fn item_module(p: &str) -> Module {
    let fields: Vec<F> = vec![
        f("code").unique(),
        f("slot").unique_with(&["scope"]),
        f("tag").unique_with(&["scope", "region"]),
        f("scope"),
        f("region"),
    ];
    let model = ModelDef {
        name: intern(&format!("{p}_item")),
        description: "d",
        read_group: U,
        write_group: U,
        access: None,
        fields,
        commands: vec![],
        ext: Default::default(),
    };
    Module {
        id: p.into(),
        name: "Items".into(),
        models: vec![model],
        ..Module::default()
    }
}

/// `typed`: `label` (a text) unique within `flag` (a boolean) and `qty` (a whole number); `num` (a
/// whole number) unique within `flag` and `grp` (a text).
fn typed_module(p: &str) -> Module {
    use obop_registry::FieldKind;
    let fields: Vec<F> = vec![
        f("label").unique_with(&["flag", "qty"]),
        f("num")
            .kind(FieldKind::Number)
            .unique_with(&["flag", "grp"]),
        f("flag").kind(FieldKind::Bool),
        f("qty").kind(FieldKind::Number),
        f("grp"),
    ];
    let model = ModelDef {
        name: intern(&format!("{p}_typed")),
        description: "d",
        read_group: U,
        write_group: U,
        access: None,
        fields,
        commands: vec![],
        ext: Default::default(),
    };
    Module {
        id: p.into(),
        name: "Typed".into(),
        models: vec![model],
        ..Module::default()
    }
}

/// Where the values of a field come from in the random tests.
enum Pool {
    Text(&'static [&'static str]),
    Num(&'static [i64]),
    Bool,
}

/// A model of the random tests: its fields in the order of a `Rec`, the pools of their values, and
/// its rules as (first field, scope fields) by position.
struct Shape {
    module: fn(&str) -> Module,
    suffix: &'static str,
    names: [&'static str; 5],
    rules: &'static [(usize, &'static [usize])],
    pools: [Pool; 5],
}

static TEXT: Shape = Shape {
    module: item_module,
    suffix: "item",
    names: ["code", "slot", "tag", "scope", "region"],
    rules: &[(0, &[]), (1, &[3]), (2, &[3, 4])],
    pools: [
        Pool::Text(&["A", "B", "C"]),
        Pool::Text(&["P", "Q", "R"]),
        Pool::Text(&["T", "U"]),
        Pool::Text(&["s1", "s2"]),
        Pool::Text(&["r1", "r2"]),
    ],
};

static TYPED: Shape = Shape {
    module: typed_module,
    suffix: "typed",
    names: ["label", "num", "flag", "qty", "grp"],
    rules: &[(0, &[2, 3]), (1, &[2, 4])],
    pools: [
        Pool::Text(&["A", "B", "C"]),
        Pool::Num(&[1, 2, 3]),
        Pool::Bool,
        Pool::Num(&[10, 20]),
        Pool::Text(&["g1", "g2"]),
    ],
};

struct Items {
    d: Arc<Dispatcher>,
    store: Store,
    model: String,
    shape: &'static Shape,
}

impl Items {
    fn open(url: &str) -> Items {
        Self::over(Store::open(url).unwrap())
    }

    fn over(store: Store) -> Items {
        Self::shaped(store, &TEXT)
    }

    fn shaped(store: Store, shape: &'static Shape) -> Items {
        let p = format!("it{}", &uuid::Uuid::now_v7().simple().to_string()[22..]);
        let d = Arc::new(Dispatcher::new());
        let module = (shape.module)(&p);
        assert!(obop_registry::check_module(&module).is_empty());
        d.register_crud(&module, &store);
        Items {
            d,
            store,
            model: format!("{p}_{}", shape.suffix),
            shape,
        }
    }

    fn call(&self, c: &str, input: Value) -> Result<Response, CallError> {
        let who = Principal::user(&format!("u{}", uuid::Uuid::now_v7().simple()), &[U]);
        self.d.call(
            Surface::Rest,
            &who,
            &mut Vault::new(),
            &format!("{}.{c}", self.model),
            &input,
        )
    }

    fn refusal(&self, c: &str, input: Value) -> Option<String> {
        match self.call(c, input) {
            Ok(_) => None,
            Err(CallError::Handler(m)) => Some(m),
            Err(other) => panic!("{c}: not a refusal of the rule: {other:?}"),
        }
    }
}

/// The value of one field of the model: not there, null, the empty text, a text, a whole number
/// or a boolean.
#[derive(Clone, Debug, PartialEq)]
enum Val {
    Missing,
    Null,
    Empty,
    S(String),
    N(i64),
    B(bool),
}

impl Val {
    /// A value that a rule holds to (as its first field): not nothing and not the empty text.
    fn constrained(&self) -> bool {
        matches!(self, Val::S(_) | Val::N(_))
    }
    /// The value as a scope: nothing and the empty text are the same.
    fn scope(&self) -> Val {
        match self {
            Val::Missing | Val::Null => Val::Empty,
            v => v.clone(),
        }
    }
    fn input(&self) -> Option<Value> {
        match self {
            Val::Missing => None,
            Val::Null => Some(Value::Null),
            Val::Empty => Some(t("")),
            Val::S(s) => Some(t(s)),
            Val::N(n) => Some(Value::Int(*n)),
            Val::B(b) => Some(Value::Bool(*b)),
        }
    }
    fn of_stored(v: Option<&serde_json::Value>) -> Val {
        match v {
            None => Val::Missing,
            Some(serde_json::Value::Null) => Val::Null,
            Some(serde_json::Value::String(s)) if s.is_empty() => Val::Empty,
            Some(serde_json::Value::String(s)) => Val::S(s.clone()),
            Some(serde_json::Value::Number(n)) => Val::N(n.as_i64().expect("a whole number")),
            Some(serde_json::Value::Bool(b)) => Val::B(*b),
            Some(other) => panic!("{other:?}"),
        }
    }
}

/// The five fields of a shape, in the order of its `names`.
type Rec = [Val; 5];

/// The rule of D57 written out again: the first field of a rule that holds a value, and the same
/// first field and scope in another record. (A missing or null scope is the empty one.)
fn conflict(shape: &Shape, others: &[&Rec], r: &Rec) -> Option<&'static str> {
    for (first, scope) in shape.rules {
        if !r[*first].constrained() {
            continue;
        }
        if others
            .iter()
            .any(|o| o[*first] == r[*first] && scope.iter().all(|s| o[*s].scope() == r[*s].scope()))
        {
            return Some(shape.names[*first]);
        }
    }
    None
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn pick(&mut self, words: &[&str]) -> Val {
        // Nothing, null and the empty text are as likely as one of the words together.
        match self.below(3 + words.len() * 2) {
            0 => Val::Missing,
            1 => Val::Null,
            2 => Val::Empty,
            n => Val::S(words[(n - 3) % words.len()].to_string()),
        }
    }
    /// A value for a column: nothing, null, or one of the pool (the empty text only for a text).
    fn pick_col(&mut self, pool: &Pool) -> Val {
        match pool {
            Pool::Text(words) => self.pick(words),
            Pool::Num(ns) => match self.below(2 + ns.len() * 2) {
                0 => Val::Missing,
                1 => Val::Null,
                n => Val::N(ns[(n - 2) % ns.len()]),
            },
            Pool::Bool => match self.below(6) {
                0 => Val::Missing,
                1 => Val::Null,
                n => Val::B(n % 2 == 0),
            },
        }
    }
    fn record(&mut self, shape: &Shape) -> Rec {
        let mut r: Rec = std::array::from_fn(|_| Val::Missing);
        for (k, pool) in shape.pools.iter().enumerate() {
            r[k] = self.pick_col(pool);
        }
        r
    }
}

fn input_of(shape: &Shape, r: &Rec) -> Value {
    m(shape
        .names
        .iter()
        .zip(r)
        .filter_map(|(n, v)| v.input().map(|i| (*n, i)))
        .collect())
}

fn ids_of(out: &Value) -> Vec<Option<String>> {
    let Value::Map(o) = out else { panic!() };
    let Some(Value::List(l)) = o.get("ids") else {
        panic!("no ids: {out:?}")
    };
    l.iter()
        .map(|v| match v {
            Value::Text(s) => Some(s.clone()),
            _ => None,
        })
        .collect()
}

fn stored(it: &Items) -> Vec<(String, Rec)> {
    let mut all: Vec<(String, Rec)> = it
        .store
        .backend
        .list(&it.model)
        .unwrap()
        .into_iter()
        .map(|(id, f)| {
            let r: Rec = std::array::from_fn(|i| Val::of_stored(f.get(it.shape.names[i])));
            (id, r)
        })
        .collect();
    all.sort_by(|a, b| a.0.cmp(&b.0));
    all
}

/// The refusal names the rule of the reference: `model.field must be unique`.
fn names(it: &Items, refusal: &str, field: &str) -> bool {
    refusal.starts_with(&format!("{}.{field} must be unique: ", it.model))
}

fn run(it: &Items, seed: u64, steps: usize) {
    let shape = it.shape;
    let mut rng = Rng(seed | 1);
    // The reference: the records the engine should hold, by id.
    let mut world: Vec<(String, Rec)> = Vec::new();
    for step in 0..steps {
        let at = format!("seed {seed} step {step} on {}", it.store.backend.kind());
        match rng.below(10) {
            // create
            0..=3 => {
                let r = rng.record(shape);
                let others: Vec<&Rec> = world.iter().map(|(_, r)| r).collect();
                let want = conflict(shape, &others, &r);
                match (it.call("create", input_of(shape, &r)), want) {
                    (Ok(out), None) => {
                        let Value::Map(o) = &out.value else { panic!() };
                        let Some(Value::Text(id)) = o.get("id") else {
                            panic!()
                        };
                        world.push((id.clone(), r));
                    }
                    (Err(CallError::Handler(e)), Some(field)) => {
                        assert!(names(it, &e, field), "{at}: {e}");
                    }
                    (got, want) => {
                        panic!("{at}: create {r:?} gave {got:?}, the rule says {want:?}")
                    }
                }
            }
            // update
            4..=6 if !world.is_empty() => {
                let i = rng.below(world.len());
                let (id, cur) = world[i].clone();
                let mut next = cur.clone();
                let mut patch = vec![("id", t(&id))];
                for (k, name) in shape.names.iter().enumerate() {
                    if rng.below(3) == 0 {
                        let v = rng.pick_col(&shape.pools[k]);
                        // A field that is not sent stays as it is; one that is sent is set.
                        if let Some(input) = v.input() {
                            patch.push((name, input));
                            next[k] = v;
                        }
                    }
                }
                let others: Vec<&Rec> = world
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| *j != i)
                    .map(|(_, (_, r))| r)
                    .collect();
                let want = if next == cur {
                    None
                } else {
                    conflict(shape, &others, &next)
                };
                match (it.call("update", m(patch)), want) {
                    (Ok(_), None) => world[i].1 = next,
                    (Err(CallError::Handler(e)), Some(field)) => {
                        assert!(names(it, &e, field), "{at}: {e}");
                    }
                    (got, want) => panic!("{at}: update gave {got:?}, the rule says {want:?}"),
                }
            }
            // delete
            7 if !world.is_empty() => {
                let i = rng.below(world.len());
                let (id, _) = world.remove(i);
                it.call("delete", m(vec![("id", t(&id))])).unwrap();
            }
            // bulk create
            _ => {
                let rows: Vec<Rec> = (0..1 + rng.below(6)).map(|_| rng.record(shape)).collect();
                let partial = rng.below(2) == 0;
                // The rows are checked in order against what is stored and what the earlier good
                // rows of the call added.
                let mut added: Vec<Rec> = Vec::new();
                let mut failed: Vec<(usize, &'static str)> = Vec::new();
                for (n, r) in rows.iter().enumerate() {
                    let mut others: Vec<&Rec> = world.iter().map(|(_, r)| r).collect();
                    others.extend(added.iter());
                    match conflict(shape, &others, r) {
                        None => added.push(r.clone()),
                        Some(field) => failed.push((n + 1, field)),
                    }
                }
                let mut input = vec![(
                    "rows",
                    Value::List(rows.iter().map(|r| input_of(shape, r)).collect::<Vec<_>>()),
                )];
                if partial {
                    input.push(("mode", t("partial")));
                }
                match it.call("bulk_create", m(input)) {
                    Ok(out) => {
                        let Value::Map(o) = &out.value else { panic!() };
                        let ids = ids_of(&out.value);
                        let errors = match o.get("errors") {
                            Some(Value::List(l)) => l.clone(),
                            other => panic!("{at}: {other:?}"),
                        };
                        assert!(partial || failed.is_empty(), "{at}: all or nothing wrote");
                        assert_eq!(errors.len(), failed.len(), "{at}: {errors:?} / {failed:?}");
                        assert_eq!(created(&out.value), added.len(), "{at}");
                        for (e, (row, field)) in errors.iter().zip(&failed) {
                            let Value::Map(e) = e else { panic!() };
                            assert_eq!(e["row"], Value::Int(*row as i64), "{at}");
                            let Value::Text(msg) = &e["error"] else {
                                panic!()
                            };
                            assert!(names(it, msg, field), "{at}: row {row}: {msg}");
                        }
                        for (n, r) in rows.iter().enumerate() {
                            if let Some(id) = &ids[n] {
                                assert!(!failed.iter().any(|(row, _)| *row == n + 1), "{at}");
                                world.push((id.clone(), r.clone()));
                            }
                        }
                    }
                    Err(CallError::Handler(e)) => {
                        assert!(!partial, "{at}: partial refused: {e}");
                        assert!(!failed.is_empty(), "{at}: refused: {e}");
                        assert!(e.contains("must be unique"), "{at}: {e}");
                        assert!(
                            e.contains(&format!("row {}: ", failed[0].0)),
                            "{at}: the first bad row is {}: {e}",
                            failed[0].0
                        );
                    }
                    Err(other) => panic!("{at}: {other:?}"),
                }
            }
        }
        // The stored state is the reference's, and no two records break a rule.
        let mut want: Vec<(String, Rec)> = world.clone();
        want.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(stored(it), want, "{at}");
    }
    for (i, (_, r)) in world.iter().enumerate() {
        let others: Vec<&Rec> = world
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, (_, r))| r)
            .collect();
        assert_eq!(
            conflict(shape, &others, r),
            None,
            "the reference ended in a conflict"
        );
    }
}

fn urls() -> Vec<String> {
    let mut urls = vec!["memory:".to_string(), "sqlite::memory:".to_string()];
    if let Ok(u) = std::env::var("OBOP_TEST_POSTGRES_URL") {
        urls.push(u);
    }
    urls
}

#[test]
fn the_check_gives_the_answers_of_the_rule() {
    for url in urls() {
        // A scratch database for PostgreSQL, so the records of other tests are not in the way.
        let scratch = url
            .starts_with("postgres")
            .then(|| obop_storage::testing::TestDb::create(&url).unwrap());
        let url = scratch.as_ref().map_or(url, |db| db.url().to_string());
        let steps = if url.starts_with("postgres") {
            120
        } else {
            400
        };
        for seed in [1, 2, 3, 4, 5, 6] {
            run(&Items::open(&url), seed * 7919, steps);
            // The same with a whole number and a boolean among the fields and the scopes.
            let typed = Items::shaped(Store::open(&url).unwrap(), &TYPED);
            run(&typed, seed * 104_729, steps);
        }
    }
}

// ---- the messages --------------------------------------------------------------------------

#[test]
fn the_messages_are_exactly_these() {
    for url in urls() {
        let scratch = url
            .starts_with("postgres")
            .then(|| obop_storage::testing::TestDb::create(&url).unwrap());
        let url = scratch.as_ref().map_or(url, |db| db.url().to_string());
        let it = Items::open(&url);
        let md = &it.model;
        let row = |pairs: Vec<(&str, &str)>| m(pairs.into_iter().map(|(k, v)| (k, t(v))).collect());
        it.call(
            "create",
            row(vec![("code", "A-1"), ("slot", "S1"), ("scope", "x")]),
        )
        .unwrap();
        // One field.
        assert_eq!(
            it.refusal("create", row(vec![("code", "A-1")])).unwrap(),
            format!("{md}.code must be unique: \"A-1\" is already used by another record")
        );
        // One scope field.
        assert_eq!(
            it.refusal("create", row(vec![("slot", "S1"), ("scope", "x")]))
                .unwrap(),
            format!(
                "{md}.slot must be unique: \"S1\" is already used by another record with the same scope"
            )
        );
        // Two scope fields.
        it.call(
            "create",
            row(vec![("tag", "T1"), ("scope", "x"), ("region", "r")]),
        )
        .unwrap();
        assert_eq!(
            it.refusal(
                "create",
                row(vec![("tag", "T1"), ("scope", "x"), ("region", "r")])
            )
            .unwrap(),
            format!(
                "{md}.tag must be unique: \"T1\" is already used by another record with the same scope and region"
            )
        );
        // The same values in another scope are fine.
        it.call("create", row(vec![("slot", "S1"), ("scope", "y")]))
            .unwrap();
        // An update to a taken value, and a bulk row, say the same.
        let other = it.call("create", row(vec![("code", "A-2")])).unwrap().value;
        let Value::Map(o) = other else { panic!() };
        let Some(Value::Text(id)) = o.get("id") else {
            panic!()
        };
        assert_eq!(
            it.refusal("update", row(vec![("id", id), ("code", "A-1")]))
                .unwrap(),
            format!("{md}.code must be unique: \"A-1\" is already used by another record")
        );
        let bulk = it
            .refusal(
                "bulk_create",
                m(vec![(
                    "rows",
                    Value::List(vec![row(vec![("code", "N-1")]), row(vec![("code", "N-1")])]),
                )]),
            )
            .unwrap();
        assert_eq!(
            bulk,
            format!(
                "1 of 2 row(s) are invalid, so nothing was created. row 2: {md}.code must be unique: \"N-1\" is already used by another record"
            )
        );
        // The same through the other model of the measurements (a text in quotes; a number and a
        // decimal are shown in `values_are_compared_in_their_canonical_form_and_by_their_kind`).
        let p = Bench::over(Store::open(&url).unwrap());
        p.call(&Bench::user(), "create", m(crate::unique_bench::row(1)))
            .unwrap();
        let e = p
            .call(&Bench::user(), "create", m(crate::unique_bench::row(1)))
            .unwrap_err();
        let CallError::Handler(e) = e else { panic!() };
        assert_eq!(
            e,
            format!(
                "{}.code must be unique: \"R0000001\" is already used by another record",
                p.model()
            )
        );
    }
}

// ---- what the check reads ------------------------------------------------------------------

/// Put `n` reaction rows straight into the store (the check is not what is being measured here,
/// and the old code would take minutes to check 20,000 rows).
fn load(b: &Bench, n: usize) {
    let model = b.model();
    for block in (0..n).step_by(5_000) {
        let rows: Vec<obop_storage::Row> = (block..(block + 5_000).min(n))
            .map(|i| {
                let fields = crate::unique_bench::row(i)
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v.to_json()))
                    .collect();
                (model.clone(), format!("seed{i:07}"), fields)
            })
            .collect();
        b.store.backend.insert_many(&rows).unwrap();
    }
}

/// What a fixed piece of work (20 creates, a bulk create of 50 rows and a refused create) reads in
/// a model of `n` rows in the database at `url`, and which statements the backend ran for it.
fn work_in(
    url: &str,
    n: usize,
) -> (
    obop_storage::testing::Reads,
    obop_storage::testing::UniqueRuns,
) {
    let (flaky, store) = tallied_over(url);
    let b = Bench::over(store);
    load(&b, n);
    let who = Bench::user();
    flaky.reset_reads();
    obop_storage::testing::reset_unique_runs();
    for i in 0..20 {
        b.create_one(&who, n + i);
    }
    let rows = Value::List(
        (n + 100..n + 150)
            .map(|i| m(crate::unique_bench::row(i)))
            .collect(),
    );
    let out = b
        .call(&Bench::user(), "bulk_create", m(vec![("rows", rows)]))
        .unwrap();
    assert_eq!(created(&out.value), 50);
    // A refused create: the code of a stored row.
    let dup = b.call(&who, "create", m(crate::unique_bench::row(n / 2)));
    let Err(CallError::Handler(e)) = dup else {
        panic!("{dup:?}")
    };
    assert!(e.contains("must be unique"), "{e}");
    (flaky.reads(), obop_storage::testing::unique_runs())
}

/// The check asks the store for its keys and for nothing else, whatever the size of the model, and
/// the backend answers them with the statement built from the index.
fn the_check_costs_the_same_in_a_model_of_any_size(small_url: &str, large_url: &str, sqlite: bool) {
    let (small, small_runs) = work_in(small_url, 2_000);
    let (large, large_runs) = work_in(large_url, 20_000);
    eprintln!("2,000 rows: {small:?}, {small_runs:?}");
    eprintln!("20,000 rows: {large:?}, {large_runs:?}");
    // Every created row asked once for each of the two rules; the refused one only about the
    // first, which it broke.
    let asked = 2 * (20 + 50) + 1;
    for (r, runs) in [(&small, &small_runs), (&large, &large_runs)] {
        // No read of the whole model, and no lookup on one field (a scan, on both databases).
        // (The one call of `list` is the sweep of expired idempotency answers after a bulk
        // create, and the model it lists is empty.)
        assert_eq!((r.list_rows, r.scan_rows, r.finds), (0, 0, 0), "{r:?}");
        assert_eq!(r.scans, 0, "{r:?}");
        assert_eq!(r.keyed, asked, "{r:?}");
        // And the answers held at most the one twin.
        assert!(r.keyed_rows <= 2, "{r:?}");
        // The backend ran its keyed statement for every one of them: it did not go the way of
        // `find`, which would give the same answers at the cost of the model.
        assert_eq!((runs.keyed, runs.fallback), (asked, 0), "{runs:?}");
    }
    // The same work reads the same, whatever the size of the model.
    assert_eq!(small, large);
    if sqlite {
        // The work SQLite did for the statements is a handful of steps each, and no more for
        // 20,000 rows than for 2,000. (For the old check it was in proportion to the model:
        // 120,026 steps for one statement over 20,000 rows, see `obop-storage`'s
        // `the_work_sqlite_does_for_the_keyed_statement_does_not_grow_with_the_model`.)
        assert!(small_runs.vm_steps > 0, "{small_runs:?}");
        assert!(small_runs.vm_steps < 100 * asked, "{small_runs:?}");
        assert!(
            large_runs.vm_steps <= small_runs.vm_steps + small_runs.vm_steps / 10,
            "{small_runs:?} {large_runs:?}"
        );
    }
}

#[test]
fn the_check_reads_the_same_few_rows_in_a_model_of_any_size() {
    the_check_costs_the_same_in_a_model_of_any_size("sqlite::memory:", "sqlite::memory:", true);
}

#[test]
fn with_postgres_the_check_reads_the_same_few_rows_in_a_model_of_any_size() {
    let Ok(base) = std::env::var("OBOP_TEST_POSTGRES_URL") else {
        eprintln!("skipped: set OBOP_TEST_POSTGRES_URL to run against PostgreSQL");
        return;
    };
    let small = obop_storage::testing::TestDb::create(&base).unwrap();
    let large = obop_storage::testing::TestDb::create(&base).unwrap();
    the_check_costs_the_same_in_a_model_of_any_size(small.url(), large.url(), false);
}

// ---- the database as the last word ---------------------------------------------------------

fn tallied_over(url: &str) -> (Arc<Flaky>, Store) {
    let flaky = Flaky::new(obop_storage::open(url).unwrap());
    let backend = obop_storage::guarded(flaky.clone());
    let bytes = obop_storage::blob::open_bytes(url, None).unwrap();
    let store = Store {
        blobs: Arc::new(obop_storage::blob::Catalog::new(backend.clone(), bytes)),
        backend,
    };
    (flaky, store)
}

/// A hook that makes another writer commit `code` as record `id` of `model`.
fn rival(model: &str, code: &'static str, id: &'static str) -> obop_storage::testing::AfterInsert {
    let model = model.to_string();
    Box::new(move |inner: &dyn obop_storage::Backend| {
        let mut f = obop_storage::Fields::new();
        f.insert("code".into(), serde_json::json!(code));
        inner.insert(&model, id, &f).unwrap();
    })
}

/// Another writer commits the same key between the check and the commit. The unique index
/// refuses the commit, and the refusal says "must be unique", not a storage fault.
fn rival_between_check_and_commit(url: &str) {
    let (flaky, store) = tallied_over(url);
    let it = Items::over(store);
    let md = it.model.clone();
    let said = format!(
        "{md}.code must be unique: another record with that value was saved at the same moment"
    );
    flaky.before_the_next_apply(rival(&md, "RACE", "rival"));
    let e = it
        .refusal("create", m(vec![("code", t("RACE"))]))
        .expect("the commit is refused");
    assert_eq!(e, said, "{url}");
    assert!(!obop_storage::fault::Fault::is_transient(&e), "{url}");
    assert_eq!(
        it.store.backend.count(&md).unwrap(),
        1,
        "{url}: the rival's record only"
    );
    // The same in a bulk create: the call is refused as a whole and writes nothing.
    flaky.before_the_next_apply(rival(&md, "RACE2", "rival2"));
    let rows = Value::List(vec![
        m(vec![("code", t("FINE"))]),
        m(vec![("code", t("RACE2"))]),
    ]);
    let e = it
        .refusal("bulk_create", m(vec![("rows", rows)]))
        .expect("the commit is refused");
    assert_eq!(e, said, "{url}");
    assert_eq!(
        it.store.backend.count(&md).unwrap(),
        2,
        "{url}: nothing of the bulk call"
    );
    // And the key is free for whoever asks next, once the rival is gone.
    it.store.backend.delete(&md, "rival").unwrap();
    it.call("create", m(vec![("code", t("RACE"))])).unwrap();
}

#[test]
fn a_rival_commit_between_check_and_commit_is_refused_as_unique() {
    rival_between_check_and_commit("sqlite::memory:");
    if let Ok(u) = std::env::var("OBOP_TEST_POSTGRES_URL") {
        let db = obop_storage::testing::TestDb::create(&u).unwrap();
        rival_between_check_and_commit(db.url());
    }
}

// ---- two writers at once -------------------------------------------------------------------

/// What one of two racing creates of one key came to.
#[derive(Debug, PartialEq)]
enum Came {
    Created,
    /// The check saw the other record.
    Checked,
    /// The index refused the commit.
    Backstop,
}

fn came(r: Result<Response, CallError>, md: &str) -> Came {
    match r {
        Ok(_) => Came::Created,
        Err(CallError::Handler(e)) if e.contains("is already used by another record") => {
            assert!(e.starts_with(&format!("{md}.code must be unique: ")), "{e}");
            Came::Checked
        }
        Err(CallError::Handler(e)) if e.contains("was saved at the same moment") => {
            assert!(e.starts_with(&format!("{md}.code must be unique: ")), "{e}");
            Came::Backstop
        }
        Err(other) => panic!("neither created nor refused as unique: {other:?}"),
    }
}

/// `trials` rounds of two threads that create one key at the same moment, through two
/// dispatchers over two connections to one database (two server processes that share no lock):
/// every round has one success and one refusal. Returns how many refusals the check gave and how
/// many the index gave.
fn race(url: &str, trials: usize) -> (usize, usize) {
    let a = Items::open(url);
    // The second "process": its own store on the same database and the same model.
    let b = Items {
        d: Arc::new(Dispatcher::new()),
        store: Store::open(url).unwrap(),
        model: a.model.clone(),
        shape: &TEXT,
    };
    let module = item_module(a.model.strip_suffix("_item").unwrap());
    b.d.register_crud(&module, &b.store);
    let (mut checked, mut backstop) = (0, 0);
    for n in 0..trials {
        let code = format!("K{n}");
        let gate = Arc::new(std::sync::Barrier::new(2));
        let outcomes: Vec<Came> = std::thread::scope(|s| {
            let x = s.spawn(|| {
                gate.wait();
                a.call("create", m(vec![("code", t(&code))]))
            });
            let y = s.spawn(|| {
                gate.wait();
                b.call("create", m(vec![("code", t(&code))]))
            });
            [x, y]
                .into_iter()
                .map(|h| came(h.join().unwrap(), &a.model))
                .collect()
        });
        let created = outcomes.iter().filter(|c| **c == Came::Created).count();
        assert_eq!(created, 1, "round {n}: {outcomes:?}");
        match outcomes.iter().find(|c| **c != Came::Created).unwrap() {
            Came::Checked => checked += 1,
            _ => backstop += 1,
        }
        let rows = a
            .store
            .backend
            .find(&a.model, "code", &serde_json::json!(code))
            .unwrap();
        assert_eq!(rows.len(), 1, "round {n}: {outcomes:?}");
    }
    (checked, backstop)
}

/// Two threads through one dispatcher: its transaction lock makes the second one see the first,
/// and the check refuses it.
fn race_in_one_server(url: &str, trials: usize) {
    let a = Items::open(url);
    for n in 0..trials {
        let code = format!("K{n}");
        let gate = Arc::new(std::sync::Barrier::new(2));
        let outcomes: Vec<Came> = std::thread::scope(|s| {
            let one = || {
                gate.wait();
                a.call("create", m(vec![("code", t(&code))]))
            };
            let (x, y) = (s.spawn(one), s.spawn(one));
            [x, y]
                .into_iter()
                .map(|h| came(h.join().unwrap(), &a.model))
                .collect()
        });
        let mut sorted: Vec<String> = outcomes.iter().map(|c| format!("{c:?}")).collect();
        sorted.sort();
        assert_eq!(sorted, ["Checked", "Created"], "round {n}");
    }
}

#[test]
fn two_creates_of_one_key_at_once_give_one_success_and_one_refusal_on_sqlite() {
    let path = std::env::temp_dir().join(format!("obop-race-{}.db", std::process::id()));
    let url = format!("sqlite://{}", path.display());
    let (checked, backstop) = race(&url, 100);
    eprintln!("sqlite: 100 of 100 rounds; refused by the check {checked}, by the index {backstop}");
    race_in_one_server(&url, 100);
    for ext in ["", "-wal", "-shm", ".blobs"] {
        let p = format!("{}{ext}", path.display());
        let _ = std::fs::remove_file(&p);
        let _ = std::fs::remove_dir_all(&p);
    }
}

#[test]
fn two_creates_of_one_key_at_once_give_one_success_and_one_refusal_on_postgres() {
    let Ok(base) = std::env::var("OBOP_TEST_POSTGRES_URL") else {
        eprintln!("skipped: set OBOP_TEST_POSTGRES_URL to run against PostgreSQL");
        return;
    };
    let db = obop_storage::testing::TestDb::create(&base).unwrap();
    let (checked, backstop) = race(db.url(), 100);
    eprintln!(
        "postgres: 100 of 100 rounds; refused by the check {checked}, by the index {backstop}"
    );
    race_in_one_server(db.url(), 100);
}

// ---- other values and other paths ----------------------------------------------------------

/// `meas`: an exact decimal that is unique, and an instant that is unique within a tray.
fn meas_module(p: &str) -> Module {
    let model = ModelDef {
        name: intern(&format!("{p}_meas")),
        description: "d",
        read_group: U,
        write_group: U,
        access: None,
        fields: vec![
            f("amount").decimal(10, 2).unique(),
            f("at").datetime().unique_with(&["tray"]),
            f("tray"),
            f("count").kind(obop_registry::FieldKind::Number).unique(),
        ],
        commands: vec![],
        ext: Default::default(),
    };
    Module {
        id: p.into(),
        name: "Measures".into(),
        models: vec![model],
        ..Module::default()
    }
}

#[test]
fn values_are_compared_in_their_canonical_form_and_by_their_kind() {
    for url in urls() {
        let scratch = url
            .starts_with("postgres")
            .then(|| obop_storage::testing::TestDb::create(&url).unwrap());
        let url = scratch.as_ref().map_or(url, |db| db.url().to_string());
        let store = Store::open(&url).unwrap();
        let p = format!("ms{}", &uuid::Uuid::now_v7().simple().to_string()[22..]);
        let d = Arc::new(Dispatcher::new());
        d.register_crud(&meas_module(&p), &store);
        let who = Principal::user("ann", &[U]);
        let call = |c: &str, input: Value| {
            d.call(
                Surface::Rest,
                &who,
                &mut Vault::new(),
                &format!("{p}_meas.{c}"),
                &input,
            )
        };
        let unique = |r: Result<Response, CallError>| match r {
            Ok(_) => None,
            Err(CallError::Handler(e)) => Some(e),
            Err(other) => panic!("{other:?}"),
        };
        // Decimals: the text a person typed is stored in the canonical form, so "1.5" and "1.50"
        // are one value.
        assert_eq!(unique(call("create", m(vec![("amount", t("1.50"))]))), None);
        let again = unique(call("create", m(vec![("amount", t("1.5"))]))).unwrap();
        assert!(
            again.starts_with(&format!("{p}_meas.amount must be unique: \"1.50\"")),
            "{again}"
        );
        assert_eq!(unique(call("create", m(vec![("amount", t("1.51"))]))), None);
        // Instants: the same moment written in another zone is the same value, within a tray.
        let at = |s: &str, tray: &str| m(vec![("at", t(s)), ("tray", t(tray))]);
        assert_eq!(
            unique(call("create", at("2026-03-01T10:00:00Z", "a"))),
            None
        );
        let again = unique(call("create", at("2026-03-01T12:00:00+02:00", "a")));
        let again = again.expect("the same instant in the same tray");
        assert!(
            again.contains("at must be unique") && again.ends_with("with the same tray"),
            "{again}"
        );
        assert_eq!(
            unique(call("create", at("2026-03-01T10:00:00Z", "b"))),
            None
        );
        // A number is compared as a number.
        let n = |v: i64| m(vec![("count", Value::Int(v))]);
        assert_eq!(unique(call("create", n(7))), None);
        let again = unique(call("create", n(7))).unwrap();
        assert!(
            again.ends_with("count must be unique: 7 is already used by another record"),
            "{again}"
        );
        assert_eq!(unique(call("create", n(8))), None);
        // Zero is a value (only the empty text is not).
        assert_eq!(unique(call("create", n(0))), None);
        assert!(unique(call("create", n(0))).is_some());
    }
}

#[test]
fn an_update_never_collides_with_the_record_itself_and_moves_a_key_cleanly() {
    for url in urls() {
        let scratch = url
            .starts_with("postgres")
            .then(|| obop_storage::testing::TestDb::create(&url).unwrap());
        let url = scratch.as_ref().map_or(url, |db| db.url().to_string());
        let it = Items::open(&url);
        let id_of = |v: Value| {
            let Value::Map(o) = v else { panic!() };
            let Some(Value::Text(id)) = o.get("id") else {
                panic!()
            };
            id.clone()
        };
        let make = |pairs: Vec<(&str, &str)>| {
            id_of(
                it.call(
                    "create",
                    m(pairs.into_iter().map(|(k, v)| (k, t(v))).collect()),
                )
                .unwrap()
                .value,
            )
        };
        let a = make(vec![("slot", "P"), ("scope", "x")]);
        let b = make(vec![("slot", "P"), ("scope", "y")]);
        let upd = |id: &str, pairs: Vec<(&str, &str)>| {
            let mut v = vec![("id", t(id))];
            v.extend(pairs.into_iter().map(|(k, x)| (k, t(x))));
            it.refusal("update", m(v))
        };
        // The record with its own key, and with its own key sent again.
        assert_eq!(upd(&a, vec![("slot", "P"), ("scope", "x")]), None);
        assert_eq!(upd(&a, vec![("region", "r")]), None);
        // The scope moves into the other record's: refused; the slot moves away first: accepted.
        let e = upd(&b, vec![("scope", "x")]).unwrap();
        assert!(
            e.ends_with(
                "slot must be unique: \"P\" is already used by another record with the same scope"
            ),
            "{e}"
        );
        assert_eq!(upd(&b, vec![("slot", "Q")]), None);
        assert_eq!(upd(&b, vec![("scope", "x")]), None);
        // Now the old key of the first is taken by nobody and the second can have it.
        assert_eq!(upd(&a, vec![("slot", "Z")]), None);
        assert_eq!(upd(&b, vec![("slot", "P")]), None);
        // Clearing a scope makes it the empty scope.
        assert_eq!(upd(&b, vec![("scope", "")]), None);
        assert!(upd(&a, vec![("slot", "P"), ("scope", "")]).is_some());
    }
}

#[test]
fn a_row_that_failed_leaves_nothing_behind_in_a_bulk_call() {
    for url in urls() {
        let scratch = url
            .starts_with("postgres")
            .then(|| obop_storage::testing::TestDb::create(&url).unwrap());
        let url = scratch.as_ref().map_or(url, |db| db.url().to_string());
        let it = Items::open(&url);
        let row = |pairs: Vec<(&str, &str)>| m(pairs.into_iter().map(|(k, v)| (k, t(v))).collect());
        it.call("create", row(vec![("slot", "P"), ("scope", "s")]))
            .unwrap();
        // Row 1 sets the code K and then fails on the slot, so it is taken back; row 2 uses K.
        // Row 3 repeats row 2's code and fails; row 4 repeats the slot of row 2 in its scope.
        let rows = Value::List(vec![
            row(vec![("code", "K"), ("slot", "P"), ("scope", "s")]),
            row(vec![("code", "K"), ("slot", "Q"), ("scope", "s")]),
            row(vec![("code", "K")]),
            row(vec![("code", "L"), ("slot", "Q"), ("scope", "s")]),
            row(vec![("code", "M"), ("slot", "Q"), ("scope", "t")]),
        ]);
        let out = it
            .call(
                "bulk_create",
                m(vec![("rows", rows), ("mode", t("partial"))]),
            )
            .unwrap()
            .value;
        let Value::Map(o) = &out else { panic!() };
        let Some(Value::List(errs)) = o.get("errors") else {
            panic!()
        };
        let failed: Vec<i64> = errs
            .iter()
            .map(|e| match e {
                Value::Map(e) => match e["row"] {
                    Value::Int(n) => n,
                    _ => panic!(),
                },
                _ => panic!(),
            })
            .collect();
        assert_eq!(failed, [1, 3, 4], "{url}");
        assert_eq!(created(&out), 2, "{url}");
        assert_eq!(it.store.backend.count(&it.model).unwrap(), 3, "{url}");
    }
}
