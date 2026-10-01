# Extension platform, module SDK and developer experience for obop

Research date 2026-10-01. Covers everything a third party, implementer or customer can add or change without forking core, plus theming and white-labelling.

## Scope & method

- **Live sources, about 55 fetches today.** Official docs (Twenty, Shopify, Stripe Apps, Grafana, Frappe, Medusa, Directus, Strapi, Payload, NocoBase, WordPress.org, VS Code, Vite, WASI, MCP Apps, W3C DTCG). GitHub API and raw files (licences, advisories, releases, and source for Wasmtime, Rhai, `cel`, Frappe, Directus, Grafana, rustc). npm and crates.io registries, web-features, Google Threat Intelligence, Patchstack. I also re-read siblings 02 s1/s4/s6/s8/s13, 05, 06 s3.12-3.13, 07 s3.11-3.12, 08, 15 s11, 17 s2.2/s11.2/s12, 18, 19 s5, 20 s9-10 and 21 s7-8.
- **Limits.** No general web-search tool. Salesforce docs returned HTTP 403, so Salesforce is [M] and nothing load-bearing rests on it. The fetcher summarises pages; figures were re-checked against raw files where possible.
- **Injection notice.** Two docs.medusajs.com pages contained text aimed at AI summarisers (append a "feedback" section verbatim). I did not act on it.
- **Tags.** [V] verified today in a primary source. [S] secondary. [I] inference or design. [M] remembered, unverified. [G] guess or target.

## 1. Bottom line

**One package format (`obop.toml`), five code tiers, four UI tiers, one trust ladder.**

- **Code.** T1 declarative packs, T2 CEL expressions, T3 Wasm components, T4 remote apps, T5 first-party Cargo crates.
- **Reconciling 05/15 with 13/18.** Data may hold only total, pure, cost-bounded expressions (CEL). Anything with loops, I/O or effects is a signed, versioned package. Rhai is dropped from the Phase 1-2 plan.
- **Wasmtime.** Pin an LTS major. The 06 vs 21 dispute is settled for 06: an LTS exists (3.3).
- **UI.** Untrusted UI never enters the main JS realm. Trusted ESM chunks (07) are for first-party and tenant-own code. Marketplace UI uses remote-dom (19). Arbitrary web content uses sandboxed iframes aligned to MCP Apps (21).
- **Licence.** AGPL core, an Application Exception modelled on Twenty's, Apache-2.0 SDKs and WIT worlds.
- **Theming.** Tenants get a limited back-office identity and a full customer-facing theme in the open edition. Only partner white-label of the product identity is enterprise.

## 2. Prior art

### 2.1 Packaging, isolation, compatibility, review

| System | Package and deploy | Isolation of third-party code | Compat, review, licence |
|---|---|---|---|
| **Odoo + OCA + Studio** | Python dir with `__manifest__.py`, `-u` per DB (02). Studio exports a `studio_customization` ZIP with per-model Updatable/Demo flags [V] https://github.com/odoo/documentation/blob/20.0/content/applications/studio/models_modules_apps.rst | None: in-process, full DB access. Odoo Online takes only data-only third-party modules, i.e. no Python files [V] https://apps.odoo.com/apps/faq | No API promise: the ORM changelog lists breaks in every major and SaaS minor (`Model._table_query` removed in 20.0; ACLs merged with record rules in 19.4) [V] https://github.com/odoo/documentation/blob/20.0/content/developer/reference/backend/orm/changelog.rst . OpenUpgrade 20.0 holds only an initial commit, dated 2026-09-28 [V] https://github.com/OCA/OpenUpgrade/commits/20.0 . Apps store: about 93,600 listings (93,613 in 03; 93,630 "Apps found" on 2026-10-01 [V] https://apps.odoo.com/apps/modules/browse), 30% commission (25% IAP), payout once unredeemed sales reach EUR 400 [V] https://apps.odoo.com/apps/faq . LGPL-3 permits OPL-1 proprietary modules (17) |
| **Frappe** | Python app (`hooks.py`, `patches.txt`, fixtures) per site via `bench` [V] https://docs.frappe.io/framework/user/en/basics/apps . Custom Field, Property Setter, Client/Server Script rows (08) | Apps: none. Server Script: RestrictedPython, off by default since v15, not allowed on public Frappe Cloud benches [V] https://docs.frappe.io/framework/user/en/desk/scripting/server-script | Parallel v15/v16 lines (05). Framework MIT, ERPNext GPL-3 (17). Marketplace rules not verified |
| **Twenty** | TS project: `defineApplication/defineObject`, logic functions, front components; npm (`twenty-app` keyword) or tarball; registration per server, install per workspace [V] https://docs.twenty.com/developers/extend/apps/publishing | Front components: remote-dom in a Web Worker inside a sandboxed opaque-origin iframe [V] https://docs.twenty.com/developers/extend/apps/front-components . Logic functions run in isolated Node.js processes, but the self-host drivers are Disabled (production default), Local ("no sandboxing", development only) and Lambda (hardware-level isolation, for untrusted code) [V] https://docs.twenty.com/developers/self-host/capabilities/setup . Effective permission = user role intersected with app role [V] | `engines.twenty` semver gate, downgrades rejected. Releases every few days (2.44.0 on 2026-10-01, 2.43.0 on 09-28) [V]. **No publish review described**; "Vetted" badge only for Twenty-maintained apps; claiming an app proves authorship via GitHub provenance and does not review code [V]. AGPL + Application Exception; `twenty-sdk`, `twenty-client-sdk`, `create-twenty-app`, `twenty-shared`, `twenty-ui` and `packages/twenty-apps` are MIT [V] https://raw.githubusercontent.com/twentyhq/twenty/main/LICENSE |
| **Medusa 2.21** | npm plugin bundling modules (DML models + service), workflows (steps with compensation), routes, admin extensions [V] https://docs.medusajs.com/learn/fundamentals/modules | Logical only: modules cannot reach other modules' services or data models, so no cross-module FKs; Links + Query [V] https://docs.medusajs.com/learn/fundamentals/modules/isolation ; same process [I] (not stated in the pages read) | MIT + `ENTERPRISE-LICENSE.md` [V] https://raw.githubusercontent.com/medusajs/medusa/develop/LICENSE |
| **Directus 12** | npm extensions: app (interface, display, layout, panel, module, theme), api (hook, endpoint, operation), bundles | Default in-process ("execute code inside the Directus server") [V] https://directus.com/docs/guides/extensions/overview . Opt-in isolated-vm sandbox (`isolated-vm` dependency, `EXTENSIONS_SANDBOX_*` settings); `MARKETPLACE_TRUST` defaults to `sandbox` [V] https://github.com/directus/directus/blob/main/packages/env/src/constants/defaults.ts | Relicensed BSL-1.1 (2023-04-26) to Monospace Sustainable Core License 1.0 / MSCL-1.0-GPL (commit 6f262df, 2026-05-28, "Add licensing (#27417)"): Competing Use ban, licence-key clause, GPL-3.0 grant from the fourth anniversary of each version's release [V] https://github.com/directus/directus/blob/main/license . The 2026-04-17 licence commit was still BSL |
| **Strapi 5 / Payload 3 / NocoBase 2.2** | npm plugins. Payload plugin is `(config) => config`. NocoBase Plugin Manager enables/disables at runtime from `storage/plugins` [V] https://docs.nocobase.com/plugin-development | All in-process | Strapi MIT + `ee/`; Payload MIT [V]. NocoBase: custom agreement dated 2026-02-24 plus `LICENSE-APACHE.txt`; GitHub says NOASSERTION, npm says Apache-2.0: inconsistent, ask counsel [V] |
| **Shopify** | Remote app + extensions in `shopify.app.toml`; Functions are Wasm; UI extensions are Preact on Polaris web components | Functions: fixed limits 256 kB binary, 10,000 kB linear memory, 512 kB stack; dynamic limits 11M instructions, 128 kB in, 20 kB out, stated for carts up to 200 lines and scaling proportionally above that [V] https://shopify.dev/docs/api/functions . UI: Web Worker, 64 KB bundle cap (128 KB for full-page customer account extensions), opt-in `api_access`/`network_access` [V] https://shopify.dev/docs/api/checkout-ui-extensions | Each stable API version supported at least 12 months, new version quarterly [V] https://shopify.dev/docs/api/usage/versioning ; the CLI blocks deploys of Polaris-based UI extensions targeting versions older than 12 months. Review: Lighthouse drop at most 10 points, billing API, session tokens [V] https://shopify.dev/docs/apps/launch/app-requirements-checklist . Revenue share 0% on the first USD 1M of gross app revenue (earned from 2025-01-01), then 15%; USD 19 one-time fee per Partner account; 2.9% processing plus sales tax. The 0% tier does not apply to developers with USD 20M+ prior-year App Store earnings or USD 100M+ company revenue (15% from the first dollar) [V] https://shopify.dev/docs/apps/launch/distribution/revenue-share |
| **Salesforce** [M] | Managed packages (namespace, 2GP), Apex/LWC | Shared runtime with per-transaction governor limits [M] | Security review for listing [M] |
| **WordPress** | Zip plugin, GPL-compatible required | None | Guidelines: no obfuscation, no trialware, no externally loaded executable code; insecure plugins closed [V] https://developer.wordpress.org/plugins/wordpress-org/detailed-plugin-guidelines/ |
| **VS Code** | VSIX via Marketplace | Separate extension-host process, "the same permissions as VS Code itself" [V] https://code.visualstudio.com/docs/configure/extensions/extension-runtime-security | Multi-engine malware scan, clean-room run, signatures, secret scanning, block list with auto-uninstall, verified publisher (domain + 6 months good standing) [V] |
| **Grafana** | Plugin (frontend + optional Go backend), signed `MANIFEST.txt` with SHA-256 per file; private plugins need `rootUrls` [V] https://grafana.com/developers/plugin-tools/publish-a-plugin/sign-a-plugin | Frontend sandbox is opt-in: "By default, the sandbox is disabled for all plugins"; public preview since Grafana 11.5 [V] https://grafana.com/docs/grafana/latest/administration/plugin-management/plugin-frontend-sandbox/ | `grafanaDependency` semver range in `plugin.json` [V]. Public plugins reviewed, then signed Community/Commercial. AGPL-3.0 core |
| **Stripe Apps** | `stripe-app.json` + React views using only Stripe components, uploaded to Stripe's CDN | Sandboxed iframe; host deserialises and renders the tree; no arbitrary HTML or refs; host owns React 17.0.2 [V] https://docs.stripe.com/stripe-apps/how-ui-extensions-work | Review about 4 business days, one app per account [V] https://docs.stripe.com/stripe-apps/publish-app . `@stripe/ui-extension-sdk` 9.4.0 (npm, 2026-09-25) |

