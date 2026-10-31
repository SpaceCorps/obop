//! The `user_account` model of the Customize module (D69): the accounts of the instance as an
//! administrator sees them, and the commands that change them. It has no records of its own; the
//! handlers call the identity provider (`Authenticator::accounts`), which holds the rules. Every
//! command that changes an account needs an administrator and, for an agent, human approval, so
//! an agent can only propose "make ada a member of sales.manager" and a person decides.
//!
//! An approved proposal runs as the administrator who owns the agent, looked up again at that
//! moment: an administrator who was disabled or lost the group since the proposal does nothing.

use obop_commands::{
    ADMIN_GROUP, AccountsApi, Authenticator, Dispatcher, MfaError, NewUser, Principal, Value,
};
use obop_registry::{CommandDef, FieldDef, FieldKind, ModelDef, Module};
use obop_types::DataClass;
use std::collections::BTreeMap;
use std::sync::{Arc, Weak};

/// Registry metadata of the model.
pub fn user_account_model() -> ModelDef {
    let f =
        |name, description| FieldDef::new(name, description, DataClass::None, false, ADMIN_GROUP);
    ModelDef {
        name: "user_account",
        description: "A person who can sign in to this instance, with the permission groups they hold. Changed only by administrators; an agent can only propose a change",
        read_group: ADMIN_GROUP,
        write_group: ADMIN_GROUP,
        access: None,
        fields: vec![
            f("groups", "The permission groups the account holds, separated by commas. Input of add and set_groups: the complete list the account should hold afterwards"),
            f("previous_groups", "Result of set_groups: the groups the account held before"),
            FieldDef::new(
                "password",
                "Input of add: the initial password (at least 12 characters); the person must choose their own at the first sign-in. Never shown",
                DataClass::Secret,
                false,
                ADMIN_GROUP,
            ),
            f("must_change_password", "Whether the person must choose a new password at the next sign-in").kind(FieldKind::Bool),
            f("note", "Input of offboard: a short reason kept in the security log (no secrets; 200 characters)"),
            f("revoked", "Result of offboard: what was ended for good, in words"),
            f("admin", "Whether the account holds the administrator group").kind(FieldKind::Bool),
            f("disabled", "Whether the account is disabled: it cannot sign in and its tokens do not work").kind(FieldKind::Bool),
            f("offboarded", "Whether the account was offboarded: disabled, with every session, agent token and connected app revoked for good").kind(FieldKind::Bool),
            f("mfa_enabled", "Whether the account has a second sign-in factor (an authenticator app or a passkey)").kind(FieldKind::Bool),
            f("sessions", "Signed-in sessions that are still valid").kind(FieldKind::Number),
            f("oid", "Input of bind_sso: the person's object id in the Microsoft directory, a GUID (not an e-mail address or a user principal name)"),
            f("sso_oid", "The object id the account is bound to for signing in with Microsoft; empty when it is not bound"),
            f("sso_only", "Whether the account signs in with Microsoft only: it has no password or passkey sign-in of its own. Input of set_sso_only (true or false)").kind(FieldKind::Bool),
        ],
        commands: vec![
            CommandDef::new(
                "add",
                "Create an account. Give id (the user name), password (at least 12 characters) and groups. The person must choose a new password at the first sign-in",
                ADMIN_GROUP,
            )
            .approval(),
            CommandDef::new(
                "set_groups",
                "Replace the permission groups of an account. Give id and groups (the complete list afterwards). Takes effect on the account's next request. Refused for unknown groups, for taking the administrator group from oneself or from the last administrator",
                ADMIN_GROUP,
            )
            .approval(),
            CommandDef::new(
                "offboard",
                "Disable an account and end for good every session, agent token and connected app it had. Enabling it later does not bring them back. Give id and optionally note",
                ADMIN_GROUP,
            )
            .approval(),
            CommandDef::new(
                "disable",
                "Disable an account: it cannot sign in and its tokens stop working until it is enabled again. Give id",
                ADMIN_GROUP,
            )
            .approval(),
            CommandDef::new(
                "enable",
                "Enable a disabled account again. Credentials revoked by an offboarding stay revoked. Give id",
                ADMIN_GROUP,
            )
            .approval(),
            CommandDef::new(
                "bind_sso",
                "Bind an existing account to a person's Microsoft identity so that they can sign in with Microsoft. Give id and oid (the person's object id in the directory, a GUID). Creates no account and gives no permission",
                ADMIN_GROUP,
            )
            .approval(),
            CommandDef::new(
                "unbind_sso",
                "Remove the binding of an account to a Microsoft identity. Refused while the account signs in with Microsoft only. Give id",
                ADMIN_GROUP,
            )
            .approval(),
            CommandDef::new(
                "set_sso_only",
                "Make an account sign in with Microsoft only (it then refuses its password and its passkey on their own), or turn that off again. Give id and sso_only (true or false). Needs the account bound, and at least two other administrators who still sign in locally with a second factor. Ends the account's sessions",
                ADMIN_GROUP,
            )
            .approval(),
        ],
        ext: Default::default(),
    }
}

