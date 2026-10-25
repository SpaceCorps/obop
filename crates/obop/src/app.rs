//! Composition root: the linked first-party modules, one dispatcher and one store.

use obop_commands::{Authenticator, Dispatcher, Principal, Store};
use obop_connectors::{Config as ConnectorConfig, Runtime as Connectors, SecretBox};
use obop_pkg::{Bundled, Chain, Http, Installer, Marketplace};
use obop_registry::Module;
use std::sync::Arc;

pub struct App {
    pub modules: Vec<Module>,
    pub dispatcher: Arc<Dispatcher>,
    pub store: Store,
    pub installer: Arc<Installer>,
    /// Updating the instance itself (D63).
    pub updater: Arc<obop_update::Updater>,
    pub market: Arc<dyn Marketplace>,
    /// The connectors engines (D54): egress policy, webhooks, OAuth, retention.
    pub connectors: Arc<Connectors>,
    /// Payments for the web shop (D62): bank transfer, Stripe Checkout, the local demo.
    pub payments: Arc<obop_payments::Payments>,
}

/// The bundled catalog, plus a remote one when `OBOP_MARKETPLACE_URL` is set.
fn marketplace() -> Arc<dyn Marketplace> {
    let mut sources: Vec<Box<dyn Marketplace>> = vec![Box::new(Bundled)];
    if let Ok(url) = std::env::var("OBOP_MARKETPLACE_URL") {
        sources.push(Box::new(Http::new(&url)));
    }
    Arc::new(Chain(sources))
}

impl App {
    /// An app over an in-memory store (tests, `modules check`).
    pub fn new() -> Self {
        Self::with_store(Store::new())
    }

    pub fn with_store(store: Store) -> Self {
        let d = Arc::new(Dispatcher::new());
        // The platform time zone (D72): `OBOP_ZONE`, any IANA name, UTC by default. A name the
        // pinned database does not know stops the start-up, not a report next month.
        match obop_values::Zones::from_env() {
            Ok(zones) => d.set_zones(zones),
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(2);
            }
        }
        // Several server processes may share one database (B1): the write lock, proposed plans,
        // sessions and rate limits go through the storage backend.
        match obop_keys::KeyRing::for_backend(&store.backend()) {
            Ok(ring) => d.set_shared(store.backend(), ring),
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(2);
            }
        }
        base::register(&d, &store);
        product::register(&d, &store);
        crm::register(&d, &store);
        sales::register(&d, &store);
        invoicing::register(&d, &store);
        let secrets = SecretBox::open_for(&store.backend()).unwrap_or_else(|e| {
            eprintln!("error: {e}");
            std::process::exit(2);
        });
        let connectors = Connectors::new(
            store.backend(),
            d.clone(),
            secrets,
            ConnectorConfig::from_env(),
        );
        ::connectors::register(&d, &store, &connectors);
        // The shop's payments reuse the connectors' hardened egress client and read the Stripe
        // keys from the instance's own sealed `stripe` connection.
        let payments = obop_payments::Payments::new(
            store.backend(),
            d.clone(),
            connectors.client.clone(),
            Arc::new(obop_payments::ConnectionKeys(connectors.clone())),
            obop_payments::Config::from_env(),
        );
        payments.install();
        let modules = vec![
            base::module(),
            product::module(),
            crm::module(),
            sales::module(),
            invoicing::module(),
            ::connectors::module(),
            obop_pkg::admin_module(),
        ];
        let market = marketplace();
        let installer = Installer::new(store.clone(), &d, modules.clone(), market.clone());
        obop_pkg::register_admin(&d, &installer);
        for (id, why) in installer.load_installed() {
            eprintln!("could not load installed module {id}: {why}");
        }
        // Updating the instance from inside it (D63): the release feed, the compatibility of the
        // installed modules, and the request to the control plane.
        let ring = d.shared_parts().map(|(_, ring)| ring).unwrap_or_default();
        let updater = obop_update::Updater::new(
            store.backend(),
            installer.clone(),
            ring,
            obop_update::Config::from_env(),
        );
        obop_update::register(&d, &updater);
        // A module installed through another server process on this database appears here too.
        let syncing = installer.clone();
        d.set_sync_hook(Box::new(move || syncing.sync_modules()));
        Self {
            modules,
            dispatcher: d,
            store,
            installer,
            updater,
            market,
            connectors,
            payments,
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
        // The seeds write straight to the store, so number what has a sequence field (SO-00001).
        let _ = self.dispatcher.backfill_sequences(&self.store, None, None);
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
        "connectors.user",
        "connectors.admin",
    ];
    match token {
        "dev-admin" => Some(Principal::user("dev-admin", ALL)),
        "dev-agent" => Some(Principal::agent("dev-agent", ALL, ALL)),
        "dev-sales" => Some(Principal::user(
            "dev-sales",
            &[
                "base.user",
                "crm.user",
                "sales.user",
                "product.user",
                "connectors.user",
            ],
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
    /// Bind the `user_account` commands (D69) to the identity provider that serves this
    /// instance. Not done with `--dev-auth`: the fixed dev principals are not accounts.
    pub fn register_accounts(&self, auth: Arc<dyn Authenticator>) {
        obop_pkg::register_accounts(&self.dispatcher, auth);
    }

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