### 2.2 Failure history

- **WordPress.** Patchstack's 2025 data: 11,334 new vulnerabilities (+42%), 91% in plugins, 46% unpatched at disclosure, about half of high-impact ones exploited within 24 h [S] https://patchstack.com/whitepaper/state-of-wordpress-security-in-2026/ .
- **Frappe.** 41 advisories published 2026-05-18 to 2026-10-01 (`gh api` security-advisories; recounted), among them critical SSTI RCE GHSA-qq98-vfv9-xmxh (2025-12-29) and Server Script bypass GHSA-v3vh-7qx4-f582 (2023-12-18) [V].
- **VS Code.** GlassWorm, a self-propagating campaign on Open VSX and VS Code (reported 2025-10-20; follow-ups 2026-01-31 and 2026-04-25; CrowdStrike, Google and Shadowserver reported a botnet takedown on 2026-05-26) [S] via the HN index of Koi, Socket and CrowdStrike posts https://www.crowdstrike.com/en-us/blog/inside-crowdstrike-takedown-of-a-developer-targeting-botnet/ .
- **Salesloft Drift (remote-app precedent).** Stolen OAuth tokens of a marketplace app reached many Salesforce orgs from as early as 2025-08-08 through at least 08-18; tokens were revoked and AppExchange pulled the app on 08-20 [V] https://cloud.google.com/blog/topics/threat-intelligence/data-theft-salesforce-instances-via-salesloft-drift .
- **Licence churn.** Directus (above), NocoBase inconsistency, Cal.com and others (17).

### 2.3 What it teaches

