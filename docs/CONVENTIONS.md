# Conventions: naming and contract authority

- **Status:** Draft 0.1
- **Date:** 2026-10-01
- **Scope:** The spellings and cross-document contracts that more than one obop document uses: identifier forms, the CLI grammar names, the plugin and registry names, the event catalogue, permission groups, data classes, shared entities and states, hub ports and provider traits, and the rename map that aligns the other documents. It defines no product behaviour.

Reading notes. "Adopted" means this file fixes the spelling or contract for every document. It does not ratify a Proposed record of [DECISIONS.md](DECISIONS.md): the T0 to T3 tier labels, the per-module wave placements (D25, D86) and every date that D26 does not name stay Proposed wherever they appear, except that the in and out calls of the W1 cut are RATIFIED (OR-15; the slip order and the landing waves of the displaced items, apart from the services variant in W3, stay Proposed). What the owner rulings of 2026-10-01 (OR-1 to OR-21, "Owner rulings" in DECISIONS) settled is RATIFIED here and cited by its OR number: the D24 renames (`BusinessTx`, `BusinessRouter`, `business_release_state`) and the X and L prefixes (RATIFIED, OR-14), the wave windows W0 to W4, the German pack date and the 1.0 target (OR-13), the plugin registry with its install and copy modes and the `community` and `verified` tiers (OR-3), its OSI-approved-licence admission rule and the meaning of `verified` (OR-17), literal UI parity with an owner-approved exemption list (OR-16), and the hosted free sandbox of the public beta (OR-20). There is no paid list, plan, price or edition to name: the whole project is Apache-2.0 and free (OR-1, OR-2). "Proposed" marks a contract that rests on a Proposed record, or a design choice this file makes that no owning spec has accepted yet; the CLI grammar of 2.4 is Proposed until the owner answers Q22. Q-numbers are the owner questions of DECISIONS section 8. Documents are abbreviated as in the rename map (section 8): `sec-priv` is [spec/security-privacy.md](spec/security-privacy.md), `retail` is [spec/retail-commerce-marketing.md](spec/retail-commerce-marketing.md), `cli` is [spec/cli.md](spec/cli.md), and so on. Nothing here is legal advice.

## 1. Purpose, authority and change procedure

**Purpose.** The other documents of the set, the thirteen specs (with [spec/cli.md](spec/cli.md) and [spec/mobile.md](spec/mobile.md)) among them, share identifiers that cross files: event types, permission groups, data classes, entities and states, port implementers, and the names the `obop` CLI, the REST API and MCP have in common. This file is the one place where such a name is spelled, so a conforming document can be checked against it.

**Authority.** Order of precedence: DECISIONS.md (its "Owner rulings (2026-10-01)" section included), then this file, then [ARCHITECTURE.md](ARCHITECTURE.md), then every other document. Where a research note, a spec or ARCHITECTURE.md differs from this file on a name or a shared contract, this file wins. Where this file differs from DECISIONS.md, DECISIONS.md wins and this file is corrected in the same change. This file never settles an owner decision: whatever DECISIONS section 8 lists or marks PROPOSED stays open here and is collected in section 9.

**Division of labour.** This file owns spelling, ownership and the shape of cross-document contracts (who emits, who implements, which class, which state names). The owning spec owns meaning and behaviour (transitions, guards, grants, numbers). The behaviour of the `obop` CLI (flags, output and exit contracts, the catalogue by area, the plugin registry client) is owned by [spec/cli.md](spec/cli.md); this file owns only the spellings listed in 2.1 and 2.4. A non-owner cites a name and never redefines it.

**Changing a name.**

1. Edit this file first: the registry row and the rename-map row (kind, from, to, documents affected).
2. Then edit every document listed, in the same change or before the next release. A document never carries a name this file does not list.
3. A change that touches a DECISIONS record goes to the owner (section 9); the name stays as listed until the owner answers.
4. A retired spelling may appear only in the rename map and in a one-line "retired" note that links to it.
5. When the generated event catalogue of ARCHITECTURE 4.3 exists, section 3.4 becomes a link; the rules in 3.1 to 3.3 stay.

## 2. Naming rules

### 2.1 Forms by artefact

| Artefact | Form | Examples and notes |
|---|---|---|
| Module id | Lower snake_case; ids of DECISIONS 5.1. A bridge joins two module ids with `_`, the extended flow first | `sales`, `stock_account`, `sale_stock`, `l10n_de`. The module is `sales`; `sale_` is the Odoo-style family prefix of its bridges and entities (`sale_order`), kept |
| Crate, package | Kebab-case, `obop-` prefix; API crate `<module-id>-api` with `_` turned into `-` | `obop-sales`, `sales-api` (not `sale-api`), `l10n-de`. The id of a distributable package is the plugin id below |
| Plugin id (package id) | `publisher.name`: a lower-case publisher slug, a dot, a kebab-case name. `obop.*` is reserved for trust level `core` (D32). "Plugin" is the owner's word (OR-3) for a package of the registry and is used in commands and user copy; architecture and data-model text say "package" (DECISIONS section 4) | `acme.payment-reminders`, `sven.dach-quote-terms-plus`. Not an external id (`module.name`, snake_case) and not a module id: a package may ship no module or several. Whether first-party packs are addressed in full (`obop.l10n-de`) or by the bare name that cli 3.3 writes (`l10n-de`) is open (section 9.1) |
| Trust level | Attribute `trust_level`, values `private`, `community`, `verified`, `core` (lower snake_case). The registry tiers are `community` (automated gates only) and `verified` (a verified publisher plus automated gates: signatures, SBOM, capability manifest; OR-17); the registry lists only plugins with an OSI-approved licence, in either tier (OR-17). `private` (business-own: authored, copied or sideloaded) and `core` (first-party) are never listed in the registry (D81) | `reviewed` and `partner` are retired (OR-2, OR-3; section 8.2). cli 4 spells the attribute `tier` in `obop.lock` and `--tier`; one spelling is open (section 9.1) |
| Plugin modes and commands | Subject `plugin` (a reserved platform noun, 2.4); the two modes are `install` and `copy`; actions `search`, `info`, `install` (documented alias `add`), `copy`, `fork`, `update`, `remove`, `verify`, `audit`, `diff`, `publish` and the local developer actions of cli 3.10 | Wire ids `plugin.install`, `plugin.copy`. `install` fetches a signed, version-pinned package; `copy` puts its source in the user's instance as business-own code. The listing rule (an OSI-approved licence, OR-17) keeps `copy` legally clean: a closed plugin can be sideloaded but is never listed. `fork` re-identifies a copy under the forker's namespace and is not a synonym for `copy`. There is no `obop ext` (section 8) |
| Provenance of a copy | No separate file. The record (source, version, digest, SPDX licence, signature check at copy time, date) is the `[[plugin]]` entry with `mode = "copy"` and its `[plugin.upstream]` table in `obop.lock`, and the `[upstream]` table of the copied tree's `obop.toml` | cli 4.2 and 4.4. A copy's trust level is `private` and copying raises no privilege. The record's name and content are Q25 (section 9.1) |
| Model, table, API resource | Singular snake_case, unique in a business. A child entity is the parent's full name plus the part. A generic noun (`team`, `stage`, `plan`) stays with its current owner (`plan` is the subscription billing shape of sales-crm 4.5, not a pricing plan of obop); other modules prefix (`helpdesk_team`, `ticket_stage`) | `sale_order`, `sale_order_line`, `journal_entry`. Odoo-dotted names (`sale.order`, `account.move`) appear only as Odoo source names in importer mappings and parity columns. Model id, table name and `/api/v1/{model}` segment are the same string |
| Column, field | Snake_case. Foreign key `<entity>_id` with the full entity name (`service_contract_id`, never bare `contract_id`). Money `*_minor` plus `currency_code`; timestamps `*_at`; dates `*_date` or `date`; ranges `*_during`; percent `*_pct`, basis points `*_bp`; booleans `is_*` or `has_*` (existing state flags such as `locked` and `gapless` stay). Columns a module adds to another module's model are `<module_id>__<field>`; business columns `x_<slug>` | `row_version`, `command_id` (one command invocation, D9), `idempotency_key` (a natural dedupe key on a row: job, booking, link). The HTTP header is `Idempotency-Key` |
| Derived progress field | `*_status` for progress along a document flow, `*_state` for a condition | As ARCHITECTURE 5.7 and the owning specs: `delivery_status`, `invoice_status`, `payment_state`, `credit_state`, `approval_state`. No renames |
| State and enum values | Lower snake_case; no spaces, hyphens or capitals; the spelling is `cancelled` (kernel lifecycle, D28). External vocabularies mirrored verbatim stay (iCalendar PARTSTAT, Odoo-mirrored `mailing_trace` statuses, Stripe types inside provider payloads) | `past_due`, `nothing_to_invoice`, `in_progress`. Diagram labels follow the stored values |
| External id | `module.name` (D9, D22) | `sales.order_form` is a view's external id (module `sales`, name `order_form`), not a model |
| Command | Rust type `<Verb><Noun>`; wire id `<model>.<action>`, both derived from one definition. The CLI line, the REST path and the MCP tool name are generated from the same catalogue entry (2.4). An operational command that no model backs takes a reserved platform noun as subject | `ConfirmSaleOrder`, `sale_order.confirm`, `obop sale_order confirm`. Not `sales_order.confirm`. Operational: `plugin.install`, `backup.create`, `upgrade.apply` |
| Event type | PascalCase, section 3.1. Webhook and MCP type: `<aggregate_type>.<past participle>`, generated | `SaleOrderConfirmed`, `sale_order.confirmed`. Opt-in record events `<model>.created`, `.updated`, `.deleted` have no PascalCase twin |
| Extension point | `Before<Verb><Noun>`; a port with a chain of implementers, never an event | `BeforeInvoicePost`, `BeforeTransferValidate` |
| Port, trait | PascalCase. Hub and module ports end in `Port`; provider traits end in the role (`Provider`, `Transport`, `Source`, `Engine`, `Driver`, `Adapter`, `Feed`, `Format`). Call syntax `Port::method` | Section 7. An unnamed "the carrier trait" is wrong: write `CarrierProvider` |
| Permission group | `<module>.<role>`, lower snake_case (section 4). Named action `<module>.<action>`. Standard operations: `read`, `write`, `create`, `delete`, `export`. Cross-app capability rows are bare snake_case | `account.manager`, `stock.validate`, `import_data` |
| API scope | `<model>:<operation>` | `invoice:create_draft`, `party:read` |
| Data class | Lower snake_case, section 5 | `confidential_hr` |
| Account role | Lower snake_case; registry in finance 4.1.1 | `stock_valuation`, `grni`, `cogs` |
| Problem code | Lower snake_case RFC 9457 `type` slug. The CLI prints it unchanged as `error.code` and derives a coarse exit code from it (2.4) | `idempotency_key_reuse`, `field_not_readable`, `plan_stale` |
| Setting, config | Setting key `<module>.<key>`; database setting `obop.company_ids`; manifest keys as ARCHITECTURE 9.3; environment variables `OBOP_<AREA>_<NAME>` with `_FILE` for secret files (Proposed). The client areas `CLI` and `AUTH` are listed in 2.4. If the hosted sandbox (D83) needs its own settings, the Proposed reservation is the server area `OBOP_SANDBOX_<NAME>` (for example a session time to live) and a client profile named `sandbox`; no other document uses either yet (section 9.1) | `stock.lock_date`, `obop.company_ids` |
| Decision ids | As the identifier-namespace table of [DECISIONS.md](DECISIONS.md) section 4: `D1` to `D86`, `Q1` to `Q36`, constraints `F1` to `F6`, and owner rulings `OR-1` to `OR-21` (written `OR-4`, never "ruling 4"; a ruled statement cites its OR number, a Proposed one "Proposed (Dn)") | A family that shares letters with another is cited with its document or qualifier ("UI tier F2", "ENGINEERING Q-E2", "finance open question 14", "retail RP11"). Q18 (keep and qualify, or rename) is open; this file keeps and qualifies |

### 2.2 Vocabulary and spelling

