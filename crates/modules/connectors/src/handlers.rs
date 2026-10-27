//! Handlers: what each command does, over the shared runtime.

use obop_commands::query::Query;
use obop_commands::{Dispatcher, Store, Value};
use obop_connectors::Runtime;
use obop_connectors::catalog::Provider;
use obop_connectors::model::*;
use obop_connectors::policy::{Policy, Why};
use obop_connectors::util::{bool_of, iso, now, str_of};
use obop_registry::Module;
use obop_storage::Fields;
use obop_types::DataClass;
use serde_json::{Map, Value as J, json};
use std::collections::BTreeSet;
use std::sync::Arc;

use crate::guards::blocked_model;
use crate::h::*;
use crate::meta::{LEGAL_BASES, USER};

type Rows = Arc<dyn Fn(&M) -> Vec<(String, Fields)> + Send + Sync>;

/// A read-only model over computed rows: `get`, `list` and `search`, with the same filter, order
/// and paging as the generated commands.
fn virtual_model(d: &Dispatcher, m: &Module, model: &'static str, rows: Rows) {
    let def = m
        .models
        .iter()
        .find(|x| x.name == model)
        .expect("declared")
        .clone();
    let r = rows.clone();
    d.register(
        m,
        model,
        "get",
        Box::new(move |input| {
            let mm = map(input)?;
            let id = txt(mm, "id");
            r(mm)
                .into_iter()
                .find(|(i, _)| i == id)
                .map(|(i, f)| row(&i, &f))
                .ok_or_else(|| format!("no {model} {id}"))
        }),
    );
    let (r, dd) = (rows.clone(), def.clone());
    d.register(
        m,
        model,
        "list",
        Box::new(move |input| {
            let q = Query::parse(&dd, input, &|_| true)?;
            let (page, _) = q.apply(r(map(input)?));
            Ok(Value::List(page.iter().map(|(i, f)| row(i, f)).collect()))
        }),
    );
    d.register(
        m,
        model,
        "search",
        Box::new(move |input| {
            let q = Query::parse(&def, input, &|_| true)?;
            let (page, total) = q.apply(rows(map(input)?));
            Ok(Value::map([
                (
                    "rows",
                    Value::List(page.iter().map(|(i, f)| row(i, f)).collect()),
                ),
                ("total", Value::Int(total as i64)),
            ]))
        }),
    );
}

fn provider_fields(rt: &Runtime, p: &Provider) -> Fields {
    let pp = provider_policy(&*rt.backend, p.id).unwrap_or_default();
    let j = |v: J| v;
    let mut f = Fields::new();
    let mut put = |k: &str, v: J| {
        f.insert(k.into(), v);
    };
    put("name", j(p.name.into()));
    put("category", j(p.category.into()));
    put("maturity", j(p.maturity.as_str().into()));
    put("engine", j(p.engine.as_str().into()));
    put("auth_kind", j(p.auth_kind.into()));
    put("direction", j(p.direction.into()));
    put("max_classes", j(p.max_classes_csv().into()));
    put("purpose", j(p.purpose.into()));
    put("description", j(p.description.into()));
    put("legal_name", j(p.legal_name.into()));
    put("country", j(p.country.into()));
    put("dpa_url", j(p.dpa_url.into()));
    put("eu_adequate", j(p.eu_adequate.into()));
    put("verify_note", j(p.verify_note.into()));
    put("allowed_hosts", j(p.allowed_hosts.join(",").into()));
    put("config_fields", j(p.config_json().to_string().into()));
    put("setup", j(p.setup.into()));
    put("webhook_scheme", j(p.webhook_scheme.into()));
    put(
        "oauth_scopes",
        j(p.oauth.as_ref().map_or("", |o| o.scopes).into()),
    );
    put("enabled", j(pp.enabled.into()));
    put("users_may_use", j(pp.users_may_use.into()));
    put("dpa_recorded", j(pp.dpa_recorded.into()));
    if !pp.legal_basis.is_empty() {
        put("legal_basis", j(pp.legal_basis.clone().into()));
    }
    put("legal_basis_text", j(pp.legal_basis_text.clone().into()));
    f
}

