# Open issues toward a finished product

Compiled 2026-10-17 from the reports of every build and review pass. Status: `open`, `in progress`, `done`.
Each row names the area that owns it. Nothing here is a new feature wish; every row is a gap an agent or a
test found, or a limit the docs state.

## A. Data model completeness (Odoo-class workflows)
| # | Issue | Status |
|---|---|---|
| A1 | Actions have no side effects: confirming an order cannot create an invoice, validating a stock move cannot change quantities, posting an invoice cannot write a journal entry | done (D56). `[[model.action.effect]]` create, update, adjust and set_state plus `assert`, atomic with the transition on all three backends; used by sales confirm, invoicing post, accounting post, inventory validate, purchase receive, pos pay. Out of scope: expenses, manufacturing and subscriptions effects; effects never call out (X3) |
| A2 | No computed fields: totals, balances, quantities on hand, SLA times are typed in by hand | done (D57). Computed on read (sum, min, max, count with `where`, arithmetic, if, concat); used for order, invoice, purchase and pos totals, accounting debit and credit totals, project progress. Not possible: SLA elapsed (no date arithmetic); `inventory_item.quantity` stays stored and is kept by `adjust` effects |
| A3 | No unique constraints or number sequences (invoice, order, receipt numbers are free text) | done (D57). `unique`, `unique_with` (checked write plus database index) and `sequence` fields (transactional counters, gapless, yearly reset); invoice, journal entry, sale order, purchase order, expense, ticket, stock move and receipt numbers; SKU, account and journal codes. Existing records are numbered with `obop sequences backfill`; party email is deliberately not unique |
| A4 | No row-level permissions: nobody can be limited to their own records (leave requests, documents, tickets) | done (D58). `[model.access]` own, via, public_field, shared_with, see_all_group enforced in every read and write path with an oracle test; applied to hr leave requests, expenses, timesheets, documents and dashboards. Aggregates count only visible records and may sum computed fields (loaded and evaluated first). Not applied: helpdesk requester, project privacy |
| A5 | `/api/meta` does not say which fields are governed, so a user without the action sees a plain field | done. `/api/meta` fields carry governed, computed, unique, sequence; models carry `access`; MCP schemas and OpenAPI leave computed and numbered fields out of inputs |
| A6 | Dashboard "shared" flag is enforced only in the browser | done. `dashboards_dashboard` and its widgets are filtered by the server (owner, shared, dashboards.manager) |

## B. Operations, scale and durability
| # | Issue | Status |
|---|---|---|
| B1 | Sessions, MCP sessions, rate limits, forum and status data live in one process's memory (one process per database) | done: all of it, and the set of installed modules, is in the database (D59); proven by two real `obop serve` and two `platform serve` processes on PostgreSQL. Counters are fixed windows; a forum or status read may lag a write on another process by 250 ms; SQLite shares between processes on one machine only |
| B2 | Connector retry queue is lost on restart | done: jobs, leases and dead letters are rows (at-least-once, stable delivery id); crash-restart and two-worker tests |
| B3 | No key rotation or credential re-sealing for `OBOP_SECRET_KEY` | done: key ring, `obop secrets status|rotate` (dry run, batches, resumable, while serving), runbook and lost-key story in docs/DEPLOY.md; no KMS |
| B4 | No backups: no `obop backup` / `restore`, no restore drill | done for the tools: `obop backup`, `backup verify`, `restore` (SQLite and PostgreSQL, round trips, tamper, refusal and version tests) and `deploy/restore-drill.sh` (run in CI). Out of scope as asked: scheduled backups of hosted instances (needs object storage); `pg_dump` and WAL archiving for the Sliplane PostgreSQL service are documented, not run against it |
| B5 | No schema versioning or migration step for upgrades (`obop migrate`) | done: `obop-migrate` (two real steps, tests for order, idempotency, rollback and concurrent runners), `obop migrate status|plan|run`, `--auto-migrate`, newer-schema refusal; compatibility promise in docs/DEPLOY.md; D59 |
| B6 | Every customer instance builds the Rust workspace on the shared server; no published image | partly done: `image.yml` (linux/amd64, cache, provenance and SBOM, never run from here), cargo-chef Dockerfile (full image built locally; the web target was broken by the core-version read and is fixed), `OBOP_IMAGE` preferred, `obop platform upgrade-instances` (fake and runner-fake tests). Not verified live: the first GitHub run, and whether Sliplane converts a repository service to an image; no arm64 |
| B7 | Changing the admin 2FA policy is not audited; turning 2FA off does not revoke the user's other sessions | done: `security.*` changes are logged (who, old, new), 2FA off and password change end other sessions, session list and sign out everywhere, password change, administrators disable, enable and reset accounts, Security tab panels in seven languages, unit, HTTP and Playwright tests |
| B8 | Existing instances created before D53 have no `OBOP_SECRET_KEY` unless an operator sets one | done (live instances) |

