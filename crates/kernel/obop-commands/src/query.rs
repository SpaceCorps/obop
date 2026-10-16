//! Server-side list queries (D49): the optional `filter`, `order`, `limit` and `offset` input of
//! the generated `list` and `search` commands.
//!
//! A query is parsed against the model's field kinds, so a bad field, operator or value is an
//! error before any record is read, and then evaluated over the stored records. Comparisons are
//! typed: numbers and money as integers, dates as calendar dates, selects by option order, so
//! nothing is ever compared as the wrong kind of text. Evaluation happens before masking and
//! withholding, which is why the dispatcher refuses to filter or sort on a field the caller
//! cannot read in clear (see `guards::query_guard`): otherwise the result would be an oracle for
//! values the caller is not allowed to see (D10).

use crate::Value;
use obop_registry::{FieldKind, ModelDef, is_valid_date};
use obop_storage::Fields;
use serde_json::Value as J;
use std::cmp::Ordering;

/// Hard maximum of `limit` per call.
pub const MAX_LIMIT: usize = 5000;
const MAX_FILTERS: usize = 50;
const MAX_ORDER: usize = 5;
const MAX_IN: usize = 1000;

const OPS: &str = "eq, ne, contains, in, gt, gte, lt, lte, is_null";

/// A comparable form of a stored or supplied value of one field. Both sides of a comparison come
/// from the same field, so they always have the same variant.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Int(i64),
    Str(String),
}

#[derive(Debug)]
enum Op {
    Eq(Key),
    Ne(Key),
    Contains(String),
    In(Vec<Key>),
    Gt(Key),
    Gte(Key),
    Lt(Key),
    Lte(Key),
    /// `true`: the field is empty; `false`: it is set.
    IsNull(bool),
}

#[derive(Debug)]
struct Cond {
    field: String,
    kind: FieldKind,
    op: Op,
}

#[derive(Debug)]
struct Sort {
    field: String,
    kind: FieldKind,
    desc: bool,
}

#[derive(Debug, Default)]
pub struct Query {
    conds: Vec<Cond>,
    order: Vec<Sort>,
    limit: Option<usize>,
    offset: usize,
}

/// The kind of a queryable field; `id` is the record id, a string.
fn kind_of(def: &ModelDef, name: &str) -> Option<FieldKind> {
    if name == "id" {
        return Some(FieldKind::Text);
    }
    def.fields
        .iter()
        .find(|f| f.name == name)
        .map(|f| f.kind.clone())
}

/// The one error for a field that does not exist and for one the caller may not read.
fn unknown(def: &ModelDef, name: &str) -> String {
    format!("unknown field {}.{name}", def.name)
}

fn date_key(s: &str) -> Option<i64> {
    if !is_valid_date(s) {
        return None;
    }
    let n = |r: std::ops::Range<usize>| s[r].parse::<i64>().ok();
    Some(n(0..4)? * 10_000 + n(5..7)? * 100 + n(8..10)?)
}

/// The key of a stored JSON value; `None` for null, missing or a value of the wrong type (such
/// a value counts as empty, it can only come from data that bypassed validation).
fn stored_key(kind: &FieldKind, v: &J) -> Option<Key> {
    match (kind, v) {
        (FieldKind::Number | FieldKind::Money, J::Number(n)) => n.as_i64().map(Key::Int),
        (FieldKind::Date, J::String(s)) => date_key(s).map(Key::Int),
        (FieldKind::Bool, J::Bool(b)) => Some(Key::Int(i64::from(*b))),
        (FieldKind::Select(o), J::String(s)) => o.iter().position(|x| x == s).map(|i| Key::Int(i as i64)),
        (FieldKind::Text | FieldKind::LongText | FieldKind::Ref(_), J::String(s)) => {
            Some(Key::Str(s.clone()))
        }
        _ => None,
    }
}

/// The key of a value in the query, which must have the field's type.
fn operand(model: &str, field: &str, kind: &FieldKind, v: &Value) -> Result<Key, String> {
    let bad = || format!("filter on {model}.{field}: value must be {}", kind.describe());
    match (kind, v) {
        (FieldKind::Number | FieldKind::Money, Value::Int(i)) => Ok(Key::Int(*i)),
        (FieldKind::Date, Value::Text(t)) => date_key(t).map(Key::Int).ok_or_else(bad),
        (FieldKind::Bool, Value::Bool(b)) => Ok(Key::Int(i64::from(*b))),
        (FieldKind::Select(o), Value::Text(t)) => o
            .iter()
            .position(|x| x == t)
            .map(|i| Key::Int(i as i64))
            .ok_or_else(bad),
        (FieldKind::Text | FieldKind::LongText | FieldKind::Ref(_), Value::Text(t)) => {
            Ok(Key::Str(t.clone()))
        }
        _ => Err(bad()),
    }
}

