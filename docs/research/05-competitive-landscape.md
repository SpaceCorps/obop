# obop competitive landscape: open-source and commercial business suites

Research date: 2026-10-01. Status labels used throughout: **[V]** verified against a live primary source during this run, **[S]** secondary source (Wikipedia, blog, review), **[I]** inference by the author, **[M]** remembered from training and not re-verified (treat as a hint only).

## Scope & method

No general web-search tool was available in this run, so discovery used the GitHub REST API (`gh api`: repo metadata, releases, LICENSE files, git trees, GitHub search over topics and languages), the crates.io and PyPI JSON APIs, Hacker News Algolia, and direct page fetches of vendor pricing/docs pages. Roughly 90 distinct API calls and page fetches were made. Vendor sites for NetSuite, SAP, Sage and Oracle returned HTTP 403 to automated fetching, so their prices are **not** verified. Star counts are a weak proxy for traction (they measure attention, not deployments) and contributor counts were derived from API pagination (approximate).

---

## 1. Headline findings

1. **Odoo is the reference and it is still accelerating.** `odoo/odoo` has 54.8k stars, 304 contributors, and the `20.0` branch reports `version_info = (20, 0, 0, FINAL, 0, '')` while `master` is `(20, 1, 0, ALPHA, 1, '')`, i.e. `20.1 alpha` [V: github.com/odoo/odoo, `odoo/release.py`]. Odoo's own "Odoo 20 Release Notes" page is dated September 2026 (exact day not stated) [V: odoo.com/odoo-20-release-notes]. Odoo claims "28 million users", "40k+ community apps" and "19.90 EUR / month for ALL apps" on its homepage [V: odoo.com]. Wikipedia reports 2023 revenue of EUR 282M (+33%) and a November 2024 secondary round at up to a USD 5.26B valuation [S: en.wikipedia.org/wiki/Odoo].
2. **No Rust business suite has meaningful traction.** GitHub `topic:erp language:Rust` tops out at 53 stars (a print agent); the actual Rust ERPs are `forgeerp` (40 stars), `rerp` (23), `obvia` (16); the stale `beowulf1416/idEAS` (27 stars, AGPL-3.0, last push 2023-02, README says it is moving to another org) also outranks `rerp`. Rust *does* have traction in adjacent infrastructure: Hyperswitch (payments, 45.3k stars), Stalwart (mail, 14.9k), Macro (unified workspace incl. CRM, 4.5k), Meteroid (billing, 1.2k), rustledger (plain-text accounting, 400) [V: GitHub API]. The white space is real and so is the warning sign.
3. **Open-core instability is the dominant licensing story of 2026.** Cal.com closed its production source on 2026-04-14 citing AI-driven vulnerability discovery and left an MIT "Cal.diy" fork with enterprise features stripped [V: cal.com/blog/cal-com-goes-closed-source-why; github.com/calcom/cal.diy]. Akaunting is under BSL 1.1 (2 users / 1 company / 1,000 invoices free), Invoice Ninja under Elastic License 2.0, Medusa and Twenty carry commercial "Enterprise" carve-outs [V: LICENSE files].
4. **"One platform" wins on data consistency and operations, not on list price.** Using only verified list prices, a 10-person composed SaaS stack can cost the same as or less than Odoo Standard (section 7). The case for one platform is integration labour, a single transactional truth, and the glue nobody budgets for.
5. **Customisation, localisation and partner ecosystems are the moats**, not the core transaction code. ERPNext v16 ships 529 DocTypes plus 160 in the separate HRMS app; Odoo 20 Community ships 229 `l10n_*` modules; ERPNext cites 200+ implementation partners [V: GitHub trees; frappe.io/erpnext/pricing]. A compiled Rust product must solve runtime customisation deliberately.

---

## 2. Open-source suites: comparison table

Stars/contributors/releases from GitHub API on 2026-10-01 [V]. "Backer" is the commercial entity, where identifiable.

