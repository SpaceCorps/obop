//! The `device` model of the Customize module (the D64 amendment, D87): the phones and tablets a
//! mobile app is signed in on, as a person and an administrator see them, and the commands that
//! change them.
//!
//! The records are written by the OAuth server (a device is created in the write that creates the
//! refresh-token family it is bound to, `last_seen` moves when the family is refreshed) and by the
//! identity provider (an offboarding or a changed credential ends devices). The commands here are
//! only what people do:
//!
//! - `get`, `list` and `search`, generated: a person sees their own devices, the holders of
//!   `api.device_admin` and the instance administrator see every device (the row access of the
//!   model);
//! - `update`, which changes the name and nothing else;
//! - `revoke`, a governed action (D48): the refresh-token family ends, the access token stops, the
//!   record says `revoked`. An agent can only propose it.
//!
//! The commands are in group `*` (a person needs no group for their own devices), and the guards
//! (`obop_commands::devices::may_see`, `may_list`) do the rest by what kind of token calls: a
//! person at the keyboard and the administrators as above; the token of a mobile app only its own
//! device (`get`, `update`, `revoke`; it cannot list or search); the token of an agent only a
//! proposal to revoke its owner's device. A narrowed token never learns what other devices the
//! person has.
//!
//! There is no `create`, `bulk_create` or `delete`: a device exists because an app signed in, and a
//! revoked one is forgotten with its family a week after it ended.
//!
//! The push token is declared and never written in this version (push is not part of the first
//! mobile slice, docs/spec/mobile.md 3); it is a `secret` field, which no surface ever returns.

use obop_commands::devices as dev;
use obop_commands::{ANY_GROUP, Dispatcher, Guard, Principal, Store, Value, Verdict};
use obop_registry::{Access, CommandDef, FieldDef, FieldKind, ModelDef};
use obop_storage::Fields;
use obop_types::DataClass;
use serde_json::Value as J;
use std::sync::Arc;

/// Registry metadata of the model.
pub fn device_model() -> ModelDef {
    let any = |name, description, class, personal| {
        FieldDef::new(name, description, class, personal, ANY_GROUP)
    };
    ModelDef {
        name: dev::MODEL,
        description: "A phone or tablet that a mobile app is signed in on: created when the app signs in, ended by revoking it. A person sees, renames and revokes their own devices; the device administrators see and revoke every one; the app itself sees and signs out only its own device",
        read_group: ANY_GROUP,
        write_group: ANY_GROUP,
        access: Some(Access {
            own: Some("user"),
            see_all: vec![dev::ADMIN_GROUP],
            ..Access::default()
        }),
        fields: vec![
            any("user", "The account the device belongs to", DataClass::Internal, true),
            any("name", "What the device is called; the person can rename it", DataClass::Internal, true),
            any("client_name", "The mobile app that signed in, as it called itself", DataClass::Internal, false),
            any("platform", "The platform the app reported, such as android or ios", DataClass::Internal, false),
            any("app_version", "The version of the app, as it reported it at sign-in and at each refresh", DataClass::Internal, false),
            FieldDef::new(
                "push_token",
                "The address of the push service for this device. Never shown to anyone and not written in this version: push is not part of the first mobile slice",
                DataClass::Secret,
                false,
                dev::ADMIN_GROUP,
            ),
            any("registered_at", "When the app signed in on this device", DataClass::None, false).datetime(),
            any("last_seen", "When the device last refreshed its sign-in (the time of the last refresh, not of every request)", DataClass::None, false).datetime(),
            any("expires_at", "When the sign-in of the device ends unless it is renewed", DataClass::None, false).datetime(),
            any("state", "active while the device may sign in; revoked once it was ended", DataClass::None, false)
                .select(&[dev::ACTIVE, dev::REVOKED]),
            any("revoked_at", "When the device was ended", DataClass::None, false).datetime(),
            FieldDef::new(
                "grant",
                "The refresh-token family the device is bound to (an identifier, not a token)",
                DataClass::Internal,
                false,
                dev::ADMIN_GROUP,
            )
            .kind(FieldKind::Text),
        ],
        commands: vec![
            CommandDef::new(
                "revoke",
                "End a device for good: its refresh tokens stop working, its access token stops at the next request and the app has to sign in again. A person revokes their own devices; an agent can only propose it. Give id",
                ANY_GROUP,
            )
            .transition("state", Some(dev::ACTIVE), dev::REVOKED)
            .approval(),
        ],
        ext: Default::default(),
    }
}

