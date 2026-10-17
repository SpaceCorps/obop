//! The provider catalogue shipped in the binary.
//!
//! Every entry says how much of it exists: `working` (implemented and tested end to end against a
//! local counterpart), `generic_adapter` (a generic engine configured for the service; tested
//! against a local counterpart, not against the service itself) or `definition_only` (metadata,
//! setup instructions and sub-processor facts, nothing runs). Facts about a vendor that were not
//! checked are marked "verify with the provider" instead of being asserted.

use obop_types::DataClass;
use serde_json::{Value as J, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Maturity {
    Working,
    GenericAdapter,
    DefinitionOnly,
}

impl Maturity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::GenericAdapter => "generic_adapter",
            Self::DefinitionOnly => "definition_only",
        }
    }
}

/// The generic engine behind a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    WebhookOut,
    WebhookIn,
    RestPull,
    CsvExport,
    /// Nothing runs.
    None,
}

impl Engine {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WebhookOut => "webhook_out",
            Self::WebhookIn => "webhook_in",
            Self::RestPull => "rest_pull",
            Self::CsvExport => "csv_export",
            Self::None => "none",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [
            Self::WebhookOut,
            Self::WebhookIn,
            Self::RestPull,
            Self::CsvExport,
            Self::None,
        ]
        .into_iter()
        .find(|e| e.as_str() == s)
    }
}

#[derive(Debug, Clone)]
pub struct ConfigField {
    pub key: &'static str,
    pub label: &'static str,
    /// Goes into the sealed credentials, never into the readable config.
    pub secret: bool,
    pub required: bool,
    pub help: &'static str,
}

#[derive(Debug, Clone)]
pub struct OAuthDef {
    pub auth_url: &'static str,
    pub token_url: &'static str,
    pub scopes: &'static str,
}

#[derive(Debug, Clone)]
pub struct Provider {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub maturity: Maturity,
    pub engine: Engine,
    /// api_key | bearer_token | basic | oauth2 | webhook_in | webhook_out | none
    pub auth_kind: &'static str,
    /// inbound | outbound | both
    pub direction: &'static str,
    /// The classes this provider may receive at most.
    pub max_classes: Vec<DataClass>,
    pub purpose: &'static str,
    pub description: &'static str,
    pub legal_name: &'static str,
    pub country: &'static str,
    pub dpa_url: &'static str,
    /// yes | no | verify
    pub eu_adequate: &'static str,
    pub verify_note: &'static str,
    /// Hosts the provider talks to; empty means the connection names its own.
    pub allowed_hosts: Vec<&'static str>,
    pub config: Vec<ConfigField>,
    pub setup: &'static str,
    /// standard | github | stripe, for inbound webhooks.
    pub webhook_scheme: &'static str,
    /// json | form | slack, how an outbound record is encoded.
    pub body_format: &'static str,
    pub oauth: Option<OAuthDef>,
}

impl Provider {
    pub fn allows_class(&self, c: DataClass) -> bool {
        c != DataClass::Secret && self.max_classes.contains(&c)
    }

    pub fn config_json(&self) -> J {
        J::Array(
            self.config
                .iter()
                .map(|c| {
                    json!({"key": c.key, "label": c.label, "secret": c.secret,
                           "required": c.required, "help": c.help})
                })
                .collect(),
        )
    }

    pub fn max_classes_csv(&self) -> String {
        self.max_classes
            .iter()
            .map(|c| c.as_str())
            .collect::<Vec<_>>()
            .join(",")
    }

    pub fn can_send(&self) -> bool {
        matches!(self.direction, "outbound" | "both")
    }

    pub fn can_receive(&self) -> bool {
        matches!(self.direction, "inbound" | "both")
    }
}

const NOT_VERIFIED: &str = "verify with the provider";
const LINKS_NOTE: &str = "Links and entity names are starting points, not legal advice: confirm the current data processing agreement and the contracting entity with the provider before you rely on them.";

