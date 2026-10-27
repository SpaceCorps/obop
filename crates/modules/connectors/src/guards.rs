//! Guards: the rules that need to know who is calling or what the record is.
//!
//! They sit behind the guards the registry already generates (`Dispatcher::add_guard`), so
//! governed `state` fields, required fields and query checks still apply first.
//!
//! - Scope: a person sees and changes only their own connections, consents, runs and logs; an
//!   administrator (`connectors.admin`) sees all of them, never their credentials. A record that
//!   is not yours looks exactly like a record that does not exist.
//! - Connection input: validated against the provider, credentials sealed before they reach the
//!   store, impact-assessment acknowledgements stamped with who and when, a policy that exposes
//!   more than the defaults makes an agent's change a proposal.
//! - Consent records are never edited or deleted: record a new one, or withdraw.

use obop_commands::{Dispatcher, Guard, Principal, Value, Verdict};
use obop_connectors::Runtime;
use obop_connectors::catalog::Provider;
use obop_connectors::model::*;
use obop_connectors::policy::Policy;
use obop_connectors::secretbox::hint;
use obop_connectors::util::{iso, now, str_of};
use obop_storage::{Backend, Fields};
use obop_types::DataClass;
use serde_json::{Map, Value as J};
use std::sync::Arc;

use crate::h::*;
use crate::meta::ADMIN;

/// Models a connector must never read from or write to: its own records and the platform's.
pub fn blocked_model(name: &str) -> bool {
    name.starts_with("connector_")
        || matches!(
            name,
            "connection"
                | "consent"
                | "provider_policy"
                | "sync_run"
                | "data_flow_log"
                | "processing_register"
                | "egress_policy"
                | "privacy_preview"
                | "module"
                | "setting"
                | "marketplace_listing"
        )
}

const CONFIG_EXTRAS: [&str; 5] = ["allowed_hosts", "header_name", "events", "filename", "url"];

fn actor_only() -> Guard {
    Box::new(|p, input| {
        let mut m = map(input)?.clone();
        put_actor(p, &mut m);
        Ok(Verdict {
            input: Some(Value::Map(m)),
            ..Verdict::default()
        })
    })
}

/// `list` and `search`: a person asks only for their own records.
fn scope_list(owner_field: &'static str) -> Guard {
    Box::new(move |p, input| {
        let mut m = map(input)?.clone();
        put_actor(p, &mut m);
        if !p.allows(ADMIN) {
            let mut filter = match m.remove("filter") {
                Some(Value::List(l)) => l,
                _ => vec![],
            };
            filter.push(Value::map([
                ("field", Value::text(owner_field)),
                ("op", Value::text("eq")),
                ("value", Value::text(&owner_id(p))),
            ]));
            m.insert("filter".into(), Value::List(filter));
        }
        Ok(Verdict {
            input: Some(Value::Map(m)),
            ..Verdict::default()
        })
    })
}

/// A command on one record: the record must be the caller's (or the caller administers).
fn own_record(model: &'static str, backend: Arc<dyn Backend>) -> Guard {
    Box::new(move |p, input| {
        let mut m = map(input)?.clone();
        let id = txt(&m, "id").to_string();
        let rec = backend
            .get(model, &id)?
            .ok_or_else(|| format!("no {model} {id}"))?;
        if !p.allows(ADMIN) && str_of(&rec, "owner_user") != owner_id(p) {
            // The same words as for a record that is not there.
            return Err(format!("no {model} {id}"));
        }
        put_actor(p, &mut m);
        Ok(Verdict {
            input: Some(Value::Map(m)),
            ..Verdict::default()
        })
    })
}

fn refuse(msg: &'static str) -> Guard {
    Box::new(move |_, _| Err(msg.into()))
}

// ---- connection input ---------------------------------------------------------------------

fn json_object(text: &str, what: &str) -> Result<Map<String, J>, String> {
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str::<J>(text) {
        Ok(J::Object(o)) => Ok(o),
        _ => Err(format!("{what} must be a JSON object")),
    }
}