- Terms and forbidden words are those of DECISIONS section 4. Additionally: "transfer", not "picking", for the document (`stock_picking_type` and the physical act of picking keep the word); "approvals engine" (D39) for the engine and "approval policy engine" (D18) for its auto, confirm and `approve_by_role` modes; "hub" never alone (hub port, store hub); "timesheet entry", not "timesheet line"; "permission group", never "group" or "role" alone.
- "Class" always carries a qualifier: data class (section 5), retention class, effect class (`read`, `draft`, `commit`, `external`, `money_and_payee`, `destructive`, ai.md 4.3) or `cost_class`. A locked order (`locked` flag), a lock date (`lock_date`) and a stock lock date (a `stock` company setting, not a ledger `lock_date` kind) are three different things.
- Prose is British (licence, organisation, authorised). Identifiers are US English (`authorized`, `fulfillment`, `color`), except `cancelled` and `Cancelled`, which D28 fixes. Modes are `approve_by_role` in code and "approve-by-role" in prose.
- Prose and code: an identifier is always in a code span, spelled as registered. The business thing in prose is lower-case English ("sales order", "quote", "vendor bill", "credit note"); "quote" is a `sale_order` with `order_kind = quote`, and new text prefers it to "quotation". A table or event is never named by its Odoo twin outside parity columns.
- A golden path is cited with its document ("sec-priv GP-2"); the hyphen is optional in local use. The golden paths of cli are lettered and cited by section ("cli 8.2").
- A spec-local marker family (RP, AZ, AD, C, OQ, and the naming rules N1 to N8 of cli 2.1) is never cited without its document.
- Words the owner rulings added (DECISIONS section 4). "Plugin" is the user-facing word for a registry package (OR-3). "Install" and "copy" are the two modes of a plugin; "fork" is a separate CLI action, not a third mode. "Trust level" is the attribute; "tier" names only the two registry values `community` and `verified` (the sense of OR-3) and the unrelated families T0 to T3 (module tiers, Proposed), F0 to F3 (UI tiers), X1 to X5 (code tiers) and L1 to L3 (localisation pack tiers); the X and L labels are RATIFIED (OR-14). "Typed command" is one entry of the command catalogue, reachable as CLI, REST and MCP (D80); "command" alone still means a business command, and "endpoint" or "RPC" is not a synonym. "Parity" is the definition of done of D80 (a command exists in CLI, API and MCP, and in the UI before the first stable release, except commands on the exemption list, OR-16), never "feature-complete". "Agent" is a principal, never a "bot" (D28). "Publisher" authors a package (SpaceCorps for obop itself, OR-10); "steward" is reserved for a foundation that may one day replace it; "partner" is a third party that implements, hosts or localises obop, with no programme, fee or revenue share (OR-2).
- Words the second batch of rulings added (OR-14 to OR-21; DECISIONS section 4). The **sandbox** is the free, ephemeral, hosted obop that SpaceCorps runs from the public beta, with sample data only (OR-20, D83). It is not a test copy of a business, which is a **business clone** (`obop business clone`, cli 3), and not the optional local demo, which is a seeded instance on the user's machine (`obop dev up`). The **exemption list** is the owner-approved list of operator-only commands that lack a UI (CLI-only), each with a written reason (OR-16, D80); the entries proposed in cli 1.1 are proposals until the owner approves them one by one, so "exempt" for an unapproved command means a proposed entry, and a UI-only step is not an exemption class.
- There is no pricing plan, edition or tier of obop (OR-2), so "plan" never names one. In this file `plan` is a command plan (`command_plan`, ai 4.3, with its `plan_id` and the problem code `plan_stale`), a plan file (`*.plan.json`, cli 2.7), an onboarding plan (`PlanCompleted`, people) or the subscription billing shape a business sells to its own customers (`plan`, sales-crm 4.5). The modules `pricing` and `subscriptions` and the groups `pricing.manager` and `subscriptions.*` serve a business's own price lists and its customers' subscriptions, not obop's. No event, group, scope or state in this file is named after a plan, price or edition.

### 2.3 Where a name is defined

An event type lives in the emitting module's `*-api` crate; a port in the implementer's `*-api` crate; an entity in the owning spec's entity table; a permission group in the owning spec, and the name is added to the seed list of sec-priv 2 when the spec introduces it. A command is defined once, as a catalogue entry generated from the owning spec's registry metadata (D80); its CLI, REST and MCP spellings (2.4) are derived from it and never written by hand. A plugin or registry name lives in platform 4.18 (the installed entity) and cli 4 (the commands and the lock file). The owner column of each registry below is that document.

### 2.4 CLI grammar and naming (Proposed until Q22)

The `obop` CLI, the REST API and MCP are three surfaces of one command catalogue (OR-4, D80). This section fixes the spelling they share, so a name is written once. [spec/cli.md](spec/cli.md) owns the behaviour and restates nothing here; where the two differ on a spelling, this file is corrected first (section 1). The whole section is Proposed until the owner answers Q22 (binary name, grammar, and the freeze point (before the 0.3 public alpha, OR-31)); no name in it is frozen earlier. The rows follow the naming rules N1 to N8 that cli 2.1 proposes for this file.

| Element | Form | Notes |
|---|---|---|
| Binary | `obop` | The same binary is the server (D37), so the CLI and the server share one version. The name follows the clearance of Q2 (OR-5) |
| Command line | `obop <subject> <action> [<id>] [options]` | The subject is a registry model id verbatim (snake_case) or a reserved platform noun. A positional `<id>` is a UUIDv7 or an external id `module.name`. Help text is the registry `help` string of the model, field, action or error code |
| Wire id, REST path, MCP tool | `<subject>.<action>`; `/api/v1/{subject}/[{id}/]{action}`; the MCP tool name is the wire id | One definition, three spellings: `obop sale_order confirm`, `sale_order.confirm`, `POST /api/v1/sale_order/{id}/confirm`. Custom fields, custom models and installed plugins appear in all three without a new binary |
| Subject spelling | Snake_case, as the model id. A hyphenated spelling (`sale-order`) is accepted on input and never printed | cli N1. Whether kebab-case becomes the canonical printed form is part of Q22 (cli open question 1); the default keeps the CLI, the wire id and the tool name the same string |
| Reserved words | The first word of every command in cli 3 that no model backs, for example `init`, `serve`, `backup`, `upgrade`, `business`, `pack`, `plugin`, `agent`, `data`, `dev`, `self`, `diag`, `telemetry` | A model id cannot equal one; the registry refuses the colliding id and an `xtask` lint checks it (cli N2). Q22 names `plugin`, `upgrade`, `diag`, `dev` and `telemetry` |
| CLI alias | At most one short alias per model, declared in registry metadata and published in the catalogue; `plan` for `command_plan` and `approval` for `approval_request` | cli N3. An alias is a CLI spelling only; the model id, table name and wire id keep the full name (Proposed). **Known collision:** `plan` is also the model id of the subscription billing shape (sales-crm 4.5, W3), and a model id and an alias cannot share a spelling (cli N2). By the ownership rule of 2.1 the model keeps `plan`, so the alias `plan` for `command_plan` is to be dropped or renamed before W3; cli, sales-crm and Q22 settle which (section 9.1). The wire id `command_plan.commit` works either way |
| Platform verbs | `plugin search\|info\|install\|copy\|fork\|update\|remove\|publish\|verify\|audit\|diff`, `pack list\|status\|install\|update\|verify`, `upgrade`, `migrate`, `diag`, `dev up\|down\|reset\|logs\|parity\|new`, `telemetry preview\|enable\|disable\|status`, `backup create\|verify\|list\|restore`, `business create\|list\|get\|clone\|export\|verify\|import\|delete`, `data export\|import`, `self update\|verify`, `agent create\|list\|suspend\|revoke\|guide` | The full set is cli 3 and it is Proposed (Q22). Built today (cli 1.4): top-level `restore` (it loads a backup, with the stable statuses 3, 4 and 5), `import MODEL FILE.csv` (D70), `backup`, `backup verify|run|list|prune`, `migrate`, `drill`, `modules` and `update`; those spellings stay as aliases if the proposed ones are adopted. Proposed: no top-level `restore`, `export` or `import` beyond the aliases: `business import` loads an export archive, full export is `business export`, model data is `data export` and `data import`, and `backup restore` restores a backup. There is no `obop ext` (section 8) |
| Global flags | Kebab-case: `--profile`, `--business`, `--company`, `--format` (with `--json` for `--format json`), `--fields`, `--dry-run`, `--plan`, `--yes`, `--no-input`, `--idempotency-key`, `--wait`, `--no-wait`, `--timeout`, `--retry`, `--api-version`, `--quiet`, `--verbose`, `--color`, `--strict` | A plan is referenced by `--plan <id\|file>`, by `obop plan commit <id>`, and as `plan_id` in JSON; there is no `--plan-id`. The idempotency key is `--idempotency-key` on the command line and `Idempotency-Key` on the wire (2.1) |
| Typed flags | The kebab-case of input property names: `--partner-id`. `--set name=value` and `--body` use the exact property names | cli N4. A secret is never an argument: `_FILE` variables, `--*-file` options or standard input (cli 2.12) |
| Output | Standard output carries data only. In JSON mode one envelope with sorted snake_case keys: `command` (the wire id), `data`, `meta`, `ok`, `warnings`; a failure adds `error` and `exit`. `data` is the server body verbatim, so the output schema is the API schema. `meta` holds `business`, `duration_ms`, `obop_version`, `request_id`, `trace_id` and `replayed` | Money is `*_minor` as a string plus `currency_code` (3.3). `table` output is not a contract and `ndjson` has no envelope (cli 2.3) |
| Exit codes | 0 `ok`, 1 `internal`, 2 `usage`, 6 `conflict`, 7 `refused`, 9 `unreachable`, 10 `approval`, 11 `partial`, 12 `auth`, 13 `denied`, 14 `invalid`, 75 `busy`, 130 `interrupted` (Proposed, cli 2.4; 3, 4, 5 and 75 are already used by the built backup, restore and call commands: `restore_key_required`, `backup_not_sealed`, `restore_files_failed` and "try again later") | A coarse class; `error.code` stays the RFC 9457 slug unchanged (2.1, Problem code). Adding a code is a minor change and repurposing one a major change (cli 2.4 and 6) |
| Environment | `OBOP_CLI_<FLAG>` for a flag (`OBOP_CLI_PROFILE`, `OBOP_CLI_FORMAT`, `OBOP_CLI_NO_INPUT`); credentials `OBOP_AUTH_API_KEY_FILE`, `OBOP_AUTH_CLIENT_ID`, `OBOP_AUTH_CLIENT_SECRET_FILE`. `NO_COLOR`, `CI` and `TRACEPARENT` are honoured | cli N7 and 2.10. Precedence: flag, variable, profile, default. Server variables keep `OBOP_<AREA>_<NAME>` (2.1) |
| Files | `obop.toml` (the package manifest of every kind, D32; keys in ARCHITECTURE 9.3), `obop.lock` (what one business installed, written by `plugin install\|copy\|update\|remove`), `config.toml` (client profiles, never secrets), `*.plan.json` (a plan for a command that runs without a server), `AGENTS.md` and `skills/` inside a plugin | cli N7, 2.7, 4.2 and 5.4. The database row `package_install` is authoritative at run time; the lock file is its reviewable projection |
| Generated artefacts | Shell completions (`obop completion bash\|zsh\|fish\|powershell\|elvish`), man pages (`obop man`), `parity.json`, `ui-bindings.json`, `deprecations.json` and `compat.json` | Built from the catalogue; `obop dev parity` prints the parity matrix (cli 6). The first-party CLI is the OAuth public client `obop-cli` at trust level `core` (cli 2.11) |
| Parity | A command exists in CLI, REST and MCP with the same input and output schemas, and has a UI binding before the first stable release | The definition of done (D80); UI parity is literal for every command of every shipped module (OR-16). A surface may expose only what the catalogue holds. A command lacks a surface only as an entry of the exemption list: CLI-only by owner-approved exemption-list entry (OR-16; the bootstrap and local entries proposed in cli 1.1 are proposals until the owner approves each one). Proposed names for the record: the catalogue attribute `exemption` with `reason`, `surfaces_lacked` (at least `ui`), `approved_by` and `approved_on`, which `obop schema` shows as the field `ui.exempt` (null for a command with a UI binding; cli 2.9). A UI-only step is not an exemption class |

### 2.5 Module ids in use

These are the ids that prefix permission groups, crates and events (DECISIONS 5.1 and the specs; names marked Proposed there stay Proposed). Kernel and platform: `base`, `product`, `mail`, `portal`, `analytic`, `automation`, `pricing`, `analytics`, `customize`, `resource`, `calendar`, `discuss`, `documents`, `knowledge`, `sign`, `todo`, `ai`. Finance: `account`, `tax`, `invoicing`, `payment`, `bank`, `edi`, `l10n`, `l10n_de`, `l10n_fr`, `l10n_pl`, `assets`, `deferral`, `budget`, `expenses`, `intercompany`, `consolidation`. Commercial: `crm`, `sales`, `subscriptions`, `rental`, `commissions`. Supply chain: `purchase`, `stock`, `stock_account`, `delivery`, `mrp`, `quality`, `maintenance`, `plm`, `repair`. Retail: `pos`, `loyalty`, `ecommerce`, `website`, `marketing`, `livechat`. People: `hr`, `recruitment`, `attendances`, `payroll`, `appraisals`. Services: `project`, `timesheet`, `helpdesk`, `planning`, `appointments`, `fleet`, `service_contract`, `mileage` (Proposed, D87). Bridges: `sale_stock`, `purchase_stock`, `sale_project`, `sale_timesheet`, `sale_payment`, `sale_mrp`, `purchase_mrp`, `mrp_account`, `pos_stock`, `pos_account`, `rental_stock`, `subscription_stock`, `expenses_payroll`, `expenses_sales`, `attendances_time_off`, `attendances_payroll`, `recruitment_website`, `hr_resource`. A bridge depends on the `*-api` crates of both modules (R5) and is specified in one spec (7.2).

## 3. Event catalogue

### 3.1 Form of an event name

