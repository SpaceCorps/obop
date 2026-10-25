//! Server-side aggregation for `<model>.search` (D4): the `aggregate` input.
//!
//! ```json
//! { "filter": [...],
//!   "aggregate": {
//!     "group_by": ["status", {"field": "created", "bucket": "month", "zone": "Europe/Stockholm"}],
//!     "measures": [{"op": "count"}, {"op": "sum", "field": "amount_minor"}] } }
//! ```
//!
//! The answer is `{rows: [], total, groups: [{key, count, measures}]}`: `total` is the number of
//! records the filter matches, `key` maps each `group_by` field to its value (`null` for empty;
//! a date or datetime bucket is `YYYY-MM-DD` for `day` and `week` (the Monday), `YYYY-Www` for
//! `iso_week`, `YYYY-MM` for `month` and `YYYY` for `year`), `count` is the number of records in
//! the group and `measures` maps `"sum:field"`, `"min:field"`, `"max:field"` and `"avg:field"`
//! to an integer, or for a `decimal` field to a decimal string (`sum` of nothing is 0, `min`,
//! `max` and `avg` of nothing are `null`). `count` as a measure only documents intent; every
//! group carries it. Groups are ordered by their keys in the field's own order (select option
//! order, numbers, decimals, dates and period labels as such, text by the `lang` collation,
//! empty last).
//!
//! Decimals (D72) are summed, minimised and maximised exactly, in whole units of the field's
//! scale (128 bit; an overflow is an error). `avg` is the exact sum divided by the number of
//! values, rounded once to the measure's `scale` (default: the field's) with its `round`
//! (`half_even` by default, or `half_up` or `down`); only decimal fields have an `avg`.
//!
//! A datetime is grouped by the wall clock of a `zone` (an IANA name; the platform zone when the
//! entry names none), with weeks starting on Monday.
//!
//! Rules: at most 2 `group_by` entries and 8 measures; `sum`, `min` and `max` apply to number,
//! money and decimal fields; `bucket` applies to date and datetime fields only (and one of them
//! must be bucketed to be grouped); no `order`, `limit` or `offset` (groups are the page). Every field named here is
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
use obop_values::datetime::{bucket_label, parse_date};
use obop_values::{Bucket, Decimal, Instant, Round, Zone, Zones};
use serde_json::Value as J;
use std::cmp::Ordering;
use std::collections::BTreeMap;

/// Most groups one aggregation may produce.
pub const MAX_GROUPS: usize = 10_000;
const MAX_GROUP_BY: usize = 2;
const MAX_MEASURES: usize = 8;

