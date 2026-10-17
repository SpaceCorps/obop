//! Server-side aggregation for `<model>.search` (D4): the `aggregate` input.
//!
//! ```json
//! { "filter": [...],
//!   "aggregate": {
//!     "group_by": ["status", {"field": "created", "bucket": "month"}],
//!     "measures": [{"op": "count"}, {"op": "sum", "field": "amount_minor"}] } }
//! ```
//!
//! The answer is `{rows: [], total, groups: [{key, count, measures}]}`: `total` is the number of
//! records the filter matches, `key` maps each `group_by` field to its value (`null` for empty;
//! a date bucket is `YYYY-MM-DD` for `day` and `week` (the Monday), `YYYY-MM` for `month`),
//! `count` is the number of records in the group and `measures` maps `"sum:field"`, `"min:field"`
//! and `"max:field"` to an integer (`sum` of nothing is 0, `min` and `max` of nothing are `null`).
//! `count` as a measure only documents intent; every group carries it. Groups are ordered by their
//! keys in the field's own order (select option order, numbers and dates as such, text by the
//! `lang` collation, empty last).
//!
//! Rules: at most 2 `group_by` entries and 8 measures; `sum`, `min` and `max` apply to number and
//! money fields only; `bucket` applies to date fields only (and a date field must be bucketed to
//! be grouped); no `order`, `limit` or `offset` (groups are the page). Every field named here is
//! checked with the same `usable` test as filters and order, so a field the caller may not read in
//! clear gets the error of a field that does not exist (D10): no group, sum or count can leak it.
//!
//! Cost: one pass over the model's records through `Backend::scan` (a record at a time on every
//! backend, `O(n)` time for `n` records of the model). Memory is `O(groups * measures)`, with at
//! most [`MAX_GROUPS`] groups: more is an error that asks for a narrower filter or a coarser
//! grouping, never a silent truncation. The filter is evaluated in the same pass.

use crate::Value;
use crate::collate::TextOrder;
use crate::query::{Key, Query};
use obop_registry::{FieldKind, ModelDef};
use obop_storage::{Backend, Fields};
use serde_json::Value as J;
use std::cmp::Ordering;
use std::collections::BTreeMap;

/// Most groups one aggregation may produce.
pub const MAX_GROUPS: usize = 10_000;
const MAX_GROUP_BY: usize = 2;
const MAX_MEASURES: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bucket {
    Day,
    Week,
    Month,
}