## C. Commerce
| # | Issue | Status |
|---|---|---|
| C1 | No payments: orders stay `pending` | done (D62: `obop-payments`; manual, Stripe, demo provider; webhook, refunds, receipt) |
| C2 | Orders do not decrement stock; no discount codes; no taxes | done (D61: atomic stock, codes, exact tax arithmetic) |
| C3 | No images anywhere (products, pages, posts); Open Graph has no image | done (D60: blob store, `image`/`file` kinds, responsive shop images, og:image) |

## D. Interface and localisation
| # | Issue | Status |
|---|---|---|
| D1 | List selection is per page; no "select all matching" | done |
| D2 | Record panel stacks below the list at tablet and phone widths | done |
| D3 | Server-paged lists sort bytewise, browser lists use the locale collator | done |
| D4 | Browser-side views are capped at 5,000 rows (kanban, pivot, counts) | done |
| D5 | Translated MCP and token docs pages (6 languages) are out of date; module card summaries and server blocker messages stay English | done |
| D6 | Changing language remounts the app and drops open dialogs | done |
| D7 | No native review of any translation | open (needs people) |

## E. Platform, trust and discovery
| # | Issue | Status |
|---|---|---|
| E1 | No OAuth authorization server, so Claude Desktop / claude.ai cannot connect to a hosted instance | done (D64; exact limits below; not tried against the real apps) |
| E2 | `is-agentic` score is cached; brand and developer-resource discoverability depend on indexing | open (needs time and a real domain) |
| E3 | No mailbox: contact details are GitHub only; privacy and terms are drafts for counsel | open (needs the owner) |
| E4 | New instances use `*.sliplane.app`; per-instance `obop.dev` subdomains need a wildcard DNS record | open (needs DNS) |
| E5 | `www.obop.dev` has no record | open (needs DNS) |
| E6 | Google and Microsoft connector adapters are definition-only; other adapters were tested only against local stand-ins | open |
| E7 | WebAuthn / passkeys; MCP spec 2026-07-28 | passkeys done with limits (D65; exact limits below; not tried on real authenticators); MCP 2026-07-28 open |

### E1 in detail: the OAuth sign-in for MCP clients (D64)

Built: metadata (RFC 8414, RFC 9728, `WWW-Authenticate` with `resource_metadata`), dynamic registration of public clients, authorization code with mandatory PKCE S256 and the RFC 8707 resource, a server-rendered sign-in (two-factor included) and consent page in seven languages, one hour access tokens bound to `<public URL>/mcp`, rotating refresh tokens with reuse detection, revocation, Connected apps in Customize, Security, audit events, rate limits and storage through D59.

| # | Limit | Status |
|---|---|---|
| E1a | **Never tried against the real Claude Desktop, claude.ai or Claude Code.** Tested with our own client (Rust over the router, Playwright in Chrome). How the real apps register, which redirect URIs and parameters they send and how they react to a refusal is from the specifications | open (needs a reachable instance and the apps) |
| E1b | One scope only: the groups the person ticks. No per-command or per-company scopes, no step-up for sensitive groups | by design |
| E1c | Open registration, so an application can pose under a familiar name; the page shows the redirect host and says obop cannot verify who runs it. No Client ID Metadata Documents, no allow-list of known clients | accepted; allow-list open |
| E1d | The sign-in asks for the password again every time (no single sign-on from an open back office); a passkey can be used there as the second step or alone (D65) | by design (D53) |
| E1e | An administrator cannot list or revoke other people's connected apps; the person can, and the security log shows every event | open |
| E1f | A password change does not end connected apps (as with agent tokens); disabling the account does | by design |
| E1g | Changing `OBOP_PUBLIC_URL` ends every connected app (the audience changes); behind a proxy without it the address comes from `Host` and `X-Forwarded-Proto` | by design |
| E1h | Exactly one redirect rule is looser than exact match: a loopback `http` URI may use any port (RFC 8252). A request without `resource` is bound to `/mcp` anyway, for clients written before RFC 8707 | by design |
| E1i | The six translations of the consent page, the Connected apps strings and the docs are machine-drafted | open (D7) |
| E1j | Rate limits are fixed windows, so a burst at the edge may pass twice the limit | accepted |

