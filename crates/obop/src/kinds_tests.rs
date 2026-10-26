//! D72 over HTTP, MCP and the CLI: the `decimal` and `datetime` kinds of a package installed at
//! runtime, from the manifest to the wire. Named acceptance tests: `rest_*`, `mcp_*`, `cli_*`,
//! `openapi_*`, `meta_*`.

use crate::app::App;
use crate::mfa_tests::{PW, post, send, token};
use obop_auth::{Auth, AuthConfig};
use obop_commands::{Dispatcher, Principal};
use obop_mcp::{Reply, Session};
use serde_json::{Value as J, json};
use std::sync::Arc;

const PACKAGE: &str = r#"
[module]
id = "ledger"
version = "0.1.0"
name = "Ledger"
summary = "Exact amounts and instants."
description = "Bookings with exact decimal amounts and instants, for the tests of the decimal and datetime kinds."
author = "Test"
license = "Apache-2.0"
homepage = "https://example.com/ledger"
keywords = ["ledger", "decimal"]
categories = ["productivity"]
maturity = "beta"
requires_core = "^0.2"

[module.dependencies]
base = "^0.2"

[[model]]
name = "ledger_entry"
description = "One booking"
read_group = "base.user"
write_group = "base.user"

  [[model.field]]
  name = "memo"
  description = "What it was for"
  class = "none"

  [[model.field]]
  name = "amount"
  description = "The amount, to the cent"
  class = "none"
  kind = "decimal"
  precision = 12
  scale = 2

  [[model.field]]
  name = "ratio"
  description = "A ratio to four decimals, rounded half up when imported"
  class = "none"
  kind = "decimal"
  precision = 10
  scale = 4
  round = "half_up"

  [[model.field]]
  name = "weight"
  description = "A weight with 8 decimals"
  class = "none"
  kind = "decimal"
  precision = 24
  scale = 8

  [[model.field]]
  name = "booked_at"
  description = "When it was booked"
  class = "none"
  kind = "datetime"

  [[model.field]]
  name = "share"
  description = "A third of the amount, to four decimals"
  class = "none"
  kind = "decimal"
  precision = 14
  scale = 4
  computed = "amount / 3"
"#;

struct Fx {
    r: axum::Router,
    app: App,
    ada: String,
    who: Principal,
}

async fn fixture() -> Fx {
    let app = App::new();
    app.installer.install(PACKAGE, "upload", "test").unwrap();
    let auth = Arc::new(Auth::with_test_config(
        app.store.backend(),
        AuthConfig {
            default_require_admins: false,
            ..AuthConfig::insecure_dev()
        },
    ));
    auth.create_user("ada", PW, &["base.user".to_string()])
        .unwrap();
    let r = obop_http::router(
        obop_http::HttpState::new(app.dispatcher.clone(), auth),
        None,
    );
    let ada = token(&r, "ada").await;
    Fx {
        r,
        app,
        ada,
        who: Principal::user("ada", &["base.user"]),
    }
}

/// A package whose decimals have the awkward shapes: whole numbers, nothing before the point,
/// one digit before it, and each of them with and without a rounding mode.
const SHAPES: &str = r#"
[module]
id = "shapes"
version = "0.1.0"
name = "Shapes"
summary = "Decimals of every awkward shape."
description = "Decimal fields of several precisions and scales, for the examples and patterns the surfaces print."
author = "Test"
license = "Apache-2.0"
homepage = "https://example.com/shapes"
keywords = ["decimal"]
categories = ["productivity"]
maturity = "beta"
requires_core = "^0.2"

[module.dependencies]
base = "^0.2"

