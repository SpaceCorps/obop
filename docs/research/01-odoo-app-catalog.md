# Odoo application catalog: what the product actually is (as of 2026-10-01)

Research input for obop ("one business, one platform"). Functional reference only: clean-room re-implementation of concepts and behaviour, never a code port. Nothing here recommends copying Odoo Enterprise code or assets.

## Scope & method

Sources, all fetched live on 2026-10-01: the Odoo site (editions comparison table parsed from raw HTML, pricing, about-us, release-notes pages for 17/18/19/20, industries index); the Odoo documentation repo (`odoo/documentation`, branch `20.0`, file tree and RST sources); and the Odoo Community source (`odoo/odoo`, branch `20.0`, read through the GitHub API and raw files). From the source I pulled all 642 addon directories and 641 `__manifest__.py` files (dependencies, `application`, `auto_install`; `mrp_subcontracting_repair` holds only an `i18n` folder and has no manifest, re-checked 2026-10-01), and the ORM model names (`_name`) of about 45 key modules. The Enterprise source is private, so Enterprise-only facts come from the editions page, docs and release notes; model names for Enterprise apps are from memory and tagged **(M)**.

Tags: **[V]** verified in a primary source, **[I]** inference from verified facts, **(M)** remembered/unverified. Edition labels: **CE** = Community (LGPL-3), **EE** = Enterprise (proprietary). Classification: **TS** = table-stakes for an SMB business OS, **SW** = valuable second wave, **LT** = long tail.

---

## 1. Headline facts

### 1.1 Versions and cadence [V]

| Release | Date | Source / note |
|---|---|---|
| Odoo 17.0 | Nov 2023 | end of standard support Sept 2026 (i.e. just ended as of 2026-10-01; the support table already shows it as unsupported on Odoo Online and extended-support-for-a-fee on Odoo.sh / on-premise) |
| Odoo 18.0 | Oct 2024 | standard support until Sept 2027 (planned) |
| Odoo 19.0 | Sept 2025 | standard support until Sept 2028 (planned) |
| SaaS 19.1 / 19.2 / 19.3 / 19.4 | Jan / Mar / May / Jul 2026 | Online-only, every 2-3 months, no extended support (support table: 19.1 and 19.2 already unsupported, 19.3 and 19.4 supported; no 20.x SaaS row yet) |
| **Odoo 20.0** | **Sept 2026** | standard support until Sept 2029 (planned) |

Sources: release list https://www.odoo.com/page/release-notes ; support table https://github.com/odoo/documentation/blob/20.0/content/administration/standard_extended_support.rst (rendered at https://www.odoo.com/documentation/19.0/administration/standard_extended_support.html).

