# Sales and CRM: product catalog, pricing, CRM, quotations and orders, subscriptions, rental, customer portal

**Status:** Draft 0.1  
**Date:** 2026-10-01  
**Scope:** Functional specification of the catalog, pricing, CRM, sales, subscription, rental, commission and customer-portal modules, including the order-to-cash flow into inventory and accounting.

Conventions. D-IDs point to the binding register [DECISIONS](../DECISIONS.md); names of events, permission groups, data classes, states and ports follow [CONVENTIONS](../CONVENTIONS.md); R-tags cite the research corpus (section 10). "Proposed" marks anything the owner has not ratified, including every tier and wave value, because D25 and D26 are themselves Proposed; the same applies wherever this document relies on D2, D4, D13, D14, D17 or D20, which are also Proposed. Targets are labelled (target) and estimates (estimate). "Odoo Community" and "Odoo Enterprise" always mean Odoo's editions; obop has one open edition plus the commercial `ee` tree (D24). Odoo 20 means "September 2026 (release commit 2026-09-17)". Numbers in section 5 are test fixtures, not statutory rates or price points. Nothing here is legal advice.

## 1. Purpose and scope

### 1.1 What this area is

The commercial front of the business: who we might sell to (CRM), what we sell (catalog), at what price (pricing), the commercial document from quote to order (sales), recurring revenue (subscriptions), time-bound hire (rental), what salespeople earn (commissions) and the surface customers touch (portal). It is the first half of the seven-app loop (Proposed, D4; Contacts/Products, CRM, Sales, Portal), and it feeds Inventory, Purchase, Invoicing and Accounting through ports and events (D27). The sale line is the join point: delivery, invoicing, projects and payments each contribute a quantity to it, and line status is a function of those quantities, never a separately edited field [R11 s7].

### 1.2 Modules

| Module (app) | Covers | Tier | Wave | Open or paid |
|---|---|---|---|---|
| `product` (Product catalog) | templates, variants, attributes, UoM, categories, combos | T0 | W1 | Open |
| `pricing` (Pricing) | pricelists, discounts, deterministic engine, explain trace | T0 | W1 | Open |
| `portal` (Customer portal) | capability links, portal accounts, acceptance primitive | T0 | W1 | Open |
| `crm` (CRM) | leads, pipeline, teams, forecast; scoring and auto-assignment in W2 | T1 | W1 | Open |
| `sales` (Sales) | quotations, orders, optional lines, down payments, margins | T1 | W1 | Open |
| `sale_stock` (bridge) | order to delivery; specified in [supply-chain](supply-chain.md) | T1 | W1 | Open |
| `sale_project`, `sale_timesheet` | service lines to tasks and billable time; see [services](services.md) | T1 | W2 (W1 only if the loop is on schedule at 0.5) | Open |
| `subscriptions` | plans, proration, dunning, MRR ledger | T2 | W3 | Open |
| `rental` | period reservations, duration pricing, deposits | T3 | W4 or community | Open |
| `commissions` | plans, targets, periods, clawback | T3 (unlisted modules default to T3) | W4 or community (Proposed) | Open |

Everything above is open (Proposed, D2; section 6 of the register): no module here is gated by plan or user count. The paid items that touch this area are hosted gateways for transmitting the invoices created here (Peppol, KSeF, Veri*Factu), the per-country Assurance service, hosted AI credits, hosting and support (Proposed, D2). No payment-processing margin is on the paid list; the list is closed, so adding one would contradict D2 and need the owner's decision.

### 1.3 Boundaries

| Neighbour | Owns | Interaction here |
|---|---|---|
| Contacts and companies ([platform](platform.md)) | party, `party_profile` (pricelist, terms, credit limit, fiscal position), typed addresses, principals | referenced, never duplicated; customer is a role, not a table (D28) |
| Finance ([finance](finance.md)) | invoices, tax, payments, posting, e-invoicing, lock dates | `sales` is an `InvoiceableSource`; totals computed once and equal to postings |
| Supply chain ([supply-chain](supply-chain.md)) | moves, reservations, valuation, kits | `sale_stock` bridge; `DemandPort`, `QtyContributionPort` |
| Services ([services](services.md)) | projects, timesheets, planning | `sale_project`, `sale_timesheet` (W2) |
| Retail and commerce ([retail-commerce-marketing](retail-commerce-marketing.md)) | POS, cart, loyalty, campaigns | reuse the pricing crate; loyalty engine not specified here |
| Platform services ([platform](platform.md)) | chatter, mail, documents, sign, approvals, automation, semantic layer | consumed, not re-specified |
| People ([people](people.md)) | payroll | commission statements hand off; no payroll in this area (D19) |
| Integrations, AI, security, UX ([integrations](integrations.md), [ai](ai.md), [security-privacy](security-privacy.md), [ux](ux.md)) | API, agents, identity and audit, views | this area supplies entities, commands and permission rows |

## 2. Personas and roles

The ICP is a 5-50 person services or light-trade firm (Proposed, D4) [R20 s3, s9.2]. Distributors need lead to quote to delivery to invoice, fully served in W1; agencies add project and timesheet billing, served in W2.

| Persona | Jobs | Default permission group (Proposed names; scope, D10) |
|---|---|---|
| Salesperson | own leads, quotes, follow-ups | `crm.user`, `sales.user` (`own`) |
| Team leader | pipeline review, assignment, approvals | `crm.team_lead`, `sales.team_lead` (`domain`: team) |
| Sales manager | all pipelines, forecast, discount policy | `crm.manager`, `sales.manager` (`company`) |
| Sales operations | catalog, pricelists, templates, plans | `product.manager`, `pricing.manager`, `subscriptions.manager` (`company`) |
| Margin reader | sees cost and margin | `product.cost_viewer` (`company`, field permission); not in the salesperson default |
| Billing clerk | invoices from orders, payment chasing | `sales.invoicing` plus finance groups |
| Customer success | renewals, dunning holds, plan changes | `subscriptions.user` (`domain`) |
| Rental coordinator | reservations, pickups, returns | `rental.user` (`company`) |
| Commission administrator | plans, period close, adjustments | `commissions.manager`; salespeople get `commissions.self` (`own`, read only) |
| Customer | view, accept, pay, self-service | principal kind portal, or a capability link |
| Agent | drafts, lookups | agent principal; permissions are the intersection of user and token scopes (D18) |
| Integrator | sync, automation | API-client principal with scoped keys |

All permission groups are default-deny; a permission row without a declared scope is a lint error (D10). The group names are registered in [CONVENTIONS](../CONVENTIONS.md) section 4 and stay Proposed until the seed list of [security-privacy](security-privacy.md) 2 carries them.

## 3. Capability map

Wave values are Proposed (D26). In the last column "Community" and "Enterprise" are Odoo's editions [R01 s5].

