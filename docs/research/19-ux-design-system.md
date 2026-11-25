# obop research 19: UX, design system and metadata-driven UI

Date of research: 2026-10-01. Tags: **[V]** verified against a live source this session, **[I]** inference from verified facts, **[G]** guess or remembered fact not re-verified.

## Scope & method

Primary sources read this session: Odoo 19.0 and 20 source and docs through the GitHub API (`odoo/odoo`, `odoo/documentation`), Odoo 19 and 20 release notes, Twenty's source (`twentyhq/twenty`), Frappe's source, W3C (WCAG 2.2, APG, DTCG), MDN browser-compat-data and the `web-features` Baseline dataset v3.40.0, npm and crates.io registries, Vite+ release notes, and vendor docs for Linear, Notion, Airtable, Stripe, Pipedrive and Shopify. Web search engines were unavailable (bot-blocked), so sentiment evidence is limited to GitHub issues. JS-heavy doc sites (Attio, Shopify help, Intuit, EUR-Lex) returned little, and those gaps are listed at the end. Sibling reports already settle the stack: React 19 + TanStack + React Aria on Vite+ ([07](./07-vite-plus-frontend.md)) and JSON-AST views ([08](./08-metadata-platforms-dynamic-orm.md)). This report builds on them rather than repeating them.

## 1. State of the reference products (2026-10-01)

| Item | Finding |
|---|---|
| Odoo versions | 19.0 released Sept 2025 (release notes headed "September 2025") [V] https://www.odoo.com/odoo-19-release-notes; Odoo 20 release notes dated Sept 2026 [V] https://www.odoo.com/odoo-20-release-notes. The `20.0` stable branch is `(20, 0, 0, FINAL)` and is now GitHub's default branch; `master` is `(20, 1, 0, ALPHA, 1)` [V] https://github.com/odoo/odoo/blob/master/odoo/release.py |
| Odoo license | Community is LGPLv3 [V] https://github.com/odoo/odoo/blob/master/LICENSE. Enterprise modules are not in the repo (see below) |
| Odoo frontend | OWL. OWL 3 is at 3.0.0-alpha.49 (2026-09-08) [V] https://github.com/odoo/owl/releases, and `odoo/upgrade_code/owl3-migration.py` exists on master [V]. Odoo's UI will not become React-based [I] |
| Vite+ | **1.0.0 stable, 2026-09-28, MIT**; bundles vite 8.3.1, rolldown 1.2.11, vitest 5.0.1, oxlint 1.85.0, oxfmt 0.70.0, tsdown 0.23.0 [V] https://github.com/voidzero-dev/vite-plus/releases/tag/v1.0.0. Oxfmt and tsdown are still 0.x, so "1.0" is the wrapper's contract, not each tool's [I]. Announcement dated 2026-09-28, "free, open source under MIT" [V] https://voidzero.dev/posts/announcing-vite-plus-1-0. The bundled versions are pins: on 2026-10-01 npm already has oxlint 1.86.0, oxfmt 0.71.0, vitest 5.0.3 and rolldown 1.2.12 [V npm], while Vite 8.3.1 and tsdown 0.23.0 are still latest |
| Twenty | v2.44.0 (2026-10-01), React 19.2, Jotai, Linaria, Lingui, Apollo 4, dnd-kit, react-data-grid, Tiptap/BlockNote [V] https://github.com/twentyhq/twenty/blob/main/packages/twenty-front/package.json. License is AGPLv3 with `/* @license Enterprise */` files under a commercial license; `twenty-ui`, `twenty-sdk`, `twenty-client-sdk`, `create-twenty-app`, `twenty-shared` and `packages/twenty-apps` are MIT, and AGPLv3 carries an added "Twenty Application Exception" (section 7) so apps using the published APIs/SDKs are not forced to AGPL [V] https://github.com/twentyhq/twenty/blob/main/LICENSE |
| Frappe | Framework MIT, ERPNext GPL-3.0, Frappe CRM/Helpdesk/Books AGPL-3.0 [V, GitHub API]. v16.36.1 released 2026-09-30 |
| Build target | Vite 8 default `build.target` is `baseline-widely-available` = chrome111, edge111, firefox114, safari16.4, ios16.4 (Baseline widely available as of 2026-01-01 for this major) [V] https://vite.dev/config/build-options; Vite 8.3.1 is npm `latest` [V] |

