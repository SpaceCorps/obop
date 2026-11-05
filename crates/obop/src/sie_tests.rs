//! SIE export and import against generated files: invented accounts and companies, no real data.

use crate::app::App;
use crate::sie::Env;
use crate::sie::export::export;
use crate::sie::import::{Options, Report, import};
use base64::Engine as _;
use obop_commands::{ADMIN_GROUP, CallError, Principal, Surface, Value, Vault};
use obop_pkg::{Bundled, Marketplace};
use obop_sie::{Account, AccountKind, Date, Line, SieFile, Voucher};
use serde_json::Value as J;
use std::collections::BTreeMap;

/// 2026-09-14.
const SEPTEMBER_2026: i64 = 1_789_400_000;

fn d(m: u8, day: u8) -> Date {
    Date::new(2026, m, day).unwrap()
}

fn app() -> App {
    let app = App::new();
    app.dispatcher
        .set_clock(std::sync::Arc::new(|| SEPTEMBER_2026));
    app.installer
        .install(&Bundled.fetch("accounting").unwrap(), "marketplace", "test")
        .unwrap();
    let se = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packages/contrib/accounting_se/accounting_se.toml"
    ))
    .unwrap();
    app.installer.install(&se, "file", "test").unwrap();
    app
}

fn admin() -> Principal {
    Principal::user("root", &[ADMIN_GROUP])
}

fn env(app: &App) -> Env<'_> {
    Env {
        d: &app.dispatcher,
        store: &app.store,
    }
}

/// A generated file: invented numbers and names, series A and B, two years of dates inside 2026.
fn sample(kinds: bool) -> SieFile {
    let mut f = SieFile::new("Provbolaget AB", d(1, 1), d(12, 31));
    f.company.org_number = Some("556000-0000".into());
    for (n, name, k) in [
        (1930, "Test bank", AccountKind::Asset),
        (1510, "Test receivable", AccountKind::Asset),
        (2081, "Test equity", AccountKind::Liability),
        (2440, "Test payable", AccountKind::Liability),
        (3001, "Test sales", AccountKind::Income),
        (5010, "Test rent", AccountKind::Cost),
    ] {
        f.accounts.push(Account {
            number: n,
            name: name.into(),
            kind: kinds.then_some(k),
        });
    }
    let v = |s: &str, n: &str, m: u8, day: u8, text: &str, lines: Vec<Line>| Voucher {
        series: s.into(),
        number: n.into(),
        date: d(m, day),
        text: text.into(),
        registered: None,
        signed: None,
        lines,
    };
    f.vouchers = vec![
        v(
            "A",
            "1",
            1,
            10,
            "Start capital",
            vec![
                Line::new(1930, 1_000_000, None),
                Line::new(2081, -1_000_000, None),
            ],
        ),
        v(
            "A",
            "2",
            2,
            3,
            "Consulting invoice",
            vec![
                Line::new(1510, 125_000, None),
                Line::new(3001, -100_000, Some("Consulting")),
                Line::new(2440, -25_000, None),
            ],
        ),
        v(
            "B",
            "1",
            3,
            1,
            "Rent",
            vec![
                Line::new(5010, 40_000, None),
                Line::new(1930, -40_000, None),
            ],
        ),
    ];
    f
}

fn opts() -> Options {
    Options {
        default_account_types: true,
        ..Options::default()
    }
}

fn ok(r: Result<Report, Report>) -> Report {
    r.unwrap_or_else(|r| panic!("import failed:\n{r}"))
}

fn problems(r: Result<Report, Report>) -> String {
    match r {
        Ok(r) => panic!("expected a refusal, got:\n{r}"),
        Err(r) => r.problems.join("\n"),
    }
}

fn recs(app: &App, model: &str) -> Vec<(String, serde_json::Map<String, J>)> {
    app.store
        .backend()
        .list(model)
        .unwrap_or_default()
        .into_iter()
        .map(|(id, f)| (id, f.into_iter().collect()))
        .collect()
}

const MODELS: &[&str] = &[
    "accounting_account",
    "accounting_journal",
    "accounting_move",
    "accounting_move_line",
    "import_map",
];

fn counts(app: &App) -> Vec<usize> {
    MODELS.iter().map(|m| recs(app, m).len()).collect()
}

/// Debit and credit per account code over every line of every move.
fn trial_balance(app: &App) -> BTreeMap<String, (i64, i64)> {
    let code: BTreeMap<String, String> = recs(app, "accounting_account")
        .into_iter()
        .map(|(id, f)| (id, f["code"].as_str().unwrap().to_string()))
        .collect();
    let mut out: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    for (_, l) in recs(app, "accounting_move_line") {
        let e = out
            .entry(code[l["account_id"].as_str().unwrap()].clone())
            .or_default();
        e.0 += l["debit"].as_i64().unwrap();
        e.1 += l["credit"].as_i64().unwrap();
    }
    out
}

