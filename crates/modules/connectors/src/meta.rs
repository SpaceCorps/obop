//! The registry metadata of the `connectors` module: models, fields (each with a data class and a
//! permission group), and commands. REST, MCP, the CLI and the backoffice are generated from it.

use obop_registry::{CommandDef, FieldDef as F, FieldKind, ModelDef, Module};
use obop_types::DataClass::{self, Internal, None as NoClass, Secret};

/// Users who connect their own accounts and see their own connections.
pub const USER: &str = "connectors.user";
/// Administrators of the app: the catalogue policy, every connection's metadata, the register.
/// `platform.admin` holds every group; other administrators hold this one next to `connectors.user`.
pub const ADMIN: &str = "connectors.admin";

fn t(name: &'static str, desc: &'static str, g: &'static str) -> F {
    F::new(name, desc, NoClass, false, g)
}
fn long(name: &'static str, desc: &'static str, g: &'static str) -> F {
    t(name, desc, g).kind(FieldKind::LongText)
}
fn num(name: &'static str, desc: &'static str, g: &'static str) -> F {
    t(name, desc, g).kind(FieldKind::Number)
}
fn flag(name: &'static str, desc: &'static str, g: &'static str) -> F {
    t(name, desc, g).kind(FieldKind::Bool)
}
fn sel(name: &'static str, desc: &'static str, g: &'static str, o: &[&str]) -> F {
    t(name, desc, g).select(o)
}
fn person(name: &'static str, desc: &'static str, g: &'static str) -> F {
    F::new(name, desc, Internal, true, g)
}
fn class(name: &'static str, desc: &'static str, c: DataClass, g: &'static str) -> F {
    F::new(name, desc, c, false, g).kind(FieldKind::LongText)
}

pub const CATEGORIES: [&str; 6] = [
    "generic",
    "messaging",
    "payments",
    "developer",
    "calendar_contacts",
    "automation",
];
pub const MATURITY: [&str; 3] = ["working", "generic_adapter", "definition_only"];
pub const ENGINES: [&str; 5] = [
    "webhook_out",
    "webhook_in",
    "rest_pull",
    "csv_export",
    "none",
];
pub const AUTH_KINDS: [&str; 7] = [
    "api_key",
    "bearer_token",
    "basic",
    "oauth2",
    "webhook_in",
    "webhook_out",
    "none",
];
pub const LEGAL_BASES: [&str; 4] = [
    "consent",
    "contract",
    "legitimate_interest",
    "legal_obligation",
];

pub const DESCRIPTION: &str = "Connect external systems and let people connect their own accounts, with controls that support GDPR compliance built in: a recorded legal basis, withdrawable consent, data minimisation, pseudonymisation by data class, a log without payloads and a processing register. Technical controls, not legal advice or a certification.";