pub(crate) fn validate_allowlist(rt: &Runtime, text: &str) -> Result<String, String> {
    let raw = json_object(text, "field_allowlist")?;
    if raw.len() > 20 {
        return Err("field_allowlist names too many models".into());
    }
    let mut out = Map::new();
    for (model, fields) in raw {
        if blocked_model(&model) {
            return Err(format!(
                "{model} cannot be on an allowlist: connectors never read or write their own records or the platform's"
            ));
        }
        let meta = rt
            .dispatcher
            .model_fields(&model)
            .ok_or_else(|| format!("unknown model {model}"))?;
        let names: Vec<&str> = fields
            .as_array()
            .ok_or_else(|| format!("field_allowlist.{model} must be a list of field names"))?
            .iter()
            .filter_map(J::as_str)
            .collect();
        if names.len() > 100 {
            return Err(format!("field_allowlist.{model} names too many fields"));
        }
        let mut keep = Vec::new();
        for n in names {
            let f = meta
                .iter()
                .find(|f| f.name == n)
                .ok_or_else(|| format!("{model}.{n} does not exist"))?;
            if f.class == Some(DataClass::Secret) || f.class.is_none() {
                return Err(format!(
                    "{model}.{n} is a secret and can never be on an allowlist"
                ));
            }
            if !keep.contains(&n) {
                keep.push(n);
            }
        }
        out.insert(model, J::from(keep));
    }
    Ok(J::Object(out).to_string())
}

fn validate_config(rt: &Runtime, p: &Provider, text: &str) -> Result<String, String> {
    let cfg = json_object(text, "config")?;
    let allowed: Vec<&str> = p
        .config
        .iter()
        .filter(|f| !f.secret)
        .map(|f| f.key)
        .chain(CONFIG_EXTRAS)
        .chain([
            "target_model",
            "mapping",
            "records_path",
            "external_id_key",
            "interval_minutes",
            "event_types",
        ])
        .collect();
    for (k, v) in &cfg {
        if p.config.iter().any(|f| f.secret && f.key == k) {
            return Err(format!(
                "{k} is a secret: put it in credentials, not in config"
            ));
        }
        if !allowed.contains(&k.as_str()) {
            return Err(format!("config.{k} is not a setting of {}", p.name));
        }
        if let Some(s) = v.as_str()
            && s.len() > 4096
        {
            return Err(format!("config.{k} is too long"));
        }
    }
    if let Some(model) = cfg.get("target_model").and_then(J::as_str)
        && (blocked_model(model) || !rt.dispatcher.has_model(model))
    {
        return Err(format!(
            "target_model {model} is not a model connectors may write"
        ));
    }
    if let Some(m) = cfg.get("mapping") {
        let ok = match m {
            J::Object(o) => o.values().all(J::is_string),
            J::String(s) => json_object(s, "mapping").is_ok(),
            _ => false,
        };
        if !ok {
            return Err("config.mapping must be a JSON object of payload path to field".into());
        }
    }
    // A mapping given as an object is stored as text, like every config value of the form.
    let mut cfg = cfg;
    if let Some(J::Object(o)) = cfg.get("mapping").cloned() {
        cfg.insert("mapping".into(), J::from(J::Object(o).to_string()));
    }
    if let Some(ev) = cfg.get("events").and_then(J::as_str) {
        for e in ev.split(',').map(str::trim).filter(|e| !e.is_empty()) {
            let ok = e.split_once('.').is_some_and(|(a, b)| {
                !a.is_empty()
                    && !b.is_empty()
                    && e.chars()
                        .all(|c| c.is_ascii_lowercase() || c == '_' || c == '.')
            });
            if !ok {
                return Err(format!(
                    "config.events: {e:?} is not a command id such as lead.mark_won"
                ));
            }
        }
    }
    if let Some(url) = cfg.get("url").and_then(J::as_str).filter(|u| !u.is_empty()) {
        let mut hosts: Vec<String> = p.allowed_hosts.iter().map(|s| s.to_string()).collect();
        if hosts.is_empty() {
            hosts = cfg
                .get("allowed_hosts")
                .and_then(J::as_str)
                .map(|s| {
                    s.split(',')
                        .map(|x| x.trim().to_string())
                        .filter(|x| !x.is_empty())
                        .collect()
                })
                .unwrap_or_default();
        }
        if hosts.is_empty() {
            // No list given: the host of the URL is the list (checked again at every request).
            hosts = url_host(url).into_iter().collect();
        }
        rt.client
            .check_shape(url, &hosts)
            .map_err(|e| e.to_string())?;
    }
    if let Some(v) = cfg.get("interval_minutes")
        && !(v.as_u64().is_some() || v.as_str().is_some_and(|s| s.trim().parse::<u64>().is_ok()))
    {
        return Err("config.interval_minutes must be a whole number".into());
    }
    Ok(J::Object(cfg).to_string())
}