fn int_option(m: &std::collections::BTreeMap<String, Value>, k: &str) -> Result<Option<i64>, String> {
    match m.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Int(i)) => Ok(Some(*i)),
        Some(_) => Err(format!("{k} must be an integer")),
    }
}

impl Query {
    /// Whether the input asks for anything beyond "every record".
    pub fn is_plain(&self) -> bool {
        self.conds.is_empty() && self.order.is_empty() && self.limit.is_none() && self.offset == 0
    }

    /// Parse the input of `list` or `search`. `usable` says whether a field may be used in a
    /// filter or order; one that may not gets the same error as a field that does not exist.
    pub fn parse(
        def: &ModelDef,
        input: &Value,
        usable: &dyn Fn(&str) -> bool,
    ) -> Result<Self, String> {
        let Value::Map(m) = input else {
            return Err("expected an object".into());
        };
        for k in m.keys() {
            if !["filter", "order", "limit", "offset", "_caller"].contains(&k.as_str()) {
                return Err(format!(
                    "unknown option {k:?} (use filter, order, limit and offset)"
                ));
            }
        }
        let field = |name: &str| -> Result<FieldKind, String> {
            if !usable(name) {
                return Err(unknown(def, name));
            }
            kind_of(def, name).ok_or_else(|| unknown(def, name))
        };
        let mut q = Query::default();
        match m.get("filter") {
            None | Some(Value::Null) => {}
            Some(Value::List(items)) => {
                if items.len() > MAX_FILTERS {
                    return Err(format!("at most {MAX_FILTERS} filter conditions"));
                }
                for item in items {
                    q.conds.push(parse_cond(def, item, &field)?);
                }
            }
            Some(_) => return Err("filter must be an array of {field, op, value}".into()),
        }
        match m.get("order") {
            None | Some(Value::Null) => {}
            Some(Value::List(items)) => {
                if items.len() > MAX_ORDER {
                    return Err(format!("at most {MAX_ORDER} order keys"));
                }
                for item in items {
                    let Value::Map(o) = item else {
                        return Err("order must be an array of {field, dir}".into());
                    };
                    for k in o.keys() {
                        if k != "field" && k != "dir" {
                            return Err(format!("unknown order key {k:?} (use field and dir)"));
                        }
                    }
                    let Some(Value::Text(name)) = o.get("field") else {
                        return Err("each order key needs a field".into());
                    };
                    let desc = match o.get("dir") {
                        None | Some(Value::Null) => false,
                        Some(Value::Text(d)) if d == "asc" => false,
                        Some(Value::Text(d)) if d == "desc" => true,
                        Some(_) => return Err("dir must be \"asc\" or \"desc\"".into()),
                    };
                    q.order.push(Sort {
                        kind: field(name)?,
                        field: name.clone(),
                        desc,
                    });
                }
            }
            Some(_) => return Err("order must be an array of {field, dir}".into()),
        }
        if let Some(l) = int_option(m, "limit")? {
            if !(1..=MAX_LIMIT as i64).contains(&l) {
                return Err(format!("limit must be an integer from 1 to {MAX_LIMIT}"));
            }
            q.limit = Some(l as usize);
        }
        if let Some(o) = int_option(m, "offset")? {
            if o < 0 {
                return Err("offset must be zero or more".into());
            }
            q.offset = o as usize;
        }
        Ok(q)
    }

    /// Filter, sort and page `rows`. Returns the page and the number of matches before paging.
    pub fn apply(&self, rows: Vec<(String, Fields)>) -> (Vec<(String, Fields)>, usize) {
        let mut rows: Vec<_> = rows.into_iter().filter(|(id, f)| self.matches(id, f)).collect();
        if !self.order.is_empty() {
            // `sort_by` is stable, so ties keep insertion order.
            rows.sort_by(|(ia, a), (ib, b)| {
                for s in &self.order {
                    let (ka, kb) = (sort_key(&s.field, &s.kind, ia, a), sort_key(&s.field, &s.kind, ib, b));
                    // Empty values sort last in both directions.
                    let ord = match (ka, kb) {
                        (None, None) => Ordering::Equal,
                        (None, Some(_)) => return Ordering::Greater,
                        (Some(_), None) => return Ordering::Less,
                        (Some(x), Some(y)) => {
                            let o = cmp_sort(&x, &y);
                            if s.desc { o.reverse() } else { o }
                        }
                    };
                    if ord != Ordering::Equal {
                        return ord;
                    }
                }
                Ordering::Equal
            });
        }
        let total = rows.len();
        let page = rows
            .into_iter()
            .skip(self.offset)
            .take(self.limit.unwrap_or(usize::MAX))
            .collect();
        (page, total)
    }

