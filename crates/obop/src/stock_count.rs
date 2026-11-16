//! The typed command `stock_count.apply`: set a stock item to the figure a person counted.
//!
//! A count is the figure on the shelf, not a difference. The difference is worked out here, from
//! the item's quantity at the moment the count is applied, and booked at once as a stock count
//! move of the inventory package (`inventory_move`, kind `adjustment`, origin "Stock count") that
//! is validated in the same call. Nothing is left as a draft: a draft difference would be added
//! to whatever the stock is when someone validates it later, and the stock would not end at the
//! counted figure. Applying a count therefore needs the group of `inventory_move.validate`
//! (`inventory.manager`), and an agent may only dry-run it (validating a move needs a person's
//! approval). The call reads the item again afterwards and says when the stock is not the counted
//! figure (a move validated by someone else between the two steps).

use crate::sie::{Env, call};
use obop_commands::{Actor, Dispatcher, Principal, Store, Value};
use obop_registry::{CommandDef, FieldDef, FieldKind, ModelDef, Module};
use obop_types::DataClass;
use serde_json::{Value as J, json};
use std::sync::{Arc, Weak};

const MODEL: &str = "stock_count";
/// The group of `inventory_move.validate`.
const GROUP: &str = "inventory.manager";
/// The largest count: the largest whole number a JSON client reads exactly (2^53 - 1).
const MAX_COUNT: i64 = 9_007_199_254_740_991;
const DRY_RUN_WORDS: &str =
    "dry_run must be true or false (also yes, no, 1 or 0); nothing was counted";

pub fn module() -> Module {
    let f = |name, description| FieldDef::new(name, description, DataClass::None, false, GROUP);
    Module {
        id: "stock_count".into(),
        name: "Stock count".into(),
        description: "Sets a stock item to the counted figure through a validated stock count move"
            .into(),
        models: vec![ModelDef {
            name: MODEL,
            description: "`apply` sets an `inventory_item` to the number of units counted: the difference to the stock at that moment is booked as an `inventory_move` of kind adjustment and validated at once. Needs the inventory package and inventory.manager; an agent may only dry-run. Rows of this model are never stored",
            read_group: GROUP,
            write_group: GROUP,
            access: None,
            fields: vec![
                f(
                    "item",
                    "Input: the id of the stock item (inventory_item) that was counted",
                ),
                f(
                    "counted",
                    "Input: the units counted, a whole number of 0 or more",
                )
                .kind(FieldKind::Number),
                f(
                    "dry_run",
                    "Input: only work out the difference, write nothing",
                )
                .kind(FieldKind::Bool),
                f("before", "Answer: the units on hand before the count").kind(FieldKind::Number),
                f(
                    "difference",
                    "Answer: counted minus before; booked as the stock count move",
                )
                .kind(FieldKind::Number),
                f("quantity", "Answer: the units on hand after the count").kind(FieldKind::Number),
                f("ok", "Answer: whether the count was worked out").kind(FieldKind::Bool),
                f("written", "Answer: whether a move was booked").kind(FieldKind::Bool),
                f(
                    "move_id",
                    "Answer: the validated stock count move, when one was booked",
                ),
                f("notes", "Answer: remarks (a list)"),
            ],
            commands: vec![CommandDef::new(
                "apply",
                "Set the stock item (item) to the units counted (counted): the difference to the stock at this moment is booked as a stock count move (inventory_move, kind adjustment) and validated at once, so the stock ends at the counted figure. A count equal to the stock books nothing. dry_run only works out the difference. Needs inventory.manager; an agent may only dry-run",
                GROUP,
            )],
            ext: Default::default(),
        }],
        ..Module::default()
    }
}

/// What a count did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Counted {
    pub before: i64,
    pub difference: i64,
    pub quantity: i64,
    pub move_id: Option<String>,
    pub notes: Vec<String>,
}

fn int(v: &J) -> i64 {
    v.as_i64().unwrap_or(0)
}

fn text(v: &J, k: &str) -> String {
    v[k].as_str().unwrap_or_default().to_string()
}

