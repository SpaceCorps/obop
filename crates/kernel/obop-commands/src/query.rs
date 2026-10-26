//! Server-side list queries (D49): the optional `filter`, `order`, `limit` and `offset` input of
//! the generated `list` and `search` commands.
//!
//! A query is parsed against the model's field kinds, so a bad field, operator or value is an
//! error before any record is read, and then evaluated over the stored records. Comparisons are
//! typed: numbers and money as integers, decimals as exact numbers (never as text: `"9.5"` is
//! below `"10.25"`), instants as points in time whatever offset they were written with, dates as
//! calendar dates, selects by option order, so nothing is ever compared as the wrong kind of
//! text.
//!
//! A condition on a `date` or `datetime` field may carry a `bucket` (`day`, `week`, `iso_week`,
//! `month` or `year`) and, for a datetime, a `zone` (an IANA name, default the platform zone):
//! the field is then cut into that period on the wall clock of the zone before it is compared,
//! and `value` is the period's label (`2026-04`, `2026-W15`, `2026-04-01`). This is also the
//! explicit "date in zone" conversion: `{field, op: "eq", bucket: "day", zone, value: "2026-04-01"}`
//! asks for the instants that fall on that calendar day in that zone. A date never compares with
//! an instant otherwise. Evaluation happens before masking and
//! withholding, which is why the dispatcher refuses to filter or sort on a field the caller
//! cannot read in clear (see `guards::query_guard`): otherwise the result would be an oracle for
//! values the caller is not allowed to see (D10).

use crate::Value;
use crate::aggregate::Aggregate;
use crate::collate::{self, TextOrder};
use obop_registry::{FieldKind, ModelDef, is_valid_date};
use obop_storage::Fields;
use obop_storage::keyexpr::Slot;
use obop_values::datetime::{bucket_label, days_from_civil, parse_date, weekday};
use obop_values::{Bucket, Decimal, Instant, Zone, Zones};
use serde_json::Value as J;
use std::cmp::Ordering;

/// Hard maximum of `limit` per call.
pub const MAX_LIMIT: usize = 5000;
const MAX_FILTERS: usize = 50;
const MAX_ORDER: usize = 5;
const MAX_IN: usize = 1000;

/// Input keys with this prefix are placed by guards (who is calling); queries ignore them.
pub const ACTOR_PREFIX: &str = "_actor";

const OPS: &str = "eq, ne, contains, in, gt, gte, lt, lte, is_null";

/// A comparable form of a stored or supplied value of one field. Both sides of a comparison come
/// from the same field, so they always have the same variant.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Key {
    Int(i64),
    /// An exact decimal; compared as a number whatever its scale (D72).
    Dec(Decimal),
    Str(String),
}

/// A condition's `bucket`: the field is cut into periods before it is compared.
#[derive(Debug, Clone)]
pub(crate) struct Bucketed {
    pub(crate) bucket: Bucket,
    /// The zone of a datetime's wall clock; none for a date.
    pub(crate) zone: Option<Zone>,
}

/// What a condition asks of the field. The match on it is exhaustive wherever a backend lowers
/// it (the pushdown, D76), so a new operator cannot be forgotten there.
#[derive(Debug)]
pub(crate) enum Op {
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
pub(crate) struct Cond {
    pub(crate) field: String,
    pub(crate) kind: FieldKind,
    pub(crate) op: Op,
    pub(crate) bucket: Option<Bucketed>,
}

#[derive(Debug)]
pub(crate) struct Sort {
    pub(crate) field: String,
    pub(crate) kind: FieldKind,
    pub(crate) desc: bool,
}

/// A parsed query taken apart for the pushdown (D76), which lowers the conditions and the order
/// to SQL and leaves paging to the database.
#[derive(Debug)]
#[allow(dead_code, reason = "read by the D76 pushdown")]
pub(crate) struct QueryParts<'a> {
    pub(crate) conds: &'a [Cond],
    pub(crate) order: &'a [Sort],
    pub(crate) limit: Option<usize>,
    pub(crate) offset: usize,
    pub(crate) lang: Option<&'a icu_locale::Locale>,
}