    fn matches(&self, id: &str, f: &Fields) -> bool {
        self.conds.iter().all(|c| {
            let key = if c.field == "id" {
                Some(Key::Str(id.to_string()))
            } else {
                f.get(&c.field).and_then(|v| stored_key(&c.kind, v))
            };
            match (&c.op, key) {
                (Op::IsNull(want), k) => k.is_none() == *want,
                (Op::Ne(x), k) => k.as_ref() != Some(x),
                (_, None) => false,
                (Op::Eq(x), Some(k)) => k == *x,
                (Op::In(xs), Some(k)) => xs.contains(&k),
                (Op::Gt(x), Some(k)) => k > *x,
                (Op::Gte(x), Some(k)) => k >= *x,
                (Op::Lt(x), Some(k)) => k < *x,
                (Op::Lte(x), Some(k)) => k <= *x,
                (Op::Contains(n), Some(Key::Str(s))) => s.to_lowercase().contains(n),
                (Op::Contains(_), Some(_)) => false,
            }
        })
    }
}

fn sort_key(field: &str, kind: &FieldKind, id: &str, f: &Fields) -> Option<Key> {
    if field == "id" {
        return Some(Key::Str(id.to_string()));
    }
    f.get(field).and_then(|v| stored_key(kind, v))
}

/// Text sorts case-insensitively, with the exact text as tie-breaker.
fn cmp_sort(a: &Key, b: &Key) -> Ordering {
    match (a, b) {
        (Key::Str(x), Key::Str(y)) => x.to_lowercase().cmp(&y.to_lowercase()).then_with(|| x.cmp(y)),
        _ => a.cmp(b),
    }
}

fn parse_cond(
    def: &ModelDef,
    item: &Value,
    field: &dyn Fn(&str) -> Result<FieldKind, String>,
) -> Result<Cond, String> {
    let Value::Map(c) = item else {
        return Err("each filter must be an object {field, op, value}".into());
    };
    for k in c.keys() {
        if !["field", "op", "value"].contains(&k.as_str()) {
            return Err(format!("unknown filter key {k:?} (use field, op and value)"));
        }
    }
    let Some(Value::Text(name)) = c.get("field") else {
        return Err("each filter needs a field".into());
    };
    let kind = field(name)?;
    let Some(Value::Text(op)) = c.get("op") else {
        return Err(format!("each filter needs an op ({OPS})"));
    };
    let value = c.get("value").unwrap_or(&Value::Null);
    let one = |v: &Value| -> Result<Key, String> {
        if *v == Value::Null {
            return Err(format!(
                "filter on {}.{name}: use is_null to test for empty",
                def.name
            ));
        }
        operand(def.name, name, &kind, v)
    };
    let ranged = !matches!(kind, FieldKind::Ref(_) | FieldKind::Bool);
    let op = match op.as_str() {
        "eq" => Op::Eq(one(value)?),
        "ne" => Op::Ne(one(value)?),
        "in" => {
            let Value::List(vs) = value else {
                return Err(format!("filter on {}.{name}: `in` needs an array", def.name));
            };
            if vs.len() > MAX_IN {
                return Err(format!("`in` takes at most {MAX_IN} values"));
            }
            Op::In(vs.iter().map(&one).collect::<Result<_, _>>()?)
        }
        "contains" => {
            if !matches!(kind, FieldKind::Text | FieldKind::LongText) {
                return Err(format!(
                    "filter on {}.{name}: contains only works on text fields",
                    def.name
                ));
            }
            match value {
                Value::Text(t) => Op::Contains(t.to_lowercase()),
                _ => return Err(format!("filter on {}.{name}: contains needs a string", def.name)),
            }
        }
        "gt" | "gte" | "lt" | "lte" => {
            if !ranged {
                return Err(format!(
                    "filter on {}.{name}: {op} does not apply to {} fields",
                    def.name,
                    kind.name()
                ));
            }
            let k = one(value)?;
            match op.as_str() {
                "gt" => Op::Gt(k),
                "gte" => Op::Gte(k),
                "lt" => Op::Lt(k),
                _ => Op::Lte(k),
            }
        }
        "is_null" => Op::IsNull(match value {
            Value::Null => true,
            Value::Bool(b) => *b,
            _ => {
                return Err(format!(
                    "filter on {}.{name}: is_null takes true or false",
                    def.name
                ));
            }
        }),
        other => return Err(format!("unknown op {other:?} (use {OPS})")),
    };
    Ok(Cond {
        field: name.clone(),
        kind,
        op,
    })
}