1. **In-process plugins give the richest ecosystem and the worst security record.** 17 adds a licence reason: in-process Rust crates would be AGPL derivatives.
2. **Hosts that own the UI runtime** (Stripe, Shopify, Twenty) **get accessibility, theming and security, and pay in author ergonomics.** Twenty front components cannot use portals, `ResizeObserver` or document listeners. Stripe does not support refs and tells authors to avoid controlled inputs because of input lag [V] https://docs.stripe.com/stripe-apps/how-ui-extensions-work .
3. **Shipped data must be immutable, with customer deltas stored separately** (Frappe Property Setter, Odoo `noupdate`, 08).
4. **Capability declaration at install works** (Shopify opt-in capabilities, Stripe `connect-src` with a purpose string, Twenty's role intersection).
5. **Code-in-DB scripting is a recurring hazard** (Frappe default-off since v15).
6. **Distribution needs automated gates plus a cheap manual tier** (VS Code's stack; Twenty's no-review model is the cautionary case).
7. **Compatibility is a product** (Shopify's enforced 12-month windows, Twenty's `engines` gate). Odoo's lack of either created OCA and OpenUpgrade.

## 3. Code tiers

| Tier | What | Trust and runtime | In-transaction? | Author | Phase |
|---|---|---|---|---|---|
| **T1 Declarative pack** | Fields, models, view patches, automations, print and report templates, dashboards, import mappings, themes | Data only, validated against the registry | Yes (declared actions) | Admin, implementer | 1 |
| **T2 CEL** | Conditions, formulas, visibility, validation, defaults, rollup filters | Pure, total, metered, no I/O; Rust on the server, Wasm build on the client | Yes | Admin, implementer | 1 |
| **T3 Wasm component** | Hooks, computed fields, validators, actions, HTTP connectors | wasmtime, capability manifest, fuel + epoch + memory limits | Sync hooks: yes, hard budget. Async: no | Developer | 2 |
| **T4 Remote app** | Any language or infra, external SaaS, AI agents | Zero trust in vendor code; OAuth 2.1, webhooks, REST, MCP | No | Vendor, partner | 1 |
| **T5 Native crate** | Posting, tax, payments core, protocol adapters | In-process Rust, AGPL, reviewed like core | Yes | obop team | 0 on |

Numbering: 21 s7.2 calls native Tier 0, Wasm Tier 1, remote Tier 2. Here T5 = 21's Tier 0, T3 = Tier 1, T4 = Tier 2.

**When each applies.** T1/T2 for anything one record or a declared rollup can express. T3 for list algorithms, multi-record validation inside the commit, or a connector with field mapping to one or two allow-listed hosts. T4 for your own runtime, database, long jobs, UI, or a SaaS vendor. T5 only if you are the project.

### 3.1 T1 declarative packs

- **Contents** (versioned JSON, JSON Schema from schemars, 19 s5.1): `models` and `extend` (fields on existing models) including a `rollup` kind (sum/count/min/max over a child relation with a CEL `where`, as NocoDB treats rollups as first-class, 08); `views` (patches by slot or node id, not XPath); `menus`, `actions`; `automations` (trigger, CEL `when`, action vocabulary `set/create/update/notify/webhook/enqueue/call-component`); `templates` (minijinja for email, Typst for PDF; minijinja has `set_fuel` (behind the optional `fuel` cargo feature) and `set_recursion_limit` [V] https://github.com/mitsuhiko/minijinja/blob/main/minijinja/src/environment.rs ); `dashboards`, `import_mappings`, `seed` data, `themes`.
- **Studio parity.** A tenant's own customisations are a package with `origin = tenant`, same format. Export is "Studio export" (a paid Odoo Studio feature: installing it on the Standard plan triggers an upsell to Custom, and no Studio addon is in the Community repo [V] https://github.com/odoo/documentation/blob/20.0/content/applications/studio.rst ); dev-to-prod promotion is `obop ext apply`.
- **Hosted SaaS.** Packs are the only third-party tier allowed on shared cloud without review, mirroring Odoo Online's data-only rule (03).

### 3.2 T2 expressions and scripting: the settlement

**Decision: CEL is the only expression language stored in data. Rhai is not shipped in Phase 1-2. Code is Wasm.**

| Source | Position | Resolution |
|---|---|---|
| 05 | Sandboxed scripting (Rhai or Wasm) so implementers customise without recompiling | Met by T1/T2 for configuration and T3 for code |
| 15 s11 | Rhai 1.26.1 for quick scripts, wasmtime for untrusted modules | Rhai overruled |
| 13 ("MUST NOT embed arbitrary code in the database"), 18 (T13: no arbitrary code in data) | CEL for conditions and formulas, Wasm for complex algorithms | Adopted as the rule |

**Rule.** A stored expression is data only if it always terminates, has no side effects or I/O, type-checks against the registry at save time, and is metered. Loops, I/O and effects fail; CEL macros (`all`, `exists`, `map`, `filter`) pass.

**Why not Rhai in the DB.**

- It is Turing-complete code in data. Its limit API exists (`set_max_operations`, `set_max_call_levels`, `set_max_expr_depths`, `set_max_string_size`, `set_max_array_size`, `set_max_map_size`, `set_max_variables`, `set_max_functions`, `set_max_modules` [V] `src/api/limits.rs`), but obop would own a bespoke language no Odoo or Frappe implementer knows.
- Frappe's experience: default-off since v15, excluded from public benches, RCE-class advisories.
- T3 gives a packaged, signed, versioned, testable artifact with a capability manifest.
- Cost: no paste-a-script Studio moment. Mitigation: `obop ext new --template action` plus hot reload, aiming under 5 s edit-to-effect [G].
- **Escape hatch (COULD).** Rhai inside a signed pack (not DB-editable), tenant flag default off, Phase 3+ only if demand is measured.

**CEL facts.** `cel` 0.14.5 (2026-09-07), MIT, MSRV 1.86, `regex` and `chrono` on by default and `json` opt-in, a conformance harness against cel-spec with a tracked ignore list, host functions via `Context::add_function` [V] https://github.com/cel-rust/cel-rust . **The README documents no cost or step limit.** obop must add, with a Phase 1 spike: save-time limits (4 KB source, AST depth 32), a runtime step budget plus wall-clock deadline (hook or fork), and only host-registered functions (`today()`, `company()`, `user()`). Domains stay JSON AST (SQL-compilable, 19); CEL is for modifiers, conditions and formulas. The client runs the same Rust evaluator compiled to Wasm (`@obop/domain`, 07 s3.11) so there is one semantics [I].

### 3.3 T3 Wasm components

**Worlds give capability by construction** (a component cannot import what its world lacks):

| World | Imports | Exports | Runs |
|---|---|---|---|
| `sync-hook` | `records`, `kv`, `log`, `clock` (no `http`, no `secrets`) | `on-event`, `compute`, `validate` | Inside the DB transaction |
| `async-hook` | sync imports + `http`, `secrets`, `queue` | `on-event`, `action` | Job queue, after commit |
| `connector` (21 s7.2) | `http` (allow-listed, SSRF-filtered), `secrets`, `kv`, `log`, `clock` | `spec`, `check`, `discover`, `read`, `write`, `handle-webhook` | Job queue |

**Host functions (`obop:ext/*@1`).** `records.search/read/create/write/unlink` with permission = triggering user's access intersected with the app role (Twenty's model [V]; running as the app is an explicit, audited scope); `rpc.call` to allow-listed methods; `http.fetch`, `secrets.get`, `kv.*`, `queue.enqueue`, `log.emit`, `clock.now`, `rand.bytes`, `i18n.t`. No WASI filesystem or sockets; since Wasmtime 48 the `wasmtime-wasi` crate's default configuration denies TCP/UDP socket creation (an embedder-crate default, not an engine property) [V] https://github.com/bytecodealliance/wasmtime/releases/tag/v48.0.0 . Guests therefore do not depend on the host's WASI minor.

**Capability manifest** (consent screen, diffed per upgrade; new scope or host forces re-consent):

```toml
[capabilities]
records.read  = ["sale.order", "sale.order.line", "res.partner"]
records.write = ["loyalty.ledger"]
hooks         = [{ on = "sale.order:after_confirm", mode = "sync" }]
egress        = []            # each host needs a purpose string (Stripe does this)
[limits]
fuel = 20_000_000   # deterministic metering
wall_ms = 50        # epoch deadline; sync hard max 250
memory_mb = 32
```

**Limits** use APIs confirmed in Wasmtime source [V]: `Config::consume_fuel`, `Store::set_fuel`, `Config::epoch_interruption`, `Store::set_epoch_deadline`, `Config::max_wasm_stack`, `Store::limiter` with `StoreLimitsBuilder::{memory_size, table_elements, instances}`.

**Never rely on fuel alone.** The 2026-09-24 advisories include fuel-accounting bypass via `call_ref` and exception `catch` (GHSA-m63x-6p34-q65x) and allocation past the hostcall fuel limit (GHSA-jqpg-j7w6-42pr) [V]. Layer epoch deadlines, StoreLimits, per-tenant concurrency caps, and an `ext` process role of the single binary (02 #14) under cgroups and seccomp or Landlock. Phase 2 may run in-process, but the role split exists from day one [I].

**Wasmtime version policy (06 vs 21).**

- **Rule verified today.** "Each release that is a multiple of 12 is considered an LTS release", supported 24 months (normal releases 2 months), two LTS lines at any time, security fixes on every supported line, other fixes best effort, majors on the 20th monthly [V] https://raw.githubusercontent.com/bytecodealliance/wasmtime/main/docs/stability-release.md .
- **Verdict.** 06 is right: v36 and v48 are LTS. 21's "no LTS policy stated" is wrong. v36.0.0 was released 2025-08-20 and v48.0.0 on 2026-08-20 [V] https://github.com/bytecodealliance/wasmtime/releases ; by arithmetic [I] v36 is supported to about 2027-08 and v48 to about 2028-08, and the next LTS is v60 around 2027-08-20. Latest is 49.0.1 (2026-09-24); the same day shipped LTS patches 48.0.3 and 36.0.16 [V]. v24.0.0 was 2024-08-20, so the v24 LTS line reached its 24-month end around 2026-08-20 [I].
- **Advisory load.** Page 1 of 6 of the advisory list alone has ten advisories from 2026-06-15 to 09-24, including one High (GHSA-vqjp-4c8c-hfgg, filesystem sandbox escape, 2026-08-20) [V]. Plan for patching in days.
- **obop policy.**
  1. Each obop release line pins one Wasmtime **LTS** major (48.x now) and takes 48.0.y patches within 7 days of an advisory, 24 h for High.
  2. One `obop-wasm-host` crate wraps Wasmtime so the yearly LTS-to-LTS bump touches one crate.
  3. obop's 5-year support goal (17 s7) exceeds Wasmtime's 24-month LTS, so each obop LTS branch rebases onto the next Wasmtime LTS mid-life. Guests see only `obop:ext@1.x`, so this is invisible to them.
  4. No tracking of monthly majors in production.

**Authoring toolchain today.**

- Rust: `wasm32-wasip2` is Tier 2 on stable. **`wasm32-wasip3` is Tier 2 (promotion commit 2026-08-10) but first on stable in Rust 1.100.0**, so not on 1.98/1.99 [V] https://raw.githubusercontent.com/rust-lang/rust/master/src/doc/rustc/src/platform-support/wasm32-wasip3.md . Rust 1.99.0 is dated 2026-10-01 in RELEASES.md and 1.100.0 is in beta (1.100.0-beta.1, 2026-09-27) [V]; by the six-week train 1.100 should reach stable about 2026-11-12 [I]. `wit-bindgen` 0.62.0 (2026-09-10); `cargo-component` is stale (0.21.1, published to crates.io 2025-03-18, GitHub release 2025-04-07, last push 2025-07-14).
- TypeScript: `@bytecodealliance/jco` 1.35.0 (2026-09-24) and `@bytecodealliance/componentize-js` 0.23.0 (2026-09-21), whose README calls it "an experimental project, no guarantees are provided for stability or support" [V] https://github.com/bytecodealliance/ComponentizeJS . Javy (QuickJS) is active (v9.1.0 2026-07-30, pushed 2026-09-28, Apache-2.0) [V]; I did not check its component story.
- **Consequence.** Phase 2 supports Rust guests and offers TS as preview. Define worlds without `async func` first (WASI 0.3.0 was 2026-06-11, 0.3.1 2026-08-11, 0.3.2 due 2026-10-13 [V] https://wasi.dev/roadmap ) and add async once Rust 1.100 is stable. `wkg` 0.16.1 publishes components to OCI registries [V].
- **Cost.** Every host call serialises, so hooks stay coarse (ids in, write-ops out).

### 3.4 T4 remote apps

- **Install.** OAuth 2.1 (still an IETF Internet-Draft: `draft-ietf-oauth-v2-1-16`, 2026-09-03, not an RFC [V] https://datatracker.ietf.org/doc/draft-ietf-oauth-v2-1/ ; PKCE, rotating refresh tokens, exact redirects), registration by Client ID Metadata Document (also a draft: `draft-ietf-oauth-client-id-metadata-document-02`, 2026-07-06 [V]; 21 s7.3), scopes per model, action and company, a consent screen, app role intersected with the acting user's role.
- **Delivery.** Standard Webhooks, REST with `$batch`, MCP (21 MUST 1-6).
- **Drift lessons [V].** Access tokens of 1 hour or less; refresh rotation with reuse detection; optional per-install IP allow-list; connected-apps inventory with one-click revoke; a registry kill switch; per-app audit.
- **Limit.** No in-transaction hooks; eventually consistent by design.

### 3.5 T5 first-party crates

A module is a crate exposing a `Module` descriptor, composed by Cargo features through a generated module list, with a startup check that registry equals manifest (06 MUST 7). Only `*-contract` crates carry semver, checked with `cargo-semver-checks` 0.50.0. Third-party native crates are unsupported: AGPL combined works that can crash the process (06 s3.13).

## 4. Frontend extension (reconciling 07, 19, 21)

| Tier | Mechanism | Who ships | Isolation | Shared deps | a11y duty | Tokens |
|---|---|---|---|---|---|---|
| **F0** | Declarative view patches, slots, built-in widgets (19 s5.4a) | Anyone | n/a | n/a | Host | Host |
| **F1** | Prebuilt ESM chunks via `import()` + import map against the versioned UI SDK (07 s3.12) | First-party; tenant-own code on self-hosted or dedicated instances | **None** (main realm, full session access) | React, `@obop/ui-sdk`, `@obop/meta` via import map | Author, CI-checked | Inherited CSS variables |
| **F2** | remote-dom: tree built in a Worker or hidden iframe, mirrored by host-owned custom elements (19 s5.4c) | Marketplace, partners | Opaque-origin sandboxed iframe + Worker; host element allow-list | None (bundle carries own framework) | **Host renders real DOM from design-system components**; author owns labels and content | Host-applied, token-constrained style props |
| **F3** | Sandboxed iframe, arbitrary HTML, JSON-RPC over `postMessage`, aligned to MCP Apps (21) | Marketplace, remote apps | Separate-origin sandbox proxy, CSP from manifest | None | **Author**: WCAG 2.2 AA statement, axe in CI | CSS variables via init handshake |

**Why in-realm loading cannot be the marketplace path.** 07 s3.12 loads third-party ESM into the main realm; any such chunk can read the DOM, cookies and tokens. Grafana (frontend sandbox) and Directus (isolated-vm) both made sandboxing opt-in, which shows how hard retrofitting is. F1 is therefore restricted to code the operator already trusts.

**Precedent for F2/F3.** Twenty, Shopify and Stripe (2.1). `@remote-dom/core` 1.12.0 (2026-09-22, MIT); its host receiver takes an `elements` option restricting element names and members [V] https://github.com/Shopify/remote-dom . MCP Apps: spec 2026-01-26 (status Stable; SDK 2.0 keeps the same wire protocol), SDK v2.0.3 (2026-09-25), mandatory iframe sandboxing, manifest-declared CSP (`connectDomains`, `resourceDomains`, `frameDomains`, `baseUriDomains`), a sandbox proxy whose origin must differ from the host's (required when the host is a web page), and optional host-supplied CSS variables (`styles.variables`; hosts "can optionally" pass them) [V] https://github.com/modelcontextprotocol/ext-apps . Note that the spec gives the sandbox proxy `allow-scripts` and `allow-same-origin` on its own separate origin, so obop's opaque-origin rule 1 below is stricter than MCP Apps, not identical [I]. Aligning F3 lets an obop UI also render in AI chat hosts.

**Rules.**

1. **Isolation.** F2/F3 iframes use `sandbox="allow-scripts"` without `allow-same-origin` (opaque origin, no cookies or storage); iframe `sandbox` is Baseline high since 2018 [V]. Apps needing OAuth callbacks or storage declare a dedicated origin as MCP Apps allows.
2. **CSP.** Host `frame-src` lists registered ext origins only. Each app's `connect-src` is built from manifest `egress`, each with a purpose string.
3. **Shared deps.** Only F1 uses an import map (Baseline high since 2025-09-27; Chrome 89, Firefox 108, Safari 16.4 [V]). Multiple import maps are not Baseline (Chrome 133, Safari 18.4) [V], so the host emits one. Vite's `build.chunkImportMap` (added in 8.1.0, 2026-06-23; latest Vite is 8.3.2) is marked Experimental and defaults to `false` [V] https://vite.dev/config/build-options#build-chunkimportmap ; do not rely on it. React stays off the F2 wire, so obop avoids Stripe's "stuck on React 17" outcome; authors may use React, Preact, Svelte or Vue (remote-dom ships examples for all) [V].
4. **Accessibility.** F2 inherits WCAG behaviour from host components; the SDK requires `label`/`description` props and offers no free DOM. F3 publishers declare WCAG 2.2 AA, pass axe in `obop ext test`, and set an iframe `title`; the host owns focus entry and exit. The SDK must ship its own overlay primitives, because portals fail in remote renderers (Twenty: Radix and MUI popovers break [V]).
5. **Design tokens.** F1 inherits CSS variables. F2 gets a `theme` object and a token-named style API (Stripe `Box`/`Inline` model [V]). F3 gets `styles.variables` in the init handshake (MCP Apps [V]).
6. **Versioned UI SDK.** `@obop/ui-sdk` is semver; the wire protocol has its own integer version; the host supports the current and previous major for at least 24 months.
7. **Vite+ plugin.** `@obop/vite-plugin-ext` (peer `vite ^8`, used through `vp`, behind one config factory per 07 MUST 2; `vite-plus` 1.0.0 (2026-09-28, MIT) bundles Vitest 5.0.1 and depends on `vite` aliased to `@voidzero-dev/vite-plus-core@1.0.0`, so verify in a spike that a `vite ^8` peer range resolves under that alias [I]): externalises F1 shared deps and emits the import-map manifest; builds single-file ESM for F2 (Twenty bundles without code splitting [V]) and fails on unsupported DOM APIs; checks declared capabilities against actual host imports; generates registry types including tenant custom fields; enforces bundle budgets (Shopify caps at 64 KB [V]; proposal 300 KB gzip for F2 [G]); runs axe in Vitest Browser Mode; emits an SBOM.
8. **No Node on customer servers.** UI is built by authors or CI (`vp` manages Node) and shipped prebuilt; servers serve static assets and verify digests (07 MUST 6).

## 5. Manifest, package, compatibility

### 5.1 One manifest for every package kind

Merges 02's `module.toml` and 21's `obop.app.toml`.

```toml
[package]
id          = "acme.loyalty"             # publisher.name; "obop.*" reserved
version     = "1.2.0"
kind        = ["pack", "wasm", "remote"]  # pack | wasm | remote | native | theme
license     = "Apache-2.0"               # SPDX
security_contact = "security@acme.example"
a11y_statement   = "https://acme.example/a11y"   # required if any F2/F3 UI
[compat]
obop = ">=1.4, <2"      # like Grafana's grafanaDependency
api  = "2026-10-01"     # REST dated-version baseline
wit  = "obop:ext@1"
[dependencies]
"obop.sale" = "^1"
[auto_install]           # bridge packages, as Odoo auto_install
when = ["obop.sale", "obop.pos"]
[data]
files = ["packs/models.json", "packs/automations.json"]
ownership = "module"     # immutable at runtime; tenant deltas are overlays
updatable = true         # inverse of Odoo noupdate
[migrations]
dir = "migrations"       # <semver>/{pre,post,end}
[uninstall]
data = "archive"         # archive | drop_after_days | keep
[scopes]
read = ["sale.order", "res.partner"]
write = ["loyalty.ledger"]
[egress]
"api.example-loyalty.com" = "Sync balances with the merchant's loyalty account"
```

- **Ownership (three layers, 08/19).** Module-owned, tenant overlay (`origin = tenant`), user-personal. Module columns on core models are namespaced `<module_id>__<field>`; tenant fields are `x_<slug>` with a stable immutable id; module records carry stable external ids. Uninstall soft-drops in two phases (08 s4) after a confirmed export.
- **Migrations.** Third-party packages use a safe vocabulary (add, rename, backfill job, drop; expand/contract per 18 s9.3) or batched resumable Wasm jobs through the record API. Raw SQL only for `native` packages. This replaces Odoo's `migrations/<version>/pre-*.py` with raw cursor access.
- **Scopes** share one vocabulary with OAuth scopes (T3 and T4).

### 5.2 Format, signing, SBOM

- **Artifact.** An OCI artifact (also exportable as one file): manifest, pack files, `.wasm`, prebuilt UI, `sbom.cdx.json`, provenance. `wkg` already publishes components to OCI [V], and self-hosters can mirror or air-gap it.
- **Signing.** Publisher ed25519 key (21 s7.2) plus cosign keyless attestation from publisher CI, verified at install; the registry countersigns after review. Trust levels follow Grafana's: `private` (tenant key), `community`, `reviewed`, `partner`, `core`.
- **SBOM.** CycloneDX, required for `wasm`, `native` and UI kinds: `cargo-cyclonedx` 0.5.9 (max CycloneDX 1.5, per 18), `pnpm sbom --sbom-format cyclonedx --sbom-spec-version 1.5` for the JS/TS side (pnpm >= 11.1; also emits 1.6 and 1.7 [V https://pnpm.io/cli/sbom]; run through `vp pm sbom` [I]), plus `cargo-auditable` 0.7.6 [V]. `@cyclonedx/cyclonedx-npm` 6.0.1 is not applicable to obop (pnpm-only, owner constraint 2026-10-01): it requires the npm CLI (npm >= 9) and reads npm projects [V its README]; `pnpm sbom` replaces it. The digest lands in the install manifest.

### 5.3 Semver, deprecation, compatibility runs

| Surface | Stability | Versioning | Window |
|---|---|---|---|
| Pack schema, `obop.toml` | Stable | `schema = N` + loader-side upgrader | At least 24 months per version |
| WIT `obop:ext` | Stable, additive within a major | WIT package semver | Host links the old major at least 24 months |
| REST and MCP | Stable | `Obop-Version` date (21) | At least 24 months (21 SHOULD 14) |
| UI SDK F2/F3 | Stable | npm semver + wire version | N and N-1, at least 24 months |
| F1 ESM | Semi-stable | Tied to core minor | 12 months, trusted code only |
| `*-contract` crates | Stable (first party) | semver, `cargo-semver-checks` | n/a |
| Internal crates | None | None | None |

24 months is the floor (aligned with 21); the core's 5-year LTS goal (17) means an LTS line that outlives one API generation must ship a compatibility layer. Budget for it.

**Automated compatibility runs.** On every merge to core `main` (nightly) and each release candidate, `obop-compat`: pulls each listed package; validates pack schemas against the new registry (catches removed fields); instantiates components against the new host with stub imports; type-checks UI bundles against new SDK types; runs each publisher's declared test suite in a sandbox tenant; and publishes pass/fail/untested per core version on the listing. Failing 30 days on a stable core earns a warning; 90 days holds the package from installation on that version. This replaces Odoo's manual annual porting (OpenUpgrade 20.0 held only an initial commit, dated 2026-09-28, after Odoo 20's September 2026 release; the exact release day, and so "nine days", is unverified, 03) with machine checks plus codemods (02 SHOULD 18).

## 6. Developer experience

- **CLI `obop ext ...`** (one Rust binary): `new` (templates pack, action, connector, ui, theme, remote), `dev` (watch, diff and sync to a sandbox tenant with hot reload; `plan` previews, idempotent `apply` exits 0 or 1 for CI, as Twenty does [V]), `test`, `lint` (manifest, licence allow-list, capability-vs-import, a11y), `types` (SDK types from a tenant registry), `logs`, `replay`, `package`, `publish`, `install --sideload`. UI builds shell out to `vp`.
- **Local setup in under 10 minutes [target, G].** Install CLI (about 1 min); `obop dev up` pulls one dev image (obop + Postgres + seeded demo tenant; Twenty's `docker:start` demo workspace is the precedent [V]); `obop ext new`; `obop ext dev`. `vp` installs its own Node and downloads the pinned pnpm (pnpm is the only package manager, owner constraint 2026-10-01) [V https://viteplus.dev/guide/install], so authors need only Docker and the CLI.
- **Seeds and fixtures.** JSON with external ids, flagged `seed`, `demo` or `test`, loaded idempotently (Odoo's data/demo split).
- **Typed SDKs.** Rust `obop-ext-sdk` (Apache-2.0; `wit-bindgen` 0.62 plus a `#[obop::hook]` macro). TS `@obop/ext-sdk` and `@obop/ui-sdk`. REST clients from per-tenant OpenAPI (includes custom fields and installed packages): `utoipa` 6.0.0 server-side; `@hey-api/openapi-ts` 0.99.0 (pre-1.0, pin) or `openapi-typescript` 7.13.0; `progenitor` 0.15.0 for Rust (MPL-2.0 per 21; check generated output) [V versions].
- **Registry-generated docs.** A versioned docs site built from a registry snapshot each release (models, fields, methods, events, webhooks, WIT docs), a per-tenant API explorer, and `obop ext docs` for package references.
- **Tracing and debugging.** Every call emits an OpenTelemetry span (`obop.ext.id/version/kind/hook`, fuel used, peak memory, host-call counts, egress hosts); W3C `traceparent` propagates to webhooks and remote apps; a per-tenant invocation log (errors always, successes sampled) is viewable in-product and via `obop ext logs --follow`. **Replay [I]:** a sync hook's only nondeterminism is host calls, so recording the call tape lets `obop ext replay <id>` rerun a failure locally.
- **Test harness.** Unit: component in-process with mocked host functions. Integration: `obop-testkit` (PG per test, 08 L0) with seeds. E2E: Playwright. UI: Vitest 5 via `vite-plus/test`.
- **CI templates.** GitHub Actions (pinned by SHA, 07 R7) and GitLab: lint, test, package, SBOM, cosign keyless, provenance, publish (sandbox registry on PR, real registry on tag). Twenty's scaffold ships CI, CD and npm-provenance (trusted publishing) workflows [V] https://github.com/twentyhq/twenty/tree/main/packages/create-twenty-app/src/constants/template/github/workflows . obop's TS templates do the same with vp and pnpm only (owner constraint, 2026-10-01): `voidzero-dev/setup-vp` pinned by SHA, then `vp install --frozen-lockfile`, `vp check`, `vp test`, `vp build`; a template `pnpm-workspace.yaml` carries pnpm's supply-chain settings (`minimumReleaseAge`, `trustPolicy: no-downgrade`, explicit `allowBuilds`; see 07 s3.13). Publishing a TS package (for example `@obop/ui-sdk`) to the npm registry uses `pnpm publish`, which has been native since pnpm 11 and implements npm trusted publishing: in GitHub Actions it exchanges the job's OIDC token (needs `id-token: write`) for a registry publish token (falling back to a configured static token only when no trusted publisher exists) and derives provenance, and other CI systems can inject the OIDC token as `NPM_ID_TOKEN`; the trusted publisher must be configured on the registry per package [V pnpm source `pnpm11/releasing/commands/src/publish/oidc/` and `pnpm/crates/publish/src/oidc/` in https://github.com/pnpm/pnpm, read 2026-10-01; the pnpm.io publish page documents only `--provenance`, not OIDC]. Staged publishing (upload without 2FA, approve later with 2FA) is `pnpm stage` (pnpm >= 11.3) or `vp pm stage` [V https://pnpm.io/cli/stage, https://viteplus.dev/guide/install]. npm-CLI publishing steps (`npm publish`, npm 2FA, minimum npm CLI versions) are not applicable to obop (pnpm-only); the replacement control is OIDC trusted publishing plus staged 2FA approval through pnpm.

## 7. Worked example: loyalty-lite at every tier

Feature: earn points on confirmed sale orders (tiered), show the balance on the partner, redeem on later orders, expire after 12 months.

**T1 pack.** Model `loyalty.ledger`, a rollup on the partner, view patches (smart button, "redeem" field), an invoice print line, a dashboard tile, an import mapping for legacy points.

```jsonc
{ "schema": 1,
  "extend": [{ "model": "res.partner", "fields": [{ "name": "acme_loyalty__points", "type": "rollup",
     "of": "loyalty.ledger.partner_id", "agg": "sum", "field": "points",
     "where": "expires_at == null || expires_at >= today()" }]}],
  "automations": [{ "id": "earn_on_confirm", "model": "sale.order",
     "trigger": { "on": "state_set", "state": "sale" },
     "when": "record.partner_id.loyalty_enabled && record.amount_untaxed > 0",
     "actions": [{ "create": { "model": "loyalty.ledger", "values": {
        "partner_id": "record.partner_id", "order_id": "record.id",
        "points": "int(record.amount_untaxed / 10)",
        "expires_at": "today() + duration('8760h')" }}}] }] }
```

**T2 CEL.** Gold multiplier `int(record.amount_untaxed / 10) * (record.partner_id.tier == 'gold' ? 2 : 1)`; redemption constraint `record.points_used <= record.partner_id.acme_loyalty__points`; a scheduled expiry automation. No code. *Cannot do:* stacking campaigns or FIFO consumption of expiring batches.

**T3 Wasm.** A `sync-hook` component computes earn and FIFO redemption for a whole order in one call.

```wit
package acme:loyalty@1.0.0;
world earn-engine {
  import obop:ext/records@1.0.0;   // read orders and lines, write ledger
  import obop:ext/kv@1.0.0;
  import obop:ext/log@1.0.0;
  export obop:ext/hooks@1.0.0;     // on-event(event) -> result<outcome, string>
}
```

The manifest declares `records.read`, `records.write = ["loyalty.ledger"]`, `egress = []` and the 3.3 limits. The world has no `http`, so the hook cannot call out, structurally. Writes commit atomically with the order confirmation, which T4 cannot do.

**T4 remote app.** An external loyalty SaaS declares `kind=["remote"]`, scopes `sale.order:read`, `res.partner:read`, `loyalty.ledger:write`, a webhook `sale.order.confirmed` and an MCP tool `loyalty_balance`. It computes externally and writes back via REST. It is eventually consistent and can fail independently of the sale.

**T5 native.** A first-party `obop-loyalty` crate behind feature `loyalty`: `#[derive(Model)] struct LoyaltyLedger`, a `Module` descriptor with `depends = ["sale"]`, a Rust hook in the per-model chain (08 s6). Only if the project decides loyalty is core.

**Custom e-invoice connector.** EN 16931 mapping, import/export mapping and PDF are T1 (in core). A connector to a Peppol access point's HTTP API is a T3 `connector` world with `egress = ["<access-point host>"]`. If the provider mandates its own SDK or private network, use T4. Audit-critical statutory algorithms (hash chains) are T5. Country packs ship as versioned `kind=["pack","wasm"]` packages (05 MUST 5).

## 8. Licensing boundary (AGPL + Application Exception)

**Template.** Twenty's exception is an AGPL s7 additional permission. It defines Application Interfaces as the HTTP APIs and webhooks, manifest and configuration formats, logic functions, platform-rendered front components, and published SDKs. An Application that "does not otherwise incorporate or modify the source code of Twenty" may use any licence. Bundling platform libraries is covered when official tooling produces the combination. It excludes modifying Twenty and grants no trademark rights [V] https://raw.githubusercontent.com/twentyhq/twenty/main/LICENSE .

| Case | Boundary analysis [I] | Proposed treatment |
|---|---|---|
| **T1/T2 packs** | Data and expressions interpreted by the host; no obop source incorporated | Any licence; covered as "manifest and configuration formats" |
| **T3 Wasm** | Separate compiled work with only author code plus Apache-2.0 bindings; in-process but memory-isolated, typed serialised calls | Covered as the "Wasm host ABI". **Most novel case; counsel must confirm** |
| **T4 remote** | Separate program over HTTP; clearest separate work (FSF plug-in guidance, 17 s2.2) | Any licence |
| **F2 remote-dom** | Sandboxed, wire protocol; authors bundle `@obop/ui-sdk` | SDK and UI tokens Apache-2.0/MIT so bundling is clean |
| **F3 iframe** | Separate origin and document, `postMessage` only | Any licence |
| **F1 ESM** | Same realm, imports SDK, shares objects: "intimate" | **Not covered.** Registry-distributed F1 code must be AGPL-compatible or under a commercial agreement |
| **T5 native** | Linked into the AGPL binary | AGPL; third-party native builds unsupported |
| **`ee/`** | Separate commercial licence (17) | Extension points documented, not exempted |

**Questions for counsel** (extending 17 s12):

1. Is a Wasm component loaded by an in-process AGPL host a separate work, and does the s7 permission survive scrutiny?
2. GPLv3 s7 lets a downstream remove additional permissions from its own copy when conveying. How does the exception bind redistributors, and does an Apache-licensed WIT/UI-SDK layer fill the gap?
3. Can the open edition require an obop notice under s7(b) while a commercial white-label licence lifts it, given the CLA licence grant?
4. Does AGPL s13 trigger when the only "modification" is loading third-party packages? (By my reading no: it applies only if you modify the Program.)
5. Where exactly is the line for F1 and a thin shim importing core types?
6. Developer-agreement content, takedown liability, and OSS-licence conformance of listed packages (WordPress requires GPL-compatible; Odoo accepts any licence compatible with dependencies).
7. CRA role of the registry operator versus publishers (17 s7).
8. Trademark use in "Works with obop" and package names.

## 9. Distribution

- **Registry tiers.** `private`, `community` (automated gates only, labelled unreviewed, excluded from the default "Vetted only" filter as in Twenty), `reviewed` (manual security review; target 5 business days vs Stripe's 4 [V]), `partner` (SLA, localisation and e-invoice packs), `core`.
- **Automated gates per version.** Manifest lint, licence allow-list, SBOM and OSV scan, capability diff vs previous version (new scope or host forces re-review and re-consent), Wasm import-vs-capability check, secret scan (VS Code blocks publish on secrets [V]), typosquat and name reservation, size caps, compat run.
- **Verified publisher.** Domain proof plus 6 months good standing, after VS Code [V].
- **Curated vs open.** Open registry, curated default view. Rejected: Twenty-style no-review as the only model.
- **Sideloading always permitted.** `obop ext install ./pkg --allow-unsigned` for tenant admins; default on self-hosted. Precedents: Twenty tarball deploy, Stripe private upload, Shopify custom apps [V]. On shared cloud limited to T1-T4.
- **Abandonment.** `unmaintained` after 12 months without release plus 2 failed compat runs; hidden from search 6 months later unless re-certified; installed copies keep working with a banner; namespace adoption or fork transfer with publisher notice (OCA shows forking works).
- **Kill switch.** The registry can disable a version everywhere within about an hour [G]; VS Code's block list auto-uninstalls [V].
- **Take rate only after verified actuals.** Comparables: Odoo 30% (25% IAP, EUR 400 payout) (03); Shopify 0% to USD 1M then 15% + USD 19 + 2.9% (no 0% tier above USD 20M prior-year App Store earnings or USD 100M company revenue) [V]; VS Code and WordPress charge none [M]. Recommendation: 0% at launch, announce any future rate and threshold 12 months ahead, cap at 15%, never on free open-source packages or sideloads, never demanded from hosts (WP Engine lesson, 17).

## 10. Theming and white-label

| Surface | Who controls | Mechanism | Limits | Edition |
|---|---|---|---|---|
| **Back-office shell** | Tenant admin | Logo (light/dark), favicon, one accent hue expanded to an OKLCH ramp, default density, login message | Automatic WCAG 2.2 AA contrast check blocks failing combinations (Stripe restricts colours "to ensure a high accessibility bar" [V] https://docs.stripe.com/stripe-apps/design ). No custom CSS or fonts | Open |
| **Portal, website, e-commerce, checkout** | Tenant (theme packs) | `kind="theme"`: DTCG tokens, block templates (12 s5), assets. DTCG 2025.10 (2025-10-28) is the first stable edition, a Final Community Group Report that "is not a W3C Standard nor is it on the W3C Standards Track"; no newer edition was published by 2026-10-01 [V] https://www.designtokens.org/tr/2025.10/format/ ; Style Dictionary 5.5.5 and Terrazzo 2.7.1 are DTCG-compatible tools listed as adopters on designtokens.org, not designated "reference implementations" (that label was unverified) [V] | Tokens and templates free; CSS sanitised (no external `@import`, allow-listed `url()`); JS only as F2/F3 or reviewed "trusted theme JS" with CSP nonce + SRI; storefront on a different origin from admin | Open |
| **Email and PDF** | Tenant, packs | minijinja (autoescape, fuel, recursion limit) for email; Typst 0.15.1 for PDF (06/15); variable schema from the registry; sample-record preview | No includes outside the package; render time and size budget; SPF/DKIM/DMARC wizard (15) | Open |
| **Custom domains** | Tenant | Portal/website via ACME (`instant-acme` 0.8.5 or `rustls-acme` 0.15.4 [V]) with DNS-TXT proof against dangling-record takeover | Admin-UI custom domain is a cloud-plan or partner feature | Portal open; admin domain EE |
| **Partner white-label** | Reseller | Rename product, all logos and PWA icons, remove obop attribution, admin/login domain, partner-branded platform emails, per-partner default theme, reseller console | Commercial licence; counsel Q3 | **EE** |

**Reconciling 17 with 19.** 19's "accent hue plus logo" limit is the back-office row. 17 gates "white-label" in EE. They agree if the line is: what a *tenant* needs to look professional to its own customers (theme, email, PDF, portal domain) is open; what a *partner* needs to resell the product under another identity is enterprise. 17's rule stands: nothing security- or portability-related is gated.

**Tokens.** Primitive, semantic and component tiers compiled to runtime CSS custom properties with `data-theme` and `data-density` (19 s5.6). Themes override semantic tokens only; F2/F3 receive the same semantic set.

## 11. Phase cut and effort

Per 20: Phase 0 (0-3 months) already includes the manifest and registry; Phase 1 (3-12) loop alpha to beta; Phase 2 (12-20) "extension SDK and marketplace alpha"; Phase 4 (32+) marketplace revenue share. Person-months, ±40%, extension-specific work on top of core metadata features [I].

| Phase | In | Out | Effort |
|---|---|---|---|
| **0** | `obop.toml` schema, module registry, dependency graph, `native` modules | Everything else | 3-4 |
| **1 (cut)** | T1 packs, tenant overlays, uninstall policy, Studio-style export (5); T2 CEL with limits spike and Wasm client evaluator (4); T4 remote apps: OAuth provider, scopes, consent UX, connected-apps inventory (5); CLI v0 and dev image (4); generated docs and TS/OpenAPI SDKs (3); tenant branding, DTCG pipeline, portal theme pack, email/PDF template packs (5); sideload and private packages (1); CI template (1) | Wasm host (internal spike only), third-party F1, F2, F3, public registry, signing beyond release signing, white-label | **26-34** |
| **2** | T3 Wasm host, worlds, capability enforcement, `ext` role, invocation log, replay (10); Rust SDK, TS preview (3); F2 host, UI SDK, allow-list, a11y tests (8); Vite+ plugin (2); registry service (8); signing, SBOM, provenance (2); review tooling (2); compat harness (3); F1 trusted loader (2) | Iframe apps, white-label, revenue share | **38-50** |
| **3** | F3 iframe apps, MCP Apps alignment, CSP builder (4); custom-domain automation (3); partner white-label and reseller console (8); Rhai-in-pack if demand (2) | Marketplace commerce | **15-22** |
| **4** | Billing, payouts and tax for paid listings, take-rate controls, certified-partner programme | n/a | **10-15** |

Phase 1 fits 2-3 FTE for 9 months on an 8-12 person team (20).

## Confidence & gaps

**High [V].** The Wasmtime LTS rule and version facts; the Twenty exception and app model; Shopify limits and revenue share; Stripe and MCP Apps sandbox models; Directus and NocoBase licence state; Rhai and Wasmtime API names in source; Frappe advisories.

**Medium.** Several pages were read through a summarising fetcher. Stripe and Twenty review timings come from docs, not experience. The `build.chunkImportMap` option name (confirmed in the Vite docs, Experimental) and Grafana's frontend-sandbox stage (public preview, off by default) were confirmed at verification; Grafana backend-plugin isolation remains unconfirmed.

**Gaps.** Salesforce entirely [M]; VS Code API-stability wording; WordPress review times; Frappe Cloud marketplace terms; Odoo Apps review internals beyond 03; `cel`'s real conformance rate (only the README and layout were read); production maturity of `componentize-js`, `jco` and Javy; Wasm per-call overhead (no benchmarks); enforceability of the exception (lawyer needed); Extism beyond 21. The 300 KB F2 budget, the under-5-second loop and the 10-minute setup are targets, not measurements.

## Implications for obop

**MUST**

1. **MUST** store only total, pure, cost-bounded expressions (CEL) in data, with save-time type-checking, a step budget and a wall-clock deadline (Phase 1 spike for the missing `cel` cost limit). No Rhai or other Turing-complete text in the DB in Phase 1-2.
2. **MUST** define one `obop.toml` for all package kinds: semver dependencies, `auto_install`, ownership flags, a safe migration vocabulary, scopes, egress with purpose strings, an uninstall policy. No raw SQL or cursor access for third parties.
3. **MUST** keep three data layers (module, tenant overlay, user) and ship Studio-style export of tenant customisations as a package in the open edition.
4. **MUST** never load third-party UI into the main JS realm. F1 is first-party and tenant-own only; marketplace UI is F2 (remote-dom, host element allow-list) or F3 (sandboxed iframes aligned to MCP Apps).
5. **MUST** make Wasm capabilities structural (separate `sync-hook` world without `http`/`secrets`), manifest-declared, shown at install and diffed per upgrade; effective permission is user role intersected with app role.
6. **MUST** pin Wasmtime to an LTS major (48.x now, next v60 around 2027-08) behind one crate, patch within 7 days (24 h for High), never rely on fuel alone, and run components in a separate `ext` process role. Correct 21's "no LTS policy" line.
7. **MUST** publish the Application Exception and Apache-2.0/MIT SDK, WIT and UI-SDK packages before soliciting third-party modules, after counsel reviews items 1-3 of section 8.
8. **MUST** commit to at least 24-month deprecation windows on the pack schema, WIT, REST/MCP and UI SDK, enforced by nightly compatibility runs published per listing.
9. **MUST** make remote apps safe by default: short-lived tokens, refresh rotation with reuse detection, per-app audit, one-click revoke, registry kill switch.
10. **MUST** permit sideloading on self-hosted and private signed packages on cloud, and keep customer servers Node-free.
11. **MUST** keep tenant theming and customer-facing surfaces (portal, website, email, PDF, portal domain) open, gate only partner white-label in EE, and never gate security or portability.

**SHOULD**

12. **SHOULD** ship Phase 1 as the section 11 cut (T1, T2, T4, T5, CLI v0, generated docs/SDKs, theming) and defer Wasm GA, F2 and the public registry to Phase 2, with one internal Wasm connector spike in months 10-12.
13. **SHOULD** support Rust guests first and TS as preview until `componentize-js` leaves "experimental"; define worlds without `async func` until Rust 1.100 (`wasm32-wasip3` on stable) ships.
14. **SHOULD** run tiered registry trust levels (`private`, `community`, `reviewed`, `partner`, `core`) with automated gates per version and a "Vetted only" default filter.
15. **SHOULD** deliver `obop ext` with `plan/apply`, hot-reload sandbox tenants, a seeded dev image, per-invocation OpenTelemetry spans, record-and-replay, and CI templates pinned by SHA with cosign keyless and SLSA provenance.
16. **SHOULD** ship `@obop/vite-plugin-ext` behind one Vite+ config factory (07 MUST 2), with capability-vs-import checks, bundle budgets and axe in Vitest Browser Mode.
17. **SHOULD** require a WCAG 2.2 AA statement and automated axe pass for F3 packages, and give the SDK its own overlay primitives.
18. **SHOULD** generate per-tenant OpenAPI including custom fields and installed packages, pinning pre-1.0 generators.
19. **SHOULD** use DTCG 2025.10 tokens with semantic-only overrides, an automatic contrast gate for the back-office shell, and sanitised CSS plus a separate origin for storefront themes.

**COULD**

20. **COULD** add Rhai as pack-shipped signed scripts (tenant flag default off) in Phase 3 if demand is measured.
21. **COULD** add an Extism-backed simpler Wasm path if component-model churn proves too costly (21 lists it).
22. **COULD** publish F3 apps as MCP Apps so obop UI renders in AI chat hosts.
23. **COULD** publish a public compatibility board per core release.
24. **COULD** introduce a take rate only after a year of verified paid-listing actuals, capped at 15% and announced 12 months ahead.

## Verification log

Adversarial fact-check on 2026-10-01 against primary sources (GitHub API and raw files, crates.io and npm registries, vendor docs, IETF datatracker, web-features). Default stance was to doubt: a claim stays only if confirmed.

**Confirmed as stated: about 105 discrete claims.** This includes all 12 flagged load-bearing claims in substance (Wasmtime LTS rule, v36/v48 LTS and 49.0.1 on 2026-09-24; Twenty s7 Application Exception and MIT packages; Shopify Functions numbers, UI 64 KB Web Worker and revenue-share figures; Twenty front-component sandbox and no-review/Vetted model; Stripe sandboxed iframe, React 17.0.2 and 4-business-day review; MCP Apps spec 2026-01-26 and SDK v2.0.3 on 2026-09-25; `wasm32-wasip3` Tier 2 and first stable in Rust 1.100.0; Directus BSL-1.1 to MSCL-1.0-GPL on 2026-05-28; Frappe Server Scripts default-off since v15, public-bench ban and GHSA-qq98-vfv9-xmxh on 2025-12-29; `componentize-js` experimental and `cel` 0.14.5 README with no cost limit; DTCG 2025.10 on 2025-10-28 as a non-Recommendation and import-map Baseline status), plus the version, date, licence and API-name claims in the tables (Odoo ORM changelog, Studio ZIP flags, OpenUpgrade initial commit, Odoo 30%/25% commission, Medusa/Strapi/Payload/NocoBase/Grafana licences, VS Code and WordPress rules, Patchstack figures, WASI 0.3.x dates, Wasmtime/Rhai/minijinja API names, and crate and npm versions for wit-bindgen, cargo-component, wkg, utoipa, progenitor, cargo-semver-checks, cargo-cyclonedx, cargo-auditable, instant-acme, rustls-acme, Typst, jco, remote-dom, Style Dictionary, Terrazzo, CycloneDX npm, hey-api, openapi-typescript, Vitest 5).

### Corrections (was / now / source)

| Was | Now | Source |
|---|---|---|
| Twenty "Logic-function isolation undocumented" | Documented: isolated Node.js processes; drivers Disabled (production default), Local ("no sandboxing", dev only), Lambda (hardware-level isolation) | https://docs.twenty.com/developers/self-host/capabilities/setup |
| Shopify "11M instructions (carts up to 200 lines), 128 kB in, 20 kB out" alongside fixed limits | 256 kB / 10,000 kB / 512 kB are fixed; the 11M, 128 kB and 20 kB are dynamic limits for carts up to 200 lines and scale above that | https://shopify.dev/docs/api/functions |
| Shopify UI extension cap "64 KB" | 64 KB, or 128 KB for full-page customer account extensions | https://shopify.dev/docs/api/checkout-ui-extensions |
| Shopify revenue share without exclusions | No 0% tier for USD 20M+ prior-year App Store earnings or USD 100M+ company revenue; USD 19 is per Partner account; 2.9% plus sales tax | https://shopify.dev/docs/apps/launch/distribution/revenue-share |
| Shopify "CLI blocks deploys to older" versions | Stated for Polaris-based UI extensions targeting versions older than 12 months | https://shopify.dev/docs/api/usage/versioning |
| MCP Apps "mandates ... host-supplied CSS variables" | CSS variables are optional ("Hosts can optionally pass"); the different-origin sandbox proxy is required when the host is a web page; the proxy itself has `allow-scripts` + `allow-same-origin` on its own origin, so obop's opaque-origin rule is stricter | https://raw.githubusercontent.com/modelcontextprotocol/ext-apps/main/specification/2026-01-26/apps.mdx |
| DTCG tools "Style Dictionary 5.5.5 and Terrazzo 2.7.1 are reference implementations" | DTCG-compatible tools listed as adopters; "reference implementation" label not supported. Spec is a Final Community Group Report, no newer edition by 2026-10-01 | https://www.designtokens.org/ ; https://www.designtokens.org/tr/2025.10/format/ ; https://www.w3.org/community/design-tokens/ |
| `cel` "optional `regex`/`chrono`/`json` features" | `regex` and `chrono` are default features; `json` is opt-in | https://raw.githubusercontent.com/cel-rust/cel-rust/master/cel/Cargo.toml |
| minijinja "optional `set_fuel` and `set_recursion_limit`" | Only `set_fuel` is feature-gated (`fuel`); `set_recursion_limit` is always available | https://raw.githubusercontent.com/mitsuhiko/minijinja/main/minijinja/src/environment.rs |
| "Wasmtime 48 already denies socket creation by default" | The `wasmtime-wasi` crate's default configuration denies TCP/UDP sockets (an embedder-crate default) from 48.0.0 | https://github.com/bytecodealliance/wasmtime/releases/tag/v48.0.0 |
| v36.0.0 date 2025-08-20 "by arithmetic [I]" | Verified [V]: v36.0.0 2025-08-20, v48.0.0 2026-08-20; LTS patches 48.0.3 and 36.0.16 shipped 2026-09-24 | https://github.com/bytecodealliance/wasmtime/releases |
| Frappe "At least 40 advisories" (2026-05-18 to 2026-10-01) | 41 by recount | `gh api repos/frappe/frappe/security-advisories` |
| Salesloft Drift "between 2025-08-08 and 08-18" | "As early as 2025-08-08 through at least 08-18"; tokens revoked and app removed 2025-08-20 | https://cloud.google.com/blog/topics/threat-intelligence/data-theft-salesforce-instances-via-salesloft-drift |
| Odoo Apps store 93,613 listings | 93,630 on 2026-10-01 (figure drifts; 93,613 came from 03) | https://apps.odoo.com/apps/modules/browse |
| Odoo "EUR 400 payout" | Payout is triggered when unredeemed sales reach EUR 400 (no fixed minimum stated) | https://apps.odoo.com/apps/faq |
| Studio export "Enterprise in Odoo [V]" | Docs show Studio is a paid add-on (upsell to the Custom plan); the docs do not use the word "Enterprise" and no Studio addon exists in the Community repo | https://github.com/odoo/documentation/blob/20.0/content/applications/studio.rst |
| OpenUpgrade 20.0 "nine days after release" | Initial commit dated 2026-09-28 is confirmed; "nine days" is unverified because Odoo 20's release notes give only "September 2026" | https://github.com/OCA/OpenUpgrade/commits/20.0 ; https://www.odoo.com/odoo-20-release-notes |
| Medusa modules "same process" tagged like a verified fact | Isolation (no cross-module access, Links, Query) is confirmed; "same process" is not stated in the pages read, now tagged [I] | https://docs.medusajs.com/learn/fundamentals/modules/isolation |
| Stripe "forbids controlled inputs" | Docs recommend uncontrolled inputs because of input lag; refs are unsupported | https://docs.stripe.com/stripe-apps/how-ui-extensions-work |
| Twenty "No publish review" | "No formal review process is described" (absence of description, not proof of absence); claiming verifies GitHub provenance only | https://docs.twenty.com/developers/extend/apps/publishing |
| Stripe accessibility quote with no URL | Source added: the quote is on the design page, not the style page | https://docs.stripe.com/stripe-apps/design |
| `cargo-component` "0.21.1, 2025-03-18" | 2025-03-18 is the crates.io date; GitHub release is 2025-04-07, last push 2025-07-14 | https://crates.io/crates/cargo-component ; `gh api repos/bytecodealliance/cargo-component/releases` |
| Vite `build.chunkImportMap` option name unconfirmed | Confirmed in Vite docs: Experimental, default `false`, added in 8.1.0 (2026-06-23) | https://vite.dev/config/build-options#build-chunkimportmap |
| Grafana frontend-sandbox stage unconfirmed | Confirmed: disabled by default, public preview since Grafana 11.5 | https://grafana.com/docs/grafana/latest/administration/plugin-management/plugin-frontend-sandbox/ |

### Additions (facts the checking revealed)

- OAuth 2.1 (`draft-ietf-oauth-v2-1-16`, 2026-09-03) and Client ID Metadata Document (`draft-ietf-oauth-client-id-metadata-document-02`, 2026-07-06) are IETF drafts, not RFCs.
- Rust 1.99.0 is dated 2026-10-01; 1.100.0 is in beta (1.100.0-beta.1, 2026-09-27), so `wasm32-wasip3` on stable is about 2026-11-12 by the six-week train [I].
- `vite-plus` 1.0.0 (2026-09-28, MIT) is current, bundles Vitest 5.0.1 and aliases `vite` to `@voidzero-dev/vite-plus-core`; the `vite ^8` peer range of the proposed plugin needs a spike [I]. Latest Vite is 8.3.2.
- Twenty's MIT list also covers `create-twenty-app` and `packages/twenty-apps`; Twenty's `engines.twenty` is enforced server-side on upload and install.
- GlassWorm botnet takedown (CrowdStrike, Google, Shadowserver) on 2026-05-26.

### Unresolved

- "About 55 fetches today" and other author-process statements in Scope & method cannot be verified.
- Salesforce row stays [M]; VS Code and WordPress charging no take rate stays [M]; Frappe Cloud marketplace terms and WordPress review times not checked.
- Exact Odoo 20.0 release day (so the "nine days" OpenUpgrade gap).
- Wasmtime's official support table is rendered by JavaScript and was unreadable; EOL dates (v36 about 2027-08, v48 about 2028-08, v60 about 2027-08-20) are arithmetic [I].
- Rust 1.100.0 stable date is a train-cadence inference, not a published date.
- Grafana backend-plugin isolation; Grafana's exact public-plugin review process; Stripe and Twenty review timings beyond the documentation.
- "Same process" for Medusa modules; whether `vite ^8` peers resolve under the Vite+ alias.
- Claims cited to sibling files (02, 03, 05, 06, 07, 08, 15, 17 to 21: Rhai 1.26.1 use, Odoo OPL-1/LGPL-3 licensing effect, Cal.com churn, CRA role, phase estimates) were not independently re-verified beyond the items above.
- Legal readings in section 8 (GPLv3 s7 removal of additional permissions, AGPL s13 trigger) are the author's interpretation and need counsel.

- Owner-constraint pass (2026-10-01): the SBOM line no longer recommends `@cyclonedx/cyclonedx-npm` (needs the npm CLI, npm >= 9) and names `pnpm sbom` (CycloneDX 1.5 to 1.7, pnpm >= 11; `--sbom-spec-version` from pnpm 11.1) instead, checked against the pnpm.io docs source and that tool's README; the `obop ext` local-setup line now says `vp` provides Node and the pinned pnpm; the CI-templates bullet gained the vp + pnpm pipeline (`setup-vp` by SHA, `vp install --frozen-lockfile`, `vp check`, `vp test`, `vp build`), pnpm's supply-chain settings, and the pnpm equivalent of npm trusted publishing (`pnpm publish` with OIDC and provenance from pnpm 11, verified in the pnpm source because pnpm.io's publish page documents only `--provenance`) and staged 2FA approval (`pnpm stage` / `vp pm stage`, pnpm >= 11.3). Twenty, Medusa, Directus, Strapi, Payload, NocoBase and Stripe "npm" mentions are third-party distribution facts and were left as written, as were registry package versions and the "npm semver" wording for the UI SDK contract (semver on registry packages). Not verified: `vp pm sbom` forwarding to `pnpm sbom` (tagged [I]).
