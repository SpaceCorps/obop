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
| E1 | No OAuth authorization server, so Claude Desktop / claude.ai cannot connect to a hosted instance | open |
| E2 | `is-agentic` score is cached; brand and developer-resource discoverability depend on indexing | open (needs time and a real domain) |
| E3 | No mailbox: contact details are GitHub only; privacy and terms are drafts for counsel | open (needs the owner) |
| E4 | New instances use `*.sliplane.app`; per-instance `obop.dev` subdomains need a wildcard DNS record | open (needs DNS) |
| E5 | `www.obop.dev` has no record | open (needs DNS) |
| E6 | Google and Microsoft connector adapters are definition-only; other adapters were tested only against local stand-ins | open |
| E7 | WebAuthn / passkeys; MCP spec 2026-07-28 | open |

## F. Quality gate
| # | Issue | Status |
|---|---|---|
| F1 | No fresh-eyes product QA pass over the whole product | done (section G) |
| F2 | The live signup, instance, sign-in, 2FA and install flow has only run locally and against fakes | open (needs the owner's go-ahead to create a test account) |

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