fn bytes_of(f: &SieFile) -> Vec<u8> {
    obop_sie::export(f)
}

#[test]
fn a_dry_run_writes_nothing_and_prints_counts() {
    let a = app();
    let rep = ok(import(
        env(&a),
        &admin(),
        &bytes_of(&sample(true)),
        &Options {
            dry_run: true,
            ..opts()
        },
    ));
    assert_eq!(
        counts(&a),
        vec![0; MODELS.len()],
        "a dry run must write nothing"
    );
    let c = &rep.counts;
    assert_eq!(
        (c.accounts_new, c.journals_new, c.vouchers_new, c.lines_new),
        (6, 2, 3, 7)
    );
    let text = rep.to_string();
    assert!(
        text.contains("dry run, nothing written") && text.contains("would create"),
        "{text}"
    );
}

#[test]
fn drafts_keep_series_and_number_and_the_totals_equal_the_file() {
    let a = app();
    let rep = ok(import(env(&a), &admin(), &bytes_of(&sample(true)), &opts()));
    assert!(rep.verified.as_ref().unwrap().contains("drafts"), "{rep}");
    let moves = recs(&a, "accounting_move");
    let mut refs: Vec<&str> = moves
        .iter()
        .map(|(_, m)| m["reference"].as_str().unwrap())
        .collect();
    refs.sort_unstable();
    assert_eq!(refs, ["A1", "A2", "B1"]);
    assert!(moves.iter().all(|(_, m)| m["state"] == "draft"));
    let tb = trial_balance(&a);
    assert_eq!(tb["1930"], (1_000_000, 40_000));
    assert_eq!(tb["3001"], (0, 100_000));
    assert!(rep.to_string().contains("not imported"));
}

