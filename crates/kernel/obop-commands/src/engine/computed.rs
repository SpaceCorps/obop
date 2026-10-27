//! Computed fields (D57), evaluated when a record is read.
//!
//! Why on read and not stored: a stored total can drift from its lines whenever any write path
//! forgets to update it (an effect, an import, a direct store write, a child edited by another
//! model's action); a value computed from the current children cannot. The price is that a read
//! of a parent costs a look at its children, which is cheap for one record (an indexed lookup)
//! and one scan of the child table for a whole list (children are grouped by parent once), the
//! same order of cost as the in-memory filtering lists already do (D49). Because computed values
//! are put into the record before filtering and sorting, they can be filtered and sorted on like
//! stored fields.

use super::unit::{R, Reads};
use obop_registry::expr::{self, AggFn, DecCtx, Env, Expr, V};
use obop_registry::{FieldKind, ModelDef, Round};
use obop_storage::Fields;
use obop_values::{Decimal, Instant};
use serde_json::Value as J;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

pub(crate) type Lookup<'a> = &'a dyn Fn(&str) -> Option<Arc<ModelDef>>;

/// How deep computed fields may refer to other computed fields and aggregates may nest models.
const MAX_DEPTH: usize = 8;

/// A stored value as the expression language sees it. The kind matters for decimals and
/// instants, which are stored as text but compute as numbers and points in time (D72); a value
/// that is not valid for its kind (data that bypassed validation) counts as empty.
pub(crate) fn to_v(kind: Option<&FieldKind>, j: Option<&J>) -> V {
    match (kind, j) {
        (Some(FieldKind::Decimal { .. }), Some(J::String(s))) => {
            Decimal::parse(s).map_or(V::Null, V::Dec)
        }
        (Some(FieldKind::Datetime), Some(J::String(s))) => {
            // Only the canonical text is an instant (D72); any other spelling is data that
            // bypassed validation and counts as empty, as it does in a filter.
            crate::query::stored_instant(s).map_or(V::Null, |i| V::Instant(i.micros()))
        }
        (_, Some(J::Number(n))) => n.as_i64().map_or(V::Null, V::Int),
        (_, Some(J::String(s))) => V::Text(s.clone()),
        (_, Some(J::Bool(b))) => V::Bool(*b),
        _ => V::Null,
    }
}

/// The value of a computed field as stored JSON. A decimal is rounded to the field's scale with
/// its `round` mode (`half_even` by default) and must fit its precision; a value that cannot be
/// represented is empty, like any computed value that cannot be computed.
pub(crate) fn from_v(kind: &FieldKind, v: V) -> J {
    match (kind, v) {
        (_, V::Null) => J::Null,
        (
            FieldKind::Decimal {
                precision,
                scale,
                round,
            },
            v,
        ) => {
            let d = match v {
                V::Dec(d) => d,
                V::Int(n) => Decimal::from_i64(n),
                _ => return J::Null,
            };
            d.fit(*precision, *scale, round.unwrap_or_default())
                .map_or(J::Null, |d| J::from(d.to_string()))
        }
        (FieldKind::Datetime, V::Instant(us)) => {
            Instant::from_micros(us).map_or(J::Null, |i| J::from(i.canonical()))
        }
        (_, V::Int(n)) => J::from(n),
        (_, V::Text(s)) => J::from(s),
        (_, V::Bool(b)) => J::from(b),
        (_, V::Dec(d)) => J::from(d.to_string()),
        (_, V::Instant(us)) => Instant::from_micros(us).map_or(J::Null, |i| J::from(i.canonical())),
    }
}

/// How a decimal division is cut in the expression of a computed field: at the field's scale,
/// rounded by its mode.
fn context_of(kind: &FieldKind) -> DecCtx {
    match kind {
        FieldKind::Decimal { scale, round, .. } => DecCtx {
            scale: *scale,
            round: round.unwrap_or(Round::HalfEven),
        },
        _ => DecCtx::default(),
    }
}

type Groups = HashMap<String, Vec<Fields>>;

pub(crate) struct Computer<'a> {
    reads: &'a dyn Reads,
    lookup: Lookup<'a>,
    preload: bool,
    depth: RefCell<usize>,
    /// (child model, via) -> parent id -> child rows with their computed fields filled.
    grouped: RefCell<HashMap<(String, String), Rc<Groups>>>,
}