#[derive(Debug, Default)]
pub struct Query {
    conds: Vec<Cond>,
    order: Vec<Sort>,
    limit: Option<usize>,
    offset: usize,
    /// Collation language of text sorts and group order (`lang`), root order when absent.
    lang: Option<icu_locale::Locale>,
    pub(crate) aggregate: Option<Aggregate>,
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

/// A stored instant: only the canonical text `YYYY-MM-DDTHH:MM:SS.ffffffZ` counts (D72, as the
/// stores write it). Any other spelling of an instant, an offset or fewer digits, can only be
/// data that bypassed validation and counts as empty, exactly as the `Instant` slot of
/// `obop_storage::keyexpr` reads it in SQL.
fn stored_instant(s: &str) -> Option<Instant> {
    // The canonical text is 27 bytes: say no to everything else before parsing.
    if s.len() != 27 {
        return None;
    }
    Instant::parse(s).ok().filter(|i| i.canonical() == s)
}

/// The key of a stored JSON value; `None` for null, missing or a value of the wrong type (such
/// a value counts as empty, it can only come from data that bypassed validation).
///
/// `obop_storage::keyexpr` is the same function in SQL, slot for slot (see [`slot_of`]); the
/// test `keyexpr_parity` runs both over the same hostile values on PostgreSQL.
pub(crate) fn stored_key(kind: &FieldKind, v: &J) -> Option<Key> {
    match (kind, v) {
        (FieldKind::Number | FieldKind::Money, J::Number(n)) => n.as_i64().map(Key::Int),
        (FieldKind::Date, J::String(s)) => date_key(s).map(Key::Int),
        // A decimal is read as it is written, whatever its scale: the SQL slot cannot know the
        // field's declared scale either, and the stores check the canonical text on write.
        (FieldKind::Decimal { .. }, J::String(s)) => Decimal::parse(s).ok().map(Key::Dec),
        (FieldKind::Datetime, J::String(s)) => stored_instant(s).map(|i| Key::Int(i.micros())),
        (FieldKind::Bool, J::Bool(b)) => Some(Key::Int(i64::from(*b))),
        (FieldKind::Select(o), J::String(s)) => {
            o.iter().position(|x| x == s).map(|i| Key::Int(i as i64))
        }
        (FieldKind::Text | FieldKind::LongText | FieldKind::Ref(_), J::String(s)) => {
            Some(Key::Str(s.clone()))
        }
        _ => None,
    }
}

/// The SQL slot (`obop_storage::keyexpr`) that reads a stored value of `kind` as `stored_key`
/// does, the one place that maps kinds to slots. `None` for an image or a file: their key is
/// always empty. A select's key is the position of its option, so the slot is `SelPos`; an
/// equality on a select may also compare the option text through `Slot::Str`. The match is
/// exhaustive, so a new kind has to say what it is.
#[allow(dead_code, reason = "read by the D76 pushdown and the D78 indexes")]
pub(crate) fn slot_of(kind: &FieldKind) -> Option<Slot> {
    match kind {
        FieldKind::Text | FieldKind::LongText | FieldKind::Ref(_) => Some(Slot::Str),
        FieldKind::Number | FieldKind::Money => Some(Slot::Int),
        FieldKind::Bool => Some(Slot::Bool),
        FieldKind::Date => Some(Slot::Date),
        FieldKind::Datetime => Some(Slot::Instant),
        FieldKind::Decimal { .. } => Some(Slot::Dec),
        FieldKind::Select(_) => Some(Slot::SelPos),
        FieldKind::Image(_) | FieldKind::File(_) => None,
    }
}

/// The period label of a stored date or instant, for a condition with a `bucket`.
fn bucket_key(kind: &FieldKind, v: &J, b: &Bucketed) -> Option<Key> {
    match (kind, v) {
        (FieldKind::Date, J::String(s)) => {
            let (y, m, d) = parse_date(s)?;
            Some(Key::Str(bucket_label(y, m, d, b.bucket)))
        }
        (FieldKind::Datetime, J::String(s)) => {
            let at = stored_instant(s)?;
            Some(Key::Str(b.zone.as_ref()?.bucket(at, b.bucket)))
        }
        _ => None,
    }
}

/// Whether `s` is a label of `bucket`: the day, the Monday, the month, the ISO week or the year
/// it names must exist.
fn label_ok(bucket: Bucket, s: &str) -> bool {
    match bucket {
        Bucket::Day => parse_date(s).is_some(),
        Bucket::Week => {
            parse_date(s).is_some_and(|(y, m, d)| weekday(days_from_civil(y, m, d)) == 0)
        }
        Bucket::Month => s.len() == 7 && parse_date(&format!("{s}-01")).is_some(),
        Bucket::Year => s.len() == 4 && parse_date(&format!("{s}-01-01")).is_some(),
        Bucket::IsoWeek => {
            let b = s.as_bytes();
            b.len() == 8
                && b[4] == b'-'
                && b[5] == b'W'
                && parse_date(&format!("{}-01-01", &s[..4])).is_some()
                && b[6..].iter().all(u8::is_ascii_digit)
                && s[6..].parse::<u32>().is_ok_and(|w| (1..=53).contains(&w))
        }
    }
}

/// The key of a value in the query, which must have the field's type.
fn operand(model: &str, field: &str, kind: &FieldKind, v: &Value) -> Result<Key, String> {
    let bad = || {
        format!(
            "filter on {model}.{field}: value must be {}",
            kind.describe()
        )
    };
    match (kind, v) {
        (FieldKind::Number | FieldKind::Money, Value::Int(i)) => Ok(Key::Int(*i)),
        (FieldKind::Date, Value::Text(t)) => date_key(t).map(Key::Int).ok_or_else(bad),
        // Any exact decimal may be compared with a decimal field, whatever its scale.
        (FieldKind::Decimal { .. }, Value::Text(t)) => Decimal::parse(t)
            .map(Key::Dec)
            .map_err(|e| format!("{}; {e}", bad())),
        (FieldKind::Datetime, Value::Text(t)) => Instant::parse(t)
            .map(|i| Key::Int(i.micros()))
            .map_err(|e| format!("{}; {e}", bad())),
        (FieldKind::Bool, Value::Bool(b)) => Ok(Key::Int(i64::from(*b))),
        (FieldKind::Select(o), Value::Text(t)) => o
            .iter()
            .position(|x| x == t)
            .map(|i| Key::Int(i as i64))
            .ok_or_else(bad),
        (FieldKind::Text | FieldKind::LongText | FieldKind::Ref(_), Value::Text(t)) => {
            no_nul(model, field, t)?;
            Ok(Key::Str(t.clone()))
        }
        _ => Err(bad()),
    }
}

/// A NUL (U+0000) cannot be stored in PostgreSQL `text` or `jsonb`, and bound as a parameter it
/// fails the statement. So no backend takes it in a filter value: the answer must not depend on
/// where the data lives (D76, a deliberate narrow change; nothing could ever match it on
/// PostgreSQL).
fn no_nul(model: &str, field: &str, t: &str) -> Result<(), String> {
    if t.contains('\0') {
        return Err(format!(
            "filter on {model}.{field}: text cannot contain NUL"
        ));
    }
    Ok(())
}

fn int_option(
    m: &std::collections::BTreeMap<String, Value>,
    k: &str,
) -> Result<Option<i64>, String> {
    match m.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Int(i)) => Ok(Some(*i)),
        Some(_) => Err(format!("{k} must be an integer")),
    }
}

