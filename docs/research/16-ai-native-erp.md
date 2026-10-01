# AI-native ERP in late 2026: what vendors ship, what the protocols require, what obop should build

Research input for obop ("one business, one platform"). Odoo is the functional reference; this is clean-room, nothing here recommends copying Odoo Enterprise code or assets. Snapshot date: 2026-10-01.

## Scope & method

Sources: live fetches on 2026-10-01 of the MCP spec site and schema source, the A2A repo and spec, Odoo 19/20 docs and the `odoo/documentation` and `odoo/odoo` repos (read through the GitHub API), Microsoft Learn, Oracle NetSuite help, SAP News Center, vendor pricing pages, GitHub repo metadata for roughly 60 projects, Hugging Face model metadata, and arXiv preprints. No web search engine was available (search-engine scraping hit CAPTCHAs and I did not bypass them), so discovery was by direct primary-source fetch; `sap.com` and `netsuite.com` marketing pages returned 403.

Tags: **[V]** verified in a primary source fetched today; **[P]** a vendor or preprint claim (we verified it is claimed, not that it is true); **[I]** inference; **(M)** from memory, unverified.

---

## 1. Headline findings

1. **MCP changed shape in July 2026.** The current spec is `2026-07-28`: stateless (no `initialize`, no `Mcp-Session-Id`), per-request capabilities in `_meta`, mandatory `server/discover`, server-initiated requests replaced by "multi round-trip requests", Tasks moved to an official extension [V]. The official Rust SDK `rmcp` 3.5.0 (2026-09-28) implements it [V].
2. **Major ERPs now expose themselves as MCP servers, and all of them inherit the calling user's permissions rather than inventing an agent permission model** (official: D365, NetSuite, Odoo 20; Frappe only through experimental/community projects; SAP's ERP-level MCP server was not verified) [V]. That is the de-facto standard. Approval gating is the differentiator and is mostly left to the client or to community projects.
3. **Odoo's AI is Enterprise-only, credit-metered, and built on pgvector.** The `ai` addon is absent from the Community tree on 19.0 and 20.0; Odoo 20 states all AI features need IAP credits; AI features require the `pgvector` extension [V].
4. **Deterministic structure beats probabilistic extraction.** E-invoices (UBL/CII/Factur-X) need no OCR; for the rest, VLM confidence must be calibrated separately because verbalized confidence is near useless (AUROC 0.54-0.74) [P].
5. **Text-to-SQL is not solved on enterprise schemas** (BIRD best test 82.95 vs human 92.96; EntSQL best 15.9% on English inputs when long-form business documents must be used). Governed semantic layers remove whole error classes, including row-level-security leaks [P].
6. **License traps exist in forecasting and OCR weights.** TimesFM 3.0 weights are non-commercial; Chronos-2 is Apache-2.0 [V].
7. **Open-weight models are now permissively licensed and good enough for a self-hosted tier:** Gemma 4 (Apache-2.0), Qwen3.6/3.8-27B (Apache-2.0), gpt-oss (Apache-2.0) [V].

---

## 2. What incumbents ship

### 2.1 Odoo 19 to 20