#[test]
fn post_runs_the_real_action_in_file_order() {
    let a = app();
    let o = Options {
        post: true,
        ..opts()
    };
    let rep = ok(import(env(&a), &admin(), &bytes_of(&sample(true)), &o));
    assert!(
        rep.verified.as_ref().unwrap().contains("all posted"),
        "{rep}"
    );
    let moves = recs(&a, "accounting_move");
    let number_of = |r: &str| {
        moves.iter().find(|(_, m)| m["reference"] == r).unwrap().1["number"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert_eq!(number_of("A1"), "JE/2026/00001");
    assert_eq!(number_of("A2"), "JE/2026/00002");
    assert_eq!(number_of("B1"), "JE/2026/00003");
}

#[test]
fn a_rerun_creates_nothing_and_post_can_follow() {
    let a = app();
    let bytes = bytes_of(&sample(true));
    ok(import(env(&a), &admin(), &bytes, &opts()));
    let before = counts(&a);
    let again = ok(import(env(&a), &admin(), &bytes, &opts()));
    assert_eq!(counts(&a), before, "a re-run must create nothing");
    assert_eq!(again.counts.vouchers_new, 0);
    assert_eq!(again.counts.vouchers_already, 3);
    assert!(
        again
            .verified
            .unwrap()
            .contains("trial balance equals the file")
    );
    // Post the drafts later.
    let o = Options {
        post: true,
        ..opts()
    };
    let rep = ok(import(env(&a), &admin(), &bytes, &o));
    assert!(rep.verified.unwrap().contains("all posted"));
    assert!(
        recs(&a, "accounting_move")
            .iter()
            .all(|(_, m)| m["state"] == "posted")
    );
    let after = counts(&a);
    assert_eq!(after, before);
}

#[test]
fn an_unbalanced_file_is_refused_and_nothing_is_written() {
    let a = app();
    let mut f = sample(true);
    f.vouchers[0].lines[1].amount_minor = -999_999;
    let p = problems(import(env(&a), &admin(), &bytes_of(&f), &opts()));
    assert!(p.contains("unbalanced"), "{p}");
    assert_eq!(counts(&a), vec![0; MODELS.len()]);
}

#[test]
fn new_accounts_need_a_confirmed_type() {
    let a = app();
    let p = problems(import(
        env(&a),
        &admin(),
        &bytes_of(&sample(true)),
        &Options::default(),
    ));
    assert!(p.contains("no confirmed type"), "{p}");
    assert_eq!(counts(&a), vec![0; MODELS.len()]);
    // A file for each account is enough as well.
    let o = Options {
        account_types: Some(
            "1930,asset_cash\n1510,asset_receivable\n2081,equity\n2440,liability_payable\n3001,income\n5010,expense\n".into(),
        ),
        ..Options::default()
    };
    ok(import(env(&a), &admin(), &bytes_of(&sample(true)), &o));
    let t = recs(&a, "accounting_account")
        .into_iter()
        .find(|(_, f)| f["code"] == "1930")
        .unwrap()
        .1["account_type"]
        .clone();
    assert_eq!(t, "asset_cash");
}

#[test]
fn an_account_without_ktyp_is_typed_from_its_class() {
    let a = app();
    ok(import(
        env(&a),
        &admin(),
        &bytes_of(&sample(false)),
        &opts(),
    ));
    let ty = |code: &str| {
        recs(&a, "accounting_account")
            .into_iter()
            .find(|(_, f)| f["code"] == code)
            .unwrap()
            .1["account_type"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert_eq!(ty("3001"), "income");
    assert_eq!(ty("5010"), "expense");
    assert_eq!(ty("2440"), "liability_payable");
}

#[test]
fn a_voucher_on_an_undeclared_account_is_refused() {
    let a = app();
    let mut f = sample(true);
    f.vouchers[2].lines[0].account = 7777;
    let p = problems(import(env(&a), &admin(), &bytes_of(&f), &opts()));
    assert!(p.contains("no #KONTO"), "{p}");
    assert_eq!(counts(&a), vec![0; MODELS.len()]);
}

#[test]
fn post_is_refused_for_an_agent_and_for_a_clerk() {
    let a = app();
    let o = Options {
        post: true,
        ..opts()
    };
    let agent = Principal::agent(
        "bot",
        &[ADMIN_GROUP],
        &[
            "accounting.user",
            "accounting.manager",
            "accounting.approver",
        ],
    );
    let p = problems(import(env(&a), &agent, &bytes_of(&sample(true)), &o));
    assert!(p.contains("--post runs the real post action"), "{p}");
    let clerk = Principal::user("clerk", &["accounting.user", "accounting.manager"]);
    let p = problems(import(env(&a), &clerk, &bytes_of(&sample(true)), &o));
    assert!(p.contains("--post runs the real post action"), "{p}");
    assert_eq!(counts(&a), vec![0; MODELS.len()]);
}

#[test]
fn a_move_with_the_same_reference_that_is_not_in_the_map_is_refused() {
    let a = app();
    let bytes = bytes_of(&sample(true));
    ok(import(env(&a), &admin(), &bytes, &opts()));
    // Same books, a file of another company: the same references collide with the first run.
    let mut other = sample(true);
    other.company.name = "Annat Bolag AB".into();
    let p = problems(import(env(&a), &admin(), &bytes_of(&other), &opts()));
    assert!(p.contains("not in the import map"), "{p}");
}

#[test]
fn export_then_import_into_a_fresh_database_gives_the_same_trial_balance() {
    let a = app();
    ok(import(
        env(&a),
        &admin(),
        &bytes_of(&sample(true)),
        &Options {
            post: true,
            ..opts()
        },
    ));
    // A company record, so the file names it.
    a.store
        .insert(
            "accounting_se_company",
            [
                ("name", Value::text("Provbolaget AB")),
                ("org_number", Value::text("556000-0000")),
                ("fiscal_year_start_month", Value::Int(1)),
                ("vat_period", Value::text("quarterly")),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
        )
        .unwrap();
    let out = export(env(&a), &admin(), 2026).unwrap();
    assert_eq!((out.vouchers, out.accounts), (3, 6));
    let parsed = obop_sie::parse(&out.bytes).unwrap();
    assert_eq!(parsed.company.name, "Provbolaget AB");
    assert_eq!(parsed.company.org_number.as_deref(), Some("556000-0000"));
    let refs: Vec<String> = parsed
        .vouchers
        .iter()
        .map(|v| format!("{}{}", v.series, v.number))
        .collect();
    assert_eq!(refs, ["A1", "A2", "B1"]);

    let b = app();
    let rep = ok(import(
        env(&b),
        &admin(),
        &out.bytes,
        &Options {
            post: true,
            ..opts()
        },
    ));
    assert!(
        rep.verified.as_ref().unwrap().contains("all posted"),
        "{rep}"
    );
    assert_eq!(trial_balance(&a), trial_balance(&b));
    // And the second export is the same file apart from the generation date.
    let again = export(env(&b), &admin(), 2026).unwrap();
    let (x, y) = (
        obop_sie::parse(&out.bytes).unwrap(),
        obop_sie::parse(&again.bytes).unwrap(),
    );
    assert_eq!(x.vouchers, y.vouchers);
    assert_eq!(x.accounts, y.accounts);
}

#[test]
fn only_posted_moves_of_the_year_are_exported() {
    let a = app();
    let bytes = bytes_of(&sample(true));
    ok(import(env(&a), &admin(), &bytes, &opts()));
    // Drafts only: nothing to export.
    assert_eq!(export(env(&a), &admin(), 2026).unwrap().vouchers, 0);
    ok(import(
        env(&a),
        &admin(),
        &bytes,
        &Options {
            post: true,
            ..opts()
        },
    ));
    assert_eq!(export(env(&a), &admin(), 2026).unwrap().vouchers, 3);
    // Another year has none.
    assert_eq!(export(env(&a), &admin(), 2025).unwrap().vouchers, 0);
}

#[test]
fn export_needs_the_accounting_group() {
    let a = app();
    let nobody = Principal::user("nobody", &["base.user"]);
    let e = export(env(&a), &nobody, 2026).err().unwrap();
    assert!(e.contains("accounting.user"), "{e}");
}

fn call(a: &App, p: &Principal, id: &str, input: Value) -> Result<Value, CallError> {
    a.dispatcher
        .call(Surface::Rest, p, &mut Vault::new(), id, &input)
        .map(|r| r.value)
}

fn get<'a>(v: &'a Value, k: &str) -> &'a Value {
    let Value::Map(m) = v else {
        panic!("not a map")
    };
    m.get(k).unwrap_or_else(|| panic!("no {k} in {v:?}"))
}

#[test]
fn the_typed_commands_export_and_import() {
    let a = app();
    let file = bytes_of(&sample(true));
    let b64 = base64::engine::general_purpose::STANDARD.encode(&file);
    let input = |dry: bool| {
        Value::map([
            ("content_base64", Value::text(&b64)),
            ("dry_run", Value::Bool(dry)),
            ("default_account_types", Value::Bool(true)),
        ])
    };
    let r = call(&a, &admin(), "sie_exchange.import", input(true)).unwrap();
    assert_eq!(get(&r, "ok"), &Value::Bool(true));
    assert_eq!(counts(&a), vec![0; MODELS.len()]);
    let r = call(&a, &admin(), "sie_exchange.import", input(false)).unwrap();
    assert_eq!(get(&r, "ok"), &Value::Bool(true), "{r:?}");
    assert_eq!(recs(&a, "accounting_move").len(), 3);
    // Drafts are not exported; post through the real action, then export.
    for (id, _) in recs(&a, "accounting_move") {
        call(
            &a,
            &admin(),
            "accounting_move.post",
            Value::map([("id", Value::text(&id))]),
        )
        .unwrap();
    }
    let r = call(
        &a,
        &admin(),
        "sie_exchange.export",
        Value::map([("year", Value::Int(2026))]),
    )
    .unwrap();
    assert_eq!(get(&r, "vouchers"), &Value::Int(3));
    let Value::Text(content) = get(&r, "content_base64") else {
        panic!()
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(content)
        .unwrap();
    assert_eq!(obop_sie::parse(&bytes).unwrap().vouchers.len(), 3);
    // Not a number: refused in words.
    assert!(call(&a, &admin(), "sie_exchange.export", Value::map([])).is_err());
    // A person without the group cannot.
    let nobody = Principal::user("nobody", &["base.user"]);
    assert!(matches!(
        call(
            &a,
            &nobody,
            "sie_exchange.export",
            Value::map([("year", Value::Int(2026))])
        ),
        Err(CallError::Forbidden(_))
    ));
}

#[test]
fn an_agent_may_only_dry_run_the_import() {
    let a = app();
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes_of(&sample(true)));
    // The agent acts for a clerk who holds the two groups; its scopes are the same two.
    let agent = Principal::agent(
        "bot",
        &["accounting.user", "accounting.manager"],
        &["accounting.user", "accounting.manager"],
    );
    let input = |dry: bool| {
        Value::map([
            ("content_base64", Value::text(&b64)),
            ("default_account_types", Value::Bool(true)),
            ("dry_run", Value::Bool(dry)),
        ])
    };
    let Err(CallError::Handler(m)) = call(&a, &agent, "sie_exchange.import", input(false)) else {
        panic!("an agent must not run the import for real")
    };
    assert!(m.contains("dry run"), "{m}");
    assert_eq!(counts(&a), vec![0; MODELS.len()]);
    let r = call(&a, &agent, "sie_exchange.import", input(true)).unwrap();
    assert_eq!(get(&r, "ok"), &Value::Bool(true), "{r:?}");
    assert_eq!(
        counts(&a),
        vec![0; MODELS.len()],
        "a dry run writes nothing"
    );
}