[[model]]
name = "shapes_box"
description = "One of each shape"
read_group = "base.user"
write_group = "base.user"

  [[model.field]]
  name = "whole"
  description = "Five digits, none after the point"
  class = "none"
  kind = "decimal"
  precision = 5
  scale = 0

  [[model.field]]
  name = "fraction"
  description = "Nothing before the point"
  class = "none"
  kind = "decimal"
  precision = 2
  scale = 2

  [[model.field]]
  name = "narrow"
  description = "One digit before the point"
  class = "none"
  kind = "decimal"
  precision = 3
  scale = 2

  [[model.field]]
  name = "whole_rounding"
  description = "Whole numbers, rounded half even when imported"
  class = "none"
  kind = "decimal"
  precision = 4
  scale = 0
  round = "half_even"

  [[model.field]]
  name = "rounding"
  description = "Two decimals, rounded half up when imported"
  class = "none"
  kind = "decimal"
  precision = 8
  scale = 2
  round = "half_up"
"#;

/// (field, precision, scale, rounds on import) of the fields of [`SHAPES`].
const SHAPE_FIELDS: [(&str, u8, u8, bool); 5] = [
    ("whole", 5, 0, false),
    ("fraction", 2, 2, false),
    ("narrow", 3, 2, false),
    ("whole_rounding", 4, 0, true),
    ("rounding", 8, 2, true),
];

/// A canonical decimal of scale 4 written with one more fractional digit, and what rounding that
/// half up (away from zero at the half) gives, worked out on the digits and not by the parser.
fn round_half_up_from_one_more_digit(base: &str, digit: u64) -> (String, String) {
    let negative = base.starts_with('-');
    let mut units: u128 = base
        .chars()
        .filter(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .unwrap();
    if digit >= 5 {
        units += 1;
    }
    let s = format!("{units:05}");
    let (int, frac) = s.split_at(s.len() - 4);
    let sign = if negative && units != 0 { "-" } else { "" };
    (format!("{base}{digit}"), format!("{sign}{int}.{frac}"))
}

/// xorshift64*
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    /// A canonical decimal of up to `precision` digits with `scale` after the point.
    fn decimal(&mut self, precision: u32, scale: u32) -> String {
        let digits = 1 + (self.next() % u64::from(precision)) as u32;
        let mut n: u128 = 0;
        for _ in 0..digits {
            n = n * 10 + u128::from(self.next() % 10);
        }
        let neg = self.next().is_multiple_of(8) && n != 0;
        let s = format!("{n:0w$}", w = scale as usize + 1);
        let (int, frac) = s.split_at(s.len() - scale as usize);
        let body = if scale == 0 {
            int.to_string()
        } else {
            format!("{int}.{frac}")
        };
        if neg { format!("-{body}") } else { body }
    }
}

fn mcp(d: &Dispatcher, p: &Principal, s: &mut Session, msg: J) -> J {
    match obop_mcp::handle_payload(d, p, s, &msg) {
        Reply::Json(j) => j,
        Reply::Accepted => J::Null,
    }
}

fn mcp_session(d: &Dispatcher, p: &Principal) -> Session {
    let mut s = Session::new();
    let r = mcp(
        d,
        p,
        &mut s,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}),
    );
    assert!(r["result"].is_object(), "{r}");
    mcp(
        d,
        p,
        &mut s,
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    );
    s
}

fn tool(d: &Dispatcher, p: &Principal, s: &mut Session, name: &str, args: J) -> J {
    let r = mcp(
        d,
        p,
        s,
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":name,"arguments":args}}),
    );
    r["result"].clone()
}