1. PascalCase `<Aggregate><Fact>`. The aggregate is the registered entity (`TransferDone` is `stock_transfer`) and is never dropped or shortened to a word another module also uses (`ContractActivated`, `ServiceDue` and `AllowanceExhausted` are not acceptable). The fact is a past participle or the adjective of the state entered (`Done`, `Due`, `Overdue`, `Expiring`); a bare noun (`SlaWarning`) is not a fact. "Requested" is allowed for an ask that starts a process (`ReturnRequested`).
2. One name per fact. Where two specs named one fact, the registry below keeps one and section 8 lists the other as retired.
3. Only the owner of the aggregate emits it. A bridge implements handlers and calls ports; it never emits in another module's namespace. A consumer never redefines an event.
4. A command is not an event (`ValidateTransfer` is a command of `stock`), and an extension point is not an event (3.6).
5. Security-log types follow the same form (`ElevationUsed`, `DataRead`) and live in `security_event`; realtime frames (`RecordChanged`, `JobProgressed`) are messages, not events.

### 3.2 Delivery kinds

An event has one name and one or both of two delivery mechanisms. **T**: handlers run in the publisher's transaction and an `Err` aborts the command; use it only for an atomic invariant across modules. **D**: an outbox row with one delivery per subscriber, after commit, at least once, deduplicated by `inbox_processed`. **T+D**: both. **SE**: a security-log event in the append-only `security_event`; **SE+D** is also published for subscribers. A new event is D unless a named T handler needs atomicity. A consumer's handler is listed only by the kind it uses.

### 3.3 Envelope

Every outbox event carries `event_id`, `type`, `schema_ver`, `company_id`, `aggregate_type`, `aggregate_id`, `aggregate_version`, `occurred_at`, `trace_id`, `command_id` of the causing command (Proposed) and `data` (ARCHITECTURE 6.6). Payload columns below are the key data members only. Payloads are thin: ids, versions and the few facts a handler needs; a webhook payload is `id`, `type`, `data.object_id`, `api_version`, with an opt-in snapshot (integrations 4.3). Amounts travel as JSON strings in minor units.

### 3.4 Cross-document events

R23 marks names from R23 D.2 to D.4. Waves follow the placement of the emitting module in DECISIONS 5.1 after the W1 cut (D86, OR-13, OR-15): a module that the cut moved out of W1 carries its events with it. The wave windows are RATIFIED (OR-13), and so are the in and out calls of the W1 cut (OR-15), including the move of the services variant to W3; the landing wave of every other displaced item and each remaining placement below is Proposed (D25, D86). All other names are Adopted here and Proposed until the generated catalogue replaces this table.

| Event | Kind | Emitter (owner spec) | Named consumers | Payload | Wave |
|---|---|---|---|---|---|
| `RegistryVersionChanged` | D | registry (ARCHITECTURE 5.1) | ux, ai, integrations (OpenAPI cache), platform search, the CLI command-tree cache (cli 2.1), peer nodes | `registry_version` | W0 |
| `PermissionChanged` | D | base authorization (sec-priv 4.2) | ai, knowledge, ux, access cache | `kind` (membership, permission_group, access), ids | W0 |
| `PrincipalInvited`, `PrincipalDeactivated`, `PrincipalSuspended` | D | base principal management (sec-priv 4.1; platform 4.4 shares) | ai (delegations end), people, approvals, notifications, knowledge, sessions, share grants | `principal_id`, `kind` (any principal kind, agents included) | W0 |
| `AgentDelegationRevoked` | D | base (ai 4.2 names it; sec-priv 4.1 adds it) | ai | delegation id | W1 |
| `CompanyCreated` | D | base companies (platform 4.3) | packs (seed templates) | company id | W1 |
| `PartyUpdated` | T+D | base contacts (platform 4.2) | T: contact persons (commercial fields propagate). D: mail, pos snapshots | party id, changed fields | W0 |
| `PartyMerged` | D | base contacts | crm, calendar, documents | winner, losers | W0 |
| `PartyBankAccountChanged` | D | base contacts (platform 4.2; Proposed) | account (approval, payment block), ai anomaly rules | party id, bank account id, change kind | W1 |
| `PartyErased` | D | erasure job (sec-priv 4.6) | search, mail, documents, ai, calendar, chatter, sales, crm, subscriptions, people, services, website, marketing, every app with a contact snapshot | subject party id | W1 |
| `ConsentRecorded`, `ConsentWithdrawn` | D | consent ledger behind `ConsentPort` (sec-priv 4.6) | mail, marketing (suppression on withdrawal) | `consent_record` id, purpose | W1 |
| `ImportRunCommitted`, `ImportRunRolledBack` | D | import engine (platform 4.16) | ux, integrations, party duplicate scan, foreign importers | run id | W1 |
| `ExportRequested` | SE | authorization (sec-priv 4.2) | audit | principal, scope, model | W1 |
| `ExportReady` | D | `export_job` (integrations 4.9) | ux | job id | W1 |
| `MailReceived` | D | mail (platform 4.7) | chatter, documents, automation, ai extraction | mail id, alias, attachment refs | W2 (the inbound gateway leaves W1, D86 slip item 11) |
| `MailBounced` | D | mail | contacts, marketing | address, kind | W2 (with the inbound gateway) |
| `AttachmentCommitted` | D | base attachments (platform 4.1; Proposed), when a blob goes `pending` to `committed` | ai extraction (ai 4.6) | attachment id | W1 |
| `MessagePosted`, `ActivityScheduled` | D | mail (platform 4.5) | NotifyPort, calendar | thread ref | W1 |
| `NotificationCreated` | D | mail notifications (platform 4.6) | ux (Inbox badge) | notification id | W1 |
| `CalendarEventChanged`, `CalendarEventCancelled` | D | calendar (platform 4.8) | appointments | event ref | W3 (`calendar` ships with the services variant, D86) |
| `ApprovalRequested`, `ApprovalTaskDecided`, `ApprovalDecided` | D | approvals engine (platform 4.12) | NotifyPort, ux; `ApprovalDecided`: ai (releases plan), sign, automation, hr | request id, decision, `plan_digest` | W2 |
| `SignRequestCompleted` | D | sign (platform 4.11) | documents, hr, recruitment, onboarding | request id | W3 |
| `ModuleInstalled`, `ModuleUpgraded`, `ModuleUninstalled` | T | base apps management (platform 4.18) | registry rebuild | module, version | W0 |
| `PackageInstalled`, `PackageCopied` | D | base apps management (platform 4.18; Proposed), aggregate `package_install`; on `plugin install` (a sideload included) and on `plugin copy` (cli 4.3) | ux (installed-plugin list), `obop agent guide` (skills of installed plugins), webhooks | `package_id`, `version`, `digest`, `trust_level`, `capability_digest`; `PackageInstalled` adds `sideloaded` | `PackageInstalled` W1 (sideload, cli 4.3); `PackageCopied` W2 (registry stage 1, D81) |
| `PackageRevoked` | D | base apps management (platform 4.18; Proposed), when the server applies an index revocation (the D32 kill switch, cli 4.7) to an installed version, which goes to `disabled` | NotifyPort (holders of `api.extension_admin`), ux, webhooks | `package_id`, `version`, `reason_class`, advisory link | W2 |
| `CredentialAdded`, `CredentialRemoved` | SE+D | base identity (sec-priv 4.1) | mail (user notice) | principal, credential kind | W1 |
| `OAuthConsentGranted`, `OAuthConsentRevoked` | SE | OAuth provider (integrations 4.2) | audit | client, scopes, grant id | W1 |
| `PackActivated` | D | base apps management (platform 4.18; Proposed) | hr, payroll, time off, ux | pack id, version, kind, company | W1 |
| `LanguageActivated` | D | i18n (platform 4.19) | catalogue loader | language | W1 |
| `FxRatesUpdated` | D | base, via `ExchangeRateSource` (finance 4.7; platform 4.3, Proposed) | account (revaluation hint), pricing, analytics, mail | rate type, date, currencies | W1 |
| `ProductChanged`, `VariantCreated` | D | product (sales-crm 4.1) | pos snapshots, website, shop feeds | ids | W1 |
| `PriceRuleChanged` | D | pricing (sales-crm 4.2) | pos snapshot deltas, website tag purge, shop cache | pricelist, rule | W2 (price lists and rules leave W1, D86 slip item 10; W1 keeps the list price and the line discount) |
| `QuoteSent` | D | sales (sales-crm 4.4) | mail, portal, crm timeline, webhooks | order id, link | W1 |
| `QuoteViewed`, `QuoteDeclined`, `QuoteAccepted` | D | portal (sales-crm 4.8) | sales (`quote_status`), crm; `QuoteAccepted`: webhooks | order id, document hash | W2 (`portal` leaves W1, D86 slip item 8) |
| `SaleOrderConfirmed` (R23) | T+D | sales (sales-crm 4.4) | T: sale_stock (`DemandPort::raise`), crm, sale_project, loyalty, ecommerce. D: subscriptions, rental, commissions, marketing, planning (Proposed), connectors, webhooks, mail, analytics, AI index | order id, lines, untaxed total, currency, opportunity, team, channel | W1 |
| `SaleOrderCancelled` | T+D | sales | sale_stock (cancels open deliveries), sale_project (detaches billing), portal (revokes links) | order id | W1 |
| `LeadCreated`, `LeadConverted`, `LeadsMerged`, `OpportunityStageChanged`, `OpportunityWon`, `OpportunityLost` | D | crm (sales-crm 4.3) | webhooks, timeline, analytics | lead id, stage | W1 |
| `SubscriptionStarted`, `SubscriptionInvoiceDue`, `SubscriptionChanged`, `SubscriptionStatusChanged`, `SubscriptionCancelled`, `DunningStepExecuted`, `MrrMovementRecorded` | D | subscriptions (sales-crm 4.5) | invoicing via `InvoiceableSource` (`SubscriptionInvoiceDue`), service_contract (`SubscriptionChanged`, `SubscriptionStatusChanged`), analytics | subscription id, period, status | W3 |
| `CommissionPeriodClosed` | D | commissions (sales-crm 4.7) | people payroll export, account (draft bill for external agents) | period, statements | W4 |
| `InvoicePosted` (R23) | T+D | invoicing (finance 4.2) | T: sales (`qty_invoiced`), sale_timesheet (locks entries). D: edi, portal, mail, documents (archive), intercompany, assets, deferral, subscriptions, commissions, purchase (vendor bills), pos (attach), webhooks (`invoice.posted`), ai anomaly, analytics | invoice id, number, `direction`, `kind`, party, currency, totals, lines with source refs | W1 |
| `InvoiceReversed` | T+D | invoicing (finance 4.2) | T: sales (quantities). D: commissions (clawback), loyalty (reverse points), stock_account (to confirm, finance 5.2 posts return COGS at `BeforeInvoicePost`), webhooks, ai anomaly | original invoice id, reversing invoice id, `kind` (credit_note, full_reversal), lines with qty, partial or full | W1 |
| `PaymentTransactionDone` | D | payment (finance 4.5) | sales via `sale_payment` (prepayment, Proposed), subscriptions, ecommerce (confirms web order), appointments (deposit), webhooks | transaction id, order ref, amount | W2 (payment providers beyond manual registration leave W1, D86 slip item 9) |
| `PaymentTransactionFailed` | D | payment | subscriptions (dunning), sales, webhooks | transaction id, reason | W2 |
| `PaymentPosted`, `PaymentVoided` | D | payment | invoicing (`payment_state`), people, webhooks; `PaymentVoided` also sales | payment id, invoice refs | W1 |
| `PaymentReconciled` (R23) | D | payment | invoicing, sales (order `payment_state`), people (claim reimbursed), payroll (run paid), subscriptions, commissions, ecommerce (payment state), ai anomaly, webhooks (`payment.reconciled`) | payment id, statement line | W1 |
| `BankStatementImported` | D | bank (finance 4.5) | suggestion job | statement id | W1 |
| `EntryPosted`, `LockDateChanged` | D | account (finance 4.3) | analytics, automation, audit | entry id or lock kind | W1 |
| `TaxClosingPosted` | D | tax (finance 4.4; not account) | analytics, automation, audit | period, entry id | W1 |
| `TaxRuleChanged` | D | tax (finance 4.4; Proposed; the alternative is retail open question 17) | pos snapshot builder | rule or version id | W3 |
| `EDocumentStatusChanged` | D | edi (finance 4.6) | invoicing (badge), mail, webhooks | e_document id, status | W1 |
| `AssetDepreciationPosted`, `DeferralReleased`, `IntercompanyMirrorCreated` | D | assets, deferral, intercompany (finance 4.9, 4.10, 4.12) | analytics, mail, approvals | schedule line or link id | W3 |
| `TransferDone` (R23) | T+D | stock (supply-chain 4.1.5) | T: sale_stock and purchase_stock (`QtyContributionPort`), stock_account (`PostingPort`), rental (Proposed). D: pos, ecommerce and channel pushers, mrp, webhooks, analytics, AI index, reservation sweep | transfer id, moves, lots, source refs | W1 |
| `TransferConfirmed`, `MoveReserved`, `ReservationLost`, `NegativeStockCreated` | T | stock | quality, chatter, activities | transfer or move id | W1 |
| `StockLevelChanged` | D, coalesced | stock | ecommerce (availability), channel sync, website cache tags, webhooks, analytics, AI index | product, location, quantity | W1 |
| `PurchaseOrderConfirmed` | T+D | purchase (supply-chain 4.2) | T: purchase_stock. D: mail, webhooks | order id | W2 (purchase orders leave W1, D86 slip item 13; W1 keeps vendor bills) |
| `ShipmentLabelCreated`, `ShipmentTrackingUpdated`, `ShipmentDelivered` | D | delivery (supply-chain 4.9) | customer email, order tracking, channel sync out | shipment id | W3 |
| `ProductionConfirmed`, `WorkOrderStarted` | T | mrp (supply-chain 4.3) | quality; `ProductionConfirmed`: plm | order id | W3 |
| `PosOrderPaid`, `PosOrderRefunded` | T | pos (retail 4.1) | loyalty, pos_stock (pending quantity) | order id, lines | W3 |
| `PosSessionClosed` (R23) | T | pos | pos_stock (`DemandPort`), pos_account (`PostingPort`) | session id, totals by method and tax group | W3 |
| `PosInvoiceRequested` | D | pos | invoicing via `InvoiceableSource` | order id | W3 |
| `PosExceptionRaised`, `ChannelOrderImported`, `ChannelSyncFailed` | D | pos, ecommerce (retail 4.1, 4.3) | notifications; the channel pair also sales | exception type, channel order | W3 |
| `LoyaltyCardChanged`, `LoyaltyOverdraftRecorded` | D | loyalty (retail 4.2) | pos edge snapshot deltas; `LoyaltyOverdraftRecorded`: pos exception | card id, balance | W3 |
| `CartAbandoned`, `CartCheckoutStarted` | D | ecommerce (retail 4.3) | marketing, automation | cart (sale_order) id | W3 |
| `ReturnRequested`, `WithdrawalRequested` | D | ecommerce | stock, invoicing | return_request id | W3 |
| `FormSubmitted` | D | website (retail 4.4) | marketing, crm, helpdesk | form, submission id | W3 |
| `EmployeeCreated`, `EmployeeVersionChanged`, `EmployeeDeparted` | D | hr (people 4.1) | onboarding, recruitment, time off, attendances, payroll, expenses, skills, appraisals | employee id, effective date, changed classes | W3 (the employee core ships with `timesheet` in the services variant, D86) |
| `LeaveApproved`, `LeaveCancelled` | D | hr time off (people 4.2) | resource via `hr_resource` (`ResourceLeavePort`), payroll, attendances, calendar | leave id, dates | W3 |
| `AttendanceClosed`, `OvertimeApproved` | D | attendances (people 4.3) | payroll, time off | attendance id | W3 |
| `ExpenseClaimApproved` | D | expenses (people 4.5) | finance side of expenses (draft bill, unique source ref), payroll via `expenses_payroll` | claim id, lines, employee party | W3 |
| `PayRunConfirmed`, `PayRunPosted` | D | payroll (people 4.10) | expenses (`PayRunConfirmed`); analytics | run id, period | W3 |
| `PayslipConfirmed` | D | payroll | portal and notifications (payslip ready); no account consumer, payroll posts itself through `PostingPort` | payslip id | W3 |
| `ResourceCalendarChanged` | D | resource (services 4.1) | helpdesk (SLA recompute), project (reschedule) | calendar id | W3 (`resource` is a W3 T0 module, D86) |
| `LeaveRecorded`, `LeaveRemoved` | D | resource, written by `hr_resource` | planning, appointments, helpdesk, project, timesheet | resource, interval | W3 |
| `TaskStateChanged`, `MilestoneReached` | T | project (services 4.2) | project (blockers), timesheet (timer); `MilestoneReached`: sale_project | task or milestone id | W3 (the services variant, D86) |
| `TimesheetEntryRecorded`, `TimesheetChanged` | T | timesheet (services 4.3) | sale_timesheet (`QtyContributionPort`) | entry id, sale line | W3 |
| `TimesheetSubmitted`, `TimesheetApproved`, `TimesheetRefused` | D | timesheet | chatter, inbox, webhooks | submission id | W3 |
| `TicketCreated`, `TicketReplied`, `TicketAssigned`, `TicketStateChanged` | T | helpdesk (services 4.4) | SLA maintenance, timers | ticket id | W3 |
| `SlaWarningRaised`, `SlaBreached`, `SlaTargetMet`, `TicketSolved`, `TicketRated`, `TicketMerged` | D | helpdesk | notifications, webhooks, reports | ticket id, applied SLA | W3 |
| `VisitStarted`, `VisitCompleted`, `VisitCancelled` | T+D | planning (services 4.5) | timesheet, stock (van move, `DemandPort`), ticket update, customer report | visit id, `command_id` | W3 |
| `BookingRequested`, `BookingConfirmed`, `BookingRescheduled`, `BookingCancelled`, `BookingNoShow`, `BookingCompleted` | D | appointments (services 4.6) | reminders, crm lead, webhooks, pos tables | booking id | W3 |
| `ExtractionCompleted`, `ExtractionFieldCorrected` | D | ai extraction (ai 4.6) | account (draft via `ExtractionPort`), vendor memory, eval set | job id, draft ref | W3 (ai open question 1 asks whether to pull extraction into W2) |
| `AnomalyRaised` | D | ai (ai 4.8) | inbox | rule, record | W3 (the deterministic controls in the owning apps are W2; ai 4.8 names no emitter for them) |
| `CommandPlanCreated`, `CommandPlanCommitted`, `CommandPlanExpired` | D | command API (ai 4.3) | approvals inbox, `obop plan` commands (cli 3.9) | plan id | W1 (dry-run with a `plan_id` is W1 kernel work, D86; the approvals engine that consumes them is W2) |

