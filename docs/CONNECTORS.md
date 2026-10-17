# Connectors: compliant data flows (D54)

- **Status:** BINDING design of the built-in `connectors` app (D54); implemented.
- **Scope:** how data enters and leaves an instance, the policy that governs it, how to add a provider, the mapping to GDPR articles, the threat model.

> **What this is, and what it is not.** The connectors provide technical controls that **support** GDPR compliance: a recorded legal basis, consent that can be withdrawn, data minimisation, pseudonymisation by data class, an audit log without payloads, a processing register and encrypted credentials. They are **not legal advice and not a certification**. Whether a purpose is lawful, which legal basis applies, whether a recipient is a processor, whether a transfer is allowed and whether an assessment is needed are decisions of the controller (the business running the instance), and they stay the controller's. The UI says so on the pages where it matters, and so does this document in section 9, which lists for each article what the software does and what remains the owner's job.

## 1. Architecture

```
 UI / REST / MCP / CLI           generated from the registry (D41): one Dispatcher
          |
   connectors module  ------------ guards: scope to the owner, validate input, seal secrets, stamp acknowledgements
          |
   obop-connectors (framework-neutral)
     Egress::prepare   the gate for everything that leaves
     Ingress::accept   the gate for everything that enters (governed create/update as the connector principal)
     net::Client       the one outbound HTTP client (SSRF guard, pinned DNS, no redirects)
     webhook, oauth    signatures and the authorization code flow with PKCE
     secretbox         HKDF + ChaCha20-Poly1305 for secrets at rest
     queue, register, checklist
          |
   obop-storage (SQLite for development, PostgreSQL for deployments, D44)
```

`obop-http` carries the two public doors (`POST /hooks/in/{connection}` and `GET /oauth/callback`); everything else is the generated command API. The crates: `obop-connectors` (core), `connectors` (module), plus small, generic additions to `obop-commands` that any module can use (`add_guard`, `observe`, `register_read_only`, `model_fields`).

### 1.1 Models

| Model | Purpose | Who |
|---|---|---|
| `connector_provider` | The catalogue shipped in the binary, with the administrator's enablement merged in (read only) | users see the providers enabled for them, administrators all |
| `provider_policy` | Per provider: enabled, people may use it, legal basis and the text shown to people, agreement recorded and where, default policy, OAuth client id and secret | `connectors.admin` |
| `connection` | A link to one external system: provider, owner scope (`instance` or `user`), `state` (draft, connected, paused, error, revoked; governed, D48), non-secret `config`, `credentials` and `oauth_tokens` (class `secret`), `field_allowlist`, `policy`, caps, `retention_days` | owner; administrators see all, never credentials |
| `consent` | The recorded legal basis: subject kind, purpose, basis, note, the data classes and fields that may flow (computed by the server), `state` (active, withdrawn, superseded). Never edited or deleted | owner; administrators |
| `sync_run` | One run: direction, kind, times, status, record counts, error summary. No payload | owner; administrators |
| `data_flow_log` | Append-only: time, direction, model, record count, field names sent and denied, treatments, SHA-256 of the payload, outcome, reason. No payload | owner; administrators |
| `processing_register` | A generated read model over connections, providers and consents for the record of processing activities; exports as CSV | `connectors.admin` |
| `egress_policy` | The singleton of instance switches: master switch, EU-only, agreement required, host allow-list, default retention; also `checklist` and `purge` | `connectors.admin` |
| `privacy_preview` | Computed: one real record after the policy | `connectors.user` |