#[derive(Debug, Clone)]
struct GroupBy {
    field: String,
    kind: FieldKind,
    bucket: Option<Bucket>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Agg {
    Count,
    Sum,
    Min,
    Max,
}

#[derive(Debug, Clone)]
struct Measure {
    f: Agg,
    field: String,
    /// `sum:amount`, `min:amount`, `max:amount`, or `count`.
    name: String,
}

type Visit<'a> = dyn FnMut(&str, &Fields) -> obop_storage::Result<bool> + 'a;

#[derive(Debug)]
pub struct Aggregate {
    group_by: Vec<GroupBy>,
    measures: Vec<Measure>,
}

/// A group key: one part per `group_by` entry, `None` for empty.
type GKey = Vec<Option<Key>>;

#[derive(Default)]
struct Acc {
    count: i64,
    /// Per measure: running sum, min or max (`None` until a value is seen).
    vals: Vec<Option<i128>>,
}

impl Aggregate {
    pub(crate) fn parse(
        def: &ModelDef,
        v: &Value,
        field: &dyn Fn(&str) -> Result<FieldKind, String>,
    ) -> Result<Self, String> {
        let Value::Map(m) = v else {
            return Err("aggregate must be an object {group_by, measures}".into());
        };
        for k in m.keys() {
            if k != "group_by" && k != "measures" {
                return Err(format!(
                    "unknown aggregate key {k:?} (use group_by and measures)"
                ));
            }
        }
        let mut group_by = Vec::new();
        match m.get("group_by") {
            None | Some(Value::Null) => {}
            Some(Value::List(items)) => {
                if items.len() > MAX_GROUP_BY {
                    return Err(format!("group_by takes at most {MAX_GROUP_BY} fields"));
                }
                for item in items {
                    let (name, bucket) = match item {
                        Value::Text(n) => (n.clone(), None),
                        Value::Map(o) => {
                            for k in o.keys() {
                                if k != "field" && k != "bucket" {
                                    return Err(format!(
                                        "unknown group_by key {k:?} (use field and bucket)"
                                    ));
                                }
                            }
                            let Some(Value::Text(n)) = o.get("field") else {
                                return Err("each group_by entry needs a field".into());
                            };
                            let b = match o.get("bucket") {
                                None | Some(Value::Null) => None,
                                Some(Value::Text(b)) => Some(match b.as_str() {
                                    "day" => Bucket::Day,
                                    "week" => Bucket::Week,
                                    "month" => Bucket::Month,
                                    _ => return Err("bucket must be day, week or month".into()),
                                }),
                                Some(_) => return Err("bucket must be day, week or month".into()),
                            };
                            (n.clone(), b)
                        }
                        _ => {
                            return Err(
                                "group_by entries are field names or {field, bucket}".into()
                            );
                        }
                    };
                    let kind = field(&name)?;
                    if name == "id" {
                        return Err(format!("cannot group {} by id", def.name));
                    }
                    match (&kind, bucket) {
                        (FieldKind::Date, None) => {
                            return Err(format!(
                                "group_by on {}.{name}: a date needs a bucket (day, week or month)",
                                def.name
                            ));
                        }
                        (FieldKind::Date, Some(_)) => {}
                        (_, Some(_)) => {
                            return Err(format!(
                                "group_by on {}.{name}: bucket only applies to date fields",
                                def.name
                            ));
                        }
                        (FieldKind::LongText, None) => {
                            return Err(format!(
                                "group_by on {}.{name}: long text cannot be grouped",
                                def.name
                            ));
                        }
                        _ => {}
                    }
                    if group_by.iter().any(|g: &GroupBy| g.field == name) {
                        return Err(format!("group_by names {name} twice"));
                    }
                    group_by.push(GroupBy {
                        field: name,
                        kind,
                        bucket,
                    });
                }
            }
            Some(_) => return Err("group_by must be an array".into()),
        }
        let mut measures = Vec::new();
        match m.get("measures") {
            None | Some(Value::Null) => {}
            Some(Value::List(items)) => {
                if items.len() > MAX_MEASURES {
                    return Err(format!("at most {MAX_MEASURES} measures"));
                }
                for item in items {
                    let Value::Map(o) = item else {
                        return Err("measures must be an array of {op, field}".into());
                    };
                    for k in o.keys() {
                        if k != "op" && k != "field" {
                            return Err(format!("unknown measure key {k:?} (use op and field)"));
                        }
                    }
                    let Some(Value::Text(op)) = o.get("op") else {
                        return Err("each measure needs an op (count, sum, min, max)".into());
                    };
                    let f = match op.as_str() {
                        "count" => Agg::Count,
                        "sum" => Agg::Sum,
                        "min" => Agg::Min,
                        "max" => Agg::Max,
                        other => {
                            return Err(format!(
                                "unknown measure op {other:?} (use count, sum, min, max)"
                            ));
                        }
                    };
                    if f == Agg::Count {
                        if o.get("field").is_some_and(|f| *f != Value::Null) {
                            return Err("count takes no field".into());
                        }
                        measures.push(Measure {
                            f,
                            field: String::new(),
                            name: "count".into(),
                        });
                        continue;
                    }
                    let Some(Value::Text(name)) = o.get("field") else {
                        return Err(format!("{op} needs a field"));
                    };
                    let kind = field(name)?;
                    if !matches!(kind, FieldKind::Number | FieldKind::Money) {
                        return Err(format!(
                            "{op} on {}.{name}: only number and money fields can be summed, minimised or maximised",
                            def.name
                        ));
                    }
                    measures.push(Measure {
                        f,
                        field: name.clone(),
                        name: format!("{op}:{name}"),
                    });
                }
            }
            Some(_) => return Err("measures must be an array".into()),
        }
        if group_by.is_empty() && measures.is_empty() {
            return Err("aggregate needs group_by or measures".into());
        }
        Ok(Aggregate { group_by, measures })
    }