### 3.5 Events with no consumer

These events have no named consumer in any document. They are legitimate facts: webhooks, analytics, chatter and the AI index receive durable events without being named, and the owners' Flows sections stay the authority. The list is reviewed when the catalogue is generated.

- Base and platform: `PartyCreated`, `PartyArchived`, `ActivityDone`, `FollowerAdded`, `MailSent`, `TodoItemCompleted`, `TodoItemConverted`, `DocumentFiled`, `DocumentVersionAdded`, `DocumentTrashed`, `DocumentPurged`, `OcrCompleted`, `ArticleUpdated`, `ArticleCommentPosted`, `SignRequestSent`, `SignRequestDeclined`, `SignRequestExpired`, `AutomationRunFailed`, `RuleAutoDisabled`, `ReportGenerated`.
- Commercial: `LoyaltyPointsEarned`, `LoyaltyRewardRedeemed`, `GiftCardIssued`, `PagePublished`, `PageUnpublished`, `CustomDomainActivated`, `MailingScheduled`, `MailingSent`, `EmailComplaintReceived`, `UnsubscribeReceived`, `SuppressionAdded`, `SendingDomainStatusChanged`, `PortalPaymentStarted`, `RentalReserved`, `RentalPickedUp`, `RentalReturned`, `RentalOverdue`.
- Supply chain: `ProductionStarted`, `ProductionDone`, `ProductionCancelled`, `WorkOrderDone`, `UnbuildDone`, `QualityCheckFailed`, `QualityAlertSolved`, `EcoApplied`, `MaintenanceRequestCreated`, `MaintenanceRequestDone`, `EquipmentDowntimeEnded`, `RepairOrderConfirmed`, `RepairOrderDone`, `PurchaseOrderCancelled`, `PurchaseLineQtyChanged`, `ReplenishmentFailed`.
- People and services: `EmployeeReportingLineChanged`, `LeaveRequested`, `LeaveRefused`, `AllocationGranted`, `BalanceExpired`, `AttendanceOpened`, the `Application*` events, `OfferAccepted`, `ExpenseClaimSubmitted`, `ExpenseClaimRefused`, `ExpenseClaimReimbursed`, `EmployeeSkillChanged`, `PlanCompleted`, `AppraisalShared`, `ObjectiveClosed`, `PayRunComputed`, `ShiftPublished`, `OpenShiftClaimed`, `ShiftSwapRequested`, `OdometerRecorded`, `VehicleContractExpiring`, `VehicleServiceDue`, `ServiceContractActivated`, `ServiceContractExpiring`, `ServiceContractAllowanceExhausted`.
- AI and UX: `AskAnswered`, `AiRunCompleted`, `AiToolCalled`, `AiBudgetThresholdReached`, `AiBudgetExhausted`, `AiFeatureToggled`, `AiCredentialRotated`, `SetupStepCompleted`, `PreferenceChanged`, `SavedViewSaved`, `ViewOverridePublished`, `ViewOverrideReset`, `AnomalyDispositioned`, `ForecastRunCompleted`.
- Security log (SE): `LoginFailed`, `SessionRevoked`, `ApiKeyCreated`, `ApiKeyRevoked`, `ElevationUsed`, `AccessDenied`, `AuditChainBroken`, `AuditCheckpointSigned`, `KeyRotated`, `DsarReceived`, `DsarDelivered`, `RetentionExpired`, `DataRead`. Officers and the audit UI read them.

Events emitted and consumed inside one document (for example `EquipmentDowntimeStarted` to mrp, `QualityAlertCreated` to maintenance, `CustomField*`, `WebhookEndpointDisabled`, `ConnectionNeedsReauth`, `SyncRunFailed`) are catalogued by their owner's Flows section and follow 3.1 to 3.3.

### 3.6 Extension points, commands and other non-events

- Extension points: `BeforeInvoicePost` (owner invoicing; implementers stock_account adds COGS lines, edi runs the EN 16931 pre-flight, the tax provider commits) and `BeforeTransferValidate` (owner stock; implementer quality). The owner "runs" an extension point; it is never listed under "Emits" (rename map, section 8).
- Commands named across documents: `PostInvoice`, `CreateInvoice`, `RegisterPayment`, `ImportStatement`, `ReconcileLines`, `PostEntry`, `ReverseEntry`, `CreateQuote`, `SendQuote`, `AcceptQuote`, `ConfirmSaleOrder`, `ValidateTransfer`, `ConfirmTransfer`, `ReserveMove`, `ConfirmPurchaseOrder`, `ConfirmProduction`, `ClosePosSession`. A command is never listed among events (3.1 rule 4).
- Operational commands (a reserved platform noun is the subject, 2.4), named across documents by cli 3: `plugin.install`, `plugin.copy`, `plugin.update`, `plugin.remove`, `backup.create`, `upgrade.apply`. The reads `plugin search|info|verify|audit|diff` and `plan show|list` emit nothing. `PackActivated` (a localisation effect at its effective date) and `PackageInstalled` (a distribution fact) are different events; installing a pack emits both (Proposed, because `obop pack` and `obop plugin` share one install engine, cli 3.3).
- Realtime frames: `RecordChanged` (invalidation `(model, ids, row_version)`, ARCHITECTURE 12) and `JobProgressed` (job runner progress, Proposed) travel on the WebSocket and are not outbox events.
- Record events: `sale_order.created` in integrations GP-1 is an opt-in record event, valid as written; no `SaleOrderCreated` exists.

### 3.7 Gaps

| Gap | Resolution path |
|---|---|
| `JobProgressed` is named (platform 4.16, ARCHITECTURE 12) but its shape is not defined | platform 4.16 or ARCHITECTURE 12 defines the frame (Proposed) |
| Named consumers that the emitter's owner lists but the consumer's owner does not: `PayslipConfirmed` (notifications in platform 4.6; portal in sales-crm 4.8), `ShipmentTrackingUpdated` and `ShipmentDelivered` (mail, platform 4.7), `LeaveApproved` (calendar, platform 4.8) and `QuoteSent` (mail, platform 4.7) | A consumption is listed on both sides. Adopted here; the consumers' owners (platform 4.6 to 4.8, sales-crm 4.8) add it |
| livechat hand-off to crm or helpdesk names no mechanism | Open in retail 4.6 and services 4.4; a port call or a `Lead` created through the crm command API |
| `plugin update` and `plugin remove` (cli 4.3) have no event, and platform 4.18 lists only `Module*` and `PackActivated` | platform 4.18 decides whether they emit `PackageUpdated` and `PackageRemoved`, or whether `PackageInstalled` and a removal fact cover them (Proposed; 3.4 adds only the three events the registry commands need) |

## 4. Permission-group registry

**Rules.** A group is `<module>.<role>`. The prefix is the owning module id, or the app or engine name where one module hosts several (`contacts` in `base`, `approvals` in `automation`), or a reserved kernel prefix (`base`, `security`, `privacy`, `audit`, `api`). sec-priv 2 owns the kernel seeds and the seed list; a group not yet on the list is added there by its owning spec. The role segment is a noun (`manager`, `operator`). A named action is `<module>.<action>` and is a permission, not a group; it is granted by rows of a group. A field-level permission is a group too (`product.cost_viewer`). Scopes are `all`, `own`, `company`, `domain` (D10); retail's "own session" is `own` and its "branch" scope is a `domain` on `branch_id`. `finance.*` is retired: it is `account.*`. No group, role or scope is named after a plan, tier, edition or price, because nothing is gated by one (OR-2).

