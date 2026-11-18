//! The D95 hints as `/api/meta` and the `obop__meta` tool serve them: each key only when it is
//! declared, in the shapes of `web/apps/backoffice/src/api.ts` (`ModelUi`, `FieldUi`,
//! `ActionUi`, `ChildMeta`).

use obop_commands::ModelInfo;
use obop_registry::ui::{DefaultLit, Filter};
use obop_registry::{ActionUi, FieldHints, ModelUi};
use serde_json::{Map, Value as J, json};

fn put_some<T: Clone + Into<J>>(o: &mut Map<String, J>, key: &str, v: &Option<T>) {
    if let Some(v) = v {
        o.insert(key.into(), v.clone().into());
    }
}

fn put_list(o: &mut Map<String, J>, key: &str, v: &[String]) {
    if !v.is_empty() {
        o.insert(key.into(), json!(v));
    }
}

fn filter_text(f: &Filter) -> J {
    json!(f.source)
}

/// `default`, `fill_from`, `readonly`, `ref_filter` and `ui` of a field.
pub fn field_hints(o: &mut J, h: &FieldHints) {
    if let Some(d) = &h.default {
        o["default"] = match d.written() {
            DefaultLit::Text(t) => json!(t),
            DefaultLit::Int(n) => json!(n),
            DefaultLit::Bool(b) => json!(b),
        };
    }
    if let Some(p) = &h.fill_from {
        o["fill_from"] = json!(p.written());
    }
    if h.readonly {
        o["readonly"] = json!(true);
    }
    if let Some(f) = &h.ref_filter {
        o["ref_filter"] = filter_text(f);
    }
    let u = &h.ui;
    if !u.is_empty() {
        let mut m = Map::new();
        put_some(&mut m, "display", &u.display);
        put_some(&mut m, "aggregate", &u.aggregate);
        if !u.tones.is_empty() {
            m.insert("tones".into(), json!(u.tones));
        }
        put_some(&mut m, "warn_before_days", &u.warn_before_days);
        if u.hidden {
            m.insert("hidden".into(), json!(true));
        }
        put_some(&mut m, "currency_from", &u.currency_from);
        put_some(&mut m, "readonly_when", &u.readonly_when);
        o["ui"] = J::Object(m);
    }
}

/// `[model.action.ui]`.
pub fn action_ui(u: &ActionUi) -> J {
    let mut m = Map::new();
    if u.primary {
        m.insert("primary".into(), json!(true));
    }
    if u.danger {
        m.insert("danger".into(), json!(true));
    }
    put_some(&mut m, "confirm", &u.confirm);
    put_some(&mut m, "done", &u.done);
    put_some(&mut m, "group", &u.group);
    put_list(&mut m, "needs", &u.needs);
    put_some(&mut m, "not_by", &u.not_by);
    J::Object(m)
}

fn model_ui(u: &ModelUi) -> J {
    let mut m = Map::new();
    put_some(&mut m, "title", &u.title);
    put_some(&mut m, "subtitle", &u.subtitle);
    put_list(&mut m, "list", &u.list);
    put_list(&mut m, "card", &u.card);
    put_some(&mut m, "card_sum", &u.card_sum);
    put_list(&mut m, "quick_create", &u.quick_create);
    put_some(&mut m, "order", &u.order);
    put_some(&mut m, "default_view", &u.default_view);
    put_some(&mut m, "default_filter", &u.default_filter);
    put_some(&mut m, "menu", &u.menu);
    put_some(&mut m, "menu_section", &u.menu_section);
    put_some(&mut m, "menu_order", &u.menu_order);
    if let Some(c) = &u.calendar {
        let mut cm = Map::new();
        cm.insert("start".into(), json!(c.start));
        put_some(&mut cm, "end", &c.end);
        put_some(&mut cm, "color", &c.color);
        m.insert("calendar".into(), J::Object(cm));
    }
    if let Some(r) = &u.report {
        let mut rm = Map::new();
        put_some(&mut rm, "rows", &r.rows);
        put_some(&mut rm, "cols", &r.cols);
        put_some(&mut rm, "measure", &r.measure);
        m.insert("report".into(), J::Object(rm));
    }
    if !u.sections.is_empty() {
        let s: Vec<J> = u
            .sections
            .iter()
            .map(|s| {
                let mut o = json!({"label": s.label, "fields": s.fields});
                if s.collapsed {
                    o["collapsed"] = json!(true);
                }
                if !s.show_in.is_empty() {
                    o["show_in"] = json!(s.show_in);
                }
                o
            })
            .collect();
        m.insert("section".into(), J::Array(s));
    }
    if !u.filters.is_empty() {
        let f: Vec<J> = u
            .filters
            .iter()
            .map(|f| json!({"label": f.label, "where": filter_text(&f.filter)}))
            .collect();
        m.insert("filter".into(), J::Array(f));
    }
    put_some(&mut m, "public_url", &u.public_url);
    J::Object(m)
}

/// `ui`, `children`, `plural` and `plural_labels` of a model.
pub fn model_hints(o: &mut J, x: &ModelInfo) {
    if let Some(u) = &x.ui {
        let ui = model_ui(u);
        if ui.as_object().is_some_and(|m| !m.is_empty()) {
            o["ui"] = ui;
        }
    }
    if !x.children.is_empty() {
        o["children"] = J::Array(
            x.children
                .iter()
                .map(|c| {
                    let mut j = json!({"model": c.model, "via": c.via});
                    if let Some(s) = &c.shape {
                        j["shape"] = json!(s);
                    }
                    j
                })
                .collect(),
        );
    }
    if let Some(p) = x.plurals.get("en") {
        o["plural"] = json!(p);
    }
    if !x.plurals.is_empty() {
        o["plural_labels"] = json!(x.plurals);
    }
    if let Some(g) = &x.guard {
        o["guard"] = guard(g);
        if !x.guard_messages.is_empty() {
            o["guard"]["message_labels"] = json!(x.guard_messages);
        }
    }
}

/// `[model.guard]` (D77) in the shape of `ModelGuard` in `web/apps/backoffice/src/api.ts`: each
/// key only when declared.
pub fn guard(g: &obop_registry::Guard) -> J {
    let states = |s: &obop_registry::StateSet| json!({"field": s.field, "values": s.values});
    let mut m = Map::new();
    if let Some(s) = &g.immutable_when {
        m.insert("immutable_when".into(), states(s));
    }
    if !g.mutable_fields.is_empty() {
        m.insert("mutable_fields".into(), json!(g.mutable_fields));
    }
    if !g.exit_to.is_empty() {
        let e: Vec<J> = g
            .exit_to
            .iter()
            .map(|e| {
                let mut o = json!({"to": e.to, "action": e.action});
                if !e.may_change.is_empty() {
                    o["may_change"] = json!(e.may_change);
                }
                o
            })
            .collect();
        m.insert("exit_to".into(), J::Array(e));
    }
    if let Some(s) = &g.no_delete_when {
        m.insert("no_delete_when".into(), states(s));
    }
    if let Some(l) = &g.lock_date {
        let mut o = json!({"field": l.field});
        if let Some(s) = l.setting {
            o["setting"] = json!(s);
        }
        if let Some(c) = l.closed_by {
            o["closed_by"] = json!(c);
        }
        m.insert("lock_date".into(), o);
    }
    if let Some(c) = g.children_of {
        m.insert("children_of".into(), json!(c));
    }
    if g.append_only {
        m.insert("append_only".into(), json!(true));
    }
    if let Some(t) = g.message {
        m.insert("message".into(), json!(t));
    }
    J::Object(m)
}