struct RecEnv<'a, 'b> {
    c: &'b Computer<'a>,
    def: &'b ModelDef,
    id: &'b str,
    rec: &'b Fields,
    memo: RefCell<HashMap<String, V>>,
    /// Computed fields being evaluated, to refuse a cycle that slipped past the gate.
    busy: RefCell<Vec<String>>,
}

impl Env for RecEnv<'_, '_> {
    fn field(&self, name: &str) -> Result<V, String> {
        if name == "id" {
            return Ok(V::Text(self.id.to_string()));
        }
        let field = self.def.fields.iter().find(|f| f.name == name);
        let Some((c, kind)) = field.and_then(|f| f.computed.clone().map(|c| (c, &f.kind))) else {
            return Ok(to_v(field.map(|f| &f.kind), self.rec.get(name)));
        };
        if let Some(v) = self.memo.borrow().get(name) {
            return Ok(v.clone());
        }
        if self.busy.borrow().iter().any(|b| b == name) {
            return Err(format!("{}.{name} depends on itself", self.def.name));
        }
        self.busy.borrow_mut().push(name.to_string());
        let v = expr::eval_in(&c.expr, self, context_of(kind));
        self.busy.borrow_mut().pop();
        let v = v?;
        // Every reader sees the value the record shows: a computed decimal is rounded to its
        // own scale and mode before another field, an assertion or an effect reads it.
        let v = match kind {
            FieldKind::Decimal { .. } => to_v(Some(kind), Some(&from_v(kind, v))),
            _ => v,
        };
        self.memo.borrow_mut().insert(name.to_string(), v.clone());
        Ok(v)
    }

    fn aggregate(
        &self,
        func: AggFn,
        child: &str,
        via: Option<&str>,
        field: Option<&str>,
        filter: Option<&Expr>,
    ) -> Result<V, String> {
        let (cm, via) = expr::resolve_child(self.def, child, via, self.c.lookup)?;
        let rows = self.c.children(&cm, via, self.id)?;
        // `where`: keep the children for which the condition holds.
        let rows: Vec<&Fields> = match filter {
            None => rows.iter().collect(),
            Some(w) => {
                let mut keep = Vec::new();
                for r in rows.iter() {
                    let env = RecEnv {
                        c: self.c,
                        def: &cm,
                        id: "",
                        rec: r,
                        memo: RefCell::new(HashMap::new()),
                        busy: RefCell::new(Vec::new()),
                    };
                    if expr::eval(w, &env)? == V::Bool(true) {
                        keep.push(r);
                    }
                }
                keep
            }
        };
        if func == AggFn::Count {
            return Ok(V::Int(rows.len() as i64));
        }
        let Some(field) = field else {
            return Err("this aggregate needs a field".into());
        };
        // A decimal child field sums, minimises and maximises exactly.
        if let Some(FieldKind::Decimal { .. }) =
            cm.fields.iter().find(|f| f.name == field).map(|f| &f.kind)
        {
            let mut vals: Vec<Decimal> = Vec::new();
            for r in rows {
                if let Some(J::String(s)) = r.get(field)
                    && let Ok(d) = Decimal::parse(s)
                {
                    vals.push(d);
                }
            }
            return Ok(match func {
                AggFn::Sum => {
                    let scale = vals.iter().map(|d| d.scale()).max().unwrap_or(0);
                    let mut sum = Decimal::zero(scale);
                    for d in &vals {
                        sum = sum.checked_add(*d).map_err(|e| format!("the sum: {e}"))?;
                    }
                    V::Dec(sum)
                }
                AggFn::Min => vals.iter().min().map_or(V::Null, |d| V::Dec(*d)),
                AggFn::Max => vals.iter().max().map_or(V::Null, |d| V::Dec(*d)),
                AggFn::Count => V::Null,
            });
        }
        let mut sum: i128 = 0;
        let (mut min, mut max): (Option<i64>, Option<i64>) = (None, None);
        for r in rows {
            let n = match r.get(field) {
                Some(J::Number(n)) => n.as_i64().unwrap_or(0),
                _ => continue,
            };
            sum += i128::from(n);
            min = Some(min.map_or(n, |m| m.min(n)));
            max = Some(max.map_or(n, |m| m.max(n)));
        }
        Ok(match func {
            AggFn::Sum => V::Int(
                i64::try_from(sum).map_err(|_| "the sum does not fit in 64 bits".to_string())?,
            ),
            AggFn::Min => min.map_or(V::Null, V::Int),
            AggFn::Max => max.map_or(V::Null, V::Int),
            AggFn::Count => V::Null,
        })
    }
}