pub fn models() -> Vec<ModelDef> {
    vec![
        ModelDef {
            name: "connector_provider",
            description: "A service or protocol a connection can use, from the catalogue shipped with obop",
            read_group: USER,
            write_group: ADMIN,
            fields: vec![
                t("name", "Name of the provider", USER).required(),
                sel("category", "Kind of service", USER, &CATEGORIES),
                sel("maturity", "How much of it exists: working, generic adapter or definition only", USER, &MATURITY),
                sel("engine", "The generic engine that runs it", USER, &ENGINES),
                sel("auth_kind", "How the connection authenticates", USER, &AUTH_KINDS),
                sel("direction", "Whether data flows in, out or both ways", USER, &["inbound", "outbound", "both"]),
                t("max_classes", "Data classes the provider may receive at most, comma separated", USER),
                long("purpose", "What the provider is used for", USER),
                long("description", "How it works and what is tested", USER),
                t("legal_name", "Legal name of the recipient (sub-processor information)", USER),
                t("country", "Country of the recipient", USER),
                t("dpa_url", "Where the provider publishes its data processing agreement", USER),
                sel("eu_adequate", "Whether transfers rest on EU or adequate-country processing: yes, no or verify with the provider", USER, &["yes", "no", "verify"]),
                long("verify_note", "What to confirm with the provider before relying on these facts", USER),
                t("allowed_hosts", "Hosts the provider connects to, comma separated; empty means the connection names its own", USER),
                long("config_fields", "Settings and credentials the provider needs, as a JSON list", USER),
                long("setup", "Setup instructions", USER),
                sel("webhook_scheme", "Signature scheme of inbound webhooks", USER, &["standard", "github", "stripe"]),
                t("oauth_scopes", "OAuth scopes requested, for OAuth providers", USER),
                flag("enabled", "An administrator enabled the provider", USER),
                flag("users_may_use", "People may connect their own accounts with it", USER),
                flag("dpa_recorded", "The agreement with the provider is recorded", USER),
                sel("legal_basis", "Legal basis the administrator set for this provider", USER, &LEGAL_BASES),
                long("legal_basis_text", "Text shown to people when they are asked for consent", USER),
            ],
            commands: vec![],
        },
        ModelDef {
            name: "provider_policy",
            description: "What an administrator decided about one provider: enabled, for whom, legal basis, agreement and OAuth client",
            read_group: ADMIN,
            write_group: ADMIN,
            fields: vec![
                t("provider", "Id of the provider this policy is for", ADMIN).required(),
                flag("enabled", "The provider may be connected at all", ADMIN),
                flag("users_may_use", "People with connectors.user may connect their own accounts", ADMIN),
                t("allowed_group", "Permission group that may use it; empty means connectors.user", ADMIN),
                sel("legal_basis", "Legal basis recorded for connections of this provider", ADMIN, &LEGAL_BASES),
                long("legal_basis_text", "Text shown to people when they are asked for consent", ADMIN),
                flag("dpa_recorded", "A data processing agreement with the provider is in place", ADMIN),
                t("dpa_reference", "Where the agreement is kept or its reference", ADMIN),
                long("default_policy", "Policy JSON that new connections of this provider start with", ADMIN),
                person_free("oauth_client_id", "OAuth client id you registered with the provider", ADMIN),
                class("oauth_client_secret", "OAuth client secret; write-only, stored encrypted", Secret, ADMIN),
            ],
            commands: vec![],
        },
        ModelDef {
            name: "connection",
            description: "A link to one external system, with its credentials, field allowlist, policy and state",
            read_group: USER,
            write_group: USER,
            fields: vec![
                t("name", "A name that tells you what the connection is for", USER).required(),
                t("provider", "Id of the provider from the catalogue", USER).required(),
                sel("owner_scope", "Whether the connection belongs to the instance or to one person", USER, &["instance", "user"]).required(),
                person("owner_user", "The person who owns the connection", USER),
                t("owner_groups", "Permission groups the owner held when the connection was last changed", ADMIN),
                sel("state", "Where the connection is: draft, connected, paused, error or revoked", USER, &["draft", "connected", "paused", "error", "revoked"]),
                long("config", "Non-secret settings as a JSON object", USER),
                class("credentials", "Credentials as a JSON object; write-only, stored encrypted, never returned", Secret, USER),
                t("credentials_hint", "Whether credentials are set, with at most the last four characters", USER),
                class("oauth_tokens", "OAuth tokens; write-only, stored encrypted, never returned", Secret, USER),
                long("field_allowlist", "Fields that may leave or be accepted, as a JSON object of model to field names; empty means nothing", USER),
                long("policy", "Treatment overrides by class or field and impact-assessment acknowledgements, as JSON", USER),
                num("rate_limit_per_hour", "Most flows per hour", USER),
                num("volume_cap_per_day", "Most records per day", USER),
                num("retention_days", "Days flow logs and runs are kept", USER),
                t("created_at", "When the connection was created", USER),
                t("last_sync_at", "When the connection last ran", USER),
                t("last_error", "Summary of the last failure, without payloads", USER),
                long("result", "Output of the last command (test, sync, authorisation link); never stored", USER),
            ],
            commands: vec![
                CommandDef::new("connect", "Start the flows: needs a recorded legal basis and complete settings", USER)
                    .transition("state", Some("draft"), "connected")
                    .approval(),
                CommandDef::new("pause", "Stop the flows without losing the settings", USER)
                    .transition("state", Some("connected"), "paused")
                    .approval(),
                CommandDef::new("resume", "Start the flows again after a pause", USER)
                    .transition("state", Some("paused"), "connected")
                    .approval(),
                CommandDef::new("retry", "Start the flows again after an error", USER)
                    .transition("state", Some("error"), "connected")
                    .approval(),
                CommandDef::new("revoke", "End the connection for good: access stops and credentials, tokens and pseudonym maps are purged", USER)
                    .transition("state", None, "revoked")
                    .approval(),
                CommandDef::new("test", "Check the settings and the address without sending personal data", USER),
                CommandDef::new("sync", "Run the connection now: send, pull or produce the file", USER).approval(),
                CommandDef::new("oauth_start", "Get the link where the person authorises the connection with the provider", USER),
            ],
        },
        ModelDef {
            name: "consent",
            description: "The legal basis recorded for a connection: who, why, which data classes, and whether it was withdrawn",
            read_group: USER,
            write_group: USER,
            fields: vec![
                F::new("connection", "The connection this basis is for", NoClass, false, USER).reference("connection").required(),
                person("owner_user", "Owner of the connection", USER),
                sel("state", "active, withdrawn, or superseded by a newer record", USER, &["active", "withdrawn", "superseded"]),
                sel("subject_kind", "Whose basis it is: a user, a party or the instance", USER, &["user", "party", "instance"]),
                person("subject", "Id of the party when the subject is a party", USER),
                person("given_by", "Who recorded it", USER),
                long("purpose", "The purpose the data is processed for", USER).required(),
                sel("legal_basis", "consent, contract, legitimate interest or legal obligation", USER, &LEGAL_BASES).required(),
                long("note", "Explanation; for legitimate interest the balancing test", USER),
                t("classes", "Data classes that may flow under this record, computed from the connection", USER),
                long("fields_summary", "Models and fields that may flow, computed from the connection", USER),
                t("recorded_at", "When it was recorded", USER),
                t("withdrawn_at", "When it was withdrawn, if it was", USER),
                person("withdrawn_by", "Who withdrew it", USER),
            ],
            commands: vec![
                CommandDef::new("withdraw", "Withdraw the basis: the connection is revoked and its credentials and tokens are purged", USER)
                    .transition("state", Some("active"), "withdrawn")
                    .approval(),
            ],
        },
        ModelDef {
            name: "sync_run",
            description: "One run of a connection: when, how many records, and how it ended; never the data",
            read_group: USER,
            write_group: ADMIN,
            fields: vec![
                F::new("connection", "The connection that ran", NoClass, false, USER).reference("connection").required(),
                person("owner_user", "Owner of the connection", USER),
                sel("direction", "Whether data went out or came in", USER, &["inbound", "outbound"]),
                sel("kind", "What started it", USER, &["manual", "event", "pull", "webhook", "test"]),
                t("started_at", "When it started", USER),
                t("finished_at", "When it ended", USER),
                sel("status", "How it ended", USER, &["running", "succeeded", "partial", "failed", "retrying", "dead_letter"]),
                num("records", "Records sent or accepted", USER),
                num("rejected", "Records refused", USER),
                num("proposed", "Changes held for approval", USER),
                t("error_summary", "Why it failed, without payloads", USER),
            ],
            commands: vec![],
        },
        ModelDef {
            name: "data_flow_log",
            description: "Append-only record of every flow: when, what, how many, which fields, how treated, and a hash of the payload; never the payload",
            read_group: USER,
            write_group: ADMIN,
            fields: vec![
                F::new("connection", "The connection the flow belongs to", NoClass, false, USER).reference("connection").required(),
                person("owner_user", "Owner of the connection", USER),
                t("at", "When it happened", USER),
                sel("direction", "Whether data went out or came in", USER, &["outbound", "inbound"]),
                t("model", "The model the records belong to", USER),
                num("record_count", "How many records", USER),
                t("fields_sent", "Fields that left or were accepted, comma separated", USER),
                t("fields_denied", "Fields the policy withheld, comma separated", USER),
                t("masking_mode", "Treatments applied, for example pseudonymise=2;allow=1", USER),
                t("payload_hash", "SHA-256 of the payload after treatment", USER),
                sel("outcome", "released, received, refused, rejected or erased", USER, &["released", "received", "refused", "rejected", "erased"]),
                t("reason", "Why a flow was refused or a record rejected", USER),
            ],
            commands: vec![],
        },
        ModelDef {
            name: "processing_register",
            description: "The record of processing activities (GDPR Article 30) generated from the connections; exportable as CSV",
            read_group: ADMIN,
            write_group: ADMIN,
            fields: {
                let mut v = vec![
                    F::new("connection", "The connection this row describes", NoClass, false, ADMIN).reference("connection"),
                    t("name", "Name of the connection", ADMIN),
                    t("state", "State of the connection", ADMIN),
                    t("owner_scope", "Instance or user", ADMIN),
                    person("owner_user", "Owner of the connection", ADMIN),
                    t("provider", "Provider id", ADMIN),
                    long("recipient", "Who receives the data (legal name)", ADMIN),
                    t("recipient_country", "Where the recipient is", ADMIN),
                    long("purpose", "Purpose of the processing", ADMIN),
                    t("legal_basis", "Recorded legal basis", ADMIN),
                    t("consent_recorded_at", "When the basis was recorded", ADMIN),
                    long("data_subjects", "Categories of data subjects", ADMIN),
                    long("personal_data_categories", "Categories of personal data, with class and treatment", ADMIN),
                    long("transfer_basis", "Basis for transfers to third countries", ADMIN),
                    t("agreement_recorded", "Whether the agreement with the recipient is recorded", ADMIN),
                    t("retention_days", "Retention of logs, in days", ADMIN),
                    long("safeguards", "Technical and organisational measures", ADMIN),
                ];
                v.push(long("csv", "The register as CSV; filled by the export command, never stored", ADMIN));
                v
            },
            commands: vec![CommandDef::new("export_csv", "Export the register as CSV", ADMIN)],
        },
        ModelDef {
            name: "egress_policy",
            description: "Instance-wide switches for data flows: the master switch, EU-only providers, the agreement requirement and the host allow-list",
            read_group: ADMIN,
            write_group: ADMIN,
            fields: vec![
                flag("flows_enabled", "Master switch: when off nothing leaves and nothing is accepted", ADMIN),
                flag("eu_only", "Personal data only goes to providers with an EU or adequacy basis", ADMIN),
                flag("require_dpa", "Personal data, even pseudonymised, only goes to providers whose agreement is recorded", ADMIN),
                t("host_allowlist", "Hosts connections may reach, comma separated; empty means no extra restriction", ADMIN),
                num("default_retention_days", "Retention for logs of connections that no longer exist", ADMIN),
                long("result", "Output of the last command (checklist, purge); never stored", ADMIN),
            ],
            commands: vec![
                CommandDef::new("checklist", "Check what is configured: agreements, retention, legal bases, acknowledgements", ADMIN),
                CommandDef::new("purge", "Remove flow logs and runs older than their retention", ADMIN).approval(),
            ],
        },
        ModelDef {
            name: "privacy_preview",
            description: "What a sample outbound record looks like after the policy: the exact payload and the treatment of each field",
            read_group: USER,
            write_group: USER,
            fields: vec![
                t("provider", "Provider the preview is for", USER),
                t("connection", "An existing connection to preview, instead of a provider", USER),
                t("target_model", "The model to preview", USER),
                t("record_id", "A record to use as the sample; empty means the first one", USER),
                long("field_allowlist", "Fields to include, as a JSON object of model to field names", USER),
                long("policy", "Policy JSON to preview", USER),
                long("payload", "The payload after treatment, as JSON; never stored", USER),
                long("treatments", "Each field with its class and treatment, as JSON; never stored", USER),
            ],
            commands: vec![CommandDef::new("render", "Render a sample record after masking and pseudonymisation", USER)],
        },
    ]
}

/// A text field that is an identifier an administrator typed, not personal data.
fn person_free(name: &'static str, desc: &'static str, g: &'static str) -> F {
    F::new(name, desc, Internal, false, g)
}

pub fn module() -> Module {
    let m = Module {
        id: "connectors".into(),
        name: "Connectors".into(),
        description: DESCRIPTION.into(),
        models: models(),
        network_destinations: vec![],
        translations: Default::default(),
    };
    m.translated(&crate::labels::docs())
}