pub(crate) fn s(v: &str) -> Value {
    Value::text(v)
}

fn msg(e: MfaError) -> String {
    match e {
        MfaError::Invalid(m)
        | MfaError::Denied(m)
        | MfaError::Locked(m)
        | MfaError::Unavailable(m) => m,
        MfaError::BadPassword => "the password is not right".into(),
        MfaError::Expired => "expired".into(),
        MfaError::NotFound => "not allowed".into(),
    }
}

pub(crate) fn input(v: &Value) -> Result<&BTreeMap<String, Value>, String> {
    match v {
        Value::Map(m) => Ok(m),
        _ => Err("expected an object".into()),
    }
}

pub(crate) fn text<'a>(x: &'a BTreeMap<String, Value>, k: &str) -> Option<&'a str> {
    match x.get(k) {
        Some(Value::Text(t)) if !t.trim().is_empty() => Some(t),
        _ => None,
    }
}

fn groups_of(x: &BTreeMap<String, Value>) -> Result<Vec<String>, String> {
    match x.get("groups") {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Text(t)) => Ok(t.split(',').map(|g| g.trim().to_string()).collect()),
        Some(Value::List(l)) => l
            .iter()
            .map(|g| match g {
                Value::Text(t) => Ok(t.clone()),
                _ => Err("groups must be a list of names".to_string()),
            })
            .collect(),
        Some(_) => Err("groups must be a list of names".into()),
    }
}

fn join(v: &serde_json::Value) -> Value {
    let names: Vec<&str> = v
        .as_array()
        .map(|a| a.iter().filter_map(|x| x.as_str()).collect())
        .unwrap_or_default();
    Value::Text(names.join(", "))
}

/// A row as `list` and `get` show it.
fn row(u: &serde_json::Value) -> Value {
    let b = |k: &str| Value::Bool(u[k].as_bool() == Some(true));
    Value::map([
        ("id", s(u["name"].as_str().unwrap_or(""))),
        ("groups", join(&u["groups"])),
        ("admin", b("admin")),
        ("disabled", b("disabled")),
        ("offboarded", b("offboarded")),
        ("mfa_enabled", b("mfa_enabled")),
        ("must_change_password", b("must_change_password")),
        ("sessions", Value::Int(u["sessions"].as_i64().unwrap_or(0))),
        ("sso_only", b("sso_only")),
        ("sso_oid", s(u["sso_oid"].as_str().unwrap_or(""))),
    ])
}

/// The administrator a call runs as: the person, or for an agent the person it acts for, read
/// again from the stored account.
pub(crate) fn acting(
    auth: &dyn Authenticator,
    x: &BTreeMap<String, Value>,
) -> Result<Principal, String> {
    let caller = text(x, "_caller").unwrap_or("");
    let user = caller.strip_suffix("/agent").unwrap_or(caller);
    let mut admin = auth
        .accounts()
        .and_then(|a| a.administrator(user))
        .ok_or_else(|| {
            "the account administration tools need an enabled administrator; the account that made this request is not one".to_string()
        })?;
    // The security log names the agent that proposed the change (`alice/agent`).
    if caller != user {
        admin.id = caller.to_string();
    }
    Ok(admin)
}

fn accounts(auth: &dyn Authenticator) -> Result<&dyn AccountsApi, String> {
    auth.accounts()
        .ok_or_else(|| "this instance has no account administration".to_string())
}

fn user_row(auth: &dyn Authenticator, admin: &Principal, name: &str) -> Result<Value, String> {
    let name = name.trim().to_lowercase();
    let list = auth
        .mfa()
        .ok_or("this instance has no account administration")?
        .users(admin)
        .map_err(msg)?;
    list["users"]
        .as_array()
        .and_then(|a| a.iter().find(|u| u["name"].as_str() == Some(&name)))
        .map(row)
        .ok_or_else(|| format!("no user {name}"))
}

/// A command of the model: it sees the identity provider, the dispatcher (for the registered
/// groups) and the input.
type AccountHandler =
    Box<dyn Fn(&dyn Authenticator, &Dispatcher, &Value) -> Result<Value, String> + Send + Sync>;