#[tokio::test]
async fn rest_and_mcp_round_trip_ten_thousand_generated_decimals() {
    let f = fixture().await;
    let mut rng = Rng(0x00de_c1a7_e5ed);

    // MCP first, on a nearly empty store (the in-memory backend copies its state on every
    // write, so a single write after thousands of rows is slow; a bulk write is not): 300
    // values created and read back through tools/call.
    let mut s = mcp_session(&f.app.dispatcher, &f.who);
    for i in 0..300 {
        let want = (rng.decimal(12, 2), rng.decimal(10, 4), rng.decimal(24, 8));
        let made = tool(
            &f.app.dispatcher,
            &f.who,
            &mut s,
            "ledger_entry__create",
            json!({"memo": format!("mcp-{i}"), "amount": want.0, "ratio": want.1, "weight": want.2}),
        );
        assert_eq!(made["isError"], false, "{made}");
        let id = made["structuredContent"]["value"]["id"]
            .as_str()
            .unwrap_or_else(|| panic!("{made}"))
            .to_string();
        let back = tool(
            &f.app.dispatcher,
            &f.who,
            &mut s,
            "ledger_entry__get",
            json!({"id": id}),
        );
        let row = back["structuredContent"]["value"]
            .as_object()
            .unwrap_or_else(|| panic!("{back}"));
        assert_eq!(row["amount"], want.0.as_str());
        assert_eq!(row["ratio"], want.1.as_str());
        assert_eq!(row["weight"], want.2.as_str());
    }
    // MCP refuses a float with an error that names the field.
    let bad = tool(
        &f.app.dispatcher,
        &f.who,
        &mut s,
        "ledger_entry__create",
        json!({"memo": "x", "amount": 12.5}),
    );
    assert_eq!(bad["isError"], true);
    assert!(
        bad.to_string().contains("ledger_entry.amount must be"),
        "{bad}"
    );

    // REST: 8,700 values in nine bulk calls, read back page by page. With the 300 above and
    // 1,000 through the CLI import (the next test) that is 10,000 values on the wire.
    let mut expect: Vec<(String, String, String)> = Vec::new();
    for batch in 0..9 {
        let size = if batch == 8 { 700 } else { 1000 };
        let rows: Vec<J> = (0..size)
            .map(|i| {
                let row = (rng.decimal(12, 2), rng.decimal(10, 4), rng.decimal(24, 8));
                expect.push(row.clone());
                json!({"memo": format!("rest-{batch}-{i}"), "amount": row.0, "ratio": row.1, "weight": row.2})
            })
            .collect();
        let (s, b) = post(
            &f.r,
            Some(&f.ada),
            "/api/ledger_entry/bulk_create",
            json!({"rows": rows}),
        )
        .await;
        assert_eq!(
            (s, b["value"]["created"].clone()),
            (200, json!(size)),
            "{b}"
        );
    }
    let mut got = Vec::new();
    for offset in [0, 4500] {
        let (s, b) = post(
            &f.r,
            Some(&f.ada),
            "/api/ledger_entry/list",
            json!({"limit": 4500, "offset": offset, "order": [{"field": "memo"}]}),
        )
        .await;
        assert_eq!(s, 200);
        got.extend(b["value"].as_array().unwrap().clone());
    }
    assert_eq!(got.len(), 9000, "8,700 from REST and 300 from MCP");
    got.retain(|r| r["memo"].as_str().unwrap().starts_with("rest-"));
    assert_eq!(got.len(), 8700);
    got.sort_by(|a, b| a["memo"].as_str().cmp(&b["memo"].as_str()));
    let mut sorted: Vec<(usize, &(String, String, String))> = expect.iter().enumerate().collect();
    sorted.sort_by_key(|(i, _)| format!("rest-{}-{}", i / 1000, i % 1000));
    for (row, (_, want)) in got.iter().zip(sorted) {
        assert_eq!(row["amount"], want.0.as_str());
        assert_eq!(row["ratio"], want.1.as_str());
        assert_eq!(row["weight"], want.2.as_str());
    }
    // The wire form is a JSON string, never a number.
    assert!(got.iter().all(|r| r["amount"].is_string()));
}