use DataClass::{Financial, Internal, None as NoClass, PublicDirectory};

fn all_but_secret() -> Vec<DataClass> {
    vec![
        NoClass,
        PublicDirectory,
        Internal,
        DataClass::ConfidentialHr,
        DataClass::SpecialCategory,
        Financial,
    ]
}

fn f(
    key: &'static str,
    label: &'static str,
    secret: bool,
    required: bool,
    help: &'static str,
) -> ConfigField {
    ConfigField {
        key,
        label,
        secret,
        required,
        help,
    }
}

#[allow(clippy::too_many_arguments)]
fn base() -> Provider {
    Provider {
        id: "",
        name: "",
        category: "generic",
        maturity: Maturity::DefinitionOnly,
        engine: Engine::None,
        auth_kind: "none",
        direction: "outbound",
        max_classes: vec![NoClass, PublicDirectory, Internal],
        purpose: "",
        description: "",
        legal_name: NOT_VERIFIED,
        country: NOT_VERIFIED,
        dpa_url: "",
        eu_adequate: "verify",
        verify_note: LINKS_NOTE,
        allowed_hosts: vec![],
        config: vec![],
        setup: "",
        webhook_scheme: "standard",
        body_format: "json",
        oauth: None,
    }
}

/// Every provider, in the order the catalogue shows them.
pub fn providers() -> Vec<Provider> {
    let recipient = "The recipient you configure. You are the controller; they are your processor or a recipient, and the agreement with them is yours to record.";
    vec![
        Provider {
            id: "webhook_out",
            name: "Webhook (send)",
            category: "generic",
            maturity: Maturity::Working,
            engine: Engine::WebhookOut,
            auth_kind: "webhook_out",
            direction: "outbound",
            max_classes: all_but_secret(),
            purpose: "Send selected, minimised records to an HTTPS endpoint you operate or contract with, when the events you choose happen.",
            description: "Posts a signed JSON document (Standard Webhooks signature) per event or per manual send. Only the fields on the allowlist leave, and each one is treated by its data class.",
            legal_name: recipient,
            country: "Set by you: the country of the recipient",
            eu_adequate: "verify",
            config: vec![
                f(
                    "url",
                    "Endpoint URL",
                    false,
                    true,
                    "An https:// address. Plain http is refused.",
                ),
                f(
                    "signing_secret",
                    "Signing secret",
                    true,
                    true,
                    "Shared with the receiver so it can verify each delivery. At least 16 characters. It is stored encrypted and never shown again.",
                ),
                f(
                    "events",
                    "Events",
                    false,
                    false,
                    "Command ids that trigger a delivery, comma separated, for example lead.mark_won. Empty means you send by hand.",
                ),
            ],
            setup: "1. Create an endpoint at the receiver and note its URL. 2. Choose a signing secret and give it to the receiver. 3. Pick the fields to send and review the privacy preview. 4. Record the legal basis and the agreement with the receiver. 5. Test, then connect.",
            ..base()
        },
        Provider {
            id: "webhook_in",
            name: "Webhook (receive)",
            category: "generic",
            maturity: Maturity::Working,
            engine: Engine::WebhookIn,
            auth_kind: "webhook_in",
            direction: "inbound",
            max_classes: all_but_secret(),
            purpose: "Receive events from a system you operate or contract with and turn them into records of one model.",
            description: "Accepts signed POSTs at a per-connection URL. The signature, the timestamp and the event id are checked before anything is read; the payload is mapped to the fields on the allowlist and created through the same governed commands as any other write.",
            legal_name: recipient,
            country: "Set by you: the country of the sender",
            config: vec![
                f(
                    "signing_secret",
                    "Signing secret",
                    true,
                    true,
                    "The sender signs each request with it. At least 16 characters. Stored encrypted, never shown again.",
                ),
                f(
                    "target_model",
                    "Target model",
                    false,
                    true,
                    "The model that receives records, for example lead.",
                ),
                f(
                    "mapping",
                    "Field mapping",
                    false,
                    false,
                    "JSON object: payload path to field, for example {\"data.title\": \"title\"}. Empty maps matching names.",
                ),
            ],
            setup: "1. Choose a signing secret. 2. Create the connection; its address is shown on the connection. 3. Configure the sender to POST there and sign with the secret (Standard Webhooks headers). 4. Review the mapping and the allowlist, then connect.",
            ..base()
        },
        Provider {
            id: "rest_api_pull",
            name: "REST API (pull)",
            category: "generic",
            maturity: Maturity::Working,
            engine: Engine::RestPull,
            auth_kind: "api_key",
            direction: "inbound",
            max_classes: all_but_secret(),
            purpose: "Periodically read records from a JSON API into one model.",
            description: "Fetches a JSON document over https, finds the list of records, maps each to model fields and creates or updates records through the governed commands. Records are matched again on later runs by an external id.",
            legal_name: recipient,
            country: "Set by you: the country of the API operator",
            config: vec![
                f(
                    "url",
                    "API URL",
                    false,
                    true,
                    "An https:// address that answers JSON.",
                ),
                f(
                    "api_key",
                    "API key or token",
                    true,
                    false,
                    "Sent in the header named below. Stored encrypted, never shown again.",
                ),
                f(
                    "header_name",
                    "Header name",
                    false,
                    false,
                    "Defaults to Authorization with the Bearer scheme.",
                ),
                f(
                    "target_model",
                    "Target model",
                    false,
                    true,
                    "The model that receives records.",
                ),
                f(
                    "records_path",
                    "Records path",
                    false,
                    false,
                    "Dotted path to the list inside the document, for example data.items. Empty means the document is the list.",
                ),
                f(
                    "mapping",
                    "Field mapping",
                    false,
                    false,
                    "JSON object: payload path to field.",
                ),
                f(
                    "external_id_key",
                    "External id key",
                    false,
                    false,
                    "Payload key that identifies a record, so the next run updates it instead of adding it again.",
                ),
                f(
                    "interval_minutes",
                    "Interval (minutes)",
                    false,
                    false,
                    "0 or empty means you pull by hand.",
                ),
            ],
            setup: "1. Create an API key with read access at the other system. 2. Enter its URL and key. 3. Map the payload to fields and set an external id. 4. Record the legal basis, test, connect and run once by hand.",
            ..base()
        },
        Provider {
            id: "csv_export",
            name: "CSV export",
            category: "generic",
            maturity: Maturity::Working,
            engine: Engine::CsvExport,
            auth_kind: "none",
            direction: "outbound",
            max_classes: all_but_secret(),
            purpose: "Produce a CSV file of selected fields for the person who downloads it.",
            description: "Writes the fields on the allowlist through the same policy as every other flow (pseudonyms instead of personal data by default) and hands the file to the person who asked. Nothing is sent to a third party by obop; what happens to the file afterwards is the recipient's responsibility.",
            legal_name: "The person who downloads the file, then whoever they pass it to.",
            country: "Set by you",
            eu_adequate: "verify",
            config: vec![f(
                "filename",
                "File name",
                false,
                false,
                "Defaults to the model name.",
            )],
            setup: "1. Pick the model and fields. 2. Review the privacy preview: this is what the file will contain. 3. Record the purpose, connect and download.",
            ..base()
        },
        Provider {
            id: "slack_webhook",
            name: "Slack (incoming webhook)",
            category: "messaging",
            maturity: Maturity::GenericAdapter,
            engine: Engine::WebhookOut,
            auth_kind: "webhook_out",
            direction: "outbound",
            max_classes: vec![NoClass, PublicDirectory, Internal],
            purpose: "Post a short text notification to a Slack channel when an event happens.",
            description: "Uses Slack's incoming webhook: a text message listing the allowed fields. Not tested against Slack itself; the generic webhook engine is tested against a local receiver.",
            legal_name: "Slack Technologies, an entity of Salesforce (confirm the contracting entity of your workspace)",
            country: NOT_VERIFIED,
            dpa_url: "",
            eu_adequate: "verify",
            allowed_hosts: vec!["hooks.slack.com"],
            body_format: "slack",
            config: vec![
                f(
                    "url",
                    "Incoming webhook URL",
                    true,
                    true,
                    "Starts with https://hooks.slack.com/services/. The URL itself is the secret, so it is stored encrypted.",
                ),
                f(
                    "events",
                    "Events",
                    false,
                    false,
                    "Command ids that post a message, comma separated.",
                ),
            ],
            setup: "1. In Slack create an app with an incoming webhook for one channel. 2. Paste the webhook URL. 3. Choose events and the few fields a notification needs; personal data is pseudonymised unless you change that. 4. Record the agreement with Slack, test, connect.",
            ..base()
        },
        Provider {
            id: "stripe",
            name: "Stripe",
            category: "payments",
            maturity: Maturity::GenericAdapter,
            engine: Engine::WebhookOut,
            auth_kind: "api_key",
            direction: "both",
            max_classes: vec![NoClass, PublicDirectory, Internal, Financial],
            purpose: "Create a Stripe customer from a minimised contact record, receive Stripe events, and take card payments for the web shop through Stripe Checkout.",
            description: "Outbound: a form-encoded POST to the Stripe API with a restricted API key (customer name and email only if your policy allows them in clear). Inbound: events signed with the Stripe webhook secret. The web shop (eCommerce) uses this connection, when it is connected and owned by the instance, to create Checkout Sessions and refunds with the same key and to verify the payment webhook at /hooks/pay/stripe with the signing secret; it sends Stripe only the order total and number, no customer data. Not tested against Stripe itself; the encodings, the signature scheme and the Checkout flow are tested against a local stand-in.",
            legal_name: "Stripe Payments Europe, Ltd. for customers in the EEA (confirm for your account)",
            country: "Ireland (confirm for your account)",
            dpa_url: "https://stripe.com/legal/dpa",
            eu_adequate: "verify",
            allowed_hosts: vec!["api.stripe.com"],
            webhook_scheme: "stripe",
            body_format: "form",
            config: vec![
                f(
                    "url",
                    "API URL",
                    false,
                    true,
                    "For customers: https://api.stripe.com/v1/customers (the shop's card payments use the key and the signing secret, not this URL)",
                ),
                f(
                    "api_key",
                    "Restricted API key",
                    true,
                    true,
                    "A restricted key. For customers it needs write access to customers; for the web shop it needs write access to Checkout Sessions and Refunds. Stored encrypted, never shown again.",
                ),
                f(
                    "signing_secret",
                    "Webhook signing secret",
                    true,
                    false,
                    "From the Stripe dashboard, for inbound events. For the web shop add a webhook endpoint https://YOUR-SHOP/hooks/pay/stripe with the events checkout.session.completed, checkout.session.async_payment_succeeded, checkout.session.async_payment_failed and checkout.session.expired.",
                ),
                f(
                    "events",
                    "Events",
                    false,
                    false,
                    "Command ids that create or update a Stripe customer.",
                ),
                f(
                    "target_model",
                    "Model for inbound events",
                    false,
                    false,
                    "The model that receives Stripe events. Leave empty to only send.",
                ),
                f(
                    "mapping",
                    "Field mapping for events",
                    false,
                    false,
                    "JSON object: payload path to field, for example {\"data.object.name\": \"contact_name\"}.",
                ),
            ],
            setup: "1. Create a restricted key in Stripe. 2. Enter it with the customers URL. 3. Allow only the fields Stripe needs, and decide per field whether they leave as a pseudonym or in clear (not needed for shop payments, which send no customer data). 4. Record the Stripe DPA and your legal basis (contract is typical). 5. For the web shop, enter the webhook signing secret, connect, and add stripe to the site setting payment_methods.",
            ..base()
        },
        Provider {
            id: "github_webhook",
            name: "GitHub (webhook)",
            category: "developer",
            maturity: Maturity::GenericAdapter,
            engine: Engine::WebhookIn,
            auth_kind: "webhook_in",
            direction: "inbound",
            max_classes: vec![NoClass, PublicDirectory, Internal],
            purpose: "Turn GitHub events (issues, pull requests) into records.",
            description: "Accepts GitHub's signed deliveries (X-Hub-Signature-256) and maps the JSON payload to the fields you allow. Not tested against GitHub itself; the signature scheme is tested with GitHub's published example.",
            legal_name: "GitHub, Inc. (confirm the contracting entity of your organisation)",
            country: NOT_VERIFIED,
            dpa_url: "https://github.com/customer-terms/github-data-protection-agreement",
            eu_adequate: "verify",
            webhook_scheme: "github",
            config: vec![
                f(
                    "signing_secret",
                    "Webhook secret",
                    true,
                    true,
                    "The secret you set on the GitHub webhook.",
                ),
                f(
                    "target_model",
                    "Target model",
                    false,
                    true,
                    "The model that receives records.",
                ),
                f(
                    "mapping",
                    "Field mapping",
                    false,
                    false,
                    "For example {\"issue.title\": \"title\"}.",
                ),
                f(
                    "event_types",
                    "Event types",
                    false,
                    false,
                    "Comma separated values of X-GitHub-Event to accept; empty accepts all.",
                ),
            ],
            setup: "1. In the GitHub repository add a webhook with content type application/json and a secret. 2. Use the address shown on the connection as the payload URL. 3. Map the payload and connect.",
            ..base()
        },
        Provider {
            id: "google_oauth",
            name: "Google (calendar and contacts, read)",
            category: "calendar_contacts",
            maturity: Maturity::DefinitionOnly,
            engine: Engine::None,
            auth_kind: "oauth2",
            direction: "inbound",
            max_classes: vec![NoClass, PublicDirectory, Internal],
            purpose: "Read calendar events or contacts of a Google account you own.",
            description: "Needs your own OAuth client (a Google Cloud project with the consent screen configured); obop ships none. The OAuth 2.0 flow with PKCE is implemented and tested against a local fake provider, but no Google-specific mapping or call exists yet: this entry is a definition.",
            legal_name: "Google (confirm: Google Ireland Limited or Google LLC, depending on your agreement)",
            country: NOT_VERIFIED,
            dpa_url: "https://cloud.google.com/terms/data-processing-addendum",
            eu_adequate: "verify",
            allowed_hosts: vec![
                "accounts.google.com",
                "oauth2.googleapis.com",
                "www.googleapis.com",
                "people.googleapis.com",
            ],
            oauth: Some(OAuthDef {
                auth_url: "https://accounts.google.com/o/oauth2/v2/auth",
                token_url: "https://oauth2.googleapis.com/token",
                scopes: "https://www.googleapis.com/auth/calendar.readonly https://www.googleapis.com/auth/contacts.readonly",
            }),
            config: vec![],
            setup: "Needs your own OAuth client. Create one in Google Cloud, add the redirect address shown in the connectors settings, and enter its client id and secret under the provider policy. Nothing runs until an adapter exists.",
            ..base()
        },
        Provider {
            id: "microsoft_oauth",
            name: "Microsoft (calendar and contacts, read)",
            category: "calendar_contacts",
            maturity: Maturity::DefinitionOnly,
            engine: Engine::None,
            auth_kind: "oauth2",
            direction: "inbound",
            max_classes: vec![NoClass, PublicDirectory, Internal],
            purpose: "Read calendar events or contacts of a Microsoft 365 account you own.",
            description: "Needs your own OAuth client (an app registration in Microsoft Entra); obop ships none. The OAuth 2.0 flow with PKCE is implemented and tested locally; no Microsoft-specific mapping or call exists yet: this entry is a definition.",
            legal_name: "Microsoft (confirm the contracting entity of your tenant)",
            country: NOT_VERIFIED,
            dpa_url: "https://www.microsoft.com/licensing/docs/view/Microsoft-Products-and-Services-Data-Protection-Addendum-DPA",
            eu_adequate: "verify",
            allowed_hosts: vec!["login.microsoftonline.com", "graph.microsoft.com"],
            oauth: Some(OAuthDef {
                auth_url: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
                token_url: "https://login.microsoftonline.com/common/oauth2/v2.0/token",
                scopes: "offline_access Calendars.Read Contacts.Read",
            }),
            config: vec![],
            setup: "Needs your own OAuth client. Register an app in Microsoft Entra, add the redirect address shown in the connectors settings, and enter its client id and secret under the provider policy. Nothing runs until an adapter exists.",
            ..base()
        },
        Provider {
            id: "zapier",
            name: "Zapier",
            category: "automation",
            maturity: Maturity::GenericAdapter,
            engine: Engine::WebhookOut,
            auth_kind: "webhook_out",
            direction: "outbound",
            max_classes: vec![NoClass, PublicDirectory, Internal],
            purpose: "Start a Zap with a minimised record when an event happens.",
            description: "A Zapier catch hook is a webhook URL: the generic webhook engine sends to it. Not tested against Zapier itself.",
            legal_name: "Zapier, Inc. (confirm the contracting entity)",
            country: NOT_VERIFIED,
            eu_adequate: "verify",
            allowed_hosts: vec!["hooks.zapier.com"],
            config: vec![
                f(
                    "url",
                    "Catch hook URL",
                    true,
                    true,
                    "Starts with https://hooks.zapier.com/hooks/catch/. Stored encrypted.",
                ),
                f(
                    "signing_secret",
                    "Signing secret",
                    true,
                    true,
                    "Used to sign deliveries; Zapier does not check it, a filter step can.",
                ),
                f(
                    "events",
                    "Events",
                    false,
                    false,
                    "Command ids that trigger the Zap.",
                ),
            ],
            setup: "1. Create a Zap with a Catch Hook trigger. 2. Paste its URL. 3. Pick fields and events, review the preview, record the agreement with Zapier, connect.",
            ..base()
        },
        Provider {
            id: "make",
            name: "Make",
            category: "automation",
            maturity: Maturity::GenericAdapter,
            engine: Engine::WebhookOut,
            auth_kind: "webhook_out",
            direction: "outbound",
            max_classes: vec![NoClass, PublicDirectory, Internal],
            purpose: "Start a Make scenario with a minimised record when an event happens.",
            description: "A Make custom webhook is an https URL: the generic webhook engine sends to it. Not tested against Make itself.",
            legal_name: "Celonis (Make) (confirm the contracting entity)",
            country: NOT_VERIFIED,
            eu_adequate: "verify",
            allowed_hosts: vec![
                "hook.eu1.make.com",
                "hook.eu2.make.com",
                "hook.us1.make.com",
                "hook.us2.make.com",
                "hook.make.com",
            ],
            config: vec![
                f(
                    "url",
                    "Webhook URL",
                    true,
                    true,
                    "The custom webhook address of the scenario. Stored encrypted.",
                ),
                f(
                    "signing_secret",
                    "Signing secret",
                    true,
                    true,
                    "Used to sign deliveries.",
                ),
                f(
                    "events",
                    "Events",
                    false,
                    false,
                    "Command ids that trigger the scenario.",
                ),
            ],
            setup: "1. Add a Custom webhook module to a scenario. 2. Paste its address. 3. Pick fields and events, review the preview, record the agreement, connect.",
            ..base()
        },
        Provider {
            id: "n8n",
            name: "n8n",
            category: "automation",
            maturity: Maturity::GenericAdapter,
            engine: Engine::WebhookOut,
            auth_kind: "webhook_out",
            direction: "outbound",
            max_classes: vec![NoClass, PublicDirectory, Internal],
            purpose: "Start an n8n workflow with a minimised record when an event happens.",
            description: "An n8n webhook node is an https URL on your own or n8n's cloud host: name the host in the connection. The generic webhook engine sends to it. Not tested against n8n itself.",
            legal_name: "n8n GmbH if you use n8n cloud, otherwise your own organisation (confirm)",
            country: NOT_VERIFIED,
            eu_adequate: "verify",
            config: vec![
                f(
                    "url",
                    "Webhook URL",
                    false,
                    true,
                    "The production URL of the webhook node.",
                ),
                f(
                    "signing_secret",
                    "Signing secret",
                    true,
                    true,
                    "Used to sign deliveries.",
                ),
                f(
                    "events",
                    "Events",
                    false,
                    false,
                    "Command ids that trigger the workflow.",
                ),
            ],
            setup: "1. Add a Webhook node to a workflow and activate it. 2. Paste its production URL. 3. Pick fields and events, review the preview, record the agreement, connect.",
            ..base()
        },
    ]
}

