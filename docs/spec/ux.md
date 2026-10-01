# UX, design system and metadata-driven UI

- **Status:** Draft 0.1
- **Date:** 2026-10-01
- **Scope:** How every obop screen is described (versioned JSON views), rendered (React registries), operated (keyboard, search, editing, undo), themed (design tokens), made accessible (WCAG 2.2 AA gate), localised, used on phones, tablets and offline, and populated on day one (setup, sample data, import).

**Conventions.** Decisions are cited by ID ([DECISIONS.md](../DECISIONS.md)); names shared with other documents (events, permission groups, data classes, entities, states) are spelled as in [CONVENTIONS.md](../CONVENTIONS.md); register items F1 to F4 are owner constraints, unrelated to the F0 to F3 UI tiers (D11). "Proposed (Dn)" means this document relies on a decision awaiting ratification; "Proposed" alone marks a choice the research and register leave open. "(target)" is a goal, "(estimate)" a model output; nothing is measured because nothing is built. Versions are as of 2026-10-01. "Odoo Community" and "Odoo Enterprise" name Odoo's editions; obop has an open edition and an `ee` tree (D24). Wave placements inherit the Proposed status of D25 and D26. D1 to D5, D12 to D14, D16, D17, D20, D25 and D26 are Proposed in the register, so every citation of one of them below is written "Proposed (Dn)". Nothing here is legal advice.

## 1. Purpose and scope

### 1.1 What this area is

UX is the contract between the runtime registry, the single source of truth for models, fields, state machines and access (D29), and the person at the screen: how screens are described, rendered, operated, themed, made accessible, localised and populated. It owns no business data. It ships as frontend packages plus registry data in each business database (views, overlays, saved views, preferences), and every other spec in `docs/spec/` describes its screens with the view types defined here.

Users are every principal that touches a screen, implementers who customise without code, extension authors (F0 to F3) and whoever signs the accessibility statement. Agents (D18) use the typed command surface; their proposals reach humans in the Inbox (4.4).

### 1.2 Boundaries with neighbouring areas

| Neighbour | Owns | UX owns |
|---|---|---|
| [platform.md](platform.md) | Registry, chatter, documents, approvals engine, import engine and per-model rules, search engine | Inbox, import wizard, search bar, report viewer surfaces |
| [finance.md](finance.md) | Report definitions, reconciliation rules, lock dates | Report viewer and reconciliation split view contracts |
| [sales-crm.md](sales-crm.md), [supply-chain.md](supply-chain.md), [services.md](services.md), [people.md](people.md) | Models and workflows | The view types their views are built from |
| [retail-commerce-marketing.md](retail-commerce-marketing.md) | POS, storefront, checkout internals | Tokens, WCAG gate, scan input, offline contract |
| [security-privacy.md](security-privacy.md) | Permission algebra, audit, step-up | UI consequences: field pruning, 404, cache exclusions |
| [integrations.md](integrations.md), [ai.md](ai.md) | API, foreign-source importer back ends, marketplace; agents, approval policy | Import screens, extension UI tiers; palette and Inbox that AI reuses |
| [LOCALIZATION.md](../LOCALIZATION.md) | Country packs, legal content | Formats, RTL and CJK rendering, pack-aware wizard |
| [ENGINEERING.md](../ENGINEERING.md), [ARCHITECTURE.md](../ARCHITECTURE.md) | CI and release; stack and topology | The gates defined here; stack referenced, not re-argued |

### 1.3 Fixed frontend stack

Fixed by the owner's Vite+ and pnpm constraint, D15 and D35; not re-argued here. Engineering detail is in [ARCHITECTURE.md](../ARCHITECTURE.md); see also [CLAUDE.md](../../CLAUDE.md).

| Concern | Choice (as of 2026-10-01) |
|---|---|
| Framework | React 19 (19.3.0, 2026-09-09) and TypeScript 7 (stable release 2026-07-08), TSX only; React Compiler via `@rolldown/plugin-babel`; no `baseUrl` (D15) [R00 s3.3; R07 s3.2] |
| Toolchain | Vite+ through the `vp` CLI only; `vite-plus` 1.0.0 (2026-09-28, MIT) bundling Vite 8.3.1, Rolldown 1.2.11, Vitest 5.0.1, Oxlint 1.85.0, Oxfmt 0.70.0, tsdown 0.23.0; eligible under the pin policy from 2026-10-05 (D23). pnpm only; CI runs `vp install --frozen-lockfile` |
| Data, tables, primitives | TanStack Query, Router, Table 9.2.4 and Virtual (Start excluded while a release candidate); React Aria Components 1.21.1; Tailwind 4.3.3 logical utilities; ECharts 6.1.0; Tiptap 3.31 core; Lingui 6.8 (D15, D34) [R00 s3.3] |
| Tests | Vitest Browser Mode, Playwright 1.63, `@axe-core/playwright` 4.13.0; visual baselines from one pinned container [R27 A2] |
| Contracts | Rust owns them: OpenAPI 3.1 (utoipa 6.0.0), meta-model types (ts-rs 12.0.1 or specta), JSON Schema (schemars 1.2.2); generated files are committed and diff-gated [R19 s5.1; R07 s3.7] |

The CI licence allow-list excludes AG Grid Enterprise, Handsontable, FullCalendar Premium, Univer Pro, BlockNote XL and HyperFormula (commercial or copyleft-only) [R00 s3.3; D15; D39].

### 1.4 Components, tier, wave, open or paid

Proposed (D25) does not list frontend packages; T0 follows its definition ("present in every business") and is Proposed.

| Component | Tier | Wave | Open or paid |
|---|---|---|---|
| UI runtime, view system, registries, URL state, record cache | T0 | W1 | Open |
| Core view types: form, list, kanban, pivot, graph, calendar, per Proposed (D25); activity, hierarchy, settings (Proposed) | T0 | W1 | Open |
| Search, saved views, command palette | T0 | W1 | Open |
| Report viewer, reconciliation split view (types T0, content from `account`, T1) | T0, T1 | W1 (D35) | Open |
| Notification centre and Inbox (`mail`); approvals lane | T0 | W1; approvals W2 (D18) | Open |
| Design system, tokens, business branding, theme packs | T0 | W1 | Open; partner white-label paid |
| Setup wizard, sample data, import and export UX | T0 (`base`) | W1; CSV 0.3, Odoo and QuickBooks 0.6, Xero and ERPNext W2, Proposed (D26 ruling 31) | Open |
| Read cache, offline shell, degraded-feature list | T0 | Starts W1 (Proposed); Proposed (D13) requires it by the 1.0 LTS, 2028-04-18 (W2) | Open |
| Timesheet grid, gallery (Proposed) | T0, T1 | W2 | Open |
| View and layout editor (Proposed); F1 loader (first-party W1, business-own code W2, as in ARCHITECTURE.md 9.1); F2 host | T0 | W2 (F2 per Proposed (D26)); form builder W3 | Open |
| Gantt, map, cohort; F3 iframes; POS screen, queued writes | T0, T2 | W3, Proposed (D25, D26) | Open |
| Dashboard container and default dashboards per app (a minimal W2 container is Open question 10) | T2 | W3, Proposed (D25) | Open |
| Spreadsheet view | none | not scheduled (COULD) | Open |
| Partner white-label (reseller console, trademark licence) | `ee` | W3 | Paid, closed list, Proposed (D2) |

## 2. Personas and roles

Group names follow the registry in [CONVENTIONS.md](../CONVENTIONS.md) section 4; the kernel seed list is in [security-privacy.md](security-privacy.md) 2. A permission group is a named bundle of permission rows (D24).

| Persona | Needs from the UI | Default groups (Proposed) | Main surfaces |
|---|---|---|---|
| Owner or manager | Approve, see cash and pipeline, trust numbers | `base.user`, manager groups | Inbox, reports, saved views; dashboard from W3 |
| Accountant, internal | Keyboard entry, batch reconciliation, period close | `account.bookkeeper`, `account.user`, `account.manager` | Editable grid, split view, reports, import |
| External accountant (free seats, up to 3, Proposed (D2)) | Read, adjust, export; MFA by default | `account.adviser` | Reports, grid, export |
| Sales representative | Pipeline, quotes, phone use | `sales.user` | Kanban, form, calendar |
| Warehouse operator | Scan, confirm, tablet, bad Wi-Fi | `stock.operator` ([supply-chain.md](supply-chain.md)) | Scan input, touch density, offline banner |
| Cashier (W3) | Fast touch checkout, offline capture | `pos.cashier` | POS screen |
| Administrator or implementer | Setup, import, customise, brand | `base.admin`, `customize.designer` ([platform.md](platform.md)) | Wizard, settings, view editor |
| Light user, Proposed (D2) | A few tasks: timesheet, expense, approve | task-scoped groups | Inbox and permitted apps only |
| Portal user | Own documents, pay, sign | `base.portal` | Portal pages (WCAG 2.2 AA, D38) |
| Assistive-technology user | Everything above without pointer or sight | same as role | Every surface |

UI permission rows (the capability rows of [CONVENTIONS.md](../CONVENTIONS.md) section 4; Proposed): `customize_views` (publish overlays; bundled with schema rights in `customize.designer`, as [platform.md](platform.md) 4.14 states), `share_views` (shared and system scopes), `import_data` (plus create or write on each target model), `manage_branding`. Export is the separate named-action permission AZ10 of [security-privacy.md](security-privacy.md), not a UI row defined here. All are audited.

## 3. Capability map

