# Retail, POS, eCommerce, Website Builder and Marketing: research for obop

Research date: 2026-10-01. Assignment 12 of the obop research set.

## Scope & method

WebSearch was not available in this run, so all research went through direct fetches of primary sources: the Odoo 20.0 source and documentation repositories (GitHub `odoo/odoo`, `odoo/documentation`, branch 20.0), npm, crates.io and GitHub release APIs, the rust-lang manifest, IETF/RFC editor, vendor developer docs (Stripe, Adyen, Star, Shopify, Amazon, Etsy, WooCommerce, fiskaly), regulator pages (AEAT, EU Commission), and the caniuse dataset. Secondary sources (Wikipedia on fiscalization) are labelled. Many pages were read through a summarising fetcher, so paraphrases of vendor pages carry some risk; load-bearing numbers were cross-checked against raw files where possible.

Labels: **[V]** verified in this run, **[I]** inference from verified facts, **[U]** not verified (recollection or guess). Odoo Community is LGPL-3 and may be studied; Enterprise-only features are proprietary and are described here only from public documentation, never to be copied.

## 1. Platform baseline that shapes every decision

| Item | State on 2026-10-01 | Source |
|---|---|---|
| Rust stable | 1.98.1, released 2026-09-03 [V]. The "2026-09-01" inside the manifest version string is the build commit date, not the release date. Rust 1.99.0 is dated 2026-10-01 in RELEASES.md but was not yet on the stable channel at 10:13 UTC on 2026-10-01, so re-check before pinning [V] | https://static.rust-lang.org/dist/channel-rust-stable.toml , https://blog.rust-lang.org/2026/09/03/Rust-1.98.1/ , https://github.com/rust-lang/rust/blob/master/RELEASES.md |
| Vite+ | **1.0.0 stable, 2026-09-28**, MIT; promoted from rc.1 with no code changes; bundles Vite 8.3.1, Rolldown 1.2.11, Vitest 5.0.1, Oxlint 1.85.0, Oxfmt 0.70.0, tsdown 0.23.0; CLI needs Node `^22.18 \|\| ^24.11 \|\| >=26` [V]. Independent check: npm `vite-plus@1.0.0` was published 2026-09-28 with licence field MIT, and the repo LICENSE is MIT (VoidZero Inc.). The bundled tool versions are pinned by the release and are not the newest standalone npm releases (on 2026-10-01: rolldown 1.2.12, vitest 5.0.3, oxlint 1.86.0, oxfmt 0.71.0) [V] | https://github.com/voidzero-dev/vite-plus/releases/tag/v1.0.0 , https://voidzero.dev/posts/announcing-vite-plus-1-0 , https://registry.npmjs.org/vite-plus , https://github.com/voidzero-dev/vite-plus/blob/main/LICENSE |
| Vite+ SSR story | Neither the announcement nor the guide mentions SSR; it is framework-agnostic and defers to Vite [V]. `vp pack` can emit standalone executables via Node SEA (needs Node 25.7+) [V] | https://viteplus.dev/guide/ , https://viteplus.dev/guide/pack |
| Vite | 8.x (Rolldown as single bundler), stable 2026-03-12 (npm `vite@8.0.0`, 8.3.1 is latest since 2026-09-24); **Environment API not yet stable**: the Vite 8 post lists stabilisation as a goal, and the current docs banner calls it "release candidate" with some specific APIs still experimental [V] | https://vite.dev/blog/announcing-vite8 , https://vite.dev/guide/api-environment |
| Odoo | **Odoo 20 listed as "Released September 2026"**; `odoo/odoo` default branch is `20.0` (LGPL-3, Python 3.12-3.14, PostgreSQL 16+); `release.py` on 20.0 already carries `version_info = (20, 0, 0, FINAL, 0, '')` [V] | https://www.odoo.com/page/release-notes , https://github.com/odoo/odoo/blob/20.0/odoo/release.py |
| Key Rust crates | axum 0.8.9, tokio 1.53.1, sqlx 0.9.0, lettre 0.11.23 (MIT, MSRV 1.85), mail-auth 0.13.3, mail-send 0.6.2, escpos 0.20.0 (MIT, MSRV 1.88), tauri 2.12.1, rusqlite 0.40.2, async-stripe 0.41.0 (latest stable; a 1.0.0-rc.9 pre-release exists since 2026-09-11), minijinja 2.24.0 (3.0.0-alpha.2 exists), mrml 6.0.1, serialport 4.10.1 (MPL-2.0), nusb 0.2.7 [V] | https://crates.io/api/v1/crates/<name> |

Consequence: the owner's stack is current. The gap is that Vite+ 1.0 is a toolchain, not a server-rendering framework, so SSR must come from a Vite-based framework or from our own design (section 7).

## 2. Functional reference: what Odoo 20 ships in this scope

Edition split is inferred from the Community `addons/` tree on branch 20.0: a module present there is Community (LGPL-3) [V]; one absent while documented is presumably Enterprise [I].

| Area | Community modules present [V] | Documented but absent from Community [I] |
|---|---|---|
| POS | `point_of_sale`, `pos_restaurant`, `pos_self_order`, `pos_loyalty`, `pos_discount`, `pos_hr`, `pos_sms`, `pos_online_payment`, terminals (`pos_adyen`, `pos_stripe`, `pos_mollie`, `pos_viva_com`, `pos_razorpay`, `pos_pine_labs`, `pos_qfpay`, `pos_mercado_pago`, `pos_bancontact_pay`, `pos_dpopay`, `pos_safaricom`), cash machines (`pos_cashdro`, `pos_cashmatic`, `pos_glory_cash`), `pos_imin`, `iot_drivers`, `iot_webserial` | Preparation-display SMS/WhatsApp extras; several fiscal modules (e.g. `l10n_de_pos_cert` via fiskaly, `l10n_at_pos`, `l10n_it_pos`) |
| eCommerce | `website_sale`, `website_sale_stock`, `website_sale_loyalty`, `website_sale_collect`, `website_sale_gelato`, `payment` + 23 `payment_*` provider modules (counted on 20.0, including `payment_demo` and `payment_custom`), `delivery`, `account_payment` | Marketplace connectors (Amazon, Shopee, Lazada, TikTok Shop; the 20.0 docs tree has no Shopify connector, Shopify appears only as a source for the website product importer, see 2.3) |
| Website | `website`, `website_blog`, `website_forum`, `website_slides`, `website_livechat`/`im_livechat`, `website_event*`, `website_mass_mailing`, `website_google_map`, `website_cf_turnstile` | Studio |
| Marketing | `mass_mailing`, `mass_mailing_sms`, `sms`, `sms_twilio`, `link_tracker`, `utm`, `loyalty`, `event*`, `survey`, `marketing_card` | Marketing Automation, Social Marketing, WhatsApp, Referrals |

Sources: https://github.com/odoo/odoo/tree/20.0/addons , docs tree https://github.com/odoo/documentation/tree/20.0/content/applications .

### 2.1 POS: features and data model

