//! The `sso_provider` model of the Customize module (D91, Proposed): the one Microsoft Entra
//! tenant people sign in with, and the commands that set it up. It has no records of its own; the
//! handlers call the identity provider (`Authenticator::sso`), which holds the rules and seals
//! the client secret.
//!
//! `configure` and `clear` are refused for an agent, also as an approved proposal: they carry a
//! client secret and decide who may sign in. (D41 puts a command on every surface or on none, so
//! they are listed to agents and refuse when they run.) Binding an account and making it SSO-only are commands of
//! `user_account` (an agent can propose them; a person decides).

use crate::accounts::{acting, input, s, text};
use obop_commands::{ADMIN_GROUP, Authenticator, Dispatcher, SsoApi, SsoError, Value};
use obop_registry::{CommandDef, FieldDef, FieldKind, ModelDef};
use obop_types::DataClass;
use std::sync::{Arc, Weak};

/// Registry metadata of the model.
pub fn sso_provider_model() -> ModelDef {
    let f =
        |name, description| FieldDef::new(name, description, DataClass::None, false, ADMIN_GROUP);
    // D41 puts a command on every surface or on none, so these two cannot be hidden from the MCP
    // tool list; they refuse an agent (and the proposal of one) when they run.
    let configure = CommandDef::new(
        "configure",
        "Set the Microsoft Entra tenant people sign in with. Give tenant (the tenant id, a GUID; common, organizations and consumers are refused), client_id (the application id of the app registration) and, the first time, client_secret; enabled defaults to true. Accounts are bound with user_account.bind_sso. Refused for agents",
        ADMIN_GROUP,
    );
    let clear = CommandDef::new(
        "clear",
        "Remove the Microsoft sign-in configuration. Bindings stay. Accounts that sign in with Microsoft only can then sign in only through the administrators who keep a password and a second factor. Refused for agents",
        ADMIN_GROUP,
    );
    ModelDef {
        name: "sso_provider",
        description: "The Microsoft Entra tenant people sign in with: one tenant, pinned by its id. Changed only by administrators, never by an agent",
        read_group: ADMIN_GROUP,
        write_group: ADMIN_GROUP,
        access: None,
        fields: vec![
            f("tenant", "The tenant id (a GUID) every Microsoft sign-in is checked against"),
            f("client_id", "The application (client) id of the app registration in that tenant"),
            FieldDef::new(
                "client_secret",
                "Input of configure: the client secret of the app registration, kept sealed under the server secret. Never shown",
                DataClass::Secret,
                false,
                ADMIN_GROUP,
            ),
            f("enabled", "Whether the sign-in page offers Sign in with Microsoft").kind(FieldKind::Bool),
            f("secret_set", "Whether a client secret is stored").kind(FieldKind::Bool),
            f("modified_at", "When the configuration was last changed"),
            f("last_editor", "Who changed it last"),
            f("break_glass", "The administrators who sign in locally with a password and a second factor (the way in when Microsoft cannot be reached), separated by commas"),
            f("break_glass_enough", "Whether there are enough of them (at least two) to make an account sign in with Microsoft only").kind(FieldKind::Bool),
        ],
        commands: vec![configure, clear],
        ext: Default::default(),
    }
}

pub(crate) fn api(auth: &dyn Authenticator) -> Result<&dyn SsoApi, String> {
    auth.sso()
        .ok_or_else(|| "this instance has no Microsoft sign-in".to_string())
}

pub(crate) fn msg(e: SsoError) -> String {
    match e {
        SsoError::Refused(m) | SsoError::Invalid(m) | SsoError::Unavailable(m) => m,
        SsoError::UnknownKey => "the signing key is not known".into(),
        SsoError::NotFound => "not allowed".into(),
    }
}

/// The row of the one provider, or `None` when none is stored.
fn row(
    auth: &dyn Authenticator,
    admin: &obop_commands::Principal,
) -> Result<Option<Value>, String> {
    let b = api(auth)?.bindings(admin).map_err(msg)?;
    let p = &b["provider"];
    if p.is_null() {
        return Ok(None);
    }
    let flag = |v: &serde_json::Value| Value::Bool(v.as_bool() == Some(true));
    let names: Vec<&str> = b["break_glass"]
        .as_array()
        .map(|a| a.iter().filter_map(|x| x.as_str()).collect())
        .unwrap_or_default();
    Ok(Some(Value::map([
        ("id", s("provider")),
        ("tenant", s(p["tenant"].as_str().unwrap_or(""))),
        ("client_id", s(p["client_id"].as_str().unwrap_or(""))),
        ("enabled", flag(&p["enabled"])),
        ("secret_set", flag(&p["secret_set"])),
        ("break_glass", Value::Text(names.join(", "))),
        ("break_glass_enough", flag(&b["break_glass_enough"])),
    ])))
}

/// The command is a person's: it carries a client secret and decides who may sign in, so an agent
/// does not run it, not even as an approved proposal.
fn person_only(x: &std::collections::BTreeMap<String, Value>) -> Result<(), String> {
    if text(x, "_caller").unwrap_or("").ends_with("/agent") {
        return Err("the Microsoft sign-in configuration is changed by a person, not by an agent or a proposal of one".into());
    }
    Ok(())
}

type Handler = Box<dyn Fn(&dyn Authenticator, &Value) -> Result<Value, String> + Send + Sync>;

/// Bind the handlers. Called with the account handlers, once at start-up.
pub(crate) fn register(d: &Arc<Dispatcher>, auth: Arc<dyn Authenticator>) {
    let m = crate::installer::admin_module();
    let reg = |cmd: &str, h: Handler| {
        let (a, _w): (Arc<dyn Authenticator>, Weak<Dispatcher>) = (auth.clone(), Arc::downgrade(d));
        d.register(&m, "sso_provider", cmd, Box::new(move |v| h(&*a, v)));
    };
    reg(
        "list",
        Box::new(|auth, v| {
            let admin = acting(auth, input(v)?)?;
            Ok(Value::List(row(auth, &admin)?.into_iter().collect()))
        }),
    );
    reg(
        "get",
        Box::new(|auth, v| {
            let admin = acting(auth, input(v)?)?;
            row(auth, &admin)?.ok_or_else(|| "no Microsoft sign-in is configured".to_string())
        }),
    );
    reg(
        "configure",
        Box::new(|auth, v| {
            let x = input(v)?;
            person_only(x)?;
            let admin = acting(auth, x)?;
            let tenant = text(x, "tenant").ok_or("missing tenant (the tenant id, a GUID)")?;
            let client_id = text(x, "client_id").ok_or("missing client_id")?;
            let enabled = match x.get("enabled") {
                None | Some(Value::Null) => true,
                Some(Value::Bool(b)) => *b,
                Some(Value::Text(t)) if t == "true" => true,
                Some(Value::Text(t)) if t == "false" => false,
                Some(_) => return Err("enabled must be true or false".into()),
            };
            api(auth)?
                .configure(&admin, tenant, client_id, text(x, "client_secret"), enabled)
                .map_err(msg)?;
            row(auth, &admin)?.ok_or_else(|| "the configuration was not stored".to_string())
        }),
    );
    reg(
        "clear",
        Box::new(|auth, v| {
            let x = input(v)?;
            person_only(x)?;
            let admin = acting(auth, x)?;
            api(auth)?.clear(&admin).map_err(msg)?;
            Ok(Value::map([("id", s("provider"))]))
        }),
    );
}