/// Bind the handlers. `auth` is the identity provider the instance serves with; `d` supplies the
/// registered permission groups. Call once at start-up, after the dispatcher has its modules.
pub fn register_accounts(d: &Arc<Dispatcher>, auth: Arc<dyn Authenticator>) {
    let m: Module = crate::installer::admin_module();
    let reg = |cmd: &str, h: AccountHandler| {
        let (a, w): (Arc<dyn Authenticator>, Weak<Dispatcher>) = (auth.clone(), Arc::downgrade(d));
        d.register(
            &m,
            "user_account",
            cmd,
            Box::new(move |v| {
                let d = w.upgrade().ok_or("shutting down")?;
                h(&*a, &d, v)
            }),
        );
    };
    reg(
        "list",
        Box::new(|auth, _, v| {
            let admin = acting(auth, input(v)?)?;
            let list = auth
                .mfa()
                .ok_or("this instance has no account administration")?
                .users(&admin)
                .map_err(msg)?;
            Ok(Value::List(
                list["users"]
                    .as_array()
                    .map(|a| a.iter().map(row).collect())
                    .unwrap_or_default(),
            ))
        }),
    );
    reg(
        "get",
        Box::new(|auth, _, v| {
            let x = input(v)?;
            let admin = acting(auth, x)?;
            user_row(auth, &admin, text(x, "id").ok_or("missing id")?)
        }),
    );
    reg(
        "add",
        Box::new(|auth, d, v| {
            let x = input(v)?;
            let admin = acting(auth, x)?;
            let groups = groups_of(x)?;
            let name = text(x, "id").ok_or("missing id (the user name)")?;
            // A proposal needs a password a person chose: a generated one could not be shown to
            // anyone, because the answer to an agent is shaped for the agent.
            let password =
                text(x, "password").ok_or("missing password (at least 12 characters)")?;
            let new = NewUser {
                name,
                password: Some(password),
                groups: &groups,
                must_change_password: true,
            };
            let done = accounts(auth)?
                .create_user(&admin, &new, &d.known_groups())
                .map_err(msg)?;
            user_row(auth, &admin, done["user"].as_str().unwrap_or(name))
        }),
    );
    reg(
        "set_groups",
        Box::new(|auth, d, v| {
            let x = input(v)?;
            let admin = acting(auth, x)?;
            let id = text(x, "id").ok_or("missing id")?;
            let groups = groups_of(x)?;
            let done = accounts(auth)?
                .set_user_groups(&admin, id, &groups, &d.known_groups())
                .map_err(msg)?;
            let Value::Map(mut r) = user_row(auth, &admin, id)? else {
                unreachable!()
            };
            r.insert("previous_groups".into(), join(&done["before"]));
            Ok(Value::Map(r))
        }),
    );
    reg(
        "offboard",
        Box::new(|auth, _, v| {
            let x = input(v)?;
            let admin = acting(auth, x)?;
            let id = text(x, "id").ok_or("missing id")?;
            let done = accounts(auth)?
                .offboard_user(&admin, id, text(x, "note").unwrap_or(""))
                .map_err(msg)?;
            let Value::Map(mut r) = user_row(auth, &admin, id)? else {
                unreachable!()
            };
            let n = |k: &str| done[k].as_u64().unwrap_or(0);
            r.insert(
                "revoked".into(),
                Value::Text(format!(
                    "{} session(s), {} agent token(s), {} connected app(s), {} device(s) of mobile apps",
                    n("revoked_sessions"),
                    n("revoked_agent_tokens"),
                    n("revoked_apps"),
                    n("revoked_devices")
                )),
            );
            Ok(Value::Map(r))
        }),
    );
    reg(
        "bind_sso",
        Box::new(|auth, _, v| {
            let x = input(v)?;
            let admin = acting(auth, x)?;
            let id = text(x, "id").ok_or("missing id")?;
            let oid = text(x, "oid")
                .ok_or("missing oid (the person's object id in the Microsoft directory)")?;
            crate::sso::api(auth)?
                .bind(&admin, id, oid)
                .map_err(crate::sso::msg)?;
            user_row(auth, &admin, id)
        }),
    );
    reg(
        "unbind_sso",
        Box::new(|auth, _, v| {
            let x = input(v)?;
            let admin = acting(auth, x)?;
            let id = text(x, "id").ok_or("missing id")?;
            crate::sso::api(auth)?
                .unbind(&admin, id)
                .map_err(crate::sso::msg)?;
            user_row(auth, &admin, id)
        }),
    );
    reg(
        "set_sso_only",
        Box::new(|auth, _, v| {
            let x = input(v)?;
            let admin = acting(auth, x)?;
            let id = text(x, "id").ok_or("missing id")?;
            let on = match x.get("sso_only") {
                Some(Value::Bool(b)) => *b,
                Some(Value::Text(t)) if t == "true" => true,
                Some(Value::Text(t)) if t == "false" => false,
                _ => return Err("missing sso_only (true or false)".into()),
            };
            crate::sso::api(auth)?
                .set_sso_only(&admin, id, on)
                .map_err(crate::sso::msg)?;
            user_row(auth, &admin, id)
        }),
    );
    crate::sso::register(d, auth.clone());
    for (cmd, disabled) in [("disable", true), ("enable", false)] {
        reg(
            cmd,
            Box::new(move |auth, _, v| {
                let x = input(v)?;
                let admin = acting(auth, x)?;
                let id = text(x, "id").ok_or("missing id")?;
                auth.mfa()
                    .ok_or("this instance has no account administration")?
                    .set_user_disabled(&admin, id, disabled)
                    .map_err(msg)?;
                user_row(auth, &admin, id)
            }),
        );
    }
}
