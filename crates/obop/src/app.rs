//! Composition root: the linked first-party modules, one dispatcher and one store.

use obop_commands::{Authenticator, Dispatcher, Principal, Store};
use obop_registry::Module;

pub struct App {
    pub modules: Vec<Module>,
    pub dispatcher: Dispatcher,
    pub store: Store,
}

impl App {
    /// An app over an in-memory store (tests, `modules check`).
    pub fn new() -> Self {
        Self::with_store(Store::new())
    }

    pub fn with_store(store: Store) -> Self {
        let mut d = Dispatcher::new();
        base::register(&mut d, &store);
        product::register(&mut d, &store);
        crm::register(&mut d, &store);
        sales::register(&mut d, &store);
        invoicing::register(&mut d, &store);
        let modules = vec![
            base::module(),
            product::module(),
            crm::module(),
            sales::module(),
            invoicing::module(),
        ];
        Self {
            modules,
            dispatcher: d,
            store,
        }
    }

    /// Fake demo records so every surface has something to show. Does nothing if the database
    /// already holds parties, so restarting never duplicates data.
    pub fn seed_demo(&self) {
        if self.store.count("party") > 0 {
            return;
        }
        let party = base::seed(&self.store);
        product::seed(&self.store);
        crm::seed(&self.store);
        let order = sales::seed(&self.store, &party);
        invoicing::seed(&self.store, &order);
    }
}

/// Development principals, selected by bearer token. Test values only: the server binds to
/// localhost and real authentication (D33) replaces this before any non-local use.
pub fn dev_principal(token: &str) -> Option<Principal> {
    const ALL: &[&str] = &[
        "base.user",
        "privacy.officer",
        "product.user",
        "product.manager",
        "product.cost_viewer",
        "crm.user",
        "sales.user",
        "invoicing.user",
        "invoicing.manager",
    ];
    match token {
        "dev-admin" => Some(Principal::user("dev-admin", ALL)),
        "dev-agent" => Some(Principal::agent("dev-agent", ALL, ALL)),
        "dev-sales" => Some(Principal::user(
            "dev-sales",
            &["base.user", "crm.user", "sales.user", "product.user"],
        )),
        _ => None,
    }
}

/// `--dev-auth`: the three fixed dev principals as an `Authenticator`. Loopback only.
pub struct DevAuth;

impl Authenticator for DevAuth {
    fn resolve(&self, token: &str) -> Option<Principal> {
        dev_principal(token)
    }
    fn login(&self, _: &str, _: &str) -> Result<String, String> {
        Err("login is disabled with --dev-auth; use a dev token".into())
    }
    fn issue_agent_token(&self, _: &Principal, _: &[String], _: &str) -> Result<String, String> {
        Err("agent tokens are disabled with --dev-auth".into())
    }
}

impl App {
    /// Every permission group the linked modules declare.
    pub fn all_groups(&self) -> Vec<String> {
        let mut g: Vec<String> = self
            .modules
            .iter()
            .flat_map(|m| m.permission_groups())
            .map(String::from)
            .collect();
        g.sort();
        g.dedup();
        g
    }
}
