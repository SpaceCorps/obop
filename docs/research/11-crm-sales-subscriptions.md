# 11 - CRM, Sales, Subscriptions, Rental and Product Catalog

Research for obop ("one business, one platform"): Odoo is the functional reference, the backend is Rust-only. Snapshot date: 2026-10-01.

## Scope & method

Primary sources: the Odoo 20.0 Community source on GitHub (read as raw files, Odoo Community is LGPL-3), the Odoo documentation repository (CC-BY-SA-4.0) and odoo.com pages, vendor docs and pricing pages (Stripe, Lago, Kill Bill, Chargebee, HubSpot, Salesforce, Attio, Twenty, SuiteCRM), GitHub and crates.io APIs for versions, licences and dates. Secondary/sentiment sources were not used. No general web-search tool was available in this session, so coverage is by direct fetch of known primary URLs; competitor pages were read through a summarising fetcher, and the places where it was ambiguous are flagged.

Labels: **[V]** verified live on 2026-10-01 from the cited source; **[I]** inference drawn from verified facts; **[G]** guess or background knowledge not re-verified today. Odoo Enterprise code was never read. Where this file describes Enterprise-only features (Subscriptions, Rental, Commissions), the only source is the public documentation.

---

## 1. Reference baseline: Odoo in October 2026

| Fact | Value | Source |
|---|---|---|
| Latest major | Odoo 20, "Released September 2026"; prior majors 19 (Sep 2025), 18 (Oct 2024), 17 (Nov 2023) [V] | https://www.odoo.com/page/release-notes |
| Code state | `odoo/odoo` default branch is `20.0`; `odoo/release.py` on 20.0 has `version_info = (20, 0, 0, FINAL, 0, '')`; docs also list a "19.4 (saas)" intermediate, so Odoo ships yearly majors plus SaaS-only point versions [V/I] | https://github.com/odoo/odoo/blob/20.0/odoo/release.py |
| Licence (Community) | LGPL v3; GitHub shows "NOASSERTION" only because of the custom LICENSE file. 54.8k stars, pushed the same day [V] | https://github.com/odoo/odoo/blob/20.0/LICENSE |
| Docs licence | CC-BY-SA-4.0 [V] | https://github.com/odoo/documentation |
| Pricing (Odoo Online, EUR) | One App Free EUR 0 (unlimited users, one app plus dependencies); Standard EUR 19.90/user/month billed yearly (EUR 24.90 monthly); Custom EUR 35.90 yearly (EUR 44.90 monthly), which adds multi-company, Studio, API, on-prem/Odoo.sh [V]. Nuances re-checked 2026-10-01: these are introductory prices ("The discount is valid for 12 months, for initial users ordered"); the struck-through list prices are Standard EUR 24.90 yearly / 31.10 monthly and Custom EUR 44.90 / 56.90; the page FAQ also says the One App Free plan allows multiple companies and can use Studio as its one app, so the plan-card "Custom only" listing of those two is not strict; a Light User rate of EUR 7.90 is shown [V] | https://www.odoo.com/pricing-plan |

### Community vs Enterprise split for this research area [V]

I checked which addon manifests exist in `odoo/odoo` branch `20.0` by requesting `addons/<name>/__manifest__.py` (HTTP 200 vs 404).

| In Community 20.0 (readable, LGPL) | NOT in Community 20.0 (Enterprise, proprietary) |
|---|---|
| `sales_team`, `crm`, `sale`, `sale_management`, `sale_crm`, `sale_margin`, `sale_stock`, `sale_project`, `sale_timesheet`, `sale_loyalty`/`loyalty`, `sale_pdf_quote_builder`, `sale_gelato`, `sale_expense`, `product`, `website_sale`, `payment`, `account_payment`, `crm_iap_mine` (lead mining), `crm_iap_enrich` | `sale_subscription` (404), `sale_renting` (404), `sale_commission` (404), `sale_amazon` (404), `website_sale_subscription`, `website_sale_renting` (404) |

Odoo's own editions page confirms the Enterprise-only status of Subscriptions, Rental and Amazon Connector (Community column crossed, Enterprise column ticked) [V, https://www.odoo.com/page/editions]; Sales Commissions is not listed on that page, so its Enterprise-only status is an inference from the missing `sale_commission` manifest [I].

Consequences: (1) CRM, quotations, pricelists, variants, margins, down payments, portal signature/payment and the stock/accounting links are legally readable for design study; (2) Subscriptions, Rental, Commissions and marketplace connectors are Odoo's paywall, so obop must design them from public docs and from Lago/Kill Bill/Stripe prior art rather than Odoo code. An OSS alternative for recurring invoicing inside Odoo exists (OCA `contract`, AGPL-3.0; its default branch is 19.0 and holds the `contract*` addons, while the 20.0 branch exists but contains no addons yet, so there is no Odoo 20 port as of today) [V, https://github.com/OCA/contract, GitHub contents API 2026-10-01]. The ERP-integrated case has no Rust prior art: the top Rust "CRM" hits are small (largest relevant: `macro-inc/macro`, 4.5k stars, a workspace app, not an ERP CRM) [V, `gh search repos`, 2026-10-01]. Rust billing prior art does exist: `meteroid-oss/meteroid` is a Rust, AGPL-3.0, 1.2k-star open-source "Pricing and Billing Infrastructure" (subscription management, invoicing, usage-based billing; last push 2026-09-18) [V, https://github.com/meteroid-oss/meteroid]; it should be evaluated as design prior art for the billing engine (AGPL, so study rather than copy). (A GitHub search for the exact words "billing subscription invoicing" restricted to Rust returns nothing; broader queries such as "usage-based billing" or "invoicing" do find it.) obop is greenfield only for the integrated ERP + CRM + billing combination.

---

## 2. Contact / partner model (`res.partner`)