| Owner spec | Groups |
|---|---|
| sec-priv 2 | `base.user`, `base.portal`, `base.public`, `base.admin`, `base.api_client`, `base.agent`, `security.officer`, `privacy.officer`, `audit.reader`. Capability rows: `customize_views`, `share_views`, `import_data`, `manage_branding` (ux 2); operation `export` (AZ10) |
| finance 2 | `account.read`, `account.billing`, `account.bookkeeper`, `account.user`, `account.adviser`, `account.manager`, `account.payment_approver`. An auditor holds `account.read` plus `audit.reader` |
| people 2 | `hr.self`, `hr.line_manager` (derived), `hr.officer`, `hr.admin`, `hr.hrbp`, `hr.special_reader`, `recruitment.recruiter`, `recruitment.interviewer`, `payroll.preparer`, `payroll.approver`, `attendances.kiosk`. Named actions: `hr.retro_change`, `hr.correct` |
| sales-crm 2 | `crm.user`, `crm.team_lead`, `crm.manager`, `sales.user`, `sales.team_lead`, `sales.manager`, `sales.invoicing`, `product.user`, `product.manager`, `pricing.manager`, `subscriptions.user`, `subscriptions.manager`, `rental.user`, `rental.manager`, `commissions.manager`, `commissions.self`. Field permission `product.cost_viewer`. Named actions `sales.price_override`, `sales.unlock_order` |
| supply-chain 2 | `stock.operator`, `stock.manager`, `stock.valuation_user`, `purchase.user`, `purchase.manager`, `mrp.user`, `mrp.manager`, `mrp.operator`, `quality.user`, `quality.manager`, `maintenance.user`, `maintenance.manager`, `delivery.user`. Named actions `stock.validate`, `stock.unreserve`, `stock.adjust`, `stock.revalue`, `stock.override_negative`, `stock.override_reservation`, `stock.unreserve_others`, `purchase.confirm`, `purchase.approve`, `mrp.confirm`, `mrp.close`, `mrp.unbuild` |
| retail 2 | `pos.cashier`, `pos.supervisor`, `pos.manager`, `pos.admin`, `ecommerce.merchandiser`, `ecommerce.fulfillment`, `ecommerce.manager`, `loyalty.manager`, `website.editor`, `website.publisher`, `website.designer`, `website.admin`, `marketing.user`, `marketing.manager`, `marketing.deliverability_admin`, `livechat.operator` |
| services 2 | `resource.admin`, `project.user`, `project.manager`, `timesheet.user`, `timesheet.approver`, `timesheet.admin`, `timesheet.cost_viewer` (field permission), `helpdesk.agent`, `helpdesk.lead`, `helpdesk.manager`, `planning.user`, `planning.dispatcher`, `planning.technician`, `appointments.host`, `appointments.manager`, `fleet.manager`, `fleet.driver`, `service_contract.manager` |
| platform 2 | `customize.designer`, `automation.designer`, `approvals.designer`, `analytics.designer`, `documents.user`, `documents.manager`, `knowledge.editor`, `knowledge.admin`, `contacts.manager`, `mail.account_admin`, `sign.user`, `sign.admin` |
| ai 2 | `ai.admin`, `ai.user`, `ai.operator`, `ai.auditor`, `ai.hr_feature_owner`, `ai.ap_intake` (service group of the intake agent) |
| integrations 2 | `api.client_admin`, `api.webhook_admin`, `api.connection_admin`, `api.import_operator`, `api.export_owner`, `api.extension_admin`, `api.device_admin` (Proposed with the mobile sign-in, spec/mobile 5.7: reads and revokes the `device` records of everybody; a person reads, renames and revokes their own with no group) |

**Rulings.**

- **Cashier.** `pos.cashier`; ux's `pos.user` is retired. **HR administrator.** `hr.admin`; ai's `hr.manager` and `hr.user` become `hr.admin` and `hr.officer`; `hr.auditor` is `audit.reader`. The two-key enablement of an HR AI feature is `ai.admin` plus `hr.admin`, with `ai.hr_feature_owner` for the switches.
- **Cost visibility.** One field permission `product.cost_viewer` covers product cost, margin, valuation entries, landed costs, revaluations and manufacturing cost (sales-crm's `catalog.cost_reader` and supply-chain's `cost_viewer` are the same thing; the module is `product`, so `catalog.*` becomes `product.*`). Employee cost is a different audience and stays `timesheet.cost_viewer`. Neither is a data class (section 5).
- **Not groups.** `sales.price_override`, `sales.unlock_order`, `hr.retro_change`, `hr.correct` and all `stock.*`, `purchase.*`, `mrp.*` actions are named actions. `sign_role` (platform 4.11) is a signer slot and `appointment_host` an entity; neither is a group.
- **Overlaps.** `mail.account_admin` manages mailbox connections and `api.connection_admin` the other connections; `api.import_operator` is `import_data` plus staging of foreign-source importers; `export` is a per-model operation and `api.export_owner` governs the full-business export job only. How "business owner only" is held (sec-priv 2 has `base.admin` for both owner and administrator) is a gap for sec-priv and integrations.
- **Package administration (Proposed).** Installing, copying, forking, updating and removing a plugin, approving a capability diff, and raising a copied plugin's capabilities or UI tier are held by the existing group `api.extension_admin` (integrations 2; cli 4.4 uses it). No `plugin.*` or `package.*` group is added. platform 4.18 lists `base.admin` for apps management and is to cite `api.extension_admin` for packages. The read-only registry commands (`plugin search`, `info`, `verify`) touch no business data and need no group; which group reads the installed-plugin list (`plugin audit`) is for platform 4.18, with `api.extension_admin` the Proposed default. Trust level `core` is never granted by a group.
- **Agent names.** The example intake agent is `ap-intake` (ai, sec-priv GP-4); "AP Assistant" in platform 5.2 is its display name. `obop agent create` makes the agent principal, its permission group and its token in one plan (cli 3.9); the agent is a principal, never a party (D28).
- **Interim HR default.** Whether `hr.officer` reads `confidential_hr` by default is open (section 9); a document that must describe it says so and cites sec-priv open question 17.

## 5. Data-class registry

A data class tags columns. Owner of the registry: sec-priv 4.2 (people 4.11 assigns the HR fields). Classes are not permission groups and not retention classes.

| Class | Meaning | Default readers | Handling |
|---|---|---|---|
| `public_directory` | Directory data: name, work contact, department, title, manager | All internal principals | None; `agent_readable` true |
| `internal` | Non-public business data about people and work: type, schedule, skills | Manager chain, HR | Audit on change; `agent_readable` true |
| `confidential_hr` | Personal employment facts that are not money | Self; `hr.officer` (open, section 9) | Separate tables behind `obop_cls_*`; read audit |
| `special_category` | GDPR Art. 9 data as the pack decides: health, union, diversity, disability, work permit | Self, `hr.special_reader` | Field encryption; purpose on every read; DPIA gate |
| `financial` | Money owed to, paid to or held for a named person or counterparty, and anything from which it can be recomputed | Self, payroll groups, finance groups | Separate tables behind `obop_cls_*`; field encryption; read audit; export approval |
| `secret` (Proposed, sec-priv 4.1) | Credentials and key material: API keys, OAuth and IdP secrets, TOTP seeds, PSP and carrier credentials, webhook secrets, BYOK keys | Nobody; write-only | Envelope-encrypted; absent from exports, logs and errors |
| `location` (Proposed, D87, Q40) | Position data of a person: the GPS points and routes of a trip recorded by the Mileage app | Self; the reviewer of a submitted trip (open) | Audit on every read; masking for readers who need only the distance; thinned or deleted after confirmation ([spec/mobile.md](spec/mobile.md) 5.6); not agent-readable by default. Not ratified and not one of the sensitive classes below; until Q40 is answered, point fields use `confidential_hr` |

`confidential_hr`, `special_category` and `financial` are the sensitive classes (ARCHITECTURE 7.2): they get `obop_cls_*` roles, never go to a non-local AI route without an impact-assessment acknowledgement (ai 4.5), are not indexed by default (ai 4.7) and are excluded from the browser cache (ux 4.13). `agent_readable` defaults to false except `public_directory` and `internal`. The `obop_cls_*` roles are database roles, not permission groups.