fn s(v: &str) -> Value {
    Value::text(v)
}

fn id_of(m: &std::collections::BTreeMap<String, Value>) -> Result<String, String> {
    match m.get("id") {
        Some(Value::Text(t)) if !t.trim().is_empty() => Ok(t.clone()),
        _ => Err("missing id".into()),
    }
}

/// A row as `get` shows it (the dispatcher shapes it for the caller: secret fields never leave).
fn row(id: &str, rec: &Fields) -> Value {
    let mut m: std::collections::BTreeMap<String, Value> = rec
        .iter()
        .map(|(k, v)| (k.clone(), Value::from_json(v)))
        .collect();
    m.insert("id".into(), s(id));
    Value::Map(m)
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The record `id` if the caller may do `what` to it. A device the caller may not see is reported
/// exactly like one that does not exist (and so is a row of somebody else's table of the same
/// name). A lookup the store refuses is the store's error, never "no device".
fn visible(store: &Store, p: &Principal, id: &str, what: dev::Use) -> Result<Fields, String> {
    match store.backend().get(dev::MODEL, id)? {
        Some(rec) if dev::is_ours(&rec) && dev::may_see(p, id, &rec, what) => Ok(rec),
        _ => Err(format!("no device {id}")),
    }
}

/// `get`: the same rule as the other commands (the row access of the model lets a person and the
/// administrators through; a mobile app's token and an agent's token get no more than they may).
fn get_guard(store: Store) -> Guard {
    Box::new(move |p, input| {
        let Value::Map(m) = input else {
            return Err("expected an object".into());
        };
        let id = id_of(m)?;
        visible(&store, p, &id, dev::Use::Read)?;
        Ok(Verdict::default())
    })
}

/// `list` and `search`: a person sees their own devices, the administrators everybody's. A token of
/// a mobile app or of an agent learns nothing of the person's devices by listing: it asks for its
/// own by id (`get`, `revoke`), which it was told when it signed in.
fn list_guard() -> Guard {
    Box::new(move |p, _| {
        if dev::may_list(p) {
            Ok(Verdict::default())
        } else {
            Err("devices cannot be listed with this token: an app or an assistant sees only its own device, by id; the person manages their devices in Customize, Security".into())
        }
    })
}

fn update_guard(store: Store) -> Guard {
    let fields: Vec<&'static str> = device_model().fields.iter().map(|f| f.name).collect();
    Box::new(move |p, input| {
        let Value::Map(m) = input else {
            return Err("expected an object".into());
        };
        let id = id_of(m)?;
        visible(&store, p, &id, dev::Use::Rename)?;
        for k in m.keys().filter(|k| *k != "id" && !k.starts_with('_')) {
            if k == "name" {
                continue;
            }
            return Err(if fields.contains(&k.as_str()) {
                format!("device.{k} cannot be changed: only the name of a device can")
            } else {
                format!("unknown field device.{k}")
            });
        }
        match m.get("name") {
            Some(Value::Text(t)) if dev::name_of(t).is_some() => {}
            _ => {
                return Err(
                    "device.name must have 1 to 80 characters that can be seen (characters that hide or reorder text are removed first)"
                        .into(),
                );
            }
        }
        Ok(Verdict::default())
    })
}

fn revoke_guard(store: Store) -> Guard {
    Box::new(move |p, input| {
        let Value::Map(m) = input else {
            return Err("expected an object".into());
        };
        let id = id_of(m)?;
        let rec = visible(&store, p, &id, dev::Use::Revoke)?;
        if !dev::is_active(&rec) {
            return Err(format!("device {id} is already revoked"));
        }
        Ok(Verdict::default())
    })
}

fn text_of<'a>(x: &'a std::collections::BTreeMap<String, Value>, k: &str) -> Option<&'a str> {
    match x.get(k) {
        Some(Value::Text(t)) if !t.is_empty() => Some(t.as_str()),
        _ => None,
    }
}