A newer version than 19 therefore exists: **Odoo 20 shipped in September 2026** (release notes: https://www.odoo.com/odoo-20-release-notes). The `odoo/odoo` default branch is `20.0`, `odoo/release.py` on 20.0 reads `version_info = (20, 0, 0, FINAL, 0, '')`, `MIN_PY_VERSION = (3, 12)`, `MAX_PY_VERSION = (3, 14)`, `license = 'LGPL-3'` (https://github.com/odoo/odoo/blob/20.0/odoo/release.py). The repo had 54,767 stars and 33,902 forks on 2026-10-01 (GitHub API `repos/odoo/odoo`, re-queried; GitHub reports its SPDX licence as NOASSERTION because the LICENSE file bundles LGPL and GPL text). The docs "latest" alias still rendered 19.0 when fetched (re-checked), so docs lag the release; a `/documentation/20.0/` path does resolve and renders "Odoo 20.0 documentation", and the 20.0 docs branch still labels its licences page "Odoo 19". Major releases are annual (Oct/Nov historically, Sept for 19 and 20).

### 1.2 Editions, licence, price [V]

- **Community (CE)**: LGPL v3 (https://github.com/odoo/odoo/blob/20.0/LICENSE). **Enterprise (EE)**: Odoo Enterprise Edition License v1.0, requires a valid subscription per user, modules you build on it may be licensed under a compatible licence (https://www.odoo.com/documentation/19.0/legal/licenses.html). EE is "built on" CE and can be switched to at any time (https://www.odoo.com/page/editions).
- Pricing (https://www.odoo.com/pricing): "One App Free" (one app, unlimited users, EUR 0, includes dependencies and multi-company); Standard EUR 24.90/user/month list on annual billing (EUR 19.90 promo for the first 12 months, initial users only); Custom EUR 44.90 list on annual billing (EUR 35.90 promo); Light User "at EUR 7.90" (no unit printed on the page; the price variable is `monthly_light`, and the FAQ says a Light User licence is obtained via an Odoo advisor quote). On the monthly-billing toggle the page shows EUR 31.10 (Standard) and EUR 56.90 (Custom) as the struck-through list prices, with EUR 24.90 and EUR 44.90 as the discounted prices [V, raw HTML of the page re-parsed 2026-10-01; prices shown are the EUR/Europe view and may differ by region, other currencies unverified]. Relative to Standard, Custom adds Studio, multi-company, external API, Odoo.sh/on-premise hosting and "Agentic AI"; Standard is Odoo Online only, without custom modules (pricing FAQ). The pricing FAQ also says One App Free allows multiple companies and lets you pick Studio as the one free app, but adding Studio or extra companies to a Standard database switches it to Custom. SMS, contact auto-complete, lead generation and bill/expense OCR consume separately purchased IAP credits (pricing FAQ), and in Odoo 20 "All AI features now need credits, which can be bought via IAP" (https://www.odoo.com/odoo-20-release-notes).
- Scale claims (vendor, unaudited; https://www.odoo.com/page/about-us): "50 main applications", 28 million users, 8,000+ employees, 21,000+ partners, 50,000+ community apps.
- The editions table has **51 app rows: 28 in CE, 23 EE-only** (my count from the parsed table; independently re-counted on 2026-10-01 from the raw HTML, excluding the General and User-interface rows and the "↳" sub-feature rows). The CE Accounting gap is the big one: CE ships "Invoicing" only; full Accounting is EE.
- Third-party ecosystem: apps.odoo.com has a 20.0 filter and already lists "Odoo 20 Accounting Community" clones; the Odoo Community Association has 271 public repos (GitHub API). [V] that these exist, [I] that the CE accounting gap is a persistent market opportunity.

### 1.3 Size of the Community tree (20.0) [V]

642 addon directories: 413 non-localization + 229 `l10n_*`. Only **34 modules are flagged `application: True`** (the installable "apps"); **400 of the 641 manifests set `auto_install` to True or a dependency list** (256 `True`, 144 lists); 20 directories are `test_*`. Largest families (each count is the base module plus its `prefix_*` children; `prefix_*` alone is one lower, except `pos_*`, which has no base module): `website` 52, `pos_*` 45, `sale` 29, `hr` 24, `payment` 24 (21 real providers plus `payment_custom` and `payment_demo`), `project` 18, `account` 16, `mrp` 12, `mass_mailing` 12. Industries: 61 industry landing pages on https://www.odoo.com/all-industries (plus newer ones named in the Odoo 20 notes).

---

## 2. How the apps interlock

### 2.1 Mechanism: thin apps plus auto-install bridges [V]

Apps are small; integration lives in `auto_install` modules that activate when both parents are installed. Examples (manifests at https://github.com/odoo/odoo/tree/20.0/addons/<module>/__manifest__.py):

| Bridge | Depends | Effect |
|---|---|---|
| `stock_account` | stock, account | valuation journal entries |
| `sale_stock` | sale, stock_account | SO confirmation creates deliveries |
| `purchase_stock` | stock_account, purchase | PO creates receipts |
| `sale_purchase` / `sale_purchase_stock` | sale, purchase (+stock) | service outsourcing; dropshipping (`stock_dropshipping`, depends on `sale_purchase_stock`, but is itself not `auto_install` [V manifest]) |
| `mrp_account` | mrp, stock_account | manufacturing valuation and analytic |
| `sale_mrp` | mrp, sale_stock | kits, make-to-order |
| `sale_crm` | sale, crm | quotes from opportunities |
| `sale_project` then `sale_timesheet` | sale_management, project_account, hr_timesheet | service lines spawn tasks; billable timesheets |
| `pos_stock`, `pos_sale`, `pos_loyalty` | point_of_sale + stock_account / sale_management / loyalty | POS to inventory, quotes, loyalty |
| `website_sale_stock`, `website_payment` | website_sale + sale_stock (+ stock_delivery); website + account_payment (+ portal) | shop stock and online payment |
| `account_payment` | account, payment | portal payment posts `account.payment` (manifest: `auto_install: ['account']`, so it triggers on `account` alone and pulls `payment` in as a dependency) |
| `sale_expense` | sale_management, hr_expense | re-invoice expenses |

Shared entities every app reuses: `res.partner`, `res.company` (multi-company and branches), `product.template/product.product`, `uom.uom`, `product.pricelist`, `account.analytic.*` (analytic plans), `resource.calendar`, `utm.*` (campaign/medium/source), `mail.thread` + `mail.activity` + `mail.followers` (chatter on nearly every model), `portal`, `digest`, `rating`.

Odoo 19-20 are loosening some couplings [V, Odoo 20 notes]: "Ship orders without Inventory", "Valuation without Inventory", POS "simplified inventory management, without installing the Inventory app". Design cue: inventory/valuation/fulfilment should be separable from accounting and sales.

### 2.2 The automatic chain: sale order to journal entries [V]

1. `crm.lead` (opportunity) to `sale.order` via `sale_crm`; team, campaign, source (`utm`) carry over.
2. `sale.order._action_confirm()` (https://github.com/odoo/odoo/blob/20.0/addons/sale/models/sale_order.py). With `sale_stock`, `sale.order.line._action_launch_stock_rule()` creates procurements; `stock.rule`/`stock.route` select pull, push, MTO, buy or manufacture; outputs are `stock.picking` + `stock.move` (delivery), RFQs (`purchase_stock`), or `mrp.production` (`sale_mrp`), or a dropship PO.
3. Delivery validated: `stock.move` goes to done; in `stock_account`, `stock.move._create_account_move()` posts a valuation `account.move` when the product's `valuation == 'real_time'` (automated valuation). Three costing methods: standard, average (AVCO), FIFO (docs cheat sheet). Odoo 20 adds retroactive COGS updates when landed cost/bill price changes, and accrual entries directly from the valuation closing report.
4. Invoicing: `sale.order._create_invoices()`; per-product policy "ordered quantities" vs "delivered quantities", down payments, milestones, time and materials (https://www.odoo.com/documentation/19.0/applications/sales/sales/invoicing/invoicing_policy.html). Posting creates receivable, revenue and tax lines (+ COGS lines linked to the stock move for anglo-saxon flows).
5. Payment: portal/online via `payment.transaction` (`account_payment` creates the `account.payment`) or bank statement line; reconciliation via `account.partial.reconcile`/`account.full.reconcile`; invoice `payment_state` updates. Odoo 20 relabelled statuses ("In Process" became "Paid", "Paid" became "Reconciled").
6. Reporting: analytic lines, `sale_margin`, per-app spreadsheet dashboards (`spreadsheet_dashboard_*`), aged receivables (EE).

Docs state the principle: "Odoo automatically creates all the underlying journal entries for all accounting transactions (customer invoices, vendor bills, point-of-sales orders, expenses, inventory valuations, etc.)" (https://www.odoo.com/documentation/latest/applications/finance/accounting.html).

### 2.3 Other chains [V unless noted]

- **Procure-to-pay**: reordering rule (`stock.warehouse.orderpoint`) or MTO to `purchase.order` (vendor prices from `product.supplierinfo`) to receipt to vendor bill (`purchase` depends `account`), `purchase.bill.line.match` for matching, `stock_landed_costs`, blanket orders (`purchase_requisition`). Bill OCR and bill-line prediction use IAP/EE.
- **Manufacturing**: `mrp.bom` to `mrp.production` to `mrp.workorder` on `mrp.workcenter`; component and finished moves are ordinary `stock.move`; subcontracting and byproducts; `mrp.workcenter.productivity` drives OEE.
- **Services**: service `sale.order.line` to project/task (`sale_project`) to `account.analytic.line` timesheets (`hr_timesheet`) to billable (`sale_timesheet`); EE adds Planning, Field Service, Helpdesk-to-timesheet billing.
- **Expenses**: `hr.expense` to (Odoo 20) draft vendor bill in the expense journal; reimbursement via bill payment or payslip (EE Payroll).
- **POS**: `pos.session` to `pos.order` to payments; Odoo 20 generates one "global sale" at session close to simplify accounting.
- **eCommerce**: `website.visitor` to cart (draft `sale.order`) to `website.checkout.step` to `payment.transaction` to normal order-to-cash.
- **HR**: `hr.employee` with `hr.version` (employees and contracts merged in 19) to `hr.leave` to work entries to payslips (EE). Odoo 20 removed work entries and the Planning-Payroll integration from Payroll.

---

## 3. Catalog by domain

Columns: Entities = ORM models [V] for CE (from `_name` scan of 20.0) or **(M)** for EE. "Needs" = manifest `depends` (summarised) for CE apps. Odoo 19/20 deltas given where they change behaviour.

### 3.1 Platform and essentials

| App (module) | Purpose and key features | Entities | Ed. | Needs | Class |
|---|---|---|---|---|---|
| Contacts (`contacts`) | Address book, companies and persons, hierarchy, merge, partner autocomplete (Dun & Bradstreet since 19; multiple identifiers e.g. DUNS in 20), bank accounts | `res.partner`, `res.partner.bank`, `res.company`, `res.users` | CE | mail, web_hierarchy | TS |
| Discuss / Mail (`mail`) | Chatter on records, messages, followers, activities and activity plans (role assignment in 20), channels, mail gateway/aliases, templates, canned responses, polls (new models in 20), push, CC on email (20) | `mail.message`, `mail.thread`, `mail.activity`, `mail.followers`, `mail.alias`, `mail.template`, `discuss.channel`, `mail.poll` | CE | base, bus | TS |
| Calendar (`calendar`) | Meetings, attendees, recurrence, alarms; Google and Outlook sync; multiple calendars (20) | `calendar.event`, `.attendee`, `.recurrence`, `.alarm` | CE | mail | TS |
| To-do (`project_todo`) | Personal memos and to-dos | project tasks | CE | project | SW |
| Automation rules (`base_automation`) | Triggered server actions, webhooks | `base.automation` | CE engine (the editing UI is tied to Studio in EE [I]) | mail, sms | TS |
| Portal, Digest, UTM, link tracker, SMS, IAP, phone validation | Customer portal, KPI emails, campaign tracking, short links, SMS via IAP/Twilio | `utm.campaign/medium/source`, `sms.sms` | CE | - | TS |
| Security | TOTP, passkeys, OAuth, LDAP, password policy; Odoo 20 replaces record rules with a domain on each access right | `res.groups`, `ir.model.access` | CE | - | TS |
| Data Recycle (`data_recycle`) | Archive/delete old records by rule. Merge-style Data Cleaning is EE [I from CE tree] | `data_recycle.*` | CE / EE | - | LT |
| Spreadsheet, Dashboards | CE holds the engine and prebuilt per-app dashboards; the editable Spreadsheet/Dashboards apps are EE | `spreadsheet.*` | CE lib / EE | - | SW |
| Documents | Folder/file manager, request, tags, Accounting/HR/Fleet filing automation, AI rename (20) | `documents.document` **(M)** | EE | - | SW |
| Knowledge | Wiki articles, templates, item kanban/list/calendar, AI article | `knowledge.article` **(M)** | EE | - | SW |
| Approvals | Listed on editions page as EE; docs now present approval rules under Studio. Status unresolved [I] | `approval.request` **(M)** | EE | - | SW |
| Phone / VoIP | Softphone, SIP providers (Axivox, DIDWW, OnSIP), call logging; Odoo 20 call-flow designer | `voip.call` **(M)** | EE | - | LT |
| IoT | Box/drivers for printers, scales, scanners. `iot_drivers` is in CE; the IoT Box app is EE | `iot.device` **(M)** | EE | - | LT |
| WhatsApp | Templates, two-way chat, notifications, flows. Not in CE tree | `whatsapp.template/message` **(M)** | EE | - | SW |
| Appointments | Online booking pages, group sessions (19), Google Reserve, capacity (20), lead creation | `appointment.type` **(M)** | EE | calendar | SW |

### 3.2 Finance

| App (module) | Purpose and key features | Entities | Ed. | Needs | Class |
|---|---|---|---|---|---|
| **Invoicing** (`account`, app) | Customer invoices, credit/debit notes, vendor bills, payments and terms, cash discounts/rounding, taxes with repartition and tax groups, fiscal positions, journals, multi-currency, sequences, follow-ups (automatic invoice reminders in 20), SEPA/EPC QR, UBL/CII, Peppol (`account_peppol`), check printing, snailmail, audit trail. Odoo 19 merged receipts with bills/invoices and replaced fiscal-position mappings with per-tax applicability; Odoo 20 replaces account groups with parent accounts | `account.move/.line`, `account.account`, `account.journal`, `account.tax`, `account.tax.group`, `account.fiscal.position`, `account.payment(.term/.method)`, `account.bank.statement(.line)`, `account.reconcile.model`, `account.partial/full.reconcile`, `account.report*` engine | CE | base_setup, onboarding, product, analytic, portal, digest | TS |
| **Accounting** (`account_accountant` + reports, bank sync; private) | Full GL, trial balance, balance sheet, P&L, aged reports; bank sync (Plaid, Ponto, Basiq, Syncfy...) and reconciliation widget; assets and depreciation models; deferred revenue/expense; budgets (reworked 18); multi-ledger consolidation; tax returns with deadlines and checks (19); intrastat; year-end closing; data inalterability; Avalara; loans; accounting-firm mode; vendor bill OCR | `account.asset`, `account.budget`, `account.online.link` **(M)**; reports over `account.report*` [V in CE] | **EE** | account | TS |
| Expenses (`hr_expense`) | Submit, approve, post, reimburse, re-invoice; expense categories as products; cards; OCR (EE/IAP); payslip reimbursement (EE). Expense reports removed in 19 | `hr.expense` | CE (OCR/payslip EE) | account, hr | TS |
| Online Payments (`payment`, 21 providers) | Provider framework, tokens, portal/website payment. Providers in CE 20.0: Adyen, APS, AsiaPay, Authorize.net, Buckaroo, DPO, ECPay, Flutterwave, Iyzico, Mercado Pago, Mollie, Nuvei, Paymob, PayPal, PayU, Razorpay, Redsys, Stripe, Toss, Worldline, Xendit; plus wire transfer and demo | `payment.provider`, `payment.transaction`, `payment.token`, `payment.method` | CE | onboarding, portal | TS |
| Payroll | Salary structures and rules, payslips, pay runs (renamed from batches in 19), salary attachments, commissions, headcount; Odoo 20: net-to-gross simulation, salary indexation, work entries removed | `hr.payslip`, `hr.salary.rule`, `hr.payroll.structure` **(M)** | EE | hr, Time Off | SW |
| Sign | E-signature: templates, envelopes, multiple signers, SMS/itsme/Aadhaar auth, QES via itsme in 30+ European countries (20), automated requests | `sign.template`, `sign.request` **(M)** | EE | - | SW |
| ESG (new 19) | Carbon footprint by GHG Protocol and Bilan Carbone scopes, emission factors (ADEME, IPCC), social metrics (pay gap) | `esg.*` **(M)** | EE | account, fleet, hr | LT |
| Equity (new 19) | Shares, shareholders, options, valuation for fiduciaries | `equity.*` **(M)** | EE | - | LT |

### 3.3 Sales

| App (module) | Purpose and key features | Entities | Ed. | Needs | Class |
|---|---|---|---|---|---|
| **CRM** (`crm`) | Pipeline kanban, leads vs opportunities, predictive lead scoring, lead mining/enrichment via IAP, assignment rules and teams, recurring revenue plans, lost reasons, email aliases, mail plugin, livechat and SMS hooks | `crm.lead`, `crm.stage`, `crm.team`, `crm.lost.reason`, `crm.recurring.plan`, `crm.tag` | CE | base_setup, sales_team, mail, calendar, resource, utm, contacts, phone_validation | TS |
| **Sales** (`sale_management` on `sale`) | Quotation to order: templates (sections and subsections, 20), optional products, variants/matrix, PDF quote builder, e-signature and prepayment, pricelists (periodic and packaging rules, 20), discounts, margins (editable, 20), down payments, invoicing policies, commissions (payment-based and manager roll-ups in 20; edition of the commissions module unverified), combo products, Sales dashboard (20) | `sale.order`, `sale.order.line`, `crm.team(.member)`, `product.pricelist(.item)` | CE | sales_team, account_payment, utm (+ digest) | TS |
| Loyalty / coupons / gift cards / eWallets (`loyalty`) | Programs, rules, rewards, cards, point expiry (20); shared by Sales, eCommerce, POS | `loyalty.program/rule/reward/card` | CE | product, portal, account | SW |
| **Point of Sale** (`point_of_sale`, `pos_restaurant`) | Shop and restaurant POS: sessions, payment terminals (Adyen, Stripe, Mollie, Razorpay, Mercado Pago, Viva, Pine Labs, QFPay, Safaricom), cash machines (Glory, Cashmatic, CashDro), self-order kiosks, floors/tables, courses, presets, printers, allergen management, GrabFood/GoFood/UrbanPiper delivery (19-20), about 28 `l10n_*` modules with `pos` in the name (country fiscal/e-invoice POS add-ons; the count includes one test module and a withholding-tax one) | `pos.config/session/order/order.line/payment/payment.method/category/preset/printer`, `restaurant.floor/table`, `pos.course` | CE (IoT box EE) | resource, product, account, GS1, html_editor, phone_validation, iot_webserial | SW (TS for retail SMBs) |
| Subscriptions | Recurring plans on sales orders, renewals, upsells, closing reasons, automatic payment via tokens, eCommerce, loyalty (20) | `sale.subscription.plan` **(M)** on `sale.order` | EE | sale, account | SW |
| Rental | Rental periods, pickup/return, deposits, availability; unified pricelist engine (20) | **(M)** | EE | sale, stock | LT |
| Marketplace connectors | Amazon (EE per editions page), Shopee, Lazada, TikTok (19-20), Gelato print-on-demand (`sale_gelato` is CE) | **(M)** | mixed | sale, stock | LT |

### 3.4 Websites and online

| App (module) | Purpose and key features | Entities | Ed. | Needs | Class |
|---|---|---|---|---|---|
| **Website** (`website`) | Drag-and-drop builder (snippets, themes, SEO, multi-website, multilingual, cookie bar, visitor tracking), AI Website Assistant (20, IAP), banners, age gate | `website`, `website.page`, `website.menu`, `website.visitor`, `website.track`, `website.rewrite` | CE | digest, web, html_editor, http_routing, portal, social_media, auth_signup, mail, google_recaptcha, utm, html_builder | SW |
| **eCommerce** (`website_sale`) | Catalog, categories, variants, cart/checkout steps, delivery carriers, payments, wishlist, ribbons, abandoned carts, pickup in store, Google Merchant feed, stock levels, loyalty, AI product editing (20) | `product.public.category`, `product.ribbon`, `product.wishlist`, `website.checkout.step`, `product.feed`, `delivery.carrier` | CE | website, sale, website_payment, website_mail, portal_rating, delivery | SW |
| Blog, Forum | Posts/tags; Q&A with karma | `blog.post`, `forum.post`, `forum.forum` | CE | website_mail / auth_signup | LT |
| eLearning (`website_slides`) | Courses, lessons, quizzes, certifications (survey), ratings, selling courses | `slide.channel`, `slide.slide`, `slide.question` | CE | portal_rating, website, website_profile | LT |
| Live Chat (`im_livechat`) | Website chat, operators, chatbot scripts, AI agents (19), expertise routing, Messenger/Instagram (20), leads | `im_livechat.channel`, `chatbot.script`, `discuss.channel` | CE | mail, utm | SW |
| Online jobs | Public job board on Recruitment | `hr.job` | CE | - | SW |

### 3.5 Supply chain

| App (module) | Purpose and key features | Entities | Ed. | Needs | Class |
|---|---|---|---|---|---|
| **Inventory** (`stock`) | Warehouses, hierarchical locations, routes and rules (pull/push, MTO), putaway, storage categories, removal strategies (FIFO, LIFO, FEFO, closest, least packages), reservation methods, lots/serials/expiry, packages (nested in 19), reordering rules (with demand-based suggestions, 20), scrap, cycle counts, dropshipping, landed costs, carrier connectors (UPS, FedEx, DHL, USPS, bpost, Sendcloud, ShipStation per docs; only generic `delivery` is in the CE tree, so the carrier modules are EE [I]), valuation (standard/AVCO/FIFO), traceability, UoM/packaging merged in 19 | `stock.warehouse`, `stock.location`, `stock.picking(.type/.batch)`, `stock.move(.line)`, `stock.quant`, `stock.lot`, `stock.route`, `stock.rule`, `stock.warehouse.orderpoint`, `stock.package`, `stock.storage.category`, `product.value` | CE | product, GS1 nomenclature, digest (`stock_account` adds valuation) | TS |
| Barcode (`stock_barcode`) | Scan receipts, picks, packs, counts; dedicated PWA | **(M)** | EE | stock | SW |
| **Purchase** (`purchase`) | RFQ, PO, vendor pricelists, bill control, blanket orders, alternatives comparison, vendor portal, expected arrival dates (20), EDI UBL | `purchase.order(.line)`, `purchase.requisition`, `product.supplierinfo`, `purchase.bill.line.match` | CE | account | TS |
| **Manufacturing** (`mrp`) | BoMs (normal, kit), MOs, work centers and routings, work orders, by-products, unbuild, scrap, subcontracting (+dropship), MPS (EE), OEE, MO costing, Gantt (19-20), continuous production (20) | `mrp.bom(.line/.byproduct)`, `mrp.production`, `mrp.workorder`, `mrp.workcenter`, `mrp.routing.workcenter`, `mrp.unbuild` | CE (Shop Floor, MPS, scheduling EE) | product, stock, resource | SW (TS for makers) |
| Shop Floor | Tablet UI for work orders, barcode, instructions | **(M)** | EE | mrp | SW |
| PLM | Engineering change orders, BoM versioning and approvals | `mrp.eco` **(M)** | EE | mrp | LT |
| Quality | Control points, checks (pass/fail, measure, picture, instruction), alerts | `quality.point/check/alert` **(M)** | EE | stock | SW |
| Maintenance (`maintenance`) | Equipment, preventive/corrective requests, teams, stages, Gantt (20) | `maintenance.equipment/request/team` | CE | mail | SW |
| Repairs (`repair`) | Repair orders, parts, service lines, invoicing (20) | `repair.order` | CE | sale_stock, sale_management | LT |
| Delivery (`delivery`) | Carrier definitions, price rules | `delivery.carrier`, `delivery.price.rule` | CE | sale | TS |

### 3.6 Human resources

| App (module) | Purpose and key features | Entities | Ed. | Needs | Class |
|---|---|---|---|---|---|
| **Employees** (`hr`, `hr_skills`) | Directory, org chart, departments, jobs, work locations, skills/resume/certifications, onboarding/offboarding plans; 19 merged employees and contracts into versioned `hr.version` | `hr.employee`, `hr.version`, `hr.department`, `hr.job`, `hr.skill(.type/.level)`, `hr.resume.line` | CE | auth_signup, resource_mail, web_hierarchy, phone_validation | TS |
| **Time Off** (`hr_holidays`) | Leave types, requests, allocations, accrual plans, mandatory days, public holidays (preloaded in 20) | `hr.leave`, `hr.leave.allocation`, `hr.leave.accrual.plan` | CE | hr_work_entry, hr_calendar, resource | TS |
| Attendances (`hr_attendance`) | Check-in/out, kiosk, overtime rules, GPS | `hr.attendance` | CE | hr, barcodes, base_geolocalize | SW |
| Recruitment (`hr_recruitment`) | Job positions, applicant pipeline, interviews, sources, refusal reasons, talent pool (replaced candidates in 19), matching score and a matching-score filter (20; whether the score is AI-based is unverified) | `hr.applicant`, `hr.recruitment.stage`, `hr.talent.pool` | CE | hr, calendar, utm | SW |
| Fleet (`fleet`) | Vehicles, contracts, services, odometer, costs | `fleet.vehicle`, `fleet.vehicle.log.*` | CE | mail | LT |
| Lunch (`lunch`) | Daily meal orders from suppliers | `lunch.order`, `lunch.supplier` | CE | mail | LT |
| Appraisals, Referrals, Frontdesk | Reviews and goals (campaigns in 19, 360); referral points and rewards; visitor kiosk | **(M)** | EE | hr | LT |

### 3.7 Marketing

| App (module) | Purpose and key features | Entities | Ed. | Needs | Class |
|---|---|---|---|---|---|
| Email Marketing (`mass_mailing`) | Mailing lists, campaigns, templates, tracking, unsubscribe, A/B-style filters, bounce handling | `mailing.mailing/list/contact/trace/filter` | CE | contacts, mail, html_builder, utm, link_tracker | SW |
| SMS Marketing (`mass_mailing_sms`) | SMS blasts via IAP or Twilio | as above | CE | mass_mailing, sms | LT |
| Events (`event`, `website_event`) | Events, tickets, registrations, slots (multi-slot in 19), booths, talks/tracks, exhibitors, questions, sales integration | `event.event/ticket/registration/booth` | CE (registration desk/check-in EE) | mail, portal, utm | LT |
| Surveys (`survey`) | Forms, scoring, certification, live sessions | `survey.survey/question/user_input` | CE | mail, gamification | LT |
| Marketing Card (`marketing_card`) | Social card generator | - | CE | - | LT |
| Marketing Automation | Campaigns, activities, audience filters; AI-assisted creation (20) | `marketing.campaign/activity` **(M)** | EE | mass_mailing | LT |
| Social Marketing | Multi-network posting and streams; Facebook, Instagram stories, LinkedIn personal (20) | `social.post/account` **(M)** | EE | - | LT |

### 3.8 Services

| App (module) | Purpose and key features | Entities | Ed. | Needs | Class |
|---|---|---|---|---|---|
| **Project** (`project`) | Projects, kanban tasks, stages, subtasks, recurrence, milestones, updates, roles (20), portal collaborators, templates (19), customer ratings. Dependencies/Gantt views are EE [I] | `project.project`, `project.task`, `project.task.type`, `project.milestone`, `project.update`, `project.role` | CE | analytic, mail, portal_rating, resource | TS |
| **Timesheets** (`hr_timesheet`) | Time on tasks as analytic lines; timesheet-based invoicing (`sale_timesheet`). Grid view, timer, reminders, validation are EE (editions page); ActivityWatch and assistant in 20 | `account.analytic.line` | CE (+EE extras) | hr, analytic, project, uom | TS |
| Planning | Shifts, roles, resources (employees and materials), auto-plan, Gantt | `planning.slot` **(M)** | EE | - | SW |
| Field Service | On-site interventions as shifts, worksheets, parts, signature, routing, live map (20) | **(M)** | EE | planning, project | LT |
| Helpdesk | Teams, tickets, SLAs, ratings, help centre, billing time to tickets, returns and repairs links | `helpdesk.ticket/team/sla` **(M)** | EE | - | SW |

### 3.9 Customization and AI

| App | Purpose and key features | Ed. | Class |
|---|---|---|---|
| **Studio** | No-code customiser inside every app: fields, views, models, menus, apps, automation rules + webhooks, PDF report editor, approval rules, security rules; Odoo 20 adds activity-plan triggers and kanban stage editing (https://www.odoo.com/documentation/19.0/applications/studio.html). Among paid plans only on Custom (installing it on Standard switches the database to Custom); the pricing FAQ also lets One App Free users pick Studio as their one free app | EE | SW |
| Properties (custom fields, `property` fields) | Per-record and per-parent custom fields without Studio; used by Field Service worksheets in 20 | CE | TS |
| Developer platform | External API, Python modules, Odoo.sh; apps store (`apps.odoo.com`) | CE/EE | TS |
| **AI** | 19: chat agents grounded on documents/Knowledge/URLs with database queries, AI fields, natural-language search to domain, draft/summarise, voice transcription, web page generation, ChatGPT and Gemini. 20: agents create and update records, run from automations and schedules, voice, "skills" (formerly topics), interactive answers, image generation, MCP ("Connect Odoo to anything: Connect to your database via MCP"; the notes give no further detail on direction or scope, so whether this is an MCP server exposing Odoo, an MCP client inside agents, or both is unverified), automatic model selection, conversations stored up to 30 days after the last message. All features need IAP credits (Odoo 20 notes), and the pricing page lists "Agentic AI" among Custom-plan features. No `ai` module in the CE tree [V], so EE/online [I] | EE | SW |
| Industries | Pre-configured bundles (restaurant, hotel, law firm, vineyard, catering, interior design, public institution...) delivered as data + Studio customisations on top of standard apps; 20 added Catering, Vineyard, Interior Design, Public Institution, Electronic Refurbishment and renamed two construction packs | EE | LT |

---

## 4. Country localizations

Counts [V, 20.0 CE tree]: 229 `l10n_*` directories, of which **118 are bare country packages** (`l10n_xx`) plus `l10n_lb_account`, `l10n_mu_account`, `l10n_tz_account`, `l10n_zm_account` and `l10n_syscohada` (OHADA chart, [V manifests] reused by 17 African wrapper packages such as `l10n_bf`, `l10n_cm`, `l10n_sn`, all depending on `l10n_syscohada`). Of the rest: 43 modules with `edi` or `ubl` in the name, about 46 counting the India e-way bill and Chorus Pro modules and excluding one test module (UBL/PINT, Peppol, and government systems: Spain SII/TicketBAI/Verifactu/Facturae, Italy SDI, India IRN and e-way bill, Poland `l10n_pl_edi`, Romania e-Factura, Saudi ZATCA, Hungary NAV, Vietnam Viettel, Taiwan ECPay, Greece myDATA, Croatia, Serbia, Jordan, Egypt ETA, Malaysia MyInvois, Pakistan), about 28 `pos`-named modules (see section 3.3), sale/stock bridges (India e-way bill, Argentina remitos, Italy DDT), LATAM document types and checks, DIN 5008 layouts (Germany), EU OSS and VIES. Docs: 103 countries listed in the fiscal-localization country list (counted from the RST source on branch 20.0; the rendered 19.0 page also shows 103), 46 of them with dedicated pages on 20.0 (40 on the rendered 19.0 page) (https://www.odoo.com/documentation/20.0/applications/finance/fiscal_localizations.html; file tree https://github.com/odoo/documentation/tree/20.0/content/applications/finance/fiscal_localizations). The docs list is smaller than the 118 bare `l10n_xx` country packages in the tree, so it is not exhaustive. Release notes touch 47 countries in 18, 53 in 19 and 55 in 20 (most entries are incremental: Odoo 20 adds a new Sri Lanka accounting package and base payroll packages for Iraq, Kuwait and Oman, while several others such as Kyrgyzstan, Tajikistan and Turkmenistan only get ISO 3166-2 state-code updates; https://www.odoo.com/odoo-20-release-notes).

Nature of a package [V, e.g. `l10n_de`]: a manifest with `countries`, `auto_install: ['account']`, chart-of-accounts templates as CSV (`account.account`, `account.tax`, `account.tax.group`, `account.fiscal.position`, `account.depreciation.model`; Germany ships SKR03 and SKR04), tax tags for returns, audit-trail defaults (GoBD), plus optional EDI, POS certification, bank/payment formats, document layouts. Core packages install automatically based on company country; a company can switch packages only before posting; branches follow the parent company's localization (same docs page).

Payroll localizations are a separate, EE-only family (no `hr_payroll` or `l10n_*_hr_payroll` module exists in the CE tree [V]). The docs have 14 dedicated pages (Australia, Belgium, Egypt, Employment Hero (an Australian payroll integration, not a country), Hong Kong, India, Jordan, Kenya, Mexico, Oman, Saudi Arabia, Turkey, UAE, US), but the payroll-localizations index on the 20.0 branch lists 24 countries as cards (Australia, Bangladesh, Belgium, Egypt, Hong Kong, India, Indonesia, Jordan, Kenya, Lithuania, Luxembourg, Mexico, Morocco, Netherlands, Oman, Pakistan, Poland, Romania, Saudi Arabia, Slovakia, Switzerland, Turkiye, UAE, US; 23 on the 19.0 branch), so "14 countries" understates coverage. Odoo 20 notes add base payroll packages for Iraq, Kuwait and Oman; Belgium alone has dozens of rules (Dimona, joint committees, flexi-jobs) (unverified). Localization is the single largest long-tail cost in Odoo [I]: regulators keep changing (Brazil alphanumeric CNPJ and IBS/CBS reform appear in the 20 notes [V]; Spain Verifactu and France PDP have CE modules `l10n_es_edi_verifactu(_pos)` and `l10n_fr_pdp(_pos)` [V tree] but are not mentioned in the 18/19/20 release notes).

---

## 5. Community vs Enterprise

**EE-only (23 rows on https://www.odoo.com/page/editions)**: Accounting (full), Payroll, Documents (+Spreadsheet), Sign, ESG, Subscriptions, Rental, Amazon Connector, PLM, Quality, Referrals, Appraisals, Social Marketing, Marketing Automation, Field Service, Helpdesk, Planning, Appointments, Approvals, VoIP, IoT, Knowledge, Studio. **EE sub-features** of CE apps: Inventory Barcode; Manufacturing Shop Floor, control panel, scheduling; Invoicing AI; Expenses OCR and payslip reimbursement; Timesheet grid, timer, reminders, validation; mobile apps; hosting, upgrades, unlimited functional support. **CE (28 rows)**: Invoicing, Expenses, CRM, Sales, POS, Website, eCommerce, Blog, Forum, Live Chat, eLearning, Inventory, Manufacturing, Purchase, Maintenance, Employees, Recruitment, Time Off, Fleet, Email Marketing, SMS Marketing, Events, Surveys, Project, Timesheet, Discuss, To-do, Calendar. Not on the table but checked by presence/absence in the CE tree [I]: Attendances, Lunch, Repairs, Marketing Card, Data Recycle are CE; WhatsApp, Frontdesk, Equity, AI, Dashboards editor, Shop Floor are not in CE.

Takeaway [I]: CE covers the operational transaction apps (sell, buy, stock, make, project, HR basics, web); EE monopolises the financial statements layer, payroll, documents/sign/knowledge, helpdesk/planning/field service, marketing automation and no-code Studio. The strongest pull toward paying is accounting.

---

## 6. Proposed tiering for obop

Mapping: TS to T1, SW to T2, LT to T3; T0 is the platform kernel that is not an "app" in Odoo's menu but underlies all of them.

| Tier | Contents | Why |
|---|---|---|
| **T0 Kernel** | Users/groups/domain-scoped access rights, multi-company + branches + currency + language, partners, products/UoM/pricelists, analytic plans, chatter/activities/followers/attachments, mail gateway and templates, portal, automation rules + webhooks, custom properties, import/export, PDF reports, declarative views (form, list, kanban, pivot, graph, calendar, gantt, map), search/favourites, sequences, cron, audit log, API, i18n, module manifest with dependencies and bridges | every Odoo app reuses these; Odoo 20 still reworks them (offline, access-right simplification, in-body tracking) |
| **T1 Must-have core** | Contacts; CRM; Sales; Purchase; Inventory (incl. lots, valuation, reordering, dropship, landed costs); **real Accounting** (GL, AR/AP, taxes, fiscal positions, bank reconciliation, assets, deferrals, closing, standard reports) in the open core; Payments (manual + Stripe + 1-2 more); Expenses; Project + Timesheets (billable); Employees + Time Off; Discuss, Calendar, To-do; Documents-lite (DMS over attachments); localization framework + 2-3 reference packs with e-invoicing (Peppol/UBL first) | the order-to-cash and procure-to-pay chains plus accounting are the product; without them it is not a business OS |
| **T2 Second wave** | Manufacturing (+Shop Floor, Quality, Maintenance, Barcode), POS (retail then restaurant), Website + eCommerce, Helpdesk, Recruitment, Attendances, Payroll (per country), Subscriptions, Sign, Knowledge, Appointments, Planning/Field Service, Email + SMS marketing, Live Chat, WhatsApp, loyalty, Studio-style no-code builder, Spreadsheet/Dashboards, AI agents + MCP server | big revenue segments (makers, retail, services) but each needs vertical depth or heavy compliance |
| **T3 Long tail** | Rental, PLM, IoT/VoIP, Social Marketing, Marketing Automation, eLearning, Forum/Blog, Events, Surveys, Fleet, Lunch, Frontdesk, Referrals, Appraisals, ESG, Equity, Marketing Card, marketplace connectors (Amazon, Shopee, Lazada, TikTok), Gelato, Repairs, industry bundles (60+), Approvals | niche, or best delivered as community add-ons over a stable extension API |

---

## 7. Confidence & gaps

- **High [V]**: Odoo 20 release (Sept 2026) and support dates; CE module inventory, dependencies, `auto_install` flags and model names (parsed from source); editions matrix (parsed from raw HTML); licence names; localization counts in the CE tree.
- **Medium**: pricing (re-parsed from the page HTML on 2026-10-01: annual vs monthly billing and promo vs list are now resolved, but prices are the Europe/EUR view and the Light User unit is not printed); docs country counts (103 listed, re-counted; approximate elsewhere, such as the EDI and POS module-name tallies); classification of Approvals (EE per editions page but absent from docs app list and 19/20 release-note indexes).
- **Low / not verified**: all EE model names (private repo, tagged **(M)**); which Shopee/Lazada/TikTok connectors are EE; whether Project Gantt/dependencies and Data Cleaning merge are EE (inferred); the IoT Box vs `iot_drivers` split; the docs licence (not checked, so no doc text should be copied); vendor-claimed user/partner numbers.
- **Not covered**: Odoo.sh/Online hosting internals, Odoo mobile apps' native features, performance characteristics, exact Odoo 20 data-model changes beyond the release notes, and a line-by-line Enterprise feature list (the comparison table hides sub-features).
- Research tooling: WebSearch was unavailable, so discovery relied on direct fetches of primary sources (about 60 fetches/API calls). No secondary sources were needed for load-bearing claims.

---

## 8. Implications for obop

1. **MUST ship real double-entry accounting in the open core.** CE Odoo offers only Invoicing; GL, bank reconciliation, reports, assets, deferrals and tax returns are EE, and third-party "Accounting Community" clones already exist on apps.odoo.com. This is obop's clearest wedge and the chain sale to delivery to invoice to payment to journal entries (section 2.2) is the acceptance test.
2. **MUST design modules as small apps plus explicit bridge modules.** Odoo's 400 `auto_install` bridges over 34 apps is the pattern that keeps apps standalone yet integrated. In Rust, use a module manifest (`depends`, `auto_install`, data, migrations) resolved at startup/tenant-enablement in a single modular-monolith binary, not dynamic loading.
3. **MUST build the T0 kernel first and make it metadata-driven.** A declarative model and view registry (fields, views, access domains, properties) is what makes Studio, industries and localizations cheap. Adopt Odoo 20's single-domain-per-access-right simplification instead of record rules.
4. **MUST treat localization as a plugin system, not code.** Ship a chart/tax/fiscal-position template format (CSV/TOML like `l10n_de`'s `account.*-de_skr03.csv`), e-invoicing adapters (Peppol/UBL/CII first, then country systems), and a POS fiscal hook. Launch with few packs; expect 229-module scale over time.
5. **MUST keep sales, inventory and valuation separable.** Odoo 20 now permits shipping without Inventory, valuation without Inventory, POS without Inventory; copy that decoupling from day one.
6. **SHOULD stage apps per section 6**: T1 within the first releases, T2 driven by demand (Manufacturing and POS first), T3 via a public extension API and marketplace rather than first-party work.
7. **SHOULD make payments pluggable but start narrow** (Stripe plus a wire/manual method; Odoo has 21 providers in CE, most regional).
8. **SHOULD make AI a cross-cutting layer with an MCP server from the start.** Odoo 20 added agents that create/update records, automation-triggered agents and MCP; it meters everything via IAP credits. obop can differentiate by BYO-model keys and no credit wall.
9. **SHOULD support industry bundles as data + no-code config** (Odoo has 60+), delaying bespoke vertical code.
10. **COULD include a Studio-equivalent (no-code fields/views/automation/report designer) in T2**; custom properties and automation rules belong in T0/T1.
11. **COULD mirror Odoo's cadence and versioning concepts** (annual major, 3-year standard support) only if you want to court Odoo migrants; otherwise prefer semver and continuous delivery.
12. **COULD build an Odoo data importer** (partners, products, chart of accounts, open items) as an adoption lever; model names in section 3 give the mapping starting points.
13. **Legal hygiene**: study CE (LGPL-3) behaviour and the public docs, but keep EE-only code, assets, templates and marketing copy out of the repo; document each feature's provenance.

Sources index (primary): https://www.odoo.com/page/editions , https://www.odoo.com/page/release-notes , https://www.odoo.com/odoo-17-release-notes , https://www.odoo.com/odoo-18-release-notes , https://www.odoo.com/odoo-19-release-notes , https://www.odoo.com/odoo-20-release-notes , https://www.odoo.com/pricing , https://www.odoo.com/page/about-us , https://www.odoo.com/documentation/19.0/ , https://github.com/odoo/odoo/tree/20.0 , https://github.com/odoo/documentation/tree/20.0 , https://apps.odoo.com/apps/modules .

---

## Verification log

Adversarial fact-check run on 2026-10-01 against live primary sources: the Odoo release-notes index and the 18/19/20 notes, the support-policy RST on `odoo/documentation` 20.0 and its rendered page, `odoo/odoo` 20.0 `release.py`, `LICENSE` and all 641 manifests (GitHub API and raw files), the editions, pricing, about-us and all-industries pages re-downloaded as raw HTML, the docs licence text, the docs fiscal- and payroll-localization sources, and apps.odoo.com. All counts below were recomputed from those sources, not copied from the earlier draft.

**Confirmed as stated (about 66 discrete claims):** Odoo 20 released September 2026 with standard support to September 2029 (planned); SaaS 19.1-19.4 dates (Jan/Mar/May/Jul 2026); 17.0 (Nov 2023, standard support ends Sept 2026), 18.0 (Oct 2024, Sept 2027 planned) and 19.0 (Sept 2025, Sept 2028 planned) dates; default branch `20.0`; Python 3.12-3.14; `license = 'LGPL-3'`; 54,767 stars, 33,902 forks, OCA 271 public repos; editions table with 51 app rows, 28 CE and 23 EE-only, with both named lists and the EE sub-feature list; Community is LGPL-3 and Enterprise uses the Odoo Enterprise Edition License v1.0 requiring a valid subscription for the correct number of users; 642 addon directories, 413 non-l10n, 229 `l10n_*`, 20 `test_*`; 34 `application: True`; 400 `auto_install`; 118 bare `l10n_xx`; 21 payment providers; bridge-module dependencies (`stock_account`, `sale_stock`, `purchase_stock`, `sale_purchase`, `mrp_account`, `sale_mrp`, `sale_crm`, `pos_*`, `sale_expense`, `website_payment`); `stock.move._create_account_move` gated on `valuation == 'real_time'`; costing methods standard/average/FIFO; One App Free, Standard 24.90/19.90, Custom 44.90/35.90, Light User 7.90; Odoo 20 AI (agents create and update records, automation and schedule triggers, MCP entry, all AI needs IAP credits); no `ai` module in the CE tree; Odoo 19 `hr.version` employee/contract merge and talent pool; Odoo 20 parent accounts, removal of record rules in favour of a domain on access rights, removal of Payroll work entries; scale claims (50 main apps, 28 million users, 8,000+ employees, 21,000+ partners, 50,000+ apps); 46 country pages in the docs source; release-note country sections 47/53/55; 61 industry pages; "Odoo 20 Accounting Community" present on apps.odoo.com; the "built on Community, switch at any time" wording on the editions page; Odoo 20 feature claims spot-checked against the notes (payment status relabel, POS global sale, expense to draft bill, shipping/valuation/POS without Inventory, DUNS multi-identifier, Sri Lanka package, Iraq/Kuwait/Oman payroll, five new industries); Odoo 19 deltas (receipts merged, fiscal-position tax applicability, pay runs, expense reports removed, packages within packages, UoM/packaging merge, Dun & Bradstreet, ESG and Equity new).

**Corrections (was / now / source):**

| # | Was | Now | Source |
|---|---|---|---|
| 1 | Docs list "about 115 countries" (also in section 7) | 103 countries (unique bullets in the list; the rendered 19.0 page also shows 103) | https://github.com/odoo/documentation/blob/20.0/content/applications/finance/fiscal_localizations.rst ; https://www.odoo.com/documentation/latest/applications/finance/fiscal_localizations.html |
| 2 | "Payroll localizations are documented for 14 countries" | 14 dedicated pages, one of which (Employment Hero) is an integration, not a country; the payroll-localizations index lists 24 countries on 20.0 (23 on 19.0) | https://github.com/odoo/documentation/blob/20.0/content/applications/hr/payroll/payroll_localizations.rst |
| 3 | "Studio, multi-company and external API are only on Custom" | True between Standard and Custom, but One App Free allows multiple companies and Studio as the one free app; Standard is Odoo Online only without custom modules; "Agentic AI" is listed under Custom | https://www.odoo.com/pricing (FAQ) |
| 4 | Monthly-billing figures 31.10 / 56.90 at "medium confidence" | Resolved: on the monthly toggle these are the struck-through list prices (discounted 24.90 / 44.90); 24.90 and 44.90 are the annual-billing list prices. Light User unit is not printed on the page | https://www.odoo.com/pricing (raw HTML `yearly_price` / `monthly_price` spans) |
| 5 | "642 `__manifest__.py` files", "400 of the 642 are auto_install" | 641 manifests (`mrp_subcontracting_repair` has only `i18n/`), so 400 of 641 | https://github.com/odoo/odoo/tree/20.0/addons/mrp_subcontracting_repair |
| 6 | `version_info = (20, 0, 0, FINAL, 0)` | `(20, 0, 0, FINAL, 0, '')` | https://github.com/odoo/odoo/blob/20.0/odoo/release.py |
| 7 | Family counts `sale_*` 29, `website_*` 52, `hr_*` 24, `payment_*` 24, `project_*` 18, `account_*` 16, `mrp_*` 12, `mass_mailing_*` 12 | Correct only as base module plus children (children alone are one lower); relabelled | GitHub tree listing of `odoo/odoo` 20.0 `addons` |
| 8 | "about 16" African wrappers of `l10n_syscohada` | 17 (now [V]) | manifests of `l10n_bf`, `l10n_bj`, `l10n_cd`, `l10n_cf`, `l10n_cg`, `l10n_ci`, `l10n_cm`, `l10n_ga`, `l10n_gn`, `l10n_gq`, `l10n_gw`, `l10n_km`, `l10n_ml`, `l10n_ne`, `l10n_sn`, `l10n_td`, `l10n_tg` |
| 9 | "about 46 EDI/UBL-named" and "28 POS-fiscal" l10n modules | 43 names containing `edi` or `ubl` (about 46 adding e-way bill and Chorus Pro modules and dropping a test module); 28 names containing `pos`, including a test module and a withholding-tax module, so "fiscal" is loose | GitHub tree listing of 20.0 `addons` |
| 10 | "17.0 standard support ends Sept 2026" (future-tense reading) | Has ended as of 2026-10-01; table shows 17.0 unsupported on Online and extended (paid) on Odoo.sh / on-premise; SaaS 19.1 and 19.2 are already unsupported | https://github.com/odoo/documentation/blob/20.0/content/administration/standard_extended_support.rst |
| 11 | Sign "QES in 30+ EU countries" | "over 30 European countries" via itsme | https://www.odoo.com/odoo-20-release-notes |
| 12 | Recruitment "AI matching score" | Notes only mention a "Matching Score" filter in 20; AI basis not stated, now marked unverified | https://www.odoo.com/odoo-20-release-notes |
| 13 | MCP described as agents "connecting over MCP" | Notes say only "Connect to your database via MCP"; direction and scope unstated, now marked unverified | https://www.odoo.com/odoo-20-release-notes |
| 14 | Bridge pattern "activate when both parents are installed", with `stock_dropshipping` and `account_payment` as examples | `stock_dropshipping` is not `auto_install`; `account_payment` has `auto_install: ['account']` only | manifests of both modules, `odoo/odoo` 20.0 |
| 15 | Section 4 regulators "Spain Verifactu, France PDP" read as coming from the 20 notes | They exist as CE modules (`l10n_es_edi_verifactu`, `l10n_fr_pdp`) but are not mentioned in the 18/19/20 notes; Brazil CNPJ and IBS/CBS are in the 20 notes | GitHub tree listing; https://www.odoo.com/odoo-20-release-notes |
| 16 | Payroll being EE-only was implied | Added [V]: zero `payroll` directories in the CE tree | GitHub tree listing of 20.0 `addons` |

**Unresolved / unverified:**

- Odoo 20 MCP: direction (Odoo as MCP server, agents as MCP client, or both) and scope are not stated in the release notes.
- Prices are the EUR / Europe view served to this fetch; other regions and currencies, and the Light User billing unit, were not verified.
- All Enterprise model names tagged **(M)** (private source), plus Enterprise feature lists that come only from release notes.
- Shopee / Lazada / TikTok connector edition (CE vs EE); Project Gantt and dependency views and the Data Cleaning merge being EE (inferred); commissions module edition.
- Belgium payroll rule names (Dimona, joint committees, flexi-jobs) were not checked; VoIP providers Axivox, DIDWW and OnSIP do have docs pages.
- Vendor-stated numbers (28 million users, 21,000+ partners, 50,000+ apps) are confirmed as what the page says, not as audited facts.
- Approvals edition classification (EE per the editions table, absent from the docs app list).
- Star and fork counts are a point-in-time snapshot; the GitHub API licence field reads NOASSERTION, so the licence rests on `LICENSE` and `release.py`.