/// Apply a count as `p`. Every refusal is in words and leaves nothing behind.
pub fn apply(
    env: Env<'_>,
    p: &Principal,
    item: &str,
    counted: i64,
    dry_run: bool,
) -> Result<Counted, String> {
    if item.is_empty() {
        return Err("item is required: the id of the stock item that was counted".into());
    }
    if counted < 0 {
        return Err("counted must be 0 or more".into());
    }
    if !p.allows(GROUP) {
        return Err(format!(
            "a count sets the stock at once, which is the validate action of inventory_move and needs the {GROUP} group; ask a stock manager, or make an adjustment move for one to validate"
        ));
    }
    let row = call(env, p, "inventory_item.get", json!({ "id": item }))?;
    let (product, location) = (text(&row, "product_id"), text(&row, "location_id"));
    if product.is_empty() || location.is_empty() {
        return Err(
            "the stock item has no product or no location, so there is nothing to count".into(),
        );
    }
    let before = int(&row["quantity"]);
    let difference = counted
        .checked_sub(before)
        .filter(|_| counted <= MAX_COUNT)
        .ok_or_else(|| format!("counted is too large: a count may be at most {MAX_COUNT} units"))?;
    let mut out = Counted {
        before,
        difference,
        quantity: before,
        move_id: None,
        notes: Vec::new(),
    };
    if difference == 0 || dry_run {
        if dry_run {
            out.quantity = counted;
        }
        return Ok(out);
    }
    let mut mv = json!({
        "kind": "adjustment",
        "origin": "Stock count",
        "product_id": product,
        "quantity": difference.abs(),
    });
    mv[if difference > 0 {
        "to_location_id"
    } else {
        "from_location_id"
    }] = json!(location);
    let made = call(env, p, "inventory_move.create", mv)?;
    let mid = made["id"]
        .as_str()
        .ok_or("inventory_move.create gave no id")?
        .to_string();
    if let Err(e) = call(env, p, "inventory_move.validate", json!({ "id": mid })) {
        // A draft difference is what a count must not leave behind; take it back.
        let _ = call(env, p, "inventory_move.cancel", json!({ "id": mid }));
        return Err(format!(
            "the stock count move could not be validated, so it was cancelled and the stock is unchanged: {e}"
        ));
    }
    let after = int(&call(env, p, "inventory_item.get", json!({ "id": item }))?["quantity"]);
    if after != counted {
        out.notes.push(format!(
            "the stock is {after}, not {counted}: another move changed it while the count was booked; count again"
        ));
    }
    out.quantity = after;
    out.move_id = Some(mid);
    Ok(out)
}

pub fn register(d: &Arc<Dispatcher>, store: &Store) {
    let m = module();
    let (weak, st): (Weak<Dispatcher>, Store) = (Arc::downgrade(d), store.clone());
    d.register_with_actor(
        &m,
        MODEL,
        "apply",
        Box::new(move |input| {
            let Value::Map(map) = input else {
                return Err("item and counted are required".into());
            };
            let item = match map.get("item") {
                Some(Value::Text(t)) => t.clone(),
                _ => String::new(),
            };
            let counted = match map.get("counted") {
                Some(Value::Int(n)) => *n,
                Some(Value::Text(t)) if t.trim().parse::<i64>().is_ok() => {
                    t.trim().parse::<i64>().unwrap_or_default()
                }
                _ => return Err("counted is required: the units counted, a whole number".into()),
            };
            // `obop call ... dry_run=true` sends text (docs/ISSUES.md Q9): a dry run must never
            // write, so the text counts too, in any case; a word that is neither yes nor no is
            // refused rather than read as a real count.
            let dry = match map.get("dry_run") {
                None | Some(Value::Null) => false,
                Some(Value::Bool(b)) => *b,
                Some(Value::Int(0)) => false,
                Some(Value::Int(1)) => true,
                Some(Value::Text(t)) => match t.trim().to_ascii_lowercase().as_str() {
                    "true" | "1" | "yes" => true,
                    "false" | "0" | "no" => false,
                    _ => return Err(DRY_RUN_WORDS.into()),
                },
                _ => return Err(DRY_RUN_WORDS.into()),
            };
            let a = Actor::from_input(input);
            if a.agent && !dry {
                return Err("an agent may only dry-run a stock count (dry_run=true); validating the move needs a person, so ask the person to apply it".into());
            }
            let groups: Vec<&str> = a.groups.iter().map(String::as_str).collect();
            let p = Principal::user(&a.caller, &groups);
            let d = weak.upgrade().ok_or("the dispatcher is gone")?;
            let c = apply(Env { d: &d, store: &st }, &p, &item, counted, dry)?;
            let mut out = vec![
                ("ok", Value::Bool(true)),
                ("written", Value::Bool(c.move_id.is_some())),
                ("before", Value::Int(c.before)),
                ("counted", Value::Int(counted)),
                ("difference", Value::Int(c.difference)),
                ("quantity", Value::Int(c.quantity)),
                (
                    "notes",
                    Value::List(c.notes.iter().map(|s| Value::text(s)).collect()),
                ),
            ];
            if let Some(m) = c.move_id {
                out.push(("move_id", Value::text(&m)));
            }
            Ok(Value::Map(out.into_iter().map(|(k, v)| (k.to_string(), v)).collect()))
        }),
    );
}