**One class per field type.** A value from which a protected value can be recomputed carries the strictest class of its inputs (D10's inference-channel rule).

| Field type | Class |
|---|---|
| Wage, salary, pay rate, hourly cost, cost rate, timesheet cost snapshot (`cost_rate`, `amount_minor`) | `financial` (timesheet cost also needs `timesheet.cost_viewer`) |
| Payslips, pay run lines, payroll amounts, salary attachments, commission period results, payouts and adjustments | `financial` |
| Bank account numbers and IBANs of parties and employees, bank statement lines, payment batch files, natural persons' tax ids | `financial` |
| Customer payment token references (PSP references) | `financial`; card numbers are never stored |
| Contract terms, national id, birth date, dependants, private address, emergency contact, appraisals, disciplinary records, offboarding reasons, attendance geo, IP and photo, applicant expected salary | `confidential_hr` |
| Sick-leave type and certificates, union, diversity, disability, work permit, biometric attendance | `special_category` |
| PSP API keys, webhook secrets, carrier and transport credentials, IdP secrets, BYOK keys, API keys and OAuth tokens | `secret` (Proposed); until ratified they are vault entries handled as `financial` |
| Product cost, margin, valuation, landed cost, pricelist cost base; party credit limit | No class: field permission (`product.cost_viewer`; finance groups) |
| Project and analytic margins (aggregates over people) | No class; read with `timesheet.cost_viewer`; the minimum group size of people 4.11 for small teams is services' call (Proposed) |
| Signature evidence (IP, user agent), serials and lots linked to a person, ticket requester email | No class; `personal_data` flag |
| Trip points (`lat`, `lon`, `accuracy_m`, `speed_mps`, `recorded_at` of a mileage point) | `confidential_hr` as an interim reading (attendance geo is already there) until `location` is ratified (Proposed, D87, Q40) |

**Other flags.** `personal_data` (registry capability flag driving DSAR and erasure, D33), `retention_class` (pack-driven, separate taxonomy) and `agent_readable` are per model or class and are not classes. `special-category` as a prose adjective is fine; the identifier is `special_category`. People 2's "public" means `public_directory`; sec-priv AZ5's "confidential and special-category columns" means the three sensitive classes.

## 6. Shared-entity registry and state machines

Owners define states; the rows below fix the names where documents disagreed or both touched the entity.

| Entity | Owner | Canonical names and states | Rulings |
|---|---|---|---|
| `sale_order` (quote, cart, order) | sales-crm 4.4 | Lifecycle `draft`, `confirmed`, `cancelled` plus `locked`; `order_kind` `quote`, `cart`; derived `quote_status` (draft, sent, viewed, accepted, declined, expired), `delivery_status` (pending, started, partial, full), `invoice_status` (`nothing_to_invoice`, `to_invoice`, `upselling`, `invoiced`), `payment_state` (`unpaid`, `partial`, `paid`) | Order `payment_state` summarises its invoices; an invoice `in_process` counts as unpaid for the order. Carts never get a number or `quote_status` (Proposed; sales-crm open question 16, retail open question 6) |
| `cart` | retail 4.3 | Extension row, one per `sale_order` with `order_kind = cart`; states `open`, `checkout`, `pending_payment`, `abandoned`, `expired`, `converted` | Kernel lifecycle stays on the order. The sales channel and the external order id are columns ecommerce adds to `sale_order`, unique on the pair: `ecommerce__sales_channel_id` (full entity name `sales_channel`, 2.1) and `ecommerce__external_order_id` (retail 4.3, ECO-2) |
| `invoice`, `invoice_line` | finance 4.2 | Lifecycle `draft`, `posted`, `cancelled` (drafts only); derived `payment_state` (`unpaid`, `partial`, `in_process`, `paid`), `credit_state` (`none`, `partially_credited`, `fully_credited`) | One model for invoice, bill and credit note, told apart by `direction` and `kind` (finance 4.1.1 lists the values). Credit note: a posted `invoice` that reverses commercial lines; it emits `InvoicePosted` and `InvoiceReversed`. No `vendor_bill` or `account.move` model. `payment_hold` (set by the three-way match through an `invoicing-api` command) is an `invoice` field finance 4.1.1 adds (Proposed) |
| `invoice_line_source` | sales-crm 4.4 | Links invoice line to sale line (qty, kind); unique per `(line, source)` | Written for every posting document; purchase side uses `purchase_line_qty_contribution` kind `billed` |
| `payment`, `payment_batch`, `payment_transaction` | finance 4.5 | `payment`: draft, posted, reconciled, voided. `payment_batch`: draft, confirmed, exported, reconciled. `payment_transaction`: draft, pending, authorized, done, cancelled, error, refunded | `posted` means outstanding entry exists; `reconciled` means bank-matched |
| `expense_claim` | people 4.5 | draft, submitted, approved, refused, cancelled; derived `reimbursement_state` (`not_due`, `to_reimburse`, `in_payroll`, `reimbursed`) | finance 4.11 drops `reimbursed` from the claim states |
| `journal_entry` | finance 4.1.1 | draft, posted, reversed; `entry_kind` values as finance | `lock_date.kind` is `fiscal`, `sale`, `purchase`, `tax`, `hard`; the stock lock date is a `stock` setting |
| `e_document` | finance 4.6 | draft, validated, queued, sent, delivered, acknowledged, rejected, failed, cancelled | |
| `account_role` | finance 4.1.1 | `receivable`, `payable`, `outstanding_receipts`, `outstanding_payments`, `suspense`, `fx_gain_realized`, `fx_loss_realized`, `fx_gain_unrealized`, `fx_loss_unrealized`, `stock_valuation`, `stock_variation`, `grni`, `cogs`, `price_difference`, `inventory_loss`, `production_cost`, `work_center_absorption`, `landed_cost_clearing`, `tax_payable`, `tax_transition`, `deferred_revenue`, `deferred_expense`, `accumulated_depreciation`, `cumulative_translation_adjustment`, `down_payment`, `rental_deposit_liability`, `gift_card_liability`, `purchases` | Role names are snake_case identifiers, not prose; the last four names are Proposed, and an account named in a golden path that is not a role (office expense, payable to employee) stays a chart account. Posting modes `perpetual_at_invoicing` (default), `perpetual_per_transfer`, `periodic`; `periodic` is W2 (supply-chain 3) and finance 5.4 marks that variant W2 |
| `import_run`, `import_mapping`, `import_row` | platform 4.16 | `import_run`: uploaded, mapped, validated, committing, paused, committed, failed, rolled_back, partially_rolled_back; `dry_run_result` (ok, failed) is a field; `posted` is a flag set by accounting importers | `import_batch` is retired; before-images are `audit_change` rows tagged with the run id (Proposed; sec-priv 4.4 adds the cause reference, integrations open question 12 keeps `import_change` as fallback). Platform open question 13 closes on this set |
| `notification` | platform 4.6 | One row per recipient and channel: `delivery_status` (pending, sent, failed) with `failure_reason`; `inbox_state` (unread, read, snoozed, done) with `snoozed_until`; `lane` (mention, activity, alert) | `inbox_item` is retired and the read flag becomes `inbox_state`. Inbox approvals and agent proposals are shown from `approval_task` and `command_plan`, not notification rows. `notification_preference` holds the matrix; `user_preference` references it |
| `command_definition` | ux 4.2 | Palette entry (label, availability, CEL condition, hotkeys) | platform 4.15's `command` is this; "command" alone means a business command |
| `principal` | sec-priv 4.1 | invited, active, suspended, deactivated, for every kind | ai's agent state `retired` is `deactivated` |
| `agent_delegation` | sec-priv 4.1 | active, suspended, expired, revoked | `revoked` added (ai, platform) |
| `api_key`, `credential`, `session` | sec-priv 4.1 | `api_key`: active, expired, revoked. `credential`: one factor (password, totp, passkey, recovery): pending, active, disabled, revoked. `session`: active, `idle_expired`, `absolute_expired`, revoked | ai's agent "credential" is an `api_key` of an agent principal |
| `oauth_client`, `consent_grant` | integrations 4.2 | `oauth_client`: active, suspended, revoked. `consent_grant`: active, revoked, expired | ai's names and states converge here; revoking a client revokes its grants. `consent_grant` (OAuth) is unrelated to `consent_record` (privacy ledger, sec-priv 4.6) |
| `share_grant` | sec-priv 4.1 | active, used, expired, revoked | `used` applies to single-use scope; platform 4.9 adopts the four |
| `consent_record` | sec-priv 4.6 | Append-only, kind `grant` or `withdrawal`, reached through `ConsentPort` | recruitment's `candidate_consent` becomes rows of this ledger with a purpose and expiry (Proposed; people 4.4, which notes that sec-priv 4.6 lists no expiry field) |
| `timesheet_entry` | services 4.3 | draft, submitted, approved, refused; 1:1 extension of `analytic_line`, whose `source_ref` tells timesheet, invoice and bill lines apart (closes services open question 3) | "timesheet line" is retired |
| `resource_work_interval`, `resource_leave` | services 4.1 | Working interval of a calendar; leave kinds `public_holiday`, `time_off`, `closure` | `resource_attendance` is retired; `attendance` means a check-in record (people 4.3) only. `leave_request` (people 4.2, events `Leave*`) and `resource_leave` (events `LeaveRecorded`, `LeaveRemoved`) are different entities |
| Contract entities | each owner | `employment_contract` (people), `service_contract` (services), `vehicle_contract` (services) | No bare `contract` entity, column or event: `employment_contract_id`, `service_contract_id`, `service_equipment_id`. `maintenance_equipment` (own assets, supply-chain 4.6) and `service_equipment` (customer sites, services 4.5) stay separate, and neither is a bare `equipment` |
| `pay_run`, `payslip` | people 4.10 | `pay_run`: draft, computed, confirmed, posted, paid, reversed | `payrun` is retired |
| `subscription` | sales-crm 4.5 | incomplete, trialing, active, past_due, unpaid, paused, cancelled | `canceled` retired |
| `mailing` | retail 4.5 | draft, pending_approval, scheduled, sending, paused, done, cancelled | `canceled` retired |
| `purchase_bill_match` | supply-chain 4.2 | matched, `within_tolerance`, exception | Owns the three-way match (finance 3 keeps only the duplicate-bill fingerprint). A mismatch sets `invoice.payment_hold` and raises an approval item |
| Multi-word state values | supply-chain, people | `quality_alert`: new, confirmed, `action_proposed`, solved, cancelled. `mrp_eco`: draft, `in_progress`, approval, done, cancelled. `appraisal`: draft, `self_review`, `manager_review`, calibration, shared, done | All snake_case |
| Reminders and dunning | finance 4.2, sales-crm 4.5 | Subscription side: `dunning_policy`, `dunning_run`. Invoice reminders: `follow_up_policy` (Proposed name; finance 4.1.1 adds it) | One step-list policy engine serves both (Proposed); neither spec names it or its host module, which stays open |
| `approval_request`, `approval_task` | platform 4.12 | request: pending, approved, executed, rejected, cancelled, expired, superseded; task: waiting, active, approved, rejected, delegated, skipped, expired | `plan_stale` is ai's HTTP 409 for a `command_plan` in `stale`, which is a request in `superseded`. CLI alias `approval` (2.4) |
| `command_plan` | ai 4.3 | open, awaiting_approval, approved, committed, rejected, expired, stale | The dry-run result of a mutating command: digest-bound, single-use `plan_id`. CLI alias `plan` (`obop plan show\|commit\|reject\|wait`, cli 3.9), which collides with the subscription model `plan` (2.4); its problem code is `plan_stale` and its exit code is 6. Not a pricing plan (2.2) |
| `package_install` | platform 4.18 | active, disabled, removed; attributes `trust_level` (`private`, `community`, `verified`, `core`), `mode` (`install`, `copy`; Proposed, cli 4.2), `sideloaded`, `capability_digest` | A version revoked by the index (the D32 kill switch, cli 4.7) goes to `disabled`; the reason is an attribute, not a state (Proposed). A copy is business-own code at `private`, and the index never revokes it (`plugin audit` warns). The listing entities `package`, `package_version` and `compat_result` (integrations 4.7) belong to the registry, not to a business; stage 1 of the index is files in a git repository (D81). `obop.lock` is the projection of this table (2.4) |

The `external_id` (module-shipped records), `external_ref` (import identity ledger, integrations 4.8) and `external_mapping` (connector sync, integrations 4.5) are three different tables. `outbox_delivery` states are pending, in_flight, succeeded, exhausted, cancelled (integrations 4.3).

## 7. Hub ports, module ports and provider traits

### 7.1 Hub ports (D27)

D27 (BINDING) names the four hub ports. Implementers beyond the first are Proposed until D27 is amended.

| Port | Implemented by | Called by | Rulings |
|---|---|---|---|
| `DemandPort` | `stock` | sale_stock, pos_stock, mrp, repair, planning (van stock) | `project` is not a caller (services 4.2 does not use it); `planning` is, for van stock. `quality` is not a caller; it uses `StockMovePort` for hold moves |
| `QtyContributionPort` | `sales` (sale lines); `purchase` (purchase lines, Proposed; already in ARCHITECTURE 4.3) | sale_stock and purchase_stock (`('transfer', id)`), sale_timesheet (`('timesheet', id)`), sale_project milestones, service_contract allowance split | Unique on `(line, source)`. `purchase` is a second implementer (Proposed), as GLOSSARY and supply-chain 4.2 list it |
| `PostingPort` (in `account-api`) | `account`, by account role | stock_account, mrp_account, payroll, pos_account, rental (deposit role) | `expenses` is not a caller: an approved claim becomes a draft bill through `invoicing`. Payroll posts itself after a confirmed run |
| `InvoiceableSource` | `sales`, `subscriptions`; Proposed: `purchase` (vendor bills), `pos`, `expenses` via `expenses_sales` | `invoicing` (`CreateInvoice`) | The port is called by invoicing, never the other way round. `timesheet` is not an implementer: billable time reaches invoicing through the sale line. Time-only invoicing is open (section 9.1) |

### 7.2 Module and bridge ports

`StockMovePort` (create, confirm, reserve, cancel) and `StockQueryPort` (availability, forecast): `stock`; callers sale_stock, pos_stock, mrp, repair, quality. `PricingPort::price`: `pricing` (pure, via the Layer 0 crate `obop-calc`, Proposed); callers sales, subscriptions, rental, ecommerce, pos. `ConsentPort` (`base-api`): the consent ledger; callers website forms, pos sign-ups, checkout, marketing, recruitment. `ApprovalPort`: approvals engine; `SignPort`: sign; `NotifyPort`: mail; `SearchPort`: kernel search; `ThreadPort`, `AttachmentPort`, `TemplatePort`, `RenderPort`: platform 4.1; `KnowledgeSearchPort`: knowledge; `EmployeePort` (public lookup, scope test, `create_draft`, `cost_rate`): hr; `SkillPort`: hr; `ResourceLeavePort`: resource (called by `hr_resource`); `ExtractionPort`: ai (optional, called by account and expenses).

Each bridge is specified in exactly one spec and listed in the module table of every spec it touches. `sale_stock`, `purchase_stock`, `stock_account`, `mrp_account`, `sale_mrp`, `purchase_mrp`: supply-chain. `sale_project`, `sale_timesheet`, `hr_resource`: services. `pos_stock`, `pos_account`: retail. `expenses_payroll`, `expenses_sales`, `attendances_time_off`, `attendances_payroll`, `recruitment_website`: people. `sale_payment`, `rental_stock`, `subscription_stock` (all Proposed): sales-crm. supply-chain 4.1.5 lists only its own six bridges and adds `pos_stock`, `rental_stock`, `subscription_stock` and `sale_payment` to its table. `sale_payment` (consumes `PaymentTransactionDone`, confirms the order at `prepayment_percent`) stays a Proposed bridge; whether `payment` instead publishes a hub port is open (section 9.1).

### 7.3 Provider traits and pack hooks

| Trait | Owner spec | Implementers |
|---|---|---|
| `PaymentProvider`, `BankFeed`, `TaxProvider`, `ExchangeRateSource`, `EDocTransport`, `DocFormat`, `AuditFileExporter`, `IntegrityProfile`, `VatIdValidator` | finance 4.4 to 4.14 | Native adapters (X5), pack hooks, connectors (X3 or X4) |
| `CarrierProvider` | supply-chain 4.9 | Aggregator and carrier connectors |
| `PayrollProvider` | people 4.10 | Provider connectors |
| `FiscalAdapter`, `TerminalDriver`, `SmsProvider` | retail 4.1, 4.5 | LocalDevice, CloudSigner, ChainedSoftware; PSP terminals; Twilio first |
| `MailProvider`, `CalendarProvider`, `LookupProvider`, `MessagingProvider` | integrations 4.5 | `CalendarProvider`, `LookupProvider`, `MessagingProvider` are Proposed names; platform 4.2 and 4.8 adopt them |
| `LlmProvider`, `Forecaster` | ai 4.5, 4.8 | Wire-format adapters; augurs baselines |
| `OcrEngine`, `DocEngine`, `SpreadsheetHost`, `RoutingProvider`, `WakeBus`, `BusinessRouter`, `object_store`, `SearchBackend` | platform, services, ARCHITECTURE | As their specs |

Retail's "the `payment` provider trait", "the carrier trait" and "provider trait, BYO Twilio first" are `PaymentProvider`, `CarrierProvider` and `SmsProvider`.

## 8. Rename map

Short names: README, PRODUCT, ARCH (ARCHITECTURE), DECISIONS, ROADMAP, BUSINESS, LOCALIZATION, ENGINEERING, GLOSSARY; specs `platform`, `sec-priv`, `finance`, `sales-crm`, `supply-chain`, `retail`, `people`, `services`, `ux`, `ai`, `integrations`, `cli`. Where a document only mentions a retired form as history, the fix is to delete or reword the sentence. Section 8.1 lists renames of single names; section 8.2 lists words and ideas that the owner rulings of 2026-10-01 retired.

### 8.1 Renames

| Kind | From | To | Documents |
|---|---|---|---|
| event | `CreditNoteIssued` | `InvoiceReversed` | finance, retail, sales-crm |
| event | `CurrencyRateUpdated` | `FxRatesUpdated` | sales-crm |
| event | `PermissionGroupChanged` | `PermissionChanged` | ARCH, platform, sec-priv |
| event | `ImportBatchCommitted` | `ImportRunCommitted` | ARCH, ux |
| event | `InboundMailRouted`, `MailMessageReceived` | `MailReceived` | ai |
| event | `AttachmentUploaded` | `AttachmentCommitted` (Proposed) | ai |
| event | `OAuthClientAuthorised`, `OAuthClientRevoked` | `OAuthConsentGranted`, `OAuthConsentRevoked` | ai |
| event | `AgentPrincipalCreated`, `AgentSuspended` | `PrincipalInvited`, `PrincipalSuspended` | ai |
| event | `VendorBillDraftCreated` | removed (`ExtractionCompleted` carries the draft ref) | ai |
| event | `SubscriptionCanceled` | `SubscriptionCancelled` | sales-crm |
| event | `data.read` | `DataRead` | people |
| event | `SlaWarning`, `ServiceDue` | `SlaWarningRaised`, `VehicleServiceDue` | services |
| event | `ContractActivated`, `ContractExpiring`, `AllowanceExhausted` | `ServiceContractActivated`, `ServiceContractExpiring`, `ServiceContractAllowanceExhausted` | services |
| event | `CalendarChanged` | `ResourceCalendarChanged` | services |
| event | `InterCompanyMirrorCreated` | `IntercompanyMirrorCreated` | finance |
| event | `TaxClosingPosted` emitted by `account` | emitted by `tax` | finance |
| event | people 4.4 consumes `BookingConfirmed` | removed | people |
| event | `PaymentReconciled` confirms a deposit booking | `PaymentTransactionDone` | services |
| event | `PaymentReconciled` as same-transaction handler; `PaymentTransactionDone` as same-transaction handler | D only | sales-crm |
| event | `PayslipConfirmed` consumed by `account` | not consumed by account; payroll calls `PostingPort` | finance |
| event | sales consumes `PaymentPosted`; edi consumes `CreditNoteIssued` and `InvoiceReversed` | sales consumes `PaymentReconciled` and `PaymentVoided`; edi consumes `InvoicePosted` only | finance |
| event | `PartyErased` not consumed by people and services | consumed | people, services |
| event | `ExportRequested` and `ExportReady` both owned by integrations | `ExportRequested` is a sec-priv security event; `ExportReady` is integrations | platform |
| event | `RegistryVersionChanged` attributed to platform | defined in ARCH 5.1; platform 4.14 only triggers it | ux |
| event | platform 4.4 "emits `PrincipalInvited`, `PrincipalDeactivated`" | base principal management (sec-priv 4.1) emits all three; platform shares the names | platform |
| permission-group | `pos_cashier`, `pos.user` | `pos.cashier` | retail, ux |
| permission-group | `pos_supervisor`, `pos_manager`, `pos_admin` | `pos.supervisor`, `pos.manager`, `pos.admin` | retail |
| permission-group | `shop_merchandiser`, `shop_fulfilment`, `shop_manager` | `ecommerce.merchandiser`, `ecommerce.fulfillment`, `ecommerce.manager` | retail |
| permission-group | `web_editor`, `web_publisher`, `web_designer`, `web_admin` | `website.editor`, `website.publisher`, `website.designer`, `website.admin` | retail |
| permission-group | `mkt_user`, `mkt_manager`, `mkt_deliverability_admin` | `marketing.user`, `marketing.manager`, `marketing.deliverability_admin` | retail |
| permission-group | `livechat_operator`, `loyalty_manager` | `livechat.operator`, `loyalty.manager` | retail |
| permission-group | `warehouse_operator`, `inventory_manager`, `inventory_valuation_user` | `stock.operator`, `stock.manager`, `stock.valuation_user` | supply-chain |
| permission-group | `purchase_user`, `purchase_manager` | `purchase.user`, `purchase.manager` | supply-chain |
| permission-group | `mrp_user`, `mrp_manager`, `mrp_operator` | `mrp.user`, `mrp.manager`, `mrp.operator` | supply-chain |
| permission-group | `quality_user`, `quality_manager`, `maintenance_user`, `maintenance_manager`, `delivery_user` | `quality.user`, `quality.manager`, `maintenance.user`, `maintenance.manager`, `delivery.user` | supply-chain |
| permission-group | `catalog.user`, `catalog.manager` | `product.user`, `product.manager` | sales-crm |
| permission-group | `catalog.cost_reader`, bare `cost_viewer` | `product.cost_viewer` | sales-crm, supply-chain |
| permission-group | `hr.manager`, `hr.user` | `hr.admin`, `hr.officer` | ai, people, sec-priv |
| permission-group | `hr.auditor` | `audit.reader` | people, sec-priv |
| permission-group | `finance.accountant`, `finance.auditor`, `finance.bookkeeper`, `finance.manager`, `finance.payment_approver` | `account.user`, `account.read` plus `audit.reader`, `account.bookkeeper`, `account.manager`, `account.payment_approver` | finance, ai, sec-priv |
| permission-group | `ap_intake` | `ai.ap_intake` | ai |
| permission-group | `sales.price_override`, `sales.unlock_order`, `hr.retro_change`, `hr.correct` listed as groups | named actions, unchanged names | sales-crm, people |
| data-class | `public` (people 2) | `public_directory` | people |
| data-class | "confidential and special-category columns" (AZ5) | the three sensitive classes | sec-priv |
| data-class | hourly cost, cost rate, timesheet cost snapshot in `confidential_hr` | `financial` | people, services, supply-chain |
| data-class | commission period results in `confidential_hr` | `financial` | sales-crm |
| data-class | PSP credentials and tokens in `financial` | credentials `secret` (Proposed), token references `financial` | finance, sales-crm |
| data-class | product cost and margin proposed as class `financial` (stale) | field permission `product.cost_viewer`, no class | supply-chain, sales-crm |
| entity | `import_batch`, `import_change`, states `dry_run_ok`, `dry_run_failed`, `posted` | `import_run`, `audit_change` rows tagged with the run id, `dry_run_result` field, `posted` flag | ARCH, platform, ux, integrations |
| entity | `inbox_item` | `notification` (`inbox_state`) | platform |
| entity | palette `command` | `command_definition` | platform |
| entity | agent `credential` | `api_key` | ai |
| entity | `payrun` | `pay_run` | people |
| entity | `resource_attendance` | `resource_work_interval` | services |
| entity | `contract_scope`, `contract_visit_plan` | `service_contract_scope`, `service_contract_visit_plan` | services |
| entity | `sale.order`, `sales_order.confirm` | `sale_order`, `sale_order.confirm` | sec-priv, ai |
| entity | `vendor_bill`, `vendor_bill:*`, `account.move:draft`, `account.payment:draft` | `invoice`, `invoice:*`, `invoice:create_draft`, `payment:create_draft` | ai, integrations, sec-priv |
| entity | notification matrix inside `user_preference` | `notification_preference` | ux |
| entity | "timesheet line" | "timesheet entry" | services |
| field | `contract_id` on `employee_version` | `employment_contract_id` | people |
| field | `contract_id` on `ticket`, `service_equipment`, `contract_scope`, `contract_visit_plan` | `service_contract_id` | services |
| field | `equipment_id` | `service_equipment_id` | services |
| field | "salary field" (GP-2) | `wage` | ai |
| state | subscription and mailing `canceled` | `cancelled` | sales-crm, retail |
| state | principal `retired`; `oauth_client` registered, removed | `deactivated`; active, revoked | ai, integrations |
| state | `agent_delegation` lists lacking `suspended` or `revoked` | active, suspended, expired, revoked | platform, ai, sec-priv |
| state | `share_grant` without `used` | active, used, expired, revoked | platform |
| state | `consent_grant` without `expired` | active, revoked, expired | ai |
| state | claim state `reimbursed` | derived `reimbursement_state` | finance |
| state | `nothing to invoice`, `to invoice` | `nothing_to_invoice`, `to_invoice` | sales-crm |
| state | `partially credited`, `fully credited` | `partially_credited`, `fully_credited` | finance |
| state | `action proposed`, `in progress`, `within tolerance` | `action_proposed`, `in_progress`, `within_tolerance` | supply-chain |
| state | `self review`, `manager review`; `not due`, `to reimburse`, `in payroll`; capitalised pay run states | snake_case, lower case | people |
| state | `idle-expired`, `absolute-expired`; capitalised erasure states | `idle_expired`, `absolute_expired`; lower case | sec-priv |
| port | `sale-api` | `sales-api` | sales-crm, supply-chain |
| port | `DemandPort.raise` | `DemandPort::raise` | sales-crm |
| port | "the carrier trait", "payment provider trait", "provider trait, BYO Twilio" | `CarrierProvider`, `PaymentProvider`, `SmsProvider` | retail |
| port | `InvoiceableSource` row of finance 4.15; "timesheet is an implementer" | implemented by `sales`, `subscriptions` (and Proposed others), called by `invoicing`; not timesheet | finance, services, GLOSSARY |
| port | `QtyContributionPort` "sales only" | `sales` and `purchase` (Proposed) | GLOSSARY, supply-chain |
| port | `DemandPort` callers include `project` | `planning` | ARCH |
| port | `expenses` as `PostingPort` caller | not a caller | finance |
| port | `candidate_consent` without `ConsentPort` | ledger rows through `ConsentPort` (Proposed) | people |
| other | "approval engine" | "approvals engine" | DECISIONS, ROADMAP, platform, sec-priv, services, supply-chain, ai |
| other | "one picking" | "one transfer" | supply-chain |
| other | prose role names (stock valuation, GRNI, COGS, deferred revenue) | snake_case account roles | finance |
| other | "Consumed from Platform: website" | "Consumed from Retail" | people |
| other | ROADMAP ids `S7`, `P1`, `P1 to P8` | `SR7`, `PR1`, `PR1 to PR8` | BUSINESS, PRODUCT, retail |
| other | effect values without `money_and_payee`; operation metadata without `cost_class` | add both (Proposed) | ai, ARCH |
| other | extension points listed under "Emits" (`BeforeInvoicePost`, `BeforeTransferValidate`) | listed under "Runs" (not an event) | finance, supply-chain, sales-crm |
| other | agent name `ap-agent` | `ap-intake` | sec-priv |
| other | finance capability "Duplicate-bill fingerprint, three-way match" as finance | duplicate-bill fingerprint stays in finance; three-way match belongs to `purchase` | finance |
| other | `periodic` posting mode shown as a W1 variant (finance 5.4) | W2 | finance |
| type | `TenantTx`, `TenantRouter` | `BusinessTx`, `BusinessRouter` (D24; RATIFIED, OR-14, no longer Proposed) | ARCH, ENGINEERING, GLOSSARY, sec-priv (the sentences that call the rename Proposed) |
| entity | `tenant_release_state` | `business_release_state` (D24; RATIFIED, OR-14) | ENGINEERING, GLOSSARY |
| trust level | `reviewed` (manual-review tier) | merged into `verified` (OR-3, D81) | BUSINESS, GLOSSARY, PRODUCT, integrations |
| trust level | `partner` (commercial SLA tier) | retired: no partner programme (OR-2); the levels are `private`, `community`, `verified`, `core` | BUSINESS, GLOSSARY, PRODUCT, integrations |
| cli | `obop ext ...`, `obop ext apply`, `obop ext test` | `obop plugin ...`, `obop plugin install ./pkg`, `obop plugin test` (cli N8, Proposed with Q22). The codemod subcommand `obop ext migrate` has no counterpart in cli 3 yet and needs a home there | platform, ux, ARCH, ENGINEERING, integrations |
| cli | `obop restore` | `obop business import` (cli N8, Proposed with Q22; `obop restore` is built and loads a backup, not an export archive, so the old spelling stays as an alias and the two commands must not be conflated) | integrations, finance |
| cli | "CLI v0" as one line of the W1 extension-platform cut | the generated CLI for every shipped module from W1 (D80, D86); the platform subcommands are those of 2.4 | PRODUCT, ROADMAP, integrations |

### 8.2 Retired vocabulary (owner rulings of 2026-10-01)

These words name ideas that OR-1 to OR-21 superseded (DECISIONS section 7 lists the statements). A current document does not use them except in a one-line "retired" note that links here (section 1, rule 4), or as an Odoo, competitor or research fact. Third-party facts (Wasmtime's own LTS lines, Odoo's Enterprise edition) stay true as facts about that software.

| Retired word or idea | Use instead | Ruling |
|---|---|---|
| "Paid list", "open or paid", "ratchet", "`ee`" tree or layer, open-core, enterprise layer, "Community" or "Enterprise" as obop editions | "open and free": one codebase under Apache-2.0, no gated capability. A former paid-list capability is an ordinary backlog item | OR-1, OR-2, D2, DECISIONS 6 |
| Plan names Free, Team, Growth, Scale (and "Business"), prices, per-seat or per-organisation pricing, "edition", "plan" or "tier" as something a customer buys | Nothing for the software: no plan, price list or pricing unit exists (2.2). The one exception is the planned hosting service of OR-22 (a dedicated server at a stated monthly price, "planned, not yet offered or charged"): call it "hosting" and name its sizes by vCPU and RAM, never as Free, Team, Growth or Scale | OR-2, OR-22 |
| "Assurance" (service, DE), legal-change SLA, capped warranty, hosted AI credits, payroll service, certification fees | Packs are as-is Apache-2.0 content earning trust through golden cases and oracles; BYOK AI | OR-2, D17, D18 |
| "LTS", "1.0 LTS", a five-year window, the 24-month deprecation floor, the monthly patch train | "1.0", the first stable release; a 2-year security-fix window from it; a 12-month deprecation window before 1.0; "series" | OR-12, D85 |
| Trust levels `reviewed` and `partner`; a partner tier, programme, white-label licence, revenue share | `verified`; nothing. The registry is free and handles no payments | OR-2, OR-3, D81 |
| Paid listings, payouts, take rate, "marketplace commission" | "plugin registry" (free, open; install or copy from the CLI; it lists only plugins with an OSI-approved licence) | OR-2, OR-3, OR-17 |
| AGPL core, Application Exception, licence-grant CLA, "OSI licence forever" pledge, steward veto | Apache-2.0 for everything; DCO sign-off, no CLA; the owner signs off every commit (`git commit -s`) and agents are recorded with a `Co-Authored-By` trailer | OR-1, OR-18, D1, D82 |
| obop Cloud, paid cloud, paid beta, hosted cells or rings as a SpaceCorps offer, SLA plan | SpaceCorps hosts nothing for others today except the ruled sample-data sandbox (OR-20, D83); the one planned hosted offer is the dedicated-server hosting service of OR-22 (planned, not offered), and the duties of a hosting provider (processing agreement, sub-processor disclosure, terms and privacy notice for hosted instances, Data Act switching, CRA role, VAT and payment provider, abuse and sanctions) are open counsel items for it (ENGINEERING 11, C10; docs/ISSUES.md K20); a "cell" is an operator's sizing rule; an optional local demo is `obop dev up` (Proposed) | OR-11, OR-20, OR-22, D83 |
| "No hosted sandbox before 1.0", "evaluation through a local demo" as the default, the sandbox as an open question (Q27) | A free hosted sandbox opens at the public beta (0.5): ephemeral, sample data only, run by SpaceCorps, with the duties listed in D83; "sandbox" never names a test copy of a business (say "business clone", 2.2) | OR-20, D83 |
| "GA", "DE GA" | "public beta" (0.5), "pack stable" (DE, 2027-09-30), "1.0" | D26 |
| "Steward" or "foundation" for SpaceCorps; the company plus EU non-profit structure | "Publisher" (SpaceCorps); "steward" only for a foundation that may replace it | OR-10, D1 |
| Hiring gates HG0 to HG3, headcount bands, ARR or paying-customer exit criteria, a funding plan | The team is the owner plus AI agents; exit criteria are correctness and adoption measures | OR-6, D84 |
| UI-first or "UI then API" build order; a feature that exists in the UI only | Command first; CLI, API and MCP generated; UI at literal parity before 1.0, except commands on the owner-approved exemption list | OR-4, OR-16, D80 |
| "Recorded exemption" or "UI-exempt" said of a command the owner has not approved; a browser-only or UI-only step as an exemption class | An entry of the owner-approved exemption list (CLI-only, each with a written reason); a UI-only step is not adopted as a class, because OR-4 bars UI-only features | OR-16, D80 |
| "RATIFIED with the closure of Q10" (said of I-1, the D24 renames and the X and L prefixes) | "RATIFIED, OR-14": the owner ratified them on 2026-10-01; Q10 is closed by OR-8, OR-9 and OR-14 | OR-14 |

## 9. Open items that affect names

### 9.1 Owner decisions

These stay open. Documents keep "Proposed" wording and cite the question; this file does not pick a side. Q10 is closed (OR-8, OR-9, OR-14): the D24 renames and the X and L prefixes are RATIFIED by the owner on 2026-10-01 (OR-14; section 8.1), so its row is gone. The other questions the rulings closed (Q1, Q3, Q4, Q7, Q13 and Q19 in the first batch; Q29 and Q31 in the second) never governed a name here. Q27 (the hosted sandbox, OR-20) added the term "sandbox" (2.2) and the Proposed sandbox spellings below, Q30 (literal UI parity, OR-16) added the term "exemption list" (2.2), whose entries and record names are below, and Q23 was answered by OR-17, which leaves the rows below only its open detail.

| Item | Why it matters for names | Reference |
|---|---|---|
| The T0 to T3 tier labels, the T0 app list (`customize` and `resource` as T0 modules), the slip order of the W1 cut with the landing waves of the displaced items (the in and out calls are RATIFIED, OR-15, and the services variant is W3), and the hr employee core shipping with `timesheet` in W3. The wave windows W0 to W4 are RATIFIED (OR-13) | Module ids `customize` and `resource`, and every per-module wave value in section 3.4 | D25, D86, Q8 |
| Identifier families that share letters: keep and qualify, or rename | Section 2.1 follows "keep and qualify" | Q18 |
| POS order capture at 1.0 or in W3 | Wave of every `pos` event, the `pos-api` contract and the `pos.*` groups | D13, D25, Q11 |
| Whether `payment` publishes a hub port instead of the `sale_payment` bridge | A new hub port amends D27 | D27; sales-crm open question 6 |
| Time-only invoicing, that is whether `timesheet` implements `InvoiceableSource` | The implementer list in 7.1 | ARCHITECTURE 16.3 item 7; services open question 14; finance open question 18; Q8 |
| Whether `hr.officer` reads `confidential_hr` by default, or each class needs an explicit grant | Default readers in section 5 | sec-priv open question 17; people open question 14 |
| The working name "obop" (RATIFIED as the working name, with clearance first; a screened fallback if counsel does not clear it, OR-5) | The `obop-` crate prefix, the `obop` binary and command, `obop.toml`, `obop.lock`, the reserved `obop.*` package namespace, any `@obop/*` package and `OBOP_` variables all change with it | D3, Q2 |
| Pack wave and country order after Germany (`l10n_fr`, `l10n_pl`, AT and CH). Germany first is RATIFIED (OR-7) and `l10n_de` is W1 | Pack module ids and their wave column | D4, D25, LOCALIZATION open questions |
| The CLI grammar of 2.4: subject spelling (model id verbatim with a hyphen alias on input, or kebab-case canonical), the reserved words, the platform verbs, flag names, exit codes and environment areas | Every name in 2.4; the CLI line, the wire id, the REST path and the MCP tool name share the subject | Q22, D80; cli open question 1 |
| The detail of `verified` that OR-17 leaves open (how a publisher is verified, the full list of automated gates), the copy-mode provenance record, the single spelling of the trust attribute (`trust_level` here, `tier` in cli 4.2 and `--tier`), and whether first-party packs are addressed as `obop.l10n-de` or `l10n-de`. The OSI-approved-licence admission rule and the meaning of `verified` as a verified publisher plus automated gates are RATIFIED (OR-17) | The plugin rows of 2.1, the `package_install` row of section 6 and the package events of 3.4 | Q25, D81, OR-17; cli 4 |
| The owner's approval of each entry of the exemption list (the bootstrap and local commands that cli 1.1 proposes are proposals until approved; OR-16) | Which commands have no UI binding, and so the values of the catalogue field `ui.exempt` and of the parity matrix (2.4) | OR-16, D80; cli 1.1 |
| Whether the hosted sandbox needs its own server area (`OBOP_SANDBOX_<NAME>`, for example a session time to live) or a client profile named `sandbox` (2.1); Proposed reservations that no other document uses yet | Setting and profile spellings of the sandbox operation; nothing else in this file depends on them | Q35, Q36, D83 |
| A data class `location` for trip points and routes, and the Proposed names of the `mileage` package (models, groups, actions) | The class table of section 5 and the group and event spellings of the `mileage` module | Q40, D87; [spec/mobile.md](spec/mobile.md) |
| `@obop/*` SDK package names and whether obop publishes anything to the npm registry | The names of the generated TypeScript SDKs and the UI SDK; the default (Q32) is no npm-registry publication, decided at W2 (OR-19), so the scope may never exist | Q32, OR-19, D3, D80 |

### 9.2 Proposed rulings awaiting the owning spec

These are not owner decisions. The named spec accepts, amends or rejects each when next edited; until then they are Proposed.

| Ruling | Spec that decides |
|---|---|
| `secret` as a sixth data class; `api.export_owner` and how "business owner only" is held | sec-priv 4.1, 2; integrations 2 |
| Pay data (wage, cost rate, commission) in `financial`; cost and margin as field permission `product.cost_viewer` and no class | people 4.11, services 2, sales-crm 4.7, supply-chain 4.1.6 (closes sales-crm open question 17 and supply-chain OQ-17) |
| `audit_change` carries the import run as cause; `import_change` is the fallback | sec-priv 4.4; integrations open question 12 |
| Recruitment consent as ledger rows through `ConsentPort`, replacing `candidate_consent` | people 4.4 |
| `purchase`, `pos` and `expenses` as `InvoiceableSource` implementers; `purchase` as `QtyContributionPort` implementer | supply-chain 4.2, retail 4.1, people 1.2; D27 note |
| Emitters of `PackActivated`, `PartyBankAccountChanged`, `AttachmentCommitted`, `TaxRuleChanged` and `FxRatesUpdated`: adopted in the owning specs as Proposed, so they stay Proposed. `JobProgressed` frame shape | platform 4.1, 4.2, 4.3, 4.18 and finance 4.4 (emitters); platform 4.16 or ARCHITECTURE 12 (frame) |
| `obop-calc` as the Layer 0 crate behind `PricingPort` and the tax engine | ARCHITECTURE 4.1; sales-crm 4.2 |
| Whether `Idempotency-Key` is required on plain CRUD or only accepted (integrations 4.1 "carries", ARCHITECTURE 8.2 "accept") | integrations open question 11 |
| Device identity: kiosk and POS devices are API-client principals plus a device record (`pos_device`), with no new principal kind (D28) | people 2, retail 4.1 |
| Whether the `device` model of native apps (D87, built as `device` in the Customize module on 2026-10-28) and `pos_device` are one model or two; the build decides nothing | mobile 15 (item 2); people 2; retail 4.1 |
| Package events `PackageInstalled`, `PackageCopied`, `PackageRevoked` (aggregate `package_install`), the `mode` attribute, whether update and removal emit events, and the group for the installed-plugin list | platform 4.18; integrations 2 (`api.extension_admin`); cli 4 |
| CLI aliases `plan` and `approval`, operational subjects with no model behind them, and the catalogue-based names of 2.4. The alias `plan` collides with the subscription model `plan`; the Proposed default keeps the model id and drops or renames the alias | cli 2.1 (N2, N3); sales-crm 4.5; owner answer Q22 |
| The per-module wave values of 3.4 after the D86 cut. Which modules are in or out of W1 is RATIFIED (OR-15, and project and timesheet are W3); the landing wave of each other displaced item (for example `PurchaseOrderConfirmed` and `PriceRuleChanged` in W2) and every other placement are Proposed | The owning specs when next edited (D25, D86) |
| The exemption record of the parity row in 2.4 (attribute `exemption` with `reason`, `surfaces_lacked`, `approved_by`, `approved_on`) and the catalogue field `ui.exempt`. The exemption list itself and its approval by the owner are OR-16; only these spellings are Proposed | cli 1.1 and 2.9; owner approval of each entry (OR-16) |

Facts that are not names (dates, version pins, counsel timing, Astro wording, lockfile lists, the exit drill) are outside this file: they live in [DECISIONS.md](DECISIONS.md) or in the owning document.

## 10. Sources

All paths are relative to this file.

| Tag | File |
|---|---|
| Register | [DECISIONS.md](DECISIONS.md) sections 4 (vocabulary, identifier namespaces), 5 (tiers, waves, the W1 cut), 7 (superseded statements), 8 (owner questions) and D9, D10, D18, D24, D27, D28, D32, D33, D39, D80, D81, D82, D83, D85, D86 |
| Owner rulings | "Owner rulings (2026-10-01)" in [DECISIONS.md](DECISIONS.md): OR-1 to OR-21, which outrank every Proposed position here and in the research |
| Architecture | [ARCHITECTURE.md](ARCHITECTURE.md) sections 4.3 (hub ports, event names), 5.2, 5.7, 6.6, 7.2, 8, 9.3, 12 |
| Specs | [spec/platform.md](spec/platform.md), [spec/security-privacy.md](spec/security-privacy.md), [spec/finance.md](spec/finance.md), [spec/sales-crm.md](spec/sales-crm.md), [spec/supply-chain.md](spec/supply-chain.md), [spec/retail-commerce-marketing.md](spec/retail-commerce-marketing.md), [spec/people.md](spec/people.md), [spec/services.md](spec/services.md), [spec/ux.md](spec/ux.md), [spec/ai.md](spec/ai.md), [spec/integrations.md](spec/integrations.md), [spec/cli.md](spec/cli.md) (the grammar, flags, output, exit codes and plugin registry client of 2.4 and the plugin rows of 2.1), [spec/mobile.md](spec/mobile.md) (the Proposed `mileage` module id and `location` class) |
| Other | [GLOSSARY.md](GLOSSARY.md), [ROADMAP.md](ROADMAP.md), [BUSINESS.md](BUSINESS.md), [PRODUCT.md](PRODUCT.md), [LOCALIZATION.md](LOCALIZATION.md), [ENGINEERING.md](ENGINEERING.md), [../README.md](../README.md), [../LICENSE](../LICENSE) and [../NOTICE](../NOTICE) (Apache-2.0, OR-1) |
| Research | [research/23-kernel-model-architecture-decisions.md](research/23-kernel-model-architecture-decisions.md) D.2 to D.4 (R23 names), [research/11-crm-sales-subscriptions.md](research/11-crm-sales-subscriptions.md) MUST 6 (the retired `CreditNoteIssued`) |
