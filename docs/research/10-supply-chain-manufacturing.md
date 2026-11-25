# 10 - Supply chain and manufacturing: inventory, purchase, MRP, PLM, quality, maintenance, barcode, repairs

Research date: 2026-10-01. Evidence tags: **[V]** verified live this session from a primary source, **[I]** my inference from verified facts, **[K]** recalled from training knowledge and not re-verified (treat as a lead, not a fact).

## Scope & method

Primary sources read this session: the Odoo Community source (`odoo/odoo` branches 19.0, 20.0, master, fetched from raw.githubusercontent.com, so model and field names below are read from code), the Odoo documentation repo (`odoo/documentation`, branch 20.0), ERPNext source and release notes (`frappe/erpnext`), InvenTree source and releases, carrier and vendor pricing pages, PostgreSQL docs, IFRS and FDA pages. Repository metadata came from the GitHub API through a read-only `gh` session.

Limits: no general web-search tool was available in this environment, so discovery was by direct URL and GitHub API. Several pages were unreachable (UPS and FedEx developer portals timed out or denied access, GS1, NetSuite and FASB returned 403, SAP Help returned no content), so those areas are labelled [K] or [I]. One fetched page (Medusa docs) contained a block of text addressed to summarising agents asking to reproduce a feedback section; it was treated as data and ignored.

---

## 1. State of the reference (Odoo) today

| Fact | Evidence |
|---|---|
| Odoo 19.0 was released 2025-09-11; Odoo **20.0 was released 2026-09-17** (commit "[REL] 20.0"); `master` is now 20.1 alpha. [V] Both dates are the UTC timestamps of the `[REL]` commits on the release branches (19.0: 2025-09-11T07:02Z; 20.0: 2026-09-17T06:48Z; master bumped to 20.1 alpha two seconds later); `version_info` on 20.0 is `(20, 0, 0, FINAL, 0)` and on master `(20, 1, 0, ALPHA, 1)`. | https://github.com/odoo/odoo/commits/20.0/odoo/release.py and https://github.com/odoo/odoo/commits/19.0/odoo/release.py |
| Community is LGPL-3; the Community tree is the open part, Enterprise lives in a private repo. [V] for LGPL | https://github.com/odoo/odoo/blob/20.0/LICENSE |
| Pricing (re-checked from the page HTML, 2026-10-01): with the Yearly toggle, Standard EUR 19.90 and Custom EUR 35.90 per user per month; both include "all apps". The page shows these as the discounted figure ("valid for 12 months, for initial users ordered") against a list price of EUR 24.90 and EUR 44.90. With the Monthly toggle: Standard EUR 24.90 (list 31.10), Custom EUR 44.90 (list 56.90). Also "One App Free" EUR 0 (one app, unlimited users, Odoo Online) and Light User EUR 7.90. Whether figures include VAT is not stated (unverified). [V] | https://www.odoo.com/pricing-plan |
| Official Odoo 20 release notes exist: page headed "Release Notes September 2026". Supply-chain items relevant here: ShipStation shipping connector added (docs added 2026-08-04); COGS updated retroactively when a product cost changes after delivery (landed cost, bill price differs from PO price); in perpetual accounting, accrual entries from the valuation closing report hit the valuation account directly instead of passing through stock variation; landed costs can be applied per product of a transfer; "Valuation without Inventory" (stock on hand and valuation without the Inventory app); improved upstream/downstream lot traceability report; flexible consumption setting removed (all MOs are flexible); "Under Repair" repair status removed; PLM ECO version update made optional; quality "Failure Location" on checks and "Vendor quality rate" in Purchase. [V] | https://www.odoo.com/odoo-20-release-notes |
| Implication: Odoo 20 is the moving target. The valuation and packaging models changed materially in 19 and again in 20 (see section 3). | section 3 |

### Community vs Enterprise in this domain