impl Query {
    /// Whether a filter or order key is a field for which `is` says yes.
    pub fn touches(&self, is: &dyn Fn(&str) -> bool) -> bool {
        self.conds.iter().any(|c| is(&c.field)) || self.order.iter().any(|s| is(&s.field))
    }

    /// Whether the input asks for anything beyond "every record".
    pub fn is_plain(&self) -> bool {
        self.conds.is_empty() && self.order.is_empty() && self.limit.is_none() && self.offset == 0
    }

    /// Parse the input of `list` or `search`. `usable` says whether a field may be used in a
    /// filter or order; one that may not gets the same error as a field that does not exist.
    /// Buckets without a zone of their own use the UTC platform zone; the dispatcher calls
    /// `parse_in` with the instance's.
    pub fn parse(
        def: &ModelDef,
        input: &Value,
        usable: &dyn Fn(&str) -> bool,
    ) -> Result<Self, String> {
        Self::parse_in(def, input, usable, &Zones::default())
    }

    /// `parse` with the time zone settings of the instance (the platform zone and the database
    /// that names the zones a `bucket` may use).
    pub fn parse_in(
        def: &ModelDef,
        input: &Value,
        usable: &dyn Fn(&str) -> bool,
        zones: &Zones,
    ) -> Result<Self, String> {
        let Value::Map(m) = input else {
            return Err("expected an object".into());
        };
        for k in m.keys() {
            // `_caller` and `_actor*` are set by the dispatcher and by guards, never by a client.
            if ![
                "filter",
                "order",
                "limit",
                "offset",
                "aggregate",
                "lang",
                "_caller",
            ]
            .contains(&k.as_str())
                && !k.starts_with(ACTOR_PREFIX)
            {
                return Err(format!(
                    "unknown option {k:?} (use filter, order, limit, offset, aggregate and lang)"
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
                    q.conds.push(parse_cond(def, item, &field, zones)?);
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
        match m.get("lang") {
            None | Some(Value::Null) => {}
            Some(Value::Text(l)) => q.lang = Some(collate::parse_lang(l)?),
            Some(_) => return Err("lang must be a BCP-47 language tag".into()),
        }
        match m.get("aggregate") {
            None | Some(Value::Null) => {}
            Some(a) => {
                if !q.order.is_empty() || q.limit.is_some() || q.offset != 0 {
                    return Err(
                        "aggregate returns groups: it takes filter, not order, limit or offset"
                            .into(),
                    );
                }
                q.aggregate = Some(Aggregate::parse(def, a, &field, zones)?);
            }
        }
        Ok(q)
    }

    /// The collation language of text sorts.
    pub(crate) fn lang(&self) -> Option<&icu_locale::Locale> {
        self.lang.as_ref()
    }

    /// The conditions, order, paging and language, for the pushdown (D76).
    #[allow(dead_code, reason = "read by the D76 pushdown")]
    pub(crate) fn parts(&self) -> QueryParts<'_> {
        QueryParts {
            conds: &self.conds,
            order: &self.order,
            limit: self.limit,
            offset: self.offset,
            lang: self.lang.as_ref(),
        }
    }

    /// `list` and the public routes do not aggregate.
    pub fn no_aggregate(&self) -> Result<(), String> {
        if self.aggregate.is_some() {
            return Err("aggregate is only available on search".into());
        }
        Ok(())
    }

    /// Evaluate `aggregate` over a model without collecting its records: the groups and the number
    /// of records the filter matched. `None` when the query has no `aggregate`.
    pub fn run_aggregate(
        &self,
        backend: &dyn obop_storage::Backend,
        model: &str,
        visible: &dyn Fn(&Fields) -> bool,
    ) -> Option<Result<(Vec<Value>, usize), String>> {
        self.aggregate
            .as_ref()
            .map(|a| a.run(self, backend, model, visible))
    }

    /// `run_aggregate` over rows already in memory (computed fields filled in).
    pub(crate) fn run_aggregate_rows(
        &self,
        rows: &[(String, Fields)],
        visible: &dyn Fn(&Fields) -> bool,
    ) -> Option<Result<(Vec<Value>, usize), String>> {
        self.aggregate.as_ref().map(|a| {
            a.run_with(
                self,
                &mut |v| {
                    for (id, f) in rows {
                        if !v(id, f)? {
                            break;
                        }
                    }
                    Ok(())
                },
                visible,
            )
        })
    }

    /// Whether the input asked for an aggregate.
    pub(crate) fn has_aggregate(&self) -> bool {
        self.aggregate.is_some()
    }

    /// Names of the fields the aggregate groups or measures.
    pub(crate) fn aggregate_fields(&self) -> Vec<String> {
        self.aggregate
            .as_ref()
            .map(|a| a.fields())
            .unwrap_or_default()
    }

    /// Filter, sort and page `rows`. Returns the page and the number of matches before paging.
    pub fn apply(&self, rows: Vec<(String, Fields)>) -> (Vec<(String, Fields)>, usize) {
        let mut rows: Vec<_> = rows
            .into_iter()
            .filter(|(id, f)| self.matches(id, f))
            .collect();
        if !self.order.is_empty() {
            // Text sorts with the collation of `lang` (see `collate`); ties end at the record id,
            // so the order does not depend on how the backend returned the records.
            let text = TextOrder::new(self.lang.as_ref());
            rows.sort_by(|(ia, a), (ib, b)| {
                for s in &self.order {
                    let (ka, kb) = (
                        sort_key(&s.field, &s.kind, ia, a),
                        sort_key(&s.field, &s.kind, ib, b),
                    );
                    // Empty values sort last in both directions.
                    let ord = match (ka, kb) {
                        (None, None) => Ordering::Equal,
                        (None, Some(_)) => return Ordering::Greater,
                        (Some(_), None) => return Ordering::Less,
                        (Some(x), Some(y)) => {
                            let o = cmp_sort(&text, &s.kind, &s.field, &x, &y);
                            if s.desc { o.reverse() } else { o }
                        }
                    };
                    if ord != Ordering::Equal {
                        return ord;
                    }
                }
                ia.cmp(ib)
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

    pub(crate) fn matches(&self, id: &str, f: &Fields) -> bool {
        self.conds.iter().all(|c| {
            let key = if c.field == "id" {
                Some(Key::Str(id.to_string()))
            } else if let Some(b) = &c.bucket {
                f.get(&c.field).and_then(|v| bucket_key(&c.kind, v, b))
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

/// Text and long text sort by collation; ids, refs and everything else by their typed key.
fn cmp_sort(text: &TextOrder, kind: &FieldKind, field: &str, a: &Key, b: &Key) -> Ordering {
    match (a, b) {
        (Key::Str(x), Key::Str(y))
            if field != "id" && matches!(kind, FieldKind::Text | FieldKind::LongText) =>
        {
            text.compare(x, y)
        }
        _ => a.cmp(b),
    }
}

fn parse_cond(
    def: &ModelDef,
    item: &Value,
    field: &dyn Fn(&str) -> Result<FieldKind, String>,
    zones: &Zones,
) -> Result<Cond, String> {
    let Value::Map(c) = item else {
        return Err("each filter must be an object {field, op, value}".into());
    };
    for k in c.keys() {
        if !["field", "op", "value", "bucket", "zone"].contains(&k.as_str()) {
            return Err(format!(
                "unknown filter key {k:?} (use field, op, value, and for a date or datetime bucket and zone)"
            ));
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
    let bucket = bucketed(def, name, &kind, c, zones)?;
    let one = |v: &Value| -> Result<Key, String> {
        if *v == Value::Null {
            return Err(format!(
                "filter on {}.{name}: use is_null to test for empty",
                def.name
            ));
        }
        match &bucket {
            // A bucketed condition compares period labels, which sort in time order as text.
            Some(b) => match v {
                Value::Text(t) if label_ok(b.bucket, t) => Ok(Key::Str(t.clone())),
                _ => Err(format!(
                    "filter on {}.{name}: with bucket {} the value must be a {} label such as {}",
                    def.name,
                    b.bucket.name(),
                    b.bucket.name(),
                    match b.bucket {
                        Bucket::Day => "2026-04-01",
                        Bucket::Week => "2026-03-30 (the Monday)",
                        Bucket::IsoWeek => "2026-W14",
                        Bucket::Month => "2026-04",
                        Bucket::Year => "2026",
                    }
                )),
            },
            None => operand(def.name, name, &kind, v),
        }
    };
    let ranged = !matches!(kind, FieldKind::Ref(_) | FieldKind::Bool);
    let op = match op.as_str() {
        "eq" => Op::Eq(one(value)?),
        "ne" => Op::Ne(one(value)?),
        "in" => {
            let Value::List(vs) = value else {
                return Err(format!(
                    "filter on {}.{name}: `in` needs an array",
                    def.name
                ));
            };
            if vs.len() > MAX_IN {
                return Err(format!("`in` takes at most {MAX_IN} values"));
            }
            Op::In(vs.iter().map(&one).collect::<Result<_, _>>()?)
        }
        "contains" => {
            if !matches!(kind, FieldKind::Text | FieldKind::LongText) || bucket.is_some() {
                return Err(format!(
                    "filter on {}.{name}: contains only works on text fields",
                    def.name
                ));
            }
            match value {
                Value::Text(t) => {
                    no_nul(def.name, name, t)?;
                    Op::Contains(t.to_lowercase())
                }
                _ => {
                    return Err(format!(
                        "filter on {}.{name}: contains needs a string",
                        def.name
                    ));
                }
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
        bucket,
    })
}

/// The `bucket` and `zone` of a condition, checked against the kind of the field.
fn bucketed(
    def: &ModelDef,
    name: &str,
    kind: &FieldKind,
    c: &std::collections::BTreeMap<String, Value>,
    zones: &Zones,
) -> Result<Option<Bucketed>, String> {
    let at = || format!("filter on {}.{name}", def.name);
    let given = |k: &str| c.get(k).filter(|v| **v != Value::Null);
    let Some(b) = given("bucket") else {
        if given("zone").is_some() {
            return Err(format!("{}: zone needs a bucket", at()));
        }
        return Ok(None);
    };
    if !matches!(kind, FieldKind::Date | FieldKind::Datetime) {
        return Err(format!(
            "{}: bucket only applies to date and datetime fields",
            at()
        ));
    }
    let bucket = match b {
        Value::Text(t) => Bucket::parse(t),
        _ => None,
    }
    .ok_or_else(|| format!("{}: bucket must be {}", at(), Bucket::NAMES))?;
    let zone = match (kind, given("zone")) {
        (FieldKind::Datetime, None) => Some(zones.zone(None).map_err(|e| e.to_string())?),
        (FieldKind::Datetime, Some(Value::Text(z))) => {
            Some(zones.zone(Some(z)).map_err(|e| format!("{}: {e}", at()))?)
        }
        (FieldKind::Datetime, Some(_)) => {
            return Err(format!("{}: zone must be an IANA time zone name", at()));
        }
        (_, Some(_)) => {
            return Err(format!(
                "{}: a date has no time zone; zone only applies to datetime fields",
                at()
            ));
        }
        (_, None) => None,
    };
    Ok(Some(Bucketed { bucket, zone }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use obop_types::DataClass;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    fn field(name: &'static str, kind: FieldKind) -> obop_registry::FieldDef {
        obop_registry::FieldDef::new(name, "d", DataClass::None, false, "t.user").kind(kind)
    }

    fn model() -> ModelDef {
        ModelDef {
            name: "t_thing",
            description: "d",
            read_group: "t.user",
            write_group: "t.user",
            fields: vec![
                field("title", FieldKind::Text),
                field("notes", FieldKind::LongText),
                field("owner", FieldKind::Ref("t_user")),
                field("state", FieldKind::Select(Arc::from(vec!["open", "done"]))),
                field("due", FieldKind::Date),
                field(
                    "price",
                    FieldKind::Decimal {
                        precision: 12,
                        scale: 2,
                        round: None,
                    },
                ),
                field("at", FieldKind::Datetime),
            ],
            commands: vec![],
            access: None,
            ext: Default::default(),
        }
    }

    fn text(s: &str) -> Value {
        Value::Text(s.into())
    }

    fn map(pairs: Vec<(&str, Value)>) -> Value {
        Value::Map(
            pairs
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect::<BTreeMap<_, _>>(),
        )
    }

    fn parse(filter: Vec<Value>) -> Result<Query, String> {
        Query::parse(
            &model(),
            &map(vec![("filter", Value::List(filter))]),
            &|_| true,
        )
    }

    fn cond(field: &str, op: &str, value: Value) -> Value {
        map(vec![
            ("field", text(field)),
            ("op", text(op)),
            ("value", value),
        ])
    }

    #[test]
    fn a_nul_in_a_filter_value_is_refused_wherever_a_text_is_compared() {
        for field in ["title", "notes", "owner", "id"] {
            for op in ["eq", "ne", "gt", "gte", "lt", "lte", "contains"] {
                if field == "owner" && matches!(op, "gt" | "gte" | "lt" | "lte" | "contains") {
                    continue; // a ref has no range and no contains
                }
                for bad in ["\0", "a\0", "\0b", "a\0b", "caf\u{e9}\0"] {
                    let e = parse(vec![cond(field, op, text(bad))]).unwrap_err();
                    assert_eq!(
                        e,
                        format!("filter on t_thing.{field}: text cannot contain NUL"),
                        "{field} {op} {bad:?}"
                    );
                }
            }
            // Inside `in`, in any position.
            for list in [vec!["\0"], vec!["a", "b\0"], vec!["\0", "a"]] {
                let items = list.into_iter().map(text).collect();
                let e = parse(vec![cond(field, "in", Value::List(items))]).unwrap_err();
                assert!(e.ends_with("text cannot contain NUL"), "{field}: {e}");
            }
            // Text without a NUL is as welcome as before, including its odd characters.
            for ok in [
                "",
                "a b",
                "%_\\'\"",
                "\u{fffd}",
                "\u{1}",
                "caf\u{e9}",
                "\u{10ffff}",
            ] {
                parse(vec![cond(field, "eq", text(ok))]).unwrap_or_else(|e| panic!("{ok:?}: {e}"));
            }
        }
        // Kinds that cannot hold a NUL anyway keep their own error.
        for (f, bad) in [
            ("state", "\0"),
            ("due", "2026-01-01\0"),
            ("price", "1\0"),
            ("at", "x\0"),
        ] {
            let e = parse(vec![cond(f, "eq", text(bad))]).unwrap_err();
            assert!(!e.contains("NUL"), "{f}: {e}");
        }
        // The empty-check is untouched.
        parse(vec![cond("title", "is_null", Value::Bool(true))]).unwrap();
    }

    #[test]
    fn a_stored_instant_counts_only_in_its_canonical_form() {
        let at = |s: &str| stored_key(&FieldKind::Datetime, &J::from(s));
        let micros = |s: &str| Instant::parse(s).unwrap().micros();
        for ok in [
            "2026-03-29T01:30:00.000000Z",
            "0001-01-01T00:00:00.000000Z",
            "9999-12-31T23:59:59.999999Z",
            "2024-02-29T12:00:00.123456Z",
        ] {
            assert_eq!(at(ok), Some(Key::Int(micros(ok))), "{ok}");
        }
        // Valid RFC 3339, but not what the stores write: the other spellings are empty.
        for other in [
            "2026-03-29T01:30:00Z",
            "2026-03-29T01:30:00.5Z",
            "2026-03-29T03:30:00.000000+02:00",
            "2026-03-29T01:30:00.000000-00:00",
            "2026-03-29t01:30:00.000000Z",
            "2026-03-29T01:30:00.000000z",
            "2026-03-29 01:30:00.000000Z",
            " 2026-03-29T01:30:00.000000Z",
            "2026-03-29T01:30:00.000000Z ",
            "2026-03-29T01:30:00.000000Z\n",
            "2026-02-30T01:30:00.000000Z",
            "2026-03-29T24:00:00.000000Z",
            "2026-03-29T23:60:00.000000Z",
            "2026-03-29T23:59:60.000000Z",
            "0000-01-01T00:00:00.000000Z",
            "\u{ff12}026-03-29T01:30:00.000000Z",
            "",
        ] {
            assert_eq!(at(other), None, "{other:?}");
        }
        assert_eq!(at("not a time"), None);
        assert_eq!(stored_key(&FieldKind::Datetime, &J::from(5)), None);
        // The bucket filter reads stored instants the same way.
        let b = Bucketed {
            bucket: Bucket::Day,
            zone: Some(Zones::default().zone(None).unwrap()),
        };
        let canonical = J::from("2026-03-29T01:30:00.000000Z");
        assert!(bucket_key(&FieldKind::Datetime, &canonical, &b).is_some());
        let other = J::from("2026-03-29T01:30:00Z");
        assert!(bucket_key(&FieldKind::Datetime, &other, &b).is_none());
    }

    #[test]
    fn a_stored_decimal_counts_when_the_strict_grammar_accepts_it() {
        // The rule of the SQL slot too: grammar and limits of Decimal::parse, whatever the scale
        // the field declares (that is checked when a value is written).
        let kind = FieldKind::Decimal {
            precision: 12,
            scale: 2,
            round: None,
        };
        let d = |s: &str| stored_key(&kind, &J::from(s));
        let val = |s: &str| Some(Key::Dec(Decimal::parse(s).unwrap()));
        let nines = "9".repeat(38);
        for ok in [
            "0",
            "-0",
            "7",
            "-0.05",
            "12.3400",
            "007.5",
            "1.234567890123",
            &nines,
        ] {
            assert_eq!(d(ok), val(ok), "{ok}");
        }
        assert_eq!(d("1.50"), d("1.5"), "compared by value, not by scale");
        let long = format!("0.{}1", "0".repeat(37)); // 38 fractional digits: the most
        assert!(d(&long).is_some());
        let (too_many, too_fine, too_long) = (
            "9".repeat(39),
            format!("0.{}1", "0".repeat(38)), // 39 fractional digits
            format!("1{}", "0".repeat(80)),   // 81 characters
        );
        for bad in [
            "", "-", "+5", ".5", "5.", "1e3", "1E3", " 1", "1 ", "1\n", "1,5", "NaN", "Infinity",
            "0x10", "\u{ff11}", "\u{661}", "1_000", "--1", "1.2.3", &too_many, &too_fine,
            &too_long,
        ] {
            assert_eq!(d(bad), None, "{bad:?}");
        }
        assert_eq!(
            stored_key(&kind, &J::from(5)),
            None,
            "a number is not a decimal"
        );
        assert_eq!(stored_key(&kind, &J::from(5.5)), None);
    }

    #[test]
    fn every_kind_has_a_slot_that_reads_it_like_stored_key() {
        let all: Vec<(FieldKind, Option<Slot>)> = vec![
            (FieldKind::Text, Some(Slot::Str)),
            (FieldKind::LongText, Some(Slot::Str)),
            (FieldKind::Ref("x"), Some(Slot::Str)),
            (FieldKind::Number, Some(Slot::Int)),
            (FieldKind::Money, Some(Slot::Int)),
            (FieldKind::Bool, Some(Slot::Bool)),
            (FieldKind::Date, Some(Slot::Date)),
            (FieldKind::Datetime, Some(Slot::Instant)),
            (
                FieldKind::Decimal {
                    precision: 5,
                    scale: 2,
                    round: None,
                },
                Some(Slot::Dec),
            ),
            (FieldKind::Select(Arc::from(vec!["a"])), Some(Slot::SelPos)),
            (FieldKind::Image(0), None),
            (FieldKind::File(0), None),
        ];
        for (kind, slot) in &all {
            assert_eq!(slot_of(kind), *slot, "{kind:?}");
        }
        // Images and files have no key at all, whatever is stored.
        for v in [J::from("a"), J::from(1), J::Null, J::from(true)] {
            assert_eq!(stored_key(&FieldKind::Image(0), &v), None);
            assert_eq!(stored_key(&FieldKind::File(0), &v), None);
        }
    }

    #[test]
    fn the_parts_of_a_parsed_query_are_what_was_asked() {
        let q = Query::parse(
            &model(),
            &map(vec![
                ("filter", Value::List(vec![cond("title", "eq", text("a"))])),
                (
                    "order",
                    Value::List(vec![map(vec![
                        ("field", text("due")),
                        ("dir", text("desc")),
                    ])]),
                ),
                ("limit", Value::Int(7)),
                ("offset", Value::Int(3)),
                ("lang", text("sv")),
            ]),
            &|_| true,
        )
        .unwrap();
        let p = q.parts();
        assert_eq!(
            (p.conds.len(), p.order.len(), p.limit, p.offset),
            (1, 1, Some(7), 3)
        );
        assert!(p.order[0].desc && p.order[0].field == "due");
        assert!(matches!(&p.conds[0].op, Op::Eq(Key::Str(s)) if s == "a"));
        assert_eq!(p.lang.map(ToString::to_string).as_deref(), Some("sv"));
    }
}