**Community vs Enterprise is the key fact for "clone Odoo UX".** In the 19.0 Community repo, GitHub returns 404 for `web_gantt`, `web_map`, `web_cohort`, `web_grid`, `web_studio`, `web_enterprise`, `account_reports`, `approvals`, `documents`, `knowledge`, `sign`, `appointment`, `planning`, `voip`, `mrp_workorder`, and code search finds no `bank_rec_widget` [V]. Community does contain the view types `list`, `form`, `kanban`, `pivot`, `graph`, `calendar`, `search`, `qweb` ([ir_ui_view.py](https://github.com/odoo/odoo/blob/19.0/odoo/addons/base/models/ir_ui_view.py)), plus `activity` (in `mail`), `hierarchy` (`web_hierarchy`) and `spreadsheet` (LGPL-3) [V]. So Gantt, map, cohort, grid, Studio, financial-report drill-through, the bank-reconciliation widget and the approvals inbox are Enterprise behaviours. obop must implement them from behavioural specification, never from Enterprise code. The 19.0 tree has 638 addon directories, 231 of them `l10n_*` localizations [V]; the current stable 20.0 tree has 642 and still lacks every module in the list above (checked by directory listing), while keeping `web_hierarchy` and `spreadsheet` [V].

## 2. Odoo UX teardown

### 2.1 Shell and navigation
- **Action stack and breadcrumbs:** each menu click opens an *action* that resolves to a set of views over one model. The control panel holds breadcrumbs, search, view switcher, pager and cog menu [V] https://github.com/odoo/odoo/tree/19.0/addons/web/static/src/search (`breadcrumbs`, `control_panel`, `search_bar`, `search_panel`, `action_menus`). Odoo 19 renders the control panel before data loads and caches data and translations [V] https://www.odoo.com/odoo-19-release-notes.
- **View resolution:** `registry.category("views")` plus a `js_class` attribute on the arch picks the implementation; each view is Arch parser + Model + Controller + Renderer [V] https://github.com/odoo/odoo/blob/19.0/addons/web/static/src/views/view.js. This is the extension seam obop should copy in spirit.
- **App switcher:** Community has a `navbar` app-menu dropdown and `burger_menu` for mobile [V, `addons/web/static/src/webclient`]; the full-screen home-menu app grid lives in Enterprise `web_enterprise`, which is absent from the repo [V/G]. obop should ship a searchable grid plus "recent" in core.
- **Command palette:** Ctrl/Cmd+K, prefixes `/` menus, `@` users, `#` Discuss channels, `?` Knowledge; Ctrl+Enter opens in a new tab. The palette is a provider registry (`command_provider`, `command_categories`) [V] https://www.odoo.com/documentation/19.0/applications/essentials/keyboard_shortcuts.html. Odoo 20 adds swipe-down-to-open on mobile [V].
- **Shortcuts:** Alt+letter on Windows/Linux (Ctrl+letter on macOS): Alt+C new, Alt+S save, Alt+J discard, Alt+B back, Alt+N/P pager, Alt+Q search, Alt+1-9 menus; hold the modifier to overlay hints [V, same page].

### 2.2 Form anatomy (the part worth preserving)
From the 19.0 view reference ([V] https://www.odoo.com/documentation/19.0/developer/reference/user_interface/view_architectures.html, source read from `odoo/documentation`): `<form>` has `<header>` (workflow buttons + `statusbar` field with `statusbar_visible` and optional `clickable`), a `button_box` div of **smart buttons** (icon + `statinfo` counter that opens a filtered action), `<sheet>` containing `<group>`/`<notebook>`/`<page>`/`<separator>`, and `<chatter/>` for models inheriting `mail.thread`. Field attributes `readonly`, `required`, `invisible`, `column_invisible` are **Python expressions** evaluated client-side over the record's fields, `parent`, `context`, `uid`, `today`, `now`. The chatter has messages, internal notes and scheduled activities; Odoo 19 adds reply/forward to a specific message and PDF attachment preview, and Odoo 20 adds CC, pinned messages and a conversations-vs-tracking filter [V release notes].

### 2.3 List, kanban, search, mass edit
- **List:** `editable="top|bottom"`, `multi_edit`, `open_form_view`, `limit`, `count_limit`, `groups_limit`, `default_order`, `decoration-*` row styling, per-column `optional="show|hide"` [V, `list_arch_parser.js`]. Default page is **80 records with the count capped at 10,000**, a pager rather than virtual scroll [V] https://github.com/odoo/odoo/blob/19.0/addons/web/static/src/model/relational_model/relational_model.js. Odoo 19 adds drag between groups, double-click column border to auto-size, and incremental mass edit (`*=1.1`) [V]; Odoo 20 adds multi-record drag-and-drop in list and kanban, and `+=`, `-=`, `*=`, `/=` on duration fields [V].
- **Kanban:** `<templates><t t-name="card">`, `progressbar`, `quick_create`, `group_create`, `records_draggable`, `highlight_color`, multi-select with Alt+click (19) [V].
- **Search:** facets, preconfigured filter groups (OR within a group, AND between groups), custom-filter domain tree editor, group-by, favorites. 19 makes favorites editable from the search panel and shareable with specific users (and shows user avatars in custom filters); 20 makes favorites keep visible columns, adds relative-range tooltips ("Last 30 days" shows the dates) and **shareable links that preserve search, filters and grouping** [V].
- **Empty states:** `sample_server.js` fills empty views with fake placeholder data behind a "no content" helper [V] https://github.com/odoo/odoo/tree/19.0/addons/web/static/src/model.
- **Field widgets:** 68 widget directories in `views/fields/` (19.0; 75 in 20.0) (monetary, many2one+avatar, many2many_tags, statusbar, priority, progress_bar, signature, pdf_viewer, float_time, properties, domain, `numpad_decimal_hook`, etc.) [V].

### 2.4 Mobile, offline, dark mode
- **Mobile:** PWA recommended (and lists SSO authentication among its features); store apps support multiple accounts but **not SSO** [V] https://www.odoo.com/documentation/19.0/administration/mobile.html. 19 shows the first form button and stacks the rest under "...", compact statusbar, bottom-sheet date picker, pull-to-refresh; 20 adds tablet bottom sheets, a new touch form design and file sharing from the phone to create bills, expenses, leads, tasks [V].
- **Offline:** Odoo 19 lets you re-open previously visited views and records **read-only**, with a 2 GB cap, and a hard refresh wipes the cache [V] https://www.odoo.com/documentation/19.0/applications/general/offline_mode.html. Odoo 20 lets you create, edit, archive, unarchive and delete offline and re-run previous searches [V, release notes; conflict semantics not documented in what I read]. POS has its own outbox: products and customers are cached at session start, orders sync later, payment terminals, real-time stock sync and customer registration are unavailable offline (requested invoices are generated after reconnect; printers and scales on the local network keep working), and the user is told not to close, reload or clear the cache until synced [V] https://www.odoo.com/documentation/19.0/applications/sales/point_of_sale/use.html.
- **Dark mode:** Community ships `*.dark.scss` and `web.assets_backend_lazy_dark` bundles, but `ir.http.color_scheme()` returns `"light"` in Community [V]. The user-facing toggle appears to be an Enterprise override [I]. The dark theme is a separate compiled asset bundle rather than runtime tokens, which is a server-side rebuild cost obop should not inherit.
- **Barcode:** uses native `BarcodeDetector` if present, else loads a ZXing JS library [V] https://github.com/odoo/odoo/blob/19.0/addons/web/static/src/core/barcode/barcode_video_scanner.js.

### 2.5 Weaknesses (evidence-labelled)
1. **Accessibility is a known gap.** Issue #13950 "WCAG2 Compliance?" (NVDA, unlabeled view-switcher buttons, missing headings, autocomplete not announced) got the maintainer reply "not a priority currently" (2018-05-01; a 2019 reply from the same Odoo developer said substantial screen-reader work had been done ~2017; the issue is now closed); a 2021 commenter on CE 12 reported blocking screen-reader problems in kanban views, tab order and navigation [V] https://github.com/odoo/odoo/issues/13950. A June 2026 issue shows read-only fields producing dangling `<label for>` on 17/18/19 [V] https://github.com/odoo/odoo/issues/270715. I found no VPAT or WCAG conformance statement in the docs repo [V by absence].
2. **Alt+letter shortcuts** collide with browser/OS accelerators and screen readers, and differ per OS [I].
3. **Imports "are permanent and cannot be undone"**; the docs advise filtering by created-on to find them; Odoo 19 at least shows the imported records after a successful import, but offers no rollback [V] https://www.odoo.com/documentation/19.0/applications/essentials/export_import_data.html, https://www.odoo.com/odoo-19-release-notes. No QuickBooks migration guide exists in the 19.0 docs (only a check-stock mention) [V by code search].
4. **Pager-bound lists** (80 per page) instead of virtualized scrolling; no spreadsheet-style range selection or paste [I from source].
5. **Offline was read-only until 20**; 20's write path is new and unproven [V/I].
6. **Modifiers are Python strings in XML**, and inheritance is xpath-based, so view extensions break when the base arch changes [I; common Odoo-developer complaint, [G]].
7. **Enterprise gating** of the most-praised UX (Studio, Gantt, reconciliation, reports drill-down, approvals) [V].

## 3. Modern reference UX: patterns to adopt

| Product | Verified patterns | Source |
|---|---|---|
| **Linear** | `/` global search, `O` then `I` quick open, typed prefixes (`i p u t l f d`), `@` builds filters; custom views saved from any filtered list with Alt+V, scoped to workspace/team, owned, favoritable, subscribable (Slack/notify), URL carries temp filters; display options are personal unless "Set as default"; sub-grouping, sticky group headers, drag-between-groups rewrites properties; **Inbox**: `G I`, `J/K`, `U` read, `H` snooze with natural-language times ("next quarter"), Backspace delete, 2,000 cap, no archive | https://linear.app/docs/search, /custom-views, /display-options, /inbox |
| **Notion** | Table/board/timeline/calendar/list/gallery/chart views; filters nested 3 deep with AND/OR; per-view property visibility; "Save for everyone" vs personal; open record as side peek, center peek or full page; freeze column | https://www.notion.com/help/views-filters-and-sorts |
| **Airtable** | Grid/form/calendar/gallery/kanban/timeline/list/gantt; collaborative vs personal vs **locked** views (personal/locked on paid plans); row heights short to extra-tall; record coloring; views over 5,000 visible records load a 1,000-record preview | https://support.airtable.com/v1/docs/getting-started-with-airtable-views |
| **Twenty** | Views are DB rows: `type` TABLE/KANBAN/CALENDAR/LIST plus *widget* variants, `openRecordIn` SIDE_PANEL or RECORD_PAGE (marked deprecated on the view entity, superseded by the object-metadata setting and a member preference), `visibility` WORKSPACE or UNLISTED, kanban aggregate op, calendar layout DAY/WEEK/MONTH, `groupLoadLimit`; child entities for view-field (+field-group), view-filter(+group), view-sort, view-group. Filter operands include IS_RELATIVE, IS_EMPTY, VECTOR_SEARCH. **Command menu items** are metadata (`hotKeys`, `availabilityType` GLOBAL / GLOBAL_OBJECT_CONTEXT / RECORD_SELECTION / FALLBACK, `conditionalAvailabilityExpression`; the older UI-context enum `CommandMenuItemViewType` has GLOBAL / INDEX_PAGE_BULK_SELECTION / INDEX_PAGE_SINGLE_RECORD_SELECTION / INDEX_PAGE_NO_SELECTION / SHOW_PAGE / PAGE_EDIT_MODE). **Page layouts**: tabs of widgets (26 widget types incl. FIELDS, RECORD_TABLE, TIMELINE, TASKS, NOTES, FILES, FRONT_COMPONENT). Extensions render via `remote-dom` + `@quilted/threads` sandbox. Table has a `virtualization` module | https://github.com/twentyhq/twenty/tree/main/packages/twenty-shared/src/types, `.../twenty-server/src/engine/metadata-modules/view`, `.../command-menu-item`, `.../twenty-front-component-renderer` |
| **Pipedrive** | Kanban pipeline with cards configurable, deal "rotting" flag, drag to another stage or pipeline, delete restorable for 30 days, favorited filters, sort by next activity | https://support.pipedrive.com/en/article/pipeline-view |
| **Stripe Dashboard** | Search syntax `field:value`, `-negation`, `>`/`<`/`..` ranges, `is:type`, dates like "last week", **search terms live in the URL (bookmarkable)**; `?` opens shortcuts; "Shortcuts" sidebar of pinned and recent pages; customizable Home widgets; sandboxes; supports last 20 Chrome/Firefox/Edge and last 4 Safari majors. Stripe Apps limit custom styling to protect contrast and offer ContextView (side-by-side drawer) / FocusView (blocking) / SettingsView | https://docs.stripe.com/dashboard/search, /dashboard/basics, /stripe-apps/design |
| **Shopify admin** | Polaris is now "web components"; Money field and Number with tabular figures exist; filters/bulk actions are composed, not components. I could not verify saved-view behaviour | https://shopify.dev/docs/api/app-home/polaris-web-components |
| **Frappe** | DocType JSON drives List, Report, Kanban, Calendar, Gantt, Dashboard, Map, Image, Tree and Inbox views and 48 form-control files plus a `quill-mention` subdirectory (barcode, geolocation, signature, rating, duration, currency, table-multiselect) | https://github.com/frappe/frappe/tree/develop/frappe/public/js/frappe/views |

Shared pattern: **a "view" is a saved, scoped, URL-addressable query + presentation**, with personal/shared/default layers, and records open in a side panel without losing list context.

## 4. What SMB users and accountants need

| Need | Minimum obop behaviour | Evidence |
|---|---|---|
| Keyboard-centric data entry | Full grid keyboard model (section 7), numpad-decimal honoured, Enter/Tab flow configurable per table, mnemonic-free single shortcut registry with cheat sheet (`?`) | APG grid [V]; Odoo `numpad_decimal_hook` [V]; Odoo 20 "optimized data entry: quick reconciliation shortcuts, batching" [V] |
| Spreadsheet-like grids | Range select, copy/paste TSV from Excel with validation preview, fill-down, incremental edit (`+=`) | AG Grid keeps range selection and clipboard in paid Enterprise ($999/dev) [V] https://ag-grid.com/license-pricing/; Odoo has `*=` edits [V] |
| Bulk and inline edit | `multi_edit` equivalent: select N rows, edit one cell, confirm "apply to N" with count | Odoo [V] |
| Undo | Session undo for drafts, 10 s undo toast for archive/delete, **reversal documents** for posted entries; imports are batch-undoable until posted | Odoo imports irreversible [V]; Odoo 19 Gantt undo [V]; posted-entry immutability [I] |
| Saved filters/views | Four scopes (personal, shared, system, locked), shareable URLs | Section 3 [V] |
| Global search / command bar | One bar: records (`field:value` grammar), navigation, commands scoped to selection | Stripe, Linear, Odoo, Twenty [V] |
| Drill-through | Every aggregate cell (pivot, chart, report line) carries a `drill` descriptor (model, domain, grouping) opening the source list; URL-addressable | Odoo reports with drill-down are Enterprise [V]; Stripe bookmarkable search [V] |
| Approval inbox | Unified inbox: approvals + mentions + activities + system alerts; inline approve/reject with reason; snooze; keyboard triage | Linear Inbox [V]; Odoo Approvals not in Community [V]; Odoo 20 activities assignable to a role [V] |
| Notification center | In-app inbox is source of truth; email and Web Push are channels; per-event preferences | Web Push is Baseline since 2023-03-27, but iOS needs a Home Screen web app and a user gesture [V] https://webkit.org/blog/13878/web-push-for-web-apps-on-ios-and-ipados/ |
| Audit confidence | Visible chatter/tracking, "who changed what", lock dates and posted-state banners | [I] |

## 5. Architecture of a metadata-driven UI

### 5.1 Layering
`meta-model (Rust, source of truth)` → `view definitions (JSON AST, versioned)` → `registries (view types, field renderers, actions, commands, services)` → `React renderers`. This matches [07 §3.11](./07-vite-plus-frontend.md) and [08](./08-metadata-platforms-dynamic-orm.md). Rules:
1. View definitions are **data**, stored in the database, validated in Rust (schemars 1.2.2 for JSON Schema, ts-rs 12.0.1 or specta for TypeScript types [V crates.io]), diffable and diff-gated in CI.
2. The server resolves the effective view per request: base view + module patches + company/role overrides + user layer. The client never sees XML or xpath.
3. The renderer registry is keyed by `(viewType)` and `(fieldType, widget)`; third parties add entries, they do not patch core code.

### 5.2 View-definition schema (sketch)
```jsonc
{
  "id": "sale.order.form", "schema": 1, "model": "sale.order", "type": "form",
  "slots": ["header", "smart_buttons", "sheet.main", "sheet.lines", "side"],
  "header": {
    "actions": [{ "action": "sale.confirm", "visible": "state in ['draft','sent']", "primary": true }],
    "status": { "field": "state", "visible": ["draft","sent","sale"], "clickable": false }
  },
  "smart_buttons": [{ "id": "invoices", "icon": "receipt", "value": "invoice_count",
    "label": "Invoices", "opens": { "action": "account.move.list", "domain": [["order_id","=","$id"]] } }],
  "sheet": { "main": [{ "group": { "cols": 2, "children": [
      { "field": "partner_id", "widget": "many2one", "props": { "create_inline": true } },
      { "field": "payment_term_id", "readonly": "state != 'draft'" } ] } }],
    "lines": [{ "field": "line_ids", "view": { "type": "list", "editable": "bottom", "columns": ["product_id","qty","price","tax_ids","subtotal"] } }] },
  "side": { "chatter": { "features": ["messages","notes","activities","followers","attachments"] } }
}
```
Design decisions: named **slots** for extension (replaces xpath); every conditional is an expression AST (5.3); sub-views for x2many are inline view definitions; `opens` is a typed action reference, never a string of JS; `schema` versions let old stored overrides migrate forward.

### 5.3 Expression language for modifiers, domains and visibility
Odoo evaluates Python strings in the browser, which forces a Python-expression parser into JS [V]. Twenty stores `conditionalAvailabilityExpression` as a string [V]. For obop the same expression must run in Rust (authorization, server validation, default/onchange) and in the client (instant show/hide).

| Option | Status | Fit |
|---|---|---|
| Own JSON AST (domains, modifiers) with text sugar compiled to it | Sibling-recommended [08 §7] | **Chosen as storage format**; versionable, SQL-compilable |
| CEL (`cel` 0.14.5 Rust; `@marcbachmann/cel-js` 8.0.0, `@bufbuild/cel` 0.6.1 JS) [V registries] | Spec from Google, multiple impls | Good **authoring syntax** compiling to the AST; non-Turing-complete, type-checkable |
| JSONLogic (`json-logic-js` 2.0.5; Rust `jsonlogic-rs` 0.5.0, last release 2024-12) | Aging | Reject as primary |
| Rhai / evalexpr | Rust-only | Server-only |

### 5.4 Component model
- **Field renderer contract** [I]: `read`, `edit`, `cell` (compact grid editor), `filter` (operators per type), `aggregate`, `format/parse` (locale-aware), `validate`, `exportValue`, `importParse`, `a11yName`. Every type must implement `cell` and `filter`, otherwise it cannot appear in lists or search.
- **Field-type catalogue** (merge of Odoo widgets, Twenty's 25 types, Frappe controls): char, text, rich text, integer, decimal, **monetary (amount + currency as one logical field)**, percentage, duration, boolean, selection, badge/state, priority/rating, many2one, many2many tags, x2many, date, datetime, date range, address (composite), full name (composite), phone(s), email(s), link(s), file(s), image, signature, PDF viewer, barcode, color, progress, gauge, JSON, geolocation, reference (polymorphic), user-defined properties, actor/created-by [V in Twenty/Odoo/Frappe listings].
- **Extension tiers** [I]: (a) declarative view patches for all modules; (b) prebuilt ESM widgets against a versioned UI SDK (07 §3 / Implication 6); (c) sandboxed remote-dom components for untrusted marketplace code, following Twenty's front-component renderer and Stripe's limited-styling model [V].

### 5.5 Customization ("Studio" without Enterprise)
Ship an in-product view editor that edits the **user/company override layer** of the same JSON (drag fields into slots, reorder columns, conditional visibility via a form builder that writes the AST). Odoo Studio is Enterprise-only [V]; Twenty lets admins reorder command-menu items and page-layout tabs as metadata [V]. Show "customized" badges and a one-click "reset to default", mirroring Linear's "Reset to default" [V].

### 5.6 Design tokens and theming
- Use the **W3C Design Tokens Format Module 2025.10**, the first stable edition (2025-10-28): supports Display P3, Oklch and all CSS Color 4 spaces, light/dark and brand themes without duplicate files, aliases; implemented by Style Dictionary, Tokens Studio, Terrazzo [V] https://www.w3.org/community/design-tokens/2025/10/28/design-tokens-specification-reaches-first-stable-version/.
- Three tiers: *primitive* (oklch ramps, spacing scale), *semantic* (`--surface`, `--text-muted`, `--border-focus`, `--money-negative`), *component* (`--grid-row-height`). Emit **CSS custom properties** switched by `data-theme`/`data-density` attributes at runtime; no per-theme asset bundle (contrast: Odoo's compiled dark bundle [V]).
- Do not rely on `light-dark()` as the only mechanism yet: it became Baseline *newly* available 2024-05-13, so it only becomes "widely available" on about 2026-11-13 (30 months later; not yet on 2026-10-01), while Vite's default target is Chrome 111 / Safari 16.4 (Baseline widely available as of 2026-01-01, also ios16.4) [V web-features; date arithmetic I]. `color-mix()`, container queries, `:has()`, logical properties, cascade layers, `inert`, `<dialog>` are Baseline widely available [V]. `popover` is newly available (2025-01-27); CSS anchor positioning is split in web-features 3.40.0: most properties (`anchor-name`, `position-area`, fallbacks) became newly available on 2026-01-13 (Chrome 125+, Firefox 147, Safari 26), `position-anchor` on 2026-09-14 (Chrome/Firefox 151, Safari 27), but the group as a whole is still not Baseline because `position-visibility: anchor-valid/anchor-visible` is Safari-only [V]; both sit below Vite's Chrome 111 / Safari 16.4 floor, so keep a positioning library (e.g. Floating UI) until they are widely available [I].
- Density tokens: *compact* (accountant tables), *comfortable*, *touch* (POS/mobile, 48 px targets). Tenant branding is limited to an accent hue plus logo, with an automated contrast check, as Stripe restricts colours "to ensure a high accessibility bar" [V].
- Icons: SVG with accessible names; Odoo 20 just migrated from Font Awesome font icons to Material Symbols [V].

## 6. Catalogue of view types obop must ship

| # | View | Tier | Odoo | Notes for obop |
|---|---|---|---|---|
| 1 | **List / editable grid** | v1 | Community | Virtualized, grouped, aggregates footer, inline + multi-edit, range select/paste (section 7) |
| 2 | **Form** | v1 | Community | Statusbar, smart buttons, slots, chatter, autosave-on-blur, dirty-state guard |
| 3 | **Kanban** | v1 | Community | Grouped columns, progress bar, quick create, "Move to..." menu (WCAG 2.5.7), rotting flag |
| 4 | **Search/filter model** (facets, favorites, domain editor) | v1 | Community | Not a view: shared by all views; Stripe-style grammar in the omni-bar |
| 5 | **Pivot** | v1 | Community | Cell click = drill-through; export |
| 6 | **Chart** (bar, line, pie, stacked) | v1 | Community (`graph`) | Accessible data-table fallback and text summary |
| 7 | **Calendar** | v1 | Community | Day/week/month, drag reschedule with keyboard alternative |
| 8 | **Activity matrix** | v1 | Community (`mail`) | Records x activity types |
| 9 | **Dashboard** | v1 | Community spreadsheet dashboards | Widgets = saved views + KPIs; private and shared |
| 10 | **Report viewer** (financial statements, aging) | v1 | Enterprise | Collapsible lines, comparison columns, every number drillable |
| 11 | **Reconciliation split view** (two-pane match) | v1 | Enterprise | Keyboard-first; suggestion engine; batch validate |
| 12 | **Hierarchy/tree** (chart of accounts, BoM, org chart) | v1 | Community (`web_hierarchy`) | Treegrid semantics |
| 13 | **Inbox** (approvals, activities, notifications) | v1 | Enterprise/Discuss | Linear-style triage |
| 14 | **Settings view** | v1 | Community | Searchable, per-app blocks |
| 15 | **Import wizard** | v1 | Community | Mapping, dry-run, batch undo (section 13) |
| 16 | **Gantt/timeline** | v2 | Enterprise | Dependencies, undo (Odoo 19), buffers (Odoo 20) |
| 17 | **Spreadsheet** (reports, ad-hoc) | v2 | Community (`spreadsheet`) | Odoo's o-spreadsheet is LGPL-3; Univer core is Apache-2.0 [V] |
| 18 | **Timesheet grid** (weekly entry matrix) | v2 | Enterprise (`grid`) | Keyboard matrix entry |
| 19 | **Map** | v3 | Enterprise | Needs tile-provider decision |
| 20 | **Cohort** | v3 | Enterprise | |
| 21 | **Gallery/catalogue** | v2 | n/a (Notion/Airtable) | Products, documents |
| 22 | **POS screen**, **barcode scan screen** | v1 | Community | Purpose-built, not metadata-rendered |
| 23 | **Portal/website** | separate app | Community | See 07 |

## 7. Grid behaviour specification

- **Semantics:** `role="grid"` with one tab stop, roving focus; `aria-rowcount/colcount` and `aria-rowindex` for virtualization; Enter/F2 enter edit mode, Esc cancels and returns to navigation; alphanumeric starts edit; Ctrl+Home/End; Shift+Space selects row; Ctrl+A [V] https://www.w3.org/WAI/ARIA/apg/patterns/grid/. Treegrid for hierarchy; plain `table` for read-only reports.
- **Data entry flow:** Enter commits and moves down (setting: right); Tab commits and moves right, wrapping to the next row's first editable cell; last-row Enter creates a new line; Ctrl+Enter saves/posts; Ctrl+D fill-down [G, Excel/Sheets convention]; multi-cell paste from TSV with per-cell validation and a preview of rejects.
- **Edit rules:** optimistic commit per cell with server validation and inline error; "apply to N selected" for multi-edit; expression edits `=`, `+=`, `*=` for numbers and durations [V in Odoo 19/20].
- **IME (CJK):** ignore Enter and shortcuts while `event.isComposing` [V, `KeyboardEvent.isComposing` is part of the `keyboard-events` feature, Baseline widely available since 2018-01-29 in web-features 3.40.0]; Safari before version 27 fires `compositionend` before the confirming keydown so `isComposing` is wrongly `false` [V MDN BCD, WebKit bug 165004; BCD lists it fixed in Safari 27], so also guard with `keyCode === 229` plus a one-tick flag for older Safari [G; test in Chrome, Firefox, Safari with real Japanese/Chinese/Korean IMEs].
- **Virtualization:** TanStack Table 9.2.4 + TanStack Virtual (both MIT) [V npm]; canvas grids (Glide) fail accessibility ([07](./07-vite-plus-frontend.md)). Handsontable is dual-licensed (non-commercial/evaluation terms vs a paid commercial licence agreement; GitHub reports no SPDX licence) [V] https://github.com/handsontable/handsontable/blob/develop/LICENSE.txt, so it is excluded from core like AG Grid Enterprise. Keep Oxlint's `jsx-a11y` rules on (36 rule files in `oxc_linter`) [V] plus axe-core 4.13.0 (MPL-2.0) in Playwright [V].

## 8. Accessibility: WCAG 2.2 AA as a hard requirement

WCAG 2.2 became a W3C Recommendation on 2023-10-05; the current edition at /TR/WCAG22/ is dated 2024-12-12 (a later edition of the same Recommendation; SC 4.1.1 Parsing is removed) [V] https://www.w3.org/TR/WCAG22/, https://www.w3.org/WAI/standards-guidelines/wcag/new-in-22/. Design rules derived from the new and relevant criteria:

| SC | Level | obop rule |
|---|---|---|
| 2.4.11 Focus Not Obscured (Min) | AA | Sticky headers, chatter, side panels and bottom bars must not fully cover a focused control; set `scroll-padding` from tokens [V] |
| 2.5.7 Dragging Movements | AA | Every drag (kanban, list reorder, Gantt, column resize) has a single-pointer alternative, e.g. "Move to..." menu or click-select-then-arrow; keyboard support alone does not satisfy it [V] https://www.w3.org/WAI/WCAG22/Understanding/dragging-movements.html |
| 2.5.8 Target Size (Min) | AA | 24x24 CSS px targets (exceptions for spacing/inline). Dense accountant rows must still give checkboxes and row actions a 24 px hit area; touch density uses 48 px [V] |
| 3.3.7 Redundant Entry | A | Never re-ask known data within a flow (billing to shipping, wizard steps) [V] |
| 3.3.8 Accessible Authentication | AA | Password managers and paste work; no cognitive-test CAPTCHA puzzles [V] |
| 3.2.6 Consistent Help | A | Help entry in the same place on every screen [V] |
| 2.1.4 Character Key Shortcuts | A | Linear-style single-key shortcuts (`G I`, `H`) **must be disable-able or remappable** or only active on focus; Alt/Ctrl combos are exempt [V] https://www.w3.org/WAI/WCAG22/Understanding/character-key-shortcuts.html |
| 4.1.3 Status Messages | AA | Toasts, autosave, "N records updated" use `role=status`/live regions with the whole string (`aria-atomic`); errors use `role=alert` sparingly [V] |

Also 4.5:1 text and 3:1 non-text contrast, 320 px reflow, text-spacing override [standard WCAG 2.x, not re-fetched], forced-colors mode (Baseline widely available [V]). Manual NVDA/VoiceOver passes on five core flows (login, list, form, grid entry, reconciliation) are release gates; automated checks cover only part of the criteria [I].

**Regulation:** the European Accessibility Act lists e-commerce and banking services among covered services [V, Commission page]; it applies from 28 June 2025, with service microenterprises (fewer than 10 staff and turnover or balance sheet up to EUR 2 million) exempt [V, secondary source https://en.wikipedia.org/wiki/European_Accessibility_Act; the Commission page and EUR-Lex text were not retrievable, so the primary-text check is still open]. A Commission news item dated 2026-09-07 announcing an updated EN 301 549 could not be re-found (unverified); what is verifiable is that ETSI's publication directory lists EN 301 549 V4.1.1 as a final publication dated 2026-09-02, newer than V3.2.1 (2021-03), content not read [V https://www.etsi.org/deliver/etsi_en/301500_301599/301549/]. The storefront and portal are the legally exposed surfaces; the back office matters for public-sector and large-customer procurement (a VPAT/EN 301 549 statement is a sales asset) [I].

## 9. Internationalization, RTL, locale formats, CJK

- **Locale data:** CLDR 48 is current stable (2025-10-29; 48.2 on 2026-03-17); CLDR 49 is in beta (beta1 2026-09-22, beta2 2026-09-25) [V https://github.com/unicode-org/cldr/releases]. Rust: ICU4X `icu` 2.3.1 (2026-08-20) [V crates.io]. Browser: `Intl` for client formatting.
- **Dates/time:** `Temporal` is **not Baseline** (Chrome 144, Firefox 139, Safari only in preview) [V web-features 3.40.0] → use `temporal-polyfill` 1.0.5 [V npm] or Rust-side formatting. Model accounting dates as plain dates (no timezone) and events as instants + IANA zone [I].
- **Numbers/money:** never floats; Rust `rust_decimal` 1.43.0 / `bigdecimal` 0.4.11, client strings + a shared decimal library or WASM [V versions, I design]. Accept both `.` and `,` as decimal input (Odoo 20 does) [V]. Monetary = amount + currency with ISO 4217 minor units; accounting negative formatting via `Intl.NumberFormat` `currencySign: "accounting"` [V, Baseline widely available since 2020-03-28 in web-features]. Tabular figures for columns (Polaris does this) [V].
- **Phone/address:** `libphonenumber-js` 1.13.14 and Rust `phonenumber` 0.3.10 [V]. Per-country address layouts must be data (format strings per country, including CJK largest-to-smallest ordering); I could not verify a maintained Rust address-format crate [gap].
- **CJK:** `Intl.Segmenter` is Baseline newly available (2024-04-16) [V] for word search/tokenization; Postgres full-text search needs a CJK strategy [I, see 08]; normalise full-width digits/letters (NFKC) in numeric and barcode fields [I]; family-name-first display and optional reading (furigana) fields [I]; verify Japanese-era calendars via `Intl` [G].
- **RTL/bidi:** `dir` on `<html>` (never CSS for base direction), logical properties (Baseline since 2021-09-20, widely available since 2024-03-20), `dir="auto"` + `<bdi>` for user-supplied strings, isolate LTR runs (invoice numbers, SKUs, phone numbers) inside RTL text [V https://www.w3.org/International/questions/qa-html-dir]. Mirror directional icons by rule, not charts' time axes [I]. Odoo 20's Sign app added Cyrillic/Devanagari/Arabic signature alphabets [V]; RTL quality of Odoo's back-office UI was not assessed [gap].
- **Translations:** Lingui 6.8 + ICU MessageFormat + `.po` (07). Translatable *data* fields (product names) need per-language values plus import/export per language, as Odoo 20 ships [V]; an in-context "translation mode" opened from the command palette is worth copying [V].

## 10. Responsive, mobile and POS

- Use **container queries** (Baseline since 2023-02-14, widely available since 2025-08-14 [V]) so the same view definition adapts in a side panel, full page or tablet split; `any-pointer: coarse` to raise target sizes [V https://web.dev/articles/accessible-tap-targets, which recommends about 48 px with 8 px spacing].
- Mobile patterns to copy from Odoo 19/20: primary button + overflow, compact statusbar, bottom-sheet pickers, pull-to-refresh, swipe-to-command-palette, share-to-create (bill, expense, lead) [V]. Lists become cards; forms become single-column with sticky action bar; chatter moves below.
- **Scanning:** `BarcodeDetector` is not Baseline: Chrome Android 83+, Chrome desktop only on ChromeOS/macOS, Safari 17 behind a flag, Firefox unsupported [V MDN BCD]. Ship `barcode-detector` 3.2.2 (ponyfill; depends on `zxing-wasm` 3.1.3, while `zxing-wasm` itself is at 3.1.4) [V npm] as the fallback, as Odoo does with ZXing [V]. Most warehouse/POS scanners are keyboard-wedge devices, so support HID input (burst keystrokes ending in Enter) first; Odoo has a dedicated `barcode_service` [V].
- **Hardware:** WebHID, WebUSB and Web Serial are all not Baseline (web-features 3.40.0: Web Serial is in Chrome/Edge 89, Chrome Android 148 and Firefox 151, no Safari) [V]; receipt printers, scales, cash drawers and payment terminals need a native shell (Tauri 2.12.1 [V crates.io], per 07) or a local bridge like Odoo's IoT box [V docs path].
- **POS tablet:** touch density, no hover dependence, 48 px minimum targets, large numeric pad, offline banner, "continue with limited functionality" pattern from Odoo POS [V].

## 11. Offline and PWA

Design three tiers [I]:
1. **Read cache** of visited views, records and search results (IndexedDB/OPFS; both Baseline widely available [V]) with a storage quota and LRU purge (Odoo: 2 GB cap [V]).
2. **Queued mutations** for allow-listed models (timesheets, stock counts, expenses, CRM notes): client-generated UUIDs, idempotency keys, per-model conflict policy (last-writer-wins for notes, reject-and-review for anything that touches stock or ledger), a visible "N changes pending" indicator.
3. **POS/warehouse outbox** with explicit degraded-feature list, like Odoo POS [V].

Platform facts: Background Sync and Periodic Background Sync are **not Baseline** [V web-features], so sync must run when the app is open; Safari grants origins up to 60% of disk in a browser and treats Home Screen apps with the same quotas, data is best-effort unless `navigator.storage.persist()` is granted [V https://webkit.org/blog/14403/updates-to-storage-policy/]; the Badging API is not Baseline [V]; iOS Web Push requires the Home Screen install and a tap-initiated permission request [V]. Full local-first sync engines exist (Zero 1.9.0 and PowerSync 2.4.1 web SDK, Apache-2.0 on npm [V]; PowerSync Rust SDK is beta [V]) but imply running their sync service next to Postgres; defer [I]. Use `vite-plugin-pwa` 1.3.0 / Workbox 7.4.1 for the shell [V versions].

## 12. Performance budgets

External, verified thresholds: INP <= 200 ms at p75, LCP <= 2.5 s, CLS <= 0.1 [V https://web.dev/articles/vitals]. Comparative facts: Odoo pages at 80 rows and counts to 10,000 [V]; Airtable previews 1,000 of >5,000 visible records [V].

Proposed obop budgets (targets to be validated in CI, **not measured facts**) [I]:

| Area | Budget |
|---|---|
| Shell JS (gzip) | <= 250 kB to interactive; each module/view lazily loaded <= 100 kB |
| View definition payload | <= 20 kB per view, ETag-cached, prefetched on menu hover |
| List | Render window <= 100 DOM rows; 100k-row scroll stays 60 fps on a mid-range laptop; server keyset pages of 200-500 rows; count capped and labelled ("10,000+") |
| Interaction | Cell edit echo <= 50 ms, optimistic commit <= 100 ms visual, server round-trip p95 <= 300 ms |
| Navigation | List -> record open in side panel <= 150 ms from cache |
| Gate | Lighthouse/INP checks and a "100k-row" Playwright perf test as `vp run` tasks |

## 13. Onboarding, first run and import

- **Setup wizard:** country/locale (loads chart of accounts, taxes, address/date/number formats, currency), company identity, fiscal year, apps to enable (app switcher populated accordingly), invite users with role templates, optional **sample data** toggle. Twenty has an `onboarding` module [V]; Odoo embeds onboarding panels (e.g. `account/static/src/components/onboarding`) [V].
- **Empty states:** show ghost sample rows labelled "sample", as Odoo's `sample_server` does [V], plus one primary call to action and a link to import.
- **Import pipeline** [I]: upload (xlsx/csv) -> auto-map columns (fuzzy on names + saved mapping per source) -> **dry-run** with row-level errors -> commit as a tracked *import batch* (`batch_id` tag on every created record) -> undo/rollback allowed until records are posted or referenced. This fixes Odoo's "cannot be undone" [V]. Odoo's own templates and external-ID matching for updates are the baseline to match [V].
- **Sources:** (1) Odoo: prefer direct JSON-RPC/XML-RPC extraction (customers, products, chart of accounts, open invoices, stock) over CSV; (2) QuickBooks Online: API or report exports [G, Intuit docs not retrievable]; (3) Excel/CSV with downloadable templates per model. Open balances, not full history, is the default migration posture for SMBs [I].

## 14. UX principles for obop

1. Every screen is a **view over a model**, addressed by URL (action, view type, domain, group-by, record).
2. **Keyboard first, mouse friendly**; shortcuts discoverable (`?` sheet, hold-key hints) and remappable.
3. **Context survives**: records open in a side panel by default; list position, filters and scroll are never lost.
4. **Saved means scoped**: personal, shared, system, locked; always resettable.
5. **Direct manipulation has a non-drag twin** (WCAG 2.5.7).
6. **Undo for drafts, reversal for posted**: never pretend ledger data is editable.
7. **Numbers are sacred**: never floats; locale-correct, tabular, drillable to source.
8. **Fast is a feature**: optimistic UI, virtualization, cached views; budgets enforced in CI.
9. **One search box** for records, navigation and commands, with a legible grammar.
10. **Progressive density**: compact, comfortable, touch; same definitions.
11. **Customize without forking**: override layers over immutable module defaults.
12. **Errors are local and actionable**, and announced to assistive tech.
13. **Honest offline**: show what works, what queues, what cannot.
14. **Accessible by construction**: primitives and lint/axe gates, not retrofits.
15. **No paid tier for core UX**: Gantt, drill-through reports, reconciliation and approvals are first-class.

## Confidence & gaps

- **High:** version/licence facts (Vite+, Odoo, Twenty, Frappe), Odoo view-architecture details, Odoo 19/20 feature lists (official release notes), WCAG 2.2 criteria, Baseline/browser-support data, DTCG 2025.10, package versions.
- **Medium:** Linear/Notion/Airtable/Stripe/Pipedrive feature claims (from vendor docs, not hands-on); Odoo 20 offline-write behaviour (release-note sentence only; conflict handling unknown); Community-vs-Enterprise inferences from file absence (GitHub code-search can be incomplete).
- **Not verified / gaps:** Attio and Shopify saved-view behaviour; QuickBooks Online export/API specifics and rate limits; EAA application date from a primary text (28 June 2025 is confirmed only via a secondary source, Wikipedia); the 2026-09-07 Commission EN 301 549 news item and the contents of EN 301 549 V4.1.1; Odoo user-sentiment beyond GitHub issues (no forum/Reddit/G2 access, search engines blocked); any hands-on timing of Odoo or Twenty grids; Safari IME behaviour in practice (the ordering bug is documented in MDN BCD but untested here); iOS eviction behaviour for Home Screen apps; a maintained Rust address-format library; real-world Web Share Target behaviour (web-features lists `app-share-targets` as not Baseline: Chrome/Edge 89, Chrome Android 76, no Firefox or Safari; Odoo's web client has a `share_target_service`); Unicode MessageFormat 2 status. Performance budgets in section 12 are proposals. No user research was done; SMB/accountant needs are inferred from product behaviour and domain knowledge.

## Implications for obop

**MUST**
1. **Views are versioned JSON ASTs** stored in the DB, owned by Rust schemas (schemars + ts-rs, generated and diff-gated), with named extension **slots** instead of xpath, and an effective-view resolver (base + module + company/role + user layers).
2. **One expression format** (JSON AST) for domains, modifiers, visibility and command availability, evaluated identically in Rust and the client (WASM or TS port, conformance-tested); offer a CEL-like text authoring syntax that compiles to it.
3. **View-type and field-renderer registries** keyed by `(viewType)` and `(fieldType, widget)`; every field type implements `read/edit/cell/filter/aggregate/format/parse/importParse`.
4. **Ship the v1 view catalogue in section 6**, including the ones Odoo gates behind Enterprise: report viewer with drill-through, reconciliation split view, approvals inbox.
5. **WCAG 2.2 AA is a release gate**: React Aria primitives, Oxlint `jsx-a11y`, axe in Playwright, manual screen-reader passes; every drag has a non-drag path; single-key shortcuts must be disable-able/remappable (SC 2.1.4); status messages via live regions.
6. **Grid built on TanStack Table 9 + Virtual** with the APG grid keyboard model, range select, TSV paste with validation preview, multi-edit, `+=`/`*=` edits, numpad-decimal handling; no AGPL/commercial grid in core (AG Grid range/clipboard are paid [V]).
7. **Design tokens in DTCG 2025.10** compiled to runtime CSS variables with theme and density attributes; no per-theme asset bundles.
8. **Saved views with four scopes** (personal, shared, system, locked), URL-addressable, resettable; search/filter state shareable by link (Odoo 20 and Stripe both do this [V]).
9. **Import with dry-run and batch rollback**; never ship irreversible imports.
10. **Logical CSS properties only, `dir` at `<html>`, Lingui/ICU/.po**, decimal-comma/dot tolerant inputs, IME-safe key handling; a CI check that bans physical margins/paddings.
11. **Unified Inbox** (approvals, mentions, activities, alerts) with keyboard triage and snooze; email and Web Push as channels.
12. **Document the Community-vs-Enterprise boundary** in contributor guidelines: implement from behaviour and this spec, never from Odoo Enterprise code.

**SHOULD**
13. Unified **command bar**: record search with `field:value`/`-not`/ranges/`is:` grammar plus commands whose availability is metadata (`global | selection | record | edit`, condition expression), hotkeys as data.
14. Side-panel record opening by default with a per-view `open_record_in` setting (Twenty, Notion [V]).
15. Three-tier offline model (read cache, queued mutations for allow-listed models, POS/warehouse outbox); no dependence on Background Sync.
16. `barcode-detector` ponyfill + HID keyboard-wedge support; native shell (Tauri) for printers/scales/terminals.
17. In-product **view and form editor** writing the override layer (Studio replacement), plus a "customized" audit badge.
18. Enforce the section 12 budgets in CI (100k-row perf test, field INP collection).
19. Density presets and a brand accent with automated contrast validation.
20. Publish a VPAT/EN 301 549 conformance statement for storefront, portal and back office.
21. Sample-data mode in the setup wizard; ghost rows in empty states.
22. Extension model: declarative patches, then prebuilt ESM widgets, then sandboxed remote-dom components for untrusted marketplace code.

**COULD**
23. Spreadsheet view on Univer core (Apache-2.0) or o-spreadsheet (LGPL-3), v2.
24. Gantt/timeline, timesheet matrix (v2); map and cohort (v3).
25. Vector/semantic search facet (Twenty has `VECTOR_SEARCH` operand [V]) and natural-language filter.
26. Local-first sync engine (PowerSync/Zero class) once the offline tiers prove insufficient.
27. Web Share Target "share to create bill/expense" on mobile (Chromium-only, not Baseline per web-features; Odoo already ships a `share_target_service`); Stripe-style pinned/recent "Shortcuts" sidebar.

## Verification log

Adversarial fact-check on 2026-10-01. Primary sources used: GitHub API and raw files (`odoo/odoo` branches 19.0, 20.0 and master, `odoo/documentation`, `odoo/owl`, `twentyhq/twenty`, `frappe/*`, `voidzero-dev/vite-plus`, `unicode-org/cldr`, `mdn/browser-compat-data`), the `web-features` 3.40.0 npm package data (installed and queried locally), npm and crates.io registries, odoo.com release notes (19 and 20), W3C/WAI pages, vendor docs (Linear, Notion, Airtable, Pipedrive, Stripe, Shopify, AG Grid), WebKit blog, web.dev, ETSI directory.

**Confirmed as stated: about 150 discrete claims** (approximate; each version, date, licence, feature or statistic counted separately). This includes all of the following flagged items: Vite+ 1.0.0 stable on 2026-09-28, MIT, and the six bundled versions; Odoo 20 release notes (offline create/edit/archive/unarchive/delete, shareable search links, multi-record drag-and-drop, swipe-down command palette, Material Symbols); Odoo 19 offline read-only with a 2 GB cap; Odoo LGPLv3 and the absence of `web_gantt`, `web_map`, `web_cohort`, `web_grid`, `web_studio`, `account_reports`, `approvals`, `documents`, `knowledge`, `bank_rec_widget` from 19.0 (also absent in 20.0); list default 80 rows and 10,000 count cap; imports "permanent and cannot be undone"; WCAG 2.2 levels for 2.4.11, 2.5.7, 2.5.8 (24x24 CSS px) and 2.1.4 options; BarcodeDetector support matrix and Odoo's ZXing fallback; Temporal, Background Sync, Periodic Background Sync, Badging, WebHID and WebUSB not Baseline in web-features 3.40.0 (which is the latest release); Vite 8 default build target; DTCG 2025.10 announcement; Twenty view, command and page-layout metadata; AG Grid Enterprise $999 per developer with clipboard, row grouping, pivoting, Excel export and range selection; Handsontable dual licence; TanStack Table 9.2.4 and Virtual MIT; CLDR 48 (2025-10-29), 48.2 (2026-03-17) and CLDR 49 beta; Odoo master 20.1 alpha; OWL 3.0.0-alpha.49 (2026-09-08). Also confirmed: Twenty v2.44.0 and its front-end dependencies, Frappe licences and v16.36.1, Odoo issues #13950 and #270715, Odoo 19/20 release-note feature claims, Linear/Notion/Airtable/Pipedrive/Stripe/Shopify behaviour claims, Core Web Vitals thresholds, WebKit Web Push and storage-quota claims, and all npm/crates.io version numbers in the file (schemars, ts-rs, cel, cel-js, @bufbuild/cel, json-logic-js, jsonlogic-rs, icu, temporal-polyfill, rust_decimal, bigdecimal, libphonenumber-js, phonenumber, axe-core, Oxlint's 36 `jsx_a11y` rule files, Tauri, Zero, PowerSync, vite-plugin-pwa, Workbox, Lingui 6.8).

### Corrections

| # | Was | Now | Source |
|---|---|---|---|
| 1 | WCAG 2.2 "is a W3C Recommendation (2024-12-12)" | Recommendation since 2023-10-05; 2024-12-12 is the date of the current edition at /TR/WCAG22/ | https://www.w3.org/WAI/standards-guidelines/wcag/new-in-22/ ; https://www.w3.org/TR/WCAG22/ |
| 2 | Container queries "Baseline widely available since 2023-02-14" | Baseline (newly available) 2023-02-14, widely available 2025-08-14 | web-features 3.40.0, feature `container-queries` |
| 3 | Logical properties "Baseline widely available since 2021-09-20" | Baseline 2021-09-20, widely available 2024-03-20 | web-features 3.40.0, feature `logical-properties` |
| 4 | `KeyboardEvent.isComposing` "Baseline newly available"; Safari IME ordering quirk tagged [G] | Part of `keyboard-events`, Baseline widely available since 2018-01-29; the Safari ordering bug is documented in MDN BCD (WebKit bug 165004) and BCD lists it fixed in Safari 27 | web-features 3.40.0 ; https://github.com/mdn/browser-compat-data/blob/main/api/KeyboardEvent.json |
| 5 | "Web Serial is absent from the Baseline dataset" | Present as feature `serial`, not Baseline (Chrome/Edge 89, Chrome Android 148, Firefox 151, no Safari) | web-features 3.40.0 |
| 6 | Twenty `availabilityType` = GLOBAL / BULK_SELECTION / SINGLE_RECORD_SELECTION / NO_SELECTION / SHOW_PAGE / PAGE_EDIT_MODE | `availabilityType` = GLOBAL / GLOBAL_OBJECT_CONTEXT / RECORD_SELECTION / FALLBACK; the older list is a separate enum `CommandMenuItemViewType` with `INDEX_PAGE_` prefixes | https://github.com/twentyhq/twenty/blob/main/packages/twenty-shared/src/types/CommandMenuItemAvailabilityType.ts and `CommandMenuItemViewType.ts` |
| 7 | Twenty MIT packages = `twenty-ui`, `twenty-sdk`, `twenty-shared` | Also `twenty-client-sdk`, `create-twenty-app`, `packages/twenty-apps`; AGPLv3 has an added "Twenty Application Exception"; `view.openRecordIn` is marked deprecated | https://github.com/twentyhq/twenty/blob/main/LICENSE ; `view.entity.ts` |
| 8 | Frappe "49 form-control files" | 48 files plus one `quill-mention` subdirectory | https://github.com/frappe/frappe/tree/develop/frappe/public/js/frappe/form/controls |
| 9 | Gantt "Dependencies, undo, buffers (Odoo 20)" | Undo is an Odoo 19 item, buffers an Odoo 20 item; the Odoo 20 notes do not mention dependencies | https://www.odoo.com/odoo-19-release-notes ; https://www.odoo.com/odoo-20-release-notes |
| 10 | Odoo 19 favorites "shareable with avatars" | Shareable with specific users; avatars appear in custom filters | https://www.odoo.com/odoo-19-release-notes |
| 11 | `barcode-detector` 3.2.2 "ponyfill on `zxing-wasm` 3.1.4" | Depends on `zxing-wasm` 3.1.3 (latest zxing-wasm is 3.1.4) | `pnpm view barcode-detector@3.2.2 dependencies` (registry metadata lookup) |
| 12 | `light-dark()` widely available "around late 2026" | About 2026-11-13 (newly available 2024-05-13 plus 30 months) | web-features 3.40.0 |
| 13 | Anchor positioning "core reached newly-available on 2026-01-13" | Split: most properties 2026-01-13, `position-anchor` 2026-09-14 (Safari 27, Chrome/Firefox 151); group still not Baseline because of two Safari-only `position-visibility` values | web-features 3.40.0, feature `anchor-positioning` |
| 14 | EAA application date 28 June 2025 tagged [G]; "Commission news item dated 2026-09-07 announces an updated EN 301 549" tagged [V] | Date confirmed via a secondary source only; the Commission news item could not be re-found and is now "(unverified)"; ETSI lists EN 301 549 V4.1.1 as a final publication dated 2026-09-02 | https://en.wikipedia.org/wiki/European_Accessibility_Act ; https://www.etsi.org/deliver/etsi_en/301500_301599/301549/ |
| 15 | Odoo POS offline: "invoices ... unavailable offline" | Requested invoices are generated after reconnect; local-network printers and scales keep working, payment terminals do not | https://www.odoo.com/documentation/19.0/applications/sales/point_of_sale/use.html |
| 16 | `master` is `(20, 1, 0, ALPHA)`; no mention of 20.0 | `(20, 1, 0, ALPHA, 1)`; a `20.0` branch is `(20, 0, 0, FINAL)` and is GitHub's default; Enterprise-module absence re-checked on 20.0 (642 addon dirs); 68 widget dirs is the 19.0 count (75 in 20.0) | https://github.com/odoo/odoo/blob/master/odoo/release.py |
| 17 | Odoo issue #13950 "not a priority currently" presented alone | Reply dated 2018-05-01; a 2019 reply from the same developer said substantial screen-reader work had been done; issue is closed | https://github.com/odoo/odoo/issues/13950 |
| 18 | Vite default target listed without iOS | Also `ios16.4`; "Baseline widely available as of 2026-01-01" for Vite 8 | https://vite.dev/config/build-options |
| 19 | `currencySign: "accounting"` tagged [G] | Verified, Baseline widely available since 2020-03-28 | web-features 3.40.0, feature `intl` |
| 20 | CLDR 49 "beta (2026-09-25)" | beta1 2026-09-22, beta2 2026-09-25 | https://github.com/unicode-org/cldr/releases |

Additions (facts found while checking): Vite+ bundled pins lag npm `latest` for oxlint (1.86.0), oxfmt (0.71.0), vitest (5.0.3) and rolldown (1.2.12); Handsontable licence note; Odoo PWA supports SSO; Odoo 19 shows records after import but has no rollback; Web Share Target is Chromium-only and Odoo ships a `share_target_service`; WCAG 4.1.1 Parsing is removed in 2.2.

### Unresolved

- Commission news item of 2026-09-07 on an updated EN 301 549 (unverified); content of EN 301 549 V4.1.1 not read.
- EAA 28 June 2025 and the microenterprise exemption are confirmed only through Wikipedia; EUR-Lex and the Commission EAA page did not return the text. Treat as high-confidence but not primary-verified.
- Odoo 20 exact release date (only "September 2026" from the release notes); Odoo 20 offline conflict semantics (not documented in what was read).
- Whether the full-screen home-menu app grid is Enterprise-only (inferred from absence of `web_enterprise`).
- Safari 27 shipping status for the IME fix is taken from MDN BCD and web-features data, not from an Apple source.
- Not re-checked and still vendor-doc-level or inferred: Attio and Shopify saved-view behaviour, QuickBooks export/API specifics, iOS Home Screen eviction behaviour, a maintained Rust address-format crate, Unicode MessageFormat 2 status, Odoo user sentiment beyond GitHub issues, and all items tagged [I] or [G] that were not touched above.

**Owner-constraint pass**
- Owner-constraint pass (2026-10-01): JS tooling is `vp` + pnpm only (no npm, npx, yarn, bun). This file's "npm" mentions are registry facts (versions, `latest` dist-tags, package metadata) and were left as they are. One change: the verification command in correction 11 is now `pnpm view barcode-detector@3.2.2 dependencies` (`pnpm view` exists since pnpm v11.0.0, aliases `info` and `show` [V https://pnpm.io/cli/view]) instead of `npm view`.