fn url_host(url: &str) -> Option<String> {
    let rest = url.split("://").nth(1)?;
    let host = rest.split(['/', '?', '#']).next()?;
    let host = host.rsplit('@').next()?;
    let host = host
        .rsplit_once(':')
        .filter(|(_, p)| p.chars().all(|c| c.is_ascii_digit()))
        .map_or(host, |(h, _)| h);
    Some(host.trim_matches(['[', ']']).to_ascii_lowercase())
}

/// Merge new credential values over the existing ones, seal the result and describe it safely.
fn credentials(
    rt: &Runtime,
    p: &Provider,
    existing: &str,
    new_text: &str,
) -> Result<(String, String), String> {
    let new = json_object(new_text, "credentials")?;
    let mut current = if existing.is_empty() {
        Map::new()
    } else {
        rt.secrets
            .open(obop_connectors::runtime::PURPOSE_CREDENTIALS, existing)
            .ok()
            .and_then(|t| serde_json::from_str::<J>(&t).ok())
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default()
    };
    for (k, v) in new {
        if !p.config.iter().any(|f| f.secret && f.key == k) {
            // The caller's key goes last, so that the text never begins with what the caller wrote.
            return Err(format!("{} has no credential called {k}", p.name));
        }
        let s = v
            .as_str()
            .ok_or_else(|| format!("credentials.{k} must be text"))?;
        if s.is_empty() {
            continue; // an empty box means "keep what is stored"
        }
        if s.len() > 2048 || s.contains(['\r', '\n']) {
            return Err(format!("credentials.{k} is not acceptable"));
        }
        if k == "signing_secret" && s.chars().count() < 16 {
            return Err("the signing secret must be at least 16 characters".into());
        }
        if k == "url" {
            // The address may be the secret (a Slack webhook): it gets the same checks as a
            // readable one, and is never repeated in the error.
            let hosts: Vec<String> = if p.allowed_hosts.is_empty() {
                url_host(s).into_iter().collect()
            } else {
                p.allowed_hosts.iter().map(|h| h.to_string()).collect()
            };
            rt.client
                .check_shape(s, &hosts)
                .map_err(|e| e.to_string())?;
        }
        current.insert(k, J::from(s));
    }
    if current.is_empty() {
        return Ok((String::new(), String::new()));
    }
    let hints = p
        .config
        .iter()
        .filter(|f| f.secret)
        .filter_map(|f| {
            current
                .get(f.key)
                .and_then(J::as_str)
                .map(|v| format!("{}: {}", f.key, hint(v)))
        })
        .collect::<Vec<_>>()
        .join("; ");
    Ok((rt.seal_credentials(&current), hints))
}

fn int_in(m: &mut M, k: &str, lo: i64, hi: i64) -> Result<(), String> {
    match m.get(k) {
        None | Some(Value::Null) => Ok(()),
        Some(Value::Int(i)) if (lo..=hi).contains(i) => Ok(()),
        _ => Err(format!("{k} must be a whole number from {lo} to {hi}")),
    }
}