#[derive(Debug, Clone)]
struct GroupBy {
    field: String,
    kind: FieldKind,
    bucket: Option<Bucket>,
    /// The wall clock a datetime is cut on.
    zone: Option<Zone>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Agg {
    Count,
    Sum,
    Min,
    Max,
    Avg,
}

#[derive(Debug, Clone)]
struct Measure {
    f: Agg,
    field: String,
    /// `sum:amount`, `min:amount`, `max:amount`, `avg:amount`, or `count`.
    name: String,
    /// The scale of a decimal field; none for whole numbers.
    dec_scale: Option<u8>,
    /// Scale and rounding of an `avg`.
    out_scale: u8,
    round: Round,
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
    /// Per measure: running sum, min or max (`None` until a value is seen). A decimal is held
    /// as whole units of its field's scale.
    vals: Vec<Option<i128>>,
    /// Per measure: how many values were seen (for `avg`).
    seen: Vec<u64>,
}

impl Aggregate {
    pub(crate) fn parse(
        def: &ModelDef,
        v: &Value,
        field: &dyn Fn(&str) -> Result<FieldKind, String>,
        zones: &Zones,
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
                    let (name, bucket, zone_name) = match item {
                        Value::Text(n) => (n.clone(), None, None),
                        Value::Map(o) => {
                            for k in o.keys() {
                                if k != "field" && k != "bucket" && k != "zone" {
                                    return Err(format!(
                                        "unknown group_by key {k:?} (use field, bucket and zone)"
                                    ));
                                }
                            }
                            let Some(Value::Text(n)) = o.get("field") else {
                                return Err("each group_by entry needs a field".into());
                            };
                            let b = match o.get("bucket") {
                                None | Some(Value::Null) => None,
                                Some(Value::Text(b)) => {
                                    Some(Bucket::parse(b).ok_or_else(|| {
                                        format!("bucket must be {}", Bucket::NAMES)
                                    })?)
                                }
                                Some(_) => {
                                    return Err(format!("bucket must be {}", Bucket::NAMES));
                                }
                            };
                            let z = match o.get("zone") {
                                None | Some(Value::Null) => None,
                                Some(Value::Text(z)) => Some(z.clone()),
                                Some(_) => {
                                    return Err("zone must be an IANA time zone name".into());
                                }
                            };
                            (n.clone(), b, z)
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
                        (FieldKind::Date | FieldKind::Datetime, None) => {
                            return Err(format!(
                                "group_by on {}.{name}: a {} needs a bucket ({})",
                                def.name,
                                kind.name(),
                                Bucket::NAMES
                            ));
                        }
                        (FieldKind::Date | FieldKind::Datetime, Some(_)) => {}
                        (_, Some(_)) => {
                            return Err(format!(
                                "group_by on {}.{name}: bucket only applies to date and datetime fields",
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
                    let zone = match (&kind, zone_name) {
                        (FieldKind::Datetime, z) => Some(
                            zones
                                .zone(z.as_deref())
                                .map_err(|e| format!("group_by on {}.{name}: {e}", def.name))?,
                        ),
                        (_, Some(_)) => {
                            return Err(format!(
                                "group_by on {}.{name}: a date has no time zone; zone only applies to datetime fields",
                                def.name
                            ));
                        }
                        (_, None) => None,
                    };
                    group_by.push(GroupBy {
                        field: name,
                        kind,
                        bucket,
                        zone,
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
                        if !["op", "field", "scale", "round"].contains(&k.as_str()) {
                            return Err(format!(
                                "unknown measure key {k:?} (use op and field, and for avg scale and round)"
                            ));
                        }
                    }
                    let Some(Value::Text(op)) = o.get("op") else {
                        return Err("each measure needs an op (count, sum, min, max, avg)".into());
                    };
                    let f = match op.as_str() {
                        "count" => Agg::Count,
                        "sum" => Agg::Sum,
                        "min" => Agg::Min,
                        "max" => Agg::Max,
                        "avg" => Agg::Avg,
                        other => {
                            return Err(format!(
                                "unknown measure op {other:?} (use count, sum, min, max, avg)"
                            ));
                        }
                    };
                    let given = |k: &str| o.get(k).filter(|v| **v != Value::Null);
                    if f != Agg::Avg && (given("scale").is_some() || given("round").is_some()) {
                        return Err(format!("{op} takes no scale or round; only avg does"));
                    }
                    if f == Agg::Count {
                        if o.get("field").is_some_and(|f| *f != Value::Null) {
                            return Err("count takes no field".into());
                        }
                        measures.push(Measure {
                            f,
                            field: String::new(),
                            name: "count".into(),
                            dec_scale: None,
                            out_scale: 0,
                            round: Round::HalfEven,
                        });
                        continue;
                    }
                    let Some(Value::Text(name)) = o.get("field") else {
                        return Err(format!("{op} needs a field"));
                    };
                    let kind = field(name)?;
                    let decimal = kind.decimal_shape();
                    let allowed = match f {
                        Agg::Avg => decimal.is_some(),
                        _ => {
                            matches!(kind, FieldKind::Number | FieldKind::Money)
                                || decimal.is_some()
                        }
                    };
                    if !allowed {
                        return Err(if f == Agg::Avg {
                            format!(
                                "avg on {}.{name}: only decimal fields have an average (an average of whole numbers needs a scale; use a decimal field)",
                                def.name
                            )
                        } else {
                            format!(
                                "{op} on {}.{name}: only number, money and decimal fields can be summed, minimised or maximised",
                                def.name
                            )
                        });
                    }
                    let field_scale = decimal.map(|(_, s, _)| s);
                    let out_scale = match given("scale") {
                        None => field_scale.unwrap_or(0),
                        Some(Value::Int(s))
                            if (0..=i64::from(obop_values::MAX_SCALE)).contains(s) =>
                        {
                            *s as u8
                        }
                        Some(_) => {
                            return Err(format!(
                                "scale must be a whole number from 0 to {}",
                                obop_values::MAX_SCALE
                            ));
                        }
                    };
                    let round = match given("round") {
                        None => Round::HalfEven,
                        Some(Value::Text(r)) => Round::parse(r).ok_or_else(|| {
                            "round must be half_even, half_up or down".to_string()
                        })?,
                        Some(_) => return Err("round must be half_even, half_up or down".into()),
                    };
                    measures.push(Measure {
                        f,
                        field: name.clone(),
                        name: format!("{op}:{name}"),
                        dec_scale: field_scale,
                        out_scale,
                        round,
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
                seen: vec![0; measured.len()],
            });
            acc.count += 1;
            for (i, m) in measured.iter().enumerate() {
                let Some(n) = measured_units(m, f.get(&m.field)) else {
                    continue;
                };
                let slot = &mut acc.vals[i];
                acc.seen[i] += 1;
                *slot = Some(match (m.f, *slot) {
                    (_, None) => n,
                    (Agg::Sum | Agg::Avg, Some(s)) => s
                        .checked_add(n)
                        .ok_or_else(|| format!("{} overflows", m.name))?,
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
                let overflow = || format!("{} overflows", m.name);
                let v = match (m.f, acc.vals[i], m.dec_scale) {
                    (Agg::Sum, None, None) => Value::Int(0),
                    (Agg::Sum, None, Some(s)) => Value::Text(Decimal::zero(s).to_string()),
                    (_, None, _) => Value::Null,
                    (Agg::Avg, Some(n), Some(s)) => Value::Text(
                        Decimal::average(n, s, acc.seen[i], m.out_scale, m.round)
                            .map_err(|e| format!("{}: {e}", m.name))?
                            .to_string(),
                    ),
                    (_, Some(n), Some(s)) => {
                        Value::Text(Decimal::new(n, s).ok_or_else(overflow)?.to_string())
                    }
                    (_, Some(n), None) => Value::Int(i64::try_from(n).map_err(|_| overflow())?),
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

/// A value to be summed, compared or averaged, as whole units: an integer, or a decimal in
/// units of its field's scale. Anything else (empty, malformed data that bypassed validation)
/// is skipped.
fn measured_units(m: &Measure, v: Option<&J>) -> Option<i128> {
    match (m.dec_scale, v?) {
        (None, J::Number(n)) => n.as_i64().map(i128::from),
        (Some(scale), J::String(s)) => Decimal::parse(s)
            .ok()?
            .rescale(scale, Round::Down)
            .ok()
            .map(Decimal::units),
        _ => None,
    }
}

fn group_key(g: &GroupBy, f: &Fields) -> Option<Key> {
    let v = f.get(&g.field)?;
    match (&g.kind, v) {
        (FieldKind::Date, J::String(s)) => bucket_of(s, g.bucket?).map(Key::Str),
        (FieldKind::Datetime, J::String(s)) => {
            let at = Instant::parse(s).ok()?;
            Some(Key::Str(g.zone.as_ref()?.bucket(at, g.bucket?)))
        }
        (FieldKind::Decimal { .. }, J::String(s)) => Decimal::parse(s).ok().map(Key::Dec),
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
        (Some(Key::Dec(d)), FieldKind::Decimal { scale, .. }) => Value::Text(
            d.rescale(*scale, Round::Down)
                .map_or_else(|_| d.to_string(), |d| d.to_string()),
        ),
        (Some(Key::Dec(d)), _) => Value::Text(d.to_string()),
        (Some(Key::Str(s)), _) => Value::text(s),
    }
}

/// The bucket of a stored `YYYY-MM-DD`; `None` for a malformed value (it counts as empty).
pub(crate) fn bucket_of(s: &str, b: Bucket) -> Option<String> {
    let (y, m, d) = parse_date(s)?;
    Some(bucket_label(y, m, d, b))
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