/// The instance policy as the record shows it. A policy the store would not give is an error, not
/// the closed default (D74): an operator who sees, or a first save that stores, "flows off, EU
/// only, no hosts" has been shown a policy nobody chose.
fn instance_fields(rt: &Runtime) -> Result<Fields, String> {
    let p = InstancePolicy::try_load(&*rt.backend)?;
    let mut f = Fields::new();
    f.insert("flows_enabled".into(), p.flows_enabled.into());
    f.insert("eu_only".into(), p.eu_only.into());
    f.insert("require_dpa".into(), p.require_dpa.into());
    f.insert("host_allowlist".into(), p.hosts.join(",").into());
    f.insert(
        "default_retention_days".into(),
        p.default_retention_days.into(),
    );
    Ok(f)
}

fn result(id: &str, extra: &[(&str, Value)], res: &J) -> Value {
    let mut m = M::new();
    m.insert("id".into(), Value::text(id));
    for (k, v) in extra {
        m.insert((*k).into(), v.clone());
    }
    m.insert("result".into(), Value::Text(res.to_string()));
    Value::Map(m)
}

fn set(rt: &Runtime, model: &str, id: &str, pairs: &[(&str, J)]) -> Result<(), String> {
    if rt.backend.update(model, id, &fields_of(pairs))? {
        Ok(())
    } else {
        Err(format!("no {model} {id}"))
    }
}

fn id_of(input: &Value) -> Result<String, String> {
    Ok(txt(map(input)?, "id").to_string())
}

fn caller(input: &Value) -> String {
    map(input)
        .map(|m| txt(m, "_caller").to_string())
        .unwrap_or_default()
}

// ---- connection lifecycle ---------------------------------------------------------------------