impl<'a> Computer<'a> {
    pub fn new(reads: &'a dyn Reads, lookup: Lookup<'a>, preload: bool) -> Self {
        Self {
            reads,
            lookup,
            preload,
            depth: RefCell::new(0),
            grouped: RefCell::new(HashMap::new()),
        }
    }

    /// The child rows of `parent_id`, computed fields filled.
    fn children(&self, child: &Arc<ModelDef>, via: &str, parent_id: &str) -> R<Rc<Vec<Fields>>> {
        let key = (child.name.to_string(), via.to_string());
        if self.preload {
            let cached = self.grouped.borrow().get(&key).cloned();
            let groups = match cached {
                Some(g) => g,
                None => {
                    let mut map: HashMap<String, Vec<Fields>> = HashMap::new();
                    let mut rows = self.reads.all(child.name)?;
                    self.fill_all(child, &mut rows);
                    for (_, f) in rows {
                        if let Some(J::String(p)) = f.get(via) {
                            map.entry(p.clone()).or_default().push(f);
                        }
                    }
                    let g = Rc::new(map);
                    self.grouped.borrow_mut().insert(key, g.clone());
                    g
                }
            };
            return Ok(Rc::new(groups.get(parent_id).cloned().unwrap_or_default()));
        }
        let mut rows = self
            .reads
            .find(child.name, via, &J::String(parent_id.to_string()))?;
        self.fill_all(child, &mut rows);
        Ok(Rc::new(rows.into_iter().map(|(_, f)| f).collect()))
    }

    /// Put the computed fields into every row. A computed value that cannot be computed (a
    /// division by zero, an overflow) is empty rather than failing the read.
    pub fn fill_all(&self, def: &ModelDef, rows: &mut [(String, Fields)]) {
        if def.computed_fields().next().is_none() {
            return;
        }
        for (id, rec) in rows.iter_mut() {
            self.fill(def, id, rec);
        }
    }

    pub fn fill(&self, def: &ModelDef, id: &str, rec: &mut Fields) {
        if def.computed_fields().next().is_none() {
            return;
        }
        {
            let mut d = self.depth.borrow_mut();
            if *d >= MAX_DEPTH {
                for f in def.computed_fields() {
                    rec.insert(f.name.to_string(), J::Null);
                }
                return;
            }
            *d += 1;
        }
        let values: Vec<(String, J)> = {
            let env = RecEnv {
                c: self,
                def,
                id,
                rec,
                memo: RefCell::new(HashMap::new()),
                busy: RefCell::new(Vec::new()),
            };
            def.computed_fields()
                .map(|f| {
                    (
                        f.name.to_string(),
                        from_v(&f.kind, env.field(f.name).unwrap_or(V::Null)),
                    )
                })
                .collect()
        };
        *self.depth.borrow_mut() -= 1;
        for (k, v) in values {
            rec.insert(k, v);
        }
    }