/// Bind the commands of the `device` model. `ops` ends a device (the OAuth server). Call once at
/// start-up, with the identity provider that serves the instance, after the dispatcher has its
/// modules; nothing is bound without an authorization server (`--dev-auth`), and the model then has
/// no commands at all.
pub fn register_devices(d: &Arc<Dispatcher>, store: &Store, ops: Arc<dyn dev::DevicesApi>) {
    let m = crate::installer::admin_module();
    d.register_read_only_models(&m, store, &[dev::MODEL]);
    d.add_guard(&m.id, "device.get", false, get_guard(store.clone()));
    d.add_guard(&m.id, "device.list", false, list_guard());
    d.add_guard(&m.id, "device.search", false, list_guard());

    d.register_guard(&m.id, "device.update", true, update_guard(store.clone()));
    let st = store.clone();
    d.register(
        &m,
        dev::MODEL,
        "update",
        Box::new(move |input| {
            let Value::Map(x) = input else {
                return Err("expected an object".into());
            };
            let id = id_of(x)?;
            let Some(Value::Text(raw)) = x.get("name") else {
                return Err("missing name".into());
            };
            let name = dev::name_of(raw).ok_or("device.name is not a valid name")?;
            let backend = st.backend();
            let before = backend
                .get(dev::MODEL, &id)?
                .filter(dev::is_ours)
                .ok_or_else(|| format!("no device {id}"))?;
            if before.get("name").and_then(J::as_str) != Some(name.as_str()) {
                let mut patch = Fields::new();
                patch.insert("name".into(), J::from(name.as_str()));
                if !backend.update(dev::MODEL, &id, &patch)? {
                    return Err(format!("no device {id}"));
                }
                let text = |k: &str| before.get(k).and_then(J::as_str).unwrap_or("").to_string();
                obop_commands::audit::record_event(
                    &*backend,
                    now(),
                    &text("user"),
                    dev::EVENT_RENAMED,
                    text_of(x, "_caller").unwrap_or(""),
                    &format!("{:?} is now {:?} [{id}]", text("name"), name),
                );
            }
            let after = backend
                .get(dev::MODEL, &id)?
                .ok_or_else(|| format!("no device {id}"))?;
            Ok(row(&id, &after))
        }),
    );

    d.register_guard(&m.id, "device.revoke", true, revoke_guard(store.clone()));
    let st = store.clone();
    d.register(
        &m,
        dev::MODEL,
        "revoke",
        Box::new(move |input| {
            let Value::Map(x) = input else {
                return Err("expected an object".into());
            };
            let id = id_of(x)?;
            let caller = text_of(x, "_caller").unwrap_or("");
            // An approved proposal runs as the agent that made it; the person who approved it is
            // who did it, and the log says both.
            let approver = text_of(x, "_actor_approver");
            let (actor, proposed_by) = match approver {
                Some(a) => (a, Some(caller)),
                None => (caller, None),
            };
            // The family, the access token and the record, or an error that says what was left.
            ops.revoke_device(&id, actor, proposed_by)?;
            // The answer is read back; a read the store refuses now does not undo the revocation.
            match st.backend().get(dev::MODEL, &id) {
                Ok(Some(rec)) => Ok(row(&id, &rec)),
                _ => Ok(Value::map([("id", s(&id)), ("state", s(dev::REVOKED))])),
            }
        }),
    );
}
