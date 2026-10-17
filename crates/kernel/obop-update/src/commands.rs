//! The `instance_update` admin commands (D63): `check`, `status`, `start` and `rollback`.
//!
//! The model is declared with the other admin models in `obop-pkg`; this binds the handlers.
//! `check` and `status` only read. `start` and `rollback` need an administrator and, because
//! they change what the whole instance runs, an agent can only propose them (the approval
//! design of D18).

use crate::updater::{Check, ReleaseView, Status, Updater};
use obop_commands::{Dispatcher, Value};
use obop_pkg::admin_module;
use std::collections::BTreeMap;
use std::sync::Arc;

fn t(v: &str) -> Value {
    Value::text(v)
}

fn caller(i: &BTreeMap<String, Value>) -> String {
    match i.get("_caller") {
        Some(Value::Text(t)) => t.clone(),
        _ => "unknown".into(),
    }
}

fn release_value(r: &ReleaseView) -> Value {
    Value::map([
        ("version", t(&r.release.version)),
        ("published_at", Value::Int(r.release.published_at as i64)),
        ("notes", t(&r.release.notes)),
        ("image", t(&r.release.image)),
        ("min_from", t(&r.release.min_from)),
        ("newer", Value::Bool(r.newer)),
        ("installable", Value::Bool(r.installable)),
        (
            "blockers",
            Value::List(r.blockers.iter().map(|b| t(b)).collect()),
        ),
        (
            "warnings",
            Value::List(r.warnings.iter().map(|b| t(b)).collect()),
        ),
        (
            "modules",
            Value::List(
                r.modules
                    .iter()
                    .map(|m| {
                        Value::map([
                            ("id", t(&m.id)),
                            ("name", t(&m.name)),
                            ("version", t(&m.version)),
                            ("requires_core", t(&m.requires_core)),
                            ("compatible", Value::Bool(m.compatible)),
                            (
                                "reasons",
                                Value::List(m.reasons.iter().map(|x| t(x)).collect()),
                            ),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
}

pub fn check_value(c: &Check, s: &Status) -> Value {
    let Value::Map(mut m) = status_value(s) else {
        unreachable!()
    };
    m.insert("mode".into(), t(c.mode.as_str()));
    m.insert("current_version".into(), t(&c.current));
    m.insert(
        "latest_version".into(),
        c.latest.as_deref().map_or(Value::Null, t),
    );
    m.insert("update_available".into(), Value::Bool(c.update_available));
    m.insert("feed".into(), t(&c.feed));
    m.insert(
        "feed_error".into(),
        c.feed_error.as_deref().map_or(Value::Null, t),
    );
    m.insert("checked_at".into(), Value::Int(c.checked_at as i64));
    m.insert(
        "releases".into(),
        Value::List(c.releases.iter().map(release_value).collect()),
    );
    m.insert(
        "dropped".into(),
        Value::List(c.dropped.iter().map(|d| t(d)).collect()),
    );
    m.insert(
        "instructions".into(),
        Value::List(c.instructions.iter().map(|d| t(d)).collect()),
    );
    m.insert(
        "backup_dir".into(),
        c.backup_dir.as_deref().map_or(Value::Null, t),
    );
    Value::Map(m)
}

pub fn status_value(s: &Status) -> Value {
    Value::map([
        ("state", t(&s.state)),
        ("kind", t(&s.kind)),
        ("target_version", t(&s.target)),
        ("from_version", t(&s.from)),
        ("phase", t(&s.phase)),
        ("error", t(&s.error)),
        ("backup", t(&s.backup)),
        ("started_at", Value::Int(s.started_at as i64)),
        ("started_by", t(&s.started_by)),
        ("finished_at", Value::Int(s.finished_at as i64)),
        ("current_version", t(&s.current)),
        ("can_rollback", Value::Bool(s.can_rollback)),
    ])
}

/// Bind the handlers. Called once at start-up, after `register_admin`.
pub fn register(d: &Dispatcher, up: &Arc<Updater>) {
    let m = admin_module();
    let u = up.clone();
    d.register(
        &m,
        "instance_update",
        "check",
        Box::new(move |_| {
            let s = u.status();
            Ok(check_value(&u.check(), &s))
        }),
    );
    let u = up.clone();
    d.register(
        &m,
        "instance_update",
        "status",
        Box::new(move |_| {
            let s = u.status();
            let Value::Map(mut v) = status_value(&s) else {
                unreachable!()
            };
            v.insert("mode".into(), t(u.mode().as_str()));
            Ok(Value::Map(v))
        }),
    );
    let u = up.clone();
    d.register(
        &m,
        "instance_update",
        "start",
        Box::new(move |input| {
            let Value::Map(x) = input else {
                return Err("expected an object".into());
            };
            let Some(Value::Text(version)) = x.get("version") else {
                return Err("give the published version to update to (version)".into());
            };
            let skip = matches!(x.get("skip_backup"), Some(Value::Bool(true)));
            u.start(version, &caller(x), skip).map(|s| status_value(&s))
        }),
    );
    let u = up.clone();
    d.register(
        &m,
        "instance_update",
        "rollback",
        Box::new(move |input| {
            let Value::Map(x) = input else {
                return Err("expected an object".into());
            };
            u.rollback(&caller(x)).map(|s| status_value(&s))
        }),
    );
}
