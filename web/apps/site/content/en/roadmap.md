---
title: "Roadmap"
description: "An honest roadmap for obop: what is built today, what is partly built, what is not built, and the planned waves from the product plan, with the decisions that are still open."
lead: "What exists, what does not, and what is planned. Dates in the product plan are targets or proposals, not promises."
eyebrow: "Built and planned"
---

obop is below version 1.0. This page separates what runs today from what the plan describes. The source documents are `docs/ROADMAP.md` and `docs/DECISIONS.md` in the repository.

## Built today

- **Kernel:** one command dispatcher behind REST, MCP (over stdio and over HTTP at `/mcp`) and the CLI, with permission groups, field-level access, data classes, masking for agents, approvals and governed state fields.
- **Storage:** SQLite and PostgreSQL.
- **Backups:** `obop backup` takes a consistent copy while the server runs, sealed (encrypted) when a key is set; `obop restore` puts a backup back, and `obop drill` tests a restore in a scratch database. `OBOP_BACKUP_SCHEDULE`, together with `OBOP_BACKUP_DIR`, runs scheduled backups with retention and, if you set `OBOP_BACKUP_COPY_DIR`, a copy to another directory.
- **Apps:** six built-in apps with a page (Contacts, Products, CRM, Sales, Invoicing, Connectors) and 24 declarative packages bundled in the marketplace (22 of them have a page in the [list of apps](/apps/)), installable and removable at runtime.
- **Users:** Argon2id passwords, sessions, scoped agent tokens, two-factor sign-in (authenticator app or passkey).
- **Web app:** generated screens: launcher, list, board, record panel, Customize (theme, apps, module builder), light and dark themes.
- **Control plane:** `obop platform serve`, where people sign up and create instances with their own database, locally or on Sliplane. SpaceCorps does not host obop for other people yet.
- **This website:** multi-page, static, with Markdown copies of every page. The interface and the main pages are available in seven languages (machine-drafted); other pages show the English text.
- **Languages of the web app:** English, German, French, Polish, Finnish, Swedish and Spanish. The six translations are machine-drafted.
- **Tests:** Rust tests, and a Playwright end-to-end suite over the built app.

## Partly built or simplified

- Bundled apps compute only some figures. Totals of their own lines (journal entries in Accounting, orders in Sales, Purchase and Point of Sale, invoices in Invoicing), numbers from a series (for example invoices, journal entries, sales and purchase orders, receipts) and stock quantities once a stock move is confirmed are computed. Other figures are still typed in, for example tax amounts in Accounting and the change and cash differences at the till in Point of Sale.
- Accounting refuses to post an entry unless its debits equal its credits, but only at the moment of posting: a posted entry can still be edited afterwards, and closing a month or year is only a label that stops nothing.
- Knowledge visibility is a label, not an enforced rule.
- MCP is served over stdio and over HTTP at `/mcp`, with a browser sign-in (OAuth) for MCP clients. That sign-in has been tried only against our own test client and automated tests, not against Claude Desktop, claude.ai or Claude Code.

## Not built yet

- **Code tiers** for modules: CEL expressions, WebAssembly components, remote apps. Until then a package can use only the small built-in expression language for computed fields, checks on actions and conditions on effects; it runs no code of its own and cannot declare calls to other systems.
- **Agent token management:** a screen, listing, expiry and revocation of single agent tokens. Today an agent token never expires, and it stops working when its owner is disabled or offboarded. An app connected through the browser sign-in can be revoked under Customize, Security, Connected apps.
- **An audit trail of changes to business records.** A security log of events such as account, sign-in, security-setting and module events exists; no change history is switched on for any model yet.
- **Row rules in most apps.** A few apps have them; the others show every record to everyone who can read the model.
- **Single sign-on** other than Microsoft Entra. Entra sign-in is built and has been tried only against a stand-in.
- **A schema manager with typed columns**; records are JSON documents. Schema versions, the migration ledger (`obop migrate`) and the `[[migration]]` steps of module updates exist; the component that issues DDL and the check that the previous release runs against the new schema do not.
- **Point-in-time recovery and a customer-facing export tool.** `obop backup` and `obop restore` exist (see above), but obop does not run point-in-time recovery (PostgreSQL does it, and it has not been tried on Sliplane), and the planned export job for a whole business is not built. A control plane does not switch on scheduled backups of the instances it creates.
- **Country packs** (tax, e-invoicing, charts of accounts) and the e-invoicing formats the plan names. The only bundled country package today is the experimental Accounting Sweden (voucher series, VAT codes and a VAT report; nothing in it has been checked against Skatteverket). Germany is the first country and the only one the owner has decided; France second and Poland third are proposals.
- **Importers** for other systems such as Odoo, QuickBooks, Xero and ERPNext. A generic CSV import (`obop import` and the Import dialog) and a SIE file import for accounting exist.
- **A hosted service.** SpaceCorps does not host obop for other people yet. A free sandbox with sample data only, run by SpaceCorps, is planned for the public beta; its details are still proposals.
- **Billing.** Nothing is charged. The one planned paid service is dedicated hosting; it is not offered yet.

## The plan in waves

The product plan names five waves. The owner ruled to keep the dates and cut scope instead (OR-13), so the wave windows below are the plan's targets; the owner is still asked to confirm that reading. The dates of the 0.x releases, the French and Polish packs and later release series are proposals. No date is a promise.

From wave 2 on, the apps and features in this table are an ordered backlog, not a promise: the plan's rule is that no new app is started without a design partner. Several apps named in the W3 row (manufacturing, point of sale, website, e-commerce and helpdesk) already exist as bundled packages, marked Beta or Experimental in the [list of apps](/apps/). The plan proposes, as the exit of wave 3, that at least two apps of its second tier (such as manufacturing and point of sale) are stable, each with a design partner; this is a proposal, not decided. The table lists the main items of each wave, not every one.

| Wave           | Window             | Aims                                                                                                                                                                                                                                                                                                                                                         |
| -------------- | ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| W0 Foundations | 2026-10 to 2026-12 | The kernel, first tagged preview build, security policy.                                                                                                                                                                                                                                                                                                     |
| W1 Loop        | 2027-01 to 2027-09 | The smallest loop: kernel, invoicing, accounting with tax, German e-invoicing, CRM, sales and basic stock; the generated command line, REST API and MCP for every shipped module; code tiers X1 and X2 (X2 only if its limits spike passes); the CSV and Odoo importers; the German country pack; a public alpha and a public beta with no commercial offer. |
| W2 Release     | 2027-10 to 2028-06 | Automation rules and the approvals engine (approval policies such as auto, confirm and approve-by-role; today a person approves each change an agent proposes), multi-company, a QuickBooks importer, a first-stage package registry, the web UI parity work, and 1.0, the first stable release, with a two-year security-fix window.                        |
| W3 Breadth     | 2028-07 to 2029-06 | The services apps first, then in demand order: the extension platform (WebAssembly host, remote apps), Xero and ERPNext importers, bank feeds, stock extensions, more apps (manufacturing, point of sale, website, e-commerce, helpdesk and others), a Polish pack (proposed), payroll engine, custom domains.                                               |
| W4 Ecosystem   | from 2029-07       | Later release series (numbering and cadence not decided yet) and verticals.                                                                                                                                                                                                                                                                                  |

## Decisions that are still open

Several choices are recorded as proposals and wait for the project owner, including the exact calendar. The licence is settled: Apache-2.0 for the whole project. The hosting prices on this site are marked as planned and not charged.

## How to follow along

The [changelog](/changelog/) is generated from the commit history. The repository has the decision register and the full roadmap.