| # | Capability | Description | Wave | Odoo parity |
|---|---|---|---|---|
| 1 | Template and variant model | three variant modes, exclusions, price extra, explosion guard | W1 | matches Community |
| 2 | Units and packagings | UoM tree, per-UoM rounding, packaging barcodes | W1 | beyond (Odoo uses one global precision [R10 s3.3]) |
| 3 | Combo products | pick one item per combo; parent and child lines | W1 | matches Community |
| 4 | Typed custom properties | JSONB values, definitions on a container | W1 | matches Community |
| 5 | Pricelist rules | fixed, discount, markup; list, cost or other-list base | W1 | matches Community |
| 6 | Explain trace and price simulator | every price reproducible with reasons | W1 | beyond |
| 7 | Ambiguity lint | save-time warning for tied or shadowed rules | W1 | beyond |
| 8 | Negotiated prices per party | pricelist on the customer profile | W1 | matches Community |
| 9 | Order discount and promotions | discount lines; loyalty via retail doc | W2 and W3 | matches Community |
| 10 | Lead and opportunity in one table | type flag, per-team stages, rotting flag | W1 | matches Community |
| 11 | Email intake and activities | alias to lead, chatter | W1 | matches Community |
| 12 | Duplicate detection and merge | email match, merge 2 to 5 records | W1 | matches Community |
| 13 | Forecast | expected revenue times probability by month; recurring revenue | W1 | matches Community |
| 14 | Predictive scoring | naive Bayes per team with explanation | W2 | matches Community |
| 15 | Capacity-based assignment | 30-day quota, domains, opt-out | W2 | matches Community |
| 16 | Lead mining and enrichment | provider connectors, user keys, no credit wall | W3 | deliberately skipped (Odoo's IAP service) |
| 17 | Quotation lifecycle | templates, sections, notes, optional lines | W1 | matches Community |
| 18 | Online acceptance and prepayment | signature evidence, percent to confirm | W1 | matches Community |
| 19 | Down payments | percent or fixed; deducted on the final invoice | W1 | matches Community |
| 20 | Invoice policies | ordered or delivered quantity, partial invoicing | W1 | matches Community |
| 21 | Service policies | prepaid, manual, timesheet, milestone | W2 | matches Community |
| 22 | Margin visibility | cost and margin behind field-level permission | W1 | matches Community, plus field security |
| 23 | Upsell flag | delivered above ordered on ordered-policy lines | W1 | matches Community |
| 24 | Quote revisions and alternatives | revision chain, alternative sets | W2 | alternatives match Community [R11 COULD 21]; revision chain beyond (Proposed) |
| 25 | Approval rules | discount and amount thresholds | W2 | matches Enterprise |
| 26 | PDF quote with embedded documents | Typst render (Proposed, D20) plus attached PDFs | W2 | matches Community |
| 27 | Subscription aggregate and plans | separate from the sales order | W3 | matches Enterprise; beyond in model |
| 28 | Proration, anchors, trials | pure billing function with preview | W3 | matches Enterprise |
| 29 | Dunning | retry ladder, notices, terminal action, hold | W3 | beyond (Odoo only flags the contract) |
| 30 | MRR movement ledger | append-only; MRR, ARR, churn, NRR | W3 | matches Enterprise reports; beyond as ledger |
| 31 | Usage-based billing | metered charges | later | beyond |
| 32 | Rental reservations and tiers | period lines, overlap guard, deposits | W4 or community | matches Enterprise |
| 33 | Rental late fees | explicit documented rule | W4 or community | beyond |
| 34 | Commission plans | targets, levels, metrics, roll-ups | W4 or community | matches Enterprise (Enterprise-only in Odoo 20, section 7) [R11 s10] |
| 35 | Clawback and period freeze | frozen results, adjustment lines | W4 or community | beyond |
| 36 | Portal access | capability links and portal accounts | W1 | matches Community |
| 37 | Subscription self-service | close, upsell, switch plan, payment method | W3 | matches Enterprise |
| 38 | API, webhooks and MCP for every entity | free on every plan | W1 | beyond (Odoo Online gates its API to the Custom plan [R21 s2.1]) |
| 39 | Agent-safe commands | dry-run and plan id (W1); approval policy engine (W2) | W1 and W2 | beyond |

## 4. Per-app specification

Event, port and extension-point names: `SaleOrderConfirmed`, `TransferDone`, `InvoicePosted`, `PaymentReconciled`, `DemandPort`, `QtyContributionPort`, `InvoiceableSource`, `PostingPort` and `PricingPort` come from [R23 D.2-D.4], as does `BeforeInvoicePost`, an extension point that `invoicing` runs and not an event. Credit notes reach sales as `InvoiceReversed`, the name used by [finance](finance.md) (Proposed); the credit note document is itself a posted invoice and emits `InvoicePosted`. The name [R11 MUST 6] used for this fact is retired (see [CONVENTIONS](../CONVENTIONS.md) section 8). Every other event name below is registered in [CONVENTIONS](../CONVENTIONS.md) section 3.4 (adopted there, Proposed until the generated event catalogue replaces that table); `PaymentTransactionDone` and `PaymentTransactionFailed` are emitted by `payment` as listed in [finance](finance.md) 4.5 and 4.15, and `PartyErased` is emitted by [security-privacy](security-privacy.md) 4.6.

### 4.1 Product catalog (`product`)

**Purpose.** One definition of what the business sells and buys, shared by Sales, Purchase, Stock, POS and the shop.

**Capabilities.** MVP (W1): rows 1-4 of section 3, plus categories (tree), archive instead of delete, images, optional-product and upsell links. Later: variant grid entry on quotes (W2, Proposed); kits through `mrp` (W3, phantom BoM in [supply-chain](supply-chain.md)).

| Entity | Purpose | Key fields | States |
|---|---|---|---|
| `product_template` | what is sold | kind (goods, service, combo), `is_storable`, category, uom, `list_price` NUMERIC(24,8), `invoice_policy` (order or delivery), `sale_ok`, `purchase_ok`, `properties`, translated name | active, archived |
| `product_variant` | SKU | template, sku, barcode, `combination_key`, `price_extra`; unique (template, key) where active | active, archived |
| `attribute`, `attribute_value` | option dimension | `create_mode` (always, dynamic, never), display type | none |
| `template_attribute_line`, `template_attribute_value` | values on a template | `price_extra`, excluded values | none |
| `product_profile` | per-company facts | cost, cost method, accounts, default taxes (with finance) | none |
| `uom`, `product_uom` | units, packagings | relative uom and factor, rounding; packaging barcode | none |
| `product_combo`, `product_combo_item` | bundle choices | combo product, item product, `extra_price` | none |
| `product_optional` | upsell links | template to template | none |

Rules. Create modes are immutable once the attribute is used on a template [R11 s3.1]; "always" is bounded by a guard (default 1,000 variants per template, mirroring Odoo's `product.dynamic_variant_limit` [R11 s3.1]; configurable). Combination validity is checked in the command, so API and agent calls cannot create an excluded combination. Cost lives in `product_profile`, not on the template [R23 B.1]. Service fields (policy, tracking, project and task templates) are added by `sale_project` in W2 ([services](services.md)). Quantities are stored in base UoM with entry quantity and UoM kept; conversion rounds half-up [R23 DR-5].

**Workflows.** (1) Define template and attributes. (2) System generates variants per mode; dynamic variants appear on first order (`VariantCreated`). (3) Quote-time configuration evaluates exclusions. (4) Archive hides from pickers; documents keep the reference.

**Flows.** Emits `ProductChanged`, `VariantCreated` (edge snapshots and shop feeds subscribe). Consumes nothing; it is foundation.

**Permissions and sensitive data.** `product.user` reads; `product.manager` writes. Cost fields are read through the field permission `product.cost_viewer` and carry no data class, as in [supply-chain](supply-chain.md) (section 9, question 17 is closed); the class `financial` stays for pay data, bank data and customer payment token references ([CONVENTIONS](../CONVENTIONS.md) section 5). Unreadable fields cannot be filtered, sorted or grouped (D10).

**Reports.** Catalog completeness (missing category, tax or cost), variant counts, products without price rule.

**Configuration.** Variant guard, default UoM, barcode nomenclature (owned by stock).

**Localization and compliance.** Names and descriptions are per-language values (D34). Base-unit price display and prior-price indication for consumers are storefront duties; the consumer-law baseline is in [R28 E.7] and needs counsel.

**Open vs paid.** Open.

### 4.2 Pricing (`pricing`)

**Purpose.** Compute the price of a product for a context, identically in quote, cart, POS, subscription, rental and API, and explain it. Pricing bugs are the most expensive class of ERP defect [R11 s4.3].

**Capabilities.** MVP (W1): rows 5-8. Later: order-level discount lines (W2), promotion hooks (W3, retail doc), subscription plan prices and rental duration tiers reuse the same rule table (W3, W4).

**Engine contract.** `PricingPort::price(ctx) -> PriceResult` is a pure function: no I/O, no clock, no database. The caller passes a rules snapshot, variant, quantity, UoM, `as_of`, currency and an FX snapshot. The crate is pure Rust with no SQL, compiled natively and to Wasm so POS, cart and server agree on totals [R23 DR-7, R12 s3.2]. Resolution:

1. Pricelist: explicit on the document, else the party profile, else the default list, which in Odoo is the first active list without a country group, by sequence [R11 s4.1]; preferring a country-group list that matches the party's country and currency is Proposed.
2. Candidates: active rules whose validity contains `as_of`, with `min_quantity` met after UoM conversion, applicable to the variant, product, category chain or globally.
3. Order (total, deterministic): specificity (variant, product, category by depth, global); larger `min_quantity`; explicit `priority` (lower wins, Proposed); newer `created_at`; rule id. First wins [R11 s4.1].
4. Base: list price plus variant `price_extra`, cost, or another pricelist. Cycles are rejected at save; depth is limited to 8 (Proposed).
5. Formula: `base - base*d/100` (markup is a negative discount), round to a multiple of `price_round` half-up, add surcharge, clamp to `[base+min_margin, base+max_margin]` [R11 s4.1].
6. Currency: convert at the stored rate (NUMERIC(28,14)) for `as_of`; the rate is recorded, never re-derived [R23 DR-5].
7. Output: unit price NUMERIC(24,8), `discount_pct` when the rule is a plain discount (the customer sees a struck-through list price, not a silently lowered price [R11 s4.2]), rule id, pricelist id, trace.

The trace is an ordered list of steps with inputs, result and reason, including every rejected candidate (`expired`, `not_yet_valid`, `min_qty`, `wrong_uom`, `not_applicable`, `shadowed_by:<rule>`). Lines always store the rule id; a full trace snapshot is stored when a quote is sent and when an order is confirmed (Proposed). Rules used by a document are versioned, not edited in place, so any past `as_of` is reproducible (Proposed).

| Entity | Purpose | Key fields |
|---|---|---|
| `pricelist` | named list | currency, company, country groups, sequence, active |
| `pricelist_rule` | one rule | applied-on, target, `min_quantity`, UoM, `valid_from/to`, compute (fixed, discount, markup), base, amounts, `price_round`, surcharge, margins, `priority` |
| `price_evaluation` | audit snapshot | document and line ref, context hash, result, trace JSON |

**Workflows.** (1) Author a rule. (2) Lint it for ties, unreachable rules and cycles. (3) Publish with an effective date. (4) Test in the simulator (`POST /pricing/explain`, also an MCP tool). (5) Documents use it through `PricingPort`.

**Flows.** Emits `PriceRuleChanged` on any pricelist or rule change (durable; shop cache purge and POS snapshot deltas subscribe, see [retail-commerce-marketing](retail-commerce-marketing.md)). Consumes `FxRatesUpdated` (Proposed; `base` emits it through `ExchangeRateSource`, [finance](finance.md) 4.7). Called through `PricingPort` by sales, subscriptions, rental, shop and POS.

**Permissions and sensitive data.** `pricing.manager` edits. A rule with cost as base reveals cost through price, so editing it needs `product.cost_viewer`. Party-specific prices are confidential: portal users see only their own result.

**Reports.** Manual-override rate, discount depth by salesperson, margin leakage, rules never used.

**Configuration.** Company rounding profile (`per_line` or `per_group`, D9), default pricelist, tax display mode per document (tax excluded or included, as Odoo 20's `document_tax_mode` [R11 s4.2]).

**Localization and compliance.** Currency exponents are data (JPY 0, KWD 3) [R23 DR-5]. Tax-included B2C display per pricelist. Each tax computation stores the rule-pack version and inputs (D38).

**Open vs paid.** Open.

### 4.3 CRM (`crm`)

**Purpose.** Track prospects to won or lost, forecast revenue, and turn an opportunity into a quotation on the same database and party [R11 s5.4].

**Capabilities.** MVP (W1): rows 10-13, teams, per-team stages with `is_won`, `fold` and `rot_days`, priority, tags, lost reasons, UTM attribution, team-defined properties, quotation creation from an opportunity. W2: rows 14-15. W3: connector-based enrichment (row 16) and AI assistance ([ai](ai.md)).

| Entity | Purpose | Key fields | States |
|---|---|---|---|
| `lead` | lead or opportunity | `type`, party plus contact snapshot, team, salesperson, stage, priority, tags, `expected_revenue`, `probability`, `probability_auto`, `recurring_revenue`, recurring plan, `expected_close`, lost reason, `properties`, UTM | open, won, lost (derived) |
| `stage` | pipeline step | teams, sequence, `is_won`, `fold`, `rot_days` | none |
| `team`, `team_member` | sales teams | leader, alias, `use_leads`, `auto_win_on_order`, member capacity (30 days), domain, opt-out | none |
| `recurring_plan` | months for recurring revenue | months | none |
| `lead_scoring_frequency` | scoring table | team, variable, value, won count, lost count | none |

```mermaid
stateDiagram-v2
    [*] --> Lead
    Lead --> Opportunity: convert
    Opportunity --> Opportunity: stage change
    Opportunity --> Won: won stage, probability 100
    Opportunity --> Lost: archive with reason, probability 0
    Lost --> Opportunity: restore
```

Invariants: probability is within [0,100]; a won stage forces 100 (database CHECK) and manual entry of 100 is refused; lost requires a reason [R11 s5.1].

**Workflows.**

1. Capture by form, API, import or email alias.
2. Dedupe by email; score (W2); assign (manual or round-robin in W1, capacity rules in W2).
3. Convert: match or create the party.
4. Work the pipeline, with a "Move to" action as the non-drag alternative (WCAG 2.5.7).
5. `CreateQuote` from the opportunity.
6. Win on `SaleOrderConfirmed` when `auto_win_on_order` is set (default on, Proposed), otherwise manually; lose with a reason.
7. Review the forecast by expected month.

**Scoring (W2).** Per-team naive Bayes. Stage and team are always variables; optional ones are state, country, phone quality, email quality, source, language and tags. Score is win/(win+lost) over per-variable likelihoods with 0.1 added to every frequency; frequencies are incremented when a lead changes state; a start date bounds training; a manual override holds until reset [R11 s5.2]. The UI shows the top contributing variables. No ML runtime is needed; the result is reproducible from the frequency table.

**Flows.**

| Direction | Name | Effect |
|---|---|---|
| emits | `LeadCreated`, `LeadConverted`, `OpportunityStageChanged`, `OpportunityWon`, `OpportunityLost`, `LeadsMerged` | webhooks, timeline, analytics |
| consumes | `SaleOrderConfirmed` | raise expected revenue to the order's untaxed total when lower and currencies match, never lower it [R11 s5.3]; mark won per [R23 D.4] (Odoo 20's `sale_crm` only raises revenue and does not mark won, checked in the 20.0 source, so auto-win is an obop addition, Proposed) |
| consumes | `QuoteSent`, `QuoteViewed`, `QuoteAccepted`, `QuoteDeclined` | activity and timeline entries |
| consumes | `FormSubmitted` (W3) | capture a lead from a website form (workflow step 1), with the same dedupe and assignment; emitted by `website` ([retail-commerce-marketing](retail-commerce-marketing.md) 4.4) |
| consumes | `PartyMerged`, `PartyErased` | relink; anonymise the contact snapshot |

`crm` subscribes to `sales-api` events directly; no bridge is needed [R23 D.4].

**Permissions and sensitive data.** Salespeople default to `own`, team leaders to the team domain. Email, phone and address are personal data with registry flags that drive export and erasure (D33) [R18 s8.1].

**Reports.** Pipeline by stage, team and source; forecast by expected month; prorated revenue (expected times probability, rounded to the minor unit [R11 s5.3]); pipeline MRR; conversion and win rate; days in stage; rotting deals; lost reasons. All are registry-declared measures on the semantic layer, with drill-through.

**Configuration.** `use_leads` per team, stages, lost reasons, scoring variables and start date, assignment cron, alias, `rot_days`, `auto_win_on_order`.

**Localization and compliance.** Record purpose and legal basis per lead; blacklist and opt-out are shared with `mail`; lost leads can be anonymised after a configurable retention period (Proposed); erasure is anonymise-in-place [R18 s8.1]. Enrichment and mining sit behind provider traits with user-supplied keys and are off by default; transfers to such providers need counsel.

**Open vs paid.** Open. Hosted AI credits are an optional paid convenience (D18).

### 4.4 Sales (`sales`)

**Purpose.** The commercial document from quotation to confirmed order, and the join point of fulfilment and billing.

**Capabilities.** MVP (W1): rows 17-20, 22-23, plus addresses, payment terms, fiscal position, currency with stored rate, PDF by Typst (Proposed, D20), portal view, acceptance and payment, lock, chatter, API. W2: rows 21, 24-26, consolidated billing across orders. W3: subscriptions from order lines, loyalty on orders, shop orders.

| Entity | Purpose | Key fields |
|---|---|---|
| `sale_order` | header | company, `order_kind` (quote, cart), `number`, `state`, `locked`, derived statuses, party, invoice and shipping address, currency and rate, pricelist, payment term, fiscal position, tax display mode, rounding profile, `validity_date`, team, salesperson, opportunity, template, terms version, `require_signature`, `prepayment_percent`, totals (bigint minor), `row_version` |
| `sale_order_line` | line tree | `display_type` (product, section, subsection, note), `parent_line_id`, `is_optional`, `is_downpayment`, `combo_item_id`, variant, qty, uom, `price_unit`, `price_source`, `discount_pct`, rule id, taxes, cost snapshot, derived `qty_delivered`, `qty_invoiced`, `qty_to_invoice`, analytic distribution |
| `sale_order_template` | reusable quote | lines, validity days, signature and prepayment defaults, terms |
| `quote_acceptance` | evidence | order, document hash, signer, signature image, method, time, IP, user agent, OTP flag |
| `sale_line_qty_contribution` | delivered quantity | line, source ref, qty; unique (line, source) [R23 B.2] |
| `invoice_line_source` | billing link | invoice line, sale line, qty, kind; makes `qty_invoiced` recomputable |

Optional products are ordinary lines under a section flagged `is_optional`, the Odoo 19/20 design, with no side table [R11 s6.1]. Quantity of optional lines before the customer adds them is undecided (section 9).

Shop carts. The shop models a cart as a draft `sale_order` with `order_kind = cart` ([retail-commerce-marketing](retail-commerce-marketing.md), itself Proposed). This spec accepts it on three conditions (Proposed): a restriction row hides carts from quote lists and reports; carts never get a `number` or a `quote_status`; confirmation uses the same `ConfirmSaleOrder` command.

**State.** Kernel lifecycle `draft -> confirmed -> cancelled` plus a `locked` flag (D28, [R23 B.2]). Derived, never user-written: `quote_status` (draft, sent, viewed, accepted, declined, expired), `delivery_status` (pending, started, partial, full), `invoice_status` (`nothing_to_invoice`, `to_invoice`, `upselling`, `invoiced`), `payment_state` (`unpaid`, `partial`, `paid`). `number` is assigned (Proposed) when the quote is first sent or confirmed, from a sequence; this deliberately departs from the header pattern, where the number stays NULL until confirmation [R23 B.2], so that a sent quote can be referenced. Gaps are allowed because legal gapless numbering belongs to invoices (D30).

| Transition | Guard |
|---|---|
| draft to confirmed | lines present; not expired unless overridden; signature or prepayment satisfied when required, else explicit human confirm; approval rule passed (W2) |
| confirmed line edit | may add or raise quantity; may not lower below delivered or invoiced; invoiced lines keep `price_unit`; all audited |
| to cancelled | no posted invoice; open deliveries cancelled through `SaleOrderCancelled` |

Upsell: a line on the ordered policy whose delivered quantity exceeds the ordered quantity sets `invoice_status` to `upselling` [R11 s6.2]. The sales team is the same `team` entity as in CRM (and the shop) [R11 s10].

**Workflows.**

1. Create from an opportunity, template or copy.
2. Lines get price and rule from `PricingPort`; a manual override sets `price_source = manual`, needs `sales.price_override` and respects a per-group discount limit (approval rules in W2).
3. Send: Typst PDF plus capability link (`QuoteSent`).
4. Customer views (`QuoteViewed`), edits optional lines, signs, pays the prepayment.
5. Confirm: `ConfirmSaleOrder`, one transaction.
6. Fulfil and invoice (next flow).
7. Lock.

**Order-to-cash automatic flow.** All steps follow the integration contract (D27): one command, one transaction; handlers in the same transaction for atomic invariants; durable events for external I/O.

```mermaid
sequenceDiagram
    participant C as Customer portal
    participant S as sales
    participant PR as pricing
    participant PAY as payment
    participant ST as stock via sale_stock
    participant INV as invoicing and account
    participant CRM as crm
    S->>PR: PricingPort::price (pure)
    S->>C: QuoteSent with link
    C->>S: AcceptQuote bound to document hash
    C->>PAY: pay prepayment
    PAY->>S: PaymentTransactionDone
    S->>S: ConfirmSaleOrder emits SaleOrderConfirmed
    S->>ST: DemandPort::raise
    S->>CRM: SaleOrderConfirmed
    ST->>S: TransferDone via QtyContributionPort
    S->>INV: InvoiceableSource
    INV->>S: InvoicePosted
    INV->>S: PaymentReconciled
```

| Step | Command (transaction) | Same-transaction handlers (TxEvent) and extension points | Durable events |
|---|---|---|---|
| Send | `SendQuote` | none | `QuoteSent`: email with link, webhook |
| Accept | `AcceptQuote` (portal) | rejects if the document hash changed | `QuoteAccepted` |
| Prepay | PSP webhook recorded by `payment` | none | `PaymentTransactionDone`: the `sale_payment` bridge (Proposed) confirms the order when the amount is at least `prepayment_percent` and optionally raises the down-payment invoice; the handler is idempotent |
| Confirm | `ConfirmSaleOrder` | `sale_stock` calls `DemandPort::raise`; `crm` updates the opportunity; `sale_project` creates tasks (W2); `loyalty` earns points (W3) | `SaleOrderConfirmed`: webhook, email, analytics, AI index |
| Deliver | `ValidateTransfer` (stock) | `sale_stock` calls `QtyContributionPort::set(('transfer', id), line, qty)`; `stock_account` posts per company policy | `TransferDone` |
| Invoice | `CreateInvoice` from `InvoiceableSource`, then `PostInvoice` | extension point `BeforeInvoicePost` (invoicing runs it): `stock_account` adds COGS lines; `InvoicePosted`: `sales` updates `qty_invoiced` | `InvoicePosted`: e-invoice transmission, email |
| Collect | `RegisterPayment`, statement line or PSP webhook | none | `PaymentReconciled`, `PaymentVoided`: `sales` updates the derived `payment_state` (no atomic invariant) |
| Correct | credit note (a posted invoice; emits `InvoicePosted` and `InvoiceReversed`) or return transfer | `InvoiceReversed`, `TransferDone` adjust quantities through the same ports | mirrors |

Also consumed by `sales` ([CONVENTIONS](../CONVENTIONS.md) section 3.4): `QuoteViewed`, `QuoteAccepted` and `QuoteDeclined`, emitted by `portal` (4.8), which drive the derived `quote_status`; and, from W3, `ChannelOrderImported` and `ChannelSyncFailed` from the channel framework ([retail-commerce-marketing](retail-commerce-marketing.md) 4.3), where an import creates one `sale_order` carrying the channel reference and a sync failure is logged on that order's chatter (Proposed).

Inventory side: `DemandPort::raise` creates moves and reservations per route and warehouse policy; a validated transfer is immutable and corrected only by new moves ([supply-chain](supply-chain.md), D30). Accounting side: `PostInvoice` takes lock dates, assigns the gapless legal number late in the transaction, and the deferred balanced-entry trigger and hash chain run at commit (D30); valuation entries follow the company posting policy (perpetual at invoicing, per transfer, or periodic) [R10 s6.4]. A locked period rolls back the whole posting transaction and the user sees the error; the coupling is intended [R23 D.4].

Invoice policy is per product: ordered or delivered quantity; service policies arrive in W2 [R14 s4.2]. Down payments are percent or fixed; the invoice is posted on a `down_payment` account role (Proposed), the line is flagged `is_downpayment`, and the final invoice adds a "Down Payments" section with negative lines, becoming a credit note if the balance is negative [R11 s6.3]. Prepayment VAT timing is a country-pack matter (section 9).

**Free versus paid along the flow.** Every step above is open: pricing, approvals, portal, PSP connectors, reservation, valuation, posting. Paid touchpoints are only (a) hosted transmission of the invoice through Peppol, KSeF or Veri*Factu gateways (W1 uses a certified partner access point, Proposed D17), (b) the Assurance service for the country pack that governs tax, numbering and e-invoice formats at the Invoice step, (c) hosting and support, and (d) hosted AI credits if AI drafting is used. Nothing is gated by user count.

**Permissions and sensitive data.** Permission groups: `sales.user`, `sales.team_lead`, `sales.manager`, `sales.invoicing`. Named actions, granted by rows of those groups and not groups themselves: `sales.price_override`, `sales.unlock_order`. Cost and margin are read through the field permission `product.cost_viewer`, not a data class (section 9, question 17, closed). Signature evidence (IP, user agent) is personal data retained with the contract. Payment transactions hold customer payment token references only (class `financial`); card data never enters obop [R18 s8.2, R12 s4.3]. Agents may create and edit drafts; confirming an order defaults to a human-approved policy (`confirm` in an interactive session, `approve_by_role` as a durable approval item otherwise), never `auto` (D18, [R16 s5]); Proposed.

**Agent and API surface.** Business commands rather than CRUD sequences: `sale_order.create_quote`, `.send`, `.confirm`, `.cancel`, `lead.convert`, `pricing.explain`. Each takes `Idempotency-Key` (W1); `dry_run` with a digest-bound `plan_id` is W1 ([ai](ai.md) 3; D18, D31); only the approval policy engine is a W2 deliverable (D18; row 39).

**Reports.** Quoted, ordered, delivered and invoiced quantity and amount by product, team, salesperson, customer and country; quote conversion and time to close; expected margin (before invoice) and realised margin (after COGS); backlog; to-invoice; upsell candidates; expiring quotes; discount analysis.

**Configuration.** Validity days, lock on confirm, default signature and prepayment, auto-invoice on payment, down-payment invoice on prepayment, discount limits per group, default invoice policy, tax display mode, terms versions.

**Localization and compliance.** Documents render in the customer's language. Terms are versioned and the accepted version hash is stored. A consumer flag on the order drives the withdrawal function (D38, [R28 E.7]). Signatures are simple electronic signatures with an evidence bundle, labelled honestly; qualified signatures need a QTSP connector in `sign` (W3) (D39, [R15 s8]). Invoice numbering and EN 16931 output are in [finance](finance.md).

**Open vs paid.** Open.

### 4.5 Subscriptions (`subscriptions`)

**Purpose.** Recurring billing with a first-class subscription aggregate. Odoo models a subscription as a confirmed sales order with a plan, which makes upsell, renewal and state reporting multi-document [R11 s8.3].

**Capabilities.** MVP (W3): rows 27-30 and 37. Later: usage-based billing (row 31) with Kill Bill's consumable and capacity distinction as the minimal design [R11 COULD 20]; goods delivered with each invoice through a `subscription_stock` bridge (Proposed).

| Entity | Purpose | Key fields |
|---|---|---|
| `plan` | billing shape | interval unit and count, trial days, anchor rule, in advance or arrears, proration policy, renewal notice days, auto-close days, self-service flags |
| `plan_price` | price per plan | plan, product or variant, pricelist, currency, amount, min quantity |
| `subscription` | aggregate | party, status, plan, start, anchor day, current period, `next_invoice_at`, `cancel_at`, `pause_until`, collection method (automatic or manual), payment token ref, source order |
| `subscription_item` | what is billed | product, qty, price snapshot |
| `subscription_event` | append-only history | kind, reason, MRR delta, actor |
| `mrr_movement` | ledger | subscription, kind, delta (subscription currency and company currency at the movement date), effective date |
| `dunning_policy`, `dunning_run` | recovery | ordered steps; run state per invoice |
| `subscription_invoice_link` | one invoice per period | unique (subscription, period start) |

```mermaid
stateDiagram-v2
    [*] --> incomplete: first payment pending
    [*] --> trialing: trial starts
    incomplete --> active: first payment ok
    trialing --> active
    active --> past_due: invoice unpaid
    past_due --> active: paid
    past_due --> unpaid: dunning exhausted
    past_due --> cancelled: dunning exhausted
    unpaid --> active: reactivated
    active --> paused
    paused --> active
    active --> cancelled
```

Vocabulary follows Stripe (trialing, active, past_due, unpaid, paused, incomplete) [R11 MUST 7], except that the terminal state is `cancelled`, the kernel spelling (D28; [CONVENTIONS](../CONVENTIONS.md) section 6). "Scheduled" and "to renew" are derived labels, not states.

**Billing engine.** `bill(subscription_state, catalog_version, as_of) -> invoice lines` is pure, and `preview` uses the same code [R11 s8.3]. Periods use civil dates in the business time zone (`jiff` 0.2.37 as of 2026-10-01 [R00 s3]); an anchor day past the end of a short month clamps to its last day. Proration (Proposed defaults): by calendar days of the actual period; applies to recurring service charges only, never to goods [R11 s8.1]; a change credits the amount actually billed for the unused time (Stripe's "flexible" rule [R11 s8.3]); each amount is rounded once, half-up. Upgrades apply immediately; downgrades at period end. Existing subscriptions keep their price until an explicit price-change action with notice. Tax is computed at each invoice from the fiscal position and rules of that date, so rate changes need no plan edit.

**Dunning (Proposed defaults).** A policy is an ordered list of steps: retry, send notice, create activity, terminal action (cancel, mark unpaid, leave past due, pause). Default: eight attempts in 14 days (day 0, then +1, +2, +3, +5, +7, +9, +11), a notice after each failure, terminal action at +14, which mirrors Stripe's recommended shape [R11 s8.2] but is deterministic. Declines are classed soft or hard (and authentication-required); hard declines stop retrying until the customer updates the payment method. A manual hold stops automation. Entitlement is separate from billing state, with configurable grace days [R11 s8.2]. Retries carry PSP idempotency keys derived from (invoice, attempt) [R21 s5]. The step-list policy engine is shared with invoice reminders in [finance](finance.md).

**MRR ledger.** Append-only movements: new, expansion, contraction, churn, reactivation. MRR is the plan amount normalised to a month by a documented rational factor (year /12, quarter /3, week x52/12, day x365/12; Proposed), not the prorated amount; trials add no MRR until the first paid period (Proposed). History is never re-derived from current rates. Metrics: MRR, ARR, net new MRR by kind, active count, trial conversion, logo and revenue churn, NRR and GRR, ARPA, cohorts (cohort view W3, D25), recovery rate of dunning.

**Workflows.**

1. Sell: confirmed order lines with a plan create the subscription on `SaleOrderConfirmed`.
2. Bill: a durable job claims due subscriptions (`SKIP LOCKED`), calls `bill`, creates the invoice through `InvoiceableSource`, posts it and collects by token, portal payment or SEPA.
3. Failure enters dunning.
4. Change, pause, renew, or cancel at period end or immediately. Every change writes an event and an MRR movement in the same transaction.

**Flows.** Emits `SubscriptionStarted`, `SubscriptionInvoiceDue`, `SubscriptionChanged`, `SubscriptionStatusChanged`, `DunningStepExecuted`, `MrrMovementRecorded`, `SubscriptionCancelled`. Consumes `SaleOrderConfirmed`, `InvoicePosted`, `PaymentReconciled`, `PaymentTransactionDone`, `PaymentTransactionFailed` (both Proposed), `PartyErased`.

**Permissions and sensitive data.** `subscriptions.user`, `.manager`; portal self-service is governed by plan flags. Customer payment token references (PSP references) are class `financial` (token references only), encrypted; card numbers are never stored [R18 s8.2]. PSP credentials are a different thing: class `secret` (Proposed, [security-privacy](security-privacy.md) 4.1), vault entries handled as `financial` until it is ratified.

**Reports.** The metrics above plus upcoming renewals, failed-payment queue, MRR by plan, salesperson and country.

**Configuration.** Plans, dunning policies, close reasons, grace days, notice templates, proration and rounding defaults.

**Localization and compliance.** Consumer subscriptions sold online show the withdrawal function in the portal (D38, [R28 E.7]); price-change notice periods and SEPA mandates are country matters for counsel; every period's invoice follows the e-invoicing rules of its pack.

**Open vs paid.** Open; Odoo keeps Subscriptions in Enterprise. Gateways and Assurance as in 4.4.

### 4.6 Rental (`rental`)

**Purpose.** Sell time-bound hire of serial-tracked or quantity-tracked goods and services. T3, specified so the pricing and order models do not block it.

**Capabilities.** MVP (first release): row 32, meaning period-aware order lines, availability check at quote (soft) and confirm (hard), duration pricing, deposits, pickup and return, calendar view. Later: row 33 (late fees), portal extension requests, maintenance blocks per asset.

| Entity | Purpose | Key fields |
|---|---|---|
| rental period on `sale_order_line` | the hire window | `rental_start`, `rental_end` (tstzrange) |
| `rental_reservation` | blocks a unit | line, variant, unit (serial or lot) or qty, blocked range (period plus padding), status |
| `deposit_rule` | security deposit | paired service product, amount or percent |
| `late_fee_rule` | overdue charge | grace period, rate per period, cap |

Reservation states: `reserved -> picked_up -> returned`; `reserved -> cancelled`; `overdue` is derived when now passes `rental_end`.

Rules. Serial-tracked units are protected by `EXCLUDE USING gist (unit WITH =, blocked_range WITH &&)` over active reservations, which works on PostgreSQL 17 (D8); `btree_gist` supports uuid [R23 DR-3]. Quantity-tracked stock sums overlapping quantity under a row lock [R11 s9]. Pricing reuses the engine with quantity = line qty x duration in the periodicity unit, and the default applies one tier to the whole duration, the one with the lowest total, as Odoo documents (only one price line is used and the cheapest wins; see section 5.4) [R11 s9]. Padding time applies to hourly rentals only. Late fees are an explicit rule because Odoo's docs show none (Proposed).

**Workflows.**

1. Quote with a soft availability check.
2. Confirm (`SaleOrderConfirmed`): hard reservation.
3. Pickup wizard scans serials and raises a transfer through a `rental_stock` bridge (Proposed).
4. Return wizard receives stock.
5. Fees are invoiced and the deposit is refunded by credit note. Service rentals (rooms) skip stock.

**Flows.** Emits `RentalReserved`, `RentalPickedUp`, `RentalReturned`, `RentalOverdue`. Consumes `SaleOrderConfirmed`, `TransferDone`.

**Permissions and sensitive data.** `rental.user`, `rental.manager`. Deposits are money held and post to a liability account role via `PostingPort`.

**Reports.** Utilisation, revenue per asset, overdue returns, availability calendar.

**Configuration.** Periodicity per product, padding time, pickup and return times, duration tiers, deposit and late-fee rules.

**Localization and compliance.** Deposit and damage-charge VAT follow the country pack; insurance is out of scope.

**Open vs paid.** Open; Odoo keeps Rental in Enterprise.

### 4.7 Commissions (`commissions`)

**Purpose.** Compute what salespeople and teams earn from orders, invoices or payments.

**Capabilities.** MVP (first release): row 34. Plans based on targets or achievements, per salesperson or team, monthly, quarterly or yearly; target plans have periods, target values, on-target commission and levels from 0% to 300% of target; metrics are amount sold, amount invoiced, quantity sold, quantity invoiced, margin and MRR when `subscriptions` is installed, filterable by product or category [R11 s10]. Later (Proposed): row 35, a selectable basis event (order confirmed, invoice posted or payment reconciled) and manager roll-ups.

| Entity | Purpose | Key fields |
|---|---|---|
| `commission_plan` | rules | basis, scope, frequency, levels, metric, filters, basis event |
| `commission_assignment` | who | plan, salesperson or team, valid range |
| `commission_period_result` | frozen result | assignment, period, metric values, amount, status (open, closed, paid) |
| `commission_adjustment` | manual or clawback line | result, amount, reason, source ref |

**Workflows.**

1. An incremental job accumulates metrics from `SaleOrderConfirmed`, `InvoicePosted`, `PaymentReconciled` and `InvoiceReversed`.
2. On close the result is frozen; only adjustment lines change it afterwards.
3. Clawback (Proposed): a credit note before close reduces the metric; after close it creates a negative adjustment in the next open period.
4. Approved statements go to an export for [people](people.md) payroll, or to a draft vendor bill for external agents. No payroll logic lives here (D19).

**Flows.** Emits `CommissionPeriodClosed`. Consumes the events above.

**Permissions and sensitive data.** Period results, payouts and adjustments are pay data tied to a person; store them in the `financial` class behind the `obop_cls_*` database roles (D10; the class assignment is Proposed). The payroll preparer reads `financial`, and approved statements hand off to payroll. Salespeople read only their own (`commissions.self`).

**Reports.** Attainment by plan, payout by period, open adjustments, clawbacks.

**Configuration.** Plans, levels, metrics, basis events, close day.

**Localization and compliance.** Employment-law treatment of variable pay is for counsel.

**Open vs paid.** Open; Enterprise-only in Odoo 20 (section 7).

### 4.8 Customer portal (`portal`)

**Purpose.** A token-scoped read-and-act surface for customers, not a special user type [R11 s11].

**Capabilities.** MVP (W1): view, download, accept or decline a quote, edit optional lines, pay (prepayment percent or full), comment to the salesperson, follow delivery status, see invoices and payments. W3: subscription self-service per plan flags (close, add products, switch plan, renew, update payment method). Later: rental extension requests.

| Entity | Purpose | Key fields |
|---|---|---|
| `share_grant` | capability link | token hash, record ref, scope (view, accept, pay), expiry, revoked [R23 B.1] |
| `principal` (kind portal) | portal account | linked party, status, credentials |

Two access modes. A capability link opens one document. A portal account shows every document of the customer's commercial party. Both run through the single query path with default-deny; a wrong id returns 404, not 403 (D10, [R18 s10]). Link states: active, expired, revoked (plus "used" for single-use grants, see [platform](platform.md)).

**Workflows.**

1. Sales sends a quote; the link is stored hashed and expires (Proposed), and a resend issues a new link.
2. On first visit the token is exchanged for a session cookie and the browser is redirected to a clean URL, with `Referrer-Policy: no-referrer` and no indexing (Proposed).
3. The customer reviews, edits optional lines, comments, declines or accepts.
4. The customer pays the prepayment or the invoice.
5. Later, a portal account (created on request) shows orders, invoices, deliveries and, from W3, subscriptions.
6. Cancelling the document revokes its links.

**Acceptance.** `AcceptQuote` stores a `quote_acceptance` row bound to a SHA-256 hash of the canonical document; if the quote changed since the customer loaded it, the call fails with 409 and the customer sees the new version. Evidence: signer name, typed or drawn signature, server time, IP, user agent, optional OTP. The PDF is sealed with an obop-held certificate, which seals the document, not the person; obop never labels this advanced or qualified [R15 s8]. The W3 `sign` app reuses the primitive.

**Payments.** Hosted fields or redirect only; PAN stays out of scope; checkout pages carry a locked script allow-list and CSP [R12 s4.3]. PSP API versions are pinned per connector (Stripe `2026-09-30.endive` as of 2026-10-01 [R09 s6]).

**Flows.** Emits `QuoteViewed`, `QuoteAccepted`, `QuoteDeclined`, `PortalPaymentStarted`. Consumes `QuoteSent`, `SaleOrderCancelled`, `InvoicePosted`.

**Permissions and sensitive data.** Portal principals see only their party's records and their own price results. IDOR tests are generated from the registry for every endpoint, role and foreign id (D36, [R27 A2]).

**Reports.** Quote views, time to accept, portal payment success, abandoned acceptances.

**Configuration.** Branding, custom domain (open), link lifetime, required OTP, consumer withdrawal text.

**Localization and compliance.** Portal pages render in the party's language. Default portal is WCAG 2.2 AA and ships the consumer withdrawal function (D35, D38). Rendering follows the storefront decision: Rust-rendered blocks plus Vite islands, no Node in production (Proposed, D14). Only strictly necessary cookies are set (Proposed).

**Open vs paid.** Open, including branding and custom domain; partner white-label is paid (Proposed, D2).

## 5. Cross-app golden paths

Fixture tax T20 is 20% and is not a statutory rate. These scenarios are acceptance tests.

### 5.1 Lead to cash with prepayment

- **Given** product A "Desk lamp" (list 100.00, cost 40.00), optional product B "Lamp stand" (list 15.00, cost 6.00), pricelist "Trade" (A: discount 10% from quantity 10; no rule for B), prepayment 30%, perpetual-at-invoicing valuation, company flag "down-payment invoice on prepayment", stock of 12 A and 12 B, and lead "Acme GmbH" converted to an opportunity with expected revenue 1,000.00.
- **When** a quote for 12 A is created, **Then** unit price is 100.00 with 10% discount, line net 1,080.00, and the trace names the Trade rule.
- **When** the customer adds 12 B on the portal, signs, and pays 30%, **Then** net is 1,260.00, tax 252.00, total 1,512.00, prepayment 453.60; the order is confirmed, numbered, and `SaleOrderConfirmed` fires; demand for 12 A and 12 B is raised; the opportunity revenue becomes 1,260.00 and it is won; a down-payment invoice of 453.60 (378.00 net, 75.60 tax) is posted and reconciled to the payment.
- **When** the warehouse validates 12 A and 12 B, **Then** `qty_delivered` is 12 for both, `delivery_status` is `full`, `invoice_status` is `to_invoice`.
- **When** the final invoice is created and posted, **Then** it shows 1,080.00 and 180.00, a Down Payments section of -378.00, net 882.00, tax 176.40, total 1,058.40; COGS lines of 480.00 and 72.00 are added.
- **When** a bank line of 1,058.40 is reconciled, **Then** `payment_state` is `paid` and `invoice_status` is `invoiced`; realised margin is 708.00, 56.19% of net.

### 5.2 Price resolution and explanation

- **Given** list price 100.00 and rules P1 global -5%; P2 category Lighting -8%; P3 Desk lamp, from quantity 10, fixed 90.00, valid 2026-10-01 to 2027-03-31; P4 Desk lamp, from quantity 10, fixed 85.00, valid to 2026-06-30.
- **When** pricing 12 lamps as of 2027-01-15, **Then** the result is 90.00 from P3; the trace lists P4 as `expired` and P1, P2 as `shadowed_by:P3`; a second call returns the identical result and trace.
- **When** the same call uses `as_of` 2026-05-01, **Then** P4 wins with 85.00 and the trace lists P3 as `not_yet_valid`.
- **When** cost is 40.00 and a rule applies markup 25% on cost, **Then** price is 50.00. **When** base is 12.34, rounding is 1.00 and surcharge -0.01, **Then** price is 11.99.
- **When** a second rule identical to P3 in specificity, quantity and validity is saved without `priority`, **Then** the lint reports an ambiguous pair.

### 5.3 Subscription, proration, upgrade, dunning

- **Given** plan Pro 50.00 per month and plan Business 80.00 per month, in advance, anchored to the first of the month, default dunning policy.
- **When** a customer starts Pro on 2027-08-10, **Then** the first invoice is 22/31 x 50.00 = 35.48 (the same figure as Lago's documented example [R11 s8.2]), and the ledger has `new` +50.00.
- **When** the customer upgrades to Business on 2027-09-16, **Then** the credit is 50.00 x 15/30 = 25.00, the charge is 80.00 x 15/30 = 40.00, the net is 15.00, and the ledger has `expansion` +30.00.
- **Variant A, when** the renewal invoice of 80.00 (2027-10-01) fails softly on 10-01, 10-02 and 10-03 and succeeds on 10-04, **Then** status is `past_due` from 10-01 to 10-04, three notices were sent, MRR is unchanged and the status returns to `active`.
- **Variant B, when** all eight attempts fail, **Then** on 2027-10-15 status is `cancelled` with reason non-payment, the ledger has `churn` -80.00 (ledger sum 0), entitlement ends and the invoice stays open for finance.
- **Variant C, when** the first attempt is a hard decline and the customer updates the payment method on 10-03, **Then** retrying resumes immediately and succeeds.
- **When** the billing job runs twice for the same period, **Then** exactly one invoice exists.

### 5.4 Rental

- **Given** serial-tracked projector SN-001; tiers 1 day 100.00, 3 days 250.00, 1 week 500.00 (Odoo's documented example [R11 s9]); deposit product 300.00; late-fee rule one day at the 1-day tier.
- **When** an 8-day rental is priced, **Then** one tier applies to the whole period and the price is 3 x 250.00 = 750.00 (1-day tier 800.00, week tier 1,000.00 are dearer). Mixing tiers (one week plus one day, 600.00) is not done by default, matching Odoo's documented rule.
- **When** a second order books SN-001 inside the window, **Then** the exclusion constraint rejects it and the user sees the conflict.
- **When** the unit is returned one day late, **Then** a 100.00 late-fee line is invoiced and a credit note refunds the 300.00 deposit; total rental revenue is 850.00.

### 5.5 CRM forecast, won rule, merge

- **Given** opportunities O1 10,000.00 at 20%, O2 20,000.00 at 50%, O3 5,000.00 at 80%, all expected 2027-03-20, O2 with recurring revenue 1,200.00 over a 12-month plan.
- **Then** the March 2027 prorated forecast is 2,000.00 + 10,000.00 + 4,000.00 = 16,000.00 and O2's monthly recurring revenue is 100.00.
- **When** O3 is moved to the won stage, **Then** its probability is 100 and open prorated revenue falls to 12,000.00.
- **When** probability 100 is typed on an open-stage opportunity, **Then** the change is refused.
- **When** O1 is lost with reason "Price", **Then** it is archived, probability 0 and excluded from the forecast.
- **When** two leads share one email and are merged, **Then** the head keeps all activities and attachments of both and a merge log exists; merging six records is refused.

### 5.6 Portal safety and agent limits

- **Given** customers A and B with quotes Q-A and Q-B and a link for Q-A.
- **When** the Q-A link is used against Q-B, **Then** the response is 404.
- **When** sales edits Q-A after the customer loaded it and the customer accepts, **Then** the response is 409 and no order is confirmed.
- **When** Q-A is cancelled, **Then** its link stops working.
- **When** an agent principal calls `sale_order.confirm`, **Then** the order is not confirmed: the call returns a pending approval (a durable approval item under `approve_by_role`) and the order stays draft until a human with the right permission approves.

## 6. Non-functional requirements

| Area | Requirement |
|---|---|
| Money and quantity | No floats. Unit prices NUMERIC(24,8); quantities NUMERIC(24,6) in base UoM; document totals bigint minor units; FX NUMERIC(28,14); `rust_decimal` 1.43.0 as of 2026-10-01 (D9, [R00 s3]). Rounding profile is per company. |
| Order integrity | Totals equal line sums and equal postings at the same rounding profile. `qty_invoiced` and `qty_delivered` are recomputable from `invoice_line_source` and `sale_line_qty_contribution`; unique (line, source). Progress statuses are derived, never written. |
| Pricing | Deterministic and idempotent: same inputs, same output and trace; proptest invariants plus golden traces before the crate leaves alpha (D36, [R27 A2]). |
| Subscriptions | Unique (subscription, period start); the sum of MRR movements equals current MRR, checked by a job; dunning steps and PSP calls are idempotent. |
| Rental | No overlapping active reservations per unit, enforced by the database. |
| CRM | Won stage implies probability 100; merges conserve activities and attachments; the scoring table equals a full recomputation (checker job). |
| Concurrency | Optimistic `row_version` on documents (409 on conflict). Confirmation and payment handlers are idempotent on `command_id` and inbox dedupe (D27 R3). Portal acceptance binds to the document hash. Quantity contributions use set semantics on a unique key, so replays and duplicates are harmless. |
| Multi-company | `company_id NOT NULL` and composite foreign keys from W1 with one company; selector in W2; mirrored interco orders are durable events in W3 (D28, [R23 C.4]). |
| Audit | Append-only, hash-chained audit (D33) of price overrides, discounts beyond limit, confirmation of expired quotes, post-confirmation edits, pricelist edits, subscription events, dunning actions, commission adjustments and acceptance evidence. |
| Accessibility | Back office and portal meet WCAG 2.2 AA as a release gate (D35); every kanban drag has a non-drag path (2.5.7) [R19 s8]; front-end tests and axe checks run through `vp test`. |
| Offline | 1.0 targets Odoo-19 parity (Proposed, D13): read cache of visited quotes and leads. Queued lead and note creation is an allow-list candidate for W3+ (Proposed). POS price snapshots are in the retail doc. |
| Performance | Targets, unmeasured: price a 100-line quote in under 50 ms (target, Proposed); create an order with 3 to 10 SQL statements (estimate [R22 s3.2], against Odoo's pinned 34 for an empty order [R22 s3.1]); p95 under 150 ms for reads and 400 ms for posting on R22's 2 vCPU, 4 GB small-business profile (target [R22 s10]). No multiple over Odoo is claimed (D21). |
| Security | Default-deny with one query path; portal links hashed; PSP credentials in the vault (class `secret`, Proposed; handled as `financial` until ratified); no card data; SSRF-hardened egress for webhooks and enrichment (D33). |

## 7. Where obop deliberately differs from Odoo, and why

| Odoo | obop | Pain addressed [R04] |
|---|---|---|
| Subscriptions, Rental and approvals are Enterprise. Sales Commissions (`sale_commission`) is Enterprise-only in Odoo 20: the module is absent from the public Community tree and Community `sale` shows its setting only as an Enterprise upgrade toggle (`upgrade_boolean`), the same mechanism as the Amazon Connector; it is not listed on Odoo's editions page; Enterprise code was not read [odoo/odoo 20.0 addons/sale/wizard/res_config_settings_views.xml; R11 s1] | all open; the paid list stays on operation and assurance (Proposed, D2) | open-core surprises (rank 2) |
| Subscription is a sales order; a failed payment only flags "contract in exception" [R11 s8.1] | first-class aggregate, Stripe vocabulary, dunning in core, append-only MRR ledger | gaps that partners patch (ranks 3-4) |
| Pricelist result is a stored number | pure engine with explain trace, versioned rules, ambiguity lint | customization debt, opaque pricing (rank 3) |
| IAP credits for lead mining, enrichment, AI | provider traits, user keys, no credit wall (D18) | pricing surprises (rank 2) |
| Float quantities, one global UoM precision [R10 s3.3] | exact decimals, per-UoM rounding | correctness |
| Partner table mixes contact and address | typed addresses, customer as a profile role (D28) | sync complexity [R11 s2] |
| External API gated to the Custom plan on Odoo Online; RPC removal in Odoo 22 (fall 2028) [R21 s2.1] | generated REST, MCP, `$batch`, free on every plan (D31) | documentation and API (rank 10) |
| Customisation in code or Studio | properties, X1 packs and X2 CEL in data, exported as packages (D11, D32) | upgrades and customization (ranks 1, 3) |
| Dozens of SQL statements per order [R22 s3.1] | registry-driven set-based writes (estimate 3 to 10) | performance (rank 7) |
| Generic portal token | capability links plus generated IDOR matrix; WCAG 2.2 AA; withdrawal function | security, accessibility |

Deliberately the same: one table for leads and opportunities, pricelist semantics and tie-break, optional products as lines, down-payment mechanics, naive-Bayes scoring, the sale line as join point. Switching cost is the main acquisition lever, so the importer covers parties, products, pricelists and open opportunities (D22, [R21 s9], [R11 COULD 22]).

## 8. Out of scope, non-goals and later

- Usage metering, invoice-level revenue recognition and deferrals (finance, W3); tax computation ([finance](finance.md)).
- Cart, checkout, loyalty and coupons ([retail-commerce-marketing](retail-commerce-marketing.md)); POS pricing uses the same crate.
- CPQ constraint solving, territory management, multi-touch attribution, telephony (CTI), reseller portals.
- Statistical forecasting and AI assistance beyond drafts ([ai](ai.md)).
- Marketplace connectors (Amazon, Shopee and similar) as X3 or X4 packages (W4).
- Non-goals: an obop-run lead data service, a payment-processing margin, Odoo XML-RPC compatibility (D22), float money, in-process third-party code (D11).

## 9. Open questions

1. Do optional lines start at quantity zero until the customer adds them? In the Odoo 20.0 source optional lines are ordinary lines counted in totals by quantity, the portal can lower them to 0 and template lines default to quantity 1, but the starting quantity of an optional line on a sent quote remains unverified [R11 s6.1].
2. Should `subscriptions` move to W2 for the services variant? D25 places it in W3; recurring retainers appear on the CRM pipeline in W1.
3. Is the default proration by calendar day, and the dunning default (eight attempts in 14 days), acceptable to design partners?
4. Does an opportunity auto-win on order confirmation by default [R23 D.4]? Odoo 20 does not (its `sale_crm` only raises expected revenue), so default-on is a deliberate departure that design partners should confirm.
5. W1 acceptance uses in-house simple signatures; what is their evidentiary value per country? Counsel.
6. Is the `sale_payment` bridge (Proposed) the right shape, or should `payment` publish a hub port? [finance](finance.md) already emits `PaymentTransactionDone` and `PaymentTransactionFailed` for sales prepayment and subscriptions, so only the bridge's name and placement remain open.
7. Prepayment and down-payment VAT timing per country pack; owner: [finance](finance.md).
8. Default visibility of cost and margin to salespeople (proposed: hidden).
9. Price history and prior-price indication for consumers: counsel and retail doc [R28 E.7].
10. MRR definition for discounts, coupons, one-time fees and currencies (net or gross; rate type).
11. Edit-after-confirm versus amendment documents for confirmed orders.
12. Commissions wave and tier: T3 by default; needs a design partner.
13. Rental deposit as a paired product (Odoo) or a first-class deposit entity.
14. Lead enrichment provider list and lawful basis: counsel.
15. Should scoring (W2) move into W1 given its low cost?
16. Confirm with the retail spec that carts as draft orders (`order_kind = cart`) fit the sales model under the three conditions in 4.4.
17. Closed. Product cost and margin are governed by the field permission `product.cost_viewer` and carry no data class; this spec and [supply-chain](supply-chain.md) agree (the same ruling closes supply-chain OQ-17), and the name is registered in [CONVENTIONS](../CONVENTIONS.md) sections 4 and 5. This is a design ruling, not an owner decision. The number is kept because other documents cite it.

## 10. Sources

Decision IDs refer to [DECISIONS](../DECISIONS.md); names of events, permission groups, data classes, states and ports are registered in [CONVENTIONS](../CONVENTIONS.md). Research tags map as follows; sections are cited as `sN` of the file.

| Tag | File |
|---|---|
| R00 | [research/README.md](../research/README.md) |
| R01 | [research/01-odoo-app-catalog.md](../research/01-odoo-app-catalog.md) |
| R04 | [research/04-odoo-voice-of-customer.md](../research/04-odoo-voice-of-customer.md) |
| R09 | [research/09-accounting-finance-compliance.md](../research/09-accounting-finance-compliance.md) |
| R10 | [research/10-supply-chain-manufacturing.md](../research/10-supply-chain-manufacturing.md) |
| R11 | [research/11-crm-sales-subscriptions.md](../research/11-crm-sales-subscriptions.md) |
| R12 | [research/12-retail-pos-ecommerce-marketing.md](../research/12-retail-pos-ecommerce-marketing.md) |
| R14 | [research/14-services-project-helpdesk-field.md](../research/14-services-project-helpdesk-field.md) |
| R15 | [research/15-platform-collaboration-documents.md](../research/15-platform-collaboration-documents.md) |
| R16 | [research/16-ai-native-erp.md](../research/16-ai-native-erp.md) |
| R18 | [research/18-security-compliance-deployment-ops.md](../research/18-security-compliance-deployment-ops.md) |
| R19 | [research/19-ux-design-system.md](../research/19-ux-design-system.md) |
| R20 | [research/20-market-personas-mvp-gtm.md](../research/20-market-personas-mvp-gtm.md) |
| R21 | [research/21-integrations-interoperability-api.md](../research/21-integrations-interoperability-api.md) |
| R22 | [research/22-data-layer-sync-realtime-performance.md](../research/22-data-layer-sync-realtime-performance.md) |
| R23 | [research/23-kernel-model-architecture-decisions.md](../research/23-kernel-model-architecture-decisions.md) |
| R27 | [research/27-quality-release-engineering.md](../research/27-quality-release-engineering.md) |
| R28 | [research/28-docs-community-support-brand.md](../research/28-docs-community-support-brand.md) |
