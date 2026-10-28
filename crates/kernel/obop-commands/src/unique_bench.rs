//! What a uniqueness check (D57) costs by the size of the model, for the ask "11.4": a model with
//! two unique rules (`code`, and `slot` within `scope`), filled through `bulk_create`, then single
//! creates.
//!
//! Two tests live here and use nothing but the public surface of the dispatcher, so the same file
//! runs against the code before the keyed lookup and shows the change:
//!
//! * `the_cost_of_a_create_does_not_grow_with_the_model` runs everywhere and bounds a ratio of two
//!   medians (generous: the old code was linear in the model, a ratio of 10 or more).
//! * `measure_seeding` is `#[ignore]`d and prints the table of the PR:
//!   `cargo test -p obop-commands --lib measure_seeding -- --ignored --nocapture`, with
//!   `OBOP_BENCH_URL` (`sqlite://FILE`, `sqlite::memory:`, `postgres://...`; default a SQLite
//!   file in the temporary directory), `OBOP_BENCH_ROWS` (default 50000) and `OBOP_BENCH_BLOCK`
//!   (rows per `bulk_create`, default 1000, at most 1000).

use crate::{CallError, Dispatcher, Principal, Response, Store, Surface, Value, Vault};
use obop_registry::{FieldDef as F, ModelDef, Module, intern};
use obop_types::DataClass;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub(crate) const U: &str = "bench.user";

pub(crate) fn f(name: &'static str) -> F {
    F::new(name, "d", DataClass::None, false, U)
}

/// The module of the measurements: `reaction` with `code` unique, and `slot` unique within
/// `scope`.
pub(crate) fn reaction_module(p: &str) -> Module {
    let model = ModelDef {
        name: intern(&format!("{p}_reaction")),
        description: "d",
        read_group: U,
        write_group: U,
        access: None,
        fields: vec![
            f("code").unique().required(),
            f("slot").unique_with(&["scope"]),
            f("scope"),
            f("note"),
        ],
        commands: vec![],
        ext: Default::default(),
    };
    Module {
        id: p.into(),
        name: "Bench".into(),
        models: vec![model],
        ..Module::default()
    }
}