fn precheck_connect(rt: &Runtime, c: &Connection, p: &Provider) -> Result<(), String> {
    if p.engine == obop_connectors::Engine::None {
        return Err(format!("{} is a definition only; nothing runs yet", p.name));
    }
    let pp = provider_policy(&*rt.backend, p.id).filter(|x| x.enabled);
    let Some(pp) = pp else {
        return Err(format!(
            "{} is not enabled: an administrator enables it under Provider policy, where the agreement with the provider is recorded",
            p.name
        ));
    };
    if c.owner_scope == "user"
        && (!pp.users_may_use
            || !c.owner_groups.iter().any(|g| {
                g == if pp.allowed_group.is_empty() {
                    USER
                } else {
                    &pp.allowed_group
                } || g == "platform.admin"
            }))
    {
        return Err(format!(
            "{} is not enabled for people to connect their own accounts",
            p.name
        ));
    }
    let consents = try_consents_of(&*rt.backend, &c.id)?;
    let Some(consent) = consents.iter().find(|k| k.is_active()) else {
        return Err(
            "record the legal basis first: purpose, basis and the data that would flow".into(),
        );
    };
    if let Some(label) = rt.missing_setting(c, p)? {
        return Err(format!("{label} is missing"));
    }
    let models: Vec<&String> = c
        .allowlist
        .iter()
        .filter(|(_, f)| !f.is_empty())
        .map(|(m, _)| m)
        .collect();
    if models.is_empty() {
        return Err(
            "choose at least one field for the allowlist: an empty allowlist means nothing flows"
                .into(),
        );
    }
    if matches!(
        p.engine,
        obop_connectors::Engine::WebhookIn | obop_connectors::Engine::RestPull
    ) {
        let target = c.cfg("target_model");
        if target.is_empty() {
            return Err("set the target model".into());
        }
        if !c.allowlist.get(target).is_some_and(|f| !f.is_empty()) {
            return Err(format!("the allowlist has no fields for {target}"));
        }
    }
    let mut uncovered = BTreeSet::new();
    for model in models {
        if let Ok(plan) = rt.egress.plan(c, p, model) {
            for class in plan.classes() {
                if class != DataClass::None && !consent.classes.contains(&class) {
                    uncovered.insert(class.as_str());
                }
            }
        }
    }
    if !uncovered.is_empty() {
        return Err(format!(
            "the recorded legal basis does not cover the data classes {}; record it again",
            uncovered.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    Ok(())
}

/// Revoke a connection: it stops (state first, from here nothing flows), what it holds is
/// forgotten, and the consent records of it are withdrawn, which are the evidence that the basis
/// ended. Every step after the first is needed once the state has changed: tried again while the
/// store refuses, and a lasting refusal makes the request a partial one (503) whose repeat
/// finishes what is left. The consents are read with the lookup that reports a refusal: read as
/// "none" they stayed active on a revoked connection under an answer of "revoked".
fn revoke_connection(rt: &Runtime, id: &str, by: &str) -> Result<usize, String> {
    use obop_storage::fault::needed;
    set(rt, CONNECTION, id, &[("state", "revoked".into())])?;
    let purged = rt.revoke(id)?;
    for k in needed(|| try_consents_of(&*rt.backend, id))?
        .iter()
        .filter(|k| k.is_active())
    {
        needed(|| withdraw_consent(rt, &k.id, by))?;
    }
    Ok(purged)
}

fn withdraw_consent(rt: &Runtime, id: &str, by: &str) -> Result<(), String> {
    set(
        rt,
        CONSENT,
        id,
        &[
            ("state", "withdrawn".into()),
            ("withdrawn_at", iso(now()).into()),
            ("withdrawn_by", by.into()),
        ],
    )
}

fn connection_command(rt: &Arc<Runtime>, d: &Dispatcher, m: &Module, name: &'static str) {
    let rt = rt.clone();
    d.register(
        m,
        "connection",
        name,
        Box::new(move |input| {
            let id = id_of(input)?;
            let (c, p) = rt.connection(&id)?;
            let to = match name {
                "connect" | "resume" | "retry" => {
                    precheck_connect(&rt, &c, &p)?;
                    "connected"
                }
                "pause" => {
                    rt.queue.drop_connection(&id)?;
                    "paused"
                }
                _ => {
                    revoke_connection(&rt, &id, &caller(input))?;
                    "revoked"
                }
            };
            if name != "revoke" {
                set(&rt, CONNECTION, &id, &[("state", to.into())])?;
            }
            Ok(Value::map([
                ("id", Value::text(&id)),
                ("state", Value::text(to)),
            ]))
        }),
    );
}

// ---- consent ------------------------------------------------------------------------------------

fn consent_create(rt: &Runtime, input: &Value) -> Result<Value, String> {
    let m = map(input)?;
    let a = actor(m)?;
    let id = txt(m, "connection");
    let (c, p) = rt
        .connection(id)
        .map_err(|_| format!("no connection {id}"))?;
    if !a.admin && c.owner_user != a.id {
        return Err(format!("no connection {id}"));
    }
    if c.state == "revoked" {
        return Err("the connection is revoked; start a new one".into());
    }
    let basis = txt(m, "legal_basis");
    if !LEGAL_BASES.contains(&basis) {
        return Err(format!(
            "consent.legal_basis must be one of: {}",
            LEGAL_BASES.join(", ")
        ));
    }
    if txt(m, "purpose").trim().chars().count() < 10 {
        return Err(
            "consent.purpose must say what the data is used for (at least 10 characters)".into(),
        );
    }
    if let Some(pp) = provider_policy(&*rt.backend, p.id)
        && !a.admin
        && !pp.legal_basis.is_empty()
        && pp.legal_basis != basis
    {
        return Err(format!(
            "the administrator set the legal basis for {} to {}",
            p.name, pp.legal_basis
        ));
    }
    if basis == "legitimate_interest" && txt(m, "note").trim().chars().count() < 20 {
        return Err("legitimate interest needs the balancing test written down in the note (at least 20 characters)".into());
    }
    let mut classes: BTreeSet<DataClass> = BTreeSet::new();
    let mut summary = Map::new();
    for model in c.allowlist.keys() {
        if let Ok(plan) = rt.egress.plan(&c, &p, model) {
            let sent: Vec<&str> = plan.sent().map(|f| f.name.as_str()).collect();
            if !sent.is_empty() {
                summary.insert(model.clone(), json!(sent));
            }
            classes.extend(plan.classes());
        }
    }
    let kind = match txt(m, "subject_kind") {
        "" if c.owner_scope == "user" => "user",
        "" => "instance",
        k => k,
    };
    if !["user", "party", "instance"].contains(&kind) {
        return Err("consent.subject_kind must be one of: user, party, instance".into());
    }
    let f = json!({
        "connection": c.id, "owner_user": c.owner_user, "state": "active", "subject_kind": kind,
        "subject": txt(m, "subject"), "given_by": a.id, "purpose": txt(m, "purpose").trim(),
        "legal_basis": basis, "note": txt(m, "note"),
        "classes": classes.iter().map(|c| c.as_str()).collect::<Vec<_>>().join(","),
        "fields_summary": J::Object(summary).to_string(),
        "recorded_at": iso(now()), "withdrawn_at": "", "withdrawn_by": "",
    });
    // The new record replaces the old one as the basis; the old one stays as history.
    // Read so that a refusal stops the record here, before the new basis is stored beside an old
    // one that stays active.
    for k in try_consents_of(&*rt.backend, &c.id)?
        .iter()
        .filter(|k| k.is_active())
    {
        set(rt, CONSENT, &k.id, &[("state", "superseded".into())])?;
    }
    let cid = uuid::Uuid::now_v7().to_string();
    rt.backend
        .insert(CONSENT, &cid, f.as_object().expect("object"))?;
    Ok(row(&cid, f.as_object().expect("object")))
}

// ---- preview ------------------------------------------------------------------------------------

fn render(rt: &Runtime, input: &Value) -> Result<Value, String> {
    let m = map(input)?;
    let a = actor(m)?;
    let model = txt(m, "target_model");
    if model.is_empty() {
        return Err("choose the model to preview".into());
    }
    if blocked_model(model) || !rt.dispatcher.has_model(model) {
        return Err(format!("{model} cannot be previewed"));
    }
    // The connection to preview: an existing one (its real pseudonyms), or a stand-in built from
    // the provider and what the person typed so far.
    let (mut conn, provider) = if !txt(m, "connection").is_empty() {
        let (c, p) = rt
            .connection(txt(m, "connection"))
            .map_err(|_| "no such connection".to_string())?;
        if !a.admin && c.owner_user != a.id {
            return Err("no such connection".into());
        }
        (c, p)
    } else {
        let p = rt.provider(txt(m, "provider")).ok_or("choose a provider")?;
        let c = Connection::from_fields(
            &format!("preview:{}", a.id),
            &fields_of(&[
                ("owner_user", a.id.clone().into()),
                ("state", "draft".into()),
            ]),
        )?;
        (c, p)
    };
    if has(m, "field_allowlist") && !txt(m, "field_allowlist").trim().is_empty() {
        let norm = crate::guards::validate_allowlist(rt, txt(m, "field_allowlist"))?;
        conn.allowlist =
            Connection::from_fields("x", &fields_of(&[("field_allowlist", norm.into())]))?
                .allowlist;
    }
    if has(m, "policy") && !txt(m, "policy").trim().is_empty() {
        let mut pol = Policy::parse(txt(m, "policy"))?;
        for ack in &mut pol.acks {
            ack.by = a.id.clone();
            ack.at = iso(now());
        }
        conn.policy = pol;
    }
    if conn.allowlist.get(model).is_none_or(Vec::is_empty) {
        return Err("the allowlist for this model is empty, so nothing would be sent".into());
    }
    let reader = rt.reader_for(&conn.id, &a.groups, model);
    let mut v = obop_commands::Vault::new();
    let rows: Vec<Map<String, J>> = if txt(m, "record_id").is_empty() {
        let r = rt
            .dispatcher
            .call(
                obop_commands::Surface::Rest,
                &reader,
                &mut v,
                &format!("{model}.list"),
                &Value::map([("limit", Value::Int(1))]),
            )
            .map_err(|_| format!("you may not read {model}"))?;
        match r.value.to_json() {
            J::Array(a) => a
                .into_iter()
                .filter_map(|x| x.as_object().cloned())
                .collect(),
            _ => vec![],
        }
    } else {
        let r = rt
            .dispatcher
            .call(
                obop_commands::Surface::Rest,
                &reader,
                &mut v,
                &format!("{model}.get"),
                &Value::map([("id", Value::text(txt(m, "record_id")))]),
            )
            .map_err(|_| format!("you may not read that {model} record"))?;
        r.value.to_json().as_object().cloned().into_iter().collect()
    };
    let (plan, out) = rt
        .egress
        .preview(&conn, &provider, model, &rows)
        .map_err(|r| r.to_string())?;
    let table: Vec<J> = plan
        .fields
        .iter()
        .map(|f| {
            json!({"field": f.name, "class": f.class.as_str(), "personal": f.personal,
                   "treatment": f.decision.treatment.as_str(), "why": f.decision.why.as_str(),
                   "denied": f.decision.treatment == obop_connectors::Treatment::Deny,
                   "raised": f.decision.why != Why::Default && f.decision.treatment != obop_connectors::Treatment::Deny})
        })
        .collect();
    let payload = out.first().map_or(J::Null, |r| J::Object(r.clone()));
    Ok(Value::map([
        ("id", Value::text("preview")),
        (
            "payload",
            Value::Text(serde_json::to_string_pretty(&payload).unwrap_or_default()),
        ),
        (
            "treatments",
            Value::Text(
                json!({
                    "fields": table,
                    "unknown": plan.unknown,
                    "classes": plan.classes().iter().map(|c| c.as_str()).collect::<Vec<_>>(),
                    "sends_personal_data": plan.sends_personal_data(),
                    "sample": rows.first().and_then(|r| r.get("id")).cloned().unwrap_or(J::Null),
                })
                .to_string(),
            ),
        ),
    ]))
}

// ---- the install ----------------------------------------------------------------------------------

pub fn install(d: &Arc<Dispatcher>, m: &Module, store: &Store, rt: &Arc<Runtime>) {
    // Generic CRUD where people write records; read-only everywhere else.
    let mut writable = m.clone();
    writable
        .models
        .retain(|x| ["connection", "provider_policy", "consent"].contains(&x.name));
    d.register_crud(&writable, store);
    d.register_read_only_models(
        m,
        store,
        &[
            "sync_run",
            "data_flow_log",
            "connector_provider",
            "processing_register",
            "privacy_preview",
            "egress_policy",
        ],
    );

    // Computed read models.
    let r = rt.clone();
    virtual_model(
        d,
        m,
        "connector_provider",
        Arc::new(move |input| {
            let a = actor(input).ok();
            r.providers()
                .iter()
                .filter_map(|p| {
                    let f = provider_fields(&r, p);
                    let visible = a.as_ref().is_some_and(|a| {
                        a.admin
                            || (bool_of(&f, "enabled")
                                && bool_of(&f, "users_may_use")
                                && provider_policy(&*r.backend, p.id)
                                    .map(|pp| {
                                        if pp.allowed_group.is_empty() {
                                            USER.to_string()
                                        } else {
                                            pp.allowed_group
                                        }
                                    })
                                    .is_some_and(|g| a.groups.contains(&g)))
                    });
                    visible.then(|| (p.id.to_string(), f))
                })
                .collect()
        }),
    );
    let r = rt.clone();
    virtual_model(
        d,
        m,
        "processing_register",
        Arc::new(move |_| {
            r.register_rows()
                .into_iter()
                .map(|mut row| {
                    let id = str_of(&row, "id").to_string();
                    row.remove("id");
                    (id, row)
                })
                .collect()
        }),
    );
    let r = rt.clone();
    virtual_model(
        d,
        m,
        "egress_policy",
        // A policy that cannot be read is no row (the request is answered 503 by the layer, which
        // saw the refusal), never a row of the closed default.
        Arc::new(move |_| {
            instance_fields(&r)
                .map(|f| vec![(INSTANCE.to_string(), f)])
                .unwrap_or_default()
        }),
    );
    d.register(
        m,
        "privacy_preview",
        "list",
        Box::new(|_| Ok(Value::List(vec![]))),
    );
    d.register(
        m,
        "privacy_preview",
        "get",
        Box::new(|_| Err("a preview is not stored".into())),
    );
    d.register(
        m,
        "privacy_preview",
        "search",
        Box::new(|_| {
            Ok(Value::map([
                ("rows", Value::List(vec![])),
                ("total", Value::Int(0)),
            ]))
        }),
    );

    // Connections.
    for name in ["connect", "pause", "resume", "retry", "revoke"] {
        connection_command(rt, d, m, name);
    }
    let r = rt.clone();
    d.register(
        m,
        "connection",
        "test",
        Box::new(move |input| {
            let id = id_of(input)?;
            Ok(result(&id, &[], &r.test(&id)?))
        }),
    );
    let r = rt.clone();
    d.register(
        m,
        "connection",
        "sync",
        Box::new(move |input| {
            let id = id_of(input)?;
            Ok(result(&id, &[], &r.send_all(&id)?))
        }),
    );
    let r = rt.clone();
    d.register(
        m,
        "connection",
        "oauth_start",
        Box::new(move |input| {
            let id = id_of(input)?;
            let url = r.oauth_begin(&id, &caller(input))?;
            Ok(result(&id, &[], &json!({"url": url})))
        }),
    );

    // Consent.
    let r = rt.clone();
    d.register(
        m,
        "consent",
        "create",
        Box::new(move |input| consent_create(&r, input)),
    );
    let r = rt.clone();
    d.register(
        m,
        "consent",
        "withdraw",
        Box::new(move |input| {
            let id = id_of(input)?;
            let rec = r
                .backend
                .get(CONSENT, &id)?
                .ok_or_else(|| format!("no consent {id}"))?;
            if str_of(&rec, "state") != "active" {
                return Err("only the active record can be withdrawn".into());
            }
            let who = caller(input);
            // The connection is revoked first, and that withdraws every active consent of it,
            // this one included. The consent itself is the last step: a request that stops
            // half way leaves it active, so the command can be repeated (it is the only one that
            // can finish the work, and "only the active record can be withdrawn").
            let conn = str_of(&rec, "connection").to_string();
            if r.backend.get(CONNECTION, &conn)?.is_some() {
                revoke_connection(&r, &conn, &who)?;
            }
            // A consent of a connection that is gone, or one that the revocation did not reach.
            let still_active = r
                .backend
                .get(CONSENT, &id)?
                .is_some_and(|k| str_of(&k, "state") == "active");
            if still_active {
                obop_storage::fault::needed(|| withdraw_consent(&r, &id, &who))?;
            }
            Ok(Value::map([
                ("id", Value::text(&id)),
                ("state", Value::text("withdrawn")),
            ]))
        }),
    );

    // Administration.
    let r = rt.clone();
    d.register(
        m,
        "processing_register",
        "export_csv",
        Box::new(move |_| {
            Ok(Value::map([
                ("id", Value::text("register")),
                (
                    "csv",
                    Value::Text(obop_connectors::register::to_csv(&r.register_rows())),
                ),
            ]))
        }),
    );
    let r = rt.clone();
    d.register(
        m,
        "egress_policy",
        "checklist",
        Box::new(move |_| Ok(result(INSTANCE, &[], &r.checklist()))),
    );
    let r = rt.clone();
    d.register(
        m,
        "egress_policy",
        "purge",
        Box::new(move |_| {
            // A refused lookup stops the run (D74): the command fails, nothing is guessed.
            let (logs, runs, other) = r.purge(now())?;
            Ok(result(
                INSTANCE,
                &[],
                &json!({"flow_logs": logs, "sync_runs": runs, "expired_entries": other}),
            ))
        }),
    );
    let r = rt.clone();
    d.register(m, "egress_policy", "update", Box::new(move |input| {
        let mm = map(input)?;
        let mut f = Fields::new();
        for (k, v) in mm {
            match (k.as_str(), v) {
                ("id" | "_caller", _) => {}
                (k, _) if k.starts_with("_actor") => {}
                ("flows_enabled" | "eu_only" | "require_dpa", Value::Bool(b)) => {
                    f.insert(k.into(), (*b).into());
                }
                ("default_retention_days", Value::Int(i)) if (1..=3650).contains(i) => {
                    f.insert(k.into(), (*i).into());
                }
                ("host_allowlist", Value::Text(t)) => {
                    let hosts: Vec<&str> = t.split(',').map(str::trim).filter(|h| !h.is_empty()).collect();
                    if hosts.iter().any(|h| !h.chars().all(|c| c.is_ascii_alphanumeric() || "-._*".contains(c))) {
                        return Err("host_allowlist takes host names such as api.example.com or *.example.com".into());
                    }
                    f.insert(k.into(), hosts.join(",").into());
                }
                (other, _) => return Err(format!("egress_policy.{other} is not a setting, or has the wrong type")),
            }
        }
        if !r.backend.update(EGRESS_POLICY, INSTANCE, &f)? {
            // Never saved before: the defaults, with what was just chosen over them. A refused
            // read stops the save; it must not store the closed policy as the operator's.
            let mut all = instance_fields(&r)?;
            all.extend(f.clone());
            r.backend.insert(EGRESS_POLICY, INSTANCE, &all)?;
        }
        // The list the operator just saved is the one the client holds; if it cannot be read back
        // the client does not fall back to "no restriction", it asks again at every send.
        let source = r.backend.clone();
        r.client.follow_instance_hosts(Arc::new(move || {
            InstancePolicy::try_load(&*source).map(|p| p.hosts)
        }));
        // The answer is the saved policy. If it cannot be read back, it is what was just written
        // (true, and less), not the closed default.
        let saved = instance_fields(&r).unwrap_or(f);
        Ok(row(INSTANCE, &saved))
    }));

    let r = rt.clone();
    d.register(
        m,
        "privacy_preview",
        "render",
        Box::new(move |input| render(&r, input)),
    );
}