### E7 in detail: passkeys (D65) and MCP 2026-07-28

**Passkeys, built.** A second factor beside (or instead of) the authenticator app, satisfying the `platform.admin` policy; optional passwordless sign-in for a person who allowed it, with a passkey that verified them; registration and authentication ceremonies in Rust (`obop-auth`, `ring` for ES256, EdDSA and RS256, attestation `none`); origin and RP id from `OBOP_PUBLIC_URL` or the request; single-use two-minute challenges in the shared store; sign-counter clone detection; a list with names and last use, add, rename and remove with the password again; the last second factor kept when the policy requires one; the break-glass reset and the administrator's reset also remove passkeys; audit events; rate limits; the same sign-in on the OAuth pages with one script allowed by hash; the Security panel and the sign-in step in seven languages (the six translations are machine-drafted).

| # | Limit | Status |
|---|-------|--------|
| E7a | **Never tried on a real authenticator or browser other than Chrome:** no phone, Touch ID or Windows Hello, security key, Safari, Firefox, synced passkey manager or cross-device sign-in. Tested with a software authenticator in Rust (ES256, EdDSA, RS256, every refusal) and Chrome's virtual authenticator in Playwright. Real devices differ in counters, flags, transports and `credProps` | open (needs devices) |
| E7b | Attestation `none` only; no restriction by make of authenticator, no metadata service | by design |
| E7c | No conditional-UI autofill and no username hint: a non-discoverable key (some hardware keys) works only as a second step | open |
| E7d | One RP id per credential: changing `OBOP_PUBLIC_URL` to another host ends every passkey; no related origins, no RP id parent | by design |
| E7e | Passkeys exist on instances, not on the control plane's own sign-in (`/api/platform`) | open |
| E7f | No step-up for sensitive actions (AU5) and no "remember this browser"; the proof only stands in for a code | open |
| E7g | Passkeys cannot be held or used by agent tokens | by design (D18, D53) |

**MCP spec 2026-07-28: not built; what the repo documents.** `/mcp` serves Streamable HTTP with the handshake revisions 2025-03-26 to 2025-11-25 and sessions that hold the masking vault (D51); the register states that 2026-07-28 is deferred, not refused, because its per-request model has no place for the session vault. What the research corpus records as required of a 2026-07-28 server (docs/research/16-ai-native-erp.md 3.1 and 3.2; docs/spec/ai.md 4.1; D18 plans it through `rmcp` 3.5.0) is: no `initialize` handshake and no `Mcp-Session-Id`; every request carrying `io.modelcontextprotocol/protocolVersion` and the client capabilities, with `UnsupportedProtocolVersionError` for an unknown version; a mandatory `server/discover`; `Mcp-Method` and `Mcp-Name` headers; multi round-trip requests with a protected `requestState` replacing server-initiated requests; Tasks as the extension `io.modelcontextprotocol/tasks`; deterministic `tools/list`; Client ID Metadata Documents preferred and dynamic registration deprecated (obop still offers only the latter, D64); and compatibility with 2025-11-25 clients. None of this was started in E7 and none of it is invented here; the open work is the stateless masking vault (token stability keyed on the token, as REST does), `server/discover`, the per-request protocol fields, Client ID Metadata Documents and the `rmcp` migration.

| # | Limit | Status |
|---|-------|--------|
| E7h | `/mcp` does not speak MCP 2026-07-28 (stateless, `server/discover`, MRTR, Tasks); clients that speak only that revision cannot use it. The real clients' behaviour is unmeasured (the corpus expects many to keep speaking 2025-11-25) | open |
| E7i | No Client ID Metadata Documents; registration is open and dynamic (E1c) | open |