    /// Fields the aggregate groups on or measures.
    pub(crate) fn fields(&self) -> Vec<String> {
        self.group_by
            .iter()
            .map(|g| g.field.clone())
            .chain(
                self.measures
                    .iter()
                    .filter(|m| m.f != Agg::Count)
                    .map(|m| m.field.clone()),
            )
            .collect()
    }

    /// Evaluate over the records of `model`, streaming. Returns the groups (ordered) and the
    /// number of matching records.
    pub(crate) fn run(
        &self,
        q: &Query,
        backend: &dyn Backend,
        model: &str,
        visible: &dyn Fn(&Fields) -> bool,
    ) -> Result<(Vec<Value>, usize), String> {
        self.run_with(q, &mut |v| backend.scan(model, v), visible)
    }

    /// `run` over any source of records: the streaming scan of a backend, or rows already loaded
    /// (when computed fields had to be evaluated first).
    pub(crate) fn run_with(
        &self,
        q: &Query,
        scan: &mut dyn FnMut(&mut Visit<'_>) -> obop_storage::Result<()>,
        visible: &dyn Fn(&Fields) -> bool,
    ) -> Result<(Vec<Value>, usize), String> {
        let mut groups: BTreeMap<GKey, Acc> = BTreeMap::new();
        let mut total = 0usize;
        let measured: Vec<&Measure> = self.measures.iter().filter(|m| m.f != Agg::Count).collect();
        scan(&mut |id, f| {
            // Row-level access (D58): a record the caller may not see is not counted, summed or
            // grouped, so an aggregate never reveals what a list would hide.
            if !visible(f) || !q.matches(id, f) {
                return Ok(true);
            }
            total += 1;
            let key: GKey = self.group_by.iter().map(|g| group_key(g, f)).collect();
            if !groups.contains_key(&key) && groups.len() >= MAX_GROUPS {
                return Err(format!(
                    "aggregate would produce more than {MAX_GROUPS} groups; narrow the filter or group by a coarser field"
                ));
            }
            let acc = groups.entry(key).or_insert_with(|| Acc {
                count: 0,
                vals: vec![None; measured.len()],
            });
            acc.count += 1;
            for (i, m) in measured.iter().enumerate() {
                let Some(n) = f.get(&m.field).and_then(J::as_i64) else {
                    continue;
                };
                let n = i128::from(n);
                let slot = &mut acc.vals[i];
                *slot = Some(match (m.f, *slot) {
                    (_, None) => n,
                    (Agg::Sum, Some(s)) => s + n,
                    (Agg::Min, Some(s)) => s.min(n),
                    (_, Some(s)) => s.max(n),
                });
            }
            Ok(true)
        })?;
        let order = TextOrder::new(q.lang());
        let mut entries: Vec<(GKey, Acc)> = groups.into_iter().collect();
        entries.sort_by(|(a, _), (b, _)| self.cmp_keys(&order, a, b));
        let mut out = Vec::with_capacity(entries.len());
        for (key, acc) in entries {
            let mut k = BTreeMap::new();
            for (g, part) in self.group_by.iter().zip(&key) {
                k.insert(g.field.clone(), render(g, part));
            }
            let mut ms = BTreeMap::new();
            for (i, m) in measured.iter().enumerate() {
                let v = match (m.f, acc.vals[i]) {
                    (Agg::Sum, None) => Value::Int(0),
                    (_, None) => Value::Null,
                    (_, Some(n)) => {
                        Value::Int(i64::try_from(n).map_err(|_| format!("{} overflows", m.name))?)
                    }
                };
                ms.insert(m.name.clone(), v);
            }
            out.push(Value::map([
                ("key", Value::Map(k)),
                ("count", Value::Int(acc.count)),
                ("measures", Value::Map(ms)),
            ]));
        }
        Ok((out, total))
    }

    fn cmp_keys(&self, order: &TextOrder, a: &GKey, b: &GKey) -> Ordering {
        for (g, (x, y)) in self.group_by.iter().zip(a.iter().zip(b)) {
            let o = match (x, y) {
                (None, None) => Ordering::Equal,
                (None, Some(_)) => Ordering::Greater,
                (Some(_), None) => Ordering::Less,
                (Some(Key::Str(x)), Some(Key::Str(y))) if matches!(g.kind, FieldKind::Text) => {
                    order.compare(x, y)
                }
                (Some(x), Some(y)) => x.cmp(y),
            };
            if o != Ordering::Equal {
                return o;
            }
        }
        Ordering::Equal
    }
}

fn group_key(g: &GroupBy, f: &Fields) -> Option<Key> {
    let v = f.get(&g.field)?;
    match (&g.kind, v) {
        (FieldKind::Date, J::String(s)) => bucket_of(s, g.bucket?).map(Key::Str),
        (FieldKind::Number | FieldKind::Money, J::Number(n)) => n.as_i64().map(Key::Int),
        (FieldKind::Bool, J::Bool(b)) => Some(Key::Int(i64::from(*b))),
        (FieldKind::Select(o), J::String(s)) => {
            o.iter().position(|x| x == s).map(|i| Key::Int(i as i64))
        }
        (FieldKind::Text | FieldKind::Ref(_), J::String(s)) => Some(Key::Str(s.clone())),
        _ => None,
    }
}

fn render(g: &GroupBy, k: &Option<Key>) -> Value {
    match (k, &g.kind) {
        (None, _) => Value::Null,
        (Some(Key::Int(i)), FieldKind::Select(o)) => {
            o.get(*i as usize).map_or(Value::Null, |s| Value::text(s))
        }
        (Some(Key::Int(i)), FieldKind::Bool) => Value::Bool(*i != 0),
        (Some(Key::Int(i)), _) => Value::Int(*i),
        (Some(Key::Str(s)), _) => Value::text(s),
    }
}

/// Days since 1970-01-01 of a calendar date (proleptic Gregorian).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// The bucket of a stored `YYYY-MM-DD`; `None` for a malformed value (it counts as empty).
pub(crate) fn bucket_of(s: &str, b: Bucket) -> Option<String> {
    if !obop_registry::is_valid_date(s) {
        return None;
    }
    let n = |r: std::ops::Range<usize>| s[r].parse::<i64>().ok();
    let (y, m, d) = (n(0..4)?, n(5..7)?, n(8..10)?);
    Some(match b {
        Bucket::Day => s.to_string(),
        Bucket::Month => format!("{y:04}-{m:02}"),
        Bucket::Week => {
            let days = days_from_civil(y, m, d);
            // 1970-01-01 was a Thursday; Monday is 0.
            let dow = (days + 3).rem_euclid(7);
            let (wy, wm, wd) = civil_from_days(days - dow);
            format!("{wy:04}-{wm:02}-{wd:02}")
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn week_buckets_start_on_monday() {
        // 2026-10-17 is a Saturday; its Monday is 2026-10-12.
        assert_eq!(bucket_of("2026-10-17", Bucket::Week).unwrap(), "2026-10-12");
        assert_eq!(bucket_of("2026-10-12", Bucket::Week).unwrap(), "2026-10-12");
        assert_eq!(bucket_of("2026-10-18", Bucket::Week).unwrap(), "2026-10-12");
        assert_eq!(bucket_of("2026-01-01", Bucket::Week).unwrap(), "2025-12-29");
        assert_eq!(bucket_of("2024-02-29", Bucket::Month).unwrap(), "2024-02");
        assert_eq!(bucket_of("2024-03-01", Bucket::Week).unwrap(), "2024-02-26");
        assert!(bucket_of("2024-02-30", Bucket::Day).is_none());
    }
}