| Project | License [V unless noted] | Stack | Stars / contrib. | Latest release [V] | Backer / model | Scope |
|---|---|---|---|---|---|---|
| **Odoo** (odoo/odoo) | Community LGPL-3 (LICENSE says "LGPLv3"); Enterprise proprietary | Python, PostgreSQL, Owl JS | 54.8k / ~304 | branch 20.0 FINAL (19.0 stable released 2025-09-18 [S]) | Odoo S.A. (Belgium), open-core SaaS + Enterprise | Full suite; the 20.0 docs index lists roughly 55 apps across 8 categories (author's count of the docs sidebar; the earlier "~24" was unsupported) |
| **ERPNext** (frappe/erpnext) | GPL-3.0 (license.txt; Frappe's pricing page says AGPL-3.0, a discrepancy) | Python, Frappe framework, MariaDB, Vue/jQuery | 39.7k / ~280 | v16.37.0 and v15.121.6 (2026-09-30) | Frappe Technologies (India); Frappe Cloud hosting | Accounting, stock, mfg, assets, selling/buying, quality, projects, subcontracting, support; HR/payroll in separate `hrms` |
| **Dolibarr** | GPL-3+ | PHP (>=7.2 badge), MariaDB/MySQL/PostgreSQL | 7.7k / ~279 | 24.0.1 (2026-09-07) | Dolibarr association (2009) + partners + DoliStore | Contacts, quotes, invoices, orders, stock, agenda, HR, ECM, manufacturing (README) |
| **Tryton** | GPL-3.0-or-later (PyPI) | Python (trytond), PostgreSQL, GTK/web (sao) | 226 (GitHub mirror) | trytond 8.0.10 (2026-09-16); 8.0.0 on 2026-04-20; 6-month series, LTS 5 years (tryton.org) | Tryton Foundation (Belgian private foundation, "Fondation Privée" [V: tryton.org/foundation]; the page does not say "non-profit") | 221 modules in repo; strong accounting/stock/production; no HR/payroll module |
| **iDempiere** | GPL-2 | Java, OSGi, ZK, PostgreSQL/Oracle | 665 / ~66 | 13 "Orion" LTS (2026-03-12, idempiere.org) | Community/sponsors | ERP/CRM/SCM, strong multi-entity accounting |
| **metasfresh** | GPL (README badge only; no LICENSE file in the repo root listing, version unverified) | Java, Spring, React | 2.4k / ~67 | last GitHub release tag 5.175 (2023-06-27), yet commits continue daily | metasfresh GmbH [M] | Food/wholesale focus |
| **Apache OFBiz** | Apache-2.0 | Java/Groovy | 1.1k / ~101 | 24.09.07 (June 2026, ofbiz.apache.org) | Apache Software Foundation | Framework + ERP/e-commerce; base for commercial products |
| **Axelor Open Suite** | AGPL-3.0 | Java, own platform (AOP) | 979 / ~107 | v9.1.8 (2026-09-17) | Axelor SA (France) | CRM, sales, finance, HR, projects, SCM, production, multi-company (README) |
| **AureusERP** | MIT | PHP, Laravel 13, FilamentPHP 5 | 12.0k / ~24 | v1.6.0 (2026-08-27) | Unclear | "Odoo-like" plugin ERP, created 2025-01 |
| **Carbon** (crbnos) | AGPL-3.0-only, except "Enterprise files" (`packages/ee/` and any file named `*.ee.*`) under a commercial Carbon Commercial License [V: LICENSE] | TypeScript, Postgres | 2.7k / ~48 | n/a | Startup | Manufacturing ERP + MES + QMS, lot/serial traceability |
| **Open Mercato** | MIT | TypeScript | 1.8k / ~138 | v0.8.0 (2026-09-18) | Startup | "AI-engineering" CRM/ERP/commerce framework, created 2025-09 |
| **Huly** (hcengineering) | EPL-2.0 | TypeScript/Svelte | 27.8k / ~123 | v0.7.426 (2026-07-05) | Huly Labs | All-in-one project mgmt, chat, CRM, HR |
| **NocoBase** | custom "NocoBase License Agreement" (updated 2026-02-24) that incorporates Apache-2.0 plus supplementary terms, with paid Commercial Editions/plugins [V: LICENSE.txt] | TypeScript | 24.4k / ~106 | v2.2.20 (2026-09-30) | NocoBase | No-code business-system builder |
| **Ever Gauzy** | AGPL-3.0 | TypeScript/NestJS | 8.1k / ~92 | v111.44.48 | Ever Co | ERP/CRM/HRM/ATS/PM |
| **Akaunting** | **BSL 1.1**, change to GPLv3 four years after each version's publication; the grant also bars white-labelling and offering an "Accounting Service" to third parties, and its cap wording is literally "more than two users or one company or one thousand invoices" [V: LICENSE.txt] | PHP, Laravel | 10.2k / ~114 | 3.2.4 (2026-09-17) | Akaunting Inc. | SMB accounting; paid app store |
| **Invoice Ninja** | **Elastic License 2.0** (no managed-service, no removing license-key features) | PHP, Laravel | 10.2k / ~110 | v5.13.43 (2026-09-18) | Invoice Ninja LLC | Invoicing, quotes, expenses, tasks |
| **Bigcapital** | AGPL-3.0 | TypeScript | 3.9k / ~44 | v0.25.42 (2026-09-23) | Bigcapital | Accounting + inventory |
| **InvenTree** | MIT | Python/Django, DRF, React | 7.7k / ~173 | 1.5.6 (2026-09-26) | InvenTree developers | Parts, stock, BOM, purchasing, build orders; plugin system |
| **SuiteCRM** | AGPL-3.0 | PHP (this repo is the 7.x line); SuiteCRM 8 is a separate Angular + Symfony rebuild in `SuiteCRM/SuiteCRM-Core` (285 stars, AGPL-3.0) | 5.8k / ~203 | v7.15.2 (2026-07-31); SuiteCRM 8: v8.10.2 (2026-07-31) [V: GitHub releases] | SalesAgility | CRM |
| **Twenty** | AGPL-3.0 + commercial files marked `/* @license Enterprise */` (a per-file header, not an `/ee` directory) + MIT packages (SDKs, `twenty-ui`, `twenty-apps`) | TypeScript, NestJS, React, Postgres, Redis | 57.8k / ~464 | v2.44.0 (2026-10-01) | YC-backed startup [M] | CRM, custom objects, workflows, MCP server |
| **Medusa** | MIT + Enterprise materials (ENTERPRISE-LICENSE.md) | TypeScript | 36.5k / ~455 | v2.21.2 (2026-09-28) | Medusa Inc. | Headless commerce |
| **Saleor** | BSD-3 | Python | 23.4k / ~260 | 3.23.37 (2026-09-30) | Saleor Commerce | Headless commerce |
| **Plane** | AGPL-3.0 | TypeScript (Next/React), Python | 60.2k / ~173 | v1.4.2 (2026-08-23) | Plane Software | Projects/issues (Jira/Linear alternative) |
| **Lago** | AGPL-3.0 | Ruby (lago-api) + Go | 10.6k / ~48 | v1.53.0 (2026-09-08) | Lago (YC) [M] | Usage billing |
| **Kill Bill** | Apache-2.0 | Java | 5.8k / ~47 | 0.24.22 (2026-09-29) | Kill Bill community/vendors | Subscription billing, payments |
| **Cal.diy** (ex Cal.com) | MIT (stripped fork) | TypeScript, Next, tRPC, Prisma | 48.8k | v6.2.0 (2026-03-01, no release since closure) | Cal.com Inc. (closed core) | Scheduling |

Notes on what the table implies:

- **Copyleft-with-a-business works at scale**: ERPNext (GPL-3) and Dolibarr (GPL-3+) have ~280 contributors each after 15 years, and Frappe monetises hosting only (Frappe Cloud from USD 5/month; "pay for what you use, not per user"; self-hosting free) [V: frappe.io/erpnext/pricing].
- **Open-core forks the community**: Odoo Community gets 229 country modules but withholds accounting features, payroll, helpdesk, planning, documents, sign, studio, quality, marketing automation, knowledge, approvals. This is directly visible in the 20.0 `addons/` tree, which contains `account` (invoicing) but no `account_accountant`, `hr_payroll`, `helpdesk`, `documents`, `sign`, `knowledge`, `planning`, `quality`, `marketing_automation`, `studio` [V: GitHub tree odoo/odoo@20.0]. The community reaction is OCA (Odoo Community Association): 271 public repos [V: GitHub org], and `OCA/OCB`, a Community fork (376 stars).
- **Suites that split**: Frappe moved HR/payroll into `frappe/hrms` (8.9k stars, 160 DocTypes) and spun out standalone CRM (3.7k), Helpdesk (3.4k), Drive, Insights, LMS (3.3k), Builder, Wiki, Books (5.0k) [V]. Even the integrated-suite vendor builds point-solution competitors to defend against best-of-breed.

### 2.1 Project-by-project detail for the most relevant

**Odoo.** Stack: Python + PostgreSQL + Owl frontend. Repo at 20.0 contains 642 addon directories: 413 functional modules plus 229 `l10n_*` localisations [V: GitHub tree]. The glue-module explosion is instructive: names like `sale_project_stock_account`, `mrp_subcontracting_dropshipping`, `website_event_booth_sale_exhibitor`, `pos_self_order_bancontact_pay` show that cross-app integration costs combinatorial bridge modules. Docs list Odoo 20 apps: Finance, Sales (incl. Amazon/Shopee/Lazada/TikTok connectors), Websites, Supply Chain (Inventory, Manufacturing, Purchase, Barcode, Quality, Maintenance, PLM, Repairs), HR (incl. Payroll), Marketing, Services (Project, Timesheets, Planning, Helpdesk), Productivity (Documents, Sign, Knowledge, Appointments, WhatsApp, Phone, AI with "AI agents" and an "AI MCP server") [V: odoo.com/documentation/20.0]. Fiscal localisations: the 20.0 docs list 103 countries, 46 with dedicated pages [V: odoo.com/documentation/20.0/applications/finance/fiscal_localizations.html; the earlier 45 figure came from the 19.0 docs]. API: a JSON-2 external API introduced in 19.0, with a migration section from XML-RPC/JSON-RPC [V: odoo.com/documentation/20.0 external API page]. Pricing (EUR per user per month, discount applies 12 months for initially ordered users): with the "Yearly" toggle Standard is 19.90 (list 24.90) and Custom 35.90 (list 44.90); with "Monthly" billing Standard is 24.90 (list 31.10) and Custom 44.90 (list 56.90). Custom adds Studio / Agentic AI, multi-company, external API, and on-premise or Odoo.sh hosting (Odoo.sh hosting cost not included); "One App Free" gives one app (plus its dependencies) with unlimited users; a Light User price of 7.90 is also shown; IAP credits (SMS, AI scanning) and implementation are extra [V: odoo.com/pricing; yearly vs monthly mapping confirmed from the page's `yearly_price` / `monthly_price` HTML classes, and the homepage's "19.90 EUR / month for ALL apps" uses the key `yearly_standard_discounted`].

**ERPNext / Frappe.** Metadata-driven: everything is a DocType (JSON schema -> table + form + API + permissions). ERPNext v16 counts: accounts 191, stock 75, manufacturing 48, setup 40, crm 28, assets 26, buying 20, selling 18, quality 16, projects 15, subcontracting 13, support 11 (529 total) [V: GitHub tree version-16, counted by me]; HRMS adds hr 117 + payroll 43. Frappe framework is MIT (10.9k stars), ERPNext GPL-3. Releases are rapid (v16.36.1 on 2026-09-28, v16.37.0 on 2026-09-30) with v15 maintained in parallel. Strengths: open accounting (not held back), DocType customisation UI, low-cost hosting, partner network of "200+" [V: frappe.io pricing]. Weaknesses [I]: dated UX historically (v16 adds redesigned UI assets and a `version-16-polished` branch), heavy multi-process ops stack (bench, MariaDB, Redis, Node, supervisor), weaker EU e-invoicing (a two-DocType `edi` module), India-centric localisations.

**Dolibarr.** PHP monolith, low hardware needs, very broad for freelancers/SMBs, mature partner/marketplace model (DoliStore) [V: dolibarr.org]. The GitHub project is on 24.0.1 after 24.0.0 (2026-08-20) with parallel 23.0.x and 22.0.x maintenance lines [V]. Weakness [I]: dated architecture and UI; accounting is simpler than ERPNext/Odoo Enterprise; no modern multi-tenant SaaS story (dedicated-instance hosting model).

**Tryton.** The cleanest architecture in the field: 221 fine-grained modules in a monorepo (e.g., `account_invoice_factur_x`, `edocument_peppol`, `account_payment_sepa`, `account_stock_anglo_saxon`, `sale_subscription`, `stock_package_shipping_ups`), a strict module dependency graph, and a 6-month release cadence with 5-year LTS [V: GitHub tree tryton/tryton modules; tryton.org/download]. Backed by the Tryton Foundation (a Belgian private foundation), no VC [I for "no VC"]. Weakness: 226 GitHub stars (the GitHub repo describes itself as "Mirror of tryton"; `code.tryton.org/tryton` redirects to `foss.heptapod.net/tryton/tryton`, so the canonical repo is on Heptapod/GitLab [V]), no HR/payroll/helpdesk/POS-as-product, small ecosystem, Python/GTK desktop heritage. **Lesson for obop**: Tryton proves that a tight, well-factored, correct core gets you respect among accountants and integrators but not mindshare.

**Twenty.** 57.8k stars in ~46 months (repo created 2022-12-01), TypeScript/NestJS/React/Postgres/Redis, MCP server built in, "unlimited seats, records, custom objects" on all tiers, Pro USD 9/user/month (yearly), Organization USD 19, Enterprise from USD 50k/year [V: twenty.com/pricing]. Licensing: AGPL-3 with commercial `@license Enterprise` files and some MIT packages [V: LICENSE]. Lesson: a modern, design-led UX plus custom-objects-as-core beats incumbents on developer attention quickly.

**Medusa.** MIT core with an Enterprise license (ENTERPRISE-LICENSE.md) whose listed Enterprise Materials are the RBAC and SSO modules; audit logs are also Enterprise-tier on the pricing page; Cloud Develop from USD 29, Launch from USD 99, Scale from USD 299 per month, 0% GMV fee [V: github.com/medusajs/medusa ENTERPRISE-LICENSE.md; medusajs.com/pricing]. Its Medusa-v2-API-compatible Rust re-implementation `oonid/toko-rs` has 1 star [V: GitHub search].

**Cal.com -> Cal.diy.** Announcement 2026-04-14 (HN thread 391 points, 317 comments, 2026-04-15): production code moved to a private repo, public `calcom/cal.diy` is MIT with Teams, Organizations, Insights, Workflows, SSO/SAML removed; company says it hopes "one day" to return to open source "as the security landscape evolves" (the blog's wording is "one day", not "someday") [V: cal.com blog; github.com/calcom/cal.diy README; news.ycombinator.com]. The Cal.diy README itself warns it is "strictly recommended for personal, non-production use" and points commercial users to Cal.com, so the MIT fork is explicitly not positioned as production-grade [V: README]. The blog page's own byline reads Apr 14, 2026 (page metadata shows a later modification date); the HN submission is 2026-04-15. Its stated rationale: AI tools make reading public code a vulnerability-discovery advantage for attackers [V: blog, quoted paraphrase]. The practical consequence for every self-hoster of the former 40k+-star project was a fork of unknown viability.

---

## 3. Rust and Rust-adjacent landscape

GitHub/crates.io search was run across topics `erp`, `accounting`, `crm`, `inventory`, `pos`, `ecommerce`, `invoice`, `ledger`, `hrm`, `bookkeeping`, `helpdesk`, plus free-text queries. crates.io has **no** crate matching ERP/CRM/POS/payroll with meaningful downloads; results were dominated by false positives [V: crates.io API].

### 3.1 Rust projects with real traction (none is a suite)

| Project | What | Stars / contrib. | License | Status |
|---|---|---|---|---|
| **juspay/hyperswitch** | Composable open-source payments orchestration | 45.3k / ~256 | Apache-2.0 | v1.127.0 (2026-09-28); created 2022-10; Juspay-backed |
| **stalwartlabs/stalwart** | All-in-one mail/collaboration server (JMAP/IMAP/SMTP) | 14.9k / ~54 | AGPL-3.0 + separate Stalwart Enterprise License (`LICENSES/` holds `AGPL-3.0-only` and `LicenseRef-SEL`) [V] | v0.16.24 (2026-09-27) |
| **macro-inc/macro** | "All-in-one workspace": email, chat, docs, tasks, agents, calls, CRM; Rust + SolidJS | 4.5k / ~25 | AGPL-3.0 | Created 2025-11; calendar-versioned releases (v2026.9.30.0); explicit "one operating system for a company" thesis [V: README] |
| **meteroid-oss/meteroid** | Pricing/billing platform | 1.2k / ~15 | AGPL-3.0 | v1.0.0-rc7 (2026-08-24); README badge "status: experimental" [V] |
| **rustledger/rustledger** | Beancount-compatible plain-text accounting | 400 / ~18 | GPL-3.0 | v0.24.0; created 2026-01 |
| **ad-si/Transity** | CLI plain-text accounting | 653 / ~7 | AGPL-3 | last release v0.8.0 2020 |
| **tackler-ng/tackler** | Git-native bookkeeping engine | 164 | Apache-2.0 | v26.08.1 |
| **typst/typst** | Typesetting (strong fit for invoices/reports) | 56.4k | Apache-2.0 | v0.15.1 (2026-07-17); `typst-pdf` 2.55M downloads |
| **AppFlowy** | Notion-like workspace (Flutter UI, Rust core) | 77.0k / ~363 | AGPL-3 | 0.14.6 (2026-10-01) |

### 3.2 Rust ERP/commerce attempts (all nascent)

| Project | Claim | Stars / contrib. | Reality check |
|---|---|---|---|
| **Ahmadnoorkhan1/forgeerp** | Event-sourced multi-tenant micro-ERP (MIT) | 40 / 2 | No releases; README is architecture-first [V] |
| **microscaler/rerp** | "71 independent microservices", OpenAPI-first, PolyForm Shield 1.0.0 (source-available, not OSI) | 23 / 3 | v0.1.3 (2026-01); microservice-first design with 3 contributors |
| **kovacsdavid/obvia** | Rust (Axum, SQLx) + React (Vite, Vitest) multi-tenant ERP, AGPL | 16 / 2 | v0.30.2-beta; a direct analogue of the obop stack constraint |
| **beowulf1416/idEAS** | "Enterprise Application Software": multi-tenant, Postgres + Kafka + Angular front end | 27 | AGPL-3.0; last push 2023-02, README says the project is moving to another org [V] |
| **PDG-Global/rcommerce-core** | Headless commerce in Rust | 4 | AGPL |
| **Reknij/elerp**, **ospos-org/open-stock**, **xelth-com/eckwms** | WMS/inventory/stock | 23 / 13 / 1 | Stale or tiny |
| **dividebysandwich/rustpos**, **AncientiCe/apex-edge** | POS; offline-first store hub with outbox to HQ | 14 / 2 | Hobby scale |
| **Govcraft/beankeeper** | Double-entry accounting primitives library | 10 | Apache-2.0; v0.8.0 |
| **Pher217/CoolERP** | "Agent-native ERP", typed MCP surface, Postgres-enforced | 1 | 2026-08 |
| **RaisFast/raisfast** | Single-binary CMS/e-commerce | 53 | Apache-2.0 |

### 3.3 Rust ecosystem building blocks (crates.io, 2026-10-01 [V])

| Crate | Version | Total downloads | Recent (90d) | Relevance |
|---|---|---|---|---|
| axum | 0.8.9 | 496M | 124M | HTTP framework |
| sqlx | 0.9.0 | 156M | 41M | Async Postgres, compile-time queries |
| diesel | 2.3.13 | 38M | 8M | Sync ORM |
| sea-orm | 2.0.4 | 26M | 4.5M | Async ORM, entity codegen |
| rust_decimal | 1.43.0 | 151M | 36M | Money arithmetic |
| utoipa | 6.0.0 | 50M | 16M | OpenAPI |
| askama / minijinja | 0.16.1 / 2.24.0 | 51M / 36M | 13M / 12M | Templates |
| cedar-policy | 4.13.0 | 9.3M | 2.9M | Authorisation policy engine |
| rhai | 1.26.1 | 12M | 4.3M | Embedded scripting |
| tantivy | 0.26.2 | 19M | 4.1M | Full-text search |
| lettre | 0.11.23 | 19M | 6.8M | SMTP |
| async-stripe | 0.41.0 | 5.5M | 1.1M | Stripe (1.0.0-rc.9 line active) |
| en16931 / en16931-formats | 0.7.0 | ~4k | ~4k | EN 16931 model + UBL/CII; brand new (updated 2026-09-04), low adoption |
| zugferd | 0.1.7 | 5.9k | 180 | Thin |

Also: Loco (Rails-style framework) 9.4k stars v1.2.0; Leptos 21.3k v0.8.21; Dioxus 39.3k v0.7.10 [V: GitHub]. **Observation**: the backend plumbing (HTTP, DB, money, PDF, auth policy, scripting, search) is mature; what is missing is *domain* libraries (ledger engine, tax engines, e-invoice formats, bank-statement parsers, payroll rules). Every ERP module obop builds is first-of-kind in the Rust ecosystem. One domain-library exception found while verifying: `cala-ledger` 0.31.0 (an embeddable double-entry ledger on Postgres from GaloyMoney/cala, Apache-2.0, 41 stars, ~155k crates.io downloads, updated 2026-09-23) [V: crates.io API, GitHub API]; it is worth evaluating against building a posting engine from scratch. Payroll-adjacent crates are tiny (e.g. `legalios`, ~11k downloads).

Non-Rust ledger infrastructure for reference: TigerBeetle (Zig, 17.1k stars, Apache-2.0), Formance Ledger (Go, 1.4k, MIT), Blnk (Go, 539) [V]. These show demand for ledger-as-a-service but are financial-infrastructure products, not SMB accounting.

---

## 4. Commercial references

| Product | Model / pricing (verified where marked) | Scope | Strengths | Weaknesses / lessons |
|---|---|---|---|---|
| **Dynamics 365 Business Central** | Essentials USD 80, Premium USD 110, Team Members USD 8 per user/month, billed yearly, sold via partners [V: microsoft.com pricing]. Agents (Sales Order Agent, Payables Agent) need separately purchased Copilot Credits + Azure subscription [V] | Finance, sales/marketing, purchasing, inventory, supply-chain planning, projects, warehouse; Premium adds service + manufacturing [V]. Heritage: Navision 1987, Microsoft bought 2002, BC launched 2018-04-02; AL language, AppSource extensions [S: Wikipedia] | Microsoft ecosystem (Office, Power BI), partner channel | Per-seat pricing + partner-mediated; payroll/POS/e-commerce via ISVs [I: the BC pricing page does not mention payroll, POS or e-commerce at all; Wikipedia says AppSource ISV add-ons cover reporting, payroll and commerce] |
| **NetSuite** | Not public; vendor site returned 403. Wikipedia: subscription varies by modules and users, OneWorld extra. Oracle bought it for ~USD 9.3B, closed 2016-11-07 [S] | GL/AP/AR, inventory, CRM, e-commerce, HCM/payroll, PSA, WMS, revenue recognition, OneWorld multi-entity (27 languages) [S] | Depth, multi-entity, SuiteScript customisation | Price opacity, implementation cost [M] |
| **SAP Business One** | Not public; 403. Wikipedia lists versions 6.2 to 10.x and a HANA edition [S] | SME ERP | Ecosystem of SAP partners | Partner-led pricing [M] |
| **Zoho One** | Flexible-user tier shown as USD 90 (unit/toggle not stated next to the figure; the page FAQ says plans are priced "per user per month with annual licensing"); All-Employee tier requires licence for all staff, price not rendered; "50+" apps in Standard, "15+" in Essentials [V: zoho.com/one/pricing.html] | 50+ apps (CRM, Books, Inventory, People, Desk, Projects, Campaigns, Commerce...) [V count, M names] | Breadth at a fixed bundle price | Integration between Zoho apps is uneven [I] |
| **Sage** | Pricing pages 403. FY2025 revenue GBP 2,513m, operating profit GBP 530m, ~6M SMB customers, products Sage Intacct/50/200/X3 [S: Wikipedia] | Accounting, payroll, HR, financial mgmt | Accountant channel, localisation | Fragmented product line [I] |
| **Xero** (US) | Early USD 27 (promo 5.40 x 3 months), Growing USD 59, Established USD 97 per month; Early capped at 20 invoices + 5 bills; "no per-user licence fees"; payroll add-on USD 36/month + USD 6 per employee [V: xero.com/us] | Accounting, basic inventory (Inventory Plus), projects, expense claims | Unlimited users, bank feeds, app marketplace | Thin outside accounting |
| **QuickBooks Online** | Simple Start USD 38 (1 user), Essentials 85 (3), Plus 140 (5), Advanced 340 (25); 50% off 3 months [V: quickbooks.intuit.com] | SMB accounting | US dominance, accountants | Per-tier user caps drive upsell |
| **Holded** (EUR, monthly billing) | Plus 15, Basic 29, Standard 59, Advanced 99, Premium 199; 50% off for 3 months; Basic 2 users, Standard 4, Advanced 7; accountant seat free; "900,000+ users" claim; Plus has 1 user + accountant and Basic caps at 1,000 invoices/year; HR (from EUR 1.50/employee/month), Inventory (from EUR 25/month) and POS (from EUR 25/store/month) are paid add-ons in the Holded Store; extra users EUR 10/month [V: holded.com/pricing] | Invoicing, accounting, CRM, projects, plus inventory, HR, POS as paid add-ons (one platform for SMB in Spain/EU) | A direct European "one platform" SaaS comparator | Smaller, closed |
| **Teamleader** | "From EUR 37.5"; SMART/GROW/FLOW packages; Peppol send (SMART) and receive (GROW); project management and planning are paid add-ons (EUR 35/month each, shown against a struck-through EUR 45); 50% off from the 6th user, 17% off annual, 25% off the first invoice, second user free [V: teamleader.eu/pricing] | CRM, quotes, invoices, time tracking, (Peppol) | Benelux SME fit, Peppol | Narrow (no inventory/mfg) |
| **Shopify POS** | POS Pro add-on SEK 870 per location/month on the Swedish page; Basic plan SEK 359, Grow 970, Advanced 4,240 (each with a SEK 9/month intro offer for 3 months; Plus is EUR 2,100/month on a 3-year term); only casual in-person selling (limited staff, simple customer profiles, returns at original location) is included on every plan, while unlimited staff, inventory management and omnichannel tools need POS Pro (standard on Plus) [V: shopify.com/pos/pricing; US page not fetched] | Omnichannel retail | Best-in-class retail POS + ecommerce | Inventory/accounting are shallow; retail-only |

**Market-structure lesson**: the SMB bands are (a) accounting-first SaaS (Xero, QuickBooks, Sage) priced per entity or tier with low or no per-user fees, (b) suite-first per-user pricing (Odoo, BC, Zoho One, NetSuite) and (c) vertical/channel SaaS (Shopify POS, Teamleader, Holded). Per-user pricing is being challenged on both sides (Xero: "no per-user licence fees"; Frappe: "not per user"; Twenty: unlimited seats).

---

## 5. Feature-coverage matrix

Legend: **●** first-party, in the base product; **◐** partial, separate add-on/app, or paid tier; **○** absent or third-party only; **?** not verifiable. Evidence basis in the last column.

| Domain | Odoo CE 20 | Odoo EE 20 | ERPNext (+Frappe apps) | Dolibarr | Tryton | BC | NetSuite | Zoho One | Evidence |
|---|---|---|---|---|---|---|---|---|---|
| Invoicing / AR / AP | ● | ● | ● | ● | ● | ● | ● | ● | CE `account`; ERPNext accounts 191 DocTypes |
| Full GL / financial reporting / assets / budgets | ◐ (no `account_accountant`) | ● | ● | ◐ | ● (`account_asset`, `account_budget`, `account_consolidation`) | ● | ● | ● (Books) | GitHub trees |
| Bank statement import / reconciliation | ◐ | ● | ● | ◐ | ● (`account_statement_*` CODA/MT940/OFX/AEB43) | ● | ● | ● | Tryton tree [V]; others [M/I] |
| EU e-invoicing (Peppol, UBL/CII, Factur-X) | ● (`account_peppol`, `account_edi_ubl_cii`) | ● | ◐ (`edi`, regional) | ○ [I] | ● (`edocument_peppol`, `account_invoice_factur_x`) | ◐ [M] | ◐ [M] | ◐ [M] | Odoo, Tryton trees [V] |
| Country fiscal localisation | ● (229 `l10n_*`) | ● | ◐ (`regional`) | ◐ | ◐ (`account_be`, `account_fr`, `account_syscohada`, ...) | ● | ● | ◐ | [V] trees |
| Sales (quote to order) | ● | ● | ● | ● | ● (`sale_*` x ~35) | ● | ● | ● | trees/README |
| CRM | ● | ● | ● (+ `frappe/crm`) | ● | ◐ (`sale_opportunity`) | ◐ | ● | ● | trees |
| Purchasing / RFQ / requisitions | ● | ● | ● | ● | ● | ● | ● | ● | trees |
| Inventory / WMS | ● (`stock`) | ● (+barcode app) | ● (75 DocTypes) | ● | ● (`stock_*` x 32) | ● | ● | ● | trees |
| Manufacturing / MRP | ● (`mrp`) | ● (+shopfloor, PLM, quality) | ● (48 DocTypes) | ● | ● (`production_*`) | ● (Premium) | ◐ (SuiteMfg/WMS) | ◐ | trees; BC pricing |
| Projects / timesheets | ● | ● (+planning, field service) | ● | ◐ | ● | ● | ● (PSA) | ● | trees |
| HR / leave / recruitment | ● (`hr_*`) | ● | ◐ separate `hrms` (160 DocTypes) | ◐ | ○ (no HR module) | ◐ [I] | ● | ● (People) | trees |
| Payroll | ○ (`hr_payroll` absent from CE) | ● (docs list Payroll) | ◐ HRMS `payroll` 43 DocTypes | ○ | ○ | ○ (ISV) | ● | ● | trees |
| POS | ● (`point_of_sale` + 45 `pos_*`) | ● | ● (POS Invoice, POS Awesome community) | ● [M] | ◐ (`sale_point`) | ○ (ISV) | ◐ | ◐ | trees |
| E-commerce / website | ● (`website_sale`, `website_*`) | ● | ◐ (`shopping_cart`, Frappe Builder) | ◐ | ◐ (`web_shop_*`: Shopify, Vue Storefront) | ○ | ● | ● | trees |
| Marketing / email / events | ● (`mass_mailing`, `event`) | ● (+automation, social) | ◐ | ◐ | ◐ (`marketing_*`) | ○ | ● | ● | trees |
| Helpdesk / ticketing | ○ (livechat only) | ● | ◐ (`support`, `frappe/helpdesk`) | ● [M] | ○ | ◐ (service) | ● | ● (Desk) | trees |
| Documents / e-sign / knowledge | ○ | ● | ◐ (`frappe/drive`, `frappe/wiki`) | ◐ (ECM) | ◐ (`document_incoming`) | ○ | ◐ | ● | trees |
| Subscriptions / recurring billing | ○ (`sale_subscription` absent) | ● | ◐ | ◐ | ● (`sale_subscription`) | ○ | ● | ◐ | trees |
| BI / dashboards / spreadsheet | ● (`spreadsheet_dashboard_*`) | ● | ◐ (reports; `frappe/insights`) | ◐ | ◐ (`dashboard`) | ● (Power BI) | ● | ● (Analytics) | trees |
| Low-code / studio / custom fields | ○ (code only) | ● (Studio, Custom plan) | ● (DocType UI, server scripts) | ◐ (custom fields, triggers) | ◐ (ir.model, no UI studio) | ● (AL extensions) | ● (SuiteScript) | ● (Creator) | docs/pricing |
| Multi-company / consolidation | ● | ● | ● | ◐ | ● (`account_consolidation`) | ● | ● (OneWorld) | ◐ | pricing/trees |
| AI agents / MCP | ◐ (docs: AI MCP server, agents; CE addon absent) | ● | ◐ (community MCP servers) | ○ | ○ | ● (agents, paid credits) | ◐ [M] | ● (Zia) [M] | Odoo 20 docs [V: "AI agents" and "AI MCP server" pages exist under the AI section]; BC pricing [V: Sales Order Agent and Payables Agent "Requires Copilot Credits (sold separately)", Azure subscription required] |
| Public API | ● (JSON-2, XML-RPC) | ● (Custom plan "External API") | ● (REST auto from DocTypes) | ● (REST) | ● (JSON-RPC, proteus) | ● | ● | ● | docs/pricing |
| Marketplace / ecosystem | ● (40k+ apps claimed) | ● | ◐ | ● (DoliStore) | ○ | ● (AppSource) | ● (SuiteApp) | ◐ | [V/M] |

### 5.1 White space (what nobody serves well)

1. **Fully-featured, genuinely-open accounting + inventory + CRM in one modern, low-ops product.** Odoo withholds accounting/payroll in Community; ERPNext is complete but operationally heavy and visually dated; Dolibarr is simple but old; Tryton is excellent but tiny. [I, grounded in the trees above]
2. **EU-first compliance as a core (not a localisation afterthought)**: Peppol BIS 3.0/EN 16931 receive+send, digital-reporting hooks for VAT in the Digital Age (ViDA adopted 2025-03-11, published in the OJ 2025-03-25, in force 2025-04-14; digital reporting requirements (cross-border B2B, e-invoicing based) from 2030-07-01; Member States with existing domestic real-time reporting must align by 2035-01-01) [V: taxation-customs.ec.europa.eu/taxation/vat/vat-digital-age-vida_en]. Odoo CE has Peppol; Tryton has it; nobody has an opinionated cross-border e-invoice/e-report pipeline. National mandates, re-checked 2026-10-01: **Belgium** structured B2B e-invoicing mandatory since 2026-01-01, with a tolerance period for the first three months of 2026 [V: einvoice.belgium.be/en]; **Poland** KSeF mandatory from 2026-02-01 for taxpayers with 2024 sales above PLN 200m and from 2026-04-01 for everyone else, with a transitional allowance for invoices up to PLN 10,000/month outside KSeF until 2026-12-31 [V: ksef.podatki.gov.pl/od-kiedy-trzeba-wystawiac-faktury-w-ksef/]; **Germany** receipt of EN 16931 e-invoices mandatory since 2025-01-01, paper issuing allowed until 2026-12-31 (and until 2027-12-31 for issuers with prior-year turnover of EUR 800,000 or less), i.e. issuing is mandatory from 2027-01-01 above that threshold and from 2028-01-01 for all [V: gesetze-im-internet.de/ustg_1980/__27.html, § 27 Abs. 38 UStG]; **France** reception for all VAT-liable companies and issuing for large companies from 2026-09-01, SMEs/micro from 2027-09-01 [S: fr.wikipedia.org/wiki/Facture_électronique, which itself cites a private site; the official impots.gouv.fr / economie.gouv.fr pages returned 403/404, so confirm the ETI/mid-size cut-off and e-reporting dates against the DGFiP calendar before scoping].
3. **Operational simplicity**: a single Rust binary + PostgreSQL, no Redis/Node/worker fleet. Frappe's own install story needs bench + multiple services [I]; Odoo needs Python + wkhtmltopdf/Paper Muncher + PG [V: addons `base_report_wkhtmltox`, `base_report_paper_muncher`]. No open-source suite competes on "one container, one database, one backup".
4. **Offline-first / edge-capable POS and warehouse** sharing one core (native + WASM). Odoo POS and Shopify POS are cloud-first; the Rust attempts (apex-edge, open-stock) are hobby scale. [I]
5. **Agent-safe business actions**: typed, permissioned, auditable, idempotent command surface designed for AI agents. Odoo 20 and Twenty ship MCP servers (bolt-ons on existing APIs); BC's agents sit behind paid credits [V]. A command/event-sourced core could make this native. [I]
6. **Honest licensing**: no competitor combines a complete feature set with a licence that cannot be pulled (Cal.com, Akaunting BSL, Invoice Ninja ELv2, Odoo EE). [I]
7. **Vertical depth** is where niche open-source wins (Carbon for manufacturing traceability, InvenTree for parts/BOM). No Rust player exists in any vertical.

---

## 6. What "one platform" offers vs a best-of-breed SaaS stack

### 6.1 What one platform concretely gives

- **A single transactional truth.** In an integrated suite, sale -> delivery -> invoice -> journal entry -> payment reconciliation is one database transaction graph. In Odoo this is literally expressed as dependency modules (`sale_stock`, `stock_account`, `pos_sale_stock`, `mrp_account`) [V]. Stack integrations replicate this with webhooks and Zapier-like glue that fail asynchronously.
- **One permission model, one audit trail, one identity, one bill.**
- **Cross-domain reporting** (project profitability needs timesheets + expenses + purchase + invoices; `project_sale_expense`, `sale_timesheet_margin` exist for this reason [V]).
- **Vendor-stated economics**: Odoo "19.90 EUR / month for ALL apps", Zoho One "50+" apps in one bundle [V]. Macro's README (Rust-built) states the founders' reason for building one system was that at ~20 people "every team got their own tools" and the company "was held together by MCP and Zapier" [V: github.com/macro-inc/macro README, paraphrase].
- **SaaS-sprawl context [S]**: Zylo's 2026 SaaS Management Index reports an average of 305 SaaS apps per organisation, 211 renewals per year, license utilisation only 54% (data year 2025) [S: zylo.com/blog/saas-statistics]. These are large-enterprise-skewed figures but support the sprawl narrative. (Caveat: Zylo sells SaaS management, so has an interest.)

### 6.2 What best-of-breed gives

- Depth: QuickBooks/Xero accounting, Shopify POS, Twenty CRM UX (57.8k stars, USD 9/user) and Plane issues (60.2k stars, USD 6-8/user) each out-execute the corresponding module in a suite [I]. Frappe building standalone CRM/Helpdesk/Drive apps (V) concedes this.
- Optionality and lower switching risk per function.
- Lower entry price for small teams (below).
- Rapid innovation per vertical.

### 6.3 Illustrative cost comparison (list prices only, verified today)

For a 10-user services-plus-stock business:

| Option | Monthly list cost | What is missing |
|---|---|---|
| Odoo Standard, 10 users (first 12 months) | EUR 199 (EUR 249 at list) | nothing structural; implementation extra |
| Odoo Custom, 10 users | EUR 359 (EUR 449 at list) | implementation extra |
| Business Central Essentials, 10 users | USD 800 | partner fees |
| Composed stack: Xero Growing USD 59 + Twenty Pro 10 x USD 9 = 90 + Plane Pro 10 x USD 6 = 60 | ~USD 209 | inventory, purchasing, manufacturing, HR, helpdesk, documents, integration labour, and the glue |

[I] arithmetic on [V] prices (Odoo's yearly/monthly labelling is now confirmed from the page HTML: the EUR 19.90 / 35.90 figures are the yearly-billing prices; monthly billing is EUR 24.90 / 44.90). EUR and USD figures are not converted in the table; at the EUR-per-USD rate embedded in Odoo's own page (0.877), EUR 199 is about USD 227, so the composed stack is still cheaper on list price. Conclusion: a one-platform vendor cannot win on list price for simple needs; it wins when a company needs more than ~4 functional domains, inventory-to-accounting consistency, or multi-company, i.e., when the integration bill exceeds the licence bill.

### 6.4 The honest downside of the suite

- **Combinatorial glue**: Odoo CE's 413 functional modules include dozens of A_B_C bridge modules [V names above]. Every new app multiplies the bridge count. Tryton avoids some of this through fine-grained modules and a strict dependency graph.
- **Quality is uneven**: the weakest app in a suite is usually worse than the point solution. [I; see also 04-odoo-voice-of-customer.md for Odoo sentiment]
- **Upgrade cost**: Odoo supports 3 major versions in parallel (each major is supported for three years, plus two more years to complete an upgrade [V: odoo.com/documentation/20.0/administration/upgrade.html]; 16.0 EOL'd when 19.0 shipped 2025-09 [S]); custom code makes major upgrades a project. ERPNext ships v15 and v16 in parallel [V].
- **Licence dependence**: the suite vendor controls your data model.

---

## 7. Realistic competitive position of a Rust-native newcomer

**What Rust actually buys in business software** [I]: (1) single static binary, low RAM, cheap hosting and trivial on-prem; (2) memory safety and strong types for money, units, tax codes, state machines; (3) fearless concurrency for posting engines and import pipelines; (4) safe plugin sandboxing if WASM is used (wasmtime, cedar-policy); (5) one language for server, offline POS core and WASM. **What it does not buy**: SMB ERP is bound by database round-trips and domain correctness, not CPU. Odoo's "operations in less than 90 ms" claim [V: odoo.com] shows performance is already adequate marketing-wise. "Written in Rust" is not a buying criterion for a bookkeeper; it is a contributor-attraction and operations argument.

**Structural disadvantages [I, evidence in parentheses]:**

1. **Talent and contribution funnel.** ERP contributors are domain people writing Python/PHP; Odoo has 304 contributors and OCA's 271 repos, ERPNext ~280, Dolibarr ~279 [V]. The best Rust success in this neighbourhood, Macro, has ~25 contributors after 11 months; Meteroid ~15 after 3 years [V].
2. **Runtime customisation.** Implementers customise Odoo (inherit models/views in Python, Studio) and ERPNext (DocType UI, server scripts) *without recompiling*. A compiled binary cannot do this unless the design is metadata-driven with an embedded scripting/WASM layer. This is the single most important architectural risk.
3. **Localisation debt.** Odoo CE ships 229 `l10n_*` modules; Odoo docs list ~103 countries; Tryton has national modules. No Rust crates exist for tax engines, bank formats or payroll rules (crates.io search [V]).
4. **Ecosystem**: 40k+ apps (Odoo claim [V]), DoliStore, Frappe's 200+ partners [V]. Zero for a newcomer.
5. **Trust and inertia**: finance systems are chosen by accountants and integrators, who move slowly.
6. **Scope**: Odoo's 413 modules and ERPNext's 529 DocTypes represent >10 years of accumulated behaviour; matching "clone how Odoo works" product scope is a decade-scale project unless scope is ruthlessly staged.

**Structural advantages [I]:**

- The 2026 licensing turmoil opens a trust gap (Cal.com, Akaunting, Invoice Ninja, Odoo EE gating) [V].
- Odoo Community lacking accounting/payroll, and ERPNext's operational weight, leave room for a complete, simple-to-run open suite.
- AI-era buyers expect typed APIs and agent surfaces; a greenfield core can make them primary.
- Modern UI expectations have been reset by Twenty/Plane/Linear-style tools; both Odoo and ERPNext carry legacy UI debt.
- Rust tooling for documents (Typst), auth policy (Cedar), search (tantivy), and DB (sqlx) is production-grade [V].

**Realistic trajectory [I]:** 12-18 months to an MVP that is credible for a *narrow* persona (EU micro/small business: invoicing + accounting + inventory + CRM + Peppol), 3-5 years to reach ERPNext-2015-level breadth. A Rust newcomer will not be an Odoo replacement for mid-market manufacturers in the near term. Winning condition: (a) become the trusted, complete, no-rug-pull open alternative for a specific segment and geography; (b) exploit operational simplicity and agent-readiness; (c) build the extension model so third parties can add domain breadth without touching core.

---

## 8. Lessons by project (what to copy, what to avoid)

| Project | Copy | Avoid |
|---|---|---|
| Odoo | Everything-is-a-model with inherited views; app/module dependency metadata; one-app-free funnel; JSON-2 API with API keys; docs-per-app | Open-core feature gating of basics (accounting); N^2 bridge modules; per-user price plus IAP credits sprawl |
| ERPNext/Frappe | DocType = schema + form + API + permission from one JSON; hosting-only monetisation; standalone apps on same platform | Heavy multi-service ops; splitting HR away from the base product confuses "one platform" |
| Dolibarr | Low resource needs, marketplace of modules, association governance | Legacy coupling (PHP 7-era) |
| Tryton | Strict module graph, LTS cadence, correctness, foundation governance (Belgian private foundation) | Desktop-first heritage, tiny ecosystem |
| Twenty | Custom objects as the core, design-led UX, unlimited seats, MCP from the start | AGPL + per-file `@license Enterprise` mixed-licence files complicate contributor and redistributor rights |
| Medusa | Modules + workflows architecture; usage-based cloud with 0% GMV fee | Enterprise-only RBAC/SSO carve-out |
| Cal.com | Community distribution model early | Letting the production codebase diverge from the public one; dependency on a single company's goodwill |
| Akaunting | Paid app store on open core | BSL with 2-user/1-company free cap kills community adoption |
| Invoice Ninja | Hosted + self-host white-label | ELv2 forbids managed-service hosting |
| Lago/Kill Bill/Meteroid | Billing engine as a separable service with clean API | Rebuilding payments orchestration (Hyperswitch, 45k stars, Apache-2.0, already exists in Rust) |
| rerp | n/a | Microservice-first with 3 contributors |
| forgeerp | Event-sourced ledger ideas (audit trail) | Architecture-manifesto READMEs with no releases |

---

## Confidence & gaps

- **High confidence [V]**: GitHub metadata (stars, licences, releases, last-push dates), Odoo 20 branch state and addon trees, ERPNext DocType counts, Tryton module list, Cal.com closure and Cal.diy terms, Akaunting BSL and Invoice Ninja ELv2 texts, pricing of Odoo, BC, Xero, QuickBooks, Holded, Teamleader, Twenty, Plane, Medusa, Akaunting, Frappe Cloud, crates.io/PyPI versions, ViDA dates.
- **Medium [S]**: Odoo revenue/valuation, company history (Wikipedia); Zylo SaaS statistics; NetSuite/SAP/Sage/BC background (Wikipedia).
- **Not verified**: NetSuite, SAP Business One and Sage prices (sites blocked automated access); Zoho One per-user price unit/toggle (page did not render it); Shopify POS USD pricing (only the Swedish SEK page fetched); national e-invoicing mandate dates for France (secondary source only; official DGFiP pages returned 403/404) and Belgium's e-reporting start (Belgium's e-invoicing start, Poland's KSeF dates and Germany's § 27 Abs. 38 UStG dates were later verified from primary pages, see 5.1); funding and headcounts of Twenty, Medusa, Plane, Lago, Frappe; Odoo 20 exact release day (the official release-notes page is dated September 2026) and feature list beyond the docs sidebar; Odoo Community/Enterprise feature split is inferred from the absence of modules in the CE repo tree, not from Odoo's own feature table (that page's checkmarks did not render); metasfresh licence version; Axelor's enterprise edition; Dolibarr DoliCloud pricing.
- **Known biases**: GitHub stars favour hype-driven projects (Twenty, Plane, Huly) and under-represent long-established, download-based products (Dolibarr via SourceForge, Odoo via odoo.com); GitHub search missed repos lacking topics/language metadata; I did not attempt bot-detection bypass on blocked sites.
- **Gap**: no interviews or user reviews were analysed here; Odoo customer sentiment is covered by 04-odoo-voice-of-customer.md.

---

## Implications for obop

**MUST**

1. **MUST build a metadata-driven core, not hard-coded modules.** Models, custom fields, views, workflows, permissions and validations must be data, with a sandboxed scripting layer (Rhai or WASM) so implementers customise without recompiling. This is the only way a compiled Rust product matches Odoo Studio / Frappe DocTypes (529 in ERPNext) [V]; otherwise the partner ecosystem cannot form.
2. **MUST choose and publish a licence and a "forever free" list before the first public commit.** Recommended (owner's decision): AGPL-3.0-or-later core, trademark policy, a written commitment that general ledger, invoicing, inventory, CRM, HR core, and the public API never move behind a paid tier; monetise hosting, support, certified localisation packs and connectors. Rationale: Cal.com (closed 2026-04-14), Akaunting (BSL), Invoice Ninja (ELv2), Odoo EE gating, Twenty (`@license Enterprise` files)/Medusa (ENTERPRISE-LICENSE.md)/Carbon (`packages/ee`) carve-outs [V]. Avoid a CLA that permits relicensing, or accept that it reads as a rug-pull risk.
3. **MUST ship one tight transaction loop first**: quote -> order -> delivery -> invoice -> payment -> journal entry, on an immutable double-entry ledger with period locks, idempotent posting, and full audit trail. Stage breadth after this; do not attempt Odoo's 413 modules.
4. **MUST make money and tax correctness a type-system feature**: `rust_decimal` or integer minor units, currency-tagged amounts, rounding policy per tax/currency, no floats [V: crate maturity]. This is a genuine Rust advantage.
5. **MUST treat localisation as data**: chart of accounts, tax rules, fiscal positions and e-invoice mappings as versioned packs (Odoo: 229 packs; Tryton: `account_be/fr/syscohada`) [V]. Start with one or two jurisdictions and Peppol BIS 3.0/EN 16931 via UBL/CII; verify national mandate dates from primary sources before committing a roadmap.
6. **MUST avoid microservices and bridge-module explosion**: a modular monolith with explicit module dependency metadata and event-based extension points so that cross-app behaviour (sale x stock x account) is not expressed as N^2 glue packages [V examples].
7. **MUST provide a typed, versioned public API and an MCP server from day one** (OpenAPI via `utoipa`; permissioned, auditable commands). Odoo 20 ships AI agents + MCP server, Twenty ships MCP, BC ships paid agents [V]; absence would be disqualifying, presence is not yet differentiating.
8. **MUST ship import tooling** (CSV plus Odoo JSON-2 / ERPNext REST / QuickBooks / Xero API importers) so migration cost is low; Odoo's own API is documented and public [V].

**SHOULD**

9. **SHOULD target a narrow launch persona**: EU micro/SMB services and trading companies needing accounting + invoicing + stock + CRM with Peppol. This maps to the Holded/Teamleader/Xero band, where price points (EUR 29-99/month; USD 27-97/month) are known [V].
10. **SHOULD make operations the headline**: one binary + PostgreSQL, no Redis/Node fleet, trivial backup/restore, one-command upgrade. This is a verifiable differentiator against Frappe/Odoo stacks [I].
11. **SHOULD price for the AI era**: free self-host; hosted plans by organisation and usage, not seat (Frappe Cloud from USD 5/month, Xero no per-user fees, Twenty unlimited seats [V]). Undercut Odoo's EUR 19.90-35.90 per user only where it creates no support burden.
12. **SHOULD reuse rather than rebuild infrastructure**: Hyperswitch (Apache-2.0, Rust) for payment orchestration via integration, Typst for invoice/report PDFs, Cedar for authorisation policies, tantivy for search [V]. Keep each behind a trait so they are swappable.
13. **SHOULD design the POS/warehouse client as an offline-first shared Rust core** (native + WASM) with an outbox/sync protocol; this is the one area where Rust provides structural advantage over Odoo/Shopify [I].
14. **SHOULD open a partner/certification program at alpha** (Frappe: 200+ partners [V]); ERP adoption is implementer-led.
15. **SHOULD keep frontend conventions aligned with the dominant modern-suite stack** (TypeScript/React: Twenty, Medusa, Plane) [V] to maximise the pool of UI contributors; defer to the Vite+ research for toolchain specifics.

**COULD**

16. **COULD** add a Beancount/plain-text import-export bridge, reusing or interoperating with `rustledger` (GPL-3.0, compatible with an AGPL-3 combination) [V] to attract the plain-text accounting community.
17. **COULD** offer a subscription/usage-billing module modelled on Lago/Meteroid (AGPL, Rust) rather than inventing one [V].
18. **COULD** pursue one vertical module with real depth (e.g., manufacturing traceability as Carbon does, or parts/BOM as InvenTree does) as a beachhead after the core loop ships [V].
19. **COULD** publish an "Odoo/ERPNext coverage tracker" as public roadmap, to turn the clean-room scope target into a transparent contribution list.
20. **COULD** investigate event sourcing for the ledger (forgeerp-style) but only for the posting engine and audit; avoid full event-sourced CRUD for every entity [I].

---

## Verification log

Adversarial fact-check run on 2026-10-01 using the GitHub REST API, crates.io and PyPI JSON APIs, direct vendor/primary-page fetches (odoo.com pricing HTML, docs, Frappe, Microsoft, Xero, Intuit, Holded, Twenty, Plane, Medusa, Zoho, Teamleader, Shopify), the EU Commission ViDA page, and national e-invoicing sites. No general web-search engine was available, so discovery of unknown unknowns was limited to GitHub and crates.io searches.

**Confirmed as stated: approximately 270 individual data points** (star counts and contributor counts for about 45 repos, about 30 latest-release tags/dates, about 25 licence identifiers, all 21 crates.io version/download rows, about 40 list-price figures, the Odoo 20.0 tree counts of 642 / 413 / 229 and the absent-module list, ERPNext's 529 DocTypes and per-module counts, HRMS 160 (117 + 43), Tryton's 221 modules and dates, ViDA dates, Cal.com dates and HN numbers, Zylo and Wikipedia-sourced figures). All 12 author-flagged load-bearing claims held up; 2 of them needed a detail correction (items 1 and 2 below) and 1 needed a wording fix (item 12).

**Corrections (was / now / source):**

1. Odoo `version_info`. Was: `(20, 0, 0, FINAL, 0)`. Now: `(20, 0, 0, FINAL, 0, '')` on 20.0 and `(20, 1, 0, ALPHA, 1, '')` on master (20.1 alpha); Odoo's release-notes page is dated September 2026. Source: github.com/odoo/odoo `odoo/release.py` on 20.0 and master; odoo.com/odoo-20-release-notes.
2. Twenty licence marker. Was: "commercial `/ee` files". Now: commercial files are marked by a `/* @license Enterprise */` header; no `/ee` directory exists. Source: github.com/twentyhq/twenty/blob/main/LICENSE and repo tree. (Edited in the table, the lessons table and MUST item 2.)
3. Odoo pricing labelling. Was: "the page text does not label which figure is the yearly one, lower is inferred yearly". Now: verified from the page HTML (`yearly_price`/`monthly_price` classes): yearly billing 19.90 (list 24.90) / 35.90 (list 44.90); monthly billing 24.90 (list 31.10) / 44.90 (list 56.90). Also added: Light User 7.90, Odoo.sh hosting cost not included. Source: odoo.com/pricing raw HTML; odoo.com homepage (`yearly_standard_discounted` = 19.90).
4. Odoo fiscal localisations. Was: 103 countries, 45 dedicated pages (19.0 docs). Now: 103 countries, 46 dedicated pages in the 20.0 docs. Source: odoo.com/documentation/20.0/applications/finance/fiscal_localizations.html (46 unique sub-page links counted).
5. Odoo app count. Was: "~24 core apps" (no source). Now: roughly 55 apps across 8 categories in the 20.0 docs index (author count). Source: odoo.com/documentation/20.0.
6. Module counts. Was: Odoo "~50 `pos_*`"; Tryton "`stock_*` x ~35". Now: 45 `pos_*` directories plus `point_of_sale`; 32 `stock_*` modules (`sale_*` is 35, unchanged). Source: GitHub trees odoo/odoo@20.0 `addons/` and tryton/tryton `modules/`.
7. Carbon licence. Was: "custom (NOASSERTION)". Now: AGPL-3.0-only except "Enterprise files" (`packages/ee/` and any `*.ee.*` file) under a commercial licence. Source: github.com/crbnos/carbon/blob/main/LICENSE.
8. NocoBase licence. Was: "custom". Now: custom NocoBase License Agreement (2026-02-24) that incorporates Apache-2.0 plus supplementary terms, with paid Commercial Editions. Source: github.com/nocobase/nocobase/blob/main/LICENSE.txt.
9. Stalwart licence. Was: "AGPL-3 + commercial [M]". Now: verified, AGPL-3.0-only plus a separate Stalwart Enterprise License (`LicenseRef-SEL`). Source: github.com/stalwartlabs/stalwart `LICENSES/`.
10. SuiteCRM. Was: table row implied the product is at v7.15.2 only. Now: that repo is the 7.x line; SuiteCRM 8 (Angular + Symfony) ships from SuiteCRM/SuiteCRM-Core, v8.10.2 (2026-07-31), 285 stars, AGPL-3.0. Source: GitHub releases API for both repos.
11. Tryton Foundation. Was: "nonprofit foundation". Now: "a private foundation (Fondation Privée) formed under the Belgian law"; the page does not use the word non-profit. "No VC" is marked [I]. Source: tryton.org/foundation. (Canonical repo on Heptapod re-confirmed: code.tryton.org/tryton redirects to foss.heptapod.net/tryton/tryton; the GitHub repo describes itself as "Mirror of tryton".)
12. Cal.com quote. Was: hopes to return to open source "someday". Now: the blog says "one day ... as the security landscape evolves". Also added from the cal.diy README: "strictly recommended for personal, non-production use". Source: cal.com/blog/cal-com-goes-closed-source-why; github.com/calcom/cal.diy README.
13. Shopify POS. Was: "in-person selling bundled on every plan". Now: only casual in-person selling is included on every plan; unlimited staff, inventory management and omnichannel tools need POS Pro (standard on Plus). Source: shopify.com/pos/pricing (Swedish page).
14. Holded scope. Was: "Invoicing, accounting, CRM, projects, inventory, HR, POS". Now: inventory, HR and POS are paid Holded Store add-ons (HR from EUR 1.50/employee/month, Inventory from EUR 25/month, POS from EUR 25/store/month). Source: holded.com/pricing.
15. Teamleader add-ons. Was: "EUR 35 each after promo". Now: EUR 35/month each against a struck-through EUR 45; also 17% annual discount, 25% off first invoice, second user free. Source: teamleader.eu/pricing.
16. Medusa licence scope. Was: Enterprise license "covering SSO/RBAC/audit". Now: ENTERPRISE-LICENSE.md lists the RBAC and SSO modules; audit logs are Enterprise-tier only on the pricing page; Cloud prices are "from" USD 29 / 99 / 299. Source: github.com/medusajs/medusa ENTERPRISE-LICENSE.md; medusajs.com/pricing.
17. Akaunting licence wording. Was: "2 users / 1 company / 1,000 invoices". Now: kept, with the literal grant text ("more than two users or one company or one thousand invoices", ambiguous) and the extra bars on white-labelling and offering an "Accounting Service" noted; change licence GPLv3 applies four years after each version's publication. Source: github.com/akaunting/akaunting/blob/master/LICENSE.txt.
18. metasfresh licence. Was: "GPL (badge)". Now: unchanged in substance but flagged: the README badge links to LICENSE.md, yet no LICENSE file exists in the repo root listing; version unverified. Source: GitHub contents API.
19. Rust ERP landscape. Added: `beowulf1416/idEAS` (27 stars, AGPL-3.0, stale since 2023-02) outranks `rerp`; `cala-ledger` 0.31.0 (GaloyMoney/cala, Apache-2.0, 41 stars, ~155k downloads) is a real double-entry ledger crate, qualifying the "no domain libraries" observation. Source: GitHub search API; crates.io API.
20. Business Central ISV claim. Was: "payroll/POS/e-commerce via ISVs [S/I]". Now: [I]; the BC pricing page says nothing about payroll, POS or e-commerce (Wikipedia only says AppSource ISVs cover reporting, payroll and commerce). Source: microsoft.com BC pricing page; en.wikipedia.org/wiki/Microsoft_Dynamics_365_Business_Central.
21. National e-invoicing mandates (previously "[M: dates unverified]"). Now verified: Belgium mandatory since 2026-01-01 with a three-month tolerance (einvoice.belgium.be/en); Poland KSeF from 2026-02-01 (2024 sales above PLN 200m) and 2026-04-01 (others), PLN 10,000/month allowance until 2026-12-31 (ksef.podatki.gov.pl); Germany § 27 Abs. 38 UStG (gesetze-im-internet.de/ustg_1980/__27.html). France is secondary only (fr.wikipedia.org/wiki/Facture_électronique). ViDA detail added (OJ publication 2025-03-25; digital reporting requirements are cross-border B2B).
22. Zoho One. Clarified: the page FAQ says plans are priced "per user per month with annual licensing"; the USD 90 figure's toggle is still undefined on the page.
23. Odoo support window. Added: each major version is supported for three years (plus two more years to complete an upgrade), which backs the "3 major versions in parallel" statement. Source: odoo.com/documentation/20.0/administration/upgrade.html.
24. Currency note. Added to section 6.3: Odoo's page embeds a EUR-per-USD rate of 0.877, so EUR 199 is about USD 227 and the composed stack remains cheaper on list price.

**Unresolved (left as unverified or inference):**

- NetSuite, SAP Business One and Sage list prices (vendor sites return 403); Shopify POS USD pricing (only the Swedish page fetched).
- France's e-invoicing calendar from an official source (impots.gouv.fr and economie.gouv.fr returned 403/404); the mid-size (ETI) cut-off and e-reporting dates are therefore secondary-source only. Belgium's e-reporting start (2028) was not found on einvoice.belgium.be.
- Zoho One's USD 90 unit/toggle, and the All-Employee and Essentials prices (not rendered on the page).
- Twenty Organization (USD 19) billing cadence (page ties "yearly" only to the Pro figure); Twenty/Lago "YC-backed" and all funding/headcount figures (remain [M]).
- Odoo 20's exact release day (release-notes page says September 2026); the Odoo Community vs Enterprise feature split is still inferred from the CE tree, not Odoo's own feature table.
- ERPNext licence discrepancy (license.txt is GPL-3.0; frappe.io/erpnext/pricing says "AGPL-3.0 licensed"); confirmed real, cause unknown, treat GPL-3.0 as authoritative for the repo.
- Wikipedia-derived figures (Odoo revenue and valuation, NetSuite, Sage, SAP B1, BC heritage) were confirmed only against Wikipedia itself, so they stay [S].
- metasfresh licence version, Axelor enterprise edition, Dolibarr cloud pricing, and the "Backer / model" column entries marked [M].