Parity compares with Odoo Community, Odoo Enterprise, "beyond" (obop does more) or "deliberately skipped". Wave shows its basis: a decision ID or "Proposed".

| ID | Capability | Description | Wave | Odoo parity |
|---|---|---|---|---|
| C01 | Virtualised list and grid | Keyset-paged, grouped, keyboard grid | W1 (D35) | Beyond: Odoo pages 80 rows, count capped at 10,000 [R19 s2.3] |
| C02 | Inline, multi and bulk edit; Excel paste | Cells, "apply to N", TSV paste | W1 | Matches Community for edit; paste beyond |
| C03 | Form and kanban | Slots, smart buttons; "Move to" twin | W1 | Matches Community; non-drag path beyond |
| C04 | Pivot, graph, calendar, activity, hierarchy, settings | Drill-through, treegrid | W1 (last three Proposed) | Matches Community |
| C05 | Report viewer | Comparisons, every number drillable | W1 (D35) | Matches Enterprise |
| C06 | Reconciliation split view | Suggestions, batch validate | W1 (D35) | Matches Enterprise |
| C07 | Inbox and notification channels | Triage; in-app, email, Web Push | W1; push W2; digests and quiet hours W2 and SMS W3 (Proposed; not placed by D25 or D26, placement proposed in [platform.md](platform.md) 4.6) | Matches Community; triage beyond |
| C08 | Approvals lane, agent proposals | Digest-bound diff | W2 (D18); W1 offers only a plain confirm of the dry-run diff in the Inbox (Proposed, [ai.md](ai.md) 1.4) | Matches Enterprise (Approvals); agents beyond |
| C09 | Dashboard, timesheet grid, gallery | Widgets; weekly matrix; cards | Grid and gallery W2 (Proposed); dashboard W3, Proposed (D25) | Dashboard matches Community (different mechanism); grid matches Enterprise |
| C10 | Gantt, map, cohort | Lag, tiles, retention | W3, Proposed (D25) | Matches Enterprise |
| C11 | Versioned JSON views, named slots | Stable node ids, structured patches | W1 (D35) | Beyond; XPath inheritance deliberately skipped |
| C12 | Expression nodes | CEL plus compiled form | W1, Proposed (D12) | Deliberately skipped: Python strings in XML |
| C13 | Overlay, reset, report, editor | Layers over immutable defaults | W1 store; W2 editor and report | Editor matches Enterprise (Studio); report beyond |
| C14 | Saved views, four scopes | URL-addressable | W1 (D35) | Beyond |
| C15 | Palette and search grammar | Records, navigation, commands | W1 | Matches Community; grammar beyond |
| C16 | Keyboard model | Remappable, IME-safe | W1 | Beyond; Alt+letter defaults deliberately skipped |
| C17 | Undo | Session, toast, reversal, rollback | W1 | Beyond: Odoo imports are irreversible [R19 s2.5] |
| C18 | Tokens, density, dark mode, branding | DTCG 2025.10 as CSS variables | W1 (D35) | Beyond: Odoo dark theme is a compiled bundle [R19 s2.4] |
| C19 | WCAG 2.2 AA gate and statement | React Aria, axe, screen-reader passes | W1 (D35) | Beyond: no Odoo conformance statement found |
| C20 | i18n, RTL, CJK | PO plus ICU MessageFormat 1 | W1 (D34) | Matches Community; CI fixtures beyond |
| C21 | Responsive, touch, barcode input | Container queries; wedge then camera | W1 layouts; scan input W2 (Proposed); screens W3; depth W4, Proposed (D25) | Matches Odoo Community for scan input; Odoo scan screens (the Barcode app) are Enterprise (verified [R10 s1; odoo.com/page/editions]) |
| C22 | Offline read cache; queued writes | Degraded-feature list; edge store | Starts W1 (Proposed), by 1.0, Proposed (D13); queued W3+ | Cache matches Odoo 19; queued matches Odoo 20 |
| C23 | Setup wizard, sample data, checklist | Pack-aware, resumable | W1 | Matches Community onboarding; beyond |
| C24 | Import with dry run and rollback; importers | CSV, Excel, Odoo, QuickBooks | W1 (D35), Proposed (D26) | Beyond |
| C25 | Extension UI tiers F0 to F3 | Slots, ESM, remote-dom, iframe | F0 W1; F1 first-party W1, business-own W2; F2 W2; F3 W3 | Beyond: Odoo patches at runtime |

## 4. Per-app specification

These components are cross-cutting, so entities, events, permissions and open or paid status shared by 4.3 to 4.14 are consolidated in 4.2; later subsections state only what is specific to them.

### 4.1 UX principles

1. **URL-addressable.** Every list, filter, group and record state round-trips through the URL; a user without access gets 404 (D10) [R19 s14; R18 s10].
2. **Keyboard first, mouse friendly.** Every path in section 5 completes without a pointer; shortcuts are discoverable and remappable.
3. **Context survives.** Records open in a side panel by default; filters and scroll survive back-navigation.
4. **Saved means scoped** (four scopes, resettable) and **customise without forking** (overlays over immutable defaults).
5. **Every drag has a non-drag twin** (WCAG 2.5.7).
6. **Undo for drafts, reversal for posted** (D30).
7. **Numbers are sacred.** No floats; tabular, locale-correct, drillable; mixed currencies never summed silently.
8. **Fast is a feature.** Optimistic UI, virtualisation, budgets gated in CI; **one search box** for records, navigation and commands.
9. **Progressive density.** Compact, comfortable, touch from one definition.
10. **Errors are local and announced; offline is honest; accessibility is structural** (primitives plus lint and axe gates).
11. **No paid tier for core UX** (Proposed (D2); D35).

### 4.2 Runtime, shell and area-wide contracts

**Architecture.** Registries for view types, field renderers, actions, commands and services. A framework-neutral metadata layer (`@obop/meta`, `@obop/rpc`, `@obop/domain`, `@obop/i18n-core` import no React) keeps a renderer swap cheap; React lives in `@obop/ui-kit`, `ui-views` and `ui-widgets`; apps are `backoffice`, `storefront` and `pos-shell` [R07 s3.11, s3.13]. Modules compose at build time through a Vite+ plugin reading each module's `frontend/` folder; nothing patches another module at runtime [R02 s8]. `vp build` embeds the result in the single binary; customer servers never run Node (D37).

**Shell.** Searchable app switcher with recents, menus, control panel (breadcrumbs, search, view switcher, pager), status strip (connection, active company, user). The control panel renders from the cached view definition before data arrives [R19 s2.1]. Menus show only permitted entries, so a light user sees a short list.

**State.** TanStack Query keeps a normalised record cache keyed by (model, id, field set); the URL holds action, view type, domain or saved-view id, group-by, sort, record id and cursor [R07 s3.4]. One WebSocket per tab resumes from `(xid8, event_id)` (D7); change events invalidate cache keys, and an open record with unsaved edits shows a stale banner instead of reloading. From W2 the active company set travels explicitly with each request, and a record outside it offers "switch to company X" when permitted instead of a bare access error [R02 s9]; W1 has one company and no selector (D28).