pub(crate) fn m(pairs: Vec<(&str, Value)>) -> Value {
    Value::Map(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

pub(crate) fn t(s: &str) -> Value {
    Value::text(s)
}

/// The row number `i` of the measurements: `code` is its own, `slot` repeats three times with
/// three scopes, so the pair is unique but `slot` alone is not.
pub(crate) fn row(i: usize) -> Vec<(&'static str, Value)> {
    vec![
        ("code", t(&format!("R{i:07}"))),
        ("slot", t(&format!("S{:07}", i / 3))),
        ("scope", t(&format!("g{}", i % 3))),
        ("note", t("measured")),
    ]
}

pub(crate) struct Bench {
    pub d: Arc<Dispatcher>,
    pub store: Store,
    pub p: String,
}

impl Bench {
    pub fn open(url: &str) -> Bench {
        Self::over(Store::open(url).unwrap())
    }

    pub fn over(store: Store) -> Bench {
        let p = format!("rx{}", &uuid::Uuid::now_v7().simple().to_string()[22..]);
        let d = Arc::new(Dispatcher::new());
        let module = reaction_module(&p);
        let v = obop_registry::check_module(&module);
        assert!(v.is_empty(), "{v:?}");
        d.register_crud(&module, &store);
        Bench { d, store, p }
    }

    pub fn model(&self) -> String {
        format!("{}_reaction", self.p)
    }

    pub fn call(&self, who: &Principal, c: &str, input: Value) -> Result<Response, CallError> {
        self.d.call(
            Surface::Rest,
            who,
            &mut Vault::new(),
            &format!("{}.{c}", self.model()),
            &input,
        )
    }

    pub fn user() -> Principal {
        Principal::user(&format!("bench-{}", uuid::Uuid::now_v7().simple()), &[U])
    }

    /// Rows `from..to` through `bulk_create`, `block` at a time, one person per call (the rate
    /// limit counts per person). The time taken.
    pub fn seed(&self, from: usize, to: usize, block: usize) -> Duration {
        let start = Instant::now();
        let mut at = from;
        while at < to {
            let end = (at + block).min(to);
            let rows = Value::List((at..end).map(|i| m(row(i))).collect());
            let out = self
                .call(&Self::user(), "bulk_create", m(vec![("rows", rows)]))
                .unwrap_or_else(|e| panic!("bulk_create of {at}..{end}: {e:?}"));
            assert_eq!(created(&out.value), end - at, "{:?}", out.value);
            at = end;
        }
        start.elapsed()
    }

    /// One `create` of row `i`. The time taken.
    pub fn create_one(&self, who: &Principal, i: usize) -> Duration {
        let start = Instant::now();
        self.call(who, "create", m(row(i)))
            .unwrap_or_else(|e| panic!("create of row {i}: {e:?}"));
        start.elapsed()
    }

    /// The median time of `n` single creates of rows `from..from + n`.
    pub fn median_create(&self, from: usize, n: usize) -> Duration {
        let who = Self::user();
        let mut times: Vec<Duration> = (from..from + n).map(|i| self.create_one(&who, i)).collect();
        times.sort();
        times[times.len() / 2]
    }
}

pub(crate) fn created(v: &Value) -> usize {
    let Value::Map(o) = v else { panic!("{v:?}") };
    match o.get("created") {
        Some(Value::Int(n)) => *n as usize,
        other => panic!("no created count: {other:?}"),
    }
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// A single create costs about the same in a model of 8,000 rows as in one of 500: before the
/// keyed lookup it cost ten times as much. The bound is a ratio of medians of 25 creates, not a
/// time, and generous (3, the ask's own figure) so a loaded machine does not fail it.
#[test]
fn the_cost_of_a_create_does_not_grow_with_the_model() {
    let b = Bench::open("sqlite::memory:");
    b.seed(0, 500, 500);
    let small = b.median_create(500, 25);
    b.seed(525, 8_000, 1000);
    let large = b.median_create(8_000, 25);
    eprintln!(
        "single create: {:.2} ms at 500 rows, {:.2} ms at 8,000 rows (ratio {:.2})",
        ms(small),
        ms(large),
        ms(large) / ms(small)
    );
    assert!(
        large.as_secs_f64() < 3.0 * small.as_secs_f64(),
        "a create takes {:.2} ms in a model of 8,000 rows and {:.2} ms in one of 500",
        ms(large),
        ms(small)
    );
}

/// Prints the table: the time of every block of a seed, and a single create at 2,500 and at the
/// last size.
#[test]
#[ignore = "a measurement, not a check: run it with --ignored --nocapture"]
fn measure_seeding() {
    let url = std::env::var("OBOP_BENCH_URL").unwrap_or_else(|_| {
        let path = std::env::temp_dir().join(format!("obop-bench-{}.db", std::process::id()));
        format!("sqlite://{}", path.display())
    });
    let rows: usize = std::env::var("OBOP_BENCH_ROWS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50_000);
    let block: usize = std::env::var("OBOP_BENCH_BLOCK")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1000)
        .min(1000);
    let b = Bench::open(&url);
    let kind = b.store.backend.kind();
    println!(
        "backend {kind} ({url}), build {}",
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    );
    let mut single = Vec::new();
    let mut at = 0;
    let total = Instant::now();
    let probes = [2_500, rows];
    while at < rows {
        let to = probes
            .iter()
            .copied()
            .find(|p| *p > at && *p <= rows)
            .unwrap_or(rows)
            .min(at + 5_000);
        let took = b.seed(at, to, block);
        println!(
            "rows {at:>6}..{to:>6}: {:>8.2} s  {:>8.0} rows/s",
            took.as_secs_f64(),
            (to - at) as f64 / took.as_secs_f64()
        );
        at = to;
        if probes.contains(&at) {
            // Twenty creates beyond the seed: row numbers the seed did not use.
            let med = b.median_create(rows + 1_000 + single.len() * 100, 21);
            single.push((at, med));
            println!(
                "single create at {at:>6} rows: median of 21 = {:.2} ms",
                ms(med)
            );
        }
    }
    println!(
        "seeded {rows} rows in {:.1} s = {:.0} rows/s",
        total.elapsed().as_secs_f64(),
        rows as f64 / total.elapsed().as_secs_f64()
    );
    if let [(a, ta), (c, tc)] = single[..] {
        println!(
            "single create: {:.2} ms at {a} rows, {:.2} ms at {c} rows, ratio {:.2}",
            ms(ta),
            ms(tc),
            ms(tc) / ms(ta)
        );
    }
}
