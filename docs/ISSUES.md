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
| A4 | No row-level permissions: nobody can be limited to their own records (leave requests, documents, tickets) | done (D58). `[model.access]` own, via, public_field, shared_with, see_all_group enforced in every read and write path with an oracle test; applied to hr leave requests, expenses, timesheets, documents and dashboards. Not applied: helpdesk requester, project privacy; aggregates ignore the rules |
| A5 | `/api/meta` does not say which fields are governed, so a user without the action sees a plain field | done. `/api/meta` fields carry governed, computed, unique, sequence; models carry `access`; MCP schemas and OpenAPI leave computed and numbered fields out of inputs |
| A6 | Dashboard "shared" flag is enforced only in the browser | done. `dashboards_dashboard` and its widgets are filtered by the server (owner, shared, dashboards.manager) |

## B. Operations, scale and durability
| # | Issue | Status |
|---|---|---|
| B1 | Sessions, MCP sessions, rate limits, forum and status data live in one process's memory (one process per database) | in progress |
| B2 | Connector retry queue is lost on restart | in progress |
| B3 | No key rotation or credential re-sealing for `OBOP_SECRET_KEY` | in progress |
| B4 | No backups: no `obop backup` / `restore`, no restore drill | in progress |
| B5 | No schema versioning or migration step for upgrades (`obop migrate`) | in progress |
| B6 | Every customer instance builds the Rust workspace on the shared server; no published image | in progress |
| B7 | Changing the admin 2FA policy is not audited; turning 2FA off does not revoke the user's other sessions | in progress |
| B8 | Existing instances created before D53 have no `OBOP_SECRET_KEY` unless an operator sets one | done (live instances) |

## C. Commerce
| # | Issue | Status |
|---|---|---|
| C1 | No payments: orders stay `pending` | in progress |
| C2 | Orders do not decrement stock; no discount codes; no taxes | in progress |
| C3 | No images anywhere (products, pages, posts); Open Graph has no image | in progress |

## D. Interface and localisation
| # | Issue | Status |
|---|---|---|
| D1 | List selection is per page; no "select all matching" | in progress |
| D2 | Record panel stacks below the list at tablet and phone widths | in progress |
| D3 | Server-paged lists sort bytewise, browser lists use the locale collator | in progress |
| D4 | Browser-side views are capped at 5,000 rows (kanban, pivot, counts) | open |
| D5 | Translated MCP and token docs pages (6 languages) are out of date; module card summaries and server blocker messages stay English | in progress |
| D6 | Changing language remounts the app and drops open dialogs | in progress |
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
| F1 | No fresh-eyes product QA pass over the whole product | queued after A to D |
| F2 | The live signup, instance, sign-in, 2FA and install flow has only run locally and against fakes | open (needs the owner's go-ahead to create a test account) |