    /// Evaluate an expression over a record (assertions and conditions). Errors are reported.
    pub fn evaluate(&self, def: &ModelDef, id: &str, rec: &Fields, e: &Expr) -> R<V> {
        let env = RecEnv {
            c: self,
            def,
            id,
            rec,
            memo: RefCell::new(HashMap::new()),
            busy: RefCell::new(Vec::new()),
        };
        expr::eval(e, &env)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::unit::Plain;
    use obop_registry::{FieldDef, FieldKind};
    use obop_types::DataClass;

    fn f(name: &'static str) -> FieldDef {
        FieldDef::new(name, "d", DataClass::None, false, "t.user")
    }

    fn defs() -> (Arc<ModelDef>, Arc<ModelDef>) {
        let order = ModelDef {
            name: "t_order",
            description: "d",
            read_group: "t.user",
            write_group: "t.user",
            fields: vec![
                f("title"),
                f("discount").kind(FieldKind::Money),
                f("total")
                    .kind(FieldKind::Money)
                    .computed("sum(lines.amount)"),
                f("net").kind(FieldKind::Money).computed("total - discount"),
                f("label").computed("concat(title, \" (\", count(lines), \")\")"),
                f("big").kind(FieldKind::Bool).computed("total >= 1000"),
                f("avg")
                    .kind(FieldKind::Money)
                    .computed("total / count(lines)"),
            ],
            commands: vec![],
            access: None,
            ext: Default::default(),
        };
        let line = ModelDef {
            name: "t_order_line",
            description: "d",
            read_group: "t.user",
            write_group: "t.user",
            fields: vec![
                f("order_id").reference("t_order"),
                f("qty").kind(FieldKind::Number),
                f("price").kind(FieldKind::Money),
                f("amount").kind(FieldKind::Money).computed("qty * price"),
            ],
            commands: vec![],
            access: None,
            ext: Default::default(),
        };
        (Arc::new(order), Arc::new(line))
    }

    fn rec(pairs: &[(&str, J)]) -> Fields {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn totals_come_from_the_current_lines_one_record_or_a_whole_list() {
        let (order, line) = defs();
        let (o, l) = (order.clone(), line.clone());
        let lookup = move |n: &str| match n {
            "t_order" => Some(o.clone()),
            "t_order_line" => Some(l.clone()),
            _ => None,
        };
        let b = obop_storage::open("memory:").unwrap();
        b.insert(
            "t_order",
            "o1",
            &rec(&[("title", "A".into()), ("discount", 50.into())]),
        )
        .unwrap();
        b.insert("t_order", "o2", &rec(&[("title", "B".into())]))
            .unwrap();
        for (id, o, q, p) in [
            ("l1", "o1", 2, 300),
            ("l2", "o1", 1, 1000),
            ("l3", "o2", 5, 7),
        ] {
            b.insert(
                "t_order_line",
                id,
                &rec(&[
                    ("order_id", o.into()),
                    ("qty", q.into()),
                    ("price", p.into()),
                ]),
            )
            .unwrap();
        }
        for preload in [false, true] {
            let reads = Plain(&*b);
            let c = Computer::new(&reads, &lookup, preload);
            let mut rows = reads.all("t_order").unwrap();
            c.fill_all(&order, &mut rows);
            let (o1, o2) = (&rows[0].1, &rows[1].1);
            assert_eq!(o1["total"], J::from(1600), "preload={preload}");
            assert_eq!(o1["net"], J::from(1550), "computed from computed");
            assert_eq!(o1["label"], J::from("A (2)"));
            assert_eq!(o1["big"], J::from(true));
            assert_eq!(o1["avg"], J::from(800));
            assert_eq!(o2["total"], J::from(35));
            assert_eq!(o2["big"], J::from(false));
        }
        // An order with no lines: sum 0, and a division by zero is empty, not an error.
        b.insert("t_order", "o3", &rec(&[("title", "C".into())]))
            .unwrap();
        let reads = Plain(&*b);
        let c = Computer::new(&reads, &lookup, false);
        let mut r = reads.get("t_order", "o3").unwrap().unwrap();
        c.fill(&order, "o3", &mut r);
        assert_eq!(
            (r["total"].clone(), r["avg"].clone()),
            (J::from(0), J::Null)
        );
    }

    /// A stored datetime is an instant in an expression only in its canonical form; any other
    /// spelling counts as empty, as in a filter, in a group and in the SQL `Instant` slot (D72).
    #[test]
    fn a_stored_datetime_is_an_instant_only_in_its_canonical_form() {
        let at = |s: &str| to_v(Some(&FieldKind::Datetime), Some(&J::from(s)));
        let canonical = "2026-03-29T01:30:00.000000Z";
        let micros = Instant::parse(canonical).unwrap().micros();
        assert!(matches!(at(canonical), V::Instant(us) if us == micros));
        for other in [
            "2026-03-29T01:30:00Z",
            "2026-03-29T03:30:00.000000+02:00",
            "2026-03-29t01:30:00.000000Z",
            "2026-02-30T01:30:00.000000Z",
            "not a time",
            "",
        ] {
            assert!(matches!(at(other), V::Null), "{other:?}");
        }
        assert!(matches!(
            to_v(Some(&FieldKind::Datetime), Some(&J::from(5))),
            V::Int(5)
        ));
        assert!(matches!(to_v(Some(&FieldKind::Datetime), None), V::Null));
    }
}