/// Validate and normalise a create or update of a connection. `existing` is the stored record
/// for an update. Returns the input to run and whether the policy was raised.
fn normalise(
    rt: &Runtime,
    p: &Principal,
    input: &Value,
    existing: Option<&Fields>,
) -> Result<(Value, bool), String> {
    let mut m = map(input)?.clone();
    strip_actor(&mut m);
    let admin = p.allows(ADMIN);
    let creating = existing.is_none();
    if let Some(ex) = existing {
        if str_of(ex, "state") == "revoked" {
            return Err(
                "a revoked connection cannot be changed; connect again with a new one".into(),
            );
        }
        for k in [
            "provider",
            "owner_scope",
            "owner_user",
            "owner_groups",
            "created_at",
            "credentials_hint",
            "last_sync_at",
            "last_error",
            "result",
        ] {
            if m.contains_key(k) {
                return Err(format!("{k} cannot be changed"));
            }
        }
    } else {
        m.remove("result");
    }
    let provider_id = if creating {
        txt(&m, "provider").to_string()
    } else {
        str_of(existing.unwrap(), "provider").to_string()
    };
    let provider = rt
        .provider(&provider_id)
        .ok_or_else(|| format!("unknown provider {provider_id:?}"))?;
    let me = owner_id(p);

    if creating {
        if provider.engine == obop_connectors::Engine::None {
            return Err(format!(
                "{} is a definition only; nothing runs yet, so it cannot be connected",
                provider.name
            ));
        }
        let pol = provider_policy(&*rt.backend, provider.id);
        let scope = if admin {
            if txt(&m, "owner_scope").is_empty() {
                "instance"
            } else {
                txt(&m, "owner_scope")
            }
        } else {
            "user"
        }
        .to_string();
        if !admin {
            let ok = pol.as_ref().is_some_and(|x| {
                x.enabled
                    && x.users_may_use
                    && p.allows(if x.allowed_group.is_empty() {
                        crate::meta::USER
                    } else {
                        &x.allowed_group
                    })
            });
            if !ok {
                return Err(format!(
                    "{} is not enabled for you; ask an administrator",
                    provider.name
                ));
            }
        }
        m.insert("owner_scope".into(), Value::text(&scope));
        m.insert("owner_user".into(), Value::text(&me));
        let mut groups: Vec<String> = p.groups.iter().filter(|g| p.allows(g)).cloned().collect();
        groups.sort();
        m.insert("owner_groups".into(), Value::text(&groups.join(",")));
        m.insert("created_at".into(), Value::text(&iso(now())));
        m.entry("rate_limit_per_hour".into())
            .or_insert(Value::Int(DEFAULT_RATE_PER_HOUR));
        m.entry("volume_cap_per_day".into())
            .or_insert(Value::Int(DEFAULT_VOLUME_PER_DAY));
        m.entry("retention_days".into()).or_insert(Value::Int(
            InstancePolicy::load(&*rt.backend).default_retention_days,
        ));
        if txt(&m, "policy").trim().is_empty()
            && let Some(d) = pol
                .as_ref()
                .map(|x| x.default_policy.clone())
                .filter(|d| !d.trim().is_empty())
        {
            m.insert("policy".into(), Value::Text(d));
        }
    }
    int_in(&mut m, "rate_limit_per_hour", 1, 100_000)?;
    int_in(&mut m, "volume_cap_per_day", 1, 10_000_000)?;
    int_in(&mut m, "retention_days", 1, 3650)?;

    if has(&m, "config") {
        let cfg = validate_config(rt, &provider, txt(&m, "config"))?;
        m.insert("config".into(), Value::Text(cfg));
    }
    if has(&m, "field_allowlist") {
        let a = validate_allowlist(rt, txt(&m, "field_allowlist"))?;
        m.insert("field_allowlist".into(), Value::Text(a));
    }
    let mut raised = false;
    if has(&m, "policy") {
        let mut pol = Policy::parse(txt(&m, "policy"))?;
        let old = existing
            .map(|e| Policy::parse(str_of(e, "policy")).unwrap_or_default())
            .unwrap_or_default();
        for a in &mut pol.acks {
            let same = old.acks.iter().any(|o| {
                o.class == a.class
                    && o.text == a.text
                    && o.by == a.by
                    && o.at == a.at
                    && !o.by.is_empty()
            });
            if !same {
                // Whatever the client sent as who and when is replaced by what happened.
                a.by = me.clone();
                a.at = iso(now());
            }
        }
        for k in pol.fields.keys() {
            let (model, field) = k.split_once('.').unwrap_or((k, ""));
            let known = rt
                .dispatcher
                .model_fields(model)
                .is_some_and(|fs| fs.iter().any(|f| f.name == field));
            if !known {
                return Err(format!("policy names {k}, which does not exist"));
            }
        }
        let class_of = |key: &str| {
            let (model, field) = key.split_once('.')?;
            let f = rt
                .dispatcher
                .model_fields(model)?
                .into_iter()
                .find(|f| f.name == field)?;
            Some((f.class.unwrap_or(DataClass::Secret), f.personal_data))
        };
        let new_raises = pol.raises(class_of);
        let old_raises = old.raises(class_of);
        raised = new_raises.iter().any(|r| !old_raises.contains(r));
        m.insert("policy".into(), Value::Text(pol.to_json().to_string()));
    }
    if has(&m, "credentials") {
        let existing_sealed = existing.map_or("", |e| str_of(e, "credentials"));
        let (sealed, hints) = credentials(rt, &provider, existing_sealed, txt(&m, "credentials"))?;
        if sealed.is_empty() {
            m.remove("credentials");
        } else {
            m.insert("credentials".into(), Value::Text(sealed));
            m.insert("credentials_hint".into(), Value::Text(hints));
        }
    }
    if creating && !has(&m, "credentials_hint") {
        m.insert("credentials_hint".into(), Value::text(""));
    }
    Ok((Value::Map(m), raised))
}