#[test]
fn cli_import_and_call_round_trip_decimals_and_instants() {
    let dir = std::env::temp_dir().join(format!(
        "obop-kinds-cli-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = |n: &str| dir.join(n).to_str().unwrap().to_string();
    let db = format!("sqlite://{}", path("cli.db"));
    {
        let app = App::with_store(obop_commands::Store::open(&db).unwrap());
        app.installer.install(PACKAGE, "upload", "test").unwrap();
    }
    let o = crate::Opts {
        db: Some(db.clone()),
        token: None,
        demo: false,
        dev_auth: true,
        public_site: false,
        trust_proxy: false,
        admin_prefix: "/admin".into(),
    };
    let rows = || {
        obop_storage::open(&db)
            .unwrap()
            .list("ledger_entry")
            .unwrap()
    };
    // obop import: 1,000 generated rows in one file; ratio has one digit too many for scale 4 on
    // every fifth row and is rounded half up, because the field says so.
    let mut rng = Rng(99);
    let mut csv = String::from("memo,amount,ratio,weight,booked_at\n");
    let mut want = Vec::new();
    for i in 0..1000 {
        let (a, w) = (rng.decimal(12, 2), rng.decimal(24, 8));
        let base = rng.decimal(9, 4);
        let (given, ratio) = if i % 5 == 0 {
            let digit = rng.next() % 10;
            round_half_up_from_one_more_digit(&base, digit)
        } else {
            (base.clone(), base)
        };
        csv.push_str(&format!(
            "cli-{i},{a},{given},{w},2026-03-29T03:30:00+02:00\n"
        ));
        want.push((format!("cli-{i}"), a, ratio, w));
    }
    let file = path("rows.csv");
    std::fs::write(&file, &csv).unwrap();
    crate::import_cmd::run(
        &o,
        &crate::import_cmd::Args {
            model: "ledger_entry",
            file: &file,
            mode: None,
            dry_run: false,
            key: None,
        },
    )
    .unwrap();
    let stored = rows();
    assert_eq!(stored.len(), 1000);
    for (_, rec) in &stored {
        let i: usize = rec["memo"].as_str().unwrap()[4..].parse().unwrap();
        let (_, a, ratio, w) = &want[i];
        assert_eq!(rec["amount"], a.as_str());
        assert_eq!(rec["ratio"], ratio.as_str());
        assert_eq!(rec["weight"], w.as_str());
        assert_eq!(rec["booked_at"], "2026-03-29T01:30:00.000000Z");
    }
    // An import rounds where the field says so, and says no where it does not.
    let rounding = path("rounding.csv");
    std::fs::write(&rounding, "memo,ratio\nround,0.12345\n").unwrap();
    let go = |file: &str| {
        crate::import_cmd::run(
            &o,
            &crate::import_cmd::Args {
                model: "ledger_entry",
                file,
                mode: None,
                dry_run: false,
                key: None,
            },
        )
    };
    assert!(go(&rounding).is_ok());
    let round = rows()
        .into_iter()
        .find(|(_, r)| r["memo"] == "round")
        .unwrap()
        .1;
    assert_eq!(round["ratio"], "0.1235", "half up");
    let strict = path("strict.csv");
    std::fs::write(&strict, "memo,amount\nstrict,0.125\n").unwrap();
    assert!(go(&strict).is_err(), "amount has no rounding mode");
    assert_eq!(rows().len(), 1001);
    // obop call: the values are text on the command line and keep their exact form.
    for (i, a) in ["12.5", "-0.07", "999999999.99"].iter().enumerate() {
        crate::call(
            &o,
            "ledger_entry",
            "create",
            &[
                &format!("memo=call-{i}"),
                &format!("amount={a}"),
                "booked_at=2026-10-25T00:30:00Z",
            ],
        )
        .unwrap();
    }
    let called: Vec<_> = rows()
        .into_iter()
        .filter(|(_, r)| r["memo"].as_str().unwrap().starts_with("call-"))
        .collect();
    let mut amounts: Vec<String> = called
        .iter()
        .map(|(_, r)| r["amount"].as_str().unwrap().to_string())
        .collect();
    amounts.sort();
    assert_eq!(amounts, ["-0.07", "12.50", "999999999.99"]);
    // The aggregate options of search are reachable from the command line: an average of a
    // decimal at three digits per ISO week of the instance zone, and a sum per year.
    let (answer, _) = crate::call_json(
        &o,
        "ledger_entry",
        "search",
        &[r#"aggregate={"group_by":[{"field":"booked_at","bucket":"iso_week"}],"measures":[{"op":"count"},{"op":"avg","field":"amount","scale":3},{"op":"sum","field":"amount"}]}"#],
    )
    .unwrap();
    let groups = answer["groups"].as_array().unwrap();
    // The thousand imported rows are on Sunday 2026-03-29 (ISO week 13 in every zone), the three
    // called ones on Sunday 2026-10-25 (week 43), and the rounding row has no instant.
    assert_eq!(groups.len(), 3, "{answer}");
    let week13 = groups
        .iter()
        .find(|g| g["key"]["booked_at"] == "2026-W13")
        .unwrap_or_else(|| panic!("{answer}"));
    assert_eq!(week13["count"], 1000);
    assert_eq!(
        groups
            .iter()
            .find(|g| g["key"]["booked_at"] == "2026-W43")
            .map(|g| g["count"].clone()),
        Some(3.into())
    );
    let avg = week13["measures"]["avg:amount"].as_str().unwrap();
    assert_eq!(avg.split_once('.').map(|(_, f)| f.len()), Some(3), "{avg}");
    assert!(week13["measures"]["sum:amount"].is_string());
    let (yearly, _) = crate::call_json(
        &o,
        "ledger_entry",
        "search",
        &[r#"aggregate={"group_by":[{"field":"booked_at","bucket":"year"}],"measures":[{"op":"max","field":"amount"}]}"#],
    )
    .unwrap();
    assert_eq!(yearly["groups"][0]["key"]["booked_at"], "2026");
    assert!(
        crate::call_json(
            &o,
            "ledger_entry",
            "search",
            &[r#"aggregate={"group_by":[],"measures":[{"op":"avg","field":"amount"},{"op":"avg","field":"amount"}]}"#],
        )
        .is_err(),
        "a measure asked for twice is refused"
    );
    assert!(
        crate::call_json(&o, "ledger_entry", "search", &["aggregate={nope"]).is_err(),
        "text that is not JSON is not an aggregate"
    );
    // And a bad one is an error, with nothing written.
    assert!(crate::call(&o, "ledger_entry", "create", &["memo=bad", "amount=1e3"]).is_err());
    assert!(
        crate::call(
            &o,
            "ledger_entry",
            "create",
            &["memo=bad", "booked_at=2026-02-30T10:00:00Z"]
        )
        .is_err()
    );
    assert_eq!(rows().len(), 1004);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn rest_refuses_floats_and_hostile_text_with_a_400_that_names_the_field() {
    let f = fixture().await;
    let create = |body: J| {
        let (r, t) = (f.r.clone(), f.ada.clone());
        async move { post(&r, Some(&t), "/api/ledger_entry/create", body).await }
    };
    // A float and an integer are both JSON numbers: refused, however they are written.
    for (body, want) in [
        (
            json!({"memo": "x", "amount": 12.5}),
            "ledger_entry.amount must be",
        ),
        (
            json!({"memo": "x", "amount": 12}),
            "ledger_entry.amount must be",
        ),
        (
            json!({"memo": "x", "amount": 1e3}),
            "ledger_entry.amount must be",
        ),
        (json!({"memo": "x", "amount": "1e3"}), "exponent"),
        (json!({"memo": "x", "amount": "NaN"}), "NaN"),
        (json!({"memo": "x", "amount": "0x10"}), "hexadecimal"),
        (json!({"memo": "x", "amount": " 1.5"}), "spaces"),
        (json!({"memo": "x", "amount": "1,5"}), "comma"),
        (
            json!({"memo": "x", "amount": "1.005"}),
            "3 fractional digits",
        ),
        (
            json!({"memo": "x", "booked_at": "2026-03-29T01:30:00"}),
            "no offset",
        ),
        (
            json!({"memo": "x", "booked_at": "2026-02-30T10:00:00Z"}),
            "calendar date",
        ),
        (
            json!({"memo": "x", "booked_at": "2026-13-01T00:00:00Z"}),
            "calendar date",
        ),
        (
            json!({"memo": "x", "booked_at": 1_700_000_000}),
            "RFC 3339 string",
        ),
    ] {
        let (s, b) = create(body.clone()).await;
        assert_eq!(s, 400, "{body}: {b}");
        let e = b["error"].as_str().unwrap();
        assert!(e.contains(want), "{body}: {e}");
    }
    // The good ones, in loose forms, come back canonical.
    let (s, b) =
        create(json!({"memo": "ok", "amount": "9.5", "booked_at": "2026-03-29T03:30:00+02:00"}))
            .await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(b["value"]["amount"], "9.50");
    assert_eq!(b["value"]["booked_at"], "2026-03-29T01:30:00.000000Z");
    assert_eq!(
        b["value"]["share"], "3.1667",
        "computed: 9.50 / 3 at four digits"
    );
    // A filter takes strings, and compares as numbers.
    let (_, b) = post(
        &f.r,
        Some(&f.ada),
        "/api/ledger_entry/create",
        json!({"memo": "big", "amount": "10.25"}),
    )
    .await;
    assert!(b["value"]["id"].is_string());
    let (s, b) = post(
        &f.r,
        Some(&f.ada),
        "/api/ledger_entry/search",
        json!({"filter": [{"field": "amount", "op": "gt", "value": "9.7"}]}),
    )
    .await;
    assert_eq!(s, 200);
    assert_eq!(
        b["value"]["total"], 1,
        "10.25 > 9.7 but 9.50 is not: numeric, not lexical"
    );
    let (s, b) = post(
        &f.r,
        Some(&f.ada),
        "/api/ledger_entry/search",
        json!({"filter": [{"field": "amount", "op": "gt", "value": 9.7}]}),
    )
    .await;
    assert_eq!(s, 400, "{b}");
}

#[tokio::test]
async fn openapi_and_mcp_describe_decimals_as_strings_and_instants_as_date_time() {
    let f = fixture().await;
    let (s, doc) = send(&f.r, "GET", Some(&f.ada), "/api/openapi.json", json!({})).await;
    assert_eq!(s, 200);
    let props = &doc["components"]["schemas"]["ledger_entry"]["properties"];
    let amount = &props["amount"];
    assert_eq!(amount["type"], "string");
    assert_eq!(amount["pattern"], "^-?(0|[1-9][0-9]{0,9})(\\.[0-9]{1,2})?$");
    assert!(
        amount["description"]
            .as_str()
            .unwrap()
            .contains("never a JSON number")
    );
    assert_eq!(
        props["ratio"]["pattern"],
        "^-?(0|[1-9][0-9]{0,5})(\\.[0-9]{1,4})?$"
    );
    assert!(
        props["ratio"]["description"]
            .as_str()
            .unwrap()
            .contains("round with half_up")
    );
    assert_eq!(
        props["weight"]["pattern"],
        "^-?(0|[1-9][0-9]{0,15})(\\.[0-9]{1,8})?$"
    );
    assert_eq!(props["booked_at"]["type"], "string");
    assert_eq!(props["booked_at"]["format"], "date-time");
    assert_eq!(
        props["share"]["readOnly"], true,
        "a computed decimal is read-only"
    );
    // That the pattern is the language of the parser is checked against the parser itself, over
    // every shape, in obop-values (pattern_tests).
    // MCP: the same shape in the tool schema.
    let tools = obop_mcp::schema::tools(&f.app.dispatcher, &f.who);
    let create = tools
        .iter()
        .find(|t| t["name"] == "ledger_entry__create")
        .expect("ledger_entry__create");
    let p = &create["inputSchema"]["properties"];
    assert_eq!(p["amount"]["type"], "string");
    assert_eq!(
        p["amount"]["pattern"],
        "^-?(0|[1-9][0-9]{0,9})(\\.[0-9]{1,2})?$"
    );
    assert!(
        p["amount"]["description"]
            .as_str()
            .unwrap()
            .contains("STRING")
    );
    assert_eq!(p["booked_at"]["format"], "date-time");
    assert!(p["share"].is_null(), "computed fields are not inputs");
    // The filter and aggregate schema know buckets, zones and avg.
    let search = tools
        .iter()
        .find(|t| t["name"] == "ledger_entry__search")
        .unwrap();
    let text = search["inputSchema"].to_string();
    for needle in ["iso_week", "\"zone\"", "\"avg\"", "half_even"] {
        assert!(text.contains(needle), "{needle}");
    }
}

/// Every example the surfaces print is a value the server takes, in the spelling it answers with;
/// and a row of bulk_create allows the extra digits that the field rounds away.
#[tokio::test]
async fn openapi_and_mcp_examples_are_values_the_server_takes_and_bulk_rows_allow_rounding() {
    let f = fixture().await;
    f.app.installer.install(SHAPES, "upload", "test").unwrap();
    let taken = |text: &str, p: u8, s: u8, what: &str| {
        let d = obop_values::Decimal::parse_for(text, p, s, None)
            .unwrap_or_else(|e| panic!("{what}: {text:?} is refused: {e}"));
        assert_eq!(d.to_string(), text, "{what}: written the way answers are");
    };
    let (s, doc) = send(&f.r, "GET", Some(&f.ada), "/api/openapi.json", json!({})).await;
    assert_eq!(s, 200);
    let props = &doc["components"]["schemas"]["shapes_box"]["properties"];
    let tools = obop_mcp::schema::tools(&f.app.dispatcher, &f.who);
    let tool = |name: &str| {
        tools
            .iter()
            .find(|t| t["name"] == name)
            .unwrap_or_else(|| panic!("{name}"))
    };
    let create = &tool("shapes_box__create")["inputSchema"]["properties"];
    let rows_of = |schema: &J| schema["properties"]["rows"]["items"]["properties"].clone();
    let bulk_openapi = rows_of(
        &doc["paths"]["/api/shapes_box/bulk_create"]["post"]["requestBody"]["content"]["application/json"]
            ["schema"],
    );
    let bulk_mcp = rows_of(&tool("shapes_box__bulk_create")["inputSchema"]);
    for (name, p, s, rounds) in SHAPE_FIELDS {
        let what = format!("{name} decimal({p}, {s})");
        let example = props[name]["example"]
            .as_str()
            .unwrap_or_else(|| panic!("{what}"));
        taken(example, p, s, &format!("OpenAPI {what}"));
        // The MCP description quotes the example it was built from.
        let description = create[name]["description"].as_str().unwrap();
        assert!(
            description.contains(&format!("\"{example}\"")),
            "{what}: {description}"
        );
        // The row of a bulk call: the schema of create, or any digits where the field rounds.
        for (surface, rows) in [("OpenAPI", &bulk_openapi), ("MCP", &bulk_mcp)] {
            let pattern = rows[name]["pattern"].as_str().unwrap();
            assert_eq!(
                pattern.contains("+)?$"),
                rounds,
                "{surface} bulk row of {what}: {pattern}"
            );
            if !rounds {
                assert_eq!(pattern, props[name]["pattern"].as_str().unwrap());
            }
        }
    }
    // The shapes that used to print a spelling the server refuses.
    assert_eq!(props["whole"]["example"], "12");
    assert_eq!(props["fraction"]["example"], "0.33");
    assert_eq!(props["narrow"]["example"], "1.33");
    // The csv text says what a cell may hold now that decimals and instants exist.
    let csv = bulk_csv_description(&tool("shapes_box__bulk_create")["inputSchema"]);
    for needle in ["decimal", "datetime", "rounds on import"] {
        assert!(csv.contains(needle), "MCP csv: {needle} in {csv}");
    }
    assert!(!csv.contains("Whole numbers only"), "{csv}");
    let csv = bulk_csv_description(
        &doc["paths"]["/api/shapes_box/bulk_create"]["post"]["requestBody"]["content"]["application/json"]
            ["schema"],
    );
    for needle in ["decimal", "datetime", "rounds on import"] {
        assert!(csv.contains(needle), "OpenAPI csv: {needle} in {csv}");
    }
}

fn bulk_csv_description(schema: &J) -> String {
    schema["properties"]["csv"]["description"]
        .as_str()
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn meta_reports_the_shape_of_each_decimal_and_the_tz_database() {
    let f = fixture().await;
    let (s, meta) = send(&f.r, "GET", Some(&f.ada), "/api/meta", json!({})).await;
    assert_eq!(s, 200);
    // The pinned IANA database and the platform zone.
    assert_eq!(meta["time"]["zone"], "UTC");
    let v = meta["time"]["tzdb_version"].as_str().unwrap();
    assert!(v.len() >= 5 && v.starts_with("20"), "{v}");
    let model = meta["modules"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|m| m["models"].as_array().unwrap())
        .find(|m| m["name"] == "ledger_entry")
        .unwrap();
    let field = |n: &str| {
        model["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["name"] == n)
            .unwrap()
            .clone()
    };
    let a = field("amount");
    assert_eq!(
        (
            a["kind"].clone(),
            a["precision"].clone(),
            a["scale"].clone()
        ),
        (json!("decimal"), json!(12), json!(2))
    );
    assert!(a.get("round").is_none());
    assert_eq!(field("ratio")["round"], "half_up");
    assert_eq!(field("booked_at")["kind"], "datetime");
    assert_eq!(field("share")["computed"], true);
    // A release that ships another database, or an instance in another zone, shows it here.
    f.app.dispatcher.set_zones(
        obop_values::Zones::new(
            Arc::new(
                obop_values::ZoneDb::fixture(
                    "2026-fixture",
                    &[("Europe/Berlin", "CET-1CEST,M3.5.1,M10.5.0/3")],
                )
                .unwrap(),
            ),
            "Europe/Berlin",
        )
        .unwrap(),
    );
    let (_, meta) = send(&f.r, "GET", Some(&f.ada), "/api/meta", json!({})).await;
    assert_eq!(meta["time"]["zone"], "Europe/Berlin");
    assert_eq!(meta["time"]["tzdb_version"], "2026-fixture");
}

#[test]
fn cli_modules_check_accepts_the_kinds_and_explains_mistakes() {
    let builtin = App::new().modules;
    let check = |text: &str| obop_pkg::check_package(text, &builtin, false).errors;
    assert!(check(PACKAGE).is_empty(), "{:?}", check(PACKAGE));
    let with = |from: &str, to: &str| PACKAGE.replace(from, to);
    let first = |errors: Vec<String>| errors.into_iter().next().unwrap_or_default();
    for (text, want) in [
        (with("scale = 2", "scale = 13"), "0 to 12"),
        (
            with("precision = 12\n  scale = 2", "precision = 39\n  scale = 2"),
            "from 1 to 38",
        ),
        (
            with("round = \"half_up\"", "round = \"up\""),
            "half_even, half_up or down",
        ),
        (
            with(
                "  kind = \"datetime\"",
                "  kind = \"datetime\"\n  scale = 2",
            ),
            "only valid for kind \"decimal\"",
        ),
        (
            PACKAGE.to_string()
                + "\n  [[model.field]]\n  name = \"half\"\n  description = \"x\"\n  class = \"none\"\n  kind = \"money\"\n  computed = \"amount / 2\"\n",
            "is a decimal number, but the field is money",
        ),
        (
            PACKAGE.to_string()
                + "\n  [[model.field]]\n  name = \"later\"\n  description = \"x\"\n  class = \"none\"\n  kind = \"decimal\"\n  precision = 10\n  scale = 2\n  computed = \"booked_at + 1\"\n",
            "must be a number",
        ),
        (
            PACKAGE.to_string()
                + "\n  [[model.field]]\n  name = \"flag\"\n  description = \"x\"\n  class = \"none\"\n  kind = \"bool\"\n  computed = \"booked_at < \\\"2026-03-29\\\"\"\n",
            "cannot order",
        ),
    ] {
        let e = first(check(&text));
        assert!(e.contains(want), "{want}: {e}");
    }
    // The command itself reads files, prints ok or FAIL and exits accordingly.
    let dir = std::env::temp_dir().join(format!("obop-kinds-check-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let good = dir.join("ledger.toml");
    let bad = dir.join("broken.toml");
    std::fs::write(&good, PACKAGE).unwrap();
    std::fs::write(&bad, with("scale = 2", "scale = 13")).unwrap();
    let (g, b) = (good.to_str().unwrap(), bad.to_str().unwrap());
    assert!(crate::modules_check_files_with(&[g], false, false).is_ok());
    assert!(crate::modules_check_files_with(&[g, b], false, false).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}
