//! `stock_count.apply`: a count ends at the counted figure, whatever moved since the person
//! looked, and leaves no draft difference behind.

use crate::app::App;
use obop_commands::{ADMIN_GROUP, CallError, Principal, Surface, Value, Vault};
use obop_pkg::{Bundled, Marketplace};
use serde_json::{Value as J, json};

fn run(app: &App, p: &Principal, cmd: &str, input: J) -> Result<J, String> {
    match app.dispatcher.call(
        Surface::Rest,
        p,
        &mut Vault::new(),
        cmd,
        &Value::from_json(&input),
    ) {
        Ok(r) => Ok(r.value.to_json()),
        Err(CallError::Handler(m)) => Err(m),
        Err(e) => Err(format!("{e:?}")),
    }
}

fn ok(app: &App, p: &Principal, cmd: &str, input: J) -> J {
    run(app, p, cmd, input).unwrap_or_else(|e| panic!("{cmd}: {e}"))
}

fn id(v: &J) -> String {
    v["id"].as_str().unwrap().to_string()
}

struct Shelf {
    app: App,
    keeper: Principal,
    product: String,
    shelf: String,
    item: String,
}

/// A shelf holding 36 units of one product.
fn shelf() -> Shelf {
    let app = App::new();
    app.installer
        .install(&Bundled.fetch("inventory").unwrap(), "marketplace", "test")
        .unwrap();
    let root = Principal::user("root", &[ADMIN_GROUP]);
    let keeper = Principal::user("kim", &["inventory.user", "inventory.manager"]);
    let product = id(&ok(&app, &root, "product.create", json!({"name": "Drill"})));
    let wh = id(&ok(
        &app,
        &keeper,
        "inventory_warehouse.create",
        json!({"name": "Hamburg", "code": "HH1"}),
    ));
    let shelf = id(&ok(
        &app,
        &keeper,
        "inventory_location.create",
        json!({"name": "Shelf A", "warehouse_id": wh, "kind": "internal"}),
    ));
    let item = id(&ok(
        &app,
        &keeper,
        "inventory_item.create",
        json!({"product_id": product, "location_id": shelf}),
    ));
    let s = Shelf {
        app,
        keeper,
        product,
        shelf,
        item,
    };
    s.moved(36, true);
    s
}

impl Shelf {
    fn moved(&self, by: i64, into: bool) {
        let side = if into {
            "to_location_id"
        } else {
            "from_location_id"
        };
        let mut m = json!({"product_id": self.product, "quantity": by});
        m[side] = json!(self.shelf);
        let mv = id(&ok(&self.app, &self.keeper, "inventory_move.create", m));
        ok(
            &self.app,
            &self.keeper,
            "inventory_move.validate",
            json!({ "id": mv }),
        );
    }

    fn on_hand(&self) -> i64 {
        ok(
            &self.app,
            &self.keeper,
            "inventory_item.get",
            json!({ "id": self.item }),
        )["quantity"]
            .as_i64()
            .unwrap()
    }

