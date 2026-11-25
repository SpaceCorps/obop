# 21 - Integrations, Interoperability and API Design for obop

Research date: 2026-10-01. Status tags used throughout: **[V]** verified against a live source in this session (URL given), **[I]** inference or design judgement, **[R]** recalled from training and not re-verified (treat as a guess until checked).

## Scope & method

Sources: official vendor docs and changelogs (Odoo, Frappe, Stripe, Shopify, Xero, Microsoft, Google, Amazon, Adyen, Mollie, PayPal), standards bodies (OpenAPI, MCP, IETF datatracker, GraphQL, JSON:API, OASIS, EU Commission), GitHub repos and raw source (Odoo documentation and `base_import`), and the crates.io JSON API for every Rust crate quoted (versions and dates are as of 2026-09-30/10-01). No search engine was available, so discovery was by direct fetches (about 100). Several vendor portals are JS-rendered or blocked (eBay, Intuit, Avalara, Vertex, Adyen libraries page, Plaid product pages), so those areas are marked [R] or listed under gaps. Pricing/sentiment from secondary sources is not used.

## 1. Executive summary

1. **Odoo is pivoting its external API right now.** JSON-2 (`/json/2/<model>/<method>`, bearer API key) arrived in 19.0; XML-RPC/JSON-RPC are deprecated and scheduled for removal in Odoo 22 (fall 2028; Online 21.1, winter 2027). Odoo 20 (released September 2026) ships a built-in MCP endpoint. External API access on Odoo Online is still gated to the "Custom" plan. obop should ship a free, complete, documented API and MCP server from day one. [V]
2. **Protocol baseline for obop:** REST/JSON described by OpenAPI (3.1 now, 3.2.1 is current), RFC 9457 errors, Stripe-style idempotency, Standard Webhooks signing, MCP 2026-07-28 (stateless) as a first-class second surface. GraphQL, OData, gRPC and JSON:API are optional add-ons, not the core.
3. **Rust has no official SDK for most providers.** Stripe, Adyen, Mollie, PayPal, Plaid, Shopify, Avalara, EasyPost etc. have either community crates (some excellent, some abandoned) or nothing. Plan on generating thin typed clients from vendors' OpenAPI specs behind obop-owned traits, not on depending on vendor crates.
4. **Connector architecture:** a tiered model. Native in-process Rust for protocol-level connectors (IMAP/SMTP/CalDAV, bank-file formats, core payment adapters), sandboxed Wasm components (wasmtime, WASI 0.3) for HTTP/JSON SaaS connectors and third-party code, and remote apps (OAuth + webhooks + MCP) for everything else. Wasmtime ships a major version roughly monthly, so the host ABI must be insulated by obop's own WIT world.
5. **Several hard deadlines land now:** Microsoft starts default-disabling EWS in Exchange Online tenant by tenant from 2026-10-01 (admins can re-enable until EWS is permanently shut down in April 2027); SMTP AUTH basic auth is disabled by default for existing tenants at end of December 2026; Shopify REST Admin is legacy; Xero moved to metered pricing on 2026-03-02; EU Data Act bans switching/egress charges for SaaS from 2027-01-12.
6. **Portability is now partly a legal duty, not only a trust feature** (if obop runs a hosted service), and the Odoo import engine (External IDs, dry-run via savepoint, saved mappings) is the design to beat for migration.

## 2. Reference systems

### 2.1 Odoo external API [V unless tagged]