/// Enough of the `connection` input to refuse a change that would widen what leaves beyond the
/// recorded consent, without refusing the save: the flow itself refuses at send time.
fn connection_input(rt: Arc<Runtime>, create: bool) -> Guard {
    Box::new(move |p, input| {
        let existing = if create {
            None
        } else {
            let id = txt(map(input)?, "id").to_string();
            Some(
                rt.backend
                    .get(CONNECTION, &id)?
                    .ok_or_else(|| format!("no connection {id}"))?,
            )
        };
        let (v, raised) = normalise(&rt, p, input, existing.as_ref())?;
        Ok(Verdict {
            input: Some(v),
            approval: raised,
            ..Verdict::default()
        })
    })
}

fn provider_policy_input(rt: Arc<Runtime>, create: bool) -> Guard {
    Box::new(move |_, input| {
        let mut m = map(input)?.clone();
        strip_actor(&mut m);
        if create || has(&m, "provider") {
            let id = txt(&m, "provider");
            if rt.provider(id).is_none() {
                return Err(format!("unknown provider {id:?}"));
            }
            let taken = rt
                .backend
                .list(PROVIDER_POLICY)?
                .iter()
                .any(|(rid, f)| str_of(f, "provider") == id && rid != txt(&m, "id"));
            if taken {
                return Err(format!("{id} already has a policy; update it"));
            }
        }
        if has(&m, "allowed_group") {
            let g = txt(&m, "allowed_group");
            if !g.is_empty()
                && !g
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '.' || c == '_')
            {
                return Err(
                    "allowed_group must be a permission group such as connectors.user".into(),
                );
            }
        }
        if has(&m, "default_policy") {
            let pol = Policy::parse(txt(&m, "default_policy"))?;
            if !pol.acks.is_empty() {
                return Err("a default policy cannot carry acknowledgements: they are recorded per connection".into());
            }
        }
        if let Some(Value::Text(secret)) = m.get("oauth_client_secret").cloned() {
            if secret.is_empty() {
                m.remove("oauth_client_secret");
            } else {
                m.insert(
                    "oauth_client_secret".into(),
                    Value::Text(rt.seal_client_secret(&secret)),
                );
            }
        }
        Ok(Verdict {
            input: Some(Value::Map(m)),
            ..Verdict::default()
        })
    })
}

pub fn install(d: &Arc<Dispatcher>, rt: &Arc<Runtime>) {
    let b = rt.backend.clone();
    let add = |id: &str, g: Guard| d.add_guard("connectors", id, false, g);

    // Catalogue and previews need to know who asks.
    for c in ["get", "list", "search"] {
        add(&format!("connector_provider.{c}"), actor_only());
    }
    add("privacy_preview.render", actor_only());
    add("privacy_preview.list", actor_only());
    add("consent.create", actor_only());
    add("processing_register.export_csv", actor_only());

    // Scope: your own records only.
    for model in ["connection", "consent", "sync_run", "data_flow_log"] {
        for c in ["list", "search"] {
            add(&format!("{model}.{c}"), scope_list("owner_user"));
        }
        add(
            &format!("{model}.get"),
            own_record(model_static(model), b.clone()),
        );
    }
    for c in [
        "delete",
        "connect",
        "pause",
        "resume",
        "retry",
        "revoke",
        "test",
        "sync",
        "oauth_start",
    ] {
        add(
            &format!("connection.{c}"),
            own_record("connection", b.clone()),
        );
    }
    add("consent.withdraw", own_record("consent", b.clone()));

    // Connection input.
    d.add_guard(
        "connectors",
        "connection.create",
        true,
        connection_input(rt.clone(), true),
    );
    d.add_guard("connectors", "connection.update", true, {
        // Ownership first, then the validation of what changes.
        let own = own_record("connection", b.clone());
        let input = connection_input(rt.clone(), false);
        Box::new(move |p, i| {
            let v = own(p, i)?;
            let i2 = v.input.unwrap_or_else(|| i.clone());
            let mut out = input(p, &i2)?;
            out.requires.extend(v.requires);
            Ok(out)
        })
    });

    // Consent records are append-only: record a new one or withdraw.
    add(
        "consent.update",
        refuse("consent records are not edited: record a new one, or withdraw this one"),
    );
    add(
        "consent.delete",
        refuse("consent records are kept as evidence and cannot be deleted"),
    );

    // Administrator records.
    d.add_guard(
        "connectors",
        "provider_policy.create",
        true,
        provider_policy_input(rt.clone(), true),
    );
    d.add_guard(
        "connectors",
        "provider_policy.update",
        true,
        provider_policy_input(rt.clone(), false),
    );
}

fn model_static(m: &str) -> &'static str {
    match m {
        "connection" => "connection",
        "consent" => "consent",
        "sync_run" => "sync_run",
        _ => "data_flow_log",
    }
}