pub fn provider(id: &str) -> Option<Provider> {
    providers().into_iter().find(|p| p.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_entries_are_complete() {
        let all = providers();
        let mut ids: Vec<_> = all.iter().map(|p| p.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), all.len());
        for p in &all {
            assert!(
                !p.name.is_empty() && !p.purpose.is_empty() && !p.description.is_empty(),
                "{}",
                p.id
            );
            assert!(
                !p.max_classes.contains(&DataClass::Secret),
                "{}: secret may never be a maximum",
                p.id
            );
            assert!(matches!(p.direction, "inbound" | "outbound" | "both"));
            assert!(matches!(p.eu_adequate, "yes" | "no" | "verify"));
            assert!(matches!(
                p.auth_kind,
                "api_key"
                    | "bearer_token"
                    | "basic"
                    | "oauth2"
                    | "webhook_in"
                    | "webhook_out"
                    | "none"
            ));
            // Only entries that run name an engine; a definition runs nothing.
            assert_eq!(
                p.maturity == Maturity::DefinitionOnly,
                p.engine == Engine::None,
                "{}",
                p.id
            );
            assert!(p.oauth.is_some() == (p.auth_kind == "oauth2"), "{}", p.id);
        }
    }

    #[test]
    fn maturity_is_stated_honestly() {
        let by = |id: &str| provider(id).unwrap().maturity;
        for id in ["webhook_out", "webhook_in", "rest_api_pull", "csv_export"] {
            assert_eq!(by(id), Maturity::Working, "{id}");
        }
        for id in [
            "slack_webhook",
            "stripe",
            "github_webhook",
            "zapier",
            "make",
            "n8n",
        ] {
            assert_eq!(by(id), Maturity::GenericAdapter, "{id}");
        }
        for id in ["google_oauth", "microsoft_oauth"] {
            assert_eq!(by(id), Maturity::DefinitionOnly, "{id}");
        }
    }

    #[test]
    fn unverified_facts_say_so() {
        // Every entry whose adequacy is not established carries the verification note.
        for p in providers() {
            if p.eu_adequate == "verify" {
                assert!(p.verify_note.contains("not legal advice"), "{}", p.id);
            }
        }
    }

    #[test]
    fn engines_round_trip() {
        for e in [
            Engine::WebhookOut,
            Engine::WebhookIn,
            Engine::RestPull,
            Engine::CsvExport,
            Engine::None,
        ] {
            assert_eq!(Engine::parse(e.as_str()), Some(e));
        }
    }
}