Determined from the public Community tree for 20.0 (https://github.com/odoo/odoo/tree/20.0/addons). A module absent from the public tree is Enterprise or a separate repo [I].

| Area | Community (public source) | Not in Community tree [I] |
|---|---|---|
| Inventory, routes, reordering rules, lots/serials, packages, putaway, removal strategies, batch/wave/cluster | `stock`, `stock_account`, `stock_landed_costs`, `stock_dropshipping`, `stock_delivery`, `delivery` (20.0 folds `stock_picking_batch` into `stock`) | |
| Purchase link, sales link | `purchase`, `purchase_stock`, `sale_stock`, `purchase_requisition` (blanket orders, tenders) | |
| Manufacturing | `mrp`, `mrp_account`, `mrp_subcontracting`, `mrp_subcontracting_account`, `mrp_subcontracting_dropshipping` | Shop Floor, OEE UI, MPS, work-center/maintenance link |
| Repairs | `repair`, `mrp_repair` | |
| Maintenance | `maintenance`, `stock_maintenance`, `hr_maintenance`, `fleet_maintenance` | `mrp_maintenance` (work-center equipment blocking) |
| Barcode | `barcodes`, `barcodes_gs1_nomenclature` (nomenclature engine only) | `stock_barcode` app |
| Quality, PLM, MPS | none | `quality*`, `mrp_plm`, `mrp_mps` |
| Carrier connectors | `delivery` base (fixed price, rule-based); only `delivery_mondialrelay` in 19.0, and no `delivery_*` carrier module at all in the 20.0 public tree [V, listing diffed 2026-10-01] | DHL Express, FedEx, UPS, USPS, Sendcloud, EasyPost, Bpost, Shiprocket, Starshipit, Envia, ShipStation (new in 20) |

Connector list and regions as documented: https://github.com/odoo/documentation/blob/20.0/content/applications/inventory_and_mrp/inventory/shipping_receiving/setup_configuration/third_party_shipper.rst [V] (regions: DHL Express, Envia, FedEx, UPS all; USPS US; Sendcloud some European countries; Bpost Belgium; EasyPost North America; Shiprocket India; Starshipit Australia and New Zealand). ShipStation has its own page, `shipstation.rst`, and is not in that table. Note the business consequence: the Odoo features that are hardest to replicate (quality, PLM, MPS, barcode UI, carrier connectors) are exactly the ones Odoo keeps proprietary. ERPNext ships MPS/MRP and quality inspection under GPL (section 5).

---

## 2. Odoo data model (read from 20.0 source)

### 2.1 Entities

| Model | Role and key fields (20.0) |
|---|---|
| `product.product/template` | `is_storable` flag on `type=consu`, `tracking` (none/lot/serial), `lot_valuated`, `route_ids`; `product.category` carries `property_cost_method` (standard/average/fifo) and `property_valuation` (`periodic` "Periodic (at closing)" or `real_time` "Perpetual (at invoicing)"; both fields are defined in the `account` module, `stock_account` adds `fifo`, and the company has an `inventory_valuation` default, which is `periodic`). [V] |
| `uom.uom` | Reworked in 19/20: a tree via `relative_uom_id` + `relative_factor`, computed absolute `factor`; **rounding uses one global "Product Unit" decimal precision**, not per-UoM rounding. https://github.com/odoo/odoo/blob/20.0/addons/uom/models/uom_uom.py [V] |
| `stock.warehouse` | `reception_steps` (one/two/three steps) and `delivery_steps` (ship_only / pick_ship / pick_pack_ship); changing them regenerates locations, picking types, routes and rules. [V] |
| `stock.location` | `usage` in supplier, view, internal, customer, inventory (loss), production, transit; parent tree; storage category; cyclic count frequency. [V] |
| `stock.picking.type` | `code` incoming/outgoing/internal; `reservation_method` at_confirm / manual / by_date; `use_create_lots`, `use_existing_lots`. [V] |
| `stock.picking` | Document ("Transfer"); state is computed from its moves. |
| `stock.move` | The planned/actual movement of one product: `product_uom_qty` (demand, in move UoM), `product_qty` (demand in product UoM), `quantity` (reserved or picked qty), `picked`, `procure_method` (MTS/MTO), `move_orig_ids`/`move_dest_ids` (chaining), `rule_id`, `origin_returned_move_id`, `reference_ids`. |
| `stock.move.line` | Detail: lot, source package, `result_package_id`, owner, from/to location, `quantity`, `picked`. One move has many lines. |
| `stock.quant` | On-hand per (product, location, lot, package, owner): `quantity`, `reserved_quantity`, `in_date` (FIFO/FEFO age), plus count fields `inventory_quantity`. **No unique constraint**: duplicates are tolerated and merged later by `_merge_quants`. [V] |
| `stock.lot` | Lot/serial with expiry dates, custom properties, optional per-lot standard price when `lot_valuated`. |
| `stock.package`, `stock.package.type`, `stock.package.history` | 19/20 rename of `stock.quant.package`; nested packages, destination package, history. [V] |
| `stock.route`, `stock.rule` | Rule `action` pull / push / pull_push; `procure_method` make_to_stock / make_to_order / mts_else_mto; `picking_type_id`, src/dest location, `delay`, `auto` (manual or transparent). [V] |
| `stock.reference` | Replaces `procurement.group`: a named many-to-many between moves ("origin chain" of SO, MO, PO). [V] |
| `stock.warehouse.orderpoint` | `trigger` auto/manual, `product_min_qty`, `product_max_qty`, multiple, `route_id`, `qty_to_order`, `lead_days`, `snoozed_until`. |
| `stock.putaway.rule`, `stock.storage.category(.capacity)`, `product.removal` | Putaway and capacity; removal strategies are data rows (FIFO, LIFO, FEFO, closest location, least packages). |
| `stock.picking.batch` | Batch / wave / cluster picking; **moved into core `stock` in 20.0** [V: the 19.0 addons `stock_picking_batch` and `delivery_stock_picking_batch` are absent from the 20.0 addons list, and `addons/stock/models/stock_picking_batch.py` exists in 20.0]. |
| `product.value` (stock_account) | History of manual cost or move-value adjustments. |
| `stock.landed.cost`, `.lines`, adjustment lines | Split methods equal, by_quantity, by_current_cost_price, by_weight, by_volume. [V] |
| `mrp.bom`, `.line`, `.byproduct` | `type` normal or phantom (kit); subcontracting adds a subcontract type; by-products have `cost_share` (percent; total must not exceed 100). [V] |
| `mrp.routing.workcenter`, `mrp.workcenter`, `.capacity`, `.productivity`, `.productivity.loss` | Operations, work centers (`costs_hour`, `time_efficiency`, `oee_target`, per-product capacity), time tracking, downtime loss types. [V] |
| `mrp.production`, `mrp.workorder`, `mrp.unbuild`, `mrp.production.group` | MO, work order, disassembly, grouping of backorders. |
| `repair.order`, `repair.service.line` | Parts are ordinary `stock.move` rows with `repair_line_type` add / remove / recycle. [V] |
| `maintenance.equipment(.category)`, `maintenance.request`, `maintenance.team`, `maintenance.stage` | Corrective or preventive requests, `repeat_interval/unit`, computed MTBF, MTTR and estimated next failure. [V] |

Source root for all of the above: https://github.com/odoo/odoo/tree/20.0/addons/stock/models (and `mrp/models`, `stock_account/models`, `repair/models`, `maintenance/models`).

### 2.2 State machines (from code)

- `stock.move`: draft -> (waiting another move | confirmed/"waiting") -> partially_available -> assigned -> done; cancel from any pre-done state. [V]
- `stock.picking`: draft, waiting (another operation), confirmed, assigned (ready), done, cancel; **computed from its moves**. [V]
- `mrp.production`: draft -> confirmed -> progress -> to_close -> done / cancel; a separate `reservation_state` (waiting, ready, waiting another operation) and `components_availability_state` (available, expected, late, unavailable). [V]
- `mrp.workorder`: blocked -> ready -> progress -> done / cancel. [V]
- `repair.order`: draft, confirmed, done, cancel. `maintenance.request`: user-defined kanban stages. [V]

### 2.3 How routes produce documents

1. A sale confirmation, orderpoint, MO component line or PO return raises a **procurement** (product, qty, UoM, destination location, origin, values dict) to `stock.rule.run()`.
2. Rule selection walks routes in priority (product, category, warehouse, then global; order recalled, [K]), picks a rule by destination location, and calls the rule-type runner: pull-from-stock makes a move in a picking, `buy` makes/extends a PO line (merging by vendor and date), `manufacture` makes an MO.
3. A rule with `procure_method = make_to_order` makes the move wait on a chained upstream move created by the next rule; `mts_else_mto` only chains when stock is short. Push rules fire when a move is done and create the next move.
4. Warehouse steps are pre-built rules: 3-step delivery = Pick (stock to packing), Pack (packing to output), Ship (output to customer), chained with `move_orig_ids`.

Source: https://github.com/odoo/odoo/blob/20.0/addons/stock/models/stock_rule.py, https://github.com/odoo/odoo/blob/20.0/addons/purchase_stock/models/stock_rule.py (`_run_buy`, `_make_po_get_domain`). [V]

### 2.4 Replenishment and planning

- **Reordering rules**: run by the scheduler in batches of 1,000, each batch under a savepoint, retrying without failed rules; `trigger=auto` rules are picked up, `manual` ones appear in the replenishment report. [V] https://github.com/odoo/odoo/blob/20.0/addons/stock/models/stock_orderpoint.py
- **Lead times**: sales security lead time, shipping policy (earliest vs latest product), manufacturing lead time and MO security, vendor lead time, "days to purchase"; dates are scheduled backwards from the promised delivery. [V] https://github.com/odoo/documentation/blob/20.0/content/applications/inventory_and_mrp/inventory/warehouses_storage/replenishment/lead_times.rst
- **MPS** (Enterprise): manual, adjustable demand forecast per period (daily to yearly), suggests replenishment but does **not** create POs/MOs; the docs tell users not to combine MPS products with reordering rules. [V] https://github.com/odoo/documentation/blob/20.0/content/applications/inventory_and_mrp/manufacturing/workflows/use_mps.rst
- **Demand forecasting**: Odoo's forecast is arithmetic (on hand + incoming - outgoing per date), not statistical. [V for docs]

### 2.5 Automatic links to purchasing, sales, accounting

| From | To | Mechanism |
|---|---|---|
| Sales order line | Delivery moves | `sale_stock` `_action_launch_stock_rule` builds procurement values; `qty_delivered` computed from done outgoing moves minus returns. [V] |
| Purchase order line | Receipt moves | `purchase_stock` creates incoming moves; `qty_received` from done moves; receipt move valued at PO price (converted to company currency). [V] |
| Bill / invoice | Stock value | The bill or invoice becomes a value source for the move (section 3.6). |
| MO | Component and finished moves | Components: stock to production location; finished goods: production to stock; by-products likewise. |
| Subcontract PO | Hidden MO | Receipt from subcontractor triggers consumption of components held in the partner's internal subcontracting location. [V] |
| Dropship | Supplier -> customer move | Move with supplier source and customer destination is not an `is_in`/`is_out` move, so it is not part of inventory valuation (`is_valued` is false); `stock_account` `_action_done` still computes a `value` on it (`is_in or is_dropship`), presumably so COGS lines can use it [I]. [V for the code facts] |

---

## 3. The hard problems, and what the reference implementations do

### 3.1 Reservation

- Odoo reserves on **quants** through move lines: `_get_reserve_quantity` reads available = quantity - reserved_quantity over quants ordered by the removal strategy, clamps to what the move UoM can represent (convert to move UoM rounding DOWN, back to product UoM HALF-UP), then creates move lines and bumps `reserved_quantity`. [V] https://github.com/odoo/odoo/blob/20.0/addons/stock/models/stock_quant.py
- Reservation timing is per operation type: at confirmation, manual, or "before scheduled date" (N days, plus a shorter window for starred priority). [V]
- On validation, each line unreserves at the source, decrements the source quant and increments the destination quant; **if the source quant would go below its reservations, Odoo frees other moves' reservations (`_free_reservation`)**, i.e. a forced done move steals stock from a reserved one. [V] https://github.com/odoo/odoo/blob/20.0/addons/stock/models/stock_move_line.py
- ERPNext takes the other approach: a separate `Stock Reservation Entry` document (reserved_qty, delivered_qty, consumed_qty, status, batch/serial detail) and `Bin.reserved_qty`, off by default (`enable_stock_reservation = 0`). Reservation on Work Order and Production Plan arrived in v16. [V] https://github.com/frappe/erpnext/releases/tag/v16.0.0
- E-commerce inventories are simpler: Shopify defines mutually exclusive states so that `on_hand = available + committed + reserved + damaged + safety_stock + quality_control`, with `incoming` outside the sum. https://shopify.dev/docs/apps/build/orders-fulfillment/inventory-management-apps [V]. Medusa has `stocked_quantity`, `reserved_quantity`, `incoming_quantity` per item and location plus `ReservationItem` rows. https://docs.medusajs.com/resources/commerce-modules/inventory/concepts [V]

### 3.2 Concurrency and locking

- Odoo runs **REPEATABLE READ** (`sql_db.py`) and re-runs the whole request on serialization failures, with up to **5 attempts in total** (`MAX_TRIES_ON_CONCURRENCY_FAILURE = 5`, i.e. at most 4 retries) and `random.uniform(0, 2**i)` backoff where `i` is the attempt number. [V] https://github.com/odoo/odoo/blob/20.0/odoo/http/retrying.py
- Quant mutation takes `SELECT ... FOR NO KEY UPDATE SKIP LOCKED LIMIT 1` on the first matching quant (`try_lock_for_update(allow_referencing=True, limit=1)`); if that row is locked the transaction **inserts a new duplicate quant** instead of waiting, and `_merge_quants` (cron and after some operations) deduplicates. [V] https://github.com/odoo/odoo/blob/20.0/odoo/orm/models.py and `stock_quant.py`
- ERPNext on PostgreSQL serialises **all stock writes per (item, warehouse)** with a transaction-scoped advisory lock, because "postgres locking reads never see rows another transaction is inserting", and a second advisory gate serialises background reposts of the same pair. Its code comments state that without the gate two writers compute from the same stale previous ledger entry and one Bin write is lost. **Only on the `develop` branch**: the commits date from 2026-07-01 (repost gate) and 2026-07-16 (per-item-warehouse `transaction_advisory_lock`); the file at tag v16.37.0 (released 2026-09-30) and at `version-16` contains neither, so released v16 relies on row locks and deadlock/serialization retries. [V] https://github.com/frappe/erpnext/blob/develop/erpnext/stock/stock_ledger.py (checked against tags v16.0.0 and v16.37.0)
- PostgreSQL's own docs warn that `SKIP LOCKED` "gives an inconsistent view" and is meant for queue-like consumers; and that `ORDER BY` plus `FOR UPDATE` under READ COMMITTED can return rows out of order. https://www.postgresql.org/docs/current/sql-select.html [V]
- Current PostgreSQL: 18.6 (18 released 2025-09-25, EOL 2030-11-14); 19 is in beta 4 (2026-09-24); 14 goes EOL 2026-11-12. https://www.postgresql.org/support/versioning/ [V]

[I] Takeaway for a Rust design: a unique index on the quant key plus `INSERT ... ON CONFLICT DO UPDATE` removes Odoo's duplicate-quant/merge machinery; `UNIQUE NULLS NOT DISTINCT` (PostgreSQL 15+; syntax confirmed in the PostgreSQL 15 CREATE TABLE docs, https://www.postgresql.org/docs/15/sql-createtable.html [V]) handles nullable lot/package/owner columns. Use `SKIP LOCKED` only where falling back to "reserve less" is acceptable, and lock in a deterministic order to avoid deadlocks.

### 3.3 Units of measure

- Odoo stores every move three ways: demand in the move UoM, demand in the product UoM, and done quantity in the product UoM (`quantity_product_uom`). All comparisons go through `float_compare` with the global "Product Unit" digits; rounding is `UP` by default on conversion. Odoo uses **floating point** quantities. [V] https://github.com/odoo/odoo/blob/20.0/addons/uom/models/uom_uom.py
- Packaging quantities in sales/purchase lines survive onto moves (`packaging_uom_id`, `packaging_uom_qty`) and reservation can be forced in whole packagings (`packaging_reserve_method = full`). [V]
- [I] A Rust implementation should use exact decimals (`rust_decimal` 1.43.0, updated 2026-09-02, or Postgres `NUMERIC`), convert only at the document edge, and store base-UoM quantity as the single source of truth. https://crates.io/crates/rust_decimal [V for version]

### 3.4 Negative stock

- Odoo lets internal quants go negative when a move is forced done; the outbound move is valued at the **last known cost**, and when the receipt arrives the outbound value is corrected to the actual cost; the gap sits in "stock variation" in the meantime. [V] https://github.com/odoo/documentation/blob/20.0/content/applications/inventory_and_mrp/inventory/inventory_valuation/operations_valuation.rst
- ERPNext: `allow_negative_stock` defaults to 0 and `validate_negative_qty_in_future_sle` checks that a backdated entry does not make any later balance negative. [V]
- [I] Make negative on-hand a per-location-type and per-company policy (default off), never silent.

### 3.5 Multi-step routes

Chains of moves linked by `move_orig_ids`/`move_dest_ids`; a downstream move is `waiting` until its origin moves are done, and reservation of the downstream move is fed by the quantity actually done upstream (not by quant availability). Cancellation and split propagation is controlled by `propagate_cancel` on rule and move. Returns link back via `origin_returned_move_id`. [V] (`stock_move.py` lines around `move_orig_ids`, `propagate_cancel`). This chaining graph is the source of most Odoo edge-case bugs (partial backorders, qty changes after reservation); the 20.0 code still carries a `quants_cache` optimisation and a `_free_reservation` repair path. [V]

### 3.6 Inventory valuation

**Odoo 19/20 model (verified in code and docs).** `stock.valuation.layer` is gone. Value lives **on the `stock.move`** (`value`, `remaining_qty`, `remaining_value`, computed `is_in`/`is_out`) with `product.value` recording manual revaluations. A move is "in" when source is not a valued location and destination is; "out" for the reverse; internal moves do not change value; dropship and consigned-owner moves are excluded. [V] https://github.com/odoo/odoo/blob/20.0/addons/stock_account/models/stock_move.py

Value source priority for an incoming move, in order: manual adjustment, posted vendor bill or invoice, production cost, quotation (PO/SO price), return at original cost, previous company in an inter-company flow, product standard price; extra costs such as landed costs are then added on top (`_get_value_from_extra`) unless a manual value was used. [V] (`_get_value_data`, 20.0). Differences from 19.0, checked in its `stock_account/models/stock_move.py`: 19.0 has no "previous company" step (six sources, which is the list MUST #5 copies) and its `_set_value` takes `correction_quantity` and an `at_date` argument; the `recompute_date` replay below is the 20.0 form.

Costing methods (worked examples in the docs): [V] https://github.com/odoo/documentation/blob/20.0/content/applications/inventory_and_mrp/inventory/inventory_valuation/cheat_sheet.rst

| Method | Unit cost of an outbound move | Receipt at a different price |
|---|---|---|
| Standard | Fixed product cost, changed only by manual update (revalues on-hand qty) | Variance to a price-difference account |
| AVCO | Running total value / total qty (recomputed after every in/out move) | Updates the average |
| FIFO | Consumed from the oldest incoming moves' `remaining_qty`; each layer keeps its own cost | Stays in its own layer |
| LIFO | Only a removal strategy, not a valuation method; the docs note LIFO is not allowed under IFRS | n/a |

IAS 2 allows FIFO or weighted average for interchangeable items and specific identification otherwise. https://www.ifrs.org/issued-standards/list-of-standards/ias-2-inventories/ [V]. (The page does not state the LIFO ban or the standard-cost rule; those are [K].)

**Backdating.** When a move's date or quantity changes, Odoo replays valuation from the earliest affected date for the product (`_set_value(recompute_date=...)` then `_correct_inventory_valuation`). ERPNext does the same with a background `Repost Item Valuation` document (fields include `based_on`, `allow_negative_stock`, `via_landed_cost_voucher`, `recreate_stock_ledgers`, `current_index`, `error_log`). [V] Both admit that history is not immutable; they re-derive it.

**ERPNext's ledger model.** `Stock Ledger Entry` is append-only per (item, warehouse) with `actual_qty`, `incoming_rate`, `outgoing_rate`, `qty_after_transaction`, `valuation_rate`, `stock_value`, `stock_value_difference`, `stock_queue` (the FIFO queue, serialised), `serial_and_batch_bundle`, `posting_datetime`, `is_cancelled`; `Bin` is the projection (actual, reserved, ordered, indented, planned, projected, reserved-for-production, reserved-for-subcontract qty). Valuation methods in the settings schema: FIFO, Moving Average, LIFO in the released v16.37.0 (and v16.0.0 and v15); **Standard Cost exists only on `develop`** (commit "feat: Standard Valuation Rate (#56570)", 2026-06-28, not in any release yet); v16 lets each **company** pick a method. [V] https://github.com/frappe/erpnext/blob/develop/erpnext/stock/doctype/stock_ledger_entry/stock_ledger_entry.json and https://github.com/frappe/erpnext/releases/tag/v16.0.0

**Lot valuation.** Odoo can value per lot/serial (`lot_valuated`); new lots created by inventory adjustment take the product cost, adjustments to existing lots take that lot's latest value; product cost shown = total value / total qty. [V] https://github.com/odoo/documentation/blob/20.0/content/applications/inventory_and_mrp/inventory/inventory_valuation/valuation_by_lots.rst

### 3.7 Accounting entries: perpetual vs periodic

**Odoo 19+ (real change).** Before 19, perpetual posted journal entries on every stock move (via stock input/output interim accounts), which hurt performance, ledger readability and auditability. Since 19, "perpetual (at invoicing)" posts to the stock valuation account only when bills and invoices post, and a **closing entry** reconciles inventory value with accounting value; interim input/output accounts are no longer used and the docs give a migration procedure to clear their balances. [V] https://github.com/odoo/documentation/blob/20.0/content/applications/inventory_and_mrp/inventory/inventory_valuation/cheat_sheet.rst (section "Changes in Odoo 19") and https://github.com/odoo/documentation/blob/20.0/content/applications/finance/accounting/get_started/inventory_valuation.rst

| Event | Perpetual (at invoicing) | Periodic (at closing) |
|---|---|---|
| Vendor bill | Dr Stock Valuation, Cr Payable | Dr Expense, Cr Payable |
| Customer invoice | Dr Receivable, Cr Income; plus Dr COGS, Cr Stock Valuation (the "cogs" lines, written on the invoice itself from `_stock_account_prepare_cogs_vals`) | Dr Receivable, Cr Income |
| Receipt / delivery with no bill or invoice yet | Nothing until closing or accrual entries | Nothing until closing |
| Inventory adjustment | Dr/Cr Stock Valuation vs the Inventory Loss location's account | Variation at closing |
| Closing entry | Dr/Cr Stock Valuation vs **Stock Variation** for the gap; accrual entries then reclassify to Bills to Receive / GRNI, Invoices to Issue, COGS | Dr Stock Valuation, Cr Stock Variation (opening vs ending stock) |
| MO done | Components and finished goods revalue the valuation account against a "Cost of Production" account on the Production location | Variation at closing |

Options: closing entry manual, daily or monthly; standard-cost products can carry a price-difference account. Odoo 18 lacked Anglo-Saxon periodic and automated closing (the cheat-sheet's "Changes in Odoo 19" table: Periodic Anglo-Saxon "Not supported" in 18, "Fully supported" in 19; Periodic Continental "Manual closing" in 18, "Automated closing" in 19). [V] Odoo 20 refinement, from its release notes (https://www.odoo.com/odoo-20-release-notes): in perpetual accounting the accrual entries (Bills To Receive, Billed Not Received, Invoices To Be Issued, Invoiced Not Delivered) are generated from the valuation closing report and hit the valuation account directly instead of passing through the stock variation account, so the "Stock Variation for the gap" wording in the closing row above describes the 19 docs and may differ in 20 (the 20.0-branch docs still carry the 19 wording; unverified in code). [V for release notes]

**Classic real-time (ERPNext, Odoo <=18) [V for ERPNext]:** receipt Dr Stock-in-hand (warehouse account) / Cr **Stock Received But Not Billed**; purchase invoice Dr SRBNB / Cr Payable; delivery Dr COGS / Cr Stock (valued FIFO or moving average); perpetual is on by default in a new ERPNext site; each warehouse maps to an account, falling back to parent warehouse then company default. Charges flagged "valuation" (freight, duty) are booked as a negative expense so they are not double counted. https://docs.frappe.io/erpnext/perpetual-inventory

### 3.8 Landed costs

Odoo: a service product flagged "Is a Landed Cost" with a default split method sits on a vendor bill; "Create Landed Costs" makes a `stock.landed.cost` tied to **validated** receipts; "Compute" allocates by equal / quantity / current cost / weight / volume, producing valuation-adjustment lines (original value, additional cost, new value); it only applies to products in AVCO or FIFO categories, and posts in a dedicated journal. [V] https://github.com/odoo/documentation/blob/20.0/content/applications/inventory_and_mrp/inventory/inventory_valuation/landed_costs.rst. ERPNext v16 added vendor-invoice linking, a Landed Cost Report, and linking landed costs to Work Orders and Subcontracting Receipts so freight enters finished-goods cost. [V]

### 3.9 Lots, serials, expiry, traceability

- Odoo: tracking by lot or unique serial, picking-type flags `use_create_lots` / `use_existing_lots`; a move line with qty > 0 for a tracked product without a lot raises "You need to supply a Lot/Serial Number"; FEFO removal uses lot expiry dates. [V] (`stock_move_line._action_done`)
- ERPNext has **Serial and Batch Bundle** documents, with a setting (`use_serial_batch_fields`) to enter serial/batch directly on rows instead; v16 added a Serial and Batch Traceability report. [V]
- InvenTree stores one `StockItem` row per lot/serial/location with a `StockItemTracking` history and test results, rather than quants plus moves. [V] https://github.com/inventree/InvenTree/blob/master/src/backend/InvenTree/stock/models.py
- Regulation: the FDA FSMA 204 Food Traceability Rule had a 2026-01-20 compliance date; FDA proposed moving it 30 months to **2028-07-20** (proposed rule, Federal Register 2025-08-07; no final rule found in the Federal Register as of 2026-10-01) and says it intends to follow a Congressional directive (Continuing Appropriations, Agriculture, Legislative Branch, Military Construction and Veterans Affairs, and Extensions Act of 2026) not to enforce earlier. Records must be made available within 24 hours of a request. It requires traceability lot codes, Critical Tracking Events (seven, including shipping, receiving, transformation) and Key Data Elements, with records produced within 24 hours. https://www.fda.gov/food/food-safety-modernization-act-fsma/fsma-final-rule-requirements-additional-traceability-records-certain-foods [V] Design lot genealogy (forward and backward through MOs) early; retrofitting is expensive.
- GS1 barcodes: Odoo ships a default GS1 nomenclature (rules by Application Identifier, FNC1 `\x1D` terminates variable-length fields). [V] https://github.com/odoo/documentation/blob/20.0/content/applications/inventory_and_mrp/barcode/operations/gs1_nomenclature.rst. The AIs (00 SSCC, 01 GTIN, 10 lot, 17 expiry YYMMDD, 21 serial) all appear in Odoo's own rule table in that page [V]; the GS1 General Specifications themselves were not reached (unverified at source).

### 3.10 Manufacturing cost, WIP, subcontracting, kits

- MO cost (expected, from BoM) vs real cost (actual consumption, actual work-order time x work-center hourly rate plus employee hourly cost). Finished-good value = consumed component value + labour/overhead; example in docs: components 78 + 180 + 20 plus 200 labour = 478 (that example is in https://github.com/odoo/documentation/blob/20.0/content/applications/inventory_and_mrp/inventory/inventory_valuation/operations_valuation.rst, section "Finished goods", not in `mo_costs.rst`). [V] MO cost vs real cost, and the average-cost update (a $65 BoM cost plus a $70 real cost gives $67.50), are in https://github.com/odoo/documentation/blob/20.0/content/applications/inventory_and_mrp/manufacturing/basic_setup/mo_costs.rst. Odoo 20 release notes: the "MO Cost" field shows the provisional cost while in progress and the real cost once complete.
- WIP: optional manual WIP journal entries that accrue real cost on an open MO and are reversed on completion; default accounts are current-asset type. [V] (`work_in_progress.rst`)
- By-products carry `cost_share` percentages taking value out of the main product. Phantom (kit) BoMs explode into component moves; kit valuation is per unit of kit from BoM composition. [V] (`mrp_account/models/stock_move.py`)
- Subcontracting: subcontracting location is an **internal** location per subcontractor; components sent there stay company-owned (no value change); the finished good arrives on a receipt whose value = consumed components + service price on the PO. Variants: basic, resupply on order/manually, dropship-to-subcontractor. [V] (`operations_valuation.rst`, `mrp_subcontracting`)
- ERPNext v16 added phantom BoMs, subcontracting-inward orders (customer-supplied materials) and MPS/MRP views. [V]

---

## 4. Adjacent functional areas

| Area | Odoo behaviour | Notes |
|---|---|---|
| **Quality** (Ent.) | Control points create checks per Operation / Product / Quantity-percentage, frequency All / Random / Periodic / On-demand; check types Instructions, Take a Picture, Pass-Fail, Measure (norm plus from/to tolerance); quality alerts; failure locations. [V] https://github.com/odoo/documentation/blob/20.0/content/applications/inventory_and_mrp/quality/quality_management/quality_control_points.rst | ERPNext has Quality Inspection with templates and parameters in open source. [V] |
| **PLM** (Ent.) | Engineering Change Orders with types, stages, approvals, effective date, BoM or product version bump; ECOs can be raised from Shop Floor. [V] | No open-source peer at Odoo depth [I]. |
| **Maintenance** (Comm.) | Equipment, categories, teams, kanban stages, corrective vs preventive with recurrence, MTBF/MTTR. [V] | Small model (about 5 tables); linking downtime to work centers is Enterprise [I]. |
| **Repairs** (Comm.) | `repair.order` with parts add / remove / recycle as stock moves, service lines, warranty flag, invoice or sale-order generation. [V] | |
| **Barcode** (Ent. UI) | Scan to receive, pick, count; GS1; light-user licence. [V] | |
| **Batch / wave / cluster picking** | Batch = group of pickings; wave = parts of pickings split then batched; cluster = sorted into per-order totes while picking. [V] https://github.com/odoo/documentation/tree/20.0/content/applications/inventory_and_mrp/inventory/shipping_receiving/picking_methods | |
| **3PL / WMS** | Owner on quants (consigned stock excluded from valuation), storage categories with capacity, putaway rules, multi-step warehouses, packages. No 3PL billing, slotting, labour management, yard, or task interleaving in Community. [V]/[I] | Commercial WMS (SAP EWM, Manhattan, Oracle) add those [K]. |
| **Demand forecasting** | Arithmetic forecast plus manual MPS (Odoo); ERPNext's `Sales Forecast` DocType in develop only has `generate_manual_demand`, i.e. manual entry, with a weekly or monthly frequency. [V] https://github.com/frappe/erpnext/blob/develop/erpnext/manufacturing/doctype/sales_forecast/sales_forecast.py | Statistical forecasting is a gap in open source; `augurs` (Grafana, licensed "MIT OR Apache-2.0" per its Cargo.toml, 577 stars, last push 2026-09-30) is a Rust time-series library. https://github.com/grafana/augurs [V] |

### Shipping carrier connectors

| Provider | Facts |
|---|---|
| EasyPost | Free up to 3,000 labels (period not stated), per-label fee beyond that (amount not published), bring-your-own-carrier $20/month, insurance 1% min $1, tracking $0.01-0.03 per shipment, "100+" carriers. https://www.easypost.com/pricing [V]. Odoo connector covers North America only. |
| Shippo | Starter free, 30 labels/month; Pro from $17/month (1-200 labels), $0.08 per label past 10,000; "40+ carriers". https://goshippo.com/pricing [V] |
| Sendcloud | Free 20 parcels/month; Lite EUR 28 (+0.10/label, max 4,800 labels/year), Growth EUR 87 (+0.09, 12,000/year), Premium EUR 175 (+0.08, 120,000/year), Pro EUR 639 (+0.07, 360,000/year), overage an extra EUR 0.15/label; prices shown appear to assume yearly billing (page has a "Yearly (-20%)" toggle; monthly-billing prices not shown); full-support shipping origins in 8 countries (AT, BE, FR, DE, IT, NL, ES, UK), plus CZ, DK, PL, PT, SE with an invoice address in one of the countries above (not UK) and a direct courier contract; destinations worldwide per Sendcloud (Odoo's docs say "to any European country"). https://www.sendcloud.com/pricing/ [V] |
| DHL Express | MyDHL API, REST/JSON, Basic auth, current **v3.3.2 (2026-09-06)**; endpoints Rating, Product, Landed Cost, Shipment, Pickup, Tracking, Address, Identifier; test environment capped at 500 calls/day; needs a DHL Express account. https://developer.dhl.com/api-reference/dhl-express-mydhl-api [V] |
| UPS | Odoo requires the REST/OAuth "UPS" provider (client id and secret from a UPS developer account) and tells users to archive "UPS Legacy" methods. [V via Odoo docs] UPS's own portal timed out; details of legacy decommission dates are unverified. |
| FedEx | Odoo documents that new FedEx setups were **blocked from 2025-10-21** while Odoo completes "solution-provider validation" with FedEx, and the legacy provider stops working **2026-06-01** (that date has now passed); it points users to Sendcloud for FedEx in the meantime. The warning banner is still on the 20.0 page, but the page body was rewritten on 2026-05-23 and 2026-05-31 ("FedEx delivery connector v18+"): the connector is now activated from the Apps list and the FedEx account is registered inside Odoo (EULA acceptance plus MFA-style validation by recent invoice or PIN by email/SMS/voice call), so the new connector may be available again; the docs do not say so explicitly (unverified). [V via Odoo docs, https://github.com/odoo/documentation/blob/20.0/content/applications/inventory_and_mrp/inventory/shipping_receiving/setup_configuration/fedex.rst] FedEx's portal did not yield retirement dates (its API reference page returned only navigation text); treat direct multi-tenant FedEx as gated by a partner programme [I]. |
| Karrio | Open-source self-hosted multi-carrier shipping API (Python), latest v2026.1.32 (2026-06-23); LGPL-3 outside an `ee/` directory under a commercial licence. https://github.com/karrioapi/karrio [V] |

---

## 5. Comparison set

| System | Licence / status | Inventory model | Mfg and planning | Notable |
|---|---|---|---|---|
| **Odoo 20** | LGPL-3 Community + proprietary Enterprise; 20.0 released 2026-09-17 | moves + move lines + quants | BoM, MO, work orders, subcontracting; MPS and Shop Floor Enterprise | Value on moves since 19; accounting at invoicing plus closing. |
| **ERPNext 16** | GPL-3.0; v16.0.0 2026-01-12; latest v16.37.0 2026-09-30 | Append-only Stock Ledger Entry + Bin | BOM, Work Order, Job Card, Production Plan, MPS/MRP views, subcontracting, quality inspection | Perpetual inventory by default; repost on backdating; Postgres advisory locks per item-warehouse (`develop` only since 2026-07-16, not in v16.37.0). |
| **InvenTree 1.5** | MIT; 1.5.0 2026-08-11, 1.5.6 2026-09-26 | StockItem rows with tracking history | Build orders (statuses pending, production, on hold, cancelled, completed), allocation to BOM lines at build time, child builds | PurchaseOrder, SalesOrder, ReturnOrder, TransferOrder; no GL [I]. https://docs.inventree.org/en/stable/manufacturing/build/ |
| **Katana** | SaaS; Free (30 SKUs), Core from $299/mo, add-ons: traceability +$249, manufacturing +$199, WMS +$149; onboarding $2,000 | | Routings, planner, shop-floor app | https://katanamrp.com/pricing/ [V] |
| **MRPeasy** | SaaS $49/$69/$99/$149 per user per month (Starter/Professional/Enterprise/Unlimited; annual billing marked "1 month free"; Unlimited needs at least 2 users); MPS, barcode and multi-site only from the $99 tier | | MRP, finite scheduling, QC, revision control | https://www.mrpeasy.com/pricing/ [V] |
| **Fishbowl** | Inventory from $229/mo (Essentials tier, billed annually, 2 users; Growth $429, Scale $729); Advanced Manufacturing from $675/mo, Advanced Warehouse from $595/mo ("starts at", priced by users and deployment; the page calls its prices illustrative) | | BOM, work orders, MRP, landed costing, QuickBooks sync | https://www.fishbowlinventory.com/pricing [V] |
| **NetSuite WMS, SAP EWM, Manhattan, Blue Yonder** | Proprietary | Task-based warehouse execution (warehouse tasks, queues, handling units, waves, slotting, labour, yard) | | Pages unreachable this session; capability statements are [K]. |

Design lesson from the three open-source peers: Odoo is **document-driven with a quant projection**, ERPNext is **ledger-driven with a Bin projection**, InvenTree is **row-per-stock-item**. Only the first two have the discipline needed for valuation and traceability at ERP scale.

---

## 6. A minimal yet correct inventory + manufacturing core

The design below is my synthesis [I], built to keep Odoo's user-visible behaviour while removing its known weak points (duplicate quants, floats, replay-by-mutation).

### 6.1 Entities (phase 1)

`uom`, `product` (tracking, is_storable, cost_method, valuation mode, base uom), `product_packaging`, `location` (tree, usage enum as above), `warehouse` (steps generate rules), `picking_type`, `route`, `rule`, `transfer`, `move`, `move_line`, `quant`, `lot`, `package`, `reorder_rule`, `bom`, `bom_line`, `bom_byproduct`, `operation`, `work_center`, `production_order`, `work_order`, `valuation_entry`, `cost_state`, `revaluation`, `landed_cost`(+lines), `stock_reference`.

Critical constraints:
- `quant`: `UNIQUE NULLS NOT DISTINCT (company, product, location, lot, package, owner)`, `CHECK (reserved >= 0)`, `CHECK (quantity >= 0)` unless location type allows negative; all changes through one `apply_delta()` statement.
- `move`: `done` is terminal and immutable; corrections are new moves (returns, adjustments) plus `revaluation` rows, never edits of done rows. (Odoo allows unlocking and editing done pickings; that is the source of the replay machinery.)
- `move_line` of a done move is the audit ledger: every quant delta traces to exactly one line.
- Invariant checker (job): `quant.reserved = sum(open move_line.quantity)`, `quant.quantity = sum(done line deltas)`; Odoo has a `_clean_reservations` repair job for the same reason. [V]

### 6.2 Valuation

Immutable `valuation_entry(id, company, product, lot?, move_line_id, direction, qty, value, unit_cost, remaining_qty, effective_at, source_kind, cost_method_snapshot)`; FIFO consumption recorded explicitly in `fifo_consumption(out_entry, in_entry, qty, value)`; `cost_state(company, product[, lot])` holds running qty/value for AVCO. Writers lock `cost_state` for update (or an advisory lock keyed on company+product), the same granularity ERPNext proved necessary. A backdated change writes **adjustment entries** from a replay job instead of mutating history. Value source hierarchy copied from Odoo 19 (manual, bill, production, order price, return, standard). Accounting consumes `valuation_entry` through posting rules (section 6.4).

### 6.3 State machines

- Move: `draft -> confirmed -> {waiting | partially_available | assigned} -> done | cancelled`. `confirm` runs procurement for MTO and pull rules; `assign` reserves per picking-type policy; `validate` consumes picked lines, splits backorders, writes quant and valuation deltas in one transaction.
- Transfer state is derived from moves.
- MO: `draft -> confirmed -> progress -> to_close -> done | cancelled`; component moves (stock to production), finished move (production to stock), by-product moves with `cost_share`; work orders `blocked -> ready -> progress -> done`; cost = actual component value + sum(work-order minutes x work-center rate).
- Reorder: `scheduler -> procurement(product, qty, uom, location, origin) -> rule -> PO line | MO | transfer`, with a per-rule savepoint and failure log.

### 6.4 Automatic links

| Link | Rule |
|---|---|
| Sales -> stock | Confirmed SO line emits procurement; delivery done sets `qty_delivered`; returns subtract. |
| Purchase -> stock | Confirmed PO line creates expected receipt move; receipt done sets `qty_received`; receipt valued at PO price until a bill exists. |
| Bill / invoice -> valuation | Bill matched to receipt re-values the move (variance handling by cost method); invoice emits COGS lines from the delivered moves' value. |
| Stock -> accounting | Posting policy per company: `perpetual_at_invoicing` (Odoo 19 behaviour), `perpetual_per_transfer` (ERPNext/Odoo <=18, GRNI), `periodic`. One journal entry per validated transfer or MO, not per move line. |
| MO -> accounting | Component out, finished in, production/WIP account as counterpart, work-center cost absorbed. |

---

## Confidence & gaps

- **High confidence** [V]: all Odoo model/field/state names, locking and retry behaviour, valuation design in 19/20, release dates, Community module list, ERPNext schema and releases (the ERPNext advisory locking and the Standard Cost valuation method are `develop`-only, not in released v16.37.0), InvenTree releases, carrier pricing pages (EasyPost, Shippo, Sendcloud), DHL MyDHL version.
- **Medium**: Community vs Enterprise split is inferred from absence in the public tree (Enterprise source is private); Odoo pricing figures are now read exactly from the page HTML, but VAT treatment is not stated; EasyPost's free tier period and per-label fee are not published on its pricing page.
- **Low / unverified**: UPS and FedEx direct API terms, retirement dates and partner programmes (portals unreachable or returned no body text; only Odoo's docs for timing, and the FedEx docs are internally inconsistent after their 2026-05 rewrite); SAP EWM, NetSuite WMS, Manhattan feature lists; the GS1 General Specifications (only Odoo's AI table was read). The Odoo 20 release notes do exist (https://www.odoo.com/odoo-20-release-notes, "Release Notes September 2026"; the `/page/...` URLs are the ones that return 404) but the Inventory and Manufacturing items were only skimmed; the file-set and code comparison remains the main basis for 20.0 claims.
- **Not done**: no runtime benchmark of quant contention; no review of Odoo OCA modules; no survey of Rust ERP prior art beyond a GitHub search (top results were small hobby projects of at most about 40 stars, e.g. `forgeerp` 40, `idEAS` 27, `microscaler/rerp` 23, `kovacsdavid/obvia` 16, nothing production grade; re-run 2026-10-01, the earlier "at most 23" figure was out of date) [V].

---

## Implications for obop

### MUST

1. **Adopt Odoo's three-layer movement model (transfer -> move -> move line) with a quant projection**, because it is the only shape that cleanly carries reservation, lots/serials, packages, ownership and multi-step chaining, and it matches what users of Odoo expect. Diverge on implementation: enforce a unique index on the quant key and mutate via `INSERT ... ON CONFLICT DO UPDATE`, so Odoo's duplicate-quant/`_merge_quants` workaround never exists.
2. **No floats for quantities or money.** Exact decimals end to end (`rust_decimal`/Postgres `NUMERIC`), base-UoM quantity as the source of truth, conversion and rounding at document edges only. Odoo's global "Product Unit" precision plus `float_compare` is a design to avoid, not copy.
3. **Make done stock moves immutable**; corrections are new moves and revaluation rows. Backdated changes trigger a replay job that writes adjustment entries (both Odoo 19 and ERPNext ended up with replay machinery; make it explicit and auditable).
4. **Serialise valuation per (company, product[, lot])** with a row lock or transaction advisory lock, as ERPNext's `develop` branch added for PostgreSQL in July 2026 (released v16 does not have it). Use deterministic lock ordering for quant updates; use `SKIP LOCKED` only where "reserve less" is an acceptable fallback (PostgreSQL docs call it an inconsistent view).
5. **Model valuation independently of accounting**: immutable valuation entries carrying qty, value and remaining qty; FIFO consumption recorded explicitly; standard, AVCO and FIFO per product category; lot-level valuation as an option. Use Odoo 19's value-source priority (manual, bill, production, order price, return, standard).
6. **Implement stock rules and routes as data** (pull/push/pull_push, MTS/MTO/mts_else_mto, delay, picking type) and generate 1/2/3-step reception and delivery from warehouse settings, plus reordering rules with a savepoint-per-rule scheduler.
7. **Lots, serials, expiry (FEFO) and forward/backward genealogy from day one.** FSMA 204 enforcement is not before 2028-07-20 but EU/US customers in food, pharma and electronics already ask for it.
8. **Keep done/validate atomic**: one transaction updates move lines, quants, valuation entries and triggers the accounting posting job; everything idempotent so retries are safe (Odoo re-runs the full request, up to 5 attempts in total).

### SHOULD

9. **Accounting posting as a company policy with three modes**: perpetual at invoicing (Odoo 19/20 behaviour, the clone target), perpetual per transfer with a GRNI account (ERPNext and Odoo <=18, simpler to audit for small firms), and periodic with closing entry. Post one entry per validated transfer or MO, never one per move line.
10. **Core Manufacturing scope**: BoM (normal, kit/phantom, subcontract), by-products with cost share, routings and work centers with hourly cost, work orders with time tracking, MO backorders, unbuild, scrap, MO real vs expected cost, optional WIP entries, subcontracting with a per-partner internal location. Defer Shop Floor UI and OEE dashboards.
11. **Landed costs** with the five Odoo split methods, linked to vendor bills and allowed on MOs and subcontract receipts (ERPNext v16 did this).
12. **Reservation policy per picking type** (at confirmation, manual, by date), removal strategies (FIFO, LIFO, FEFO, closest, least packages) and packages/putaway/storage categories; store reservations at move-line level and run an invariant checker.
13. **Barcode**: store barcodes on product, packaging, lot, location, package; parse GS1-128 with a rule table (AI-driven, FNC1 aware). The scanning UI can follow later; the nomenclature engine is Community in Odoo.
14. **Carrier layer as a trait** (`rate`, `create_label`, `cancel`, `track`, `pickup`), first adapters EasyPost and Sendcloud (open APIs, free tiers), then Shippo and DHL Express MyDHL (REST, v3.3.2). Treat UPS as an OAuth REST adapter. Do not promise direct multi-tenant FedEx before confirming FedEx's partner-programme terms (Odoo itself was blocked on it from 2025-10-21).
15. **Quality and maintenance as thin phase-2 modules** on the same event hooks: control points that create checks on move/MO/work-order events, pass-fail/measure/instruction/picture check types, quality alerts; maintenance equipment with preventive recurrence and MTBF/MTTR. Shipping these in the open core is a differentiator (Odoo keeps quality, PLM, MPS and carrier connectors proprietary while ERPNext ships MPS/MRP and quality inspection under GPL).

### COULD

16. **MPS/MRP planning run** (ERPNext v16 has it open source): netting of demand against stock, open POs/MOs and lead times, producing suggestions, not orders (Odoo's own stance).
17. **PLM / ECO** (versioned BoMs with effective dates and approvals) after core MRP is stable.
18. **Statistical demand forecasting** (seasonal exponential smoothing, etc.) as a differentiator, since both Odoo and ERPNext only offer manual forecasts; evaluate `augurs` (MIT OR Apache-2.0) or a service boundary.
19. **WMS depth**: wave/cluster picking, slotting, task queues, labour management, yard, 3PL billing. Odoo Community stops at batch/wave/cluster; a 3PL use case is the natural tier-2 upsell.
20. **Optional Karrio sidecar** for long-tail carriers (LGPL-3 core, Python; would be an external service to respect the Rust-only backend constraint), or ZPL/EPL label generation in-house.
21. **EPCIS/SSCC event export** for retailer and regulator traceability [K].

---

## Verification log

Fact-check pass dated 2026-10-01 (adversarial, independent sources where possible: GitHub API for commit/tag/file contents, odoo.com HTML, vendor pricing pages, FDA, Federal Register, PostgreSQL docs, crates.io).

**Confirmed as stated: 96 discrete claims.** This covers, among others: Odoo 19.0 (2025-09-11) and 20.0 (2026-09-17) `[REL]` commits and master at 20.1 alpha; Odoo LGPL-3 and the absence of quality, PLM, MPS, barcode UI and carrier modules from the 20.0 public tree; Odoo 19 "Perpetual (at invoicing)" plus closing entry, interim input/output accounts no longer used, Periodic Anglo-Saxon and automated closing new in 19; 20.0 valuation on `stock.move` with `product.value` and the `recompute_date` replay; quant `FOR NO KEY UPDATE SKIP LOCKED` and `_merge_quants`; REPEATABLE READ and exponential backoff; warehouse steps, location usage, picking-type reservation methods, move/picking/MO/work-order/repair states, scheduler batches of 1,000 with savepoint, landed-cost split methods, BoM types and `cost_share`, MTBF/MTTR, UoM global "Product Unit" precision; ERPNext GPL-3.0, v16.0.0 (2026-01-12), v16.37.0 (2026-09-30), the v16 feature list (MPS/MRP views, reservation on Work Order and Production Plan, per-company valuation method, phantom BOMs, landed cost linked to Work Order), Stock Ledger Entry and Bin fields, `enable_stock_reservation` and `allow_negative_stock` defaults, perpetual inventory default and SRBNB; InvenTree MIT, 1.5.0 (2026-08-11), 1.5.6 (2026-09-26), PurchaseOrder/SalesOrder/ReturnOrder/TransferOrder, build statuses; DHL MyDHL v3.3.2 (2026-09-06), REST/JSON, Basic auth, 500 test calls/day; PostgreSQL 18.6, 18 released 2025-09-25, EOL 2030-11-14, 14 EOL 2026-11-12, 19 beta 4 (2026-09-24), the `SKIP LOCKED` and `ORDER BY ... FOR UPDATE` warnings; FSMA 204 original date 2026-01-20, proposed 2028-07-20, Congressional non-enforcement directive, seven CTEs, TLC, 24-hour records; EasyPost, Shippo, Sendcloud, Katana, MRPeasy and Fishbowl prices; Karrio v2026.1.32 (2026-06-23) and its LGPL-3 plus `ee/` split; `augurs` 577 stars; `rust_decimal` 1.43.0 (2026-09-02); Shopify and Medusa inventory fields; IAS 2 cost formulas; Odoo FedEx and UPS documentation statements; Odoo connector regions; Odoo pricing (19.90 and 35.90 EUR, yearly toggle).

**Corrections (was / now / source)**

| # | Was | Now | Source |
|---|---|---|---|
| 1 | Odoo pricing "approximate, page text garbled" | Exact: Yearly toggle Standard 19.90 and Custom 35.90 EUR per user per month, shown as a 12-month discount off 24.90 and 44.90; Monthly toggle 24.90 and 44.90 (list 31.10 and 56.90); Light User 7.90; One App Free 0 | https://www.odoo.com/pricing-plan (HTML read directly) |
| 2 | "no official release-notes page: odoo.com release-note URLs returned 404" | Official page exists, headed "Release Notes September 2026" (only the `/page/...` variants 404). Relevant items added to section 1 | https://www.odoo.com/odoo-20-release-notes |
| 3 | Odoo "retries the whole request up to 5 times" (also MUST #8) | Up to 5 attempts in total (`MAX_TRIES_ON_CONCURRENCY_FAILURE = 5`, loop starts at try 1, so at most 4 retries) | https://github.com/odoo/odoo/blob/20.0/odoo/http/retrying.py |
| 4 | ERPNext per-(item, warehouse) advisory lock presented as ERPNext behaviour (sections 3.2, 5, MUST #4) | Only on `develop` (commits 2026-07-01 and 2026-07-16); absent from tags v16.37.0 and v16.0.0 and from `version-16` | https://github.com/frappe/erpnext/blob/develop/erpnext/stock/stock_ledger.py vs the same path at tag v16.37.0 |
| 5 | ERPNext valuation methods "FIFO, Moving Average, LIFO, Standard Cost" | Released v16.37.0, v16.0.0 and v15 offer FIFO, Moving Average, LIFO; Standard Cost is `develop` only (feat "Standard Valuation Rate (#56570)", 2026-06-28) | https://github.com/frappe/erpnext/blob/develop/erpnext/stock/doctype/stock_settings/stock_settings.json and the same path at tag v16.37.0 |
| 6 | Category fields `cost_method` and `valuation` | `property_cost_method` (standard, average, plus `fifo` from `stock_account`) and `property_valuation` (`periodic` or `real_time` labelled "Perpetual (at invoicing)"), defined in the `account` module; company `inventory_valuation` defaults to `periodic` | https://github.com/odoo/odoo/blob/20.0/addons/account/models/product.py and `company.py` |
| 7 | Value priority "quotation plus extra costs such as landed costs" as one source | Extra costs are added on top of any non-manual source; 19.0 has only six sources (no "previous company" step) and a different `_set_value(correction_quantity)` signature | https://github.com/odoo/odoo/blob/20.0/addons/stock_account/models/stock_move.py and the 19.0 version of the file |
| 8 | MO cost example "78 + 180 + 20 + 200 = 478" cited to `mo_costs.rst` | The example is in `operations_valuation.rst` ("Finished goods"); `mo_costs.rst` has a different example ($65, $70, average $67.50) | https://github.com/odoo/documentation/blob/20.0/content/applications/inventory_and_mrp/inventory/inventory_valuation/operations_valuation.rst |
| 9 | Dropship moves "excluded from valuation" | Excluded from inventory valuation (`is_valued` false) but `_action_done` still computes a `value` for them (`is_in or is_dropship`) | https://github.com/odoo/odoo/blob/20.0/addons/stock_account/models/stock_move.py |
| 10 | `augurs` licence Apache-2.0 (two places) | "MIT OR Apache-2.0" (Cargo.toml; both LICENSE files present) | https://github.com/grafana/augurs |
| 11 | Rust ERP prior art "at most 23 stars" | Up to 40 stars (`forgeerp` 40, `idEAS` 27, `microscaler/rerp` 23) on re-run | GitHub repository search, 2026-10-01 |
| 12 | Carrier connectors: "only `delivery_mondialrelay` in 19.0", list without ShipStation | No `delivery_*` carrier module in the 20.0 public tree at all; ShipStation connector is new in 20 (docs added 2026-08-04) | https://github.com/odoo/odoo/tree/20.0/addons ; https://www.odoo.com/odoo-20-release-notes |
| 13 | FedEx: legacy provider "stops on 2026-06-01" as a forward-looking fact | Date has passed; the same doc page was rewritten 2026-05-23 and 2026-05-31 (FedEx connector from Apps, in-Odoo account registration with EULA and MFA validation) while the 2025-10-21 warning banner remains; current operational status not stated | https://github.com/odoo/documentation/blob/20.0/content/applications/inventory_and_mrp/inventory/shipping_receiving/setup_configuration/fedex.rst |
| 14 | Sendcloud origins "8 countries", prices without billing basis | Plus CZ, DK, PL, PT, SE under conditions; listed paid prices appear to be yearly billing (Yearly -20% toggle); per-plan annual label caps and per-label fees added; Odoo's docs say destinations are "any European country" | https://www.sendcloud.com/pricing/ ; Odoo `third_party_shipper.rst` |
| 15 | GS1 AI table "[K]" | Verified against Odoo's own rule table (00, 01, 10, 17, 21; FNC1 `\x1D`); the GS1 General Specifications themselves still not reached | https://github.com/odoo/documentation/blob/20.0/content/applications/inventory_and_mrp/barcode/operations/gs1_nomenclature.rst |
| 16 | `UNIQUE NULLS NOT DISTINCT` (PostgreSQL 15+) "[K]" | Verified in PostgreSQL 15 docs | https://www.postgresql.org/docs/15/sql-createtable.html |
| 17 | Section 3.7 closing row implied accruals pass through Stock Variation | Odoo 20 release notes say perpetual accrual entries (Bills To Receive, Billed Not Received, Invoices To Be Issued, Invoiced Not Delivered) hit the valuation account directly; the 20.0-branch docs still carry the 19 wording (noted, not resolved in code) | https://www.odoo.com/odoo-20-release-notes |
| 18 | `stock.picking.batch` moved into `stock` "[I from file diff]" | Verified: `stock_picking_batch` and `delivery_stock_picking_batch` are gone from 20.0 addons, `stock/models/stock_picking_batch.py` is present | GitHub addons listings for 19.0 and 20.0 |
| 19 | FSMA 204 status without legal trail | Added: extension is still a proposed rule (Federal Register 2025-08-07, document 2025-14967, no final rule found); the enforcement bar comes from the Continuing Appropriations, Agriculture, Legislative Branch, Military Construction and Veterans Affairs, and Extensions Act of 2026 | https://www.fda.gov/food/food-safety-modernization-act-fsma/fsma-final-rule-requirements-additional-traceability-records-certain-foods ; https://www.federalregister.gov/documents/2025/08/07/2025-14967/requirements-for-additional-traceability-records-for-certain-foods-compliance-date-extension |

Also clarified without changing the claim: Fishbowl tier names and "illustrative" pricing note; MRPeasy annual-billing and minimum-user notes; Odoo release dates are `[REL]` commit timestamps.

**Unresolved / still unverified**

- UPS and FedEx direct API retirement dates and partner-programme terms: FedEx's developer page returned only navigation text, UPS's portal timed out, and the one search engine tried showed a bot check (not bypassed). Only Odoo's docs are available for timing.
- Whether Odoo's new-API FedEx connector is operational again after the 2026-05 doc rewrite (docs ambiguous).
- Whether the ERPNext `develop`-only advisory locks and Standard Cost valuation will ship in a v16.x or v17 release.
- How Odoo 20 code implements the accrual change (closing report entries hitting the valuation account directly); only the release notes were read, and the Inventory, Manufacturing and Quality release-note items beyond those cited were skimmed, not diffed against the file's other claims (for example "Simplified returns" and the removal of flexible consumption).
- Whether Odoo's EUR prices include VAT; EasyPost's free-tier period and per-label fee (unpublished); the GS1 General Specifications; SAP EWM, NetSuite WMS, Manhattan and Blue Yonder capability statements (left as [K]).
- Community-vs-Enterprise split remains an inference from absence in the public tree (Enterprise source is private); IAS 2's page does not state a LIFO ban (the Odoo docs assert it); "nothing production grade" for Rust ERP projects is judged from star counts only.