**Odoo 19 (Sept 2025)** [V, https://www.odoo.com/odoo-19-release-notes, https://www.odoo.com/documentation/latest/applications/productivity/ai.html]:
- *Ask AI*: Ctrl+K command palette and a top-right AI button with context-dependent prompts; the stock agent "cannot change data", it opens views and shows reports. Responses can be sent as message, logged as note, or copied (editable "Default Prompts").
- *Agents* (AI app): LLM choice (ChatGPT and Gemini versions), Response Style (Analytical/Balanced/Creative), *Restrict to Sources*, Topics, System Prompt, Sources (PDF, weblinks, Documents, Knowledge; status Processing to Indexed). Preconfigured topics: Natural Language Search, Information retrieval, Create Leads (needs CRM). https://www.odoo.com/documentation/latest/applications/productivity/ai/agents.html
- *AI fields* (via Studio or property fields): Text, Multiline Text, HTML, Integer, Decimal, Monetary, Date, Datetime, Checkbox, Many2one, Tags; prompts reference fields with `/field`; refreshed manually or by a daily scheduled action "AI Fields: Compute AI fields" that fills empty text and property fields. https://www.odoo.com/documentation/latest/applications/productivity/ai/fields.html
- Also: Gemini bring-your-own-account, ChatGPT 5.0, email-template prompts, live-chat agents that create leads, voice transcription, web search, web page generation, CRM win-probability insights, Documents "sort and trigger actions through AI prompts".
- *Document digitization* is a separate IAP service: "OCR and artificial intelligence", no engine named, no accuracy claim, extracts vendor, reference, dates, bank, currency and lines (description, qty, unit price, taxes, amounts); modes "automatic" or "on demand"; a "Reload AI data" button; per-vendor *Auto-post bills*: Always / Ask after 3 validations without edits / Never. https://www.odoo.com/documentation/19.0/applications/finance/accounting/vendor_bills/invoice_digitization.html

**Odoo 20 (Sept 2026)** [V, https://www.odoo.com/odoo-20-release-notes]:
- "All AI features now need credits, which can be bought via IAP." The "AI API keys" docs page is still "under construction", so the provider/BYOK story is undocumented; release notes say only "Select an AI provider and Odoo automatically uses the best AI model".
- Topics renamed **Skills** (instructions + tools): View Builder, Information retrieval, Update Records, Create Records, Generate Images, Web Search, Website Builder, Create Leads. Agents can be triggered by automated or scheduled actions, revise themselves, ask for confirmation when they hit a tool-call limit; conversations are stored up to 30 days. https://www.odoo.com/documentation/20.0/applications/productivity/ai/agents.html
- **AI server action = manager/worker.** The AI picks one *tool* and its arguments; a tool is an ordinary *Execute Code* server action with "Use in AI" and an *AI Schema* declaring each argument. Odoo's own docs warn that a called tool "runs unconditionally unless its own code blocks it" so business rules must live in the tool. https://www.odoo.com/documentation/20.0/applications/productivity/ai/server-actions.html
- **Document AI sort**: per-folder prompt, actions = move, tag, schedule activity, create vendor bill/invoice; prompts can start with exception checks (e.g. multipage bundle gets a "To Split" tag and halts). https://www.odoo.com/documentation/20.0/applications/productivity/ai/document_sort.html
- **MCP server** at `<db-url>/mcp`, authenticated by a *static API key* (scope `MCP`, chosen expiry, shown once); tools are server actions. Exposed by default: Get Models, Get Fields, Retrieve initial context, Search, Read group (all other tools are hidden until an admin exposes them); catalogue also includes Get Menus, Compute Date, view-opening tools, and exactly five tools whose suggested *Readonly* flag is false (Update Records, Create Records, Write Custom CSS, Apply HTML to Page, Generate an Image). Docs show Claude, Antigravity and Codex via `npx mcp-remote` (Odoo's own instruction; obop never uses npx, so obop's docs would show `vp dlx mcp-remote` or omit the bridge, owner constraint 2026-10-01). https://www.odoo.com/documentation/20.0/applications/productivity/ai/mcp_server.html , https://www.odoo.com/documentation/20.0/applications/productivity/ai/mcp_server/mcp_tools.html

**Edition and infra facts** [V]:
- `addons/ai` returns 404 on both `19.0` and `20.0` of `odoo/odoo` (Community is LGPL-3); pricing lists "Agentic AI" only under Custom plan (EUR 35.90/user/month on the yearly toggle vs EUR 19.90 Standard; the page footnote says these are first-12-month discounted prices and shows crossed-out list prices of 44.90 / 24.90, mapping read from the page HTML (I)); IAP credits (OCR, SMS...) are billed separately. https://www.odoo.com/pricing
- AI needs `pgvector` (Odoo's docs say the extension is "available only for PostgreSQL version 15 and up", although pgvector's own README says it supports Postgres 13+; Odoo 20's minimum is PostgreSQL 16, Python 3.12). https://github.com/odoo/documentation/blob/20.0/content/administration/on_premise/source.rst
- Programmatic API: `POST /json/2/<model>/<method>`, `Authorization: bearer <key>`, optional `X-Odoo-Database`. Keys: 160-bit, max lifetime 3 months, scope `rpc`. **Each call is its own SQL transaction; multi-call transactions are impossible, and the docs tell integrators to use a single `action_*` method for atomicity. No idempotency-key mechanism is documented.** XML-RPC/JSON-RPC at `/xmlrpc`, `/xmlrpc/2`, `/jsonrpc` are scheduled for removal in Odoo 22 (fall 2028); the 20.0 docs add that the RPC `db` service was already removed in Odoo 20 and that `common`/`object` go in Odoo 22 and Online 21.1 (winter 2027), i.e. hosted customers lose them earlier. External API is Custom-plan only. https://github.com/odoo/documentation/blob/19.0/content/developer/reference/external_api.rst , https://github.com/odoo/documentation/blob/19.0/content/developer/reference/external_rpc_api.rst , https://github.com/odoo/documentation/blob/20.0/content/developer/reference/external_rpc_api.rst

### 2.2 SAP Joule

[V, SAP News Center, May-Sept 2026; `sap.com` product pages blocked]:
- **SAP Business AI Platform** (Sapphire keynote 2026-05-12; the News Center article is dated 2026-05-13) merges BTP, Business Data Cloud and AI Foundation with context/build/governance layers; "more than 200 agents and over 50 assistants" across Finance, Spend, SCM, HCM, CX "in the coming months". https://news.sap.com/2026/05/sap-sapphire-keynote-business-ai-platform-power-autonomous-enterprise/
- **Joule Studio** (new version, Early Adopter Care) for building agents (Q2 highlights page). From the keynote article (not the Q2 highlights page): **Joule Work** adds "better support for open standards such as MCP and A2A" (quote from SAP Executive Board member Muhammad Alam), and **SAP AI Agent Hub** (built on SAP LeanIX) was announced as "generally available in Q3" and "included in SAP Business AI Platform at no additional charge"; the Q2 highlights page confirms the Hub discovers agents, LLMs and MCP servers across Microsoft, Google, AWS and now ServiceNow and SAP AI Core. Whether the Hub actually reached GA by end of Q3 (today) was not verified (unverified). https://news.sap.com/2026/07/sap-business-ai-release-highlights-q2-2026/ , https://news.sap.com/2026/05/sap-sapphire-keynote-business-ai-platform-power-autonomous-enterprise/
- Generative AI hub supported-model list (Q2 highlights page): Gemini 3.1 Flash Lite, Claude Opus 4.7, GPT Realtime, Mistral Small, GPT 5.4, GPT 5.4-nano, GPT 5.3-Codex. SAP and Anthropic "plan to" make Claude "a primary reasoning and agentic capability" (press release, 2026-05-12). https://news.sap.com/2026/07/sap-business-ai-release-highlights-q2-2026/ , https://news.sap.com/2026/05/sap-anthropic-to-bring-claude-sap-business-ai-platform/
- **SAP Document AI**: LLM selection and document-level confidence bands (low/medium/high, colour-coded); Ariba Invoicing multi-model extraction GA. CEO Christian Klein at the Sapphire keynote: "Eighty percent is just not good enough" for business-critical use [P, keynote article].
- Not found: AI-unit pricing, A2A specifics, sovereignty terms. SAP's own `@cap-js/mcp-server` (Apache-2.0) is developer tooling only. https://github.com/cap-js/mcp-server

### 2.3 Oracle NetSuite

[V, https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_0714082142.html]: the **AI Connector Service** is a remote MCP server: `https://<account>.suitetalk.api.netsuite.com/services/mcp/v1/suiteapp/<id>` (standard tools SuiteApp `com.netsuite.mcpstandardtools`) or `/v1/all`. Auth is **OAuth 2.0 Authorization Code, Public Client** via an integration record with an "AI Connector Service" scope (created automatically on the first Claude connection; for other clients an administrator must enable the pending record); the ChatGPT connection steps tell users to sign in with "the correct, non-administrator role"; a "MCP Server Connection" permission gates it; per-integration **concurrency limit**; execution log per integration retained **21 days (production) / 7 days (sandbox)**. Custom tools are SuiteScript 2.1 "custom tool" scripts in an SDF project; NetSuite 2026.1 adds a `toolset` SDF object, execution logs, and an **MCP App sample with bundled HTML UI**. https://github.com/oracle-samples/netsuite-suitecloud-samples/tree/main/MCP-Sample-Tools . NetSuite's marketing AI pages (Next, SuiteAgent, pricing) were 403: not verified.

### 2.4 Microsoft Dynamics 365

[V, https://learn.microsoft.com/en-us/dynamics365/fin-ops-core/dev-itpro/copilot/copilot-mcp , .../mcp/mcp-security; both updated Aug-Sept 2026]:
- **Dynamic ERP MCP server** (needs 10.0.47, or PQU builds of .46/.45): three tool classes. *Data* (OData entity find/create/update/delete; `data_find_entities_sql` replaces OData find in 10.0.48), *Form* (13 tools that drive forms as a human would, via server view-model, not screen scraping), *Action* (`api_find_actions`/`api_invoke_action` over developer classes implementing `ICustomAPI`). The older 13-tool static server **retires 2026-10-01** (today).
- Context is **dynamic per call and per security role**: a tool only returns menu items, entities, fields and actions the role may access; explicit calls to others are rejected.
- Security: every call runs as the authenticated user (delegated) or as the agent's own identity (autonomous); roles/duties/privileges/record-level security apply; business logic and workflows are not bypassed; MCP server stores no customer data; **admins allowlist MCP clients by Entra client ID** (defaults: Copilot Studio, VS Code, Microsoft Cowork, Finance Agent, Finance Agent (Sydney)). Security-admin forms are excluded from agent access. Agent identities need no user licence if given the empty "System agent" role.
- Pricing: outside Copilot Studio, **0.1 Copilot Credit per tool call** (one credit per 10 calls) plus the client's own token bill; users with Dynamics 365 Finance Premium or Supply Chain Management Premium licences are exempt from this tool-execution charge on non-Copilot-Studio clients. Inside Copilot Studio the call is folded into the fixed *Agent Action* rate.
- Limitations: en-US metadata only, no advanced grid filters, some controls unsupported, unavailable during servicing windows.

### 2.5 Zoho and ERPNext

- **Zoho** [P, https://www.zoho.com/zia/ , https://www.zoho.com/mcp/]: Zia Agents / Chat / Skills; claims no "pricy AI licenses or per-use fees" and no third-party data processors (marketing; no tiers given). Zoho MCP: "500+ integrations", OAuth, user-level permissions, model-agnostic. Zia LLM specs not found.
- **ERPNext/Frappe**: no AI feature appears in the ERPNext docs intro (https://docs.frappe.io/erpnext/user/manual/en/), but the Frappe GitHub org itself hosts an AI-agent app, `frappe/flow_client` ("Frappe Flow", AGPL-3.0, 76 stars, created 2026-06-02; in-Desk assistant that "respects your permissions", agents, RAG), and an archived `frappe/llm` helper superseded by `frappe/otto` [V, GitHub metadata 2026-10-01]; treat as early/unreleased product. Frappe ships `frappe/mcp` (MIT, 168 stars, "highly experimental", tools only, Streamable HTTP, built because the official Python SDK is async-only). Community: Frappe Assistant Core (AGPL-3.0, 316 stars, 24 tools, OAuth 2.0 as a real ERPNext user, "Assistant Audit Log", a paid SaaS chat layer added in 3.0), Casys-AI/mcp-erpnext (MIT), LazyChat (20 stars, MIT, BYO LLM, "Apply-gated" staged edits, 95 or 101 permission-scoped tools: the README gives both numbers). https://github.com/frappe/mcp , https://github.com/buildswithpaul/Frappe_Assistant_Core , https://github.com/soumyasethy/lazychat-erpnext [V]

### 2.6 Odoo MCP ecosystem (community)

| Project | Stars / licence | Transport to Odoo | Notable safety design |
|---|---|---|---|
| erpipe-org/mcp-odoo | 417 / MIT | XML-RPC (16-18), JSON-2 (19+); MCP 2026-07-28 | Direct create/write/unlink blocked; writes need live `fields_get` validation + approval token + explicit confirm + env gate, optional MCP elicitation diff, JSONL audit, field-level ACL, rate limit |
| ivnvxd/mcp-server-odoo | 396 / MPL-2.0 | XML-RPC; Odoo-side module (16+) or "YOLO" mode | Per-model read/write/create/delete toggles; credential-like fields excluded by default; HTTP transport has no client auth |

Also pantalytics/odoo-mcp-pro (82 stars) and rosenvladimirov/odoo-claude-mcp (AGPL-3.0, 197+ tools), not inspected. Sources: https://github.com/erpipe-org/mcp-odoo , https://github.com/ivnvxd/mcp-server-odoo [V]. The erpipe README also asserts Odoo's built-in AI is Enterprise-only [P; independently confirmed above].

### 2.7 Pattern across vendors [I]

Odoo (static key = user), D365 (Entra OAuth, role-filtered tool list), NetSuite (OAuth 2.0 code grant, non-admin role) and Frappe (OAuth 2.0 as a real user) all expose MCP with the caller's own permissions. Approval gates are client-side or in tool code (Odoo), undocumented (NetSuite), or community add-ons (erpipe, LazyChat). Metering differs: Odoo IAP credits, D365 0.1 credit per tool call, NetSuite concurrency limits.

---

## 3. MCP: spec, authorization, Rust

### 3.1 Current version [V]

`2026-07-28` is "current" (https://modelcontextprotocol.io/specification/versioning); changelog https://modelcontextprotocol.io/specification/2026-07-28/changelog :
- No `initialize` handshake; every request carries `io.modelcontextprotocol/protocolVersion`, `clientCapabilities`, optionally `clientInfo`; `server/discover` is mandatory on servers; version mismatch returns `UnsupportedProtocolVersionError`.
- Streamable HTTP without sessions; SSE resumability removed; `subscriptions/listen` replaces GET stream and `resources/subscribe`; `Mcp-Method` / `Mcp-Name` headers required, tool params can be mirrored into `Mcp-Param-*` headers via `x-mcp-header` so WAFs/gateways can route without parsing bodies.
- **Multi round-trip requests (MRTR)**: a server needing input (elicitation, sampling, roots) returns `resultType: "input_required"` with `inputRequests` and an opaque `requestState`; the client retries with `inputResponses`. Servers MUST treat `requestState` as attacker-controlled and, when it influences authorization, resource access or business logic, MUST protect its integrity (HMAC/AEAD) and reject state that fails verification; they SHOULD bind it to principal, TTL and a digest of the original request; servers that need single-use MUST enforce it server-side. https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns/mrtr
- **Tasks** are now the official extension `io.modelcontextprotocol/tasks`: durable `taskId`, poll via `tasks/get`, `input_required` state for human approvals answered via `tasks/update`, cooperative `tasks/cancel`. https://modelcontextprotocol.io/extensions/tasks/overview
- Deprecated: Roots, Sampling, Logging, HTTP+SSE transport, and **Dynamic Client Registration** (in favour of Client ID Metadata Documents). Results carry `ttlMs`/`cacheScope`; `tools/list` should be deterministic; OpenTelemetry `traceparent`/`tracestate`/`baggage` propagate in `_meta`; error codes -32020..-32099 reserved for MCP.
- Tool annotations (`readOnlyHint` default false, `destructiveHint` default true, `idempotentHint` default false, `openWorldHint` default true) are **hints; clients MUST treat them as untrusted unless the server is trusted**. Source: https://github.com/modelcontextprotocol/modelcontextprotocol (`schema/2026-07-28/schema.ts`). Tools page: "there SHOULD always be a human in the loop"; servers MUST validate inputs, enforce access control, rate limit and sanitize outputs. https://modelcontextprotocol.io/specification/2026-07-28/server/tools

### 3.2 Authorization model [V, https://modelcontextprotocol.io/specification/latest/basic/authorization]

- Optional; HTTP transports SHOULD follow it, stdio SHOULD NOT. MCP server = OAuth 2.1 **resource server**; authorization server may be separate.
- Servers MUST publish **Protected Resource Metadata (RFC 9728)**; clients MUST use it to find the authorization server, and support both RFC 8414 and OIDC discovery.
- Client registration: **Client ID Metadata Documents** (SHOULD support, for client/server pairs with no prior relationship), pre-registration (SHOULD support), DCR (MAY, deprecated). Client selection order (SHOULD): pre-registered credentials first, then CIMD if the authorization server advertises `client_id_metadata_document_supported`, then DCR, then prompt the user. https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/client-registration
- Clients MUST send **`resource` (RFC 8707)** on authorization and token requests; servers MUST validate the token audience is themselves and MUST NOT accept or forward other tokens (token passthrough is forbidden).
- PKCE (clients MUST use S256 and refuse to proceed if `code_challenge_methods_supported` is absent); `iss` per RFC 9207 (anti mix-up: authorization servers SHOULD emit it, clients MUST validate a present one); tokens only in the `Authorization` header, never the query string (https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/security-considerations).
- Scopes: `WWW-Authenticate` `scope=` on 401, and **403 `insufficient_scope` for step-up**, all needed scopes in one challenge; clients send the union of scopes; retry limited.
- Extensions: *Enterprise-Managed Authorization* (Stable), *OAuth client credentials* (Draft) https://github.com/modelcontextprotocol/ext-auth .
- Security best practices (confused deputy, SSRF on metadata fetch, state-handle hijacking, scope minimization, local-server compromise): https://modelcontextprotocol.io/docs/tutorials/security/security_best_practices

### 3.3 Rust SDK [V]

`modelcontextprotocol/rust-sdk` crate **`rmcp` 3.5.0** (2026-09-28, ~30.9M downloads, 3,963 stars): "implements the stable MCP 2026-07-28 specification while remaining fully compatible with 2025-11-25", with discover lifecycle, MRTR, tasks, caching, stateless Streamable HTTP, OAuth support, JSON Schema 2020-12 via `schemars`. Licence is mid-transition MIT to Apache-2.0 (new code Apache-2.0; un-relicensed contributions stay MIT). https://github.com/modelcontextprotocol/rust-sdk . crates.io independently shows `rmcp` 3.5.0 published 2026-09-28 with `Apache-2.0` crate metadata and 30,870,434 downloads (https://crates.io/crates/rmcp). Alternative: `rust-mcp-sdk` (MIT, 196 stars, crate v2.0.0 of 2026-08-27). A Rust MCP/A2A-aware proxy exists: `agentgateway` (Apache-2.0, Rust, 5.1k stars, latest stable v1.5.0 of 2026-08-27; v1.6.0 alphas exist). https://github.com/agentgateway/agentgateway

---

## 4. Agent-to-agent and commerce protocols

- **A2A**: v1.0.0 released 2026-03-12, v1.0.1 2026-05-28; Apache-2.0; 25,977 stars; Linux Foundation package prefix. Proto-first normative definition (`a2a.proto`), bindings JSON-RPC, gRPC and HTTP+JSON, `A2A-Version` header, Agent Card (identity, skills, interfaces, signatures), Task states (`SUBMITTED, WORKING, COMPLETED, FAILED, CANCELED, REJECTED, INPUT_REQUIRED, AUTH_REQUIRED`), streaming and push webhooks, OAuth2 (device code + PKCE added, implicit/password removed), OIDC, mTLS, signed cards. Official SDKs: Python, Java, JS, .NET, Go, and Rust (`a2aproject/a2a-rs`, Apache-2.0, 87 stars, crate `a2a-lf` 0.4.1 of 2026-09-30, axum/tonic; the Rust SDK is pre-1.0 while the protocol is 1.0.x). https://github.com/a2aproject/A2A/releases , https://a2a-protocol.org/latest/specification/ , https://github.com/a2aproject/a2a-rs [V]
- **Relationship**: MCP = agent-to-tool (deterministic capability surface); A2A = agent-to-agent (opaque peers, long tasks). [I] For obop, MCP is the primary surface; A2A matters for cross-company flows (your purchasing agent negotiating with a supplier's agent), which is a later feature.
- **Commerce protocols** [V existence only; specs not read]: Agentic Commerce Protocol (maintained by OpenAI and Stripe, spec in `beta`, Apache-2.0, 1,555 stars), Universal Commerce Protocol (Google- and Shopify-led, with council members from Etsy, Meta, Amazon, Target, Microsoft, Stripe, Salesforce and Wayfair; Apache-2.0, 3,406 stars), AP2 and x402 for payments (existence not re-checked). https://github.com/agentic-commerce-protocol/agentic-commerce-protocol , https://github.com/Universal-Commerce-Protocol/ucp . [I] Relevant to the obop e-commerce/POS modules (agents as buyers), not to the first AI milestone.

---

## 5. Safe tool design for transactional systems

What the evidence says, and the resulting obop pattern:

| Concern | Evidence | obop pattern |
|---|---|---|
| Permission inheritance | D365: "never do more through MCP than in the client", role-filtered tool lists [V]; NetSuite non-admin role [V]; Odoo key = user [V] | Agent is a *principal*. Effective permission = intersection(user grants or service-role grants, token scopes). Tool list is per-principal (spec allows list to vary by authorization). |
| Business logic not bypassed | D365: same APIs, validations, workflows [V]; Odoo docs: tool "runs unconditionally" unless code blocks it [V] | Agents call the same command handlers as the UI. No "agent-only" write path; no raw SQL writes. |
| Atomicity | Odoo JSON-2: one transaction per call, use `action_*` [V] | Expose business-level commands (`sales_order.confirm`), never field-level CRUD sequences, as the default agent tools. |
| Idempotency | Odoo: none documented [V]; IETF `Idempotency-Key` header draft is an expired Internet-Draft (-07, 2025-10-15), never an RFC [V]; MCP `idempotentHint` is only a hint [V] | Every write command takes a client-supplied idempotency key, stored with a request fingerprint; replay returns the original result; same key + different payload = 422; a retry that arrives while the first request is still in flight = 409 (both SHOULDs per draft-ietf-httpapi-idempotency-key-header-07 [V], https://www.ietf.org/archive/id/draft-ietf-httpapi-idempotency-key-header-07.txt). Retention at least 24 h (obop design choice, [I]; the draft only says the resource SHOULD define key expiry). |
| Dry-run / preview | erpipe: validate then approval token [V]; LazyChat "Apply-gated" [V]; Odoo `Create Records` "optionally with a preview menu" [V] | `dry_run=true` on every write command returns the diff, validation errors, risk tier, approvals required, and a signed `plan_id`. Commit must present the `plan_id` (digest-bound, principal-bound, TTL, single-use), so what was approved is what runs. |
| Approval gate | MCP: human in loop SHOULD; MRTR `input_required` and Tasks `input_required` carry the approval round-trip [V] | Policy engine decides `auto` / `confirm` / `approve-by-role`. `confirm` uses MRTR elicitation; `approve-by-role` creates a durable approval item and returns a Task. Agent principals can never approve their own proposals (separation of duties). |
| Prompt injection into payments | Preprint "Certified Multi-Source Integrity for Structured Agent Actions" (2026-09-28): a realistic injection "fools every model but one" (5 models, real agent loop) and a naive agent executes the fraudulent action on most attacks; defence certifies action-critical fields against corroborating, corruption-distinct evidence classes. Paying an invoice is only the motivating example; the empirical worlds are sanctions-designation data (70,966 entities) and software supply-chain provenance (450 packages), not payments [P, https://arxiv.org/abs/2609.34245] | Treat document-derived fields (bank account, amount, payee) as *untrusted*; changes to bank details, new payees, and payment amounts above threshold always require human approval regardless of agent confidence. |
| Earned autonomy | Odoo auto-post: "ask after 3 validations without edits" [V] | Per-vendor / per-rule trust counters that lower friction only after N unedited human validations, resettable by any edit. |
| Audit | MCP clients SHOULD log tool use; NetSuite execution logs 21 d/7 d [V]; Frappe audit log [V]; erpipe JSONL [V] | Append-only event per tool call: principal chain (user, agent, client id), model id, trace id, idempotency key, args hash, result, approval ids. Correlate via W3C `traceparent` in MCP `_meta`. Retain per policy; store prompt text only if the tenant opts in. |
| Rate / budget | MCP MUST rate limit [V]; NetSuite concurrency limit [V]; erpipe rate limit [V] | Per-principal token bucket, per-tool concurrency, per-run tool-call cap (Odoo 20 asks for confirmation at the cap [V]), and spend caps. |
| State handles | MCP: handles are names not capabilities; bind to user [V] | Any multi-step draft (basket, batch import) is a server-side object keyed `(principal, handle)`; never trust the handle alone. |

[I] Default stance: **agents create drafts, humans post.** ERPs already have draft/posted lifecycles (invoices, journal entries, POs); mapping "agent may create and edit drafts, posting/sending/paying needs a human or a policy" gives most of the value at low risk.

---

## 6. Natural-language query over business data

Evidence [P unless noted]:
- BIRD leaderboard (2026-09-26): best test execution accuracy **82.95** (GrainSQL) vs human **92.96**; single-model track best 80.04 (Gemini-SQL2). https://bird-bench.github.io/ [V]
- Spider 2.0 (enterprise-scale, multi-dialect): best **96.70** on Snow (specialised agent, 547 tasks), **76.23** on Lite, **65.6** on DBT; classic prompt-pipeline baselines (DIN-SQL / CHESS / DAIL-SQL + GPT-4o) score 0.00-2.20 on Snow. Entries carry no dates, and the page says scores may shift slightly as metrics are rechecked. https://spider2-sql.github.io/ [V]
- EntSQL (SQL that depends on private business documents; 1,066 aligned Chinese-English examples, five domains): best **15.9%** on English inputs when long-form documents are supplied. https://arxiv.org/abs/2606.03363
- Context ablation on DABStep: curated semantic content lifts hard-task accuracy (13.9% to 55.1%, 37.0% to 77.4% on other models); retrieval scaffolding alone is worth roughly 0-5 points on the two flash-tier models and 14-15 points on the two frontier models; ungoverned arms issued **166 mutating statements**, governed arms none. https://arxiv.org/abs/2609.22259
- Governed semantic layer (GROUND): zero measured hallucinations across six categories; ungoverned systems "violate row-level security on many questions"; exact metric definitions *without* an access policy still leak. https://arxiv.org/abs/2608.26157
- Large schemas: context compression (2.6M to 34.7K tokens on the largest Spider 2.0-Snow subset) raises strict schema-linking recall from 0% to 56.5% (DeepSeek-V3.2; 63.1% with Claude Opus 4.7) but end-to-end execution accuracy only 1.8-1.9 points; the authors' claim is that the bottleneck is representation, not reasoning. https://arxiv.org/abs/2606.28601

How vendors do it: Odoo's agent does *not* write SQL; it uses ORM-level tools (Get Models, Get Fields, Search by domain, Read group, Compute Date) and turns requests into view filters, under the user's access rules [V]. D365 moved to a SQL read tool only in 10.0.48 and still enforces role security [V]. NetSuite community uses SuiteQL [P].

Pitfalls to design against [I]: wrong grain/double-counting across joins; ambiguous metrics ("revenue" vs invoiced vs paid); currency and company scope in multi-company data; timezone/period boundaries; soft-deleted and draft rows; silent row-level-security bypass via views or a privileged DB role; expensive cross-joins; answers that cannot be reproduced.

**Decision**: *semantic layer first, text-to-SQL as a read-only fallback.* The model emits a typed query (model, measures, dimensions, filters, period, currency), not SQL. The platform compiles it to SQL under the caller's row-level security. Every answer returns the structured query and record references so the UI can show "how this was computed". A raw-SQL tool, if offered at all, runs on a separate read-only role against allow-listed views with `statement_timeout`.

---

## 7. Document extraction (bills, receipts, statements, POs)

**Reduce the problem first** [I]: prefer structured sources. E-invoices (UBL, CII, Factur-X/ZUGFeRD, Peppol BIS), bank feeds (ISO 20022 camt.053, MT940, OFX, open-banking APIs), EDI and PO portals should be parsed deterministically; OCR/VLM is the fallback for PDFs and photos. (Mandates are expanding across the EU; dates were not re-verified today, (M).)

**Open OCR/VLM stack** [V, GitHub metadata 2026-10-01]: Docling (MIT, 68k stars, v2.132.0 published 2026-10-01), PaddleOCR (Apache-2.0, 90k stars, v3.7.0), olmOCR (Apache-2.0, 7B VLM, v0.4.27), MinerU (custom licence, verify), marker (Apache-2.0 code; verify weight terms), DeepSeek-OCR (MIT). olmOCR-2: **82.4 on olmOCR-Bench**, tables 84.9, old scans 47.7, "<$200 per million pages", needs an NVIDIA GPU with at least 12 GB VRAM [P vendor claim]. https://github.com/allenai/olmocr

**Accuracy and confidence**:
- Verbalized VLM confidence is unusable for straight-through processing: AUROC 0.54-0.74, clearing only 0.1-7.0% of fields at <10% target error; a decomposed score (perception, layout, validation) plus conformal risk control reaches AUROC 0.90-0.99 and auto-approves 49-72% of fields at the target error (Qwen3.6-27B and Gemini-3.1-Flash-Lite, three datasets, some synthetic). https://arxiv.org/abs/2609.20110 [P]
- On DocILE (55-field invoices, 26% of extractions fail), a multi-signal confidence classifier (ExtractConf: disagreement between two differently prompted calls plus LLM-internal uncertainty, OCR, image quality and layout; the disagreement is one input, not the whole score) gets ROC AUC 0.928 and 99.1% accuracy at 80% coverage. https://arxiv.org/abs/2606.24420 [P]
- Pre-inference routing between a cheaper and a stronger extractor (choice made from document signals before inference, within 0.02 F1 of always using the large model) saves 31-33% on receipts and up to 77% on degraded ad-buy forms, but **not** on clean digital invoices or nutrition labels; routers need retraining per dataset and do not transfer even within a genre. https://arxiv.org/abs/2608.06607 [P]
- Vendors: Odoo publishes no accuracy, offers "Reload AI data" and earned auto-post [V]; SAP Document AI exposes per-document confidence bands and model selection [V].

**Pipeline for obop** [I]: ingest (mail alias, upload, API) then structured-format detect then classify then OCR/VLM to a typed schema with per-field evidence boxes then **deterministic validators** (line sums = totals, tax recomputation, VAT-ID checksum, IBAN checksum, duplicate invoice fingerprint, PO/receipt three-way match within tolerance) then calibrated confidence then auto-draft, or review queue with the source image highlighted. Corrections feed per-vendor few-shot memory and the eval set. Bank statements: parse structured first; VLM only for PDF statements, with running-balance reconciliation as the hard validator.

---

## 8. Forecasting

- **Chronos-2** (Amazon): Apache-2.0, 120M params (28M small), zero-shot univariate, multivariate and covariate forecasting, package v2.3.2 (2026-09-08). https://github.com/amazon-science/chronos-forecasting [V]
- **TimesFM 3.0** (Google, Aug 2026): #1 on fev-bench, TIME and GIFT-Eval per its README [P]; **3.0 weights are under the separate `timesfm-non-commercial-license-v1.0`, restricted to non-commercial, non-production use** (the Hugging Face card is tagged `license: other`); commercial and production use is permitted only through authorized Google Cloud services (BigQuery ML; the README also lists Vertex Model Garden). Source code is Apache-2.0 and weights up to 2.5 remain Apache-2.0. TimesFM 3.0 was released 2026-08-28 (GitHub tag v3.0.0). https://github.com/google-research/timesfm , https://huggingface.co/google/timesfm-3.0-pytorch [V]. This is a licence trap for a self-hostable product.
- Statistical baselines: `statsforecast` (Apache-2.0, v2.1.1) [V]; Rust: `augurs` (Grafana, dual MIT OR Apache-2.0, v0.10.2 of 2026-02-24) [V], which also offers outlier detection (`augurs-outlier` crate, per its README) [V].
- [I] Demand/cash-flow forecasting in ERPs is dominated by seasonality, promotions, stock-outs (censored demand) and hierarchies; start with seasonal-naive/ETS/Theta baselines, hierarchical reconciliation, and backtests; add a foundation model as an optional `Forecaster` behind the same trait. Always show prediction intervals; never auto-order from a point forecast without a safety-stock policy.

---

## 9. Anomaly and fraud detection

I could not fetch current ACFE statistics (page 404), so no fraud-prevalence numbers are asserted. [I] Order of preference: (1) **deterministic controls**: duplicate-invoice fingerprints (vendor + number + amount + date fuzzy), vendor bank-detail change followed by payment, split payments just under approval thresholds, round-number and Benford deviations on populations, journal entries at odd hours or by unusual users, segregation-of-duties conflicts, three-way-match breaches; (2) **robust statistics** (MAD/IQR per vendor or account, seasonal residuals from the forecaster) and unsupervised models (isolation forest; `linfa` MIT OR Apache-2.0, 0.8.1 of 2025-12-23 [V] in Rust); (3) **LLM only to explain and triage**, never to decide. Each flag is an `anomaly` record with rule id, evidence, score, disposition, and reviewer; dispositions become labels for later supervised ranking. GDPR Art. 22 and works-council rules apply where employees are scored (see section 12).

---

## 10. Embeddings and RAG over business records

- Odoo uses `pgvector` for agent Sources (PDF, links, Documents, Knowledge) [V]. That validates the "same Postgres" design.
- **pgvector** v0.8.6 (2026-07-29): types `vector` (<=2,000 dims indexable), `halfvec` (<=4,000), `bit` (<=64,000), `sparsevec` (<=1,000 non-zeros); HNSW and IVFFlat; **ANN filtering is applied after the index scan**, so selective filters return fewer rows than requested unless iterative scans (0.8.0+) or partial indexes / partitioning are used. https://github.com/pgvector/pgvector [V]. [I] The same post-filter effect applies to row-level security and ACL predicates: permissioned RAG must over-fetch or partition, and must re-check ACLs on the final rows.
- **Qdrant** (Rust, Apache-2.0, v1.19.1): recommends one collection with a tenant payload index (`is_tenant`, v1.11+), user-defined sharding (1.7+), tiered multitenancy (1.16+, promote tenants at roughly 20,000 points). https://qdrant.tech/documentation/guides/multiple-partitions/ [V]
- Extensions: pgvectorscale (PostgreSQL licence, Rust, 0.9.1), VectorChord (custom licence, 1.1.1), ParadeDB (AGPL-3.0, BM25-in-Postgres) [V metadata]; check licences against obop's own before depending.
- Embedding models: Qwen3-Embedding (Apache-2.0), bge-m3 (MIT), nomic-embed-text (popular on Ollama); Rust runtime `fastembed-rs` 7.1.0 and `ort` 2.0.0-rc.13 [V].
- [I] ERP-specific advice: embed *records rendered as text with provenance* (document number, partner, date, status) plus unstructured attachments; store `model_id` and `dim` on each chunk (switching embedding model means re-embedding); hybrid retrieval (Postgres full-text + vectors, fused); do not use RAG for numeric questions (use the semantic layer); include freshness (`updated_at`) and reindex triggers from the event log.

---

## 11. Private and on-prem inference

| Runtime | Version / licence (2026-10-01) | Notes |
|---|---|---|
| Ollama | v0.35.0, MIT, 182k stars | Easiest single-node; library includes gemma4, qwen3.5/3.6/3.8, gpt-oss, embeddings |
| vLLM | v0.30.0, Apache-2.0, 93k stars | Production serving; OpenAI **and Anthropic Messages** APIs, structured outputs (xgrammar/guidance), tool-call parsers, multi-LoRA, broad quantization |
| llama.cpp | build b11317 (tags now `bNNNN`; a `v0.5.0` tag also exists), MIT | CPU/consumer GPU, GGUF |
| mistral.rs | v0.9.4 (GitHub release 2026-09-24), MIT (Rust); the latest `mistralrs` / `mistralrs-core` crate on crates.io is still 0.8.1 (2026-04-02), so a Cargo dependency lags the GitHub tag | OpenAI-compatible `/v1` plus Anthropic `/v1/messages`; Gemma 4; its own benchmarks show wins vs llama.cpp on Q8 but vLLM far ahead on large-MoE BF16 prefill |
| candle | 0.11.0 (crates.io 2026-06-26), MIT OR Apache-2.0 (Rust) | Minimal ML framework |
| burn | v0.21.0 (2026-05-07; 0.22.0 pre-releases on crates.io), MIT OR Apache-2.0 (Rust) | Training/inference framework |

Sources: https://github.com/ollama/ollama , https://docs.vllm.ai/en/latest/ , https://github.com/EricLBuehler/mistral.rs , https://github.com/ggml-org/llama.cpp , https://github.com/huggingface/candle , https://github.com/tracel-ai/burn [V].

**Open-weight models (licence from Hugging Face metadata)** [V]: Gemma 4 (E2B, E4B, 12B, 26B-A4B, 31B; **Apache-2.0**, a change from Gemma 3's custom licence), Qwen3.6-27B and Qwen3.8-27B (Apache-2.0; the flagship Qwen3.8-2.4T and Flash-Next carry an "other" licence), Qwen3.5-122B-A10B (Apache-2.0), gpt-oss-20b/120b (Apache-2.0). https://ollama.com/library

[I] Rust-only backend implication: **do not embed inference in the server process.** Talk to any OpenAI-compatible (and Anthropic-compatible) endpoint; ship Ollama/vLLM/mistral.rs as sidecars in the self-hosted profile. mistral.rs or candle can be an optional in-tree engine later, since mistral.rs already exposes both API dialects.

**Rust client crates** [V metadata]: `rig` (crate `rig-core`, MIT, v0.43.0 of 2026-09-30), `genai` (MIT OR Apache-2.0; 0.6.5 stable, 0.7.0-rc.1 pre-release; Ollama, OpenAI, Anthropic, Gemini, DeepSeek, OpenRouter...), `async-openai` (MIT, 0.42.1). [I] A thin in-house provider trait over `reqwest` is viable because the wire formats converge on OpenAI-compatible, Anthropic Messages, Gemini.

---

## 12. Privacy and regulation

- **Data flow is the product decision.** D365's MCP server stores no data but the agent client may, and Microsoft tells admins to audit the client's retention [V]. Odoo stores conversations 30 days [V]. Gemini's free tier uses content "to improve Google's products", paid does not [V, https://ai.google.dev/gemini-api/docs/pricing]: BYOK users on free keys silently leak business data. [I] obop must warn when a configured key is a known free/consumer tier and let tenants classify fields (PII, financial, health) with per-provider egress policy.
- **Data residency has a price**: Anthropic first-party `inference_geo: "us"` is a 1.1x multiplier; regional Bedrock/Vertex endpoints carry a 10% premium [V, https://platform.claude.com/docs/en/about-claude/pricing].
- **EU AI Act** (per https://artificialintelligenceact.eu/implementation-timeline/, updated 2026-08-31) [V]: general application 2026-08-02; Art. 50(2) transparency for generative systems placed before that date by 2026-12-02; high-risk Annex III obligations listed at 2027-12-02 and Annex I at 2028-08-02. The page does not explain the shift from the original 2026-08-02; confirm the delay instrument before relying on it. [I] Annex III includes employment and worker-management uses and creditworthiness, so AI in obop's HR and credit-limit features is the exposure, not invoice OCR. Design: human oversight, logging, and the ability to disable AI per module.
- **GDPR** [I, (M)]: Art. 22 limits solely automated decisions with legal effect (e.g. automated rejection of a job applicant or expense); DPA and sub-processor lists are needed when the operator chooses a hosted model; the self-hosted profile avoids third-country transfer by construction.
- SAP/Anthropic and Zoho statements on sovereignty are marketing and unspecific [P].

---

## 13. Cost model

Verified price points (USD per million tokens, input/output) [V]: Claude Sonnet 5.5 **$2/$10** (cache read $0.20; batch $1/$5), Haiku 4.5 $1/$5, Opus 5.5 $4/$20, Fable 5.1 $10/$50; Gemini 3.5 Flash-Lite $0.30/$2.50, 3.1 Flash-Lite $0.25/$1.50, 3.8 Flash $0.75/$3.75 until 2026-12-31 then $1.50/$7.50; batch 50% off on both. Claude 4.7+ tokenizers emit roughly 30% more tokens for the same text; full 1M context at standard price (Claude 4.6 and later). Sonnet 5's $2/$10 was launched as introductory pricing through 2026-08-31 and is now the standard price; the planned 2026-09-01 rise to $3/$15 will not occur. Opus 5.5 cache reads are billed at 0.05x input ($0.20). https://platform.claude.com/docs/en/about-claude/pricing , https://ai.google.dev/gemini-api/docs/pricing

Worked estimates [I, assumptions stated; multiply to taste]:

| Workload | Assumption | Sonnet 5.5 | Flash-Lite class | Self-hosted |
|---|---|---|---|---|
| Invoice page extraction | 2,500 in (image+schema) + 600 out | ~$0.011 (batch ~$0.0055) | ~$0.0015 (3.1 FL) | olmOCR "<$0.0002/page" [P], GPU amortization extra |
| 10,000 invoices/month | same | ~$110 (batch ~$55) | ~$15 | ~$2 marginal + hardware |
| One chat turn | 20k context, 15k cached, 1k out | ~$0.023 | ~$0.0045 (unverified: assumes a ~10% Gemini cache-read rate) | n/a |
| Multi-step agent task | ~7 model calls, context grows | ~$0.15-0.30 | ~$0.03 | n/a |

Metering precedents: Odoo 20 bills all AI through IAP credits (price not found) [V]; D365 0.1 credit per MCP tool call plus client LLM cost [V]; Zoho claims no per-use fees [P]. [I] obop's position is strongest if it meters nothing itself: BYOK means the tenant pays the provider directly; obop provides a **usage ledger** (tokens, cost estimate, per agent/user/task) and hard budget caps. Self-hosted GPU cost was not researched (no verified figures); the only hard number is olmOCR's 12 GB VRAM minimum [V].

---

## 14. Recommended architecture for obop

```
                   +-------------------- obop core (Rust) -----------------------+
 Human UI (Vite+)  |  Command/Query API (typed, versioned, one definition each)   |
 Ask AI, drafts,   |    metadata per operation: effect, risk tier, idempotency,    |
 approvals inbox --+--> permissions, dry_run, approval policy, cost class          |
                   |        | generates (same source of truth)                     |
 OpenAPI 3.1 <-----+        +--> MCP tools (rmcp)  +--> A2A skills (a2a-rs)        |
                   |  Authorization server / OIDC bridge (OAuth 2.1, CIMD, RFC 8707)|
                   |  Policy engine -> proposals -> approvals -> commit (idempotent)|
                   |  Event log + audit (OTel GenAI trace ids)                      |
                   +---+-------------------+---------------------+-----------------+
                       |                   |                     |
            AI runtime (in-process     Postgres (+pgvector,   Sidecars (optional)
            orchestration, no          RLS, semantic-layer    Ollama / vLLM / mistral.rs
            inference)                 views)                 OCR: docling / PaddleOCR / olmOCR
            LlmProvider trait:                                Forecast: Chronos-2
            OpenAI-compat, Anthropic,
            Gemini; BYOK vault
```

Principles:
1. **The platform API is the agent surface.** Define each business operation once (Rust types to JSON Schema 2020-12 via `schemars`), then generate the REST/OpenAPI binding, the MCP tool and the A2A skill. Operation metadata (`effect`, `idempotent`, `risk`, `approval`, `dry_run`) feeds MCP annotations, policy and docs. MCP annotations are advertised but never trusted for enforcement.
2. **Agents are principals**, delegated (on behalf of a user) or autonomous (service identity with explicit roles). Tokens carry scopes derived from operation metadata; tool lists are filtered per principal.
3. **Write path = propose, validate, approve, commit**, with digest-bound `plan_id`, idempotency keys, and drafts by default.
4. **AI is a replaceable service tier.** `LlmProvider` for chat/tools/structured output/vision/embeddings; per-tenant routing table (task to model), key vault (envelope-encrypted, never logged), budgets, egress policy by data class. Self-hosted profile = Postgres + pgvector + Ollama or vLLM + an OCR sidecar, fully offline.
5. **Evals are a product feature**: golden sets for extraction (field F1, coverage at target error), semantic-layer queries (execution accuracy), and tool-safety red-team cases (indirect prompt injection in attachments). Model or prompt changes are gated by evals in CI.

Candidate data-model entities (proposed names): `ai_provider`, `ai_credential` (encrypted BYOK), `ai_model_route`, `ai_agent`, `ai_skill`, `ai_source`, `ai_chunk` (record_ref, acl_hash, halfvec, model_id, dim), `ai_conversation`/`ai_message`, `ai_run`/`ai_tool_call`, `ai_proposal` (plan, diff, digest, status), `ai_approval`, `idempotency_record`, `ai_field_def`/`ai_field_value` (provenance, staleness), `extraction_job`/`extraction_field` (value, bbox, confidence channels, validator results), `anomaly`, `forecast_run`, `eval_set`/`eval_result`, `ai_usage_ledger`, `agent_principal`, `oauth_client`.

Odoo-parity priorities (proposal): P0 = Ask AI palette, agents with skills and sources, MCP server, document digitization with earned auto-post. P1 = AI fields (on-demand or job-computed, with provenance; never overwrite human edits) and the manager/worker AI action (worker is a normal policy-checked command). P2 = folder-prompt document sorting, email/live-chat agents, voice.

---

## 15. Confidence & gaps

- **High [V]**: MCP 2026-07-28 content, rmcp version and claim, A2A version/features, Odoo 19/20 documented AI features, Odoo API facts, D365 and NetSuite MCP docs, GitHub metadata, pricing tables, HF licences.
- **Medium**: SAP details (press-release language, not product docs); all arXiv figures (preprints with 26xx identifiers, some synthetic data, unreviewed); vendor claims on cost and accuracy; EU AI Act dates read from one tracker page.
- **Not verified / gaps**:
  - No web search engine; SAP and NetSuite marketing pages 403. Not found: SAP AI-unit pricing, NetSuite Next/SuiteAgent, Zoho Zia LLM specs, D365 agent catalogue (404).
  - Odoo's AI code is proprietary, so internals (prompt design, chunking, tool-call loop) are unknown; the LLM providers Odoo 20 supports are undocumented.
  - Odoo 20 exact GA day not confirmed (release notes say September 2026).
  - Idempotency-Key draft-07 body was read during verification (422 for a reused key with a different payload, 409 for an in-flight duplicate); it is still an expired Internet-Draft, not a standard.
  - Postgres RLS interplay with ANN indexes, Chronos-2 via ONNX in Rust, MinerU/marker weight licences, EU e-invoicing mandate dates, ACFE fraud statistics, self-hosted GPU costs: all unverified.
  - EU AI Act: the instrument that moved the high-risk dates (2027-12-02 / 2028-08-02) was still not identified on 2026-10-01 (the tracker page does not name it; Wikipedia does not mention it); confirm on EUR-Lex before relying on it.
  - SAP AI Agent Hub general availability (announced for Q3 2026) was not confirmed as of 2026-10-01.
  - crates.io was reachable on 2026-10-01 and confirmed rmcp, rust-mcp-sdk, a2a-lf, rig-core, genai, async-openai, fastembed, ort, augurs, linfa, candle-core, burn and mistralrs versions; crates.io can lag GitHub tags (notably mistral.rs).
  - MCP client ecosystem support for 2026-07-28 is unmeasured (spec is two months old); expect clients that still speak 2025-11-25, which rmcp handles.

---

## Implications for obop

**MUST**
1. **MUST** make the platform's typed command/query API the single agent surface, with MCP tools and A2A skills generated from the same operation definitions and metadata (effect, risk, idempotency, approval, dry-run). No parallel "AI API".
2. **MUST** implement MCP `2026-07-28` (stateless, `server/discover`, MRTR, Tasks extension) using `rmcp` 3.x, and stay backward compatible with `2025-11-25` clients.
3. **MUST** use OAuth 2.1 per the MCP authorization spec (RFC 9728 metadata, RFC 8707 `resource` + audience validation, PKCE, CIMD, step-up via 403 `insufficient_scope`); no token passthrough. Static API keys only as an opt-in, scoped, expiring (max 90 days, like Odoo) fallback.
4. **MUST** make agents principals whose effective permissions are the intersection of the user (or service role) and token scopes, with per-principal tool lists; no agent path bypasses command handlers, validations or record rules.
5. **MUST** implement the write protocol: dry-run with diff, digest-bound single-use `plan_id`, mandatory idempotency key with request fingerprint on every mutating agent call, append-only audit with principal chain and trace id.
6. **MUST** ship an approval policy engine (auto / confirm / approve-by-role) with separation of duties; agents can create drafts but cannot post, pay, or change bank details without human approval.
7. **MUST** keep AI model-agnostic and BYOK: `LlmProvider` abstraction over OpenAI-compatible, Anthropic Messages and Gemini; keys encrypted per tenant; no obop-run proxy in the data path by default.
8. **MUST** support a fully self-hosted profile (Postgres + pgvector + Ollama/vLLM + OCR sidecar) with no outbound calls, and make every AI feature degrade gracefully to "off".
9. **MUST** avoid non-commercial weights and licences in shipped defaults (notably TimesFM 3.0) and record model licences in a manifest.

**SHOULD**
10. **SHOULD** answer business questions through a governed semantic layer (typed queries compiled to SQL under RLS) and return provenance with every answer; offer raw SQL only read-only on allow-listed views with timeouts.
11. **SHOULD** parse structured e-invoices and bank formats deterministically before any OCR/VLM, and run extraction through deterministic validators plus a calibrated per-field confidence (not verbalized), with earned auto-post per vendor.
12. **SHOULD** use pgvector in the primary Postgres (`halfvec` for high-dimension models), store `model_id`/`dim` per chunk, enforce ACLs on final rows with iterative scans or tenant partitioning; keep Qdrant as an adapter for very large tenants.
13. **SHOULD** build an eval harness (extraction, semantic queries, injection red-team) in CI and gate model/prompt changes on it.
14. **SHOULD** ship a usage ledger with per-agent/user/task cost and hard budget caps; rely on BYOK billing rather than an obop credit system.
15. **SHOULD** define per-data-class egress policy and warn on free/consumer provider keys; implement retention controls for conversations (Odoo uses 30 days as a reference point).
16. **SHOULD** treat document-derived payee, bank and amount fields as untrusted and require corroboration or human approval before any payment-affecting action.
17. **SHOULD** do forecasting with statistical baselines first, behind a `Forecaster` trait; Chronos-2 (Apache-2.0) as the optional model sidecar.
18. **SHOULD** make the HR and credit-scoring features AI-optional per module, with logging and human oversight, in view of EU AI Act Annex III.

**COULD**
19. **COULD** add an A2A endpoint (Rust `a2a-lf`) for cross-company procurement/sales flows after the MCP surface is stable; follow Agentic Commerce Protocol and UCP once e-commerce lands.
20. **COULD** front MCP/A2A with `agentgateway` (Rust) for central policy, rate limits and observability in multi-tenant hosting.
21. **COULD** publish MCP Apps (interactive UI inside third-party clients) for approvals and charts, as NetSuite's 2026.1 sample does.
22. **COULD** embed `mistral.rs` as an optional in-process engine for small single-node deployments; keep vLLM/Ollama as the documented default.
23. **COULD** offer "Skills over MCP" style packaged workflows (month-end close, three-way match) as distributable, versioned skills.

---

## Verification log

Independent adversarial check on 2026-10-01. Method: direct fetches of primary sources (MCP spec pages and schema, GitHub REST API, crates.io API, Hugging Face API, IETF archive, Odoo docs repo on branches 19.0/20.0, Microsoft Learn, Oracle docs, SAP News Center, vendor pricing pages, arXiv abstracts). No web search engine was available to the checker either, so discovery was by direct URL. Where a different primary source from the author's was available it was used (crates.io for rmcp, HF API for licences, Odoo pricing HTML, IETF archive text, the SAP keynote article).

**Confirmed count**: all 11 load-bearing claims flagged by the author were confirmed in substance (MCP 2026-07-28 spec; rmcp 3.5.0 and licence transition; MCP authorization MUST/SHOULD language; A2A v1.0.0/v1.0.1, bindings and `a2a-rs`; Odoo addons/ai 404 on 19.0/20.0, Custom plan "Agentic AI", pgvector, IAP credits; Odoo JSON-2 transaction, 3-month keys, Odoo 22 removal, Custom-plan API; Odoo 20 MCP endpoint, MCP-scope key, five non-read-only tools; D365 security model, allowlist, 0.1 credit, static-server retirement; NetSuite auth, role, concurrency, 21/7-day logs; TimesFM 3.0 licence and Chronos-2 Apache-2.0; arXiv 2609.20110 figures), several with the nuances listed below. In addition about 170 discrete secondary facts (versions, dates, stars, licences, prices, benchmark numbers, regulatory dates) were confirmed as stated (count approximate, per discrete fact).

**Corrections (was / now / source)**

1. Odoo 20 MCP default-exposed tools: was "Get Models, Get Fields, Retrieve initial context" / now five: Get Fields, Get Models, MCP Retrieve initial context, Search, Read group; others hidden until exposed; the Readonly column is a "suggested" status / https://github.com/odoo/documentation/blob/20.0/content/applications/productivity/ai/mcp_server.rst
2. Odoo RPC removal: was only "Odoo 22 (fall 2028)" / now also: `db` RPC service already removed in Odoo 20, `common`/`object` removed in Odoo 22 and Online 21.1 (winter 2027) / https://github.com/odoo/documentation/blob/20.0/content/developer/reference/external_rpc_api.rst
3. Odoo pricing: was "EUR 35.90 vs 19.90" unqualified / now flagged as first-12-month discounted prices with crossed-out list prices 44.90 / 24.90 (toggle mapping inferred from HTML, (I)) / https://www.odoo.com/pricing
4. pgvector/PostgreSQL: was "extension available for PostgreSQL 15+" stated as fact / now attributed to Odoo's docs, pgvector README says Postgres 13+ / https://github.com/pgvector/pgvector
5. SAP Sapphire date: was 2026-05-13 / now keynote 2026-05-12, article dated 2026-05-13 / https://news.sap.com/2026/05/sap-sapphire-keynote-business-ai-platform-power-autonomous-enterprise/
6. SAP source attribution: Joule Work MCP/A2A quote, AI Agent Hub "built on SAP LeanIX", "GA in Q3", "no additional charge" were cited to the Q2 highlights page but appear only in the keynote article; the supported-model list was cited to the Anthropic press release but appears only in the Q2 highlights page / URLs as given in section 2.2
7. SAP "80% accuracy" quote: was "CTO-level quote" / now CEO Christian Klein, keynote / same keynote URL
8. SAP-Anthropic wording: was "primary reasoning capability" / now "a primary reasoning and agentic capability" / https://news.sap.com/2026/05/sap-anthropic-to-bring-claude-sap-business-ai-platform/
9. D365 pricing/allowlist: added that Finance Premium and Supply Chain Management Premium licensed users are exempt from the 0.1-credit charge outside Copilot Studio, and that the default allowlist also includes "Finance Agent (Sydney)" / https://learn.microsoft.com/en-us/dynamics365/fin-ops-core/dev-itpro/copilot/copilot-mcp , .../mcp/mcp-security
10. NetSuite role wording: was "admin-enabled; docs advise non-administrator role" / now integration record is auto-created on first Claude connection, admin must enable it for other clients, non-admin role advice is from the ChatGPT steps / https://docs.oracle.com/en/cloud/saas/netsuite/ns-online-help/section_0714082142.html
11. Headline 2: was "Every major ERP ... (D365, NetSuite, Odoo 20, Frappe)" / now Frappe is community/experimental only and SAP's ERP-level MCP is unverified / sections 2.4-2.5 and https://github.com/frappe/mcp
12. Frappe/ERPNext: was "no official AI feature found" / now the ERPNext docs intro has none, but `frappe/flow_client` (AI agents for Frappe, AGPL-3.0, 76 stars, created 2026-06-02) and `frappe/otto` exist / https://github.com/frappe/flow_client
13. LazyChat: was "101 tools" / now README gives both 95 (headline) and 101 (catalog); 20 stars, MIT / https://github.com/soumyasethy/lazychat-erpnext
14. MCP client registration order: was "CIMD (SHOULD), pre-registration, DCR" / now SHOULD-order is pre-registered, then CIMD, then DCR, then prompt user / https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/client-registration
15. MCP `requestState`: was unconditional "MUST protect with HMAC/AEAD" / now MUST when it influences authorization, resource access or business logic; single-use MUST only where one-time redemption is required / https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns/mrtr
16. Idempotency-Key semantics: was "422/409 (M)" / now verified: 422 for a reused key with a different payload, 409 for an in-flight duplicate, both SHOULD, draft-07 expired / https://www.ietf.org/archive/id/draft-ietf-httpapi-idempotency-key-header-07.txt
17. Prompt-injection preprint 2609.34245: was described as payment-fraud evidence / now its empirical worlds are sanctions-designation data and software supply-chain provenance; invoice payment is only an example; title "Certified Multi-Source Integrity for Structured Agent Actions" / https://arxiv.org/abs/2609.34245
18. EntSQL 15.9%: was "when private business knowledge is required" / now "on English inputs when long-form documents are supplied" / https://arxiv.org/abs/2606.03363
19. DABStep scaffolding effect: was "0-15 points" / now about 0-5 on the two flash-tier models and 14-15 on the two frontier models / https://arxiv.org/abs/2609.22259
20. Schema compression: was recall 0 to 56.5% alone / now adds that end-to-end execution accuracy rose only 1.8-1.9 points and that the "representation not reasoning" line is the authors' claim / https://arxiv.org/abs/2606.28601
21. DocILE AUC 0.928: was "a two-call disagreement engine" / now a multi-signal classifier (ExtractConf) of which disagreement is one input / https://arxiv.org/abs/2606.24420
22. Extractor routing: was "cheap-then-strong" / now pre-inference routing, within 0.02 F1, needs retraining per dataset / https://arxiv.org/abs/2608.06607
23. Spider 2.0 "plain baselines near zero": now classic DIN-SQL/CHESS/DAIL-SQL + GPT-4o pipelines score 0.00-2.20 on Snow / https://spider2-sql.github.io/
24. Docling: was v2.131.0 / now v2.132.0 (published 2026-10-01) / https://github.com/docling-project/docling/releases
25. Licence names (crates.io): candle, burn, `augurs`, `linfa`, `genai` were "Apache-2.0" / now "MIT OR Apache-2.0" / https://crates.io/crates/candle-core and the other crate pages
26. mistral.rs: GitHub v0.9.4 is right, but crates.io `mistralrs`/`mistralrs-core` is still 0.8.1 (2026-04-02); added / https://crates.io/crates/mistralrs
27. TimesFM: licence named `timesfm-non-commercial-license-v1.0`, commercial use via authorized Google Cloud services (BigQuery ML and Vertex Model Garden), release 2026-08-28 / https://github.com/google-research/timesfm
28. `augurs` outlier detection: was "(M)" / now verified (`augurs-outlier`) / https://github.com/grafana/augurs
29. Section 15 claim "crates.io API was unreachable": was true for the author, but the API is reachable; versions re-verified there / https://crates.io/api/v1/crates/rmcp
30. Added facts: rust-mcp-sdk is at v2.0.0; agentgateway v1.5.0 is latest stable (v1.6.0 alphas exist); a2a-lf is 0.4.1; ACP spec is `beta` and maintained by OpenAI and Stripe, UCP has a multi-company council; Sonnet 5's $2/$10 is now permanent (planned rise to $3/$15 cancelled) / respective repos and https://platform.claude.com/docs/en/about-claude/pricing

**Unresolved / unverified**

- EU AI Act: the amending instrument that moved high-risk dates to 2027-12-02 / 2028-08-02 was not identified (tracker page updated 2026-08-31 lists the dates but names no instrument). Dates themselves match the tracker.
- SAP AI Agent Hub: GA (announced for Q3 2026) not confirmed.
- Odoo 20 exact GA day and IAP credit price: not found. Odoo pricing toggle-to-price mapping is inferred from page HTML.
- Zoho "500+ integrations" is a marketing figure (the page names about 32 apps); Zoho and SAP sovereignty claims remain vendor claims.
- EU e-invoicing mandate dates, ACFE fraud statistics, self-hosted GPU costs, Postgres RLS behaviour with ANN indexes, Chronos-2 via ONNX in Rust, MinerU/marker weight licences, AP2 and x402: not checked (still unverified).
- `rosenvladimirov/odoo-claude-mcp` "197+ tools" is the project's own README claim (not inspected); Frappe Assistant Core's README says both "24 built-in tools" and "22 tools across four plugins".
- SAP and NetSuite marketing pages (403 for the author) were not retried; Spider 2.0 leaderboard entries carry no dates.
- Sections 5, 13 (worked estimates), and 14 are design proposals [I]. The cost-table arithmetic was recomputed: the Sonnet 5.5 rows and the 3.1 Flash-Lite invoice row hold; the Flash-Lite chat-turn figure (~$0.0045) only reproduces if Gemini cached reads cost about 10% of input, a rate that was not checked (unverified).

- Owner-constraint pass (2026-10-01): the owner fixed `vp` (Vite+) plus pnpm as the only JavaScript tooling (no npm, npx, yarn or bun). The one `npx` hit (Odoo 20 MCP docs telling clients to bridge via `npx mcp-remote`, section 2.1) stays as a factual description of Odoo and now says obop would show `vp dlx mcp-remote` (`vp dlx` runs a package binary without adding it to the project, viteplus.dev guide "Package Management") or omit the bridge. No npm trusted publishing, 2FA or npm CLI version material exists in this file.