## F. Quality gate
| # | Issue | Status |
|---|---|---|
| F1 | No fresh-eyes product QA pass over the whole product | done (section G) |
| F2 | The live signup, instance, sign-in, 2FA and install flow has only run locally and against fakes | open (needs the owner's go-ahead to create a test account) |

## H. Updating from inside the instance (D63)

| # | Item | Status |
|---|---|---|
| H1 | Every package declares `requires_core` and a range per dependency; wildcards are catalogue errors; `depends = [...]` is retired | done (D63) |
| H2 | `module.upgrade`, `module.upgrade_all`, `module.rollback`: atomic, locked across processes, audited, agents propose | done |
| H3 | Declarative `[[migration]]` steps (rename, default, drop) in the update batch; breaking changes refused without them | done |
| H4 | Customize, Updates: core version, release notes, module compatibility against the new core, backup first, control-plane request, live progress, rollback; self-hosted instructions | done |
| H5 | Per-instance update token (hashed on the control plane, sealed on the instance), rate limit, audit on both sides, rollback to the previous image when the new one does not start | done (tested against the fake provisioner) |
| H6 | The update path against live Sliplane and the real obop.dev feed | open (needs the owner: publish a release, then try on a test instance) |
| H7 | The release feed is integrity-checked by digest, not signed | open (I-1: a signing key and its distribution) |
| H8 | Rollback of a module is one level; an update that dropped data is rolled back from a backup only | by design |
| H9 | Migration steps hold a model's records in one batch | accepted for the current sizes |
| H10 | Hosted instances need `OBOP_BACKUP_DIR` on a volume for the automatic backup before an update; otherwise the administrator confirms their own | open (needs the owner to mount a volume) |

## G. Fresh-eyes QA pass (F1)

A first-time-user pass over the running product (all 29 installed modules, 91 models, headless Chrome screenshots at 1280, 768 and 390 px, light and dark, the seven languages; the public shop with demo payments; the sign-up app on the fake provisioner; the website's 506 pages; REST parity with a restricted user; backup, restore, migrate and secrets on a fresh SQLite database; environment variables named in the docs).

| # | Defect | Fix |
|---|---|---|
| G1 | Every yes/no field in the record panel showed two controls: a native checkbox and the switch (the generic checkbox style beat the rule that hides the native one, because the input had no `role="switch"`) | `fields.tsx`: the input is a real switch |
| G2 | A record with a number field was titled by whichever text field came first: an invoice card read "EUR", and a draft invoice its id | `labelField` prefers the required text field, then the sequence field, then any text field |
| G3 | The demo sale order had no number (the seeds write straight to the store, so the sequence never ran); anything that pointed at it showed `01a14b33` | `App::seed_demo` numbers the seeded records; test `demo_records_get_their_sequence_numbers` |
| G4 | A plain text field named `tax_id` was labelled "Tax" (the `_id` suffix is meant for references only) | `fieldTitle` keeps "Tax ID" for non-reference fields |
| G5 | "Order (0)", "Order (0)" in the related-record buttons of a contact: two apps, one name | the button names the app when two related models share a name ("eCommerce: Order") |
| G6 | Model and field names like `sla`, `bom`, `sku` were shown as "Sla", "Bom", "Sku" in English | `tx` upper-cases known acronyms in the humanised English fallback |
| G7 | Connectors: the guided "Connect" dialog stayed open when the address changed to another page of the app (back button, search) | `ConnectorsApp` closes its dialogs on a page change |
| G8 | `obop site seed` printed a serve command without saying the backoffice must be built for `/admin/` | message and the build step spelled out |

Checked and found sound: all 91 models list, open, create and edit through REST; actions run and refuse wrong states; no console or network errors on any backoffice page in seven languages; no English leaks in the generated UI (only the items below); no horizontal scroll at 390 px; the shop from product to demo payment in en, de and pl; the sign-up app; website links, anchors and assets (0 broken); environment variables in the docs all exist in the code; a restricted user cannot list, create, read or delete 12 sampled models outside their groups (404, no leak); backup, verify, restore (empty, refused, `--force`), `migrate status|plan|run --dry-run` and `secrets status|rotate --dry-run` on a fresh SQLite database.

Not fixed (left open):

| # | Issue | Why |
|---|---|---|
| G9 | The Guide tab (module author's guide) and module card summaries stay English in every language | known (D5); needs translation work, not a code fix |
| G10 | Order lines keep the product name at the time of the order, so a German buyer sees English names for products that have no German listing translation | by design (the order records what was sold); the listing translation is the remedy |
| G11 | The totals row of a list sums fields with no value as `0` (Dashboards "Position") | cosmetic, no data is wrong |
| G12 | The design documents name commands that are marked Proposed and do not exist yet (`obop upgrade`, `obop ext`, `obop diag`, `obop ledger verify`) | these are roadmap items, flagged Proposed where they appear |
| G13 | `obop-community`'s `both_stay_consistent_on_postgres_when_available` fails when it runs twice within about a minute against the same PostgreSQL database (the status leader lease from the first run is still held) | test hygiene only; use a fresh database per run, as CI does |

## I. Small unblockers (batch 1: D66 to D68)

| # | Item | Status |
|---|---|---|
| I1 | TLS to PostgreSQL: `sslmode=disable|prefer|require|verify-ca|verify-full` and `sslrootcert` in the database URL, rustls, one connector for every PostgreSQL connection; a URL without `sslmode` stays plain | done (D66, PROPOSED). Tested against a throwaway `postgres:17` with a self-signed certificate |
| I2 | PostgreSQL TLS never tried against a managed service, a connection pooler, an intermediate chain or a public CA | open (needs a real target) |
| I3 | PostgreSQL client certificates (`sslcert`, `sslkey`), CRL/OCSP and channel binding (`SCRAM-SHA-256-PLUS`) | open (refused at start-up; not built) |
| I4 | The database URLs the control plane gives to new hosted instances carry no `sslmode` | open (the provisioner should pass the cell's mode on) |
| I5 | Health and metrics: `/healthz`, `/readyz` (database ping, schema compatibility), `/metrics` (build, uptime, requests by class, duration histogram, database ping, sessions, connector queue depth), closed unless token or loopback | done (D67, PROPOSED). Not tried against a real Prometheus, probe or proxy |
| I6 | The control plane (`platform serve`) has no health or metrics routes; counters are per process; no per-route histograms | open |
| I7 | The Sliplane upgrade check still asks `/api/openapi.json`; move it to `/readyz` once no instance runs an image that predates it | open |
| I8 | `OBOP_ALLOWED_ORIGINS`: validated exact origins (https or loopback http, no wildcards, refused at start-up), CORS for `/api` with correct preflight, `Vary: Origin`, no credentials; `/mcp` shares the list; OAuth pages unchanged | done (D68, PROPOSED). Tested with raw requests and a real process; not tried in a real browser or behind a CDN |
| I9 | CORS has one global list: no per-route, per-token or per-method lists; no private-network-access preflight; no cookies or credentialed mode | by design (D68) |
## J. People administration and bulk create (D69, D70)

Rows of this section only; another section of the register is kept apart so the two can be merged.

| # | Item | Status |
|---|---|---|
| J1 | Anonymising or erasing the display data of an offboarded person later (offboarding disables and revokes; it rewrites nothing) | open (needs the erasure design of D54 first) |
| J2 | People administration asks for no step-up re-authentication (it sits with the other administrator actions, E7f) | open |
| J3 | People administration never tried against concurrent edits from several server processes on PostgreSQL, nor with the real Claude apps as the connected application | open (needs a multi-process drill) |
| J4 | The six translations of the People panel strings and of the `user_account` labels are machine-drafted | open (D7) |
| J5 | Groups can only be given by name; there is no per-group delegation ("may give sales.user but not platform.admin") and no group directory with descriptions | open |

### Bulk create (D70)

| # | Item | Status |
|---|---|---|
| J6 | `bulk_create` is not tried against PostgreSQL under concurrent writers from several server processes, nor with 1,000-row calls over a network database; the unit tests run 60 rows on PostgreSQL and 1,000 on the local backends | open (needs a multi-process drill) |
| J7 | No remapping of ids (decimal and date-time cells are supported since D72; a decimal point in a number or money cell is still an error), no reference from one row to another row of the same call, no image or file columns; related records are imported in two calls, parents first | by design for now |
| J8 | A call holds the write lock from its first check to its commit, so a slow database makes a large call hold it for long (about half a minute for 1,000 rows at a 7 ms round trip); splitting a call into batches under shorter locks is not built | open |
| J9 | The validation errors of an import are the server's English text, shown as received; the strings of the dialog are machine-drafted in six languages | open (D7) |
| J10 | The answer of an `idempotency_key` call is kept 24 hours in one table (`bulk_idempotency`), purged lazily every ten minutes; there is no setting for the time | accepted |

## K. Storage foundations (step 1: D72 to D74)

| # | Item | Status |
|---|---|---|
| K1 | The backoffice pivot, graph, kanban, calendar and dashboard views do not offer decimal measures or datetime, iso_week and year dimensions (the server, CLI, REST and MCP do; D72 "Not delivered"). The owner's rule is that UI parity is literal, so this needs a follow-up or the owner's written exemption | open |
| K2 | `round` on a decimal field cannot cut input beyond the parser's 38 significant digits and 80 characters (D72) | accepted, documented |
| K3 | A JSON float or a number beyond 64 bits sent to a text field is refused now; it used to be stored as its digits (D72). Integers were always refused | accepted |
| K4 | The D73 operator commands (`migrate`, `backup`, `restore`, `drill`, `storage check|attest|show`) are CLI-only; D73 asks the owner to approve that exemption in writing. Until then the status views (`backup list`, `migrate status|verify`, `storage check|show`) have no REST, MCP or UI | open (owner) |
| K5 | The findings D73 leaves open are listed in its record ("Second review round, Not delivered"): a plain backup may carry the `.obbk` name, the 400-day retention default overrides longer monthly counts, two mounts of one host directory are not recognised as one place, a long backup under the migration lock can time out a second starter, a persistent failing step grows the ledger | open |
| K6 | Hosted instances need a volume and `OBOP_INSTANCE_BACKUP_DIR` on the control plane before a tenant can apply a module update that rewrites records (D73); `--auto-migrate` without `OBOP_BACKUP_DIR` still only warns | open (owner: the volume) |
| K7 | GitHub Actions runs since the repository became public (2026-10-26). `ci.yml` was invalid YAML (an unquoted ": " in a step name), two gate tests and a counter test failed on Linux, the import dialog closed itself after the first import into an empty model and two OAuth e2e tests were wrong for the headless browser; all fixed, and `v0.2.0-r10` is the first commit with a fully green run. `image.yml` publishes `ghcr.io/spacecorps/obop:<sha>`; the package is private until the owner makes it public | open (owner: package visibility) |
| K8 | `vp run i18n:check` in `web/apps/site` reports "internal link targets differ" for `docs/agents/tokens.md` in six languages on Windows (the keys carry backslashes); not seen on other platforms, not investigated | open |
| K9 | Timing-sensitive tests: the counter windows of `shared_tests` were widened to 400 to 600 ms after a CI flake; the MCP session tests may still flake under load. The licences of every crate in the build graph (271) and of the installed JavaScript packages (195) were read on 2026-10-26: all permissive, MPL-2.0 only in cssparser, dtoa-short, resvg-js and lightningcss; run `cargo-deny` once it is part of the pipeline | open |
| K10 | Found by the last review of D74 and already open before it: a password change does not end a session minted by a sign-in whose password check began before the change (`obop-auth` `account.rs` `revoke_user_sessions` lists once; `mfa.rs` `session_outcome` re-reads only `disabled`). Fix: compare a credential version or `password_changed_at` at the re-read, or hold the guard across the write. Same window for admin password reset, passkey removal, admin MFA reset and a group change | open |
| K11 | The link of an imported record to its external id is lost in a request scope when its write is refused, so the next run adds the record again (`obop-connectors` `ingress.rs:313-323`, the retry is dead there) | open |
| K12 | A `consent.withdraw` answered "partial" cannot be repeated (`crates/modules/connectors/src/handlers.rs:640`), and `revoke_connection` withdraws consents with the lenient read after the state change committed (`handlers.rs:240`); `instance_fields()` shows a refused read as "flows off, EU only, no hosts" (`handlers.rs:115`) | open |
| K13 | Erasure: the "erasure owed" note is written by the worker thread after it took the event from an in-memory channel (`runtime.rs:1252`), so a restart in between loses it; the "erased" flow-log row is lost when its write is refused after the deletes (`egress.rs:621-638`); queued jobs and dead letters may keep a treated copy (one verifier of two) | open |
| K14 | The operator's host allow-list is a per-process snapshot that is never refreshed once known (`obop-connectors` `net.rs:218`), so a save in another process does not apply to this one until restart | open |
| K15 | `obop site set` rewrites all instance settings from a read that falls back to an empty object when the store refuses (`crates/obop/src/main.rs:1080`) | open |
| K16 | Low items of the last D74 review: the sign-in lockout writes a false "lockout" security event for an unreadable counter and counts a refused account read as a wrong password (`mfa.rs:233,307,433`, `lib.rs:259`); `obop migrate run` needs the uploaded-file store open even when no backup is taken (`migrate_cli.rs:156`); the strict restore-point gate reads a refused `archive_mode` as off (`gates.rs:823`); the stranded-payment sweep records a refused store call as a failed refund and skips the payment for an hour (`obop-payments` `lib.rs:758`); `uninstall_blocker`, queue `complete()` and a refreshed OAuth token pair swallow a refusal; no test covers the pool's in-flight mark, the backup schema-version and ledger sites; the D74 status line says "Revised twice" while its Review records six rounds | open |