| Aspect | Facts |
|---|---|
| JSON-2 | "New in version 19.0". `POST /json/2/<model>/<method>`; `Authorization: bearer <API key>`; optional `X-Odoo-Database`; body = JSON of `ids`, `context` and named args only (no positional args); success 200 with JSON return value; errors 4xx/5xx with `name, message, arguments, context, debug`. https://raw.githubusercontent.com/odoo/documentation/19.0/content/developer/reference/external_api.rst |
| Transactions | Every JSON-2 call is its own SQL transaction; no multi-call transaction. The docs tell integrators to write a single server method that does all related work. Same source. |
| API keys | Per user; expiry mandatory (19.0, 20.0 and master all state "not possible to create keys that last for more than three months"; programmatic generation is additionally validated against a maximum duration set by the user's roles); shown once; 10 keys/user programmatically by default (`base.programmatic_api_keys_limit`); programmatic `res.users.apikeys.generate/revoke`; "bot users" recommended; scopes (`rpc` in external_api.rst; the `MCP` scope is documented only in mcp_server.rst, see the MCP row). https://raw.githubusercontent.com/odoo/documentation/master/content/developer/reference/external_api.rst |
| Gating | "Access to data via the external API is only available on Custom Odoo pricing plans" (Odoo Online). Same 19.0 source. |
| Legacy RPC | `/xmlrpc`, `/xmlrpc/2`, `/jsonrpc` deprecated since 19.0; common+object services "scheduled for removal in Odoo 22 (fall 2028) and Online 21.1 (winter 2027)"; db service "was removed in Odoo 20 (fall 2026) and Online 19.1 (winter 2025)" (that sentence is in the master/20.0 docs, not the 19.0 page). https://raw.githubusercontent.com/odoo/documentation/19.0/content/developer/reference/external_rpc_api.rst , https://raw.githubusercontent.com/odoo/documentation/master/content/developer/reference/external_rpc_api.rst |
| Docs | Per-database `/doc` page lists models/fields/methods; the "Dynamic Documentation" section is still a stub ("Under construction"); no OpenAPI document is mentioned. |
| Odoo 20 | Released September 2026 (https://www.odoo.com/page/release-notes). Release notes: MCP connection, AI agents, Syncfy (LatAm) and Basiq (Australia) bank sync, PISP bill payment, Sendcloud labels, Google/Outlook calendar sync rework, Dun & Bradstreet replacing Clearbit (https://www.odoo.com/odoo-20-release-notes). |
| MCP | `/mcp` endpoint on the database; auth = static API key with MCP scope; Odoo *server actions* flagged "Available in MCP" become tools; five defaults (Get Fields, Get Models, Retrieve initial context, Search, Read group); clients bridge via `npx mcp-remote` (Odoo's own instruction; obop never uses npx, so obop's docs would show `vp dlx mcp-remote` or omit the bridge, owner constraint 2026-10-01). https://raw.githubusercontent.com/odoo/documentation/master/content/applications/productivity/ai/mcp_server.rst |
| Webhooks | Automation rules can receive a webhook (External trigger) and send one ("Send Webhook Notification" POSTs selected fields). The docs I could read say nothing about signing or retries. https://raw.githubusercontent.com/odoo/documentation/master/content/applications/studio/automated_actions.rst |
| License | Community is LGPL v3 (https://github.com/odoo/odoo/blob/19.0/LICENSE). |
| Payment connectors | 21 online providers plus the Demo provider (the 19.0, 20.0 and master tables all list 21): Adyen, Amazon Payment Services, AsiaPay, Authorize.Net, Buckaroo, DPO Pay, ECPay, Flutterwave, Iyzico, Mercado Pago, Mollie, Nuvei, Paymob, PayPal, PayU, Razorpay, Redsys, Stripe, Toss Payments, Worldline, Xendit. Tokenization: Adyen, Authorize.Net, Flutterwave, Mercado Pago, Razorpay, Stripe, Worldline, Xendit. Refunds: Adyen, Razorpay, Stripe (full+partial), Authorize.Net (full). https://raw.githubusercontent.com/odoo/documentation/19.0/content/applications/finance/payment_providers.rst |

Lessons: (a) RPC-style `model/method` access is powerful but gives no atomic multi-step writes; (b) key-per-bot-user with the user's ACLs is a sound security model; (c) Odoo gave ~3 years' notice for XML-RPC removal, a good benchmark for obop's own deprecation policy; (d) gating API access by plan is the opposite of a trust feature.

### 2.2 Frappe / ERPNext [V]

REST at `/api/resource/:doctype[/:name]` (CRUD) and `/api/method/<dotted.path>` (whitelisted functions); auth by `Authorization: token api_key:api_secret`, OAuth bearer, or session cookie; list defaults to 20 rows with `fields`, `filters`, `or_filters`, `order_by`, `limit_start`, `limit_page_length`; errors carry `exc_type`/`exc`. https://docs.frappe.io/framework/user/en/api/rest. Source shows two mounted route sets, `/api` (v1 default) and `/api/v2` (https://raw.githubusercontent.com/frappe/frappe/develop/frappe/api/__init__.py). I found no documented rate limiting on the REST page. ERPNext is GPL-3.0, about 39.7k GitHub stars (https://github.com/frappe/erpnext). Lesson: resource-oriented CRUD generated from the metadata model is what integrators find easiest; Frappe's explicit v2 shows versioning pain even in metadata-driven systems.

## 3. API standards: what to expose

| Standard | State today [V] | Rust support | Verdict for obop |
|---|---|---|---|
| OpenAPI | 3.2.1 dated 2026-09-10 (adds QUERY method, `additionalOperations`, `$self`, `in: querystring`, streaming/SSE media types, server `name`). https://spec.openapis.org/oas/latest.html | `utoipa` 6.0.0 (2026-09-22, MIT/Apache, advertises OpenAPI 3.1, axum bindings); `aide` 0.15.1 stable, 0.16 alpha | **Core.** Emit 3.1 today, track 3.2. Generated from the model registry, not hand-written. |
| REST/JSON | de facto; errors via RFC 9457 `application/problem+json` (obsoletes RFC 7807) https://www.rfc-editor.org/rfc/rfc9457 | `axum` 0.8.9, `reqwest` 0.13.5 | **Core.** |
| MCP | Current spec **2026-07-28**: stateless (no `initialize`, no `Mcp-Session-Id`), per-request `_meta` protocol version, mandatory `server/discover`, tasks moved to an extension, multi-round-trip requests replace server-initiated sampling/elicitation, Roots/Sampling/Logging and HTTP+SSE deprecated, Dynamic Client Registration deprecated in favour of Client ID Metadata Documents. https://modelcontextprotocol.io/specification/2026-07-28/changelog | official `rmcp` **3.5.0** (2026-09-28, Apache-2.0, 30.9M downloads), implements 2026-07-28 incl. stateless Streamable HTTP | **Core second surface.** |
| GraphQL | Spec edition September 2025 (2025-09-03); working draft 2026-09-28. https://spec.graphql.org/ | `async-graphql` stable 7.2.1 (8.0.0-rc.5 pending), `cynic` 3.14.0 (client, MPL-2.0) | Optional (COULD), read-mostly. Need cost limiting (cf. Shopify). |
| JSON:API | v1.1 stable, finalised 2022-09-30. https://jsonapi.org/ | `jsonapi` 0.7.0 last released 2020, dormant | Skip. Verbose; no Rust ecosystem. |
| OData | 4.01 OASIS Standard, 2020-04-23 (no 4.02 seen). https://docs.oasis-open.org/odata/odata/v4.01/odata-v4.01-part1-protocol.html | `odata-params` 0.4.0 (parsers only) | Optional read-only `$filter/$select/$top` subset for BI tools (Excel/Power BI consume OData feeds [R]). Borrow `$batch` changeset semantics. |
| gRPC | mature | `tonic` 0.14.6 (2026-05-07, MIT) | Internal/high-throughput only; awkward for a Vite+ browser frontend. |
| A2A (agent-to-agent) | Linux Foundation project; v1.0 referenced; official Rust SDK listed at `a2aproject/a2a-rs`, published on crates.io as `a2a-lf` 0.4.1 and `a2a-client-lf` 0.2.7 (both 2026-09-30, Apache-2.0); the crates.io crate literally named `a2a-rs` (0.10.0) belongs to an unrelated community repo (emillindfors/a2a-rs). https://a2a-protocol.org/latest/ , https://github.com/a2aproject/a2a-rs | yes | COULD; watch. |
| Agentic Commerce Protocol | Apache-2.0, OpenAI+Stripe maintainers, beta, latest 2026-04-17 (cart, feed, orders, auth, MCP). https://github.com/agentic-commerce-protocol/agentic-commerce-protocol | types crate `agentic-commerce-protocol` 0.2.0 (Svix, published 2025-10-02, so it predates the 2026 spec versions) | COULD for e-commerce module. |
| OAuth 2.1 | `draft-ietf-oauth-v2-1-16`, 2026-09-03, WG milestone to IESG Dec 2026; PKCE S256 mandatory, no implicit/password grants, exact redirect match, sender-constrained tokens (DPoP/mTLS) recommended. https://datatracker.ietf.org/doc/draft-ietf-oauth-v2-1/ | `oauth2` 5.0.0, `openidconnect` 4.0.1 (clients) | Build to it. |
| RateLimit headers | `draft-ietf-httpapi-ratelimit-headers-11` (2026-05-23, expires 2026-11-24 unless renewed), still a draft, review of -10 "Not ready". https://datatracker.ietf.org/doc/draft-ietf-httpapi-ratelimit-headers/ | - | Emit as additive headers; always also `Retry-After`. |
| Idempotency-Key | `draft-ietf-httpapi-idempotency-key-header-07` (2025-10-15), **expired**, no RFC. https://datatracker.ietf.org/doc/draft-ietf-httpapi-idempotency-key-header/ | - | Follow Stripe semantics instead (section 5). |
| CloudEvents | v1.0.2 latest. https://github.com/cloudevents/spec/releases | - | Optional payload format (COULD). |

**Design conclusion [I]:** one resource/action REST API, plus `POST /$batch` with an `atomic: true` option (fixing Odoo's single-call transaction limit, borrowing OData changesets), plus MCP tools auto-derived from the same model registry and permissions. Everything in the registry (fields, relations, actions, access rules) generates OpenAPI, MCP tool schemas, webhook event schemas and import/export mappings from one source of truth; that is the structural advantage of "one shared data model".

## 4. Webhooks, events and delivery guarantees

### 4.1 What providers actually guarantee [V]

| Provider | Retry | Ordering / duplicates | Authenticity | Notes |
|---|---|---|---|---|
| Stripe | Live: up to 3 days, exponential backoff; sandbox: 3 tries over a few hours; manual resend 15 days (Dashboard) / 30 days (CLI) | No ordering; duplicates possible; dedupe by event ID (and object ID+type for twin events) | `Stripe-Signature: t=..,v1=..`, HMAC-SHA256 over `timestamp.body`, default 5-min tolerance, secret roll with up to 24h overlap | 16 endpoints max; 3xx = failure; TLS 1.2/1.3; thin vs snapshot events; event payload version pinned per destination (`snapshot_api_version`). https://docs.stripe.com/webhooks , https://docs.stripe.com/upgrades |
| Shopify | (count not confirmed on page) | No order guarantee; dedupe with `X-Shopify-Webhook-Id`; order by `X-Shopify-Triggered-At` | `X-Shopify-Hmac-Sha256` | Delivery to HTTPS, Google Pub/Sub, or EventBridge; docs explicitly say to run periodic reconciliation. https://shopify.dev/docs/apps/build/webhooks |
| Mollie | 10 attempts over about 26h (immediate, 1,2,4,8,16,29 min, 1h,2h,22h); 15 s timeout | Body is only `id`; fetch the resource | No signature on classic webhooks; authenticity = authenticated re-fetch; "next-gen webhooks" in beta | https://docs.mollie.com/reference/webhooks |
| PayPal | up to 25 attempts over 3 days; then Failed, manual resend | - | Cert/CRC32 check or call PayPal's verify endpoint | 10 webhook URLs per app. https://developer.paypal.com/api/rest/webhooks/ |
| Adyen | (not on page read) respond 2xx | - | HMAC-SHA256 base64, hex key; signature in `additionalData.hmacSignature` (standard) or `hmacsignature` header (others) | https://docs.adyen.com/development-resources/webhooks/secure-webhooks/verify-hmac-signatures/ |
| Microsoft Graph | subscription-based webhooks, Event Hubs, Event Grid | - | validation token + `clientState` [R] | Max 1,000 active subscriptions per mailbox across all apps. https://learn.microsoft.com/en-us/graph/api/resources/change-notifications-api-overview |
| Google Calendar | no auto-renew of channels; expire and must be re-created | Notifications have **no body**; "not 100% reliable" | channel `token` echoed back | Must re-sync after each ping. https://developers.google.com/workspace/calendar/api/guides/push |

Takeaway: **at-least-once, unordered, possibly late or lost** is the universal contract. Every inbound integration therefore needs (1) raw-body signature verification, (2) an inbox table keyed by provider event ID, (3) fetch-current-state instead of trusting payload state (Mollie's model is the safest), and (4) a scheduled reconciliation sweep (Shopify's explicit advice).

### 4.2 Standard Webhooks for obop's own events [V]

The Standard Webhooks spec (https://raw.githubusercontent.com/standard-webhooks/standard-webhooks/main/spec/standard-webhooks.md) defines headers `webhook-id` (stable across retries), `webhook-timestamp` (per attempt), `webhook-signature` (space-separated list, enabling zero-downtime rotation); signed content `id.timestamp.body`; HMAC-SHA256 secrets as `whsec_` base64 (24-64 bytes, prefix `v1,`) or ed25519 (`whsk_`/`whpk_`, prefix `v1a,`); retry over days with jitter (example 5s, 5m, 30m, 2h, 5h, 10h, 14h, 20h, 24h); 2xx success, 3xx failure, 410 disables the endpoint, honour `Retry-After`, 15-30 s timeout; event types dotted lowercase; payload `type`, `timestamp`, `data`; keep under ~20 kB; SSRF protection via a filtering egress proxy; steering committee includes Zapier, Twilio, Svix, Kong, ngrok. Rust: `standardwebhooks` 1.0.1 (2024-03) and `svix` 2.6.1 (2026-09-30, client+verification). Adopt it wholesale; it is also what Zapier-class consumers increasingly expect.

Outbound design points [I]: thin payload default (`id`, `type`, `data.object_id`, `api_version`) with opt-in snapshot; per-endpoint pinned event schema version (Stripe lesson: never re-render old events); delivery log with replay for 30 days; auto-disable after sustained failure with notification via another channel; outbox table written in the same DB transaction as the business change; egress through an SSRF-filtering proxy.

## 5. Idempotency, retry, rate limiting

- **Idempotency (Stripe semantics) [V]:** `Idempotency-Key` on all POST; up to 255 chars; server stores status+body of the first execution (including 500s); keys may be pruned after at least 24 h; parameter mismatch on replay is an error; validation failures and concurrent-conflict responses are not stored. https://docs.stripe.com/api/idempotent_requests. The IETF draft is expired, so obop defines this contract itself in its API docs.
- **Outbound calls to providers:** always send the provider's idempotency key derived from an obop operation ID (Stripe, Adyen `Idempotency-Key` [R]); store the operation before the call (outbox); retry only on network error/5xx/429 with jittered backoff; never auto-retry non-idempotent legacy endpoints.
- **Inbound rate limiting:** token bucket per (API key, company) plus a global per-tenant cap; `governor` 0.10.4 (MIT) and `tower_governor` 0.8.0 for axum. Emit `RateLimit`/`RateLimit-Policy` (draft-11) and `Retry-After`. Distinguish request-count from cost-based limits for expensive endpoints.
- **Provider limits obop must respect [V]:** Shopify GraphQL is cost-based (restore rate by plan: 100 points/s Standard, 200 Advanced, 1,000 Plus, 2,000 Commerce Components; a single query may not exceed 1,000 points, error `MAX_COST_EXCEEDED`; input arrays max 250; the 1,000-point bucket / 50 points/s figures are only from a sample response; a 25,000-object pagination cap is not stated on either page (unverified)) https://shopify.dev/docs/apps/build/apis/graphql-admin/rate-limits , https://shopify.dev/docs/api/admin-graphql/latest ; Xero daily limits 1,000/org (Starter) or 5,000/org (paid tiers) plus egress metering https://developer.xero.com/pricing ; GoCardless Bank Account Data: banks may allow as few as 4 calls/day/account, 90-day access, 24 months history https://docs.gocardless.com/bank-account-data/overview ; Microsoft Graph 1,000 subscriptions/mailbox. Build an adaptive per-connector limiter that reads provider response headers and persists state.
- **Jobs:** `apalis` 0.7.4 stable (1.0.0-rc.10) or a Postgres `SKIP LOCKED` queue; all connector work runs as durable jobs with checkpointed cursors [I].
- **Errors:** RFC 9457 everywhere, with stable machine codes and a `retryable` extension member.

## 6. Provider survey and Rust availability

### 6.1 Payments

| Provider | Facts | Rust |
|---|---|---|
| Stripe | Latest named API version in current docs: `2026-09-30.endive` (plus `2026-09-30.preview` for v2 APIs). Clients should pin `Stripe-Version` per request. v2 "event destinations" API, thin events. https://docs.stripe.com/upgrades | No official SDK (Stripe's server-side SDK list is Ruby, Python, Go, Java, Node, PHP, .NET; https://docs.stripe.com/sdks). Community `async-stripe` **1.0.0-rc.9** (2026-09-11; stable 0.41.0; MIT/Apache; generated weekly from Stripe's OpenAPI; modular crates, separate `stripe-webhook` crate). https://github.com/arlyon/async-stripe |
| Adyen | Validators in Java, PHP, C#, JS, Ruby, Python; HMAC details above | `adyen` 0.1.24 (third-act, 19.7k downloads, 2026-05-18): unofficial. Generate from Adyen's OpenAPI [R]. |
| Mollie | Payment-ID-only webhooks | `mollie-rs` 0.8.1 (2026-09-03, 19k downloads), community. |
| PayPal | 25-attempt webhook retry; verification via API or certificate | Only community crates, all stale or alpha: `paypal-rs` 0.2.6 (2024-06-03, 69k downloads, github.com/edg-l/paypal-rs), `paypal-rust` 0.0.9-alpha (2023-05-30), `paypal` 0.2.0 (2019). Write a thin client. |
| Local methods | Odoo 20 docs list 21 online providers; Redsys, iDEAL/Wero (via Mollie/Adyen), SEPA Direct Debit, PIX, UPI (Razorpay) etc. arrive through PSPs | Model payments as `PaymentProvider` trait with `Transaction`, `Token`, `Refund`, `Payout`, `Dispute` entities; mirror Odoo's provider+token design [I] |

Recommendation: the payment core is the one place where a **first-party Rust adapter** is worth its cost (Stripe first, then Adyen, Mollie, PayPal). Use `async-stripe` initially behind the trait, pinned to an exact version (it is a pre-1.0 RC), with a fallback plan to generate from `stripe/openapi` (https://github.com/stripe/openapi, linked from https://docs.stripe.com/sdks).

### 6.2 Banks / open banking

| Aggregator | Facts | Rust |
|---|---|---|
| GoCardless Bank Account Data (ex-Nordigen) | EEA PSD2 AISP; details+balances+transactions; 90-day continuous access, 24 months history; strict per-bank rate limits; "verification" only for paying customers; I could not confirm whether new sign-ups are open or what it costs | `gocardless-bankaccountdata` 2.0.7 (OpenAPI client, 2026-01-12), `gocardless-unofficial` 0.2.0 |
| Enable Banking | Finnish, FIN-FSA registered AISP; "2,700+ banks across 30 European countries" (page is inconsistent: 2,500/29 elsewhere); you bring your own PIS licence; no public pricing; says adapting to PSD3 and FiDA. https://enablebanking.com/ | none found |
| Plaid | JSON over HTTPS POST, `PLAID-CLIENT-ID`/`PLAID-SECRET`, OpenAPI spec in `plaid-openapi` repo https://plaid.com/docs/api/ | `plaid` 9.0.1 (libninja-generated, 2025-01-21) |
| TrueLayer | - | `truelayer-signing` 0.3.1 (official org); `truelayer-rust` 0.1.0 is a PoC, do not use |
| Odoo reference | Odoo 20 added Syncfy and Basiq; earlier used Plaid, Yodlee, others [R] | - |

Regulatory [V, with caveat]: PSD3/PSR provisional political agreement 2025-11-27; most provisions apply 18 months after entry into force, payee-name verification 24 months; the Wikipedia summary (edited 2026-07-30) says formal adoption was still pending in May 2026, so the Wikipedia-based status is stale. The European Parliament procedure file for the Payment Services Regulation (2023/0210(COD)) shows Parliament's first reading on 2024-04-23, committee approval of the agreed text on 2026-05-05, current stage "Awaiting Council's 1st reading position", and an indicative plenary date of 2026-12-14, so formal adoption and Official Journal publication had not happened as of 2026-10-01; PSD3 (the separate directive file) was not checked (https://oeil.europarl.europa.eu/oeil/en/procedure-file?reference=2023/0210(COD)). US CFPB 1033 status could not be fetched [gap].

Design [I]: define `BankFeed` as a trait with three implementations: (1) file import (CAMT.053, MT940, OFX, QIF, CSV: works everywhere, zero licence), (2) aggregator connectors (bring-your-own credentials, as self-hosters cannot share one AISP licence), (3) optional hosted broker run by the obop project. Keep reconciliation rules independent of the feed source.

### 6.3 Marketplaces and commerce

| Platform | Current facts [V] | Rust |
|---|---|---|
| Shopify | Quarterly date versions; `2026-07` latest stable, `2026-10` release candidate (scheduled 2026-10-01); each stable version supported at least 12 months, 9-month overlap; unsupported versions "fall forward" to the oldest supported one; REST Admin is "legacy as of October 1, 2024"; since 2025-04-01 new public apps must be GraphQL-only; webhooks via HTTPS/Pub/Sub/EventBridge; app revenue share 0% on first $1M gross (counted from 2025-01-01), 15% after, $19 one-time registration, plus a separate 2.9% processing fee; the 0% tier does not apply to developers with $20M+ prior-year App Store revenue or $100M+ gross company revenue (they pay 15% on everything). https://shopify.dev/docs/api/usage/versioning , https://shopify.dev/docs/api/admin-rest , https://shopify.dev/docs/apps/launch/distribution/revenue-share | `shopify_api` 0.10.0 (community, 46k downloads); `shopify_function` 2.2.0 and `shopify_function_wasm_api` 0.3.1 (Shopify's own Wasm-based Functions); GraphQL via `cynic` 3.14.0 |
| Amazon SP-API | Dated release notes: Orders v0 deprecated, replaced by Orders v2026-01-01; `BuyerEmail` dropped for FBA orders (2026-01-28); notification subscriptions gain CEL `filterExpression` (2026-05-27); new notification types 2026-09-30; roles gate PII. I found no developer-fee item in the release notes (reports of API fees are unconfirmed [R]). https://developer-docs.amazon/sp-api/docs/sp-api-release-notes (the old developer-docs.amazon.com host now 301-redirects here) | community crates exist but are unvetted: `amazon-spapi` 2.0.4 (2026-08-19), `amazon-sp-api` 0.1.16 (2025-04-16); alternatively generate from Amazon's OpenAPI models [R] |
| eBay | Could not fetch (blocked). [R] REST Sell APIs with OAuth and an application growth check; legacy Trading API being phased out. | none checked |
| Etsy | Open API v3: Seller (own shop only, auto-approved), Personal (deeper review) and Commercial tiers; Commercial Access requires an approved Personal app first and is manually reviewed; `buyer_email` needs separate approval. https://developers.etsy.com/documentation/ | none checked |
| WooCommerce | REST `v3` (WC 3.5+); keys over HTTPS Basic, OAuth 1.0a over HTTP; `/batch` endpoints; 10/page default with `X-WP-Total`; official clients JS/PHP/Python/Ruby. https://woocommerce.github.io/woocommerce-rest-api-docs/ | `rust-woocommerce` 0.1.3 (tiny) |

Observations: every major marketplace moves toward **versioned, deprecating APIs with 12-month windows, role/approval gating of personal data, and reduced buyer PII**. obop's marketplace connectors must store the API version per connection, run contract tests against each new version in CI, and have a "field unavailable" model for PII that was withheld.

### 6.4 Shipping carriers

I could only verify FedEx's portal (sandbox needs no account; production requires a FedEx account linked to the developer organisation; Advanced Integrated Visibility is a paid webhook product for US accounts; no SOAP retirement date on the page) https://developer.fedex.com/api/en-us/home.html. UPS, EasyPost and Shippo pages were unreadable or returned no substance. Odoo's reference list [R, could not fetch the page]: DHL, FedEx, UPS, USPS, bpost, EasyPost, Sendcloud, Starshipit; Odoo 20 routes Mondial Relay through Sendcloud. No Rust crate found for EasyPost or Sendcloud (crates.io searches returned nothing relevant); Shippo has only a stale generated client, `shippo` 0.1.29 (2021-07-20, oxidecomputer/cio). Design [I]: a `CarrierProvider` trait (rate, label, void, track, manifest, pickup) with aggregator-first implementations (EasyPost/Shippo/Sendcloud equivalents) and direct UPS/FedEx/DHL later, since carrier onboarding is per-account credentials anyway.

### 6.5 Email and calendar

- **Microsoft 365 [V]:** Basic auth is already disabled for IMAP, POP, EWS, EAS etc.; OAuth 2.0 is supported for IMAP/POP/SMTP AUTH (https://learn.microsoft.com/en-us/exchange/clients-and-mobile-in-exchange-online/deprecation-of-basic-authentication-exchange-online). **EWS requests from non-Microsoft apps start being blocked 2026-10-01** (https://techcommunity.microsoft.com/blog/exchange/retirement-of-exchange-web-services-in-exchange-online/3924440), but the later plan is phased and admin-controllable: from 2026-10-01 tenants still on `EWSEnabled=Null` are flipped to `False` as the change rolls out (admins can set it back to `Null`, or to `True` with an AppID Allow List, at the cost of a service interruption), and **EWS is permanently disabled from 2027-04-01 with no exceptions** (https://techcommunity.microsoft.com/blog/exchange/exchange-online-ews-your-time-is-almost-up/4492361 , https://learn.microsoft.com/en-us/exchange/clients-and-mobile-in-exchange-online/deprecation-of-ews-exchange-online). Exchange Server on-premises EWS is unaffected. **SMTP AUTH Basic:** unchanged until December 2026; disabled by default for existing tenants at end of December 2026; unavailable by default for new tenants after that; final removal date to be announced in H2 2027 (https://techcommunity.microsoft.com/blog/exchange/updated-exchange-online-smtp-auth-basic-authentication-deprecation-timeline/4489835). Implication: Graph (mail, calendar, delta queries, change notifications) is the only durable Microsoft path; IMAP/SMTP with XOAUTH2 is a fallback.
- **Gmail [V]:** `gmail.readonly`, `.modify`, `.compose`, `.insert`, `.metadata`, `.settings.*` and `https://mail.google.com/` are **restricted** scopes; `gmail.send` is sensitive; restricted scopes require restricted-scope verification and a security assessment if data is stored on servers (https://developers.google.com/workspace/gmail/api/auth/scopes). Consequence: every self-hosted obop would need its own Google Cloud project or a central verified broker [I].
- **Calendar sync [V]:** Google push channels expire, carry no body, and are "not 100% reliable", so sync tokens/polling are mandatory; Odoo 20 improved Google/Outlook calendar sync and supports several Google calendars.
- **CalDAV/CardDAV:** `libdav` 0.11.0 (ISC, 2026-09-05, ~48k downloads) is the most used live Rust client; `fast-dav-rs` 0.17.0 (2026-09-10, ~3k downloads) is a newer small alternative, and `mailrs-dav` 1.1.2 (2026-08-04) offers server-side CalDAV/CardDAV handlers.
- **Mail stack crates:** `lettre` 0.11.23 (MIT, 2026-08-03), `async-imap` 0.12.0 (2026-09-30), `imap-proto` 0.17.0, `mailparse` 0.17.0; `google-gmail1` 7.0.0+20251215 (MIT, generated); `graph-rs-sdk` 3.0.1 (MIT, 2025-04-20, aging).
- Design [I]: inbound mail via IMAP IDLE / Graph delta / Gmail watch into a `mail.message` model with thread headers (Message-ID, In-Reply-To, References) as the chatter-equivalent; outbound via SMTP relay, provider API, or user's own OAuth mailbox; bounce/complaint webhooks from transactional providers.

### 6.6 Telephony

Rust SIP/VoIP crates are immature (`rvoip-sip` 0.3.12, `xphone` 0.6.0, `wavekat-sip` 0.3.0; `libsip` last updated 2020) and Twilio crates are stale (`twilio` 1.1.0 from 2024-03, `twilly` 0.2.2 from 2025-02). Odoo's VoIP is SIP/WebRTC in the browser to an external PBX [R]. Recommendation [I]: do **CTI integration, not a softphone stack**: click-to-call and call logging through provider APIs/webhooks (Twilio, Telnyx, Vonage, 3CX-class), with WebRTC softphone in the browser frontend; signature-verified call/SMS webhooks via the common inbound pipeline.

### 6.7 Tax engines

Avalara AvaTax REST v2 core: `CreateTransaction`, `CommitTransaction`, `VoidTransaction`, `AdjustTransaction`, `RefundTransaction`, `AuditTransaction`, `ResolveAddress`, `Ping`, certificates/customers endpoints (https://developer.avalara.com/api-reference/avatax/rest/v2/); SDK language list not confirmed; no Rust crate exists on crates.io for Avalara or TaxJar (both searches return zero results); a "vertex" search only returns unrelated graphics/geometry crates, so no Vertex Inc. tax crate was found either. TaxJar: owned by Stripe, still operating, described as "two separate products" from Stripe Tax, no end-of-life date published on the pages I could read (https://www.taxjar.com/taxjar-and-stripe-tax). Stripe Tax: 100+ countries, filing via partners in 90+, and a Tax API usable with non-Stripe payments (https://stripe.com/tax). Vertex not verified [gap]. Design [I]: a `TaxProvider` trait with `calculate(document) -> lines+tax codes`, `commit`, `void`, `refund`, with a built-in rules engine (VAT/GST rates, fiscal positions, as Odoo) as the free default, and engines as plug-ins. Document lifecycle (calculate on draft, commit on post, void on cancel) is the key contract.

### 6.8 Identity, SSO and KYC

- **SSO:** `openidconnect` 4.0.1 and `oauth2` 5.0.0 (clients), `samael` 0.0.22 (SAML2, MIT, 2026-07-07). For obop acting as an OAuth/OIDC **provider**, no mature embeddable Rust crate exists: `oxide-auth` 0.6.1 was last released 2024-06; `rauthy` (Apache-2.0, pre-1.0 at 0.36.x, independent security audit published for v0.32.1, supports DPoP, passkeys, SCIM v2, dynamic client registration) is a standalone IdP (https://github.com/sebadob/rauthy). Plan an in-house authorization server with a JOSE library, using Rauthy as a reference or an optional external IdP [I].
- **KYC:** Stripe Identity covers IDs from 120+ countries with selfie match, available to merchants in ~34 countries (https://docs.stripe.com/identity). Model `IdentityVerification` as a provider-agnostic record (status, evidence refs, provider ID, expiry) so Persona/Onfido/Sumsub-class adapters can be added [I; vendors not researched].

### 6.9 Automation platforms

| Platform | Facts | Use for obop |
|---|---|---|
| Zapier | Directory of 10,198+ apps; build via Visual Builder or CLI; MCP entry at zapier.com/mcp. https://zapier.com/apps | Publish an obop Zapier app (triggers via REST hooks, polling fallback). Requires public OAuth + stable webhooks. |
| Make / n8n | n8n is under the Sustainable Use License: internal business and non-commercial use only; redistribution only free and non-commercial; `.ee` files need an enterprise licence. https://raw.githubusercontent.com/n8n-io/n8n/master/LICENSE.md | Do not bundle n8n. Ship a community n8n node and a Make app. |
| Activepieces | MIT community edition, EE under `packages/ee/`, TypeScript "pieces" on npm, pieces exposed as MCP servers. https://github.com/activepieces/activepieces | Permissive, but a Node runtime conflicts with the Rust-only backend; integrate over API/webhooks, do not embed. |
| Airbyte | "700+ connectors" claimed (the catalog page lists 653; https://airbyte.com/connectors); protocol with `spec/check/discover/read/write` and `RECORD/STATE/LOG/TRACE` messages (plus SPEC, CONNECTION_STATUS, CATALOG) over stdio (JSONL) or Unix sockets (protobuf); opaque, checkpointed state. https://docs.airbyte.com/platform/understanding-airbyte/airbyte-protocol | Borrow the *shape* for obop's sync connectors (spec/check/discover/read with cursor state). |

### 6.10 Consolidated Rust crate shortlist (crates.io, 2026-09-30)

| Area | Crate (version, date) |
|---|---|
| HTTP server / client | axum 0.8.9 (2026-04), reqwest 0.13.5 (2026-09) |
| OpenAPI | utoipa 6.0.0 (2026-09), aide 0.15.1 / 0.16.0-alpha.4, progenitor 0.15.0 (MPL-2.0 client generator, 2026-09) |
| MCP / agents | rmcp 3.5.0 (2026-09), a2a-lf 0.4.1 (official A2A SDK, repo a2aproject/a2a-rs; not the unrelated `a2a-rs` crate) |
| Webhooks | svix 2.6.1, standardwebhooks 1.0.1 |
| Rate limit / jobs | governor 0.10.4, tower_governor 0.8.0, apalis 0.7.4 (1.0.0-rc.10) |
| Wasm | wasmtime 49.0.1 (2026-09-24), wit-bindgen 0.62.0, extism 1.30.0 (BSD-3) |
| Mail / calendar | lettre 0.11.23, async-imap 0.12.0, mailparse 0.17.0, libdav 0.11.0 |
| Spreadsheets | calamine 0.36.1 (reader, MIT), rust_xlsxwriter 0.99.1 (writer), csv 1.4.0 |
| Auth | oauth2 5.0.0, openidconnect 4.0.1, samael 0.0.22 |

## 7. Connector architecture

### 7.1 Options compared [I, grounded in verified facts]

| Model | Strengths | Weaknesses | Evidence |
|---|---|---|---|
| In-process Rust crates | Fastest, full type safety, can do raw TCP (IMAP, SMTP, SIP, CalDAV) | A bug or malicious code owns the server; licence coupling (AGPL-style linking); recompiles for every change; no third-party ecosystem | Odoo's Python modules run with full DB access and no sandbox (Odoo Apps store sells paid third-party connectors, e.g. a Shopify connector at EUR 549) https://apps.odoo.com/apps/ |
| Wasm components (wasmtime + WIT) | Capability-based sandbox, language-neutral, portable, resource limits, hot-loadable | Still young: WASI 0.3 (native async, `stream<T>`/`future<T>`) published **2026-06-11**, wasmtime majors roughly monthly (48.0.0 2026-08-20, 49.0.0 2026-09-21), recent advisories in fuel accounting and outgoing HTTP body memory (both 2026-09-24), plus a high-severity filesystem sandbox escape (GHSA-vqjp-4c8c-hfgg, 2026-08-20); WASI 0.3.1 followed on 2026-08-11 with 0.3.2 scheduled for 2026-10-13, and Wasmtime has enabled 0.3 by default since v46; raw-socket (wasi:sockets) maturity not verified, so target HTTP-only connectors first [I] | https://wasi.dev/roadmap , https://github.com/bytecodealliance/wasmtime/releases , https://github.com/bytecodealliance/wasmtime/security/advisories ; precedent: Shopify Functions run Rust/Wasm |
| Out-of-process workers (Airbyte-style protocol, gRPC/HTTP) | Any language, strongest isolation, independent release | Operational weight, latency | Airbyte protocol above |
| Remote apps (OAuth + webhooks + API/MCP) | Zero trust in the vendor's code; how Shopify/Zapier work | No in-transaction hooks | Shopify app model |

### 7.2 Recommended tiering

- **Tier 0, native crates (project-maintained only):** mail protocols, bank-file parsers, payment core adapters, tax rules, OAuth broker. Reviewed like core code.
- **Tier 1, Wasm components (first-party and vetted third-party):** HTTP/JSON SaaS connectors (marketplaces, shipping, tax engines, KYC). Defined by an obop WIT world exporting `spec`, `check`, `discover`, `read(state)`, `write`, `handle-webhook`, and importing `http-fetch` (host-mediated, allow-listed hosts, SSRF-filtered, TLS and timeouts enforced by host), `secrets.get(scope)`, `kv` (connector-scoped state), `log`, `clock`. No filesystem, no raw sockets, fuel/epoch and memory limits per invocation. Extism (BSD-3, Rust host SDK) is the alternative if the WIT/component surface proves too unstable; its HTTP is host-controlled and PDKs exist for Rust, JS, Python, Go, .NET.
- **Tier 2, remote apps/workers:** anything else, via OAuth app + webhooks + the public API, and optionally an Airbyte-shaped stdio/HTTP worker contract.

Pin a single wasmtime major per obop release (Bytecode Alliance also still patches 36.0.x, suggesting an unofficial long-lived line; no LTS policy stated [V]). Insulate with obop's own WIT package so host upgrades do not break connector binaries; sign connector packages (ed25519) and record the digest in the install manifest.

### 7.3 OAuth app model

obop needs OAuth in **both directions**:

1. **obop as OAuth client** (to Google, Microsoft, Shopify, Xero, Etsy...): a token vault per connection (encrypted at rest, per-tenant key), refresh with single-flight locking, incremental scope upgrades, revocation on disconnect, and a documented BYO-app path. Hosted "obop OAuth broker" is needed for Gmail restricted scopes and Xero certification, otherwise each self-hoster would need its own verified app [I from the Google and Xero facts above].
2. **obop as OAuth provider** (for third-party apps, Zapier, MCP clients): Authorization Code + PKCE (S256), client credentials for service accounts, refresh-token rotation, exact redirect matching, RFC 9207 `iss` in responses, optional DPoP; scopes mapped to model/action permissions and company; consent screens listing exactly what is granted; app registration via **Client ID Metadata Documents** (what MCP 2026-07-28 now prefers over RFC 7591 DCR). Mirror Odoo's good idea of bot users with ordinary ACLs, but add resource-level scopes and per-app rate limits.

## 8. App and plugin marketplace model

Reference models [V]: **Odoo Apps store** (apps.odoo.com): versions 5.0 to 20.0, free and paid, upload-based submission, vendor guidelines; apps are in-process Python. **Shopify**: remote apps declared in `shopify.app.toml`, versioned APIs, delisting for use of unsupported versions, 0%/15% revenue share with a $19 fee. **Zapier**: curated directory, CLI/visual builder.

Recommended model [I]:
- Package type `obop-app`: manifest (`obop.app.toml`) declaring scopes, webhook subscriptions, declarative custom fields/models, UI extension points, Wasm connector modules, required egress hosts, data-processing declaration. Permissions are shown at install.
- No arbitrary in-process code from third parties. Business logic = Wasm (Tier 1) or remote (Tier 2); UI = sandboxed web components/iframes built on the Vite+ toolchain.
- Signed packages, reproducible-build attestation, mandatory review for listed apps, versioned API contract tests, and a kill switch.
- Commercial terms: default to a Shopify-like low take rate; keep the open registry indexable by anyone (sideloading always allowed for self-hosters).
- Licence hygiene: the Wasm/remote boundary means third-party code is not a derivative of an AGPL/LGPL core; in-process linking would raise that question [I; legal review needed].

## 9. Data migration into obop

### 9.1 Odoo import tooling, decoded [V]

Odoo's import engine (`base_import`, https://raw.githubusercontent.com/odoo/odoo/19.0/addons/base_import/models/base_import.py; user docs https://raw.githubusercontent.com/odoo/documentation/19.0/content/applications/essentials/export_import_data.rst):

- Formats: CSV, XLS, XLSX, ODS (reader chosen by sniffing content, then declared type, then extension); CSV encoding by chardet, separator guessing.
- Mapping: suggestions come from saved mappings (`base_import.mapping`), exact technical-name/label match, then `difflib` fuzzy match (distance < 0.2) restricted to type-compatible columns; field types guessed from the first ten rows.
- Identity: an `id` column is the **External ID**; relational fields accept name, `field/id` (External ID) or `field/.id` (database ID); records with an External ID or DB ID are **updated**, not created, so files are re-importable. One2many = continuation rows; many2many = comma list. Exports made for import include External IDs; records without one get `__export__.*` IDs.
- Safety: `dryrun` runs the whole load inside a savepoint, rolls back and resets caches; results return per-row errors; batching via `limit` with `skip/nextrow`.
- Attachments/images: filenames matched to uploaded files; or URLs fetched only if an admin enabled it, with `import_file_maxbytes`, timeouts and image validation.

### 9.2 Importing an Odoo database into obop [I]

1. **Source adapters:** (a) Odoo export files (keep External IDs), (b) live API: JSON-2 for Odoo 19+, XML-RPC/JSON-RPC for 14-18 (`fields_get`, `search_read`, `ir.model.data` for XML-IDs, `ir.attachment` for files), (c) read-only direct PostgreSQL dump.
2. **Identity ledger:** table `external_ref(system, model, source_id, xml_module, xml_name, obop_id, hash, imported_at)`. Re-runs upsert by `(system, model, xml_id)`; the xmlid (`module.name`) and numeric id are both retained so links can be reconciled later.
3. **Ordering:** topological by relational dependencies (companies, currencies, partners, products, taxes, accounts, journals, then documents, then payments and reconciliations); opening balances imported as dated journal entries, plus a trial-balance diff against the source.
4. **Attachments:** copy bytes, keep `res_model/res_id` link and mimetype, verify checksums.
5. **UX:** reproduce Odoo's good parts (dry-run, saved mappings, per-row errors, template downloads) and add what it lacks: resumable batches, diff report, and rollback by import run ID.
6. **Legal/clean room:** implement from behaviour and public docs; Odoo Community is LGPL v3, so reading it for design is fine, but Enterprise code and assets must not be touched.

### 9.3 QuickBooks, Xero, Shopify, Excel/CSV

- **Xero [V]:** API now metered. Tiers (effective 2026-03-02, 4 Dec 2025 start of transition): Starter 5 connections, free; Core 50 connections AUD 35/month, 10 GB egress; Plus 1,000 connections AUD 245/month, 50 GB; Advanced 10,000 connections AUD 1,445/month, 250 GB; overage AUD 2.40/GB; daily limits 1,000/org (Starter) vs 5,000/org; terms prohibit using API data to train AI/ML. https://developer.xero.com/pricing . OpenAPI specs (MIT, accounting, assets, bank feeds, files, projects, payroll AU/UK/NZ) at https://github.com/XeroAPI/Xero-OpenAPI make a generated Rust client cheap. Plan: file import first; API import through the user's own Xero app on the free Starter tier (migration is one-shot and small).
- **QuickBooks Online [R, could not fetch]:** OAuth 2.0, minor-version query parameter, per-realm throttling, batch and change-data-capture endpoints, HMAC-signed webhooks (`intuit-signature`); Intuit's partner program and pricing changes in 2025-2026 are unverified [gap]. Desktop (IIF/QBXML) is file-only.
- **Shopify [V/R]:** for a one-shot store migration use a merchant-created *custom app* with an Admin GraphQL token (no App Store registration needed), then bulk operations/JSONL for large exports [R]; alternately CSV exports.
- **Excel/CSV:** `calamine` 0.36.1 + `csv` 1.4.0 for reading, `rust_xlsxwriter` 0.99.1 for templates; sheet-level mapping profiles identical in spirit to Odoo's.

## 10. Export and portability guarantees

Legal context [V]: EU Data Act applicable since 2025-09-12. Commission FAQ v1.4 (2026-01-22): switching charges must be limited to cost now and **prohibited from 2027-01-12** (data egress included, except parallel use); maximum notice 2 months, transition up to 30 days (longer only in narrow circumstances, up to 7 months); Chapter VI applies to SaaS, and Art. 30(2)-(3) require SaaS providers to offer open interfaces and export in a structured, commonly used, machine-readable format (functional equivalence applies only to IaaS). https://ec.europa.eu/newsroom/dae/redirection/document/108144 (landing page https://digital-strategy.ec.europa.eu/en/library/commission-publishes-frequently-asked-questions-about-data-act). [I] A hosted obop would likely be a data processing service in scope (get legal review); self-hosted open source is not, but exceeding the standard is the trust play. Contrast: Odoo Online gates the API to its Custom plan; Xero now meters egress.

Guarantees obop should publish and test [I]:
1. Full tenant export on every plan, including free, async and resumable: NDJSON per model with stable IDs and relations, OpenAPI/JSON Schema of the export, attachments with checksums, audit log, configuration (custom fields, workflows), optional Postgres dump for self-hosters.
2. Per-model CSV/XLSX in import-compatible form with External IDs (Odoo's "import-compatible export" is the pattern).
3. Standards exports where they exist: iCalendar/vCard, UBL/Peppol invoices, SAF-T/FEC style audit files (accounting research doc to confirm formats).
4. No egress fees, no export throttling below API rate limits, no credentials or OAuth tokens in exports (re-link on import).
5. CI round-trip test: export -> wipe -> import -> deep diff must be empty; publish the result on every release.
6. Public deprecation policy: at least 24 months for API removals (Odoo gave about 3 years for XML-RPC; MCP mandates at least 12 months for spec features).

## Confidence & gaps

**High confidence [V]:** Odoo JSON-2/RPC/MCP facts; Odoo import engine behaviour; Stripe webhook/idempotency/versioning; Standard Webhooks spec; MCP 2026-07-28; OpenAPI 3.2.1; OAuth 2.1 and RateLimit/Idempotency draft statuses; Shopify versioning/REST legacy/revenue share; Xero tiers; Microsoft EWS (phased from 2026-10-01, full shutdown 2027-04-01)/SMTP AUTH dates; Data Act switching rules; wasmtime/WASI 0.3 status; all crate versions.

**Medium [I]:** connector tiering, marketplace model, OAuth broker necessity, migration schema, MCP-tool generation. These are design judgements built on verified facts.

**Not verified / gaps:**
- QuickBooks Online limits, minor versions, partner pricing; eBay API state; Vertex; Avalara SDK languages; UPS/FedEx retirement dates; EasyPost/Shippo specifics; Adyen library list and retry schedule; Shopify's webhook retry count; Plaid webhook signing and European coverage; GoCardless BAD sign-up status and price; PSD3 (directive) adoption status (the PSR file shows Council first-reading position still pending); CFPB 1033; Make.com; Twilio webhook signature details; Shopify GraphQL per-plan bucket sizes and the 25,000-object pagination cap (unverified); Odoo's shipping-connector list and VoIP architecture (still [R]).
- Amazon SP-API developer fees (unconfirmed reports only).
- JS-rendered pages were summarised by a model, so exact numbers from those should be re-checked before contracts or pricing decisions (the Xero tiers and Shopify revenue-share terms were re-checked against the live pages on 2026-10-01 and match).
- Rust crate quality was judged by metadata (versions, dates, downloads), not by code review.

## Implications for obop

**MUST**
1. Generate the public API from the model registry: REST/JSON resources plus action endpoints, OpenAPI 3.1 output (upgrade path to 3.2.1), RFC 9457 errors, cursor pagination with `updated_since` and tombstones, free on every tier and plan.
2. Ship an atomic `POST /$batch` (changeset semantics) so integrators do not need custom server methods for multi-step writes, which is Odoo JSON-2's acknowledged gap.
3. Ship a first-class MCP server (rmcp 3.5.0, MCP 2026-07-28 stateless Streamable HTTP) with tools derived from the registry, per-tool scopes and read-only annotations, OAuth with Client ID Metadata Documents, not a single static key; do not implement deprecated features (Sampling, Roots, Logging, HTTP+SSE, DCR-first).
4. Implement Stripe-semantics idempotency (`Idempotency-Key`, 255 chars, 24 h minimum retention, replay mismatch error) on all mutating endpoints and in every outbound provider call via an outbox.
5. Emit webhooks in Standard Webhooks format (HMAC `v1` and ed25519 `v1a`, multi-signature rotation, jittered multi-day retries, 410 disable, replay UI, SSRF-filtered egress), with inbound handling via signature check on raw body, inbox dedupe by provider event ID, state re-fetch, and scheduled reconciliation.
6. Build obop as an OAuth 2.1 provider (PKCE, rotation, exact redirects, scopes per model/action/company) and a token vault for outbound connections.
7. Ship import/export as a product: Odoo-compatible import (External ID ledger, dry-run in a transaction, saved mappings, batching, attachments) and a full-fidelity tenant export with a CI round-trip test; publish the no-egress-fee commitment.
8. Keep provider adapters behind obop-owned traits (`PaymentProvider`, `BankFeed`, `CarrierProvider`, `TaxProvider`, `MailProvider`); pin all pre-1.0 vendor crates exactly.

**SHOULD**
9. Adopt the tiered connector model: native Rust for protocol-level and payment-core adapters, Wasm components (wasmtime + obop WIT world, WASI 0.3) for HTTP SaaS connectors, remote apps for the rest; pin one wasmtime major per release and insulate with own WIT.
10. Start with Stripe (async-stripe behind trait), then Adyen, Mollie, PayPal; generate clients from vendor OpenAPI (`progenitor`, mind its MPL-2.0) where no maintained crate exists (Xero, Adyen, Avalara; Plaid's and Amazon's crates are stale or unvetted).
11. Prioritise Microsoft Graph and Gmail API over IMAP/EWS for cloud mailboxes, keep IMAP/SMTP (`async-imap`, `lettre`) for generic hosts, and plan a hosted OAuth broker for Google restricted scopes and Xero certification.
12. Make bank feeds file-first (CAMT.053/MT940/OFX/CSV), then BYO-credential aggregator connectors (GoCardless BAD, Enable Banking, Plaid, TrueLayer), decoupled from reconciliation logic.
13. Publish Zapier, Make and n8n integrations rather than bundling any of them (n8n's licence forbids commercial redistribution; Activepieces is MIT but Node-based).
14. Set a written deprecation policy of at least 24 months and date-versioned API behaviour (`Obop-Version`) with a public machine-readable changelog; adopt Shopify-style 12-month support windows for connector APIs.
15. Build a marketplace around signed `obop-app` packages (manifest-declared scopes, Wasm/remote only), sideloading always permitted, modest take rate.

**COULD**
16. Read-only OData subset for Excel/Power BI, read-only GraphQL (async-graphql, once 8.0 is stable), CloudEvents payload option, A2A agent card (a2a-lf), Agentic Commerce Protocol endpoints for the storefront module.
17. CTI telephony via provider APIs and browser WebRTC instead of a Rust SIP stack.
18. Evaluate Rauthy as an optional external IdP and reference for the authorization server.

## Verification log

Independent adversarial fact-check run on 2026-10-01 (sources: crates.io JSON API, GitHub API via `gh`, raw Odoo documentation branches 19.0/20.0/master, vendor docs, IETF datatracker, RFC editor, the EU Commission FAQ PDF text-extracted, and the Microsoft Exchange Team blog HTML). Where the author's source was a JS-rendered page, the underlying page JSON or a different primary source was used.

**Confirmed as stated: about 200 discrete statements** (approximate tally by group: ~55 crate versions/dates/licences/download counts; ~22 Odoo facts incl. JSON-2, RPC removal schedule, API-key rules, plan gating, MCP endpoint/scope/default tools, Odoo 20 release and release-note items, import-engine behaviour, Apps store; ~10 MCP 2026-07-28 changes; OpenAPI 3.2.1 date and features; ~8 IETF/standards statuses; ~19 Stripe webhook/idempotency/versioning facts; ~12 Standard Webhooks facts; SMTP AUTH timeline and Graph limits; ~14 Shopify facts; ~10 Xero facts; ~8 EU Data Act facts; WASI 0.3.0 date and wasmtime versions/dates/advisories; ~50 provider-level facts for Mollie, PayPal, Adyen, Google Calendar/Gmail, GoCardless, Enable Banking, Plaid, Stripe Identity/Tax, TaxJar, Avalara, Amazon SP-API, FedEx, Etsy, WooCommerce, n8n/Activepieces/Zapier/ERPNext licences and counts, A2A, ACP, Rauthy, Extism). All 12 author-flagged load-bearing claims held up on their core facts; two needed qualification (EWS timing, Rust tax/payment crate availability).

**Corrections (was / now / source):**

1. Odoo online payment providers: was "22 online providers plus Demo" (and "Odoo 20 supports 22 providers") / now 21 providers plus Demo in the 19.0, 20.0 and master tables (the list in the file already had 21 names) / https://raw.githubusercontent.com/odoo/documentation/20.0/content/applications/finance/payment_providers.rst
2. Microsoft EWS: was "Microsoft begins blocking EWS requests to Exchange Online on 2026-10-01" (read as a hard block) / now a phased, admin-controllable tenant-by-tenant default-disable (`EWSEnabled` Null to False) from 2026-10-01, admins can re-enable, permanent shutdown 2027-04-01 with no exceptions / https://techcommunity.microsoft.com/blog/exchange/exchange-online-ews-your-time-is-almost-up/4492361 and https://learn.microsoft.com/en-us/exchange/clients-and-mobile-in-exchange-online/deprecation-of-ews-exchange-online
3. Odoo API key expiry: was "19.0 doc states a 3-month ceiling, master says role-dependent" / now all branches state the 3-month ceiling, and programmatic generation is additionally checked against a role-based maximum; the `MCP` scope appears only in mcp_server.rst, not external_api.rst / https://raw.githubusercontent.com/odoo/documentation/master/content/developer/reference/external_api.rst and https://raw.githubusercontent.com/odoo/documentation/master/content/applications/productivity/ai/mcp_server.rst
4. Odoo db service: was cited to the 19.0 external_rpc_api page / now the "removed in Odoo 20 (fall 2026) and Online 19.1" sentence is in the master/20.0 page only / https://raw.githubusercontent.com/odoo/documentation/master/content/developer/reference/external_rpc_api.rst
5. PayPal Rust crates: was "No PayPal crate surfaced in my crates.io search" / now community crates exist (`paypal-rs` 0.2.6 from 2024-06-03 with 69k downloads, `paypal-rust` 0.0.9-alpha, `paypal` 0.2.0) / https://crates.io/api/v1/crates?q=paypal
6. Tax-engine crates: was "no crate exists for Avalara, Vertex or TaxJar (search returned zero)" / now zero results only for Avalara and TaxJar; "vertex" returns unrelated crates, no Vertex Inc. crate found / https://crates.io/api/v1/crates?q=avalara , ?q=taxjar , ?q=vertex
7. A2A Rust SDK: was crate name `a2a-rs` (shortlist and COULD item) / now the official SDK (repo a2aproject/a2a-rs) publishes `a2a-lf` 0.4.1 and `a2a-client-lf` 0.2.7; the crates.io crate `a2a-rs` 0.10.0 is an unrelated community project (emillindfors/a2a-rs) / https://github.com/a2aproject/a2a-rs (workspace Cargo.toml) and https://crates.io/api/v1/crates/a2a-lf
8. CalDAV/CardDAV: was "libdav is the only live Rust client I found" / now also `fast-dav-rs` 0.17.0 (2026-09-10) and server-side `mailrs-dav` 1.1.2 exist / https://crates.io/api/v1/crates?q=caldav
9. Airbyte connector count: was "600+ connectors" / now the site claims "700+" (catalog shows 653 listed) / https://airbyte.com/connectors
10. Shopify GraphQL limits: was "sample response: 1,000-point bucket, 50 points/s restore; pagination capped at 25,000 objects" / now restore rate is 100/200/1,000/2,000 points per second by plan, single query max 1,000 points, array inputs max 250; 1,000/50 appear only in a sample response; the 25,000 cap is not in the docs (now marked unverified) / https://shopify.dev/docs/apps/build/apis/graphql-admin/rate-limits
11. Shopify revenue share: was "0% on first $1M, then 15%, $19 registration" / now also the separate 2.9% processing fee and the exclusion for $20M+ App Store or $100M+ gross-revenue developers (15% on everything) / https://shopify.dev/docs/apps/launch/distribution/revenue-share
12. Shipping crates: was "No Rust crate found for EasyPost/Shippo" / now none for EasyPost or Sendcloud; Shippo has a stale generated `shippo` 0.1.29 (2021-07-20) / https://crates.io/api/v1/crates?q=shippo
13. Amazon SP-API: was "Rust crate not verified" and docs host developer-docs.amazon.com / now community crates `amazon-spapi` 2.0.4 and `amazon-sp-api` 0.1.16 exist (unvetted) and the docs host 301-redirects to developer-docs.amazon / https://developer-docs.amazon/sp-api/docs/sp-api-release-notes
14. PSD3/PSR status: was "current status is unverified" / now the Parliament procedure file for the Payment Services Regulation (2023/0210(COD)) shows the committee approved the agreed text 2026-05-05, stage "Awaiting Council's 1st reading position", indicative plenary 2026-12-14; the PSD3 directive file itself was not checked / https://oeil.europarl.europa.eu/oeil/en/procedure-file?reference=2023/0210(COD)
15. Stripe `stripe/openapi` repo name: was tagged [R] / now verified (linked from Stripe's SDK page); Stripe has no official Rust SDK (official list: Ruby, Python, Go, Java, Node, PHP, .NET) / https://docs.stripe.com/sdks
16. Rauthy: was "independently audited" / now the published audit covers v0.32.1, the project is at 0.36.x / https://github.com/sebadob/rauthy
17. Etsy tiers: was "Seller/Personal/Commercial, commercial manual review" / now Commercial Access additionally requires an approved Personal app first, Seller apps are own-shop only and auto-approved / https://developers.etsy.com/documentation/
18. Implication 10: was "no crate exists (Plaid, Xero, Adyen, Amazon, Avalara)" / now Plaid (`plaid` 9.0.1, 2025-01-21) and Amazon have stale or unvetted crates; no maintained crate for Xero, Adyen, Avalara / crates.io API.
19. Additions (not errors): WASI 0.3.1 shipped 2026-08-11 and 0.3.2 is scheduled for 2026-10-13 (https://wasi.dev/roadmap); high-severity wasmtime filesystem sandbox escape GHSA-vqjp-4c8c-hfgg (2026-08-20) (https://github.com/bytecodealliance/wasmtime/security/advisories); ratelimit-headers draft-11 expires 2026-11-24; the `agentic-commerce-protocol` crate was last published 2025-10-02 and predates the 2026 spec versions; Odoo Online 21.1 (winter 2027) added to the executive summary.

**Unresolved (left as [R], [gap] or marked unverified):**
- QuickBooks Online limits, minor versions and partner pricing; eBay API state and Trading API phase-out; UPS/DHL/USPS/EasyPost/Shippo API specifics; FedEx states no SOAP retirement date on the page read.
- Odoo's shipping-connector list and VoIP architecture (the documentation path tried returned 404), still [R].
- Shopify GraphQL per-plan bucket sizes and the 25,000-object pagination cap; Shopify webhook retry count (the page states none).
- PSD3 directive adoption status (only the PSR procedure file was read); the 2025-11-27 provisional agreement date rests on Wikipedia because the Council press release returned 403.
- CFPB 1033, Make.com, Twilio webhook signatures, Plaid webhook signing, GoCardless Bank Account Data sign-up status and price, Vertex, Avalara SDK languages, Adyen retry schedule, Amazon SP-API developer fees.
- Stripe Tax and TaxJar pages were read in Swedish-localised form; numbers (100+/90+ countries) matched but English wording was not checked.
- Download counts, star counts and the Zapier app count are point-in-time values and drift daily.
- Crate quality remains judged by metadata only, not code review.

- Owner-constraint pass (2026-10-01): the owner fixed `vp` (Vite+) plus pnpm as the only JavaScript tooling (no npm, npx, yarn or bun). The one `npx` hit (Odoo 20 MCP docs telling clients to bridge via `npx mcp-remote`, section 2.1 table) stays as a factual description of Odoo and now says obop would show `vp dlx mcp-remote` (`vp dlx` runs a package binary without adding it to the project, viteplus.dev guide "Package Management") or omit the bridge. The Activepieces "pieces on npm" row describes a third-party distribution and is unchanged. No npm trusted publishing, 2FA or npm CLI version material exists in this file.