Odoo uses ONE table for people, companies and postal addresses [V, https://github.com/odoo/odoo/blob/20.0/odoo/addons/base/models/res_partner.py]:

- Identity: `name`, `complete_name` (stored), `ref`, `is_company` (stored, computed), `parent_id` ("Related Company"), `child_ids`, `commercial_partner_id` (stored, recursive: self if no parent, else parent's commercial partner), `function`, `active`.
- `type` selection: `contact` (default), `invoice`, `delivery`, `other`. A name is required only when type is `contact`. Child rows of type `invoice`/`delivery` are the mechanism for multiple addresses (used by `sale.order.partner_invoice_id` / `partner_shipping_id`, both required on the order).
- Commercial sync: `vat` and the new `additional_identifiers` (JSON) are "synced commercial fields" pushed down from the commercial entity; `industry_id` is also commercial. Address fields of a `contact` child are synced from the parent, and a child with a different address silently flips to `other`.
- Ownership/segmentation: `user_id` (salesperson, inherited from parent for non-company children), `company_id`, `category_id` (tags), `lang`, `tz`, `bank_ids`, `barcode`, `comment`, `website`, geolocation (`partner_latitude/longitude`, digits (10,7)).
- Mixins: address formatting, VAT label, avatar, and Odoo `Properties` (user-defined typed custom fields via `properties.base.definition.mixin`).
- Sales extensions (from `sale`, `crm`, `payment`): pricelist per partner, payment terms, fiscal position, credit warnings, lead/opportunity counts, loyalty cards [V from manifests/views lists].

Weaknesses worth not copying [I]: contact person and postal address are conflated in one row, which is why the sync code (`_fields_sync`, the `other` fallback, upward/downward propagation) is intricate. Twenty and Attio instead separate People and Companies as distinct objects [V, Twenty server modules `person`, `company`, `opportunity`, `note`, `task` at https://github.com/twentyhq/twenty/tree/main/packages/twenty-server/src/modules].

---

## 3. Product catalog and variants

### 3.1 Odoo 20 model [V, product_template.py / product_product.py / product_attribute.py]

- `product.template` (what is sold): `name`, `type` in {`consu` Goods (default), `service`, `combo`}, `is_storable` (goods only, replaces the older "storable" type), `service_tracking` (extended by project/timesheet/event modules), `categ_id`, `product_tag_ids`, `list_price`, `standard_price` (cost, derived from variant), `uom_id`, `uom_ids` (packagings, now modelled as UoMs), `sale_ok`/`purchase_ok`, `seller_ids`, `optional_product_ids` (upsell/cross-sell, table `product_optional_rel`), `invoice_policy` (`order` or `delivery`), `reinvoice_policy`, `combo_ids` (-> `product.combo`), `product_properties` (custom fields by category), `attribute_line_ids`, `base_unit_*` (unit price display).
- `product.product` (sellable SKU): `product_tmpl_id`, `default_code`, `barcode`, `product_template_attribute_value_ids`, stored+indexed `combination_indices`, `price_extra`, computed `lst_price`, per-variant `standard_price`, weight/volume. A partial unique index enforces `(product_tmpl_id, combination_indices) WHERE active`.
- Attribute graph: `product.attribute` (name, `create_variant`, `display_type`) -> `product.attribute.value` -> `product.template.attribute.line` (which attribute on which template, with chosen values) -> `product.template.attribute.value` (PTAV: per-template value with `price_extra`, `excluded_value_ids`, `ptav_active`).
- Variant creation modes (`create_variant`, immutable once an attribute is on a product): **always** ("Instantly": full cartesian product), **dynamic** ("Dynamically": created when first ordered), **no_variant** ("Never": informational, carried on the order line). Display types: radio, pills, select, color, multi-checkbox, image [V, https://www.odoo.com/documentation/master/applications/sales/sales/products_prices/products/variants.html]. A guard config `product.dynamic_variant_limit` (default 1000) aborts runaway cartesian products [V, product_template.py].
- Sale-time configuration: order lines store `product_custom_attribute_value_ids` (free-text custom values) and `product_no_variant_attribute_value_ids`; exclusions are evaluated by `_is_combination_possible*` [V, sale_order_line.py].
- Combo products (`type = combo`, `product.combo`, `product.combo.item`) sell a bundle where the customer picks one item per combo; the order stores `combo_item_id` and `linked_line_id` so that the parent and chosen children form a line tree [V].

### 3.2 Comparison hint [I]

Stripe models `Product` -> many `Price`s and has no variant matrix; Kill Bill models `Product` -> `Plan` -> `Phase` with price lists [V, https://docs.killbill.io/latest/userguide_subscription]. Billing catalogs and ERP catalogs differ structurally: obop needs the ERP template/variant tree for goods and a `plan`/`price` layer on top for recurring items (section 8).

---

## 4. Pricing engine (pricelists and discounts)

### 4.1 Odoo `product.pricelist` + `product.pricelist.item` [V, product_pricelist_item.py, product_pricelist.py]

Pricelist: `name`, `currency_id`, `company_id`, `country_group_ids`, `sequence`, `item_ids`, `active`. The "Default" pricelist is the first one without a country group, by sequence order; a partner's own pricelist is auto-filled on the quotation [V, docs pricing.rst].

Rule (`item`) fields: `applied_on` (`0_product_variant`, `1_product`, `2_product_category`, `3_global`), `categ_id`/`product_tmpl_id`/`product_id`, `min_quantity`, `date_start`/`date_end` (datetime), `uom_id` (packaging), `compute_price` (`fixed` | `discount` | `markup`, where markup is stored as a negated discount), `base` (`list_price` | `standard_price` cost | `pricelist` another list, with recursion depth guard), `fixed_price`, `price_discount`, `price_round`, `price_surcharge` ("Extra Fee"), `price_min_margin`, `price_max_margin`.

Formula: `price = base - base*discount/100`, then round to the `price_round` multiple, then add surcharge, then clamp to `[base+min_margin, base+max_margin]`. Price in another currency is converted at the quotation date [V].

Rule selection when several match on one pricelist: (1) most specific (variant > product > subcategory > parent category > global > any-categorised), (2) larger `min_quantity` wins, (3) category hierarchy order, (4) newest rule wins [V, https://www.odoo.com/documentation/master/applications/sales/sales/products_prices/prices/pricing.html]. Pricelists are checked top to bottom to auto-select one.

### 4.2 How the price lands on the order line [V, sale_order_line.py]

`price_unit` is a computed-and-stored field (depends on product, UoM, quantity) that is skipped for down-payment lines, invoiced lines, and lines with a manual price; `discount` is computed as `(base_price - pricelist_price)/base_price*100` when the rule is a plain discount, so the customer sees a struck-through discount rather than a silently lowered price. `pricelist_item_id` records which rule fired. `technical_price_unit` stores the raw computed price. New in Odoo 20: order-level `document_tax_mode` (`tax_excluded`/`tax_included`), making B2C tax-included quoting a first-class mode. Order-level discounts come from a wizard (`sale_order_discount`) and loyalty programs (`sale_loyalty`), which add discount lines [V, manifest].

### 4.3 Design read [I]

The pricing function is pure: `(pricelist, product_variant, qty, uom, date, currency) -> (unit_price, discount_pct, rule_id)`. It belongs in a dependency-free Rust crate with an explain trace, because pricing bugs are the most expensive class of ERP defect, and rental (section 9) and subscriptions reuse the same rule table with different "quantity" semantics.

---

## 5. CRM

### 5.1 Data model [V, crm_lead.py, crm_stage.py, crm_team.py, crm_team_member.py, crm_recurring_plan.py in branch 20.0]

| Model | Key fields / behaviour |
|---|---|
| `crm.lead` | ONE table for leads and opportunities, discriminated by `type` (`lead`/`opportunity`; leads are optional via group `crm.group_use_lead`). Mixins: mail thread, blacklist, phone, activities, UTM, duration tracking in stage (`mail.tracking.duration.mixin`). Fields: `name`, `user_id`, `team_id`, `company_id`, `partner_id` plus denormalised contact copy (`contact_name`, `partner_name`, `email_from`, `phone`, address, `lang_id`, `email_state`/`phone_state` quality flags), `stage_id`, `priority` ('0'-'3'), `tag_ids`, `expected_revenue`, `probability`, `automated_probability`, `is_automated_probability`, `prorated_revenue`, `recurring_revenue` + `recurring_plan` -> `recurring_revenue_monthly` (= revenue / plan months) and prorated variants, `date_deadline` (expected closing), `date_closed`, `date_open`, `date_conversion`, `won_status` (`won`/`lost`/`pending`, computed stored), `lost_reason_id`, `lead_properties` (custom fields defined per team), UTM `campaign/medium/source`, `active`. |
| `crm.stage` | `name`, `sequence`, `is_won`, `fold`, `rotting_threshold_days` (stale-deal highlighting; added in 19.0, absent in 18.0), `requirements`, `team_ids` (stages can be team-specific). |
| `crm.team` (defined in `sales_team`, extended by `crm`) | `name`, `user_id` (leader), `company_id`, `currency_id`, `member_ids`, `use_leads`, `use_opportunities`, email `alias_id` (incoming mail creates leads), `assignment_optout`, `assignment_max`, `assignment_domain`, `lead_properties_definition`. |
| `crm.team.member` | (`user_id`, `crm_team_id`), `assignment_max` (30-day capacity; `0` = out of rotation, at/above a quota constant = unlimited), `assignment_domain`, `assignment_domain_preferred`; computed 24h and 30-day lead counts. |
| `crm.recurring.plan` | `name`, `number_of_months` (>= 0): expected revenue can be flagged recurring and normalised to MRR before the deal is won. |
| `crm.lost.reason`, `crm.tag`, `crm.lead.scoring.frequency` | lost reasons; tags; the Bayes frequency table (see 5.2). |

Lifecycle rules in code [V]: `action_set_won` moves to a won stage (after the current stage by sequence, else the last earlier) with probability 100; `action_set_lost` archives the lead and sets probability 0 (restore unarchives and clears the reason); a lead in a won stage must have probability 100 (DB-level check), probability is in [0,100]; a user cannot manually set 100 % (docs) [V]; `convert_opportunity` sets `type=opportunity`, stamps `date_conversion`, optionally finds or creates the partner and assigns salespeople round-robin; `merge_opportunity` accepts 2 to 5 records (except superuser), picks the "most confident" head, moves followers, messages, activities, attachments and meetings, logs a summary and deletes the tail; duplicate detection is by email (`_get_lead_duplicates`). Database indexes: `(user_id, team_id, type)`, `(create_date, team_id)`.

### 5.2 Predictive lead scoring (PLS) [V]

Docs: "always active", a naive-Bayes classifier; `Stage` and `Team` are always variables; optionally `State`, `Country`, `Phone Quality`, `Email Quality`, `Source`, `Language`, `Tags` (the docs list seven optional variables, including geographical `State`); a start date limits training data; manual override is remembered until the user clicks the AI icon [V, https://github.com/odoo/documentation/blob/master/content/applications/sales/crm/track_leads/lead_scoring.rst]. Code: win score / (win score + lost score) with product of per-variable likelihoods; zero-frequency avoided by adding 0.1 to every frequency; frequencies live in `crm_lead_scoring_frequency` (variable, value, won_count, lost_count, team); the config keys are `crm.pls_fields` and `crm.pls_start_date`; recompute batches of 50,000 (`PLS_COMPUTE_BATCH_STEP`) and 5,000 updates; frequencies are incremented when a lead changes state (`_pls_increment_frequencies`) [V, crm_lead.py]. This is cheap, explainable and needs no ML runtime, which suits a Rust implementation.

### 5.3 Assignment, dedup, forecasting, enrichment [V]

- Rule-based assignment: enabled via config `crm.lead.auto.assignment`, run manually or on a cron (`_cron_assign_leads(force_quota, creation_delta_days=7)`); per team it de-duplicates by email (merging duplicates), then allocates by member domain, preferred domain and the rolling 30-day quota; unassigned leads otherwise go to the team leader.
- Forecast report: opportunities grouped by month of expected closing date; "prorated revenue" = expected revenue x probability; stored in `prorated_revenue` (code: `round(expected_revenue * probability / 100, 2)`) [V, docs `crm/performance/forecast_report.rst`, crm_lead.py]. Other reports: pipeline analysis, expected revenue, lead distribution, unattended leads, quality leads, marketing attribution, resellers, CRM gamification [V, https://www.odoo.com/documentation/master/applications/sales/crm.html].
- Lead mining and enrichment are In-App Purchase services billed in credits (enrichment: one credit per lead, cron every 60 minutes, keyed on email domain) and depend on Odoo's hosted IAP backend [V, docs lead_enrichment.rst]; the `crm_iap_*` modules in Community are thin clients to that service. obop cannot assume an equivalent.
- `sale_crm` glue: `sale.order.opportunity_id`; on `action_confirm` the linked opportunity's `expected_revenue` is raised to the order's untaxed total if lower and currencies match (never lowered); the lead exposes `quotation_count`, `sale_order_count`, `sale_amount_total` [V, sale_crm/models].

### 5.4 Competitor CRMs

| Product | Licence / status (2026-10-01) | Pricing (as shown) | Notes |
|---|---|---|---|
| HubSpot Sales Hub | proprietary | Free up to 2 users; Starter USD 7/seat/month annual (USD 20 monthly); Professional USD 90 annual (USD 100), one-time onboarding USD 1,500; Enterprise from USD 150, onboarding USD 3,500; credits for AI (extra USD 0.01 each) [V, https://www.hubspot.com/pricing/sales] | Lead scoring: up to 5 scores on Professional, 10 on Enterprise (none on Free/Starter; re-fetched 2026-10-01 through a summarising fetcher, so [I]); pipelines 15/100/350; quotes/CPQ sit under Revenue Hub |
| Salesforce Sales | proprietary | Starter Suite USD 25, Pro Suite USD 100, Core USD 195, Advanced USD 395, Max USD 550 per user/month (annual); Agentforce for Sales add-on from USD 125 [V, https://www.salesforce.com/sales/pricing/] | The page's tier names differ from the older Enterprise/Unlimited names; mapping not confirmed by the page |
| Pipedrive | proprietary | NOT VERIFIED: pricing page returned HTTP 403 to the fetcher | [G] a pipeline-centric per-seat CRM |
| Attio | proprietary | Free; Plus EUR 31 annual / 39 monthly; Pro EUR 74 annual / 93 monthly (the fetched text was garbled for Pro, treat as [I]); Enterprise custom. Limits scale with objects (3/5/12/unlimited) and records (50k/250k/1M) [V, https://attio.com/pricing] | Custom objects, workflows, call intelligence; data-model-first CRM |
| Twenty | AGPL-3.0 with commercial "Enterprise" files and MIT SDK/UI packages, plus a "Twenty Application Exception"; v2.44.0 released today; 57.8k stars; TypeScript (NestJS 11, TypeORM, BullMQ, Postgres, GraphQL) [V, https://github.com/twentyhq/twenty/blob/main/LICENSE] | Pro USD 9/user/month (yearly), Organization USD 19, Enterprise from USD 50k/year; SSO, row-level permissions, audit logs gated to Organization [V, https://twenty.com/pricing] | Objects/fields/views defined "as code" (`defineObject`), MCP server, workflow "agent" steps; no quoting, no invoicing |
| SuiteCRM | AGPL-3.0; 8.10.2 released 2026-07-31 (SuiteCRM-Core), also 7.15.x maintained [V, GitHub + https://suitecrm.com/] | free self-host; hosted and support plans | Leads, opportunities, quotes, campaigns, cases; PHP |

Takeaway [I]: Odoo's CRM differentiator is not the pipeline (every CRM has one) but that an opportunity converts to a quotation on the same database and partner, and the quotation's confirmation updates the opportunity, inventory and accounting. Twenty and Attio show what users now expect of the CRM surface: custom objects and fields, MCP/API-first access and agentic workflows.

---

## 6. Sales (quotation to cash)

### 6.1 `sale.order` and `sale.order.line` [V, sale_order.py, sale_order_line.py, 20.0]

- Order `state`: `draft` (Quotation), `sent`, `sale` (Sales Order), `cancel`; a separate boolean `locked` (replacing the old `done` state), `invoicing_closed`. Order `invoice_status`: `upselling`, `invoiced`, `to invoice`, `no`. Stored `delivery_status`: `pending`, `started`, `partial`, `full` (defined in `sale_stock` in 18.0 and 19.0, moved into core `sale` in 20.0).
- Header: `partner_id`, `partner_invoice_id`, `partner_shipping_id`, `pricelist_id`, `currency_id` + `currency_rate`, `payment_term_id`, `fiscal_position_id`, `journal_id`, `user_id`, `team_id`, `incoterm`/`incoterm_location`, `date_order`, `validity_date`, `commitment_date`, `client_order_ref`, `origin`, UTM, `tag_ids`, `note` (terms), `document_tax_mode`.
- Portal fields: `require_signature`, `signature`, `signed_by`, `signed_on`, `prepayment_percent`, `prepayment_amount`, `amount_paid`, `transaction_ids`. Mixins: `portal.mixin` (tokenised access URL), `utm.mixin`, `product.catalog.mixin`.
- Line: `display_type` in {`line_section`, `line_subsection`, `line_note`}, `product_id`/`product_template_id`, `product_uom_qty`, `product_uom_id`, `price_unit`, `discount`, `tax_ids`, `price_subtotal`/`price_total`, `customer_lead` (days), `qty_delivered_method` (`manual`, `analytic`, and `stock_move` added by `sale_stock`), `qty_delivered`, `qty_invoiced`, `qty_to_invoice`, `invoice_policy` (`order`/`delivery`), `invoice_status`, `is_downpayment`, `is_expense`, `analytic_distribution`, accrual links. Section lines can collapse prices or composition when printed (`collapse_prices`, `collapse_composition`, `section_qty`).
- **Optional products changed in Odoo 19 (already so in 20)**: the old `sale.order.option` model is gone; `sale_management/models/sale_order_option.py` returns HTTP 200 on branches 17.0 and 18.0 but 404 on 19.0 and 20.0, so the removal happened in 19.0, not 20. Optional products are now ordinary lines in the normal line tree: `is_optional` ("Optional Line") is a boolean set on a section or subsection line, and a line is optional (and editable on the portal) when it sits directly under an optional (sub)section (`_is_line_optional`) [V, sale_management/models/sale_order_line.py and sale_order_template.py]. Whether optional lines start at quantity 0 until the customer adds them on the portal was not confirmed in the code read (unverified). A single-table line tree is simpler than a parallel option table, and obop should copy that.
- Quotation templates (`sale.order.template`, type `quotation` or `section`): lines, terms, validity in days (`number_of_days`), default `require_signature`, `prepayment_percent`, confirmation mail template, journal, allowed `team_ids`. `sale_pdf_quote_builder` lets a quote embed product/company PDFs with dynamic form fields [V, https://www.odoo.com/documentation/master/applications/sales/sales.html].

### 6.2 Confirmation, signature, payment, margin [V]

- `action_confirm`: allowed from `draft`/`sent`, checks analytic distribution, writes `state=sale` and `date_order=now`, calls the `_action_confirm` hook (which other modules extend), auto-locks if configured, sends the confirmation email.
- Online signature or payment: the portal confirms the order automatically when signature is required/provided or when the paid amount reaches `prepayment_percent`. A payment transaction's post-processing (`sale/models/payment_transaction.py`) calls `_check_amount_and_confirm_order()` and, if the company flag `sale_automatic_invoice` is set, creates and posts the invoice and sends it by cron.
- Margin lives in a separate auto-installed module (`sale_margin`): per line `purchase_price` (cost), `margin`, `margin_percent`; a reporting measure, not core.
- Upsell: line `invoice_status = upselling` flags a product invoiced on ordered quantity whose delivered quantity exceeds the ordered quantity (e.g. a project overran but the extra was not billed) [V, `_compute_invoice_status`]; subscriptions add an explicit Upsell button (section 8).

### 6.3 Invoicing policies and down payments [V, sale_make_invoice_advance.py, docs down_payment.rst]

- Policy per product: invoice on **ordered** quantity or **delivered** quantity (services can also be "prepaid/fixed price"; timesheet/milestone variants live in `sale_timesheet`/`sale_project`).
- Create-invoice wizard methods: `delivered` (regular), `percentage` or `fixed` down payment. The down-payment invoice is posted against a dedicated income/"down payment" account; the order gets a line with `is_downpayment = True`. On the final invoice, `_create_invoices` adds a "Down Payments" section and negates the earlier down-payment lines (quantity -1), with an option `deduct_down_payments`; negative totals become credit notes when `final`. Consolidated billing across orders exists (`consolidated_billing`). Down payments are documented as not the same thing as payment-term instalments.

---

## 7. Automatic flows into inventory and accounting

Reconstructed from the 20.0 sources above (the Odoo behaviours marked [G] come from general Odoo knowledge of `stock`/`account`, not re-read today):

| Trigger | Inventory | Accounting | CRM / other |
|---|---|---|---|
| SO confirmed (`sale`) | `sale_stock`: for goods lines with `stock_move` delivery method, `_action_launch_stock_rule` creates procurements using line `route_ids`/warehouse (`warehouse_id` computed from routes or order), grouped per (warehouse, commitment/expected date) into one picking group; policy `picking_policy` = direct/one [V, sale_stock_line.py, sale_stock_order.py]. Reservations/MTO/manufacturing follow from routes [G] | Nothing posted yet. An accrual wizard (`account.accrued.orders.wizard`) can book accrued revenue for delivered-not-invoiced lines [V, manifest] | `sale_crm` raises opportunity expected revenue; `sale_project` can create tasks/projects per service line [I from module list] |
| Delivery validated | `qty_delivered` for stock-move lines is derived from done moves; order `delivery_status` updates; with delivery-policy lines, `qty_to_invoice` rises and `invoice_status` becomes `to invoice` [V/I] | Perpetual valuation entries (COGS/stock) are booked by `stock_account` if configured [G] | |
| Invoice created (draft) | - | `_create_invoices` builds `account.move` lines via `_prepare_invoice_line`, linked by `sale_order_line_invoice_rel`; grouping keys `_get_invoice_grouping_keys`; analytic distribution copied [V] | |
| Invoice posted / refunded | - | Journal entries (AR, revenue, tax); `qty_invoiced` = invoice qty minus refund qty, ignoring cancelled moves [V, `_prepare_qty_invoiced`] | |
| Online payment | - | Transaction done -> order confirmed if amount >= prepayment; optional auto-invoice; payment reconciled to the invoice [V] | |
| Recurring billing date (Enterprise, docs) | Subscriptions with goods generate a delivery order along with each invoice (coffee-beans example) [V, docs subscriptions_deliveries.rst] | Recurring invoice created by cron; failure -> "Contract in exception" flag blocks the cron [V] | MRR log entry |
| Rental pickup/return (Enterprise, docs) | Pickup/return wizards create stock moves (serial numbers tracked); services have no stock movement [V, docs] | Deposit refund via credit note [V] | |

Design rule to carry over [I]: the sale line is the join point. Delivery, invoicing, projects and procurement each contribute a quantity to the line, and the line state is a function of those quantities, not a separately edited status.

---

## 8. Subscriptions and recurring billing

### 8.1 Odoo Subscriptions (Enterprise; docs only) [V]

- Model concept: a **subscription is a confirmed sales order with a recurring plan**; renewals and upsells are additional linked sales orders ("Sales History" smart button, each with its own "Subscription Status"); there is no separate subscription entity visible in the docs [V, https://github.com/odoo/documentation/blob/master/content/applications/sales/subscriptions.rst and subscriptions/renewals.rst].
- Plan ("Recurring Plan", formerly recurrence period): billing period in weeks, months or years (days are reserved for Rental), `Automatic Closing` (days after which an unpaid subscription closes), `Align to Period Start` (first invoice prorated to the start of the next period), self-service flags (`Closable`, `Add Products`, `Renew`, `Optional Plans`), per-plan recurring price table (product x plan x pricelist).
- Statuses visible in report filters: Quotation, In Progress, Paused, Churned, To renew [V, subscriptions/reports.rst].
- Upsell: only after the subscription has been invoiced; proration applies to Service products only, not Goods; Close: only after invoicing, requires reason and date, sets Churned banner.
- Payments: automatic payment needs a tokenising provider; docs list Adyen, Authorize.net, Flutterwave, Razorpay, Stripe and Xendit. **On a failed automatic payment Odoo only adds a "Payment Failure" tag and ticks "Contract in exception", which halts scheduled actions until a human checks whether the money arrived** [V, subscriptions/automatic_payments.rst]. There is no documented retry ladder, dunning email sequence, or status machine like `past_due`/`unpaid`. This is the clearest functional gap to beat.
- Metrics: four reports (Subscriptions Analysis, Retention Analysis, MRR Breakdown, MRR Analysis) with measures Monthly Recurring, Yearly Recurring, Recurring Revenue, Expected Cost, MRR Change, ARR Change, Active Subscriptions Change; dimensions include plan, template, salesperson, team, referrer, country, product, first contract date, next invoice date, end date [V].
- Integrations: delivery of goods with each invoice, loyalty on subscriptions, eCommerce selling (auto-creates and confirms the subscription quotation) [V].

### 8.2 Billing engines compared

| | Stripe Billing | Lago | Kill Bill | Chargebee |
|---|---|---|---|---|
| Status / licence | Proprietary SaaS | Open source AGPL-3.0, v1.53.0 (2026-09-08), API in Ruby, 10.6k stars on `getlago/lago`; premium add-ons [V] | Apache-2.0, latest GitHub Release 0.24.22 (2026-09-29; git tag dated 2026-09-28), Java, 5.8k stars, admin UI Kaui; managed "Aviate" tier (proprietary, fixed price, no amount shown on killbill.io). Newer 0.25.x artifacts already exist: Maven Central lists 0.25.4 as latest (metadata updated 2026-09-28), git tags run to killbill-0.25.5 (2026-09-24) and `master` is 0.25.6-SNAPSHOT, but no GitHub Release is published for 0.25.x, so whether 0.25.x is a development or a supported line is unverified [V/I; https://github.com/killbill/killbill, https://repo1.maven.org/maven2/org/kill-bill/billing/killbill/maven-metadata.xml] | Proprietary SaaS |
| Price (as shown) | Pay-as-you-go 0.7 % of Billing volume; or annual contract in volume tiers (e.g. SEK 6,250/month up to SEK 1M volume, overage 0.67 %); Smart Retries, dunning emails, portal, quotes, schedules included [V, https://stripe.com/billing/pricing (Sweden page)] | Premium pricing only via sales; dunning campaigns, progressive billing, quotes/order forms, CRM/CPQ/accounting integrations are listed as add-ons ("available on demand" / "on request"); the page also groups prepaid credits, credit notes, plan overrides, invoice preview, SSO and an MCP server under Lago Premium without saying which of these are also in the AGPL edition (unverified) [V, https://www.getlago.com/pricing] | Free self-hosted | "$0 + 0.80 %" and "$99 + 0.65 %" of invoicing volume shown without clear tier labels (ambiguous, [I]); 100M usage events/month; CPQ Lite free for 50 quotes [V, https://www.chargebee.com/pricing/] |
| Core entities | Customer, Product, Price, Subscription, SubscriptionItem, SubscriptionSchedule (phases), Invoice, InvoiceItem, Credit note, Coupon/Promotion, Meter | Customer, Billable metric, Plan, Charge, Subscription, Event, Invoice, Fee, Credit note, Wallet (prepaid credits), Add-on, Coupon, Dunning campaign, Quote/Order form/Order [V] | Account, Bundle, Subscription, Product (BASE/ADD_ON/STANDALONE), Plan, Phase (TRIAL/DISCOUNT/FIXEDTERM/EVERGREEN), Price list, Invoice (items typed RECURRING, FIXED, USAGE, TAX, ITEM_ADJ, CREDIT_ADJ, REPAIR_ADJ, CBA_ADJ) [V] | Plans/add-ons/charges (item model) [G] |
| Proration | Default per second; `create_prorations` / `always_invoice` / `none`; `billing_mode` classic vs flexible (flexible credits the last price actually billed, fixing credit-for-unpaid-upgrade) [V, https://docs.stripe.com/billing/subscriptions/prorations] | Daily: e.g. USD 50 plan from Aug 10 = 22/31 x 50 = 35.48; calendar vs anniversary billing time; pay in advance or arrears [V, https://getlago.com/docs/guide/subscriptions/assign-plan] | Billing alignment ACCOUNT/BUNDLE/SUBSCRIPTION; BCD after month end clamps to last day; any mismatch gives a prorated first invoice; cancel END_OF_TERM or IMMEDIATE (credit); upgrade = new RECURRING item plus negative REPAIR_ADJ, excess carried as CBA_ADJ credit [V] | [G] |
| Dunning | Smart Retries (ML timing), recommended 8 tries in 2 weeks, or custom up to 3 retries; after exhaustion: cancel, unpaid, leave past_due or pause; on hard-decline codes retries stay scheduled (attempt_count keeps incrementing) but only execute once a new payment method is detected [V, https://docs.stripe.com/billing/revenue-recovery/smart-retries] | Campaigns (add-on) plus a beta "Dunning Agent" that reads 12 months of payment history and loops in a human [V, https://getlago.com/docs/guide/ai-agents/dunning-agent.md] | Payment retry default 8, 8, 8 days; `overdue.xml` state machine (conditions on unpaid invoice count/balance/age or control tags; actions: subscription cancellation policy, block changes, disable entitlement; no "pause" action in the docs) [V, https://docs.killbill.io/latest/userguide_subscription and /latest/overdue] | [G] |
| Statuses | trialing, active, incomplete, incomplete_expired, past_due, unpaid, paused, canceled; configurable "status resolution" by latest invoice or all invoices [V, https://docs.stripe.com/billing/subscriptions/overview] | pending, active, terminated, canceled, incomplete [V, `lago-api` `app/models/subscription.rb`; the docs page read only names active and pending] | Entitlement vs billing state are distinct (access can end before billing does) [V] | [G] |
| Usage billing | Meters API (up to 100M events/month included); advanced usage pricing via Metronome [V] | Aggregations COUNT, COUNT_UNIQUE, LATEST, MAX, SUM, WEIGHTED SUM, plus CUSTOM; charge models standard, graduated, package, percentage, volume, graduated_percentage, custom, dynamic [V, `lago-api` `app/models/billable_metric.rb` and `charge.rb`] | CONSUMABLE (tiers ALL_TIER/TOP_TIER) and CAPACITY, IN_ARREAR only; late usage re-invoiced for the previous 2 periods [V] | Usage events included |
| Dry-run preview | `invoices.create_preview` with `proration_date` [V] | Invoice preview endpoint (premium) [V] | UPCOMING_INVOICE, TARGET_DATE, SUBSCRIPTION_ACTION dry runs [V] | |

### 8.3 Lessons [I]

1. Separate the **subscription** (state machine) from the **sales order** (commercial document). Odoo's SO-as-subscription makes upsell/renewal multi-document and state reporting awkward; Stripe/Lago/Kill Bill all have a first-class subscription with items and a schedule.
2. The billing engine must be a pure function of (subscription state, catalog version, as-of date) returning invoice lines, with a preview mode. This is what makes proration testable and dunning safe.
3. Decide the proration semantics explicitly (second vs day; classic vs "last price billed"); Stripe's own history shows the classic choice produces credits for never-paid upgrades.
4. Store an append-only **MRR movement ledger** (new, expansion, contraction, churn, reactivation) at every subscription change; MRR/ARR/churn/retention reports then become SQL over the ledger. Odoo's "MRR Change / ARR Change / Active Subscriptions Change" measures imply the same design [I].
5. Dunning is first-class: retry schedule, customer notices, terminal action, and manual hold; Odoo lacks it, Stripe includes it, Lago monetises it.
6. Calendar-month arithmetic (anchor day clamped to the last day of short months, Kill Bill's BCD rule) needs a real civil-date library.

---

## 9. Rental

Odoo Rental is Enterprise-only (`sale_renting` not in Community 20.0) [V]. From the public docs [V, https://github.com/odoo/documentation/blob/master/content/applications/sales/rental.rst and rental/*]:

- A rental product has a **Periodicity** (a single unit of time per product, e.g. hours or days; the full option list was not re-read), **padding time** (in hours, makes the product unavailable to rent for that duration; the product form shows it only when Periodicity is Hours), pickup/return times for web orders (shown for every Periodicity except Hours), and extra price tiers in the normal pricelist structure [V, rental/configure_products/products.rst].
- **Price rule: only one price line is used, the cheapest wins** (example: 8 days with tiers 1 day USD 100, 3 days USD 250, 1 week USD 500 bills 3 x the 3-day tier = USD 750, not 7 days + 1 day). Minimum quantity for a rule = line quantity x rental duration in the periodicity unit (different from Sales/Subscriptions).
- Confirmed orders **reserve** stock for the period; the Pickup and Return buttons open wizards with Reserved / Picked-up / Returned columns; serial/lot tracking is supported; services (e.g. hotel rooms) skip stock moves.
- **Deposits**: a second product per rental item (service, invoicing policy "Delivered quantity (manual)"), added as optional product or via template, refunded by credit note on return.
- Late fees: not documented as an automatic feature on the pages read [gap].

Design read [I]: rental = sales order lines carrying a `[start, end)` period plus an availability check against overlapping reservations. In PostgreSQL this is a `tstzrange` plus an `EXCLUDE USING gist` constraint per serial-tracked unit [G], and for quantity-tracked stock an overlap sum query under a row lock. Pricing reuses the pricelist engine with duration-based quantity.

---

## 10. Sales teams, targets and commissions

- Teams are `crm.team` (shared by CRM, Sales and eCommerce; one of them is "Website"), with leader, members (`crm.team.member`), optional lead assignment settings, email alias, and team-specific pipeline stages [V].
- Commissions (Sales app, enabled under Settings > Invoicing; Enterprise, since `sale_commission` is absent from Community) [V, https://github.com/odoo/documentation/blob/master/content/applications/sales/sales/commissions.rst]: a **plan** is "Based on" Targets or Achievements, "per" salesperson or team, with Target Frequency monthly/quarterly/yearly; target plans have periods with target values, "On Target Commission" and **levels** from 0 % up to 300 % of target; Achievements are metrics: Amount Sold, Amount Invoiced, Quantity Sold, Quantity Invoiced, Margin, and MRR (only when Subscriptions is installed), filterable by product or category. Plans are assigned to salespeople (bulk-add in developer mode).
- Design read [I]: commission = `plan x salesperson x period`, computed from posted invoices/orders by an incremental job and frozen when the period closes, with manual adjustment lines. Clawback on credit notes and on invoice reversal must be decided up front.

---

## 11. Customer portal

Odoo: every quotation/order/invoice has a tokenised access URL (`portal.mixin`), so an anonymous customer can view, sign, pay (prepayment percentage or full), download the PDF and add optional products; logged-in portal users (`res.users` with `share = true`, attached to a partner) also manage subscriptions (close, upsell, switch plan, update payment token) according to plan self-service flags [V/I from sale_order.py and subscription docs]. Stripe's hosted portal is included in Billing (custom domain USD 10/month per its pricing page); Lago's is Premium [V]. obop should treat the portal as a token-scoped read/act surface, not a special user type.

---

## 12. Analytics

Odoo CRM reports: pipeline analysis, expected revenue, forecast, lead distribution, unattended/quality leads, UTM attribution. Sales: `sale.report` (quoted/ordered/delivered/invoiced quantities and amounts, margin, by product/team/country). Subscriptions: four reports (section 8). All are pivot/graph views over SQL views with Group By and Measures [V, docs pages above]. For obop [I]: Postgres (materialised) views plus one generic pivot/graph component, with "measures" declared as a catalogue so reports are configuration.

---

## 13. Proposed obop data model (summary)

Postgres tables (snake_case), all tenant/company scoped; `id uuid` or `bigint` is a separate decision.

| Module | Tables and key columns |
|---|---|
| partner | `partner`(id, kind person/org, name, parent_id, commercial_partner_id, ref, tax_id, identifiers jsonb, salesperson_id, lang, tz, pricelist_id, payment_term_id, fiscal_position_id, active, properties jsonb); `partner_address`(partner_id, role contact/invoice/delivery/other, address fields, geo); `partner_tag` |
| catalog | `product_template`(kind goods/service/combo, is_storable, category_id, uom_id, list_price, cost, invoice_policy, service_tracking, properties jsonb); `product_variant`(template_id, sku, barcode, combination_key, price_extra; unique(template_id, combination_key) where active); `attribute`(create_mode always/dynamic/never, display_type); `attribute_value`; `template_attribute_line`; `template_attribute_value`(price_extra, excluded jsonb/rel); `product_combo`, `product_combo_item`; `uom`, `uom_category`; `product_optional` |
| pricing | `pricelist`(currency, sequence, country_groups); `pricelist_rule`(applied_on, category/template/variant, min_qty, date_from/to, compute fixed/discount/markup, base list/cost/pricelist, fixed_price, discount_pct, round, surcharge, min_margin, max_margin, uom_id); `price_evaluation_log` optional for explain |
| crm | `lead`(type, stage_id, team_id, user_id, partner_id, contact snapshot, expected_revenue, probability, probability_auto, auto_mode bool, recurring_revenue, recurring_plan_id, expected_close, won_status, lost_reason_id, properties jsonb, utm_*); `stage`(is_won, fold, rot_days); `team`, `team_member`(capacity_30d, domain); `lost_reason`; `scoring_frequency`(team_id, variable, value, won, lost); `activity`/`activity_plan` |
| sales | `sale_order`(state, locked, partner/invoice/shipping ids, pricelist_id, currency, tax_mode, validity_date, require_signature, prepayment_pct, signed_by/at/signature, team_id, salesperson_id, opportunity_id, template_id, portal_token); `sale_order_line`(display_type, parent_line_id, is_optional, is_downpayment, product_variant_id, qty, uom, price_unit, discount, pricelist_rule_id, tax_ids, delivery_method, qty_delivered, qty_invoiced, invoice_policy, rental_start/end, properties); `sale_order_template`(+ lines); `sale_order_invoice_line_link` (sale line to invoice line) |
| subscription | `plan`(interval unit+count, trial_days, anchor rule, pay_in_advance, proration policy); `plan_price`(plan_id, product/variant, pricelist, currency, amount, min_qty); `subscription`(partner, status, plan_id, start, anchor_day, current_period_start/end, next_invoice_at, cancel_at, pause_until, collection_method, payment_token_id, source_sale_order_id); `subscription_item`; `subscription_event`(append-only, with MRR delta, reason); `mrr_movement`; `dunning_policy`(step list); `dunning_run`; `close_reason` |
| rental | `rental_pricing_tier` (reuse pricelist_rule with duration unit), `rental_reservation`(line_id, variant/serial, period tstzrange, status reserved/picked/returned) with exclusion constraint; `deposit_rule` |
| commission | `commission_plan`(basis target/achievement, scope salesperson/team, frequency, levels); `commission_metric`; `commission_assignment`; `commission_period_result` (frozen) |

Rust implementation notes [I, versions verified on crates.io 2026-10-01]: money with `rust_decimal` 1.43.0 (MIT) stored as `NUMERIC` with per-currency rounding (`iso_currency` 0.7.1, `rusty-money` 0.5.1 exist); civil-time arithmetic for billing anchors with `jiff` 0.2.37 (chrono 0.4.45 also current); DB layer `sqlx` 0.9.0 or `sea-orm` 2.0.4 (MIT OR Apache-2.0); PDF quotation rendering candidate `typst` 0.15.1 (Apache-2.0) as an embeddable typesetter (`printpdf` 0.12.8 for low-level drawing; `genpdf` last released 2021). Express billing periods as (unit, count, anchor) rather than iCal RRULEs (`rrule` 0.14.0, last release 2025-04).

---

## Confidence & gaps

- High confidence [V]: Odoo 20 release and Community/Enterprise split (direct file probes); all field names and selections quoted from the 20.0 Community source; pricelist rule semantics; PLS algorithm and smoothing constant; subscription, rental and commission behaviour as documented publicly; Stripe proration/retry/status semantics; Kill Bill catalog, billing alignment and overdue mechanics; licences and release dates of Lago, Kill Bill (GitHub Releases line only, see the 0.25.x note in section 8.2), Twenty, SuiteCRM, OCA/contract.
- Medium [I]: Enterprise internals (no code read): the exact model names for subscription/rental data, the log model behind MRR reports, how `sale_renting` stores reservations. Flow table rows marked [G] (`stock_account` valuation, perpetual COGS entries, routes/MTO) rely on prior Odoo knowledge.
- Field lists for `crm.lead` and `sale.order` came partly from a summarising fetcher over truncated files; they match the raw-file greps I ran but are not guaranteed exhaustive (e.g. mixin-added fields).
- Not verified: Pipedrive pricing/features (HTTP 403); Chargebee tier mapping and plan/item data model (page text ambiguous); Attio Pro price (garbled digits); Salesforce mapping of new tier names to Enterprise/Unlimited; Lago charge-model pages and dunning-campaign docs (several doc URLs returned 404 after a site move, information above is from the plan-model, assign-plan, quotes, invoicing and llms.txt-listed pages only); Lago's and Kill Bill's real-world scale limits; Odoo Rental late-fee handling; Odoo 20 feature changelog beyond what the source reveals (the "What's new" page was not reachable).
- Pricing figures are snapshots of regional pages (Stripe: Swedish site; Odoo: EUR) and move frequently; star counts and dates are GitHub API values on 2026-10-01.

---

## Implications for obop

**MUST**

1. **MUST adopt Odoo's clean-room boundary explicitly**: read only Community (LGPL-3) code and CC-BY-SA docs; treat Subscriptions, Rental, Commissions, Amazon/marketplace connectors as "specify from public docs and competitor prior art". Add a CONTRIBUTING rule banning Enterprise code or screenshots. Shipping subscriptions, rental and commissions in obop's open core is a competitive wedge, because they are Odoo's paid tier.
2. **MUST use one shared `partner` identity table** referenced by CRM, sales, accounting, inventory and portal, with a materialised `commercial_partner_id`. Keep Odoo's commercial-field sync (tax id downwards), but store postal addresses as typed child rows (`invoice`/`delivery`/`other`) separate from contact persons, to avoid Odoo's `_fields_sync` complexity.
3. **MUST implement the template/variant product model** with three variant modes (always, dynamic, never), attribute exclusions, per-value `price_extra`, a partial unique index on (template, combination key) for active variants, a variant-explosion guard, and product kinds goods/service/combo with a storable flag. Keep "optional products" as lines under a section flagged `is_optional` on the same line table (Odoo 19/20 design), not a side table.
4. **MUST build the pricing engine as a standalone, deterministic Rust crate** with an explain trace: rule applicability by product/category/global, min quantity, validity dates, UoM, currency, base = list/cost/other pricelist (depth-bounded), formula with rounding, surcharge, margin clamp, and Odoo's four-level tie-break. Property-test it. Expose discounts to the customer as a discount percentage, as Odoo does.
5. **MUST model the sale order as document plus line tree** (sections/subsections/notes/optional/down-payment/combo child lines) with per-line `qty_delivered`, `qty_invoiced`, `qty_to_invoice` and derived statuses; keep order `state` (draft/sent/sale/cancel) separate from the `locked` flag and from `invoice_status`/`delivery_status`.
6. **MUST wire quote-to-cash through domain events via a transactional outbox**: `OrderConfirmed` -> inventory procurement, project/task creation, CRM revenue update; `DeliveryDone` -> delivered qty; `InvoicePosted`/`CreditNoteIssued` -> invoiced qty; `PaymentSucceeded` -> order confirmation and optional auto-invoice. Persist a sale-line to invoice-line link table so quantities are recomputable.
7. **MUST make subscriptions a first-class aggregate separate from the sales order**: plan, items, anchor, trial, status machine (adopt Stripe's vocabulary: trialing, active, past_due, unpaid, paused, canceled, plus incomplete for first payment), renewal and upsell as linked change-events, and a pure `bill(subscription, as_of) -> invoice lines` function with dry-run preview.
8. **MUST ship dunning in core** (retry schedule, customer notices, terminal action cancel/unpaid/past_due/pause, manual hold, hard-decline handling). Odoo only flags "Contract in exception"; Stripe includes retries free; Lago sells dunning as an add-on.
9. **MUST NOT depend on Odoo-style IAP services** for lead mining, enrichment or partner autocomplete. Define provider traits (enrichment, address/VAT validation, lead sources) with pluggable adapters and user-supplied API keys.
10. **MUST use exact decimal money** (`rust_decimal`) with per-currency rounding and store the rounding method per order (`round_per_line` vs `round_globally`); use a civil-date library for billing anchors with month-end clamping.

**SHOULD**

11. **SHOULD keep lead and opportunity in one table with a `type` flag** (Odoo-proven), plus `probability` + `probability_auto` + `auto_mode`, won stages (`is_won`) forcing 100 %, lost = archived with reason, merge of 2 to 5 records with activity/attachment transfer, and e-mail duplicate detection.
12. **SHOULD implement predictive lead scoring as per-team naive Bayes** on stage, team and configurable variables (country, source, language, tags, email/phone quality), with +0.1 smoothing and an incrementally updated frequency table, plus an "explain" tooltip. It is cheap in Rust and needs no ML runtime.
13. **SHOULD implement capacity-based lead assignment** (per-member 30-day quota, domain filters, preferred domain, opt-out, dedupe-before-assign) and the forecast as expected revenue x probability grouped by expected closing month; store recurring revenue with a plan in months so pipeline MRR is available before a deal is won.
14. **SHOULD add typed, user-defined properties (JSONB with a definition table)** on partners, products, leads and teams (Odoo `Properties`, Twenty/Attio custom fields), so vertical needs do not require schema changes.
15. **SHOULD maintain an append-only subscription event and MRR-movement ledger** and build MRR, ARR, churn, net revenue retention and cohort reports from it. Choose proration semantics explicitly; prefer Stripe's "flexible" rule (credit the last amount actually billed) over "classic".
16. **SHOULD model rental as period-aware order lines** with reservations guarded by exclusion constraints, duration-based price tiers reusing the pricelist crate (default rule: single cheapest tier, as Odoo), padding time, pickup/return events as stock moves, and deposits as a paired service product with credit-note refund. Add late fees as an explicit, documented rule (Odoo docs do not show one).
17. **SHOULD implement commissions as plan x salesperson x period** over invoice/order events, with target levels (0 to 300 %), the Odoo metric set (sold/invoiced amount and quantity, margin, MRR), period freeze and clawback on credit notes.
18. **SHOULD render quotations with a template-driven PDF pipeline** (candidate: `typst` crate, Apache-2.0) and keep Odoo's template concepts: quotation and section templates, validity days, signature and prepayment defaults, embeddable PDFs. Online signature plus prepayment-percent confirmation is table stakes.
19. **SHOULD expose everything through an API and MCP surface** from day one (Twenty ships an MCP server in its repo, Lago lists an MCP server under Premium), since CRM users now expect agent access.

**COULD**

20. **COULD add Lago-style usage metering** (events, billable metrics with SUM/COUNT/MAX/UNIQUE/weighted-sum aggregation, graduated/volume/package tiers) as phase two, Postgres-first with an optional columnar store; Kill Bill's CONSUMABLE/CAPACITY distinction is a good minimal design.
21. **COULD add a Lago-like quote -> order form -> order lifecycle** (versioned quotes, frozen order form, signature) to the core sales flow; Odoo offers "Create Alternative" quotations but, as far as the docs read, no frozen order-form stage [I].
22. **COULD offer an Odoo CSV/JSON import** (partners, products, pricelists, open opportunities) to lower switching cost.
23. **COULD embed an AI dunning/collections agent** later (Lago's beta design: deterministic policy engine, LLM only drafts, human approval above a risk threshold), after the deterministic campaign exists.

**Suggested build order** [I]: partner -> catalog/variants -> pricelist crate -> sale order + invoice link -> CRM (pipeline, PLS, assignment) -> payments/portal/e-sign -> subscriptions + dunning + MRR ledger -> rental -> commissions -> usage billing.

---

## Verification log

Adversarial re-check on 2026-10-01 (primary sources: raw files of `odoo/odoo` 20.0, 19.0, 18.0, 17.0 and `odoo/documentation` 20.0, GitHub REST API via `gh`, crates.io API, Maven Central metadata, vendor pricing and docs pages).

**Confirmed: 53 atomic claims as stated.** All 12 flagged claims were checked; 10 held (Odoo Online prices with the nuance in correction 6; Odoo 20 release notes, `version_info` and default branch; LGPL-3 and CC-BY-SA-4.0; the six 404 and nine 200 manifests; automatic-payment failure behaviour, provider list and absence of any retry/dunning text in the subscriptions docs; PLS naive Bayes with +0.1, stage/team always on, per-team `crm_lead_scoring_frequency`, prorated revenue; pricelist priority and formula; Odoo Rental cheapest-single-line pricing, min quantity and deposits; Stripe 0.7 %, per-second proration, 8 tries in 2 weeks, post-retry options, flexible billing mode; Lago AGPL-3.0, v1.53.0 on 2026-09-08 and add-on list, Kill Bill Apache-2.0, 8,8,8 retries and overdue state machine; Twenty licence structure, v2.44.0 on 2026-10-01 and USD 9 / 19 / 50k pricing). Two flagged claims needed correction or qualification (below): the `sale.order.option` removal version (correction 1) and Kill Bill's "latest" version (correction 4). Also confirmed beyond the flagged list: Odoo majors 19/18/17 dates and the 19.4 saas entry, Odoo 54.8k stars, `product.template.type` consu/service/combo with `is_storable`, order `document_tax_mode` new in 20.0, lead merge 2 to 5 records, member `assignment_max` semantics, commission levels 0 to 300 %, subscription report names/measures/statuses, recurring-plan fields, upsell and close rules, Stripe Swedish-page tiers and portal fee, Stripe subscription statuses and status resolution, Stripe custom retries (up to 3), Lago proration example (22/31 x 50 = 35.48), Lago Dunning Agent beta, Lago 10.6k stars, Kill Bill 5.8k stars, Kill Bill catalog/alignment/cancel/usage/invoice-item types and late-usage window of 2 periods, HubSpot, Salesforce, Attio and Chargebee prices as read, SuiteCRM 8.10.2 and 7.15.2 on 2026-07-31, OCA/contract AGPL-3.0, Twenty 57.8k stars and stack, `macro-inc/macro` 4.5k stars, and all 11 crate versions plus the 3 licences in the Rust notes (crates.io).

**Corrections**

| # | Was | Now | Source |
|---|---|---|---|
| 1 | Odoo 20 removed `sale.order.option`; optional products are lines with `is_optional = True` (quantity 0 until added) | The model was removed in Odoo 19.0 (file exists on 17.0/18.0, 404 on 19.0/20.0). `is_optional` is a flag on section/subsection lines; lines directly under an optional (sub)section are optional (`_is_line_optional`). Quantity-0 behaviour not confirmed. Flagged claim partly wrong (version and flag placement) | https://github.com/odoo/odoo/blob/20.0/addons/sale_management/models/sale_order_line.py ; raw `addons/sale_management/models/sale_order_option.py` on 18.0 vs 19.0 |
| 2 | OCA `contract` "has a 19.0 and 20.0 branch" | 20.0 branch exists but has no addons; the `contract*` modules are on the default 19.0 branch | https://github.com/OCA/contract (contents API, both refs) |
| 3 | "Nothing comparable in Rust exists ... obop is greenfield here" | `meteroid-oss/meteroid`: Rust, AGPL-3.0, 1.24k stars, open-source pricing and billing infrastructure, pushed 2026-09-18. Greenfield only for the integrated ERP+CRM+billing case | https://github.com/meteroid-oss/meteroid |
| 4 | Kill Bill latest is 0.24.22 (2026-09-29) | 0.24.22 is the latest GitHub Release (tag dated 2026-09-28), but Maven Central lists 0.25.4 as latest, tags reach killbill-0.25.5 and `master` is 0.25.6-SNAPSHOT; status of the 0.25 line unverified | https://github.com/killbill/killbill ; https://repo1.maven.org/maven2/org/kill-bill/billing/killbill/maven-metadata.xml |
| 5 | PLS optional variables: Country, Phone/Email Quality, Source, Language, Tags | Also `State` (seven optional variables) | https://github.com/odoo/documentation/blob/20.0/content/applications/sales/crm/track_leads/lead_scoring.rst |
| 6 | Odoo Online prices presented as plain list prices; Studio/multi-company only on Custom | Prices are introductory ("valid for 12 months, for initial users ordered"); list prices are 24.90/31.10 (Standard) and 44.90/56.90 (Custom); FAQ says the free plan allows multiple companies and Studio as its one app; Light User EUR 7.90 | https://www.odoo.com/pricing-plan |
| 7 | `rotting_threshold_days` "new" (implied Odoo 20) | Added in 19.0 (absent in 18.0) | raw `addons/crm/models/crm_stage.py` 18.0/19.0/20.0 |
| 8 | `delivery_status` "New stored" field in 20.0 `sale` | Existed in `sale_stock` in 18.0 and 19.0; moved to core `sale` in 20.0 | raw `addons/sale/models/sale_order.py`, `addons/sale_stock/models/sale_order.py` |
| 9 | Stripe: "no retry on hard-decline codes" | Retries stay scheduled but execute only after a new payment method is detected | https://docs.stripe.com/billing/revenue-recovery/smart-retries |
| 10 | Kill Bill overdue actions: "cancel, block changes, pause" | Cancellation policy, block changes, disable entitlement (no pause action in docs) | https://docs.killbill.io/latest/overdue |
| 11 | Lago statuses "pending, active, terminated"; charge models "[G, doc page not reachable]" | Statuses: pending, active, terminated, canceled, incomplete; charge models: standard, graduated, package, percentage, volume, graduated_percentage, custom, dynamic; aggregations also include CUSTOM | https://github.com/getlago/lago-api (`subscription.rb`, `charge.rb`, `billable_metric.rb`) |
| 12 | Rental padding time and pickup/return times apply generally | Padding Time shows only with Periodicity = Hours; Pickup/Return times show for every Periodicity except Hours | https://github.com/odoo/documentation/blob/20.0/content/applications/sales/rental/configure_products/products.rst |
| 13 | HubSpot lead scoring "5 on one tier, 10 on a higher one (unclear)" | Professional 5, Enterprise 10 (summarising fetcher, kept as [I]) | https://www.hubspot.com/pricing/sales |
| 14 | Enterprise-only status of Subscriptions/Rental inferred from missing manifests | Confirmed by Odoo's editions table (Community crossed, Enterprise ticked for Subscriptions, Rental, Amazon Connector); Commissions not listed there, so still inferred | https://www.odoo.com/page/editions |
| 15 | Lago premium/open-source boundary stated as add-on list only | Add-ons confirmed, but the pricing page also lists prepaid credits, credit notes, invoice preview, SSO and MCP server under Premium without an OSS/premium split; marked unverified | https://www.getlago.com/pricing |

**Unresolved / unverified**

- Pipedrive pricing and features: pricing page returns HTTP 403 (still unverified).
- Attio Pro price (EUR 74 annual / 93 monthly) and Chargebee tier labels for "$0 + 0.80 %" and "$99 + 0.65 %": page text is ambiguous; readings are inferences.
- Salesforce mapping of Core/Advanced to the older Enterprise/Unlimited names; HubSpot lead-scoring tier mapping (summariser output only).
- Kill Bill 0.25.x: whether it is a development or supported line.
- Sales Commissions being Enterprise-only (inferred from absence of `sale_commission` in Community; not on the editions page).
- Whether optional lines in Odoo 20 start at quantity 0 on the portal; full list of rental Periodicity options.
- Lago: which Premium-grouped features are also in the AGPL edition; Twenty "no quoting, no invoicing" claim was not re-checked.
- Stripe and Odoo prices are regional snapshots (Stripe Swedish page in SEK; Odoo page served in EUR to a Swedish IP) and may differ by region.
- Enterprise internals (model names for subscriptions, rental reservations, MRR log) remain docs-only by design.