Docs structure (Odoo 20): Workflow, Products, Hardware and network (Local Network Access, IoT, receipt printers, electronic shelf labels, customer display, scale), Shop features, Restaurant features (floors, tables, booking, courses, fire courses, bill splitting, tips, UrbanPiper delivery), Extra features (multi-employee, preparation display, self-ordering QR menu/kiosk, presets, pricing), Payment methods (cash machines, customer account, QR, terminals), Reporting [V] (https://www.odoo.com/documentation/20.0/applications/sales/point_of_sale.html).

Odoo 20 POS additions worth cloning [V] (https://www.odoo.com/odoo-20-release-notes): multiple currencies at checkout in one POS; multiple printers per POS with per-printer selection; "split per product" preparation tickets; snoozing products; simplified receipts; service fees via presets; simplified inventory (no Inventory app, single on-hand quantity); convert order lines into combos; "Supervised" employee level; session-end global sale entry; order-specific QR for pay-after-meal; WhatsApp/SMS receipts (listed for self-orders); GrabFood/GoFood/UrbanPiper delivery integrations; label printing via USB proxy; drag-and-drop product grid.

Entities (inferred from `addons/point_of_sale/models` and `addons/pos_restaurant/models` filenames [I]): `pos.config`, `pos.session`, `pos.order`, `pos.order.line`, `pos.payment`, `pos.payment.method`, `pos.category`, `pos.printer`, `pos.preset`, `pos.bill` (cash denominations), `pos.note`, `pos.snooze`, `pos.prep.order`/`pos.prep.line` (kitchen tickets), `pos.course`, `restaurant.floor`, `restaurant.table`, product combos (`product.combo`, `product.combo.item`), pricelists, fiscal positions, cash rounding, barcode rules.

### 2.2 eCommerce

`website_sale` models [V, filenames]: product template/variant/attribute/attribute category, `product.public.category`, ribbon, wishlist, stock notification, product image/document, `product.feed` (Google Merchant and similar), pricelist and pricelist item, `website.checkout.step`, `website.sale.extra.field`, `sale.order`/line, `delivery.carrier`, `payment.transaction`, visitor tracking, `withdrawal_request`.

Odoo 20 additions [V] (same release notes): AI product editing, automated cross-sell and review requests, donations incl. recurring, email gift cards, loyalty progress bar, minimum quantity, preferred delivery date, category-level sales blocking, order dashboard, pricelist selector on every page, tax-included vs excluded display per pricelist (B2C and B2B on one site), stock-based auto publish/unpublish, return requests from portal, WhatsApp abandoned-cart follow-up, GA4 event coverage, Sendcloud label formats PDF/ZPL/PNG with Mondial Relay folded into Sendcloud.

### 2.3 Website

Models [V, filenames]: `website`, `website.page`, `website.menu`, `website.rewrite` (redirects), `website.visitor`, `website.snippet.filter`, `website.controller.page`, theme models, form handling, `ir.asset`. Pages are QWeb views (`ir.ui.view`), so content and templates share one mechanism [I]. Odoo 20 adds an AI Website Assistant (page/form/menu creation, "vibe coding" with snippets), age-verification popup, llms.txt setting, banners, breadcrumbs, link-tracker QR codes, GTM single field, blog redesign with recommended post and scheduled posts, and a Messenger/Instagram link into Live Chat [V]. The Odoo 20 docs also describe a Website > Settings > "Import Website" flow that pulls pages and products from an existing site (JSON-LD, RDFa, Open Graph and Microdata metadata, plus Shopify's public API, WooCommerce and PrestaShop) [V] (https://github.com/odoo/documentation/blob/20.0/content/applications/websites/website/website_creation.rst); a useful reference for obop's migration tooling.

Adjacent apps: Blog (`website_blog`), Forum (`forum.forum/post/vote/tag` plus karma via gamification), eLearning (`slide.channel`, `slide.slide`, `slide.question`), Live Chat (`im_livechat_channel`, `chatbot.script/step/answer`, `discuss.channel`).

### 2.4 Marketing

`mass_mailing` entities [V]: `mailing.mailing` (states, KPIs opened/clicked/replied/bounced), `mailing.list`, `mailing.contact`, `mailing.subscription`, `mailing.subscription.optout` (reasons/feedback), `mailing.trace` (status: outgoing, process, pending, sent, open, reply, bounce, error, cancel), `mail.blacklist`, `mailing.filter`, `link.tracker` (click tracking), `utm.*` (campaign, medium, source, stage, tag). SMS: `sms.sms`, `sms.template`, `sms.tracker`, IAP credit account, and `sms_twilio` as bring-your-own provider. Odoo 20 email adds conditional content blocks, dynamic lists, click tracking per link, template library, social links, product/event snippets; UTM "Reference" mapping [V]. Odoo 20 Marketing Automation (Enterprise) adds a drag-and-drop workflow builder, webhook/date/anniversary/event triggers [V].

Loyalty (`loyalty.program/rule/reward/card/history`) [V]: program types **coupons, gift_card, loyalty, promotion, ewallet, promo_code, buy_x_get_y, next_order_coupons**; rewards are discount, free product, or free shipping with a points cost; gift cards and eWallets act as payment programs. The same engine serves POS and eCommerce (`pos_loyalty`, `website_sale_loyalty`). Events: `event.event/type/ticket/registration/question/stage/tag/slot`, scheduled mails, booths, tracks, and Odoo 20 "event combos" (ticket plus food with distinct VAT). Surveys: `survey.survey/question/user_input` with badges/challenges. Referrals and WhatsApp are not in Community [I].

Price anchor [V, label: plan mapping is an inference from the page layout]: Odoo Online Standard EUR 19.90 and Custom EUR 35.90 per user per month on yearly billing, One App Free at EUR 0; POS, eCommerce and Website carry no separate fee; IAP credits (SMS, AI) are extra (https://www.odoo.com/pricing). The page also states that the discount is valid for 12 months for initial users and shows further figures (24.90 / 44.90 and 31.10 / 56.90 EUR, plus a 7.90 EUR light-user rate) whose exact mapping to list price versus monthly billing is unverified; treat 19.90 / 35.90 as a first-year promotional yearly-billing price.

## 3. POS

### 3.1 How Odoo's offline POS actually works (primary source: code and docs)

- Browser app (Owl); at session start it preloads products and customers into the browser cache; all data models are mirrored into **IndexedDB**, and a service worker caches GETs with a network-first, cache-fallback strategy [V] (https://github.com/odoo/odoo/blob/20.0/addons/point_of_sale/static/src/app/service_worker.js , `plugins/pos_data_plugin.js`).
- Orders carry a client-generated **UUID** with a database `unique(uuid)` constraint, per-device sequence numbers, and server-side de-duplication of order lines by uuid; the client tracks `localUnsyncedPaidOrderUuids` and warns on potential data loss [V] (https://github.com/odoo/odoo/blob/20.0/addons/point_of_sale/models/pos_order.py). This is a textbook idempotent outbox.
- The generic Odoo offline plugin is deliberately neutralised inside the POS; POS has its own path [V].
- Documented offline limits [V] (https://github.com/odoo/documentation/blob/20.0/content/applications/sales/point_of_sale/use.rst): invoices are generated after reconnect; customer registration, detailed product info, real-time stock sync and **payment terminals do not work offline**; LAN printers and scales keep working if the local network is unchanged; users must not reload or clear the browser cache until synced.

### 3.2 Recommended offline architecture for obop [I]

What must work with zero internet: catalog and prices for the store, pricelists and promotions, tax computation, cash and "other" tender, order creation/edit/split/transfer, kitchen tickets, receipt and ticket printing, cash drawer, barcode and scale input, customer display, shift open/close with counted cash, fiscal signing where the device is local, loyalty/gift card redemption against a cached balance snapshot (with risk policy), and queueing of everything else.

What cannot be offline: card-present payment through cloud-driven readers (Stripe's server-driven integration explicitly does not support offline card collection [V], https://docs.stripe.com/terminal/payments/setup-integration?terminal-sdk-platform=server-driven), online-order ingestion, cloud fiscal signing (fiskaly), live stock across stores, and gift card issuance against the central ledger.

Design:
1. **Local store**: SQLite everywhere. Native shell uses `rusqlite`. Browser uses SQLite-WASM on OPFS; the `opfs-sahpool` VFS needs no COOP/COEP headers and has the best performance but no multi-connection support, whereas the plain `opfs` VFS needs cross-origin isolation [V] (https://sqlite.org/wasm/doc/trunk/persistence.md). Stripe.js documents that it does not support cross-origin-isolated sites [V] (https://docs.stripe.com/security/guide), so choose sahpool and own a single-tab/SharedWorker arbitration [I].
2. **Sync model**: append-only event/outbox (order events with UUIDv7 ids, device id, per-device gapless counter), server endpoint idempotent on event id, catalog pulled as versioned snapshots plus deltas. Avoid general CRDT sync for orders because an order has one owner at a time; use CRDTs only for collaborative content editing (Loro 1.16.4, Automerge 3.5.0, both MIT [V] via GitHub releases).
3. **Sync-engine build vs buy** [V, GitHub]: PowerSync service is **FSL-1.1-ALv2** (source-available, converts to Apache later), its SQLite core is Apache-2.0; ElectricSQL is Apache-2.0 but Postgres read-path sync with writes through your own API; Turso Database (Rust, MIT) is at v0.8.1, pre-1.0; cr-sqlite's last release is from January 2024. Recommendation is to own a small purpose-built protocol rather than adopt any of these for POS.
4. **Multi-device restaurant consistency**: a **store hub** (the same Rust binary running on a store PC/mini-server) is the LAN authority for open tables, course state and kitchen routing, speaking WebSocket/SSE to tablets and KDS screens. Internet loss then does not split the room. Odoo instead relies on each browser and the server bus; the hub is an upgrade [I].
5. **One rules engine**: pricing, tax, rounding, pricelist, promotion and loyalty logic written once in Rust, compiled natively for the server and to WASM for POS and cart. This removes JS-vs-Python totals mismatches. Vite 8 supports `.wasm?init`, but in SSR builds only for Node.js-compatible runtimes because the implementation relies on `node:fs` [V] (https://vite.dev/guide/features). Proposal [I].

### 3.3 Hardware matrix

| Device | Practical protocols | Browser-only path | Native (Rust) path | Notes |
|---|---|---|---|---|
| Receipt printer | ESC/POS over TCP 9100, USB, serial; Epson ePOS (HTTP); Star CloudPRNT (printer polls your server over HTTP/HTTPS, MQTT on some models, keyed by MAC) [V] | Epson ePOS over LAN needs HTTPS or Chrome Local Network Access; WebUSB/Web Serial for USB | `escpos` 0.20.0 (features: usb, native_usb, hidapi, serial_port, usbprint; barcodes, QR, raster) [V] | Odoo: recommends Epson TM-m30 i/ii/iii over network; TM-T20/T88 and generic ESC/POS need an IoT box; **Bluetooth printers unsupported** [V] |
| Cash drawer | Kick pulse through the printer (ESC/POS pulse command) or cash machines (Cashdro, Cashmatic, Glory) [U for pulse command detail] | via printer | via printer | Multi-drawer sites need per-printer selection (Odoo 20) [V] |
| Payment terminal | Cloud: Stripe server-driven (WisePOS E, Reader T600/S700/S710, Verifone), Adyen cloud device-api; LAN: Adyen NEXO `/nexo` on port 8443, needs cert handling; vendor DLL/serial for Worldline/SIX (Odoo wraps these in Python ctypes IoT drivers) [V] | Stripe Terminal JS SDK; cloud APIs | Cloud APIs from the Rust server; LAN NEXO from the hub | Odoo terminal list: Adyen, DPO, Mercado Pago, Mollie, Pine Labs, QFPay, Razorpay, SIX, Stripe, Tyro, Viva.com, Worldline/Axepta [V] |
| Barcode scanner | USB HID keyboard wedge (default), serial/CDC | keydown stream with timing heuristics; Web Serial for serial | `serialport` 4.10.1, `nusb` 0.2.7, `hidapi` | Needs barcode nomenclature rules (EAN-13 price/weight-embedded prefixes, GS1) [U on prefix details] |
| Scale | Serial protocols per vendor; legal-for-trade rules in EU | Web Serial (Odoo ships `iot_webserial` for this) [V] | `serialport` | Odoo docs flag European legal settings for scales [V] |
| Customer display | Second screen/window, or serial VFD pole | Second window on the same device; separate route like Odoo's `customer_display_assets` bundle [V] | Native second window | |
| Fiscal devices | BE: Boitenoire.be FDM over RS-232 + USB adapter, one IoT box per FDM; IT: RT fiscal printers (separate from ePOS printers) [V] | Not feasible in browser | Rust serial driver in the hub/agent | Certified vendor hardware and registration |

Odoo's answer to all local hardware is the **IoT box** (Raspberry Pi image) or Windows virtual IoT, a Python service (`addons/iot_drivers`: printer, serial scale, display, keyboard, camera, Bluetooth caliper, Worldline/SIX terminals, Belgian and Swedish blackbox drivers); production use requires an IoT box subscription [V] (https://www.odoo.com/documentation/20.0/applications/general/iot.html). A single static Rust **obop agent** (printing, serial, USB, LAN discovery, fiscal device bridge, store hub) would replace it [I].

### 3.4 Browser capability reality check (caniuse data, 2026-10) [V]

| API | Chrome/Edge | Firefox | Safari / iOS |
|---|---|---|---|
| WebUSB | yes | no | no |
| Web Serial | yes | yes from v151 (desktop only, behind a site-permission add-on; Firefox 151 released 2026-05-19) | no |
| WebHID | yes | no | no |
| Web Bluetooth | yes | no | no |
| Background Sync | yes | no (unknown status) | no |
| IndexedDB, Service Workers, Wasm | yes | yes | yes |

Source: https://github.com/Fyrd/caniuse (data.json). Chrome 142 added **Local Network Access**: a permission prompt for public sites calling LAN/loopback addresses, which also relaxes mixed-content blocking once granted (https://developer.chrome.com/release-notes/142); Odoo's POS uses it for ePOS printers and requires a static printer IP (https://github.com/odoo/documentation/blob/20.0/content/applications/sales/point_of_sale/hardware_network/pos_lna.rst). WebKit may evict script-writable storage of idle sites and only grants persistence by heuristics (https://webkit.org/blog/14403/updates-to-storage-policy/).

Conclusion [I]: an iPad running Safari cannot reach USB/serial/Bluetooth hardware and is a poor offline host. Ship (a) a **Tauri 2** shell (v2.12.1, Apache-2.0/MIT; its official Vite guide targets Vite 8, https://v2.tauri.app/start/frontend/vite/) for Windows/macOS/Linux/Android/iOS with the Rust agent built in, and (b) a plain PWA for "no hardware" use.

### 3.5 Restaurant, floor and kitchen

Odoo 20 behaviours to mirror [V, docs restaurant.rst and extra/preparation.rst]: floors and tables (number, seats, shape, live occupancy), set table/tab, table merge, bookings via Appointments (Booked, Checked-in, No-show), orders without table, transfer between tables, **courses** (manual/automatic allocation, fire courses), bill splitting, kitchen/bar routing by product category to printers and to **preparation displays** with stages and alert timers, tips (including tip-after-payment for the US), QR-menu and kiosk self-order with pay-after-meal, delivery platform integration. Our hub-based design (3.2) gives KDS screens LAN-local resilience that Odoo lacks.

### 3.6 Fiscal compliance (per country)

Three archetypes exist (Wikipedia's survey is a secondary source: https://en.wikipedia.org/wiki/Fiscalization): hardware-based (fiscal printer/black box), signature-device or cloud-signature (TSE, RKSV), and software-certified with hash chains (France) or online clearance (Croatia, Serbia, Spain).

| Country | Mechanism | How Odoo 20 does it [V] | Status note |
|---|---|---|---|
| Germany | Certified TSE (hardware or cloud) per KassenSichV; DSFinV-K export; registration under section 146a(4) AO | `l10n_de_pos_cert` via **fiskaly** cloud TSS, restaurant variant `l10n_de_pos_res_cert` (https://github.com/odoo/documentation/blob/20.0/content/applications/finance/fiscal_localizations/germany.rst) | fiskaly ships SIGN DE, DSFinV-K and SUBMIT DE; its workspace claims BSI certification valid through 2033 [V, vendor claim] |
| Austria | RKSV signature creation unit, FinanzOnline registration | `l10n_at_pos` via fiskaly | |
| France | NF525 / CGI 286 I-3 bis anti-fraud certification; e-invoicing reform | `l10n_fr_pos_cert` (certificate of conformity), `l10n_fr_pdp_pos` (PDP flux 10 e-reporting) | From **1 Sep 2026** every company must receive e-invoices and large/mid-size firms must issue and e-report (B2C included); **1 Sep 2027** for SMEs and micro-enterprises [V]. Cross-checked against the tax authority: all VAT-registered firms must be able to receive from 1 Sep 2026, large and mid-size firms (GE/ETI) must issue from that date, small and micro firms have until 1 Sep 2027, and e-reporting follows the same calendar (https://www.impots.gouv.fr/professionnel/je-decouvre-la-facturation-electronique) |
| Belgium | Certified cash register with FDM black box + VAT signing card for food-service businesses with yearly earnings above EUR 25,000 (excluding VAT, drinks and take-away food) | `pos_blackbox_be` (not in the Community `addons/` tree, so presumably Enterprise [I]); only Boitenoire.be FDM (cert BMC04); one IoT box per FDM; **disables** global discounts, loyalty, reprint, price edits and line edits; tax-included prices; 0.05 cash rounding; clock-in | A certified POS is a different, restricted product mode |
| Italy | Certified RT printers/servers, daily transmission | `l10n_it_pos` driving the fiscal printer | Not an ePOS printer; separate config |
| Spain | Verifactu (national) and TicketBAI (Basque) | `l10n_es_edi_verifactu(_pos)`, `l10n_es_edi_tbai_pos` | **RDL 15/2025 (2 Dec 2025) moved deadlines: corporate-tax taxpayers before 1 Jan 2027, all others before 1 Jul 2027** [V] (https://sede.agenciatributaria.gob.es/Sede/iva/sistemas-informaticos-facturacion-verifactu/nota-informativa-ampliacion-plazo-adaptacion-facturacion.html , page last updated 2026-03-26). The decree was adopted 2 Dec 2025, published in the BOE on 3 Dec 2025 and in force 4 Dec 2025 (https://www.boe.es/buscar/act.php?id=BOE-A-2025-24446). The separate obligation on software producers (art. 3.2, nine months after Orden HAC/1177/2024) was not moved, which matters for obop as a software vendor [V] |
| Portugal | Certified software, SAF-T(PT), ATCUD, QR | `l10n_pt` | fiskaly SIGN PT API dated 2026-05-04 |
| Poland, Hungary, Greece, Romania, Bulgaria, Turkey | Online cash registers / fiscal printers / clearance | Poland: `l10n_pl_reports_pos_jpk` "JPK_VAT PoS Enterprise" (Enterprise, absent from the Community tree); others not in Odoo docs for POS [V absence] | Not researched in depth [U] |
| Odoo 20 newer | Egypt ETA e-receipts, Pakistan FBR real-time, Indonesia QRIS, Malaysia MyInvois, Peru SUNAT 58/80 mm, Guatemala FEL, Taiwan ECPay, Vietnam SInvoice | listed in the Odoo 20 release notes [V] | |

fiskaly covers DE, AT, ES, IT, FR, PT plus Belgium e-invoicing; Sweden is "coming soon" (https://developer.fiskaly.com/ , https://workspace.fiskaly.com/). One oddity: the workspace page displays a blog date of 30 Nov 2026, after today, so treat its dated claims with caution [V observation].

Design consequences [I]: the order ledger must be append-only (corrections are new documents), numbering gapless per device/register, receipts marked as copies on reprint, and the fiscal layer a trait with three implementations: `LocalDevice` (serial/USB bridge in the agent), `CloudSigner` (fiskaly-style HTTP), `ChainedSoftware` (hash chain plus certification file). Certification cost and process per country is a product/legal workstream, not a code task.

## 4. eCommerce

### 4.1 Landscape of open-source stores (as of 2026-10-01) [V via GitHub API]

| Project | Licence | Latest | Note |
|---|---|---|---|
| Medusa | MIT core + Enterprise Edition (RBAC) under commercial agreement | v2.21.2 (2026-09-28) | Node; modules/workflows; markets itself for marketplaces and POS too |
| Saleor | BSD-3-Clause | 3.23.37 (2026-09-30) | Python, GraphQL-only, headless |
| Vendure | GPLv3 or commercial | v3.7.3 (2026-09-02) | Node/TS, headless |
| Spree | BSD-3-Clause | v5.6.1 | Ruby |
| Sylius | MIT | v2.3.0 | PHP/Symfony |
| Bagisto | MIT | v2.5.0-beta6 (2026-10-01) | PHP/Laravel |
| Shopware | MIT | v6.7.15.0 | PHP |
| WooCommerce | GPLv3 (WordPress plugin) | 11.1.2 (2026-09-22) | REST v3 |
| PrestaShop / OpenCart | PrestaShop core OSL-3.0 with modules AFL-3.0; OpenCart GPLv3 | 9.2.0 / 4.1.0.4 | |
| ERPNext | GPL-3.0 | v15.121.6 | ERP with POS |

Copyleft note for obop: Vendure (GPL), listmonk/Webstudio/Stalwart (AGPL) cannot be embedded in a permissively licensed core; Directus now ships under a source-available licence (MSCL-1.0-GPL) [V].

### 4.2 Capabilities to build (Odoo parity)

Catalog with variants, attribute values, public categories, ribbons, pricelists per country/currency/customer group, comparison and wishlist, stock notifications, combo products; cart and multi-step checkout with extra steps; guest checkout; abandoned cart; delivery methods with rules (price rules, zip prefixes, pickup points, Sendcloud); taxes and fiscal positions; invoices; returns; reviews; subscriptions; donations; gift cards/coupons; Google Merchant feed. Multi-website: every content and commerce record scopes to `website_id` with host-based routing; multi-currency via pricelists (currency per list) and a pricelist selector [V].

### 4.3 Payments, security, tax

- Odoo ships ~25 provider modules (Stripe, Adyen, PayPal, Mollie, Authorize.net, Worldline, Razorpay, PayU, Redsys, Xendit, Mercado Pago and more) and Odoo 20 adds pay-on-invoice, per-pricelist/amount provider restriction, tokenised recurring payments [V]. Rust: `async-stripe` 0.41.0 is the latest stable release, with a 1.0.0-rc.9 pre-release published 2026-09-11, so expect an API break soon; the old `stripe-rust` was last published in 2020 [V].
- Use hosted fields/redirect so card data never touches obop. PCI DSS v3.2.1 retired 31 Mar 2024 [V] (https://blog.pcisecuritystandards.org/pci-dss-v4-0-resource-hub). The future-dated PCI DSS v4.x requirements took effect 31 Mar 2025 [V] (PCI SSC blog "Now is the Time for Organizations to Adopt the Future-Dated Requirements of PCI DSS v4.x"), and the current published version is v4.0.1 [V] (https://www.pcisecuritystandards.org/document_library/). PCI SSC FAQ 1588 (Feb 2025) names requirements 6.4.3 and 11.6.1 as the script-protection techniques for merchants embedding a payment form [V] (https://www.pcisecuritystandards.org/faq/articles/Frequently_Asked_Question/how-does-an-e-commerce-merchant-meet-the-saq-a-eligibility-criteria-for-scripts/); the wording "payment-page script inventory and tamper detection" is my paraphrase of their content [U]. They matter because a website builder lets editors inject scripts, so checkout pages need a locked script allow-list and CSP.
- EU VAT: ViDA adopted 11 Mar 2025; 1 Jan 2027 OSS/IOSS clarifications, 1 Jul 2028 platform deemed-supplier (Member States may postpone this to 1 Jan 2030) and single VAT registration elements, 1 Jul 2030 digital reporting; implementing regulation 2026/1869 of 28 Jul 2026 introduces the transfer-of-own-goods scheme [V] (https://taxation-customs.ec.europa.eu/taxation/vat/vat-digital-age-vida_en). Keep tax as a pluggable engine (built-in rules plus Avatax-style connectors; Odoo POS has an Avatax module flag [V]).
- EU Cyber Resilience Act: reporting obligations from 11 Sep 2026, main obligations from 11 Dec 2027 [V] (https://digital-strategy.ec.europa.eu/en/policies/cyber-resilience-act); it affects obop as distributable software. For open source, the Commission says the CRA covers it only when supplied in the course of a commercial activity, and creates an "open-source software steward" category with a lighter regime (Article 24; stewards are exempt from administrative fines under Article 64(10)) [V] (https://digital-strategy.ec.europa.eu/en/policies/cra-open-source); whether obop's own distribution model counts as commercial is a legal question [U]. The European Accessibility Act covers e-commerce; the EC page lists e-commerce and self-service machines (ATMs, ticketing and check-in machines) [V] and the application date of 28 Jun 2025 (transposition was due 28 Jun 2022) is confirmed only from a secondary source (Wikipedia), with a microenterprise exemption for services [V, secondary] (https://en.wikipedia.org/wiki/European_Accessibility_Act); that payment terminals are in scope comes from my recollection of the directive text [U].

### 4.4 Marketplace and channel sync

| Channel | State [V unless noted] | Implication |
|---|---|---|
| Shopify | Quarterly API versions; 2026-07 latest stable, 2026-10 release candidate on 2026-10-01; each stable version lives at least 12 months. REST Admin API is legacy since 2024-10-01; new public apps GraphQL-only since 2025-04-01 (https://shopify.dev/docs/api/usage/versioning , https://shopify.dev/docs/api/admin-rest) | Use GraphQL Admin API, pin a version, upgrade quarterly |
| Amazon SP-API | Deprecation schedule as of 2026-09-09: Orders v0 deprecated 2026-01-28, **removed 2027-03-27**; Finances v0 removed 2027-08-27; settlement report types removed 2026-11-11; flat-file/XML feed types removed 2025-12-03 (https://developer-docs.amazon.com/sp-api/docs/sp-api-deprecations) | Build on current Orders/Listings/Feeds JSON APIs only |
| eBay | Developer pages returned 403 to the fetcher | [U] Assume REST Sell APIs with OAuth; verify before design |
| Etsy | Open API v3; tiers Seller/Personal/Commercial, commercial access needs manual review; app must carry a non-endorsement disclaimer (https://developers.etsy.com/documentation/) | Budget approval lead time |
| WooCommerce | REST v3 at `/wp-json/wc/v3`, key/secret Basic over HTTPS, batch endpoints, webhooks (https://woocommerce.github.io/woocommerce-rest-api-docs/) | Straightforward |
| Odoo connectors | Amazon, Shopee, Lazada, TikTok Shop (the 20.0 docs tree has no Shopify connector; Shopify, WooCommerce and PrestaShop appear only as import sources in the website importer); Amazon connector syncs **orders in, available quantity and shipment confirmation out**, supports FBA/FBM, multiple accounts and marketplaces; it does not push listings (https://github.com/odoo/documentation/blob/20.0/content/applications/sales/sales/amazon_connector/features.rst). No eBay/Etsy connector appears in the docs tree [V] | Match this scope first: orders in, stock/price/tracking out |

Model: a `sales_channel` entity with credentials, a per-channel mapping table (external id to local product/variant/partner), inbound order importer, outbound stock/price/fulfilment pushers, webhook receivers with signature checks, rate-limit-aware job queue, idempotency keys [I].

### 4.5 Headless storefronts

Shopify Hydrogen 2026.4.6 is built on React Router 7 with Vite peer support up to Vite 8 (https://github.com/Shopify/hydrogen), evidence that commerce storefronts on Vite are mainstream. Saleor describes itself as GraphQL-only and API-only headless; Medusa's README positions its framework for DTC/B2B stores, marketplaces and PoS systems [V from READMEs]; I did not verify Medusa's storefront starters [U]. For obop: publish a versioned OpenAPI (and optionally GraphQL) storefront API plus a reference storefront, generated typed client, and a webhook/event catalog [I].

## 5. Website builder and CMS

Capabilities to clone: drag-and-drop sections with reusable blocks ("snippets"), themes with design tokens, menus, redirects, forms writing to CRM, blog with scheduled posts, forum with karma, eLearning, live chat with chatbots, multilingual, SEO fields, cookie bar, visitor tracking, link tracker (section 2.3).

Open-source parts available [V, GitHub/npm]:

| Component | Licence / state | Use |
|---|---|---|
| Puck (`puckeditor/puck`) | MIT, 0.23.0 (2026-08), pre-1.0 | React page editor driven by JSON |
| GrapesJS | v0.23.6 (2026-08-26); BSD-3-Clause per the npm `grapesjs` package metadata (GitHub does not auto-detect the licence) [V] | Framework-agnostic visual builder |
| Craft.js | MIT, last push 2025-02-14 | Possibly stale |
| Tiptap / Lexical / Slate | MIT | Rich text |
| Payload / Ghost | MIT | Headless CMS references |
| Webstudio | AGPL-3.0 | Reference only |
| Directus | MSCL-1.0-GPL | Source-available, avoid |

Recommended content model [I]: pages are versioned JSON trees of blocks (block type, props, slots, translations keyed by locale), compiled at publish time into immutable snapshots; themes are design tokens plus block templates; multilingual by per-field translation maps with locale-prefixed routes, `hreflang` and canonical generation. Core Web Vitals targets to design against: LCP 2.5 s, INP 200 ms, CLS 0.1 at the 75th percentile [V] (https://web.dev/articles/vitals).

## 6. Marketing suite and deliverability

### 6.1 Deliverability requirements (authoritative, current)

- **Google bulk-sender rules** (5,000+/day): SPF and DKIM both required (DKIM keys at least 1024-bit, 2048 recommended), DMARC record required (p=none allowed) with From-domain alignment, TLS, valid forward/reverse DNS, RFC 5322 compliance, one-click unsubscribe with both `List-Unsubscribe` (HTTPS URL) and `List-Unsubscribe-Post: List-Unsubscribe=One-Click`, visible body unsubscribe link, spam rate below 0.10% and never 0.30%; ramp volume gradually [V] (https://support.google.com/a/answer/81126).
- **Yahoo**: SPF+DKIM+DMARC, RFC 8058 one-click preferred, unsubscribe processed within 2 days, complaint rate under 0.3% [V] (https://senders.yahooinc.com/best-practices/).
- **Microsoft/Outlook.com**: senders above 5,000 messages/day to Outlook.com need SPF, DKIM and DMARC from 5 May 2025; non-compliant mail goes to Junk first and is later rejected [V, summarised fetch of a short page] (Microsoft Tech Community blog "Strengthening email ecosystem: Outlook's new requirements for high-volume senders"); detailed DMARC/alignment rules not re-read [U].
- **DMARCbis is now RFC 9989 (May 2026)**, obsoleting RFC 7489 and RFC 9091; aggregate and failure reporting moved to RFC 9990 and RFC 9991; `pct` removed, new `np` and `t` tags, DNS Tree Walk replaces the public-suffix list [V] (https://www.rfc-editor.org/rfc/rfc9989). RFC 8058 defines one-click unsubscribe signalling [V].
- US CAN-SPAM: honour opt-outs within 10 business days, postal address required, up to USD 53,088 per violating email [V] (https://www.ftc.gov/business-guidance/resources/can-spam-act-compliance-guide-business; 16 CFR 1.98 on eCFR as of 2026-09-29 still lists USD 53,088 for penalties assessed after 17 Jan 2025, and the figure is adjusted for inflation periodically). US SMS needs A2P 10DLC brand and campaign registration (unregistered traffic incurs extra carrier fees) [V] (https://www.twilio.com/docs/messaging/compliance/a2p-10dlc).
- GDPR/ePrivacy consent and soft opt-in rules: standard practice, not re-verified here [U]. EU Digital Omnibus changes not checked [U].

### 6.2 Rust mail stack

| Need | Choice | Notes [V] |
|---|---|---|
| Submission via SMTP relay | `lettre` 0.11.23 (MIT) or `mail-send` 0.6.2 (Apache-2.0/MIT) | lettre MSRV 1.85 |
| DKIM sign/verify, SPF, DMARC eval, ARF, TLS-RPT, MTA-STS | `mail-auth` 0.13.3 (Apache-2.0/MIT), also implements DKIM2; ARC described by the crate README as "historic" (pending IETF draft `draft-ietf-dmarc-arc-to-historic`; RFC 8617 is still listed as Experimental by the RFC Editor), opt-in; `main` is at 0.14.0 (unreleased, API reorganisation) | Stalwart Labs crate |
| Own MTA at volume | KumoMTA (Rust, Apache-2.0, release 2026.09.29) | Separate service, Lua-configured |
| Full mail server | Stalwart (AGPL-3.0 or commercial SELv2) | Licence conflict for embedding |
| MJML rendering | `mrml` 6.0.1 | For email block editor output |
| Reference products | Postal (MIT, Ruby), Mautic (PHP), listmonk (AGPL, Go) | Study behaviour only |

### 6.3 Pipeline design [I]

Compose (block model to MJML to inline HTML plus text) then personalise then enqueue per-recipient `mailing_trace` rows then send through pluggable transports (SMTP relay by default, built-in MTA optional) with per-domain rate limits and warm-up ramps. Sign DKIM with a per-tenant selector (2048-bit), use a VERP/bounce subdomain return-path, parse DSNs and ARF feedback-loop reports, classify hard/soft bounces, feed a suppression list, process one-click unsubscribe POSTs, keep an immutable consent ledger (source, timestamp, text version, IP), run a DNS checker for SPF/DKIM/DMARC/PTR, and surface spam-rate thresholds. Odoo does the same with a bounce alias, bounce propagation to all records with the address, and one-click URLs [V, `mail_thread._routing_handle_bounce`, `mass_mailing._get_unsubscribe_oneclick_url`]. Open tracking is unreliable where mail clients prefetch images [U]; make click and conversion the primary KPIs.

### 6.4 Other marketing modules

Loyalty/coupons/gift cards/eWallet: adopt Odoo's single program/rule/reward/card/history schema, with program types as data, applicable to POS and web alike (2.4). SMS: provider abstraction (Twilio first, as Odoo's `sms_twilio` does; IAP-style credits are optional). Marketing automation: event triggers, waits, branching, webhook enrolment (Odoo 20 workflow builder). Social posting: platform-specific scheduling, stories, first-comment (Odoo 20) but gated by each platform's API terms [U]. Events: tickets, questions, slots, scheduled mails, combos with distinct VAT. Surveys and referrals: straightforward.

## 7. Architecture on Rust + Vite+

### 7.1 Layering

```
Browser / native shell (Tauri 2) / kiosk
   |  islands + apps built by vp build (Vite 8 / Rolldown), Vitest for tests
Edge / CDN  (s-maxage + stale-while-revalidate, tag purge)
   |
obop-web (axum)  <- host->website routing, static snapshots, block renderer, SSR gateway
obop-api (axum)  <- JSON/OpenAPI, webhooks, auth, jobs
obop-core        <- domain crates: pricing/tax/loyalty engine (native + WASM), ledger, fiscal traits
Postgres (sqlx)  + object storage + queue;  store hub / agent (same Rust code, SQLite)
```

### 7.2 SSR options under the constraints

The owner requires Rust backend and Vite+ frontend only. Four workable options, ranked [I]:

1. **Rust-rendered blocks + Vite islands (recommended default for website and content pages).** Page JSON renders to HTML in Rust (minijinja 2.24, which also compiles to WASM so the editor preview uses the identical renderer); Vite+ builds the interactive islands and emits `manifest.json`; Rust injects preload/CSS/script tags exactly as the Vite backend-integration guide prescribes (`build.manifest`, walk `imports` recursively, modulepreload) [V] (https://vite.dev/guide/backend-integration). No Node in production; trivially cacheable.
2. **Node SSR sidecar for app-like storefront flows.** Build a Vite SSR app (client + server bundles, SSR manifest for preload hints [V], https://vite.dev/guide/ssr; run through `vp build`, since obop drives all JS tooling via `vp`: Vite's guide shows the `vite build --ssr` flag, but the `vp build` page documents no `--ssr` pass-through [V https://viteplus.dev/guide/build], so configure SSR in `vite.config.ts` or confirm the flag before relying on it [I]) with a Vite-based framework and run it behind axum. Framework compatibility with Vite 8 from npm metadata [V]: Astro 7.3.5 (depends on Vite ^8.0.13; islands plus `server:defer` server islands [V], https://docs.astro.build/en/concepts/islands/), SvelteKit 2.70.3, React Router (Hydrogen pattern), TanStack Start (Vite >= 7), SolidStart 2.0.5 (^8 or ^9), vike (>= 6.3). **Qwik 1.20.1 (the stable `@builder.io/qwik` line) peers Vite `>=5 <8`, so it is out; the Qwik 2 beta (`@qwik.dev/core` 2.0.0-beta.47) peers Vite `>=8 <9` and Vitest `<5` (Vite+ bundles Vitest 5), so it is a pre-release option at best** [V npm]. `vp pack` can bundle the sidecar as a single executable [V], which a Rust supervisor can spawn.
3. **Embedded JS engines in Rust** (rquickjs 0.14.0, deno_core 0.412.0, `ssr_rs` 0.8.3, tuono 0.19.7). `ssr_rs` and tuono last released May 2025 and `deno_core`'s standalone repo is archived [V], so this is the weakest option.
4. **Rust-native UI (Leptos 0.8.21, Dioxus 0.7.10)** conflicts with the "frontend exclusively Vite+" rule, so it should not be used for product UI without the owner's explicit waiver.

Note the Vite SSR guide targets framework authors ("low-level API meant for library and framework authors") and the Environment API is not yet stable (docs banner: release candidate, some APIs still experimental) [V], so prefer frameworks that absorb that churn.

### 7.3 SEO and edge caching

- Static generation for published pages, incremental regeneration on publish via a job that re-renders dependent pages and purges by cache tag.
- Headers: `Cache-Control: public, s-maxage=…, stale-while-revalidate=…, stale-if-error=…` (RFC 5861 [V]), `CDN-Cache-Control` for CDN-only policy (RFC 9213 [V]), `Cache-Status` for debugging (RFC 9211 [V]), 103 Early Hints from the Vite manifest (RFC 8297; the RFC text is labelled Experimental but the IETF datatracker and RFC Editor now show it as Proposed Standard after a status change whose date I did not find [V]). Surrogate-key/cache-tag header names are vendor-specific [U].
- Never cache cart, account, checkout; serve them as uncached islands (server islands or client fetch) into a cached shell; key cached product pages by website, locale, currency/pricelist, not by cookie.
- Structured data (schema.org Product/Offer), sitemaps per website and locale, canonicals, `hreflang`, Merchant feed, `llms.txt` [I from Odoo 20 list].
- Edge compute is optional: `@cloudflare/vite-plugin` 1.62.3 supports Vite 6 to 8 [V].
- Multi-website TLS: ACME per custom domain (e.g. `instant-acme` 0.8.5 [V exists], selection otherwise [U]).

### 7.4 What runs offline in the POS (summary)

Shell (Tauri or PWA) with cached Vite bundle; local SQLite; Rust rules engine in WASM/native; agent for hardware; outbox syncing to `obop-api`; store hub for multi-device sync and KDS; fiscal `LocalDevice` signer when applicable. Online-only: cloud terminals, cloud fiscal signing, central stock and gift-card issuance, online orders.

## Confidence & gaps

High confidence [V]: toolchain versions, Odoo 20 structure and feature lists, Odoo POS offline behaviour, Spain deadlines, France reform dates, deliverability rules, DMARCbis RFC numbers, Shopify versioning, Amazon deprecations, browser API support, repo licences and versions.

Medium: anything taken from a summarising fetcher without a raw file (Stripe and Adyen architecture pages, Etsy, Star CloudPRNT, Tauri guide, ViDA, CRA). Vendor claims (fiskaly BSI validity, Odoo pricing mapping) are unaudited.

Not verified [U]: eBay API state (403), Microsoft Outlook detailed rules (only the headline dates and thresholds were read), PCI DSS 6.4.3/11.6.1 exact wording, whether payment terminals fall under the European Accessibility Act, GDPR/ePrivacy and Digital Omnibus, Apple Mail Privacy Protection effect, TCPA, Poland KSeF and Italy POS-to-RT linking rules, Hungary/Greece/Romania/Bulgaria/Czech/Croatia/Slovakia POS regimes, cash-drawer pulse specifics, EAN price/weight prefix rules, PowerSync/Electric production fitness, whether WebFetch-summarised vendor pages are complete. Odoo's "released September 2026" comes from odoo.com's own list; no independent launch announcement was fetched (the only independent signal is that `release.py` on the 20.0 branch is already at `FINAL`). Odoo 20 docs are moving (the docs repo files were read from the 20.0 branch on 2026-10-01).

## Implications for obop

**MUST**
1. **Append-only POS ledger with idempotent sync**: client UUIDv7 order ids, device id, gapless per-register counters, unique-key server dedupe, no edit/delete of paid lines (corrections are new documents). This is what Odoo does (UUID unique constraint) and what Belgium-style certification requires.
2. **Fiscal adapter trait with three archetypes** (`LocalDevice`, `CloudSigner`, `ChainedSoftware`) designed in the first POS milestone, even if only one country ships. Plan order by market demand: France (Sep 2026 reform in force), Germany (TSE via a fiskaly-like signer), Spain (Verifactu deadlines 2027), Italy RT, Belgium FDM, Austria RKSV. Treat certification as a dated legal workstream.
3. **Native hardware path**: Tauri 2 shell plus a Rust agent (escpos, serialport, nusb) and PWA fallback; do not depend on browser hardware APIs (no Safari WebUSB/Serial/HID/Bluetooth; Chrome 142 LAN permission prompt).
4. **Explicit offline contract**: cash/other tenders, printing, scales, scanners, kitchen tickets, cached loyalty/gift-card balances work offline; cloud terminals, cloud fiscal signing, online orders and central stock do not. Document it in the product as Odoo documents its limits.
5. **One Rust pricing/tax/promotion/loyalty engine** compiled native and WASM, shared by POS, cart and server.
6. **Email deliverability as a built-in subsystem**: DKIM 2048-bit per tenant, SPF/DKIM/DMARC/PTR checker, `List-Unsubscribe` + One-Click (RFC 8058) on every marketing message, bounce and ARF processing into a suppression list, consent ledger, spam-rate monitoring against 0.10% and 0.30%. Build on DMARCbis (RFC 9989) semantics, not RFC 7489.
7. **Pin the frontend toolchain to Vite+ 1.0.x (Vite 8.3, Node 22.18/24.11/26)** and choose only SSR frameworks with Vite 8 peers; exclude Qwik 1.x (Qwik 2 is still beta).
8. **Checkout pages locked down**: hosted payment fields only, script allow-list and CSP, since the website builder lets editors add scripts (PCI DSS 4.0).

**SHOULD**
9. Website pages stored as versioned block JSON compiled into immutable snapshots, rendered in Rust (minijinja, also compiled to WASM for editor preview), with Vite islands hydrated via the manifest; Node SSR sidecar (Astro 7 or React Router) only for app-like storefront flows.
10. Build a small purpose-built sync protocol rather than adopt PowerSync (FSL), Electric or a pre-1.0 Turso; use CRDTs only for collaborative content editing.
11. Store-level hub (same Rust binary) owning open tables, courses and kitchen routing, with WebSocket/SSE to tablets and KDS, so restaurants survive internet loss.
12. Payment terminals behind a `TerminalDriver` trait: Stripe (server-driven and JS SDK) and Adyen (cloud and LAN NEXO) first, then Mollie, Viva.com, SumUp-class providers; acknowledge that cloud-driven readers need connectivity.
13. Channel sync scope equals Odoo's first: orders in, stock/price/tracking out. Order: Shopify (GraphQL Admin, pinned quarterly version), WooCommerce REST v3, Amazon SP-API (avoid Orders v0, removal 2027-03-27), then Etsy (approval lead time), eBay after verification.
14. Adopt Odoo's loyalty schema (8 program types, rule/reward/card/history) as the one engine for POS, web, gift cards and eWallets.
15. HTTP caching strategy: SSG plus tag purge, `s-maxage` + `stale-while-revalidate`, `CDN-Cache-Control`, `Cache-Status`, uncached islands for cart/account, cache keys by website/locale/pricelist.

**COULD**
16. Replace Odoo's IoT box with a ready-made obop agent image for Raspberry Pi / mini-PCs as a differentiator.
17. Optional self-hosted MTA path via KumoMTA; keep the default as an SMTP relay abstraction. Avoid embedding AGPL Stalwart.
18. Reference headless storefront (Hydrogen-like) on Vite+ with an OpenAPI-generated SDK; `llms.txt` and AI page assistant later, following Odoo 20.
19. Edge rendering through `@cloudflare/vite-plugin`, after the origin-cache design works.
20. Clean-room guardrail: use only Odoo Community source and public docs for behaviour; do not import Enterprise-only features' code or assets (Marketing Automation, Social, WhatsApp, Referrals, marketplace connectors, Studio); re-specify them from documentation and our own design.

## Verification log

Fact-check pass on 2026-10-01 (around 10:00-10:30 UTC) using primary sources fetched directly (GitHub via `gh`, npm and crates.io registries, rust-lang manifest and blog, RFC Editor JSON, Odoo 20.0 source and docs trees, vendor docs, regulator pages).

**Confirmed: 116 claims** (13 of the 14 flagged claims as stated, plus about 103 other versions, licences, dates, RFC numbers, module names, release-note items and vendor-doc statements). The flagged claims confirmed as stated: Vite+ 1.0.0 (date, MIT, bundled versions, Node range), Vite 8 and the Vite+ announcement's silence on SSR, Odoo 20 (release list, default branch, LGPL-3, Python 3.12-3.14, PostgreSQL 16+), Odoo POS offline design (IndexedDB, service worker, `unique (uuid)`, documented limits), Spain RDL 15/2025 deadlines, France e-invoicing dates, Stripe Terminal server-driven scope, Chrome 142 Local Network Access plus caniuse browser gaps, DMARCbis RFC 9989/9990/9991 (May 2026), Google and Yahoo bulk-sender rules, Shopify versioning and REST legacy dates, Amazon SP-API Orders v0 removal, and the PowerSync, Stalwart, Vendure and Medusa licences.

**Corrections (was / now / source)**

| # | Was | Now | Source |
|---|---|---|---|
| 1 | Rust stable 1.98.1 "(2026-09-01)" | Released 2026-09-03; 2026-09-01 is the build commit date in the version string. Rust 1.99.0 is dated 2026-10-01 in RELEASES.md but was not yet on the stable channel at 10:13 UTC | https://static.rust-lang.org/dist/channel-rust-stable.toml , https://blog.rust-lang.org/2026/09/03/Rust-1.98.1/ , https://github.com/rust-lang/rust/blob/master/RELEASES.md |
| 2 | Odoo connectors "Amazon, Shopify, Shopee, Lazada, TikTok Shop" (sections 2 and 4.4) | Amazon, Shopee, Lazada, TikTok Shop only; Shopify, WooCommerce and PrestaShop appear only as sources for the website "Import Website" feature (added to 2.3) | https://github.com/odoo/documentation/tree/20.0/content/applications/sales/sales , https://github.com/odoo/documentation/blob/20.0/content/applications/websites/website/website_creation.rst |
| 3 | Vite Environment API "unstable" | Not yet stable, but the docs banner calls it "release candidate" with some APIs still experimental | https://vite.dev/guide/api-environment |
| 4 | Vite 8 supports `.wasm?init` in SSR environments | Only for Node.js-compatible runtimes in SSR builds (relies on `node:fs`) | https://vite.dev/guide/features |
| 5 | RFC 8297 (103 Early Hints) "experimental" | RFC text says Experimental, but datatracker and RFC Editor now show Proposed Standard after a status change (date not found) | https://datatracker.ietf.org/doc/rfc8297/ , https://www.rfc-editor.org/rfc/rfc8297.json |
| 6 | "Qwik 1.20.1 peers Vite <8, so it is out" (implied all Qwik excluded) | Qwik 1.x (stable `@builder.io/qwik`) is out; Qwik 2 beta (`@qwik.dev/core` 2.0.0-beta.47) peers Vite >=8 <9 and Vitest <5 | https://registry.npmjs.org/@qwik.dev/core |
| 7 | Bagisto v2.5.0-beta5 | v2.5.0-beta6 (2026-10-01) | https://github.com/bagisto/bagisto/releases |
| 8 | `async-stripe` 0.41.0 "is current" | Latest stable yes, but 1.0.0-rc.9 was published 2026-09-11 (also noted minijinja 3.0.0-alpha.2 and serialport MPL-2.0) | https://crates.io/crates/async-stripe |
| 9 | "~25 `payment_*` providers" | 23 `payment_*` provider modules on 20.0 (including `payment_demo`, `payment_custom`) | https://github.com/odoo/odoo/tree/20.0/addons |
| 10 | Belgium certified register "for horeca above EUR 25,000" | Food-service businesses above EUR 25,000 yearly earnings excluding VAT, drinks and take-away; `pos_blackbox_be` is absent from the Community tree (Enterprise) | https://github.com/odoo/documentation/blob/20.0/content/applications/finance/fiscal_localizations/belgium.rst |
| 11 | Odoo 20 POS "WhatsApp/SMS receipts" | Listed for self-order receipts only | https://www.odoo.com/odoo-20-release-notes |
| 12 | Odoo Online Standard EUR 19.90 / Custom EUR 35.90 presented as the price | Promotional yearly price valid 12 months for initial users; page also shows 24.90 / 44.90 and 31.10 / 56.90 (mapping unverified) | https://www.odoo.com/pricing |
| 13 | Firefox Web Serial "yes from v151" | Also desktop only and behind a site-permission add-on (Firefox 151 released 2026-05-19) | https://developer.mozilla.org/en-US/docs/Mozilla/Firefox/Releases/151 |

Upgraded from [U] to [V] during this pass: GrapesJS licence (BSD-3-Clause via npm), PrestaShop (core OSL-3.0, modules AFL-3.0) and OpenCart (GPLv3) licences, Microsoft Outlook.com high-volume sender rules (5 May 2025, 5,000/day), PCI DSS future-dated requirements effective 31 Mar 2025 and 6.4.3/11.6.1 as script-protection controls (PCI SSC FAQ 1588), European Accessibility Act application date 28 Jun 2025 (secondary source only), CRA open-source steward regime. Added facts: Spain's software-producer deadline was not moved by RDL 15/2025; ViDA deemed-supplier postponement option to 1 Jan 2030; Odoo "Import Website" feature; France cross-check on impots.gouv.fr; Stripe.js, SQLite OPFS and WebKit storage statements re-confirmed on primary docs.

**Unresolved**

- eBay API state: developer.ebay.com returned HTTP 403 again; still unverified.
- Rust 1.99.0 is due on 2026-10-01 (RELEASES.md date) and was not yet live at check time; re-verify the "latest stable" pin.
- Odoo 20 general-availability date: only odoo.com's own list plus `release.py` at FINAL; no independent launch announcement found.
- Odoo Online pricing: which of the 19.90 / 24.90 / 31.10 and 35.90 / 44.90 / 56.90 figures is list price versus monthly billing.
- PCI DSS 6.4.3 and 11.6.1 exact requirement wording; whether payment terminals are in EAA scope (EC page lists ATMs, ticketing and check-in machines, not payment terminals); date of the RFC 8297 status change.
- Still unchecked: GDPR/ePrivacy, EU Digital Omnibus, TCPA, Apple Mail Privacy Protection, Poland KSeF, Italy POS-to-RT linking, Hungary/Greece/Romania/Bulgaria/Czech/Croatia/Slovakia POS regimes, cash-drawer pulse details, EAN price/weight prefix rules, PowerSync and Electric production fitness.
- Vendor claims left as vendor claims: fiskaly BSI certificate validity "2033" (seen on workspace.fiskaly.com) and the workspace blog post dated 2026-11-30 (a future date).
- Edition split (Community versus Enterprise) is inferred from the `addons/` tree, and Odoo model names were taken from file names and spot-checked in source, not read exhaustively.
- France: "B2C included" in e-reporting for large/mid-size firms comes from the Odoo docs; the tax authority page only says e-reporting follows the same calendar.

**Owner-constraint pass**
- Owner-constraint pass (2026-10-01): JS tooling is `vp` + pnpm only (no npm, npx, yarn, bun). This file had no npm-CLI commands or npm-CLI security material; its "npm" mentions are registry facts (package metadata, versions, licences) and were left as they are. One change: the Node SSR sidecar option no longer cites the bare `vite build --ssr` command and is run through `vp build` instead; the `vp build` docs document no `--ssr` pass-through, so configuring SSR in `vite.config.ts` (or confirming the flag) is flagged [I] [V https://viteplus.dev/guide/build].