    fn moves(&self) -> Vec<J> {
        ok(
            &self.app,
            &self.keeper,
            "inventory_move.list",
            json!({ "limit": 100 }),
        )["items"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    }
}

#[test]
fn a_count_ends_at_the_counted_figure_even_when_stock_moved_since_it_was_seen() {
    let s = shelf();
    // The person saw 36 and counts 40; meanwhile a delivery of 10 left (stock 26).
    s.moved(10, false);
    let out = ok(
        &s.app,
        &s.keeper,
        "stock_count.apply",
        json!({"item": s.item, "counted": 40}),
    );
    assert_eq!(s.on_hand(), 40);
    assert_eq!(out["before"], 26);
    assert_eq!(out["difference"], 14);
    assert_eq!(out["quantity"], 40);
    assert_eq!(out["written"], true);
    let mv = ok(
        &s.app,
        &s.keeper,
        "inventory_move.get",
        json!({ "id": out["move_id"] }),
    );
    assert_eq!(mv["kind"], "adjustment");
    assert_eq!(mv["state"], "done");
    assert_eq!(mv["quantity"], 14);
    // A lower count takes units out; an equal count books nothing.
    let out = ok(
        &s.app,
        &s.keeper,
        "stock_count.apply",
        json!({"item": s.item, "counted": 33}),
    );
    assert_eq!((out["difference"].as_i64(), s.on_hand()), (Some(-7), 33));
    let n = s.moves().len();
    let out = ok(
        &s.app,
        &s.keeper,
        "stock_count.apply",
        json!({"item": s.item, "counted": 33}),
    );
    assert_eq!(out["written"], false);
    assert_eq!(s.moves().len(), n);
}

#[test]
fn a_count_leaves_no_draft_difference_and_a_dry_run_writes_nothing() {
    let s = shelf();
    let n = s.moves().len();
    // Someone who may not validate a move cannot apply a count (the command has the group of
    // inventory_move.validate); nothing is left behind.
    let clerk = Principal::user("cleo", &["inventory.user"]);
    let e = run(
        &s.app,
        &clerk,
        "stock_count.apply",
        json!({"item": s.item, "counted": 40}),
    )
    .unwrap_err();
    assert!(e.starts_with("Forbidden"), "{e}");
    assert_eq!(s.moves().len(), n);
    // A dry run says what a count would book and writes nothing.
    let out = ok(
        &s.app,
        &s.keeper,
        "stock_count.apply",
        json!({"item": s.item, "counted": 30, "dry_run": true}),
    );
    assert_eq!(
        (out["difference"].as_i64(), out["written"].as_bool()),
        (Some(-6), Some(false))
    );
    assert_eq!((s.on_hand(), s.moves().len()), (36, n));
    // The command line sends `dry_run=true` as text: still a dry run.
    let out = ok(
        &s.app,
        &s.keeper,
        "stock_count.apply",
        json!({"item": s.item, "counted": 30, "dry_run": "true"}),
    );
    assert_eq!(out["written"], false);
    assert_eq!((s.on_hand(), s.moves().len()), (36, n));
    // Negative counts are refused in words.
    let e = run(
        &s.app,
        &s.keeper,
        "stock_count.apply",
        json!({"item": s.item, "counted": -1}),
    )
    .unwrap_err();
    assert!(e.contains("0 or more"), "{e}");
}

#[test]
fn dry_run_text_is_read_without_regard_to_case_and_unknown_text_is_refused() {
    let s = shelf();
    let n = s.moves().len();
    for dry in ["TRUE", "True", " yes ", "Yes", "1"] {
        let out = ok(
            &s.app,
            &s.keeper,
            "stock_count.apply",
            json!({"item": s.item, "counted": 40, "dry_run": dry}),
        );
        assert_eq!(out["written"], false, "dry_run={dry:?}");
        assert_eq!((s.on_hand(), s.moves().len()), (36, n), "dry_run={dry:?}");
    }
    // A word that is neither yes nor no is refused, never read as "count for real".
    for dry in ["maybe", "ture", "2"] {
        let e = run(
            &s.app,
            &s.keeper,
            "stock_count.apply",
            json!({"item": s.item, "counted": 40, "dry_run": dry}),
        )
        .unwrap_err();
        assert!(e.contains("dry_run"), "{dry:?}: {e}");
        assert_eq!((s.on_hand(), s.moves().len()), (36, n), "dry_run={dry:?}");
    }
    // A no word is a real count.
    let out = ok(
        &s.app,
        &s.keeper,
        "stock_count.apply",
        json!({"item": s.item, "counted": 40, "dry_run": "FALSE"}),
    );
    assert_eq!((out["written"].as_bool(), s.on_hand()), (Some(true), 40));
}

#[test]
fn a_count_too_large_for_the_difference_is_refused_in_words() {
    let s = shelf();
    s.moved(46, false);
    assert_eq!(s.on_hand(), -10);
    let n = s.moves().len();
    for counted in [i64::MAX, 9_007_199_254_740_992] {
        let e = run(
            &s.app,
            &s.keeper,
            "stock_count.apply",
            json!({"item": s.item, "counted": counted}),
        )
        .unwrap_err();
        assert!(e.contains("too large"), "{counted}: {e}");
    }
    assert_eq!((s.on_hand(), s.moves().len()), (-10, n));
}

#[test]
fn a_count_where_the_product_is_kept_in_lots_is_refused_before_any_move_is_made() {
    let s = shelf();
    ok(
        &s.app,
        &s.keeper,
        "inventory_item.create",
        json!({"product_id": s.product, "location_id": s.shelf, "lot": "L-1"}),
    );
    let n = s.moves().len();
    let e = run(
        &s.app,
        &s.keeper,
        "stock_count.apply",
        json!({"item": s.item, "counted": 40}),
    )
    .unwrap_err();
    assert!(e.contains("lot"), "{e}");
    assert!(!e.contains("effect"), "{e}");
    // No cancelled move is left behind.
    assert_eq!((s.on_hand(), s.moves().len()), (36, n));
}

#[test]
fn a_manager_without_the_inventory_user_group_is_told_which_group_is_missing() {
    let s = shelf();
    let boss = Principal::user("bo", &["inventory.manager"]);
    let e = run(
        &s.app,
        &boss,
        "stock_count.apply",
        json!({"item": s.item, "counted": 40}),
    )
    .unwrap_err();
    assert!(e.contains("inventory.user"), "{e}");
    assert!(!e.contains("import"), "{e}");
}

/// Q5: an item kept in a lot is counted through a move that names the lot, and only that item
/// changes.
#[test]
fn an_item_kept_in_a_lot_is_counted_and_the_other_lots_stay() {
    let s = shelf();
    let lot = |name: &str| {
        id(&ok(
            &s.app,
            &s.keeper,
            "inventory_item.create",
            json!({"product_id": s.product, "location_id": s.shelf, "lot": name}),
        ))
    };
    let (l1, l2) = (lot("L-1"), lot("L-2"));
    let r = ok(
        &s.app,
        &s.keeper,
        "stock_count.apply",
        json!({"item": l2, "counted": 12}),
    );
    assert_eq!(r["quantity"], json!(12), "{r}");
    let qty = |item: &str| {
        ok(&s.app, &s.keeper, "inventory_item.get", json!({"id": item}))["quantity"]
            .as_i64()
            .unwrap_or(0)
    };
    assert_eq!((qty(&l1), qty(&l2), s.on_hand()), (0, 12, 36));
    let mv = ok(
        &s.app,
        &s.keeper,
        "inventory_move.get",
        json!({"id": r["move_id"]}),
    );
    assert_eq!(mv["lot"], json!("L-2"));
    assert_eq!(mv["state"], json!("done"));
    // Counted down again, through the source side.
    let r = ok(
        &s.app,
        &s.keeper,
        "stock_count.apply",
        json!({"item": l2, "counted": 5}),
    );
    assert_eq!(
        (qty(&l1), qty(&l2), r["difference"].clone()),
        (0, 5, json!(-7))
    );
}