Groups: `connectors.user` (connect your own accounts), `connectors.admin` (administer the catalogue policy, see every connection's metadata, the register, the checklist). `platform.admin` holds both. A person who administers connectors holds both groups.

## 2. The policy model

### 2.1 Treatment by data class

Every outbound field has a data class (CONVENTIONS section 5) and a personal-data flag, both from the registry. The default treatment is a function of those two and nothing else:

| Class | Personal data | Default treatment | Can the owner change it? |
|---|---|---|---|
| `secret` | n/a | **deny** (never leaves) | No. A policy that tries is refused when saved |
| `confidential_hr`, `special_category`, `financial` | yes | **deny** | Yes, to any treatment, only with a class or field override **and** an impact-assessment acknowledgement for that class (at least 30 characters, who and when stamped by the server) **and** a provider whose maximum classes include it. An agent can only propose the change (approval) |
| `public_directory`, `internal` | yes | **pseudonymise** | Yes, per class or per field, to `allow`, `mask`, `redact` or `deny`. Raising above the default is a proposal for an agent |
| `none` or any class | no | **allow** | Yes |

Treatments: `allow` (as is), `pseudonymise` (a keyed HMAC-SHA256 pseudonym `psn_...`, stable per connection, different on every other connection, not reversible without the master key and, even with it, only by guessing the plain value), `mask` (a reversible token `TKN_...`; the value is kept sealed in the instance and restored only in an inbound response of the same connection), `redact` (the fixed marker `[redacted]`), `deny` (the field is left out). The provider's **maximum classes** are applied last and cannot be raised: a field whose class the provider may not receive is denied whatever the policy says.

Pseudonymised data is still personal data (GDPR Article 4(5)). That is why, by default, personal data goes only to a provider whose agreement is recorded.

### 2.2 The gate, step by step

`Egress::prepare(connection, model, rows)` refuses, with a reason that is also written to the log, unless all hold:

1. the instance's master switch is on (`egress_policy.flows_enabled`);
2. the provider can send in this direction;
3. the connection is `connected`;
4. an **active** consent record exists, and its recorded data classes cover the classes of the fields that would leave (widening the allowlist later without recording the basis again is refused);
5. the allowlist for the model is not empty (**minimisation: an empty allowlist sends nothing**) and at least one allowlisted field survives the policy;
6. if personal data would leave: the agreement with the provider is recorded (`require_dpa`, on by default) and, if the instance is EU-only, the provider is adequate;
7. the rate cap (flows per hour) and the volume cap (records per day) hold, and at most 1,000 rows per flow.

It then keeps only the allowlisted fields, treats each, replaces the record id by a keyed pseudonym (`ref`; the instance remembers which record it stands for, so inbound events that cite it can be matched), and writes one `data_flow_log` row. The rows it was given were read as the connection's owner, so the owner's permissions have already applied.

### 2.3 Privacy preview

`privacy_preview.render` runs the same plan on a real record without a gate, a log row, a token or a pseudonym map, so looking stores nothing. The wizard shows each field's class and treatment ("Sent as a pseudonym", "Never sent") while you pick fields, and the exact payload for a sample record.

### 2.4 Inbound

`Ingress::accept(connection, model, items)` applies steps 1 to 5 (the same class policy: a connector cannot write a class it could not send), then rejects an item that names a field that is not on the allowlist or does not exist (not silently trimmed), restores tokens this connection issued, and writes through the governed `<model>.create` and `<model>.update` as the **connector principal**: an agent-kind principal whose groups are the groups the owner holds (intersected with the user table when the owner is a known user, so a demoted or deleted owner narrows the connection) and whose scopes are the read, write and action groups of the one target model. It can never do more than its owner, and an action that needs approval (posting an invoice) comes back as a proposal and is counted in the run. A record the sender identifies by an external id is updated on the next delivery instead of duplicated. Where a record came from is remembered.

## 3. Transport

**One outbound client** (`net::Client`) for every connector and for OAuth:

- `https` only. Plain `http` is accepted only for a literal loopback host and only when the operator sets `OBOP_CONNECTORS_ALLOW_LOOPBACK=1` (tests, local development); the checklist flags it.
- No credentials in the URL. The host must be on the allow-list of the provider (for example `hooks.slack.com`) or, for a generic provider, the connection's `allowed_hosts` or the host of its URL, and on the operator's allow-list (`egress_policy.host_allowlist`) when there is one.
- The name is resolved **once per request**, **every** address is checked against the non-public ranges (this-network, private, loopback, carrier-grade NAT, link-local including the cloud metadata address, documentation, multicast, reserved; IPv6 unique-local, link-local, site-local, multicast, documentation, Teredo; IPv4-mapped, NAT64 and 6to4 forms judged by the embedded address), and the connection is made to exactly those addresses. One non-public answer among several refuses the request, and a name that answers differently the next time cannot redirect an established request (DNS rebinding).
- **Redirects are never followed**; a 3xx is an error. Timeouts, a 1 MiB request cap and a 2 MiB response cap. Header values with line breaks are refused. No URL query, header or body ever appears in an error or a log.

**Webhooks in** (`POST /hooks/in/{connection}`): a per-connection secret; the signature is checked on the raw body in constant time before the body is read as JSON, with the timestamp (five minutes either way) and a replay record in the database (an event id is accepted once; a request refused because the connection is paused does not use its id up). Schemes: Standard Webhooks (`webhook-id`, `webhook-timestamp`, `webhook-signature`, several signatures accepted so a secret can rotate), GitHub (`X-Hub-Signature-256`) and Stripe (`Stripe-Signature`). A bad signature, a stale timestamp, a missing header and a replay all answer the same `401`. Body cap 256 KiB; 240 requests per minute per connection address, counted before any signature work.

**Webhooks out** are signed with the same Standard Webhooks scheme, sent through the one client, and retried with backoff (5 s, 30 s, 5 min, 30 min, 2 h, with jitter) by an in-process queue; after the last retry the run is `dead_letter`. A retry first checks that the connection is still connected and its basis still active, so withdrawing consent stops queued deliveries.

**OAuth2** (authorization code with PKCE S256) is a generic framework. The administrator registers an application with the provider and enters the client id and secret under the provider policy (the secret is sealed; obop ships none). `connection.oauth_start` returns the authorization link; the redirect lands on `GET /oauth/callback` with a single-use `state` that is bound to the connection and to the person who started it; the verifier is stored sealed, the token endpoint must be on the provider's host list, the tokens are stored sealed, and the access token is refreshed before it expires. The flow is tested against a fake provider on loopback. No real provider is called in any test, and no shipped provider uses it yet (see the catalogue).

## 4. Secrets

`credentials`, `oauth_tokens` and the provider's OAuth client secret are class `secret`. They are

- **sealed at rest** with ChaCha20-Poly1305 under a key derived with HKDF-SHA256 from `OBOP_SECRET_KEY` (at least 32 characters), a fresh nonce per value and the purpose as associated data, so a sealed credential cannot be opened as a token. Without `OBOP_SECRET_KEY` a random key is generated and kept in the database next to the data: acceptable for evaluation, not for production, and the compliance checklist says so;
- **write-only**: the generic shaping of every response withholds a `secret` field from every caller, the field is named in `denied` and its value never appears; the connection shows only a hint (`signing_secret: set, ends cret` at most the last four characters, and nothing for a short value);
- **absent from logs and errors**: nothing logs a body, a header or a URL; the egress client and the OAuth code report the kind of failure only.

Per-connection pseudonym keys and the master-key subkeys come from the same HKDF. There is no key rotation yet (section 10); the secretbox is a small module behind two calls (`seal`, `open`) so a KMS or a shared helper can replace it.

## 5. People connecting their own accounts

A person with `connectors.user` sees the catalogue entries an administrator enabled for people (`provider_policy.enabled` and `users_may_use`, optionally limited to a group) and creates connections whose owner scope is forced to `user` and whose owner is forced to themselves. In the wizard they see the recipient (legal name, country, adequacy, whether the agreement is recorded), the purpose they type, the exact data classes that would flow, the legal basis (fixed by the administrator when one is set) and the text the administrator wrote for them. Confirming creates a `consent` record. **One click withdraws it** (`consent.withdraw`, in the connection's detail): the connection is revoked, its credentials, OAuth tokens, pseudonym maps and vault tokens are purged, queued deliveries are dropped, and nothing flows again. Only a new connection and a new basis can restart it. Administrators can pause or revoke any connection; they can see its metadata and never its credentials.

## 6. Agents and MCP

Everything above is a command of the one dispatcher, so MCP tools exist without any extra work (`connection__create`, `connection__connect`, `privacy_preview__render`, `processing_register__export_csv`, ...) and an agent token is held to its scopes. `connection.connect`, `pause`, `resume`, `retry` and `revoke`, `connection.sync`, `consent.withdraw`, `egress_policy.purge`, every `delete`, and any change to a connection's policy that exposes more than the defaults are **approval-required**: the agent gets `approval_required` and a `plan_id`, a human with the group approves, and the guards run again at approval. Credentials never appear in a tool result (the field is listed as withheld). An agent acting for a person owns what that person owns.

## 7. Retention, erasure, register

- **Retention.** `retention_days` per connection (default 90) applies to its flow log rows and runs. `obop connectors purge` (also run daily by `obop serve`, and available as `egress_policy.purge`, approval-required for agents) removes older rows and expired webhook ids and OAuth states.
- **Erasure.** When `party.erase` succeeds, the instance drops the pseudonym maps, vault tokens and external-id links that point at that party, in every connection, and writes an `erased` row to the data flow log. It does not recall what a recipient already received; telling recipients (Article 19) and erasing personal data copied into other models are the owner's job (section 10).
- **Register.** `processing_register` has one row per connection: recipient and country, purpose, legal basis and when it was recorded, categories of data subjects, categories of personal data with class and treatment, transfer basis, whether the agreement is recorded, retention, safeguards. `processing_register.export_csv` exports it; cells that a spreadsheet could run as a formula are quoted. It is a starting point for the Article 30 record, not a replacement.
- **Checklist.** `egress_policy.checklist` lists what is configured: key source, master switch, agreements, retention, bases, high-risk acknowledgements, fields sent in clear, providers without an established transfer basis, loopback, the public URL. Every item is a statement about configuration. None says the instance is compliant.

## 8. The provider catalogue and how to add one

| Provider | Status | Engine | Notes |
|---|---|---|---|
| `webhook_out` | **working** | webhook_out | any HTTPS endpoint, signed, minimised, on events or by hand |
| `webhook_in` | **working** | webhook_in | signed receiver into one model with a path mapping |
| `rest_api_pull` | **working** | rest_pull | periodic or manual pull of a JSON API, mapping, external ids |
| `csv_export` | **working** | csv_export | the same policy, a file for the person who downloads it |
| Slack incoming webhook | generic adapter | webhook_out | message body format; not tested against Slack |
| Stripe | generic adapter | webhook_out + Stripe-signed events in | form-encoded customer POST; also the card provider of the web shop (Checkout Sessions, signed webhook at `/hooks/pay/stripe`, refunds; D62); not tested against Stripe itself |
| GitHub webhook | generic adapter | webhook_in | `X-Hub-Signature-256`; tested with GitHub's published example |
| Zapier, Make, n8n | generic adapter | webhook_out | their catch hooks are HTTPS URLs; not tested against them |
| Google, Microsoft (calendar, contacts, read) | **definition only** | none | needs your own OAuth client; the OAuth flow is tested, no Google or Microsoft call exists |

`working` means implemented and tested end to end against a local counterpart. `generic_adapter` means the generic engine is configured for the service and is not tested against the service itself. `definition_only` means metadata, setup instructions and sub-processor facts, and nothing runs. Facts about a vendor (legal entity, country, adequacy, agreement links) that were not checked say "verify with the provider" and carry a note that links and names are starting points, not legal advice.

**To add a provider** (a generic-engine service takes one catalogue entry):

1. Add a `Provider` to `crates/kernel/obop-connectors/src/catalog.rs`: id, name, category, `maturity` (be honest), `engine`, `auth_kind`, `direction`, `max_classes` (the most it may ever receive; never `secret`), `purpose`, `description`, sub-processor facts (or "verify with the provider"), `allowed_hosts`, the `config` fields (mark credentials `secret`), `setup`, `webhook_scheme` for inbound, `body_format` for outbound.
2. If it needs an encoding or a signature scheme the engines do not have, add it to `runtime.rs` (`encode`) or `webhook.rs` with a test vector.
3. Add the entry to `maturity_is_stated_honestly` in the catalogue tests, and run `OBOP_UPDATE_SNAPSHOT=1 cargo test -p connectors the_website_snapshot` only if you changed the module's metadata (the catalogue itself is not part of the snapshot).
4. A provider with its own protocol is a new `Engine`; it takes a handler in `Runtime` and goes through `Egress::prepare` or `Ingress::accept` like the others. There is no other way to move data.

## 9. Compliance mapping

What the software does for each article, and what stays with the owner. Nothing here is legal advice.

| Article | What obop does | What remains the owner's job |
|---|---|---|
| **5** Principles | Purpose recorded on every consent record and shown in the register (purpose limitation); an empty allowlist sends nothing and only listed fields leave (minimisation); pseudonymisation by default; typed inbound validation; retention per connection and a purge; an append-only flow log (accountability); integrity and confidentiality through sealed credentials and the gate | Deciding the purposes and that they are compatible; accuracy of the data; choosing retention periods that fit the purpose; keeping the evidence |
| **6** Lawfulness | A flow cannot start without a recorded legal basis (consent, contract, legitimate interest, legal obligation); legitimate interest needs the balancing test written down; the administrator can fix the basis people must use | Choosing the basis and being able to defend it; the balancing test itself; special-category conditions under Article 9 (a class is withheld until acknowledged, the condition is yours) |
| **7** Conditions for consent | Consent is a record with who, when, purpose and the exact data classes; it is as easy to withdraw as to give (one click); withdrawal revokes the connection and purges what it held; a changed scope needs a new record | Freely given, specific, informed and unambiguous consent in your situation; the information shown in the text you write; not tying consent to a service where that is not allowed |
| **25** Data protection by design and by default | Defaults deny the sensitive classes, pseudonymise personal data, send nothing without an allowlist, and need an acknowledgement to relax; the preview shows the effect before anything is sent | Reviewing the defaults you relax and the fields you list; assessing whether the design suits your risk |
| **28** Processors | The catalogue records the recipient's legal name, country and agreement link per provider; personal data is not sent to a provider whose agreement is not recorded (default); the register lists recipients | Concluding the processing agreement, checking it, choosing processors, the sub-processor chain. The "recorded" flag is an attestation, not a check |
| **30** Records of processing | The processing register is generated from the connections and exports as CSV | Your complete Article 30 record: processing outside connectors, the controller's details, the data protection officer, the security description as a whole |
| **32** Security | Credentials sealed at rest and write-only; HMAC-signed webhooks with replay protection; one hardened egress client against SSRF; scoped connector principals; approval for agents; rate and volume caps; an audit log | Setting `OBOP_SECRET_KEY` and keeping it outside the database; backups; access control for the hosts; incident response; the rest of your security programme |

Not covered by this app: information duties toward data subjects (Articles 13 and 14), their rights requests beyond the erasure hook (Articles 15 to 22; see the spec for the subject-access and erasure work), impact assessments (Article 35; the acknowledgement records that one was done, it does not do it), international transfer mechanisms (Chapter V; the catalogue states what it knows and marks the rest "verify with the provider"), breach notification (Articles 33 and 34).

## 10. Known gaps and limits

- **No key rotation.** Changing `OBOP_SECRET_KEY` makes sealed values unreadable; there is no re-seal tool and no pseudonym-key rotation.
- **The retry queue is in memory.** A restart drops pending retries; the run stays `retrying` and the owner sends again by hand.
- **Erasure is scoped.** It drops the pseudonym maps, tokens and links of the erased `party` only. Personal data copied into other models (for example a lead's contact name) is erased by those models' owners; data already delivered to a recipient cannot be recalled.
- **The agreement flag is an attestation.** obop cannot know whether the document exists.
- **Rate counting scans the flow log** (bounded by retention); a high-volume deployment wants the typed-column schema manager first (D44).
- **Group snapshot.** A connection stores the owner's groups from the last time the owner changed it and narrows them to the user table at run time. A principal that is not in the user table (the fixed development tokens) keeps the snapshot.
- **The inbound door's flood limit is per process and in memory.**
- **The catalogue is small and mostly generic.** Two entries are definitions only, six are untested against the real service, and nothing was verified against a vendor's current terms.
- **SQLite is outside the compliance claims** (D44): the guarantees that depend on the database exist on PostgreSQL deployments only.
- **Inbound data subjects** are not informed by obop (Article 14); a connection that receives personal data from a third party leaves that to the owner.

## 11. Threat model

Each finding names the test that holds it. "Fixed" means the control exists and the named tests pass; "open" means a limit in section 10.

| # | Threat | Control | Tests | Status |
|---|---|---|---|---|
| T1 | A configured URL makes the server fetch an internal or metadata address (SSRF) | https only, host allow-lists, resolved addresses checked against non-public ranges, loopback only by operator switch | `net::tests::private_and_special_ranges_are_not_public`, `schemes_credentials_and_hosts_are_vetted`, `literal_addresses_in_odd_notations_are_judged_after_parsing`, `loopback_needs_the_operator_switch_and_only_allows_loopback`; module `connectors_cannot_touch_their_own_records_or_the_platforms_and_secrets_cannot_be_allowlisted` (URLs refused when saved) | fixed |
| T2 | DNS rebinding, or a mixed public and private answer | One resolution per request, all answers checked, connection pinned | `net::tests::dns_answers_are_all_checked_and_resolved_once` | fixed |
| T3 | A redirect leads to another host | Redirects are never followed | `net::tests::sends_and_reads_a_response_and_never_follows_redirects` | fixed |
| T4 | Oversized or malformed requests and responses, header injection | Size caps, timeouts, line breaks refused | `net::tests::size_caps_and_header_injection` | fixed |
| T5 | Credentials leak through an API response, MCP, the UI, a log or an error | `secret` class: withheld by the shaping, sealed at rest, hints of at most four characters, errors name no URL query or header | module `credentials_are_encrypted_at_rest_and_never_returned`; `obop` `connectors_tests::an_api_key_never_appears_in_a_response_a_log_row_or_the_database`, `agents_get_the_generated_tools_and_the_connect_family_needs_approval`; `net::tests::errors_never_contain_the_url_query_or_secrets`; `secretbox::tests::hints_reveal_at_most_four_characters` | fixed |
| T6 | A sealed value is moved or tampered with | AEAD with the purpose as associated data | `secretbox::tests::round_trip_and_purpose_binding`, `another_master_cannot_open` | fixed |
| T7 | A person reads or changes another person's connection, consent, runs or logs | Guards scope every command to the owner; a foreign record looks missing; filters cannot widen the scope | module `people_see_only_their_own_connections`; `obop` `the_command_api_scopes_records_to_their_owner_and_holds_agents_to_proposals`; e2e `people see only their own connections, and the administration pages are for administrators` | fixed |
| T8 | A connection does more than its owner may (privilege escalation) | The connector principal is agent-kind, limited to the owner's groups and the target model's groups; narrowed by the user table | module `a_connector_can_never_do_more_than_its_owner_and_approval_actions_stay_proposals` | fixed |
| T9 | Approval-required actions run unattended from a connector or an agent | Approval flag on actions and on raised policies; inbound changes that need approval become proposals | module `an_inbound_change_that_needs_approval_becomes_a_proposal`, `an_agent_may_only_propose_connecting_pausing_revoking_and_raising_the_policy` | fixed |
| T10 | A connection rewrites its own policy, consent or another connector's data | Connector models and the platform's are never allowlistable or writable by a connector | module `connectors_cannot_touch_their_own_records_or_the_platforms_and_secrets_cannot_be_allowlisted` | fixed |
| T11 | A sensitive or secret class leaves | Deny by default; raise needs override, stamped acknowledgement and provider maximum; the consent must cover the class | `policy::tests::every_class_times_every_treatment`, `provider_maximum_wins_over_everything`, `secret_cannot_be_raised`; `egress::tests::denied_classes_never_leave_even_when_allowlisted`, `high_risk_needs_an_override_an_acknowledgement_and_a_provider_that_allows_it`; module `high_risk_classes_need_a_raised_policy_an_acknowledgement_and_a_consent_that_covers_them`; e2e wizard test | fixed |
| T12 | Plain personal data on the wire or in the log | Pseudonyms by default, no raw ids, hash-only log | `egress::tests::minimisation_pseudonyms_and_the_log_without_payload`, `pseudonyms_are_stable_per_connection_and_differ_across_connections`; e2e wizard test (asserts the received payload) | fixed |
| T13 | Data flows after consent is withdrawn or the connection is paused | Active-consent check on every flow and every retry; revocation purges and drops the queue | `egress::tests::consent_withdrawal_stops_flows`; module `withdrawing_consent_revokes_the_connection_and_purges_credentials`, `a_failing_receiver_is_retried_then_dead_lettered_and_a_withdrawal_stops_the_retries`; e2e withdrawal test | fixed |
| T14 | Forged, replayed or stale inbound webhooks | Constant-time HMAC over the raw body, timestamp tolerance, persistent replay record, one answer for all failures | `webhook::tests::*` (vectors, tampering, tolerance, rotation, GitHub, Stripe, replays); module `a_signed_webhook_creates_records_through_the_governed_commands`; `obop` `the_webhook_door_checks_the_signature_the_age_and_the_size_before_anything_else` | fixed |
| T15 | Flood or oversized body at the public door | Body cap, per-connection request limit before signature work | same HTTP test; `a_flood_at_the_webhook_door_is_limited_before_any_signature_work` | fixed (per process) |
| T16 | OAuth code interception, CSRF on the redirect, token leakage | PKCE S256, single-use state bound to connection and person, sealed verifier and tokens, host-listed token endpoint | `oauth::tests::*`; `obop` `the_oauth_flow_runs_against_a_local_fake_provider_and_tokens_stay_sealed` | fixed |
| T17 | A formula in exported data runs in a spreadsheet | Leading `=`, `+`, `-`, `@` and tab are quoted | `util::tests::csv_cells_cannot_carry_formulas`, `register::tests::csv_cannot_carry_a_formula`; module `the_register_exports_csv_without_secrets_or_formulas_and_the_checklist_carries_the_disclaimer` | fixed |
| T18 | Erased personal data stays linkable through the connectors | Pseudonym maps, tokens and external-id links of the party are dropped and the erasure is logged | `egress::tests::erasure_and_revocation_drop_pseudonym_maps_and_tokens`; module `erasing_a_party_drops_the_pseudonym_maps_and_tokens_that_pointed_at_it` | fixed for the party's own maps; copies in other models and at recipients are open |
| T19 | Logs and runs kept forever | Retention per connection, purge command and daily purge | module `retention_removes_old_flow_logs_and_runs`; `obop` `the_purge_command_applies_retention_to_a_real_database` | fixed |
| T20 | Uncontrolled volume or rate | Per-connection caps, a row cap per flow | `egress::tests::rate_and_volume_caps` | fixed |
| T21 | An administrator disables the safety valves unnoticed | The checklist lists relaxed settings; high-risk releases and clear-text fields are reported; raising is approval-required for agents | `checklist::tests::checklist_flags_what_is_missing_and_always_carries_the_disclaimer` | fixed (reporting) |
| T22 | The master key is lost, leaked or never set | Environment key preferred; the generated key is flagged; no rotation | `secretbox::tests::generated_key_is_created_once_and_reused`, checklist item `secret_key` | open (rotation) |
| T23 | Retry queue lost on restart | Documented | none | open |

## 12. Configuration

| Setting | Meaning |
|---|---|
| `OBOP_SECRET_KEY` | Master secret (at least 32 characters) for sealing credentials, tokens and pseudonym keys |
| `OBOP_PUBLIC_URL` | The instance's public address: webhook addresses and the OAuth redirect (`<url>/oauth/callback`) |
| `OBOP_CONNECTORS_ALLOW_LOOPBACK=1` | Allow plain http to loopback targets; tests and local development only |
| `egress_policy` | `flows_enabled`, `eu_only`, `require_dpa`, `host_allowlist`, `default_retention_days` (admin) |
| `provider_policy` | per provider: enabled, `users_may_use`, `allowed_group`, `legal_basis`, `legal_basis_text`, `dpa_recorded`, `dpa_reference`, `default_policy`, OAuth client |
| CLI | `obop connectors purge`; every command is also `obop call <model> <command> k=v` |