| Entity | Purpose | Key fields | States |
|---|---|---|---|
| `view_definition` | Base and package views | UUIDv7 key (D9) plus external id (`module.name`, D22), model, type, `schema`, arch JSON, origin | immutable per module version |
| `view_override` | Patches by node id or slot | target, ops, scope (business, company, permission group) | active, reset |
| `saved_view` | Query plus presentation | action, type, scope (personal, shared, system, locked), owner, domain AST, group-by, order, columns, options | active, reset |
| `action`, `menu` | Navigation | kind (view, url, command, report, custom screen; the counterparts of Odoo's window, URL, server, report and client actions [R02 s4]), model, view modes, default domain, groups, optional parent action for embedded tabs (W2, Proposed) | none |
| `command_definition` | Palette commands and hotkeys as data (the name [platform.md](platform.md) 4.15 uses) | label, availability, CEL condition, hotkeys | active |
| `user_preference` | Per principal | UI language, format locale, time zone, density, theme, shortcuts; references the principal's `notification_preference` rows ([platform.md](platform.md) 4.6) | none |

This document owns `view_definition`, `view_override`, `saved_view`, `command_definition` and `user_preference`; [platform.md](platform.md) 4.14 and 4.15 point here for view layers, `saved_view` and `command_definition`. Inbox state is not a separate entity: it is the `inbox_state` field of the `notification` row of [platform.md](platform.md) 4.6 (unread, read, snoozed, done), with `snoozed_until` and a `lane` (mention, activity, alert) as in [CONVENTIONS.md](../CONVENTIONS.md) section 6, while approval tasks (`approval_task`, platform.md 4.12) and agent proposals (`command_plan`) appear in the Inbox through their own records. The import engine entities belong to [platform.md](platform.md) 4.16 and the foreign-source importers to [integrations.md](integrations.md) 4.8. Brand settings (4.9), the `external_ref` ledger and setup progress (4.14) are described where used.

**Events (names as in [CONVENTIONS.md](../CONVENTIONS.md) 3.4 and 3.6, which stay Proposed until the catalogue generated from the `*-api` crates, ARCHITECTURE.md 4.3, replaces that table).** Consumed by the client as invalidations over the realtime channel (D7; invalidations rather than record bodies is Proposed in ARCHITECTURE.md 12): the realtime frames `RecordChanged` and `JobProgressed` (messages, not outbox events), and the events `RegistryVersionChanged` (defined in ARCHITECTURE.md 5.1; platform.md 4.14 only triggers it), `PermissionChanged` (emitted by [security-privacy.md](security-privacy.md) 4.2; invalidate and re-resolve views), `NotificationCreated` (platform.md 4.6), `ApprovalRequested`, `ApprovalTaskDecided`, `ApprovalDecided` (platform.md 4.12), `ImportRunCommitted`, `ImportRunRolledBack` (platform.md 4.16), `ExportReady` (an export job finished and its archive can be downloaded; emitted from `export_job`, [integrations.md](integrations.md) 4.9) and `PackActivated` (reload formats and labels; emitted by base apps management on pack install and at each effective date of a pack version, [platform.md](platform.md) 4.18, Proposed as resolved in CONVENTIONS 3.7). Emitted by UX commands as durable events (kind D, [CONVENTIONS.md](../CONVENTIONS.md) 3.2; no in-transaction handler needs atomicity, and none has a named consumer, 3.5): `SavedViewSaved`, `ViewOverridePublished`, `ViewOverrideReset`, `PreferenceChanged`, `SetupStepCompleted`.

**Permissions and sensitive data.**
- Unreadable fields never reach the client: resolved views omit their nodes, and the query compiler rejects filter, sort or group on them (D10).
- Saved views and links never widen access; recipients see results under their own permissions, and an inaccessible record answers 404, not 403 [R18 s10].
- URL state can leak through referrers and logs: `Referrer-Policy: same-origin` and log redaction of view-state parameters (Proposed).
- Overlay, sharing, import and export actions are audited (D33). Export controls appear only for principals holding the named export permission (AZ10) and ask for step-up when sensitive classes or the whole business are exported ([security-privacy.md](security-privacy.md)).

**Extension UI tiers.**

| Tier | Who | Isolation | Accessibility duty |
|---|---|---|---|
| F0 patches, slots, built-in widgets | Anyone | n/a | Host |
| F1 prebuilt ESM on `@obop/ui-sdk` | First-party and business-own code only (D11) | None | Author, CI-checked |
| F2 remote-dom | Marketplace, partners | Opaque-origin sandbox plus Worker; host element allow-list | Host renders real DOM; author supplies labels |
| F3 sandboxed iframe | Marketplace, remote apps | Separate origin, manifest CSP | Author: WCAG 2.2 AA statement, axe in CI, iframe `title`; host owns focus entry and exit |

Tokens reach F1 as CSS variables, F2 as token-named style props and F3 through the init handshake. The SDK ships its own overlay primitives because portals fail in remote renderers. `@obop/ui-sdk` is semver; the host supports the current and previous major of F2 and F3 for at least 24 months, F1 for 12, Proposed (D16) [R24 s4].

**Open or paid.** All of 4.1 to 4.14 is open. Only partner white-label is paid, Proposed (D2); business branding is open.

### 4.3 View definitions: AST, slots, expressions, layers

**MVP and later.** W1: AST, registries, resolver, overlay store, patches, reset. W2: form, list and kanban layout editor and the customisation report (platform.md). W3: form builder with conditional visibility (Proposed) [R19 s5.5].

**Rules.** Views are data in the business database, validated in Rust, with generated, diff-gated TypeScript types [R19 s5.1]. The server resolves the effective view per request; the client never sees XML or XPath. Registries are keyed by view type and by (field type, widget); extensions register entries and never patch core.

```mermaid
flowchart LR
  REG["Registry: models, fields, state machines"] --> RES["Effective-view resolver"]
  MOD["Layer 1: module and package views"] --> RES
  BIZ["Layer 2: business overlay"] --> RES
  USR["Layer 3: user layer and saved views"] --> RES
  PERM["Permission algebra D10: field pruning"] --> RES
  RES --> OUT["Resolved view per view, groups, company, language"]
  OUT --> CLI["Client registries"]
  CLI --> UI["React renderers"]
```

Illustrative AST, adapted from [R19 s5.2] to obop vocabulary (model and command names follow [sales-crm.md](sales-crm.md); other field and action names are placeholders):

```jsonc
{
  "id": "sales.order_form", "schema": 1, "model": "sale_order", "type": "form",
  "nodes": {
    "actions": { "slot": "header.actions", "children": [
      { "id": "btn_confirm", "kind": "action", "command": "sale_order.confirm", "primary": true,
        "visible": { "cel": "state == 'draft'" } } ] },
    "stats": { "slot": "smart_buttons", "children": [
      { "id": "sb_invoices", "kind": "smart_button", "value": "invoice_count",
        "opens": { "action": "invoicing.invoice_list", "domain": [["order_id","=","$id"]] } } ] },
    "main": { "slot": "sheet.main", "children": [
      { "id": "f_party", "kind": "field", "field": "party_id", "widget": "relation" },
      { "id": "f_terms", "kind": "field", "field": "payment_term_id",
        "readonly": { "cel": "state != 'draft'" } } ] },
    "lines": { "slot": "sheet.lines", "children": [
      { "id": "f_lines", "kind": "field", "field": "line_ids",
        "view": { "type": "list", "editable": "bottom", "columns": ["product_id","qty","price_unit","tax_ids"] } } ] },
    "chat": { "slot": "side.chatter", "features": ["messages","notes","activities","followers"] }
  }
}
```

**Node rules.**
- Every node has a stable `id`; ids and slot names are a public contract under the 24-month deprecation floor, Proposed (D16).
- `visible`, `readonly`, `required` and `decoration` are expression nodes. Domains are a JSON AST in Odoo's syntax compiled to SQL (D22); everything else stores CEL source plus a type-checked compiled form, run by one Rust evaluator on the server and in Wasm on the client under conformance tests (Proposed (D12); 4 KB and depth 32 limits, D11). R19 proposed JSON AST for all; D12 supersedes it if ratified. Expression nodes are a tagged union `{cel}` or `{ast}` (Proposed), so either D12 outcome changes evaluators, not the view schema.
- `opens` and `command` are typed references, never script strings; to-many sub-views are inline definitions; `html` nodes are sanitised server-side and packs cannot inject script.

**Slots** (declared per view type; patches may target only declared slots and nodes flagged `extensible`).

| View | Slots |
|---|---|
| Form | `header.actions`, `header.status`, `banner`, `smart_buttons`, `sheet.title`, `sheet.main`, `sheet.tabs.<id>`, `sheet.lines`, `side.chatter`, `footer.totals` |
| List | `toolbar.actions`, `toolbar.bulk`, `columns`, `row.decorations`, `row.actions`, `footer.aggregates`, `empty_state` |
| Kanban | `column.header`, `card.title`, `card.body`, `card.footer`, `card.actions`, `quick_create` |
| Report, Calendar, Inbox | `filters`, `lines`, `columns`, `footer.notes`; `event.title`, `event.popover`; `item.summary`, `item.detail`, `item.actions` |

**Patches** are structured (`insert_before`, `insert_after`, `insert_into`, `replace`, `set_props`, `remove`, `move`), addressed by node id or slot, and validated at package install and overlay publish. A patch whose target vanished after an upgrade is skipped, flagged in the customisation report and never breaks the screen (Proposed).

**Layers (D32).** (1) Module and package views, immutable per version. (2) Business overlay (`view_override`) by `customize.designer`, scoped to the business, a company or a permission group. (3) User layer: columns, order, density, saved views. Resolution applies them in order and caches per (view, permission groups, company scope, language); payloads are ETag-cached.

**Customisation.** Custom fields (typed or properties tier, D29) appear in a `custom` slot, in search, import and the API. Customised views show a badge and a one-click reset [R19 s5.5]; the customisation report lists each overlay with its upgrade risk [R04 Implications 13]; a business's customisation exports as a package (D32). Stored overlays migrate forward with each `schema` integer, and a schema version is removed only at an `N.0` at least 24 months after deprecation, Proposed (D16).

### 4.4 View catalogue and behaviour contract

Every view type registers: arch JSON Schema, a typed data spec [R02 s4], renderer, controller (selection, bulk, keyboard, undo), capabilities (search, bulk edit, export, drill-through, print), the ARIA Authoring Practices pattern it follows, and whether it caches offline. A type without an accessible pattern cannot register.

| View | Wave | Behaviour contract |
|---|---|---|
| List and editable grid | W1 | `role="grid"`, one tab stop, roving focus, `aria-rowcount` and `aria-rowindex`. Keyset pages of 200 to 500 rows (target); sticky group headers with lazy children and per-group aggregates; footer aggregates; column chooser, resize, pin, reorder saved in the view; row `decoration` never by colour alone. Count beyond the cap reads "10,000+" (configurable, Proposed). Editing in 4.7. Read-only reports use a plain table, hierarchies a treegrid [R19 s7] |
| Form | W1 | Slots per 4.3; primary action plus overflow; statusbar; smart buttons open a filtered action. Locked documents are read-only with a banner naming the reason (posted, period locked, approval pending). Dirty guard; single save with `row_version` (4.7) |
| Kanban | W1 | Columns by any selection or relation; progress and sums computed server-side; quick create; fold; multi-select; stage-age flag from a per-stage threshold [R11 s5.1]. Drag rewrites the grouped field through a command. Twin: card menu "Move to" or select-then-arrow; focus stays on the card and a live region announces column and position. `@dnd-kit/react` 0.5.0 behind a facade, pinned exactly [R07 s3.3] |
| Pivot | W1 | One semantic query (measures, dimensions, grain, target currency, rate type) compiled under the caller's access [R23 B.3]; expand, collapse, flip; cell click drills through to the source list; CSV and XLSX export; size cap with warning |
| Graph | W1 | Bar, line, pie, stacked on ECharts; data-table toggle and generated text summary; never colour alone; click drills through |
| Calendar | W1 | Day, week, month; create by click, drag or "New event at"; reschedule by drag or dialog; recurrence edits this, following or all; zone shown. FullCalendar 7 core behind a facade is the R07 candidate; its WCAG 2.2 fitness is unverified and it must pass 4.10 or be replaced |
| Activity matrix | W1 | Records by activity type with overdue, today, planned states; owned by `mail` |
| Hierarchy | W1 | Treegrid: Right expands, Left collapses. Chart of accounts, bill of materials, org chart; drag re-parenting has a "Move under" twin; cycles rejected server-side |
| Report viewer | W1 | Declarative lines and columns, collapsible, "as of" and comparison selectors, posted-only toggle, lock-date banner, generated-at stamp. Every number carries a drill descriptor (model, domain, grouping; tax lines add the rule-pack version, D38) opening a URL-addressable source list. Accounting format, tabular figures; export via Typst, Proposed (D20), and XLSX |
| Reconciliation split view | W1 | Statement lines left, candidate items and detail right. Each suggestion states its reason ("exact amount plus reference", "same party, partial"). Accept, search counterpart, split, write off via a model, batch validate. Partial sums never exceed the residual in either currency; exchange differences flagged system-generated [R09 s9]. Frame shared with the document review split view in [ai.md](ai.md) |
| Inbox | W1; approvals W2 | List plus detail; lanes: mentions, activities, alerts, approvals, agent proposals (in W1 an agent plan shows as a single confirm action on its dry-run diff, without policy rules or approve-by-role, Proposed, [ai.md](ai.md) 1.4). Snooze, done, inline approve or reject with reason. An approval shows the dry-run diff bound to the plan digest; if records changed since, the button reads "Plan outdated, re-run dry run". Separation of duties is enforced server-side and disabled buttons say why |
| Dashboard | W3, Proposed (D25) | Grid of widgets: saved views, KPI tiles from registry measures, text; each a headed landmark with drill-through; private or shared. Proposed (D25) lists "dashboards" as a T2 W3 app and [platform.md](platform.md) 4.17 schedules `dashboard` in W3; a minimal container in W2 is Open question 10. Odoo Community ships ten-plus prebuilt dashboards per app [R15 s10]; the counterpart is default dashboards per app (for example pipeline, cash position, stock, open tickets), Proposed as X1 pack content (ARCHITECTURE.md 9.1) that arrives with the container. Until then each app ships system-scope saved views and pivot or graph presets (4.6), Proposed |
| Settings | W1 | Searchable, per app; each setting states effect and prerequisites, including "requires module X" |
| Timesheet grid | W2 (Proposed) | Rows task, columns days; keyboard matrix; durations `1:30`, `1.5`, `90m`; totals against expected hours from the resource calendar; submit and approve |
| Gallery | W2 (Proposed) | Card grid for products and documents; alt text required where public |
| Gantt, resource timeline | W3 | Task DAG with dependency types including precedes or follows with lag in working days, manual or automatic scheduling, server-side critical path [R14 Implications 9]. A treegrid of tasks is the accessible primary representation and the canvas a projection; moves and dependencies have a dialog path ("Move by N days"); undo. Library spike: SVAR React Gantt 2.7.3, DHTMLX Community or custom [R07 s3.3] |
| Map, cohort | W3 | Map: clustered records with a result list beside it as the accessible alternative; MapLibre GL JS with the tile source a setting, since public OSM tiles are unsuitable for production ([services.md](services.md); [R14 s6.3]). Cohort: retention matrix by measure and interval, cells drill through |
| Spreadsheet | Not scheduled | Univer core (Apache-2.0) behind a host interface; Univer Pro excluded. o-spreadsheet (LGPL-3) is Odoo-authored code and is OWL-coupled, so adopting it as a dependency needs an F3 clean-room ruling first [R15 s10] |
| POS and scan screens | W3; scan depth W4 | Purpose-built, not metadata-rendered; touch density, offline banner, large numeric pad; Tauri 2 shell with PWA baseline (D37) |
| Portal, website | separate | Rust-rendered blocks plus Vite islands, Proposed (D14); tokens and the WCAG gate apply |

### 4.5 Field renderers and widgets

**Contract.** A renderer registers under (field type, widget) and implements `read`, `edit`, `cell` (compact grid editor), `filter` (operators by type), `aggregate`, `format` and `parse` (locale-aware), `validate`, `exportValue`, `importParse` and `a11yName`; a type without `cell` or `filter` cannot appear in lists or search [R19 s5.4]. Field security applies before a renderer is reached (D10).

**Catalogue.** Text (char, text, email, phone, link, rich text via Tiptap, server-sanitised); numbers (integer, decimal, percentage, duration, quantity with unit, monetary as amount plus currency in one field); time (plain date for accounting, date-time as instant plus IANA zone, range); choice (boolean, selection, state badge, rating, colour); relations (to-one with avatar and inline create, to-many tags, line lists, polymorphic reference, user-defined properties); composite (address from per-country layout data, full name); files (file, image, signature, PDF, barcode); JSON, geolocation, progress, gauge. Odoo ships 68 widget directories (75 in 20.0) [R19 s2.3].

**Money and quantity (D9 bans floats).**
1. Money crosses the API as integer minor units plus a currency code, and unit prices, quantities and rates as decimal strings ([integrations.md](integrations.md) 4.1, Proposed shape). The client never does floating-point arithmetic on money or quantities; arithmetic uses the shared Rust decimal module in Wasm [R07 s3.8], pending a spike on bundle size and start-up cost.
2. Ledger views use the currency's exponent as data and `Intl.NumberFormat` with `currencySign: "accounting"` [R19 s9].
3. Footers and pivots sum within one currency; mixed currencies show per-currency subtotals or require a target currency and rate type [R23 B.3].
4. Input accepts `.` and `,`; the numpad decimal key inserts the locale separator; ambiguous group-versus-decimal input shows the parsed value before commit; full-width digits are NFKC-normalised.
5. Form totals are previewed locally with the server's rounding profile (`per_line` or `per_group`, D9); the server is authoritative, and any minor-unit difference shows a diff banner (Proposed).

**Explain popover.** Values with a trace (price rule, lead score, tax breakdown, an AI answer's structured query) share one widget [R11 s4.3; R16 s6].

### 4.6 Search, saved views, command palette, shortcuts

**MVP and later.** W1 search is Postgres full-text, `pg_trgm`, `unaccent` and ICU collations; CJK through PGroonga or `pg_bigm` where installed (W3), else `ILIKE` (D34; [platform.md](platform.md)). Later: `tantivy` when measured need arises [R15 s15].

**Search model.** Facet chips; filter groups (OR within, AND between); a domain tree editor emitting the JSON AST; multi-level group-by with date granularities; relative ranges with resolved-date tooltips, as in Odoo 20 [R19 s2.3]. An optional category side panel for list and kanban (Odoo's search panel; a relation or selection field with counts computed server-side under the caller's access, so a count never reveals an inaccessible record) is Proposed for W2.

**Omni-bar grammar (Proposed), after Stripe's dashboard search [R19 s3]:** `customer:"Acme GmbH"` (field), `-status:paid` (negation), `total:>1000` and `date:2027-01..2027-03` (comparison, range), `is:overdue` (named filter); prefixes `/` navigation, `@` people, `#` channels (W2), `?` knowledge (W3) are familiar to Odoo users [R19 s2.1]. The grammar compiles to the domain AST through each field's `filter` operators; errors show inline. Results group by model, menus and commands and come from the same permission-compiled path as lists (D10), never from a side index without access checks. Commands are metadata: availability (global, selection, record, edit), CEL condition, hotkeys as data [R19 s3]. Ctrl or Cmd + K opens the palette; Ctrl or Cmd + Enter opens a result in a new tab.

**Saved view scopes (Proposed rules).**

| Scope | Created by | Seen by | Edited by |
|---|---|---|---|
| Personal | Anyone | Owner | Owner |
| Shared | `share_views` | Chosen users, groups or the business | Owner, `share_views` |
| System | Module or `customize.designer` | All with access to the action | Administrators |
| Locked | Owner | As shared | Owner or `base.admin`; others "save a copy" |

A saved view stores filters, grouping, sort, columns, view type and options; the user default beats the system default; links preserve live state, as in Odoo 20 [R19 s2.3].

**Shortcuts.** A registry (data, not code) declares scope, command and remappability. The modifier is Ctrl, or Cmd on macOS. Odoo's Alt+letter defaults are not adopted because they collide with browser, OS and screen-reader keys [R19 s2.5]. Single-key shortcuts exist only in triage contexts (Inbox, reconciliation list, a focused kanban) with a global off switch, and the one global two-key sequence (G then I) follows the same switch and is remappable (WCAG 2.1.4 requires turn-off, remapping or active-on-focus; defaults Proposed). Defaults: `?` sheet; Ctrl or Cmd + Enter commits the form, wizard or batch; Esc cancels; Ctrl or Cmd + Z undoes; Ctrl or Cmd + D fills down inside a focused grid only, because browsers use it to bookmark a page (the Excel and Sheets convention, which R19 s7 tags as a guess); G then I opens the Inbox; J, K, E, H in the Inbox; Enter accepts the top suggestion in reconciliation; holding the modifier overlays hints. Every handler ignores events while `event.isComposing` is true (4.11).

### 4.7 Editing: inline, bulk, undo, concurrency

All of this ships in W1 except offline edits (4.13).

**Grid [R19 s7].** Enter or F2 edits; Esc cancels; typing starts an edit; Enter commits and moves down (setting: right); Tab commits and moves right, wrapping to the next row; Enter on the last row creates a line; Shift + Space selects a row. TSV paste from Excel shows a per-cell validation preview and rejects before anything commits; number and duration cells accept `=`, `+=`, `-=`, `*=`, `/=` (Odoo 19 and 20).

**Form saving (Proposed).** One `update` command sends changed fields with `row_version` and a client `command_id` (D9), so a double click cannot post twice. Existing drafts autosave when focus leaves the form (setting, default on); new records need Create. Errors appear at the field and in a summary that takes focus.

**Bulk edit.** Select N rows, edit one cell, confirm "Apply to N records" with permission-skipped rows shown. It runs as one atomic `$batch` (D31); on any failure nothing applies and per-row errors are listed, with an explicit "apply to valid rows only" (Proposed).

**Undo scopes (Proposed).**

| Scope | Behaviour |
|---|---|
| Editing session | Client undo and redo of unsaved and saved-draft edits (step limit 50 is a proposal); saved-draft undo sends inverse commands with new `command_id`s; never on posted or locked records |
| Archive, delete | Archive is reversible indefinitely. Drafts delete to a trash with 30-day retention, as Odoo Documents does [R15 s6]; where law requires erasure, anonymise in place (D33). A 10-second Undo toast [R19 s4] is duplicated in the record history, so nothing depends on a timer |
| Bulk edit | "N updated, Undo" restores before-images for rows unchanged since (`row_version`) and reports the rest |
| Posted documents | No edit; "Reverse" or "Credit note" creates a reversal (D30) |
| Imports | Run rollback until posted or referenced (4.14) |

**Conflicts.** Optimistic `row_version`. On 409 the dialog names who changed what and when, lists differing fields and offers keep mine, take theirs or per-field merge [R22 s6]. Presence and co-editing of forms are out of scope; collaborative text is documents and knowledge only (D39).

### 4.8 Notification centre and Inbox

The in-app inbox is the source of truth; email and Web Push are delivery channels [R19 s4]. W1: in-app and email, mentions, activities, alerts, snooze, triage. W2: Web Push (VAPID, RFC 8030, 8291, 8292), approvals lane and agent proposals with the approval policy engine (D18) [R15 s12]. Proposed, placed by [platform.md](platform.md) 4.6 and not by D25 or D26: digests and quiet hours in W2, SMS through a connector in W3. That section owns the `notification` entity and its delivery channels, so this one follows its placement and carries it only as a Proposed label.

**States.** One `notification` row per recipient and channel carries both the `delivery_status` (pending, sent, failed, with a `failure_reason` per channel [R15 s2]) and the `inbox_state` (unread, read, snoozed, done, with `snoozed_until`) ([platform.md](platform.md) 4.6; [CONVENTIONS.md](../CONVENTIONS.md) section 6). An approval item shows the `approval_request` state: pending, approved, executed, rejected, cancelled, expired or superseded (digest changed) ([platform.md](platform.md) 4.12).

**Workflow.** (1) An event creates notifications per the `notification_preference` matrix (event type by channel; [platform.md](platform.md) 4.6). (2) The badge updates over the WebSocket. (3) J and K move, Enter opens, E marks done, H snoozes (later today, tomorrow, next week, a date). (4) An approval shows diff and risk; Approve and Reject take a reason; payments above the business threshold and bank-detail changes require a fresh passkey or TOTP step-up ([security-privacy.md](security-privacy.md) 4.1, Proposed there).

**Privacy and limits.** Email and push carry a link and minimal metadata unless the business opts in to content (Proposed), because mail leaves the trust boundary. iOS push needs a Home Screen web app and a tap-initiated prompt, so the UI explains this [R19 s4]. Agents cannot approve their own proposals. **Configuration:** default matrix per business, quiet hours and digests (W2, Proposed), retention of done items (90 days, Proposed). **Reports:** unread by lane, approval age, failed deliveries.

### 4.9 Design system and theming

One component set and one token pipeline serve back office, portal, POS and extensions; accessibility lives in the primitives (React Aria Components).

**Tokens.** The Design Tokens Format Module 2025.10 of the W3C Design Tokens Community Group (DTCG), the first stable edition (2025-10-28), a Final Community Group Report and not a W3C Standard [R19 s5.6; R24 s10]. Tiers: primitive (OKLCH ramps, spacing), semantic (`--surface`, `--text-muted`, `--border-focus`, `--money-negative`), component (`--grid-row-height`). A DTCG-compatible compiler (Style Dictionary 5.5.5 or Terrazzo 2.7.1, choice Proposed) emits CSS custom properties as a `vp run` task. The root element carries `data-theme`, `data-density`, `dir` and `lang`; switching is an attribute change, never a second bundle. `light-dark()` is not widely available until about 2026-11-13 (estimate) and Vite 8's default target is Chrome 111, Firefox 114, Safari 16.4, so tokens compile with fallbacks for that floor (Proposed) and CSS anchor positioning stays unused [R19 s5.6].

**Density.** Compact for accountants (checkbox and row-action hit areas keep 24 by 24 CSS px, WCAG 2.5.8), comfortable by default, touch for POS, warehouse and phones (48 px targets) [R19 s10]. Theme follows `prefers-color-scheme`; forced colors and reduced motion are honoured.

**Business branding (open).** Logo (light and dark), favicon, one accent hue expanded to an OKLCH ramp, default density, login message. An automatic WCAG 2.2 AA contrast check blocks failing combinations in light and dark; no custom CSS or fonts in the back office [R24 s10]. Portal, website and checkout use `kind="theme"` packages: tokens plus block templates, sanitised CSS, storefront on a separate origin from admin [R24 s10]. Partner white-label (rename, remove attribution, admin domain, reseller console) is `ee` and paid, Proposed (D2).

**Other rules.** System font stack; any bundled font is self-hosted (Proposed). SVG icons with accessible names or `aria-hidden`. Logical CSS only; a CI check bans physical margins and paddings because Oxlint cannot [D35; R07 s3.5]. Visual regression with `toMatchScreenshot` takes baselines from one pinned container only [R27 A2] and starts once the team reaches about 12 ([ENGINEERING.md](../ENGINEERING.md) 1.8); until then it is not a release gate.

### 4.10 Accessibility: WCAG 2.2 AA as a release gate

**Scope.** Back office, portal, storefront, checkout, POS. The European Accessibility Act has applied to services since 2025-06-28 (microenterprise services exempt); the storefront, checkout, portal, website builder and POS front end are in scope when merchants sell to consumers, so defaults meet WCAG 2.2 AA and the consumer withdrawal function ships (D38) [R28 E.5, E.7].

| SC | Level | obop rule |
|---|---|---|
| 2.4.11 Focus Not Obscured | AA | Sticky headers, chatter and panels never fully cover a focused control; `scroll-padding` from tokens |
| 2.5.7 Dragging Movements | AA | Every drag has a single-pointer alternative (https://www.w3.org/WAI/WCAG22/Understanding/dragging-movements.html) |
| 2.5.8 Target Size | AA | 24 by 24 CSS px, including compact rows; 48 px in touch density |
| 3.3.7 Redundant Entry | A | Never re-ask known data within a flow |
| 3.3.8 Accessible Authentication | AA | Password managers, paste and passkeys work; no cognitive-test CAPTCHA |
| 3.2.6 Consistent Help | A | Help in the same place on every screen |
| 2.1.4 Character Key Shortcuts | A | Single-key shortcuts can be disabled or remapped (https://www.w3.org/WAI/WCAG22/Understanding/character-key-shortcuts.html) |
| 4.1.3 Status Messages | AA | Toasts, autosave and "N records updated" use `role="status"` with `aria-atomic`; `role="alert"` only for errors |

Also 4.5:1 text and 3:1 non-text contrast, 320 px reflow, text-spacing override and forced colors (standard WCAG 2.x criteria, not re-fetched) [R19 s8].

**Gates** (release blockers; wiring in [ENGINEERING.md](../ENGINEERING.md)): (1) `vp check` runs Oxlint with `jsx-a11y` rules; (2) `vp test` component tests and `vp run e2e` Playwright with axe cover every view type and every golden path in section 5; (3) manual NVDA and VoiceOver passes on five flows (sign-in, list, form, grid entry, reconciliation) before each release, since automation covers only part of the criteria [R19 s8]; (4) canvas grids are banned for primary lists [R07 s3.3]; (5) a VPAT-style statement for back office, portal and storefront, mapped to EN 301 549; ETSI lists V4.1.1 (2026-09-02) but its content was not read, so the mapping is pending [R19 s8].

**Extensions.** F3 publishers declare a WCAG 2.2 AA statement in `obop.toml` and pass axe in `obop ext test` [R24 s4]. Registry `help` strings feed tooltips and the `?` link to the docs page of the installed version [R28 B.3].

### 4.11 Internationalisation, RTL, CJK, locale formats

**MVP (W1).** PO catalogues with an ICU MessageFormat 1 subset: Lingui 6.8 in TypeScript, a PO parser and `icu` 2.3.1 in Rust, same golden fixtures in both; Weblate; 12 target locales at first pack stable (D34; R25 proposes de, fr, pl, en, nl, es, it, pt, sv, da, nb, fi, which R25 tags as a guess, and R28 proposes a different launch set; see [LOCALIZATION.md](../LOCALIZATION.md)). Machine translation is a suggestion and banned for statutory labels, which come from the pack's authority vocabulary ([LOCALIZATION.md](../LOCALIZATION.md)). MessageFormat 2 is reviewed in 2027-H2.

**Three locale axes (Proposed):** UI language (user), format locale (user, defaults to UI language) and document language (party or document), so an invoice can show in English and print in German.

| Topic | Rule |
|---|---|
| Direction | `dir` on `<html>`; logical properties; `dir="auto"` and `<bdi>` for user strings; invoice numbers, SKUs, phones isolated as LTR runs; directional icons mirrored by rule, time axes never [R19 s9] |
| Dates | Accounting dates are plain dates; events are instants plus IANA zone; `Temporal` is not Baseline, so `temporal-polyfill` 1.0.5 or Rust formatting |
| Numbers, phone, address | CLDR 48.2 (49 in beta); `libphonenumber-js` 1.13.14 and Rust `phonenumber` 0.3.10; per-country address format data including CJK order (no maintained Rust address crate verified) |
| CJK input | Ignore Enter and shortcuts while `event.isComposing`; also guard `keyCode === 229` for Safari before 27, where `compositionend` precedes the confirming keydown (WebKit 165004, fixed in Safari 27 per MDN data); test with real IMEs in Chrome, Firefox, Safari [R19 s7] |
| CJK data | NFKC in numeric and barcode fields; family-name-first display; optional reading field; `Intl.Segmenter` for word search |
| Search | Capability-detected: `pg_trgm`, `unaccent`, ICU; PGroonga for CJK (D34) |
| Data translation | Per-language values with import and export; in-context translation mode from the palette [R19 s9]; PDFs embed CJK and Arabic fonts (a guess in [R25 C4]; whether Typst shapes bidirectional and complex scripts correctly is unresearched, [LOCALIZATION.md](../LOCALIZATION.md)) |
| CI | RTL fixtures (ar, he, fa) and CJK fixtures (ja, ko, zh) in every UI snapshot test (D34); none of the 12 target locales is RTL or CJK, so these are fixtures until a market needs them |

### 4.12 Responsive layouts, phones, tablets, scanning, hardware

W1: responsive layouts, touch density. W2: scan input widget with the barcode nomenclature (Proposed; [supply-chain.md](supply-chain.md)). W3: POS shell (Tauri 2.12.1 plus a Rust hardware agent, PWA baseline, D37) and the warehouse edge command outbox, Proposed (D13). W4: barcode and WMS depth, Proposed (D25).

**Layout.** Container queries (widely available 2025-08-14) adapt one view definition to side panel, full page or tablet split; `any-pointer: coarse` raises targets [R19 s10]. Phone: lists become cards; forms become one column with a sticky action bar (primary action plus overflow), compact statusbar, bottom-sheet pickers, pull to refresh; chatter moves below; swipe-down opens the palette (Odoo 20). Tablet: list left, record right.

**Scanning.** (1) Keyboard wedge first: most scanners type a burst ending in Enter; only fields or screens that declare scan mode capture bursts by inter-key timing, so ordinary typing is never hijacked [R19 s10]. (2) Camera: `BarcodeDetector` where present (not Baseline: Chrome Android 83+, desktop Chrome on ChromeOS and macOS only, Safari behind a flag, no Firefox), otherwise the `barcode-detector` 3.2.2 ponyfill on `zxing-wasm` 3.1.3 [R19 s10]. (3) GS1-128 parsing by application identifier (00 SSCC, 01 GTIN, 10 lot, 17 expiry, 21 serial; FNC1 is `\x1D`) in the shared Rust-to-Wasm module [R10 s3.9; R07 s3.8]. (4) Feedback is visual and haptic, never sound alone; unknown codes and denied camera permission fall back to manual entry.

**Hardware.** WebHID, WebUSB and Web Serial are not Baseline, Safari has none, and Chrome 142 prompts for Local Network Access. The POS shell therefore reaches printers, scales, drawers and terminals only through the Rust hardware agent (D37); other surfaces use browser APIs opportunistically [R12 s3.3, s3.4]. The POS tablet follows Odoo's "continue with limited functionality" pattern [R19 s10]; internals are in [retail-commerce-marketing.md](retail-commerce-marketing.md).

### 4.13 Offline and PWA contract

Basis: Proposed (D13) targets Odoo 19 parity at 1.0 for the apps that ship by then: a read cache of visited views and a published degraded-feature list. POS order capture with post-reconnect invoicing ships with `pos` and `obop-edge` in W3, after the 1.0 LTS, unless the owner pulls a slice into W2 (Q11, open); queued writes follow in W3+. No Electric, PowerSync or Zero. The edge store is fixed-schema SQLite, at most 15 tables, never a projection of the server schema (D37).

| Feature | By the 1.0 LTS, 2028-04-18 (read cache starts W1, Proposed) | W3 and later |
|---|---|---|
| Visited views and records | Readable, marked as saved data | Same |
| Create, edit, validate | Disabled with a reason | Queued for allow-listed models (timesheets, stock counts, expenses, notes); reject-and-review for stock and ledger, last-writer-wins for notes [R19 s11] |
| POS orders | n/a until `pos` ships in W3 (Proposed (D25)); D13 places order capture with `pos` in W3 (Q11 open) | UUIDv7 ids and per-device counters, idempotent replay |
| Card terminals, cloud fiscal signing, online orders, central stock | Unavailable, stated in the banner | Unchanged [R12 s3.2] |

**States.** `online`, `degraded`, `offline`, `syncing`, shown in words and icon in the status strip; a banner links the degraded-feature list; a "N changes pending" counter appears from W3. Nothing drops silently, and users are told not to clear site data before sync completes.

**Storage.** IndexedDB or OPFS with a quota and LRU purge (Odoo caps at 2 GB; obop sets its own cap, Proposed). Background Sync is not Baseline, so sync runs while the app is open; Safari storage is best-effort unless `navigator.storage.persist()` is granted [R19 s11]. Shell caching: `vite-plugin-pwa` 1.3.0 with Workbox 7.4.1. Sign-out, session revocation and account switch purge the cache. Fields in sensitive classes (`confidential_hr`, `special_category`, `financial`) are excluded from the browser cache by default (Proposed). A per-view "data caching" switch can disable caching, as in Odoo 20 [R22 s2].

### 4.14 Onboarding, setup, sample data, import

The learning curve is the most common complaint in Software Advice reviews of Odoo [R04 s4.8]. Target: a compliant first invoice within 15 minutes of starting (target; R20 s8 calls it a hypothesis to earn), tested with the 3 to 5 design partners signed in W0, Proposed (D26).

**Setup wizard** (resumable, each step skippable, state in `setup_progress`; sample data off by default) [R19 s13]:
1. Country and locale: lists packs with their gate status (draft, preview, stable, certified; D34) and never calls an unfinished pack ready; loads chart, taxes, formats and currency from the signed, effective-dated pack. A country without a pack gets a generic configuration with a warning (Proposed).
2. Legal entity and company identity (D28). 3. Fiscal year. 4. Apps to enable; starting with one is fine (the free invoicing on-ramp, Proposed (D4)). 5. Invite users with permission-group templates. 6. Sample data toggle.

A setup checklist (configuration linter, Proposed) stays on the home screen until complete: missing tax mappings, bank account, numbering, e-invoice identifiers [R04 Implications 11].

**Sample data (Proposed).** Empty views show ghost rows labelled "sample" that are never persisted (Odoo's `sample_server` pattern [R19 s13]). Opt-in sample records exist only as drafts in non-ledger apps (contacts, products, leads, draft quotes), tagged with a sample-run id and removable in one action. Nothing in sample data may reach a posted state, since posted entries are immutable (D30). Users who want to try accounting use a separate demo business (embedded PostgreSQL, D37).

**Import pipeline** (the engine entities `import_run`, `import_mapping` and `import_row` and the run states are owned by [platform.md](platform.md) 4.16; foreign-source adapters, the `external_ref` ledger, the accounting diff and the posting step are in [integrations.md](integrations.md) 4.8; the dry-run outcome is the `dry_run_result` field (ok, failed) of the run and `posted` is a flag that the accounting importers' posting step sets, so neither is a run state; this section draws the screens and the user-visible state path) [R19 s13; R21 s9; D35].

```mermaid
stateDiagram-v2
  [*] --> uploaded
  uploaded --> mapped
  mapped --> mapped: fix file or mapping after failed dry run
  mapped --> validated: dry run passes
  validated --> committing
  committing --> paused: stopped, chunks resume
  paused --> committing: resume
  committing --> committed
  committing --> failed: job error, nothing kept
  committed --> rolled_back: none referenced
  committed --> partially_rolled_back: some referenced
```

1. **Upload** CSV, XLSX or ODS, with downloadable templates per model.
2. **Map:** saved mappings, then exact name, then type-compatible fuzzy match; an `id` column is the external id (`module.name`), so re-imports update instead of duplicating (D22).
3. **Dry run** in a transaction with per-row, per-cell errors and a downloadable error file; large files stage through non-RLS tables because `COPY` into RLS tables is rejected (D10; [R22 s8]).
4. **Commit** as a tracked run: the run id is on every created or updated record, updates revert from `audit_change` rows tagged with it (platform.md 4.16; forced capture of every changed field is Proposed, with a cause reference added in [security-privacy.md](security-privacy.md) 4.4, and a separate `import_change` table is the fallback, integrations.md open question 12; see Open question 14), chunks resume, progress arrives via `JobProgressed`.
5. **Rollback** by run id: all-or-nothing by default, refused while a created record is posted or referenced, with the blockers listed; "unreferenced only" is the explicit alternative and ends in `partially_rolled_back`. Records that were posted are never deleted: corrections are reversal documents (D30).
6. **Report:** created, updated, skipped, rejected.

| Source | Wave | Notes |
|---|---|---|
| CSV, Excel | W1, 0.3 | Templates; import-compatible exports carry external ids [R21 s10] |
| Odoo | W1, 0.6 | Export files with external ids, or live JSON-2 (Odoo 19 and 20); a legacy-RPC reader for Odoo 14 to 18 is proposed by R21 but D22 lists only JSON-2 and exports, so it waits on [integrations.md](integrations.md) open question 3; `external_ref` identity ledger; opening balances as dated journal entries plus a trial-balance diff; open balances rather than history by default (Proposed) [R21 s9.2; R19 s13]; CI round-trip test (D22) |
| QuickBooks | W1, 0.6 | API or report exports; Intuit specifics were not retrievable, so the mapping spec is a gap [R19 s13] |
| Xero, ERPNext | W2 | Xero's API is metered; the free Starter tier suffices for a one-shot migration [R21 s9.3] |

**Permissions.** Import needs `import_data` plus create or write on each target model, checked per row. Uploaded files may hold personal data, so they are stored and encrypted like attachments and deleted a configurable period after commit (Proposed). **Configuration:** maximum file size, per-model rights, retention. **Localisation:** parsing follows the file's declared locale, defaulting to the user's format locale. **Open or paid:** open.

## 5. Cross-app golden paths

Acceptance tests: each runs with axe and must pass before its wave exits. Amounts use English formatting unless a locale is stated.

**GP-1. Keyboard-first invoice entry with Excel paste (W1).**
- *Given* the German pack, a EUR company, user format locale `de-DE`, and an empty draft customer invoice in compact density,
- *when* the user pastes four tab-separated rows (`Beratung` 8 at 95,00; `Reisekosten` 1 at 120,50; `Lizenz` with quantity `abc` at 49,00; `Support` 2 at 60,00; all 19 % VAT) and accepts the preview with Ctrl or Cmd + Enter,
- *then* the preview lists 3 valid rows and 1 rejected cell ("Quantity must be a number") and commits nothing until accepted; net is 1.000,50, VAT 190,10 (`per_group`, half-up assumed) and total 1.190,60, with no diff banner because the local preview equals the server total; a live region says "3 lines added, 1 rejected"; the explicit Post action assigns the legal number and makes the grid read-only (D30).

**GP-2. Reconciliation by keyboard (W1, `de-DE`).**
- *Given* open invoices RE-2027-0042 (1.190,60) and RE-2027-0050 (2.000,00) for one customer and a statement with +1.190,60 "RE-2027-0042", -350,00 "Büromaterial Müller" and +500,00 "Teilzahlung",
- *when* an accountant works only by keyboard: Enter on line 1 (suggestion "exact amount plus reference"), a new entry for line 2, Enter on line 3 ("same customer, partial"), then Ctrl or Cmd + Enter,
- *then* the batch commits as one command with one `command_id` (pressing twice creates no duplicate); RE-2027-0042 shows paid and RE-2027-0050 a residual of 1.500,00; a live region says "3 of 3 lines validated"; un-reconciling later creates reversals.

**GP-3. CSV import with dry run and rollback (W1).**
- *Given* a CSV of 1,200 contacts whose headers (`Firma`, `E-Mail`) differ from field labels, 37 rows lacking the required country,
- *when* the administrator maps columns (saved as "Kontakte CSV"), dry-runs, commits valid rows, then rolls back after 12 new contacts were used on draft quotes,
- *then* the dry run reports 1,163 valid and 37 errors with row numbers and persists nothing; commit creates 1,163 contacts tagged with the run id; rollback is refused and lists the 12; "unreferenced only" removes 1,151 and keeps 12, ending in `partially_rolled_back`; the audit log shows the run.

**GP-4. Approving an agent proposal (W2).**
- *Given* a policy "vendor bills over 2,500.00 or with changed bank details need an `account.payment_approver`" (configurable) and an agent acting for Nora that drafted a 4,800.00 bill whose supplier bank account differs from the stored one (dry-run plan digest `9f2c`),
- *when* Marco, who holds `account.payment_approver`, opens the Inbox (G then I), selects the item (J, K) and approves with the reason "Bank change confirmed by phone",
- *then* the item reads "requested by agent for Nora" with the diff bound to digest `9f2c`; a passkey step-up precedes approval; Nora and the agent see Approve disabled with "Separation of duties"; approval applies the plan with digest `9f2c` only and the bill stays a draft until a human posts it; if the draft changed after that dry run the request is `superseded` and the button reads "Plan outdated, re-run dry run".

**GP-5. Offline (A by the 1.0 LTS with the read cache starting in W1, B in W3).**
- *Given* (A) a warehouse tablet in touch density that visited delivery DO-0017 and two other transfers today, *when* Wi-Fi drops for 7 minutes, *then* a banner says "Offline, showing data saved at 10:41"; visited records are readable; Edit and Validate are disabled with reasons; nothing is queued; the degraded-feature list opens from the banner; on reconnect the banner clears and open records refresh.
- *Given* (B) a POS shell with the same drop, *when* three cash orders of 24.90, 31.50 and 30.00 (total 86.40) are captured, *then* card payment is disabled with a reason; on reconnect all three sync and a duplicate upload yields 3 orders, not 6; the invoice requested for order 2 is generated after reconnect and gets its legal number at posting.

**GP-6. Kanban without dragging (W1).**
- *Given* a CRM pipeline with New (4 cards), Qualified (2) and Won (1),
- *when* a keyboard user focuses "Acme, 12,000.00", opens its menu and chooses "Move to Qualified",
- *then* counts become 3, 3, 1; focus stays on the card in its new column; a live region says "Acme moved to Qualified, position 3 of 3"; the same works with single clicks; axe reports no violations.

**GP-7. RTL and CJK input (W1).**
- *Given* (fixture locales: none of the 12 target locales is RTL or CJK) a business with Arabic UI and format locale showing invoice `INV/2027/0042`, and a `ja` user editing a product-name cell with an IME,
- *when* the list renders and the user presses Enter to confirm a conversion,
- *then* `dir="rtl"` is on `<html>`; numeric columns align by logical properties; the invoice number is an isolated LTR run; the first Enter (composing) does not commit the cell, the second does; the ar, he, fa, ja, ko and zh snapshot fixtures pass.

**GP-8. Saved view across permissions (W1).**
- *Given* Anna (Sales), whose permission rows restrict quotes to her own, and a manager,
- *when* Anna saves "Open quotes over 5,000 by salesperson" as personal, the manager promotes it to shared for the Sales group, and both open it,
- *then* Anna sees 14 quotes and the manager 31, each under their own permissions; a shared link to a quote outside Anna's scope shows the generic not-found page with no record detail; the URL reproduces filters and grouping.

## 6. Non-functional requirements

### 6.1 Integrity, concurrency, audit

1. The UI never computes authoritative money; previews use shared engines and the server wins.
2. Posted and locked documents have no edit path in any view type; corrections are reversal documents (D30).
3. Every mutation is one typed command with a `command_id`, so double submits are idempotent (D9, D27).
4. Constraints compile from field metadata and the server revalidates; mixed-currency sums never appear without a target currency and rate type; a drilled-through tax or payroll figure shows its rule-pack version and inputs (D38).
5. Saves use optimistic `row_version` (4.7); realtime events invalidate caches but never overwrite dirty edits; bulk operations are atomic `$batch` calls by default (D31).
6. Overlay publication, sharing, import and export are audited; record history shows who changed what [R19 s4]; reads of sensitive classes and exports are read-audited ([security-privacy.md](security-privacy.md)).

### 6.2 Accessibility and offline

The gates in 4.10 block releases, and GP-1, GP-2, GP-6 and GP-7 double as screen-reader scripts. The degraded-feature list (4.13) is published with each release, and tests fail when a feature silently works or fails differently offline.

### 6.3 Performance budgets (targets, to be validated; not measurements)

External thresholds: Core Web Vitals INP at most 200 ms, LCP at most 2.5 s, CLS at most 0.1 at the 75th percentile (https://web.dev/articles/vitals) [R19 s12]. Comparators: Odoo pages 80 rows; Airtable previews 1,000 of more than 5,000 visible records.

| Area | Budget (target) |
|---|---|
| Shell JavaScript | 250 kB gzip to interactive; each module or view lazy-loaded at 100 kB or less |
| View definition | 20 kB or less per view, ETag-cached, prefetched on menu hover |
| List | 100 DOM rows or fewer in the render window; a 100,000-row list scrolls at 60 fps on the reference laptop; keyset pages of 200 to 500; count capped and labelled |
| Interaction | Cell edit echo within 50 ms; optimistic commit visible within 100 ms; server round trip p95 within 300 ms |
| Navigation | List to side-panel record within 150 ms from cache |
| Gate | A 100k-row Playwright test and Lighthouse and INP checks as `vp run` tasks (names Proposed), nightly on a dedicated machine with a plus or minus 10 % alarm because shared runners are noisy [R27 A2]; field INP collection is opt-in, Proposed (D16) |

Whether the Wasm modules fit the per-view budget is unknown until the R07 spike (Open question 7), and "reference laptop" is undefined (Open question 4). Jumping to an arbitrary position beyond the count cap is unsupported with keyset pages; users narrow by search, sort, filter and group (Proposed; Open question 5).

### 6.4 Client security and compatibility

Strict CSP with nonces and no inline script; server-side sanitising of stored HTML [R18 s10]; F2 and F3 iframes use `sandbox="allow-scripts"` without `allow-same-origin` [R24 s4]; no third-party code in the main realm (D11); `Referrer-Policy: same-origin`. The browser floor is Vite 8's default target (Chrome 111, Edge 111, Firefox 114, Safari 16.4, iOS 16.4), Proposed; each release runs the UI suite on floor and current browsers.

## 7. Where obop deliberately differs from Odoo, and why

Pain ranks are from the voice-of-customer study [R04 s1]: 1 upgrades, 2 open-core surprises, 3 customisation debt, 6 module quality, 7 performance, 8 UX and learning curve.

| # | Odoo | obop | Pain |
|---|---|---|---|
| 1 | Gantt, map, cohort, grid, Studio, report drill-through, reconciliation widget, approvals are Odoo Enterprise behaviours [R19 s1] | All in the open edition (Proposed (D2); D35) | 2 |
| 2 | XPath inheritance breaks when the base view changes [R19 s2.5; R02 s4] | Slots, stable node ids, structured patches, overlays, customisation report | 1, 3 |
| 3 | Python expressions in XML evaluated in the browser; a possible server round trip on every field change for onchange [R19 s2.2; R02 s2.3] | CEL plus compiled form, Proposed (D12), domains as JSON AST, one evaluator, no `eval`; shared engines preview totals and the server stays authoritative | 3, 7 |
| 4 | 80-row pager, count capped at 10,000 [R19 s2.3]; no range select or paste (inferred from source, [R19 s2.5]) | Virtualised keyset grid, range select, TSV paste | 7 |
| 5 | Imports are permanent [R19 s2.5] | Dry run and run-level rollback | 1, 8 |
| 6 | Accessibility "not a priority" in 2018, no conformance statement found [R19 s2.5] | WCAG 2.2 AA gate and statement | 8 |
| 7 | Alt+letter shortcuts collide with browser, OS, screen readers [R19 s2.5] | Ctrl or Cmd combinations, remappable, single-key opt-in | 8 |
| 8 | Owl 3.0.0-alpha.49 beside legacy JS [R02 s0; R04 s4.8] | One architecture, no runtime patching | 8 |
| 9 | Odoo 20 queues offline writes [R22 s2] | Odoo 19 parity at 1.0, queued writes W3+, Proposed (D13), explicit degraded list | n/a (scope) |
| 10 | Steep learning curve, consultant-led setup [R04 s4.8] | Wizard, checklist, importers, in-product help | 8 |
| 11 | Studio is Custom-plan only and edits XPath XML [R15 s11] | Open overlay editor; customisations export as packages | 2, 3 |
| 12 | Extensions monkey-patch at runtime [R02 s8] | F0 to F3; no third-party UI in the main realm (D11) | 6 |

## 8. Out of scope, non-goals and later

- Native store apps beyond PWA and Tauri shells; Tauri mobile maturity is unverified [R07 s3.9].
- Local-first sync engines or CRDTs for business data, Proposed (D13); Yjs only for documents (D39).
- Queued writes, Gantt, map, cohort (W3+); spreadsheet view (not scheduled).
- Canvas and commercial grids, FullCalendar Premium resource views (D15); custom CSS and fonts in the back office.
- A visual report or PDF template designer (templates use minijinja feeding Typst, Proposed (D20); [platform.md](platform.md)); the website visual builder and storefront internals, Proposed (D14) ([retail-commerce-marketing.md](retail-commerce-marketing.md)).
- Importing Odoo XML views, Studio customisations or any Odoo Enterprise asset (clean-room rule); the importer moves data, not UI.
- Voice interfaces; presence and co-editing of forms.
- User research: none has been done; needs are inferred from product behaviour [R19 Confidence]. A research plan with the W0 design partners is required before the W1 exit.

## 9. Open questions

1. **Single-key shortcut defaults:** which contexts default on? Needs sessions with bookkeepers and screen-reader users.
2. **Calendar and Gantt libraries:** FullCalendar 7 core, SVAR, DHTMLX Community or custom; WCAG 2.2 and ERP-scale behaviour are unverified [R07 s3.3]. Time-boxed spike before W1 beta.
3. **Map tiles:** the tile source is a setting, but the default provider, its cost and the privacy of sending coordinates to it are undecided; self-hosted tiles?
4. **Reference laptop and browser floor:** define the budget hardware; confirm Vite 8's default floor as obop's.
5. **Scrolling beyond the count cap:** accept "narrow your query", or add a bounded random-access mode?
6. **Proposed (D12), owner question Q9 by 2026-12-31, and Proposed (D13), Q11 by 2027-09-30:** the tagged-union node limits the cost of either D12 outcome; offline parity at 1.0 is Odoo 19 or Odoo 20.
7. **Wasm evaluator and decimal module:** bundle size and start-up cost against the budget [R07 s3.8].
8. **QuickBooks source:** API, report export or both; Intuit documentation was not retrievable.
9. **Sample data on the ledger:** are draft-only samples plus a demo business enough for trials?
10. **Dashboard and Gantt placement:** this document follows the T2 W3 "dashboards" of Proposed (D25) and platform.md 4.17. Owner questions: does a minimal dashboard container ship earlier, in W2, so owners have a home screen when the loop lands; are default dashboards per app (pipeline, cash position, stock, open tickets) a committed X1 pack deliverable with it; and does a basic Gantt ship with `project` in W2 for the services variant?
11. **Address formats:** no maintained Rust crate verified; data source and licence undecided [R19 s9].
12. **EN 301 549 V4.1.1** content, statement template and owner; **IME and iOS Home Screen behaviour** need hands-on tests [R19 Confidence].
13. **Permission rows** `customize_views`, `share_views`, `import_data` and `manage_branding` are named in [security-privacy.md](security-privacy.md) 2 and registered as capability rows in [CONVENTIONS.md](../CONVENTIONS.md) section 4 (Proposed). Still open: which permission groups hold them by default, to be set with `customize.designer` in [platform.md](platform.md), which already expects `customize_views` from this document.
14. **Names shared with sibling specs.** Settled by [CONVENTIONS.md](../CONVENTIONS.md) sections 3.4, 6 and 8: (a) the run is `import_run` with `ImportRunCommitted` and `ImportRunRolledBack`, in the nine states of the diagram in 4.14, with the dry-run outcome as the `dry_run_result` field (ok, failed) and `posted` as a flag; (b) the Inbox reads `inbox_state`, `snoozed_until` and `lane` on the `notification` row, and the matrix is `notification_preference`; (c) events are listed in CONVENTIONS 3.4 until the catalogue generated from the `*-api` crates (ARCHITECTURE.md 4.3) exists. Still open and Proposed: forcing `audit_change` capture of every changed field on an import run, with a cause reference in security-privacy.md 4.4, against keeping a separate `import_change` table (integrations.md open question 12); until it is answered the rollback in 4.7 and 4.14 assumes `audit_change`.
15. **W1 UI scope against capacity.** Proposed (D25) lists six view types for W1; this document adds activity, hierarchy, settings, the report viewer, the reconciliation split view, the Inbox, saved views, the palette, the setup wizard, the import screens and the offline read cache, while [ROADMAP.md](../ROADMAP.md) warns that three items alone take about 78% to 96% of W1 capacity and names business branding, the DTCG pipeline and theme packs as the first trim at the 0.3 checkpoint. Which UI items slip first? A proposal: the report viewer and reconciliation split view stay (they are the accounting loop), while branding, theme packs, hierarchy, the settings view, the offline cache and the palette's non-navigation commands may follow in W2.
16. **o-spreadsheet and the clean-room rule (F3).** Using Odoo's own LGPL-3 spreadsheet library as a dependency is not copying Enterprise code, but it is Odoo-authored code that is coupled to OWL; a counsel or owner ruling is needed before it is considered, otherwise Univer core is the default.

## 10. Sources

Decision IDs (D1 to D39) refer to [DECISIONS.md](../DECISIONS.md); the spellings of shared names are those of [CONVENTIONS.md](../CONVENTIONS.md). Research tags map to the files below; "s" numbers are section numbers inside each file, and "Implications n" refers to the numbered list in its "Implications for obop" section.

| Tag | File |
|---|---|
| R00 | [research/README.md](../research/README.md) |
| R02 | [research/02-odoo-architecture.md](../research/02-odoo-architecture.md) |
| R04 | [research/04-odoo-voice-of-customer.md](../research/04-odoo-voice-of-customer.md) |
| R07 | [research/07-vite-plus-frontend.md](../research/07-vite-plus-frontend.md) |
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
| R24 | [research/24-extension-platform-sdk.md](../research/24-extension-platform-sdk.md) |
| R25 | [research/25-localization-operating-model.md](../research/25-localization-operating-model.md) |
| R27 | [research/27-quality-release-engineering.md](../research/27-quality-release-engineering.md) |
| R28 | [research/28-docs-community-support-brand.md](../research/28-docs-community-support-brand.md) |

The inline WCAG 2.2 Understanding and web.dev URLs are cited as verified in [R19 s8] and [R19 s12].
