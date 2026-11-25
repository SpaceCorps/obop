# Odoo architecture and primitives: what obop must replicate

Baseline: Odoo 20.0 (Community source, branch `20.0`), researched 2026-10-01.

## Scope & method

- **Primary sources read directly:** a sparse clone of `odoo/odoo` branch `20.0` (`odoo/orm`, `odoo/modules`, `odoo/http`, `odoo/service`, `odoo/addons/base`, and the `mail`, `web`, `bus`, `rpc`, `base_automation`, `spreadsheet*` addons). Also the `odoo/documentation` repo (branch `20.0`, developer, administration, Studio and general-settings docs), the GitHub API (branches, languages, repo stats), the OCA `OpenUpgrade` repo, and the odoo.com docs, pricing and release-notes pages. File links use `https://github.com/odoo/odoo/blob/20.0/<path>`.
- **Secondary:** one Hacker News thread, for sentiment only.
- **Limits:** no web search engine was usable, so there was no broad survey of blogs or forums. The odoo.com pages were fetched through a summarising fetcher, and the 20.0 release-notes page came back truncated.
- **Tags:** [V] = verified in source or docs. [I] = inference. [G] = guess.
- **Enterprise edition:** no Enterprise code was read, and none should be. What Enterprise does is taken from public docs, or from the *absence* of a module in the Community tree.

## 0. Baseline: what is current (corrects stale knowledge)

| Fact | Status | Source |
|---|---|---|
| Odoo 20.0 is released (September 2026), standard support planned to Sept 2029. 19.0 is Sept 2025 (support to Sept 2028). 18.0 is Oct 2024 (support to Sept 2027). 17.0 standard support ends Sept 2026. SaaS 19.1, 19.2, 19.3 and 19.4 were released Jan, Mar, May and Jul 2026 (Odoo Online only, no extended support). | [V] docs table (re-checked 2026-10-01); the 20.0 release-notes page is headed "September 2026"; exact day of 20.0 not found | https://www.odoo.com/documentation/master/administration/standard_extended_support.html , https://www.odoo.com/odoo-20-release-notes |
| GitHub default branch is `20.0`. Nightly `odoo_20.0.20261001.tar.gz` exists (also `odoo_20.0.latest.tar.gz`). | [V] | https://github.com/odoo/odoo , https://nightly.odoo.com/20.0/nightly/src/ |
| Python >= 3.12 (max 3.14), PostgreSQL >= 16. The install docs agree ("Python 3.12 or later", "PostgreSQL ... 16.0 or above"). | [V] | https://github.com/odoo/odoo/blob/20.0/odoo/release.py , https://github.com/odoo/documentation/blob/20.0/content/administration/on_premise/source.rst |
| License LGPL-3 for Community (`LICENSE`, `release.py`, every checked manifest). Repo is ~52% Python and ~43.5% JS by bytes, ~54.8k stars, ~33.9k forks (live counts, 2026-10-01). | [V] | https://github.com/odoo/odoo/blob/20.0/LICENSE , GitHub API |
| The CE tree has 642 addon directories: 229 `l10n_*` and 413 others including tests. Not in CE: `web_studio`, `web_gantt`, `web_map`, `web_cohort`, `web_grid`, `documents`, `helpdesk`, `sign`, `planning`, `approvals`, `knowledge`, `account_accountant`, `voip`, `marketing_automation`, `appointment`, `hr_payroll`, any AI or MCP module. | [V] by absence in the git tree (re-checked 2026-10-01 with the GitHub tree API, not truncated) | https://github.com/odoo/odoo/tree/20.0/addons |

**Major architecture shifts in the last three releases.** All are from the ORM changelog, https://github.com/odoo/documentation/blob/20.0/content/developer/reference/backend/orm/changelog.rst, unless noted.

- **`ir.access` (19.4):** merges ACLs and record rules into one model.
- **Custom domain `access` operator (19.3).**
- **`Field.compute_sql` (19.1):** computed fields can be grouped and sorted.
- **`BinaryValue` (19.3):** base64 is no longer carried through the data flow.
- **SQL objects as attributes:** `models.Constraint`, `Index` and `UniqueIndex` (18.1).
- **JSON-2 API:** new in 19.0.
- **Report engines modularised:** prepares for `paper-muncher` (19.4).
- **New h11-based HTTP server (19.4).**
- **Thread-safe `ormcache` (19.4).**
- **ORM moved into `odoo/orm/*.py`:** 21.6k lines, with `odoo/fields` and `odoo/models` as thin re-export packages.
- **Stored `mail.tracking.value` no longer in core `mail` (20):** `addons/mail/models/mail_tracking_value.py` exists on `19.0` and not on `20.0`. The release notes say the tracking message is now generated on the fly and that admins who need the old values can install a dedicated module. That module is the new opt-in CE addon `mail_tracking` ("Discuss Tracking", `depends: ['mail']`, no `auto_install`), which still defines `mail.tracking.value`. `mail.track.mixin` still has the `tracking=True` field option. https://www.odoo.com/odoo-20-release-notes , https://github.com/odoo/odoo/tree/20.0/addons/mail_tracking
- **Owl 3 (20):** `addons/web/static/lib/owl/owl.js` reports `version = "3.0.0-alpha.49"`, with an Owl 2 to Owl 3 compatibility layer (self-described as a temporary bridge) in `addons/web/static/src/owl2/`. So Odoo 20 ships an **alpha** UI framework. `v3.0.0-alpha.49` is the newest tag of `odoo/owl`, flagged pre-release on GitHub, published 2026-09-08. https://github.com/odoo/owl/tags
- **Web client (20):** an offline mode with a 2 GB cap on offline data (oldest data is evicted beyond that). Odoo's docs describe it as a solution for short-term interruptions, not full or long-term offline use. https://github.com/odoo/documentation/blob/20.0/content/applications/general/offline_mode.rst
- **Other 20 addition:** a `card` view type (absent from `addons/web/static/src/views` on `19.0`, present on `20.0`).

## 1. Module system and manifest

**(a) What it does.** A module is a directory with `__manifest__.py`, a dict literal.

- **Keys and defaults** are in `_DEFAULT_MANIFEST` in https://github.com/odoo/odoo/blob/20.0/odoo/modules/module.py . The keys are `depends`, `data`, `demo`, `assets`, `auto_install`, `application`, `category`, `version`, `installable`, `external_dependencies`, `pre_init_hook`, `post_init_hook`, `uninstall_hook`, `post_load`, `countries`, `sequence` and others.
- **Mandatory keys:** `name`, `author` and `license` (per the comment on `_DEFAULT_MANIFEST`). In practice a missing `license` defaults to `LGPL-3` with a warning, and a missing `author` falls back to `contributors` or `maintainer` with a warning.
- **`base`:** the only module with empty `depends`. Any other empty list is coerced to `['base']`.
- **`auto_install`:** `True` means install when all `depends` are installed. A list means install when exactly those are installed. This "glue module" mechanism links apps, for example sale with stock.
- **Load order** (`odoo/modules/module_graph.py`): `base` is phase 0. Modules are then ordered by `(depth, name)`, where depth is the longest path to `base`. Update mode alternates "no init needed" and "needs init" phases. Modules named `test_*` load right after their last dependency.
- **States:** `uninstallable, uninstalled, installed, to upgrade, to remove, to install`, stored in `ir.module.module`, with dependency and exclusion tables.
- **Migrations** (`odoo/modules/migration.py`): `<module>/migrations/<version>/{pre,post,end}-*.py`, each exposing `migrate(cr, installed_version)`.
  - `pre` runs before module init, `post` after it, and `end` after all modules.
  - A `0.0.0` folder runs on every version change.
  - Server-specific versions such as `9.0.1.1` only run on that series.
- **Loading** (`odoo/modules/loading.py`): data files, then demo files, then hooks, per module.

**(b) Why users care.** Installing an app pulls its dependencies, glue modules give cross-app integration without a monolith, and upgrades are per module.

**(c) Rust design.**

- **Module as crate.** Each first-party module is a crate exposing a `Module` descriptor (name, version, `depends`, `auto_install`, models, data files, migrations). Modules self-register into a link-time registry (`inventory` or `linkme`-style). The binary is composed by Cargo features. A topological sort (`petgraph`) runs at boot and reproduces Odoo's depth phases.
- **Per-database state.** Installed-module state lives in Postgres, as in `ir.module.module`.
- **Third-party extensions.** Use sandboxed Wasm components or declarative packs (metadata, views, server actions), because Rust cannot hot-load native code safely. [I]
- **Migrations.** Versioned per-module steps with `pre/post/end` phases, implemented as SQL files or a Rust `Migration` trait.
- **Manifest.** A `module.toml` parsed with `serde`, with no code execution.

**(d) What Odoo gets wrong.**

- Dependency metadata is Python, so tooling must import or evaluate it.
- Module code and DB state are coupled: a code change needs `-u module`, and the registry rebuild is per database.
- Uninstall is fragile and relies on `@api.ondelete(at_uninstall=...)` conventions.
- Many glue modules (413 non-l10n addon dirs, including tests), and the CE/EE split hides part of the capability graph.

## 2. ORM

### 2.1 Registry, model definitions, inheritance

One `Registry` per database (`odoo/orm/registry.py`). Model *definitions* (Python classes in modules) are composed into one dynamic *model class* per model by Python MRO. The design comment at the top of `odoo/orm/model_classes.py` explains it.

| Mode | Declaration | Effect | Example |
|---|---|---|---|
| Extension in place | `_inherit='a'`, no `_name` | Adds fields and methods to the same model and table. Load order determines override order. | Most module extensions |
| Prototype (copy) | `_name='b'`, `_inherit=['a','b']` | New model and table that copies the fields and methods. | mixins into concrete models |
| Delegation | `_inherits={'res.partner':'partner_id'}` | Composition. The child exposes the parent's fields, stored on the linked parent row. | `res.users` inherits `res.partner` (`res_users.py:177`) |
| Abstract and transient | `AbstractModel`, `TransientModel` | Mixins with no table (`mail.thread`, `image.mixin`). Wizards with vacuumed rows. | |

Base attributes (`odoo/orm/models.py` ~L425-520):

- `_name`, `_rec_name`, `_rec_names_search`, `_order`
- `_parent_name` and `_parent_store`, which maintain `parent_path` for `child_of` and `parent_of`
- `_active_name`, `_fold_name`, `_check_company_auto`
- `_allow_sudo_commands`, `_table`, `_table_objects`, `_log_access`

### 2.2 Field types

All in `odoo/orm/fields*.py` and re-exported by `odoo/fields/__init__.py`.

| Type | Storage / notes |
|---|---|
| Boolean, Integer, Float (digits via `decimal.precision`) | Native columns. |
| **Monetary** | `numeric`, with `currency_field='currency_id'`. The value is a Python float rounded by the currency. |
| Char/Text/**Html** | `Html` is sanitised server-side. `translate=True` or a callable (term-level) stores **jsonb `{lang: value}`**. |
| Selection, Date, Datetime | Native. |
| **Json** | `jsonb`. |
| **Binary/Image** | `BinaryValue`. With `attachment=True` it is stored in `ir.attachment` (filestore). `cloud_storage*` addons add blob stores. |
| Many2one / One2many / Many2many | FK column, inverse-driven virtual field, join table. `ondelete`, `check_company` and `index` are options. |
| **Reference / Many2oneReference** | Reference stores a `"model,id"` string. Many2oneReference is `model_field` plus an integer id. There is no FK integrity. `mail_followers.py` says "no integrity check on model names for performance reasons". |
| **Properties / PropertiesDefinition** | Per-container schema plus `jsonb` values. Light user-defined fields without DDL. |
| `company_dependent=True` | Any field type stored as `jsonb {company_id: value}`, with `ir.default` as fallback. |

Field options worth replicating:

- **Computed values:** `compute`, `depends`, `depends_context`, `store`, `precompute`, `compute_sudo`, `recursive`, `inverse`, `search`, `related`.
- **New in 19.1:** `compute_sql`.
- **Indexes:** `index` in `btree`, `btree_not_null` or `trigram` forms.
- **Access and copying:** `groups`, `copy`.
- **Aggregation and grouping:** `aggregator`, `group_expand`.
- **Tracking:** `tracking` (added by `mail.track.mixin`).

### 2.3 Environment, recordsets, cache, flush

- **Recordset identity:** a recordset is `(env, _ids, _prefetch_ids)`.
- **`Environment` identity:** `(cursor, uid, context, su)`.
- **Cache:** a `Transaction` object (`odoo/orm/environments.py` ~L579) holds the per-transaction state. That state is `field_data` (the cache), `field_dirty` (unwritten changes), `tocompute` (pending recomputations), `protected`, and an access-permission cache.
- **Lazy reads with prefetch:** accessing a field on one record loads it for all `_prefetch_ids`.
- **Deferred writes:** writes go to the cache and reach SQL on `flush_model`, `flush_recordset` or `env.flush_all()`. Since 17.1, `execute_query` auto-flushes fields referenced by the SQL object it is given.
- **Invalidation:** `invalidate_model`, `invalidate_recordset`, `invalidate_all`.
- **`@ormcache`:** registry-level caches invalidated across processes via `orm_signaling_*` tables, previously sequences (`registry.py` L1078-1113).
- **Context:** `with_context`, `with_user`, `sudo()`. In 20.0 `env.cache` is deprecated in favour of field methods.
- **`create()`:** `@api.model_create_multi`.
- **`write()`:** `_write` is the low-level SQL path.
- **`unlink()`:** `@api.ondelete`.
- **Constraints:** `@api.constrains`, plus `models.Constraint`, `models.Index` and `models.UniqueIndex` (replace `_sql_constraints`).
- **`copy_data`.**
- **Onchange:** `@api.onchange` methods are run from `BaseModel.onchange`, which is **implemented in the `web` addon** (`addons/web/models/models.py` L2189). It builds a snapshot on `NewId` virtual records, applies the changes, and diffs. Every form field change can be a server round-trip.
- **Sequences** (`ir_sequence.py`): `standard` uses a PostgreSQL `SEQUENCE`. `no_gap` uses `SELECT ... FOR UPDATE NOWAIT`. Prefix/suffix patterns and date ranges are supported.
- **Display names:** `display_name` is a computed field from `_rec_name`. `name_get` was deprecated in 16.4/17.0 (a warning stub remains on `17.0`) and is gone on `18.0`. Name search is `_search_display_name` over `_rec_names_search`.

### 2.4 Domains, search, grouping

- **Domain language:** `odoo/orm/domains.py`, a `Domain` AST with normalisation and optimisation levels.
  - Logical nodes: Polish-notation `&`, `|`, `!`.
  - Leaf operators: `= != < > <= >= in not in like ilike =like =ilike`.
  - Relational operators: `any`, `not any`, `child_of`, `parent_of`.
  - `access` operator (19.3+): checks the comodel's permissions inside a domain.
  - Dynamic dates (19.0).
  - Custom SQL-injecting domains (18.4).
  - Related-path traversal such as `partner_id.country_id`.
- **Search methods:** `_search` returns a `Query`. `search_fetch` combines search and read.
- **Grouping:** `_read_group(domain, groupby, aggregates, having, offset, limit, order)` in `odoo/orm/models.py` ~L1996.
  - Granularities: day, week, month, quarter, year, plus `*_number` date parts.
  - Aggregates are PostgreSQL aggregates plus `count_distinct` and `recordset`.
  - `GROUPING SETS` support for pivot (19.0).
  - The public web API is `formatted_read_group`.
  - `read_group` is deprecated since 18.2.

### 2.5 Rust design option (ORM)

- **Metadata-first.** A runtime `ModelRegistry` holds models, fields, constraints and indexes. It is the single source of truth for SQL DDL, the API schema, view validation, security and customisation. Core models are typed Rust structs (derive macro) that register into it. User-defined models and fields exist only as registry rows. [I]
- **Domains as data.** A serialisable enum AST, normalised, then compiled to SQL (`sea-query` or hand-rolled over `sqlx`). The same AST is reused by UI filters, record security, automation conditions and the API.
- **No implicit lazy I/O.** Loading is explicit: read specs (like Odoo's own `web_read` specification), DataLoader-style batching, and typed projections. This keeps the batch-prefetch benefit without hidden N+1.
- **Unit of work.** Write-behind change tracking per transaction, flushed before queries that could observe the changes. The compute DAG of stored fields is built at boot from declared `depends`. Recomputation is batched at flush. A `compute_sql` equivalent lets computed fields be grouped and sorted.
- **Money.** `Decimal` or integer minor units, never floats. Note Odoo's `Monetary` is `numeric` in SQL, but the Python value is a float.
- **Translations.** `jsonb` per translatable field, searchable with trigram indexes, which is what Odoo does.
- **Company-dependent values.** `jsonb` map or a side table.

### 2.6 What Odoo gets wrong (ORM)

- **Hidden global state.** A per-transaction mutable cache plus implicit lazy loading plus `sudo()` makes correctness context-dependent. One confirmed bug class: "cache pollution for x2many fields" (fixed in 19.3, changelog).
- **`sudo()` is pervasive.** A grep of the 20.0 source (tests excluded, re-run independently) finds 433 `.sudo(` calls in `addons/mail` (only 134 `# sudo:` justification comments exist in that addon) and 220 in `odoo/addons/base`.
- **API churn.** Nearly every release breaks ORM signatures: `name_get`, `read_group`, `_sql_constraints`, `group_operator`, `attrs`, `env.cache`, Binary base64, `request` in models. The changelog is the evidence.
- **Floats for money.**
- **Onchange emulation** on virtual `NewId` records is complex and server-bound.
- **Overlapping grouping APIs** (`read_group`, `_read_group`, `formatted_read_group`).
- **One registry per database** in memory; the server sizes an LRU from the memory limit (`service/server.py`).

## 3. Security

**(a) What it does.**

- **Users and groups.**
  - `res.groups` has `implied_ids` (transitive) and `implied_by_ids`.
  - Also present: `privilege_id` (`res.groups.privilege`, exists on `19.0`) and `api_key_duration`.
  - Built-in groups are defined in `odoo/addons/base/security/base_groups.xml`: `group_user` (internal), `group_portal`, `group_public`, `group_everyone` (new), `group_erp_manager`, `group_system`, `group_no_one` (debug), `group_multi_company`, `group_multi_currency`.
  - `res.users` inherits `res.partner`.
  - `share` marks portal and public users.
- **Model and record access in 20.0: `ir.access`** (`odoo/addons/base/models/ir_access.py`, data in `odoo/addons/base/security/ir.access.csv`). Fields: `model_id`, `group_id`, `operation` (a subset of `crud`), `domain`, `active`.
  - A row **with a group is a permission**. A row **without a group is a global restriction**.
  - The effective domain is assembled in `BaseModel._access_domain` (`odoo/orm/models.py` ~L3603) as **`Domain.OR(permissions) & Domain.AND(restrictions)`**.
  - `_inherits` parents add restrictions: `(parent_field, 'any', parent_access_domain)`.
  - Evaluation context: `user`, `time`, `company_ids`, `company_id`.
  - All active rows are cached once under `ormcache(cache='stable')` (`IrAccess._get_all_access`). The computed per-model domain is cached per `(operation, uid, allowed_company_ids)` (`_access_context`).
  - Module overrides exist. The `mail` addon's `ir_access.py` makes access rows mail threads with tracked fields.
- **Before 19.4** these were two systems: `ir.model.access` (CSV, per group, CRUD booleans) and `ir.rule` (domain, group or global). The developer docs for `security.rst` still describe the old pair, so **docs lag code**. [V] https://github.com/odoo/documentation/blob/20.0/content/developer/reference/backend/security.rst
- **Field-level:** `groups=` on fields (with `fields.NO_ACCESS` as the "nobody" value) and on view nodes and menus.
- **Multi-company rules:** a domain over `company_ids`, typically `['|', ('company_id','=',False), ('company_id','parent_of', company_ids)]`.
- **Escalation:** `sudo()`, `with_user()`.
- **`_allow_sudo_commands = False`** on sensitive models stops x2many Commands under sudo (for example `ir.access`, `res.groups`).
- **HTTP auth types:** routes declare `auth='none'|'public'|'user'|'bearer'` (`ir_http.py`).
- **API keys:** `res.users.apikeys` with a scope (`rpc`), a 160-bit key (`API_KEY_SIZE = 20` bytes), and a per-group cap on lifetime (`res.groups.api_key_duration`, 90 days on `base.group_user`, which matches the docs' "not more than three months"; system users are exempt in code).
- **Portal:** `portal` users plus per-record `access_token`.

**(b) Why users care.** Row-level rules ("salespeople see only their own leads"), portal customers seeing only their own documents, and company isolation are what make one shared database safe.

**(c) Rust design.**

- **Access as data.** Store `Access{model, group?, ops, domain}` in the same `(permission OR) AND (restriction)` algebra. Compile the result into every query's WHERE clause, cached per `(model, op, group-set, company-set)`.
- **Typed privilege.** The environment type encodes privilege (`Env<User>`, `Env<Elevated>`). Elevation is an explicit, audited call and is not a boolean `su` flag.
- **Field groups.** Field-level restrictions live in registry metadata and are enforced in projection.
- **Defense in depth.** Optionally mirror domains into Postgres RLS. [I]
- **Portal tokens.** Capability tokens with scope, expiry and revocation.
- **Domain evaluation.** The domain language is data. Do not evaluate stored expressions with `eval`.

**(d) What Odoo gets wrong.**

- The two-system split (ACL CSV plus rules with global and group semantics) confused everyone. Odoo itself merged them in 19.4.
- Domains are stored as Python-expression strings and evaluated with `safe_eval`.
- Access domains are injected into every search, and heavy ones such as `mail.message` need special handling (`_access_domain_heavy`).
- `sudo` sprawl.
- Permission is implicit in record-level Python overrides (`has_access`), and the group-introspection code carries a FIXME that it ignores overrides.

## 4. Views, actions, menus, QWeb

**(a) What it does.**

- **View model.** `ir.ui.view` (`ir_ui_view.py`) has `model`, `type`, `arch_db` (translatable XML), `arch_fs`, `arch_prev`, `inherit_id`, `mode` (`primary` or `extension`), `priority` (default 16), `groups`.
- **View types in CE 20.0:** list, form, graph, pivot, calendar, kanban, search, **card** (new), qweb, plus `activity` (in `mail`) and `hierarchy` (`web_hierarchy`).
- **Enterprise-only:** gantt, map, cohort, grid. The Studio docs list gantt, map and cohort (not grid): https://github.com/odoo/documentation/blob/20.0/content/applications/studio/views.rst . `web_gantt`, `web_map`, `web_cohort` and `web_grid` are all absent from the CE tree, so grid is Enterprise by inference from absence only.
- **Inheritance.** `odoo/tools/template_inheritance.py`. Extension specs are XPath or `<field name=...>` locators with `position` = `inside | after | before | replace | attributes | move`. The resolved arch is validated against model metadata (`view_validation.py`).
- **Actions.** `ir.actions.actions` base, with `act_window` (res_model, view_mode, domain, context, views), `act_window_close`, `act_url`, `server`, `client`, `report` and `todo`. `ir.embedded.actions` (present since 18.0) attaches sub-actions to a parent action.
- **Menus.** `ir.ui.menu` is a tree with groups and an action reference.
- **Server-side QWeb.** `ir_qweb.py` (3.2k lines) compiles an XML template into a Python generator function cached in `ormcache`. Directives are `t-if`, `t-foreach`, `t-call`, `t-out`, `t-att-*`, `t-groups`. It serves website pages, emails and PDF reports. A separate client-side QWeb is part of Owl.
- **Client view API.** `web_read`, `web_search_read`, `web_save`, `onchange`, `formatted_read_group`, `read_progress_bar` and the search-panel RPCs are all **spec-driven**. The client sends a nested field specification and gets exactly those fields back (`addons/web/models/models.py`).

**(b) Why users care.** Lists, forms and kanban over any model, plus customisation by adding fields to views, are the product.

**(c) Rust design.**

- View architectures become **typed, versioned documents** (serde/JSON), not raw XML. Inheritance is expressed as structured patch operations (insert_after, replace, set_attr, move) addressed by stable node ids or names, not XPath positions. [I]
- The server resolves inheritance into a canonical tree per `(view, groups, lang)` and caches it. It validates against the model registry at module load.
- For printed and emailed documents use a sandboxed template engine (`minijinja`) with autoescape.
- The frontend (Vite+) renders from the resolved spec, with a registry of field, view and action components.
- Implement gantt, map, cohort and timeline natively. They are Enterprise-only in Odoo, so this is a product differentiator. [I]

**(d) What Odoo gets wrong.**

- XPath-based inheritance is positional and breaks when upstream views change.
- View arch lives in both files and the database, with soft/hard reset machinery (`arch_prev`, `arch_fs`).
- Two template engines (Python QWeb, JS QWeb).
- Features are gated by edition.

## 5. Mail stack

**(a) What it does.**

- **`mail.thread`** (abstract mixin, `addons/mail/models/mail_thread.py`, **5,446 lines**) adds `message_ids`, `message_follower_ids`, `message_post`, tracking, routing (`message_route`, `message_process`, `message_new`, `message_update`) and notification fan-out (`_notify_thread_by_inbox`, `_by_email`, `_by_web_push`).
- **`mail.message`** carries the chatter content. It has:
  - `model` and `res_id` (a `Many2oneReference`), `parent_id`, `subject`, `body` (Html)
  - `message_type` (`email, comment, email_outgoing, notification, auto_comment, out_of_office, user_notification, tracking`)
  - `subtype_id`, `author_id` or `author_guest_id`, `partner_ids`, `attachment_ids`, `is_internal`
  - reactions and link previews
- **`mail.notification`** holds per-recipient delivery state.
- **`mail.followers`** is unique on `(res_model, res_id, partner_id)` and holds `subtype_ids`.
- **`mail.message.subtype`** decides which event types a follower is notified of.
- **`mail.activity`** is a scheduled to-do on any record, with type, due date, assignee and plan. `mail.activity.mixin` adds state (overdue, today, planned) to a model.
- **`mail.alias` / `mail.alias.domain`** maps `alias@domain` to a target model with defaults (`alias_defaults`), a contact policy (`alias_contact`) and bounce content. This is how email-to-lead and email-to-ticket work.
- **`mail.mail`** is the outgoing queue, sent by cron.
- **`mail.template`** has `render_engine` in `inline_template | qweb | qweb_view`, per-language rendering, and a template reset mixin.
- **Other pieces:** `fetchmail` incoming, Discuss channels, calls, polls, roles, canned responses, web push.
- **Tracking change in 20:** `mail.tracking.value` is no longer part of core `mail`. Tracked changes are built dynamically for the chatter, and the stored model lives on only in the opt-in `mail_tracking` addon. The `tracking=True` field option still exists.

**(b) Why users care.** One conversation log per record, with followers, email in and out, and activities, ties every app together.

**(c) Rust design.**

- Model the thread as a *capability* (`trait Threaded`) plus tables keyed by `(thread_kind, thread_id)`.
- Inbound: `mail-parser` plus IMAP (`async-imap`) or an SMTP-receiving webhook. A route table replaces the stringly alias routing.
- Outbound: a **transactional outbox** table drained by workers, sent via `lettre`.
- Templates: `minijinja` in a sandbox, with per-language variants.
- Activities: a first-class scheduled-task table with states derived at query time.
- Tracking: store a compact JSON diff per message, not a row per tracked field. [I]

**(d) What Odoo gets wrong.**

- A 5.4k-line god mixin inherited by nearly every business model.
- Polymorphic references without FK integrity.
- Per-field stored tracking values, which Odoo made opt-in (moved to a separate `mail_tracking` addon) only in 20.
- Mail sending is coupled to the request transaction [I].

## 6. Automation

**(a) What it does.**

- **Server actions** (`ir.actions.server`): `state` in `object_write, object_create, object_copy, code, webhook, multi`. The mail, sms and other addons add email, activity, followers and SMS. The `code` type runs arbitrary Python stored in the database through `safe_eval`.
- **Automated actions** (`base_automation`): 18 triggers including `on_create`, `on_create_or_write`, `on_unlink`, `on_change`, `on_time`, `on_time_created`, `on_time_updated`, `on_stage_set`, `on_state_set`, `on_tag_set`, `on_message_received`, `on_message_sent`, `on_webhook`. Each trigger has a domain filter and a list of server actions.
- **Cron** (`ir_cron.py`): `ir.cron` rows.
  - Workers claim jobs with `FOR NO KEY UPDATE SKIP LOCKED`.
  - `ir.cron.trigger` holds one-off wake-ups, and `ir.cron.progress` holds progress.
  - A failure-count policy deactivates jobs after at least 5 consecutive failures spanning at least 7 days.
  - `--max-cron-threads` defaults to 2.
  - Since 18.3 the API supports progress notification with batch commits.

**(b) Why users care.** Business users build "when stage = Won, create a task and notify the manager" without developers.

**(c) Rust design.**

- A Postgres job queue using `SKIP LOCKED` and LISTEN/NOTIFY for wake-up. Cron entries are rows that enqueue jobs.
- Automation rules are **declarative**: trigger, domain condition and action list.
- Expression evaluation uses a sandboxed non-Turing-complete language such as CEL (crate `cel`, the renamed successor of `cel-interpreter`) or Rhai. Custom code runs as a sandboxed Wasm action. There is no host-language `eval`.
- Webhook action and webhook trigger are built in.

**(d) What Odoo gets wrong.** Python code stored in the database (a security and upgrade hazard). Three overlapping concepts (server action, automated action, cron) with different UIs. Cron tied to worker processes and polling.

## 7. Reporting

- **PDF reports.** `ir.actions.report` with `report_type` (qweb-pdf, qweb-html, qweb-text), paperformat and QWeb templates. `ir_actions_report.py` L788 defaults the engine to `wkhtmltopdf`. [V] The upstream wkhtmltopdf repository is **archived**, last push 2022-11-22. https://github.com/wkhtmltopdf/wkhtmltopdf
- **Engine modularisation.** 19.4 modularised engines "and prepare paper-muncher" (changelog). `paper-muncher` is Odoo's own HTML/CSS to PDF or image engine, written in C++, LGPL-3.0, ~300 stars, actively pushed (last push 2026-09-30; latest release `v0.8.0`, 2026-09-18, so still pre-1.0). The 20.0 core source (`odoo/`) has no `paper-muncher` references, and `_get_pdf_engine` still defaults to `wkhtmltopdf`. Its production maturity was not assessed. https://github.com/odoo/paper-muncher
- **Spreadsheet dashboards.** CE includes `spreadsheet` and `spreadsheet_dashboard` (both LGPL-3) with the `o_spreadsheet` JS library. Data sources bind pivots, lists and charts to models through RPC. Models are `spreadsheet.dashboard`, `.group`, `.share` and `.favorite.filters`. The editing side (`documents_spreadsheet`) is Enterprise (unverified beyond its absence from the CE tree; Enterprise docs were not read).
- **Pivot and graph views** sit on `_read_group`/`formatted_read_group` with GROUPING SETS.

**Rust design.**

- A `PdfEngine` trait with adapters. A Rust-native typesetter (`typst`) handles built-in business documents. A headless-Chromium (`chromiumoxide`) or `paper-muncher` adapter handles HTML-authored customer reports.
- Dashboards: a metadata-defined query layer (group-by plus aggregates compiled to SQL), with a spreadsheet engine later. `o_spreadsheet` is LGPL-3 but tightly coupled to Owl. [I]

**What Odoo gets wrong.** HTML to PDF relied on an abandoned WebKit-based tool for years. The report designer is Enterprise (Studio). PDF generation needs spare workers (the `workers` engine state in the code).

## 8. Web client and asset pipeline

- **Owl.** Odoo's own component framework, https://github.com/odoo/owl (TypeScript, newest tag `v3.0.0-alpha.49`, a pre-release published 2026-09-08). The web client is a tree of Owl components.
- **Registries.** `registry.category("fields" | "views" | "services" | "actions" | ...)` are the extension points. Features look up definitions there. https://github.com/odoo/documentation/blob/20.0/content/developer/reference/frontend/registries.rst Extensions also use runtime `patch()` monkey-patching.
- **Asset bundles.** Manifest `assets` keys map bundle names (`web.assets_backend`, ...) to globs. Directives are `append, prepend, before, after, replace, remove, include` (`ir_asset.py`). Bundles are concatenated and minified at runtime and cached in `ir.attachment`. SCSS compiles via libsass. ES modules are converted to `odoo.define` modules by a **regex-based transpiler** (`odoo/tools/js_transpiler.py`, no real parser).
- **Offline mode (20):** queued create/edit/delete of previously opened records, 2 GB cap, and a service worker.

**Rust / Vite+ design.**

- Replace the runtime bundler with **build-time composition**: a Vite+ plugin discovers each module's `frontend/` directory and generates virtual registry modules. No runtime patching of other modules' internals.
- Keep the *registry* idea: typed registries for fields, views, actions, services and systray items.
- The server publishes a JSON schema, resolved views and a generated TypeScript client.

**What Odoo gets wrong.** A hand-rolled module system and transpiler, an alpha UI framework in a stable release, fragile runtime patching, and per-database server-side bundling.

## 9. Multi-company, multi-currency, multi-language

- **Companies.** `res.company` is a tree (`parent_id`, `child_ids` as "Branches", `parent_path`, `root_id`). The UI switcher sets `allowed_company_ids` in the context. Rules use `env.companies` and `env.company`. `_check_company_auto` plus `check_company=True` relational fields reject cross-company links. Currency rates live on the root company. Odoo Online gates multi-company behind the Custom plan: https://github.com/odoo/documentation/blob/20.0/content/applications/general/companies/multi_company.rst
- **Currencies.** `res.currency` has `rounding` and `decimal_places`. `res.currency.rate` is date-based and company-aware, looked up with a `LEFT JOIN LATERAL`. `_convert(amount, to_currency, company, date, round)` converts. Monetary fields reference a currency field. `base.group_multi_currency` toggles the UI.
- **Languages.** `res.lang` holds formats. `translate=True` stores jsonb per field. The user language comes from context `lang` (fallback `en_US`). PO files are per module.

**Rust design.**

- `Money{amount: Decimal, currency: CurrencyCode}`.
- `CompanyScope` is a required part of the environment, not an ambient context key.
- Branch hierarchy stored with a path column or ltree.
- Translations use `Translated<T>` with jsonb storage. Formatting via ICU4X. UI strings via Fluent or gettext.

**What Odoo gets wrong.** Float arithmetic plus rounding helpers. Company scope is an ambient context value, and "company mismatch" errors are among the most common access errors (the access error code has dedicated multi-company advice).

## 10. External API

| API | Status | Detail |
|---|---|---|
| XML-RPC `/xmlrpc/2/{common,object}`, JSON-RPC `/jsonrpc` | Deprecated in 19, **scheduled for removal in Odoo 22** (fall 2028; Online 21.1, winter 2027). The `db` service was already removed in Odoo 20 (Online 19.1). | `addons/rpc/controllers/__init__.py` (`RPC_DEPRECATION_NOTICE`), https://github.com/odoo/documentation/blob/20.0/content/developer/reference/external_api.rst ("Migrating from XML-RPC / JSON-RPC") |
| **JSON-2** `POST /json/2/<model>/<method>` | New in 19.0 | `addons/rpc/controllers/json2.py` |

JSON-2 details, from https://github.com/odoo/documentation/blob/20.0/content/developer/reference/external_api.rst :

- **Auth:** `Authorization: bearer <api key>`.
- **Database:** optional `X-Odoo-Database` header.
- **Body:** `{ids?, context?, ...method kwargs}`.
- **Response:** 200 with the JSON return value, or a 4xx/5xx with `{name, message, arguments, context, debug}`.
- **Keys:** described in section 3, including programmatic generation through `res.users.apikeys.generate`.
- **Transaction behaviour:** the route opens a read-only cursor only for methods decorated `@api.readonly`; everything else gets a read-write transaction (`json2.py`).
- **Documentation:** each database serves `/doc`, generated by the `api_doc` addon (playground with examples).
- **Method exposure:** `@api.model` methods take no `ids`. `@api.private` hides methods from RPC.
- **Commercial restriction:** "Access to data via the external API is only available on *Custom* Odoo pricing plans." Odoo Online pricing, per user per month, read from the page HTML on 2026-10-01 (verify live): with the Yearly toggle, Standard is EUR 19.90 and Custom EUR 35.90 (list prices EUR 24.90 and EUR 44.90; the lower figure is a discount "valid for 12 months, for initial users ordered"). With the Monthly toggle, Standard is EUR 24.90 and Custom EUR 44.90 (list EUR 31.10 and EUR 56.90). Light User is EUR 7.90. The Custom plan lists "Odoo Studio / Agentic AI", "Multi-Company" and "External API". The multi-company docs confirm that enabling multi-company on Standard triggers an upsell to Custom. https://www.odoo.com/pricing-plan
- **Odoo 20 release notes:** mention MCP connectivity to the database ("Connect to your database via MCP") and say all AI features now need credits bought via IAP, but no MCP or AI module is in the CE tree. So it is Enterprise or Online only. [I]

**Rust design.** Keep the useful idea: one uniform `POST /api/<model>/<method>` RPC surface derived from the registry. Add a generated OpenAPI 3.1 spec, typed error bodies, idempotency keys, cursor pagination and webhooks. Also offer resource-style REST for CRUD. Scoped, expiring API keys as in Odoo. An MCP server generated from the same registry.

**What Odoo gets wrong.** Untyped kwargs RPC, errors shaped as Python exception names and tracebacks, three RPC generations in six years, and API access as a paid-plan feature. HN commenters called the old API poorly documented and the removal abrupt (secondary, thread of 2025-12-31; one commenter noted that each database serves `/doc`; the claim there that removal comes in the "next major version" is wrong, it is Odoo 22): https://news.ycombinator.com/item?id=46439993

## 11. Data files, demo data, upgrades

- **Data files.** Manifest `data` and `demo` lists.
  - XML tags in `odoo/tools/convert.py`: `record`, `menuitem`, `template`, `function`, `delete`, `asset`.
  - CSV files are named for the model (`ir.access.csv`, `res.country.state.csv`) with an `id` column for the external ID.
  - External IDs live in `ir.model.data(module, name, model, res_id, noupdate)`.
  - `noupdate="1"` and `forcecreate` control re-loading. `eval=` expressions use `ref()` and `Command`.
  - A data `record` can nest One2many children.
  - `is_standard` on access rows is derived from the external-ID module (anything not `__export__`, `__custom__` or `studio_customization`).
- **Demo data.** Not loaded by default since 18.3. `odoo populate` generates fake data (20).
- **Upgrades.**
  - Per-module scripts, see section 1.
  - `odoo/upgrade_code/` ships **source codemods** for custom modules (for example `19.4-00-ir-access.py`, `owl3-migration.py`).
  - Odoo's own database upgrade service is free only for Enterprise: https://github.com/odoo/documentation/blob/20.0/content/administration/upgrade.rst (L394).
  - For Community, the OCA project `OpenUpgrade` (AGPL-3.0, https://github.com/OCA/OpenUpgrade ) carries migration scripts. Branch `19.0` has 329 entries under `openupgrade_scripts/scripts` (`18.0` has 430). **Branch `20.0` holds a single "Initial commit" dated 2026-09-28 and no scripts (re-checked 2026-10-01: still the only commit, no `openupgrade_scripts/` directory).** So there is currently no open-source upgrade path to Odoo 20. [V]

**Rust design.**

- Seed data as typed RON, TOML or YAML/JSON with stable external IDs (`module.name`).
- Idempotent diff-apply. A per-record ownership flag (module-owned versus customised) replaces `noupdate` plus the `is_standard` heuristic.
- A migration framework with `pre/post/end`, SQL-first and checksummed.
- A codemod tool for extension authors on each breaking release.

**What Odoo gets wrong.** The upgrade service is a commercial moat. DB evolution is coupled to arbitrary Python, data files use `eval=`, and the customisation layer (`studio_customization`, `__export__`) is mixed with the module layer.

## 12. Server, workers, websocket bus

- **Server modes** (`odoo/service/server.py`): `ThreadedServer` (`workers=0`, default), `PreforkServer` (`--workers N`, HTTP workers plus cron workers) and a gevent server for websockets on port 8072.
- **Limits:** soft memory 2048 MiB, hard 2560 MiB, CPU time 60 s, real time 120 s, `limit_request` 65,536 (`odoo/tools/config.py`). Workers are recycled after a request when they exceed the soft limit.
- **Bus.** The `bus` addon stores notifications in `bus.bus` and signals via `pg_notify` (payload size limit configurable). `/websocket` fans out to channels (partner, user, guest, record). Max websocket message is 1 MiB. `bus.listener.mixin` lets models publish.
- **Registry signalling** between processes uses `orm_signaling_*` tables.

**Rust design.**

- One async binary (tokio, axum, sqlx), with roles chosen by flags: `http`, `cron`, `bus`, `all`.
- Postgres LISTEN/NOTIFY to an in-process broadcast to WebSocket or SSE.
- No RSS-based recycling. Backpressure and per-request budgets instead. Cache invalidation via NOTIFY.

**What Odoo gets wrong.** The prefork model exists to contain Python's GIL and memory growth [I]. Websockets need a separate gevent process and port, and HTTP, cron and bus are deployed as three separate concerns.

## 13. Studio (Enterprise, no-code customisation)

Public docs only (https://github.com/odoo/documentation/blob/20.0/content/applications/studio/).

- **What it does:**
  - Custom models (technical names `x_...`, Studio-created fields `x_studio_...`) with up to 14 "suggested features" bundles.
  - Form, list, kanban, search, activity, calendar, pivot, graph, gantt, cohort and map editors.
  - Approval rules on buttons, automated-action UI, a PDF report editor, and **export of customisations as a module ZIP**.
- **Mechanism, inferred from CE model code** ([V] for the CE pieces, [I] for the Studio composition):
  - Rows in `ir.model`/`ir.model.fields` with `state='manual'`, so columns are created at runtime.
  - Inherited `ir.ui.view` records with external IDs in the `studio_customization` namespace.
  - `ir.access` rows.
  - CE already contains the hooks: `ir_model.py` `state` selection and the `is_standard` exclusion list.
- **Rust design.** Treat customisation as a **separate layer** from module-owned metadata, both stored in the registry tables. Custom models and fields are real registry rows that generate DDL (an `x_` namespace), so they are queryable, indexable and API-visible. The light alternative is a Properties-style `jsonb` extension column.
- **What Odoo gets wrong.** The feature is Enterprise-only and paid (Custom plan), and it edits XPath-inherited XML, which is fragile across upgrades.

## Confidence & gaps

**High confidence [V]:**

- Everything derived from reading 20.0 source: the `ir.access` algebra, field list, module graph, migration layout, JSON-2 and the RPC deprecation, cron claiming, worker limits.
- Version and support dates (odoo.com docs table).
- The CE/EE boundary, derived by absence in the git tree.
- OpenUpgrade's 20.0 branch state as of 2026-09-28/30 (re-confirmed 2026-10-01).

**Medium confidence:**

- The Odoo 20 release notes come from a truncated page. Items on record rules, offline mode, MCP and tracking were cross-checked against source where possible: `ir.access` yes, tracking model moved out of core `mail` into the opt-in `mail_tracking` addon yes, offline docs yes, MCP no.
- The odoo.com edition-comparison table could not be read, so I did not state any Enterprise-only claim that the source tree could not back by absence.
- Pricing figures were re-read from the raw pricing-page HTML on 2026-10-01 and the yearly/monthly mapping is now resolved (section 10). Prices change, so re-check before quoting.
- The `sudo` counts are grep counts on the sparse checkout (mail and base only, tests excluded). They are illustrative, not whole-repo totals, though an independent re-run gave the same numbers.

**Gaps:**

- The exact 20.0 release day was not found.
- No Odoo instance was run, and no performance measurements were taken.
- Not studied: accounting, stock and manufacturing internals, the website builder, POS/IoT, and Owl internals beyond version and registry.
- Studio internals come from docs only.
- Rust crate suggestions (`inventory`, `petgraph`, `sea-query`, `sqlx`, `minijinja`, `cel-interpreter`, `typst`, `chromiumoxide`, `lettre`, `async-imap`, `mail-parser`, ICU4X, Fluent) were **not** checked for current status or versions by the author. The fact-check pass looked at crates.io on 2026-10-01 for most of them: `sqlx` 0.9.0, `sea-query` 1.0.2, `minijinja` 2.24.0, `typst` 0.15.1, `chromiumoxide` 0.9.1, `lettre` 0.11.23, `async-imap` 0.12.0, `mail-parser` 0.11.9, `inventory` 0.3.24, `linkme` 0.3.37, `petgraph` 0.8.3, `rhai` 1.26.1 and `cel` 0.14.5 (all exist and most were updated in 2026; `petgraph` last released 2025-09-30). `cel-interpreter` has not been released since 0.10.0 (2025-07-23); use `cel`. ICU4X (`icu` 2.3.1) and Fluent (`fluent` 0.17.0, last released 2025-05) were also found. Fitness for obop was not assessed, so the Rust and Vite+ researchers should still confirm them.
- Sentiment evidence is a single HN thread.

## Implications for obop

**MUST**

1. **Build a metadata-first model registry.** Models, fields, constraints, indexes, computed-field dependencies and translation flags live in a runtime registry. ORM, API schema, view validation, security and customisation all derive from it. Core models are typed Rust structs that register into it.
2. **Adopt Odoo's domain language as the lingua franca,** as a serialisable AST. It is used by UI filters, security, automation, API queries and reports. Include `any`/`not any`, `child_of`/`parent_of`, dynamic dates and an `access`-style cross-model permission operator. Never `eval` strings.
3. **Implement the `ir.access` algebra from day one.** Group-bound rows are permissions (OR-ed). Group-less rows are restrictions (AND-ed). Add field-level groups, `_inherits`-style parent restrictions, multi-company restrictions and a `sudo` that is a typed, audited escalation.
4. **Use exact decimals for money.** `Money{Decimal, Currency}` everywhere. Rates are company-scoped and date-based. No floats.
5. **Ship a module system with a manifest, dependency graph, `auto_install` and per-module versioned migrations** with `pre/post/end` phases. Data has stable external IDs (`module.name`) and an ownership flag that separates module data from customisations.
6. **Provide stored computed fields** with declared dependencies, batched recompute at flush, and a SQL-level computed expression so they can be grouped and sorted.
7. **Make multi-company, multi-currency and multi-language core.** Company scope is an explicit part of the environment. Company-dependent values and translations are first-class field options.
8. **Treat chatter as core.** Record thread, followers, subtypes, activities, aliases, templates, outbound outbox and inbound email routing are the integration glue across apps.
9. **Ship an open, documented API with no plan gating.** A JSON-2-style uniform method-call endpoint plus generated OpenAPI 3.1, scoped expiring API keys, and typed errors.
10. **Stay clean-room.** Odoo CE is LGPL-3, so reading it is fine. Do not copy its code, view XML, translations or assets. Do not use Enterprise sources. CE lacks Studio, gantt, map, cohort, documents, helpdesk, sign, planning, accountant and AI entirely, so those must be designed from public behaviour docs.
11. **Target PostgreSQL only,** version 16 or later, like Odoo. Use jsonb, trigram indexes, `SKIP LOCKED`, LISTEN/NOTIFY and `GROUPING SETS`.

**SHOULD**

12. **Make views typed, versioned JSON** with structured patch operations addressed by stable node ids, not XPath. Resolve and cache per `(view, groups, lang)`. Validate against the registry at module load. Support form, list, kanban, search, pivot, graph, calendar, activity, hierarchy and card. Build gantt, map and cohort natively as a differentiator.
13. **Compose modules statically** (Cargo features plus link-time registration) for first-party code. Offer sandboxed Wasm or declarative packs for third-party extensions.
14. **Run one async binary with role flags** (`http`, `cron`, `bus`, `all`). Use a Postgres job queue (`SKIP LOCKED`) and LISTEN/NOTIFY to WebSocket or SSE.
15. **Make automation declarative** (trigger, domain condition, action list). Use a sandboxed expression language and Wasm for custom code. Include webhook actions and triggers.
16. **Define a `PdfEngine` trait** with a Rust-native engine for built-in documents and an HTML-to-PDF adapter, keeping `paper-muncher` as an option.
17. **Compose the frontend at build time:** a Vite+ plugin generates typed registries (fields, views, actions, services) from module `frontend/` folders. Generate the TypeScript client from the registry. Avoid runtime monkey-patching.
18. **Ship upgrade tooling,** meaning codemods for extension authors and a public migration test suite per release. This directly addresses the gap where Odoo 20 has no open upgrade path yet.
19. **Expose an MCP server** generated from the same registry and access rules.

**COULD**

20. Mirror access domains into Postgres row-level security as defense in depth.
21. Implement a Properties-style `jsonb` extension field early for cheap user-defined fields, before full runtime DDL customisation.
22. Reuse the LGPL-3 `o_spreadsheet` library or write a Rust/WASM formula engine for spreadsheet dashboards.
23. Build an Odoo-database importer (partners, products, accounting masters) as an adoption path.
24. Offer web-client offline mode with queued writes once the core is stable.

## Verification log

Adversarial fact-check run on 2026-10-01 against live sources: GitHub raw files and API for `odoo/odoo` (branches `17.0`, `18.0`, `19.0`, `20.0`; a fresh sparse clone of `20.0` at commit `9df43e71` from 2026-10-01), `odoo/documentation` `20.0`, `OCA/OpenUpgrade`, `odoo/owl`, `odoo/paper-muncher`, `wkhtmltopdf/wkhtmltopdf`, the odoo.com pricing page (raw HTML), the support-table and release-notes pages, nightly.odoo.com, the HN item and crates.io.

**Confirmed as stated: about 96 atomic claims.** This covers all flagged claims except the tracking one (corrected below) and the pricing mapping (resolved below). Highlights:

- Release and support dates for 17.0 to 20.0 and SaaS 19.1 to 19.4; default branch `20.0`; nightly tarballs.
- LGPL-3 licence and `release.py` limits (Python 3.12 to 3.14, PostgreSQL 16).
- The `ir.access` merge in 19.4 and the `Domain.OR(permissions) & Domain.AND(restrictions)` algebra in `_access_domain`.
- RPC deprecation with removal in Odoo 22, JSON-2 (new in 19.0), 160-bit API keys with a 90-day cap on `base.group_user`.
- The external-API restriction to Custom plans and the multi-company upsell.
- CE tree counts (642 directories, 229 `l10n_*`) and the absence of all listed Enterprise modules and of any AI or MCP module.
- Owl `3.0.0-alpha.49` bundled with a compatibility layer, and being the newest `odoo/owl` tag.
- OpenUpgrade `20.0`: one "Initial commit" dated 2026-09-28, no scripts; `19.0` has 329 and `18.0` has 430 script entries; AGPL-3.0.
- Enterprise-only free upgrade service (`upgrade.rst` L394).
- `wkhtmltopdf` as default engine (L788), the upstream repo archived with last push 2022-11-22, and `paper-muncher` under LGPL-3.0.
- Cron `FOR NO KEY UPDATE SKIP LOCKED`, deactivation at 5 or more failures spanning more than 7 days, and prefork defaults (2048/2560 MiB, 60 s CPU, 120 s real, `limit_request` 65,536, `max_cron_threads` 2).
- Source line counts (`mail_thread.py` 5,446; `ir_qweb.py` 3,212; `odoo/orm` 21,627), the 18 `base_automation` triggers, and the `.sudo(` counts (433, 134 and 220).
- ORM changelog version attributions (19.1 `compute_sql`, 19.3 `BinaryValue` and `access` operator, 18.1 constraints as attributes, 19.4 h11 and thread-safe ormcache, and so on).

**Corrections (was / now / source):**

1. Was: "Stored `mail.tracking.value` was removed in Odoo 20 (model file present on 19.0, absent on 20.0)". Now: it was moved out of core `mail` into the new opt-in CE addon `mail_tracking` ("Discuss Tracking", no `auto_install`), which still defines `mail.tracking.value`. The chatter builds the tracking message dynamically, and the release notes say admins can install a dedicated module for the old values. Sections 0 and 5 and the Confidence section were updated. Source: https://github.com/odoo/odoo/tree/20.0/addons/mail_tracking , https://www.odoo.com/odoo-20-release-notes
2. Was: "393 others" non-l10n addon directories (642 total, 229 l10n). Now: 413 (642 - 229). Source: GitHub tree API `repos/odoo/odoo/git/trees/20.0:addons`.
3. Was: pricing "yearly/monthly mapping unresolved". Now: the Yearly toggle shows Standard EUR 19.90 and Custom EUR 35.90, both first-12-month discounts off list EUR 24.90 and EUR 44.90. The Monthly toggle shows EUR 24.90 and EUR 44.90, off list EUR 31.10 and EUR 56.90. Light User EUR 7.90. Source: raw HTML of https://www.odoo.com/pricing-plan (`yearly_standard`, `monthly_standard`, `yearly_custom`, `monthly_custom` price spans).
4. Was: "`name_get` was removed in 17". Now: deprecated in 16.4/17.0 (warning stub still present on `17.0`), gone on `18.0`. Source: ORM changelog (16.4 entry), `odoo/models.py` on branches `17.0` and `18.0`.
5. Was: "Mandatory keys: `author` and `license`". Now: `name`, `author` and `license`; a missing license defaults to LGPL-3 and a missing author falls back to `contributors`/`maintainer`, both with a warning. Source: `odoo/modules/module.py` (`_DEFAULT_MANIFEST`, L456-470).
6. Was: access rows "cached under `ormcache('stable')` keyed by `allowed_company_ids`". Now: all active rows are cached once under `ormcache(cache='stable')`; the computed per-model domain is cached per `(operation, uid, allowed_company_ids)`. Source: `ir_access.py` `_get_all_access`, `orm/models.py` `_access_domain`, `orm/environments.py` `_access_context`.
7. Was: `_access_domain` at `models.py` ~L3590. Now: ~L3603.
8. Was: repo "~44% JS", "54.7k stars". Now: ~43.5% JS (Python ~52.1%), ~54.8k stars (live). Source: GitHub API languages and repo endpoints.
9. Was: CEL via crate `cel-interpreter`. Now: crate `cel` (same `cel-rust` repo); `cel-interpreter` was last released as 0.10.0 on 2025-07-23, while `cel` is at 0.14.5 (2026-09-07). Source: https://crates.io/crates/cel , https://github.com/cel-rust/cel-rust
10. Was: "The Studio docs list [gantt, map, cohort, grid]". Now: Studio docs list gantt, map and cohort; grid is Enterprise only by inference from the absence of `web_grid` in the CE tree. Source: `content/applications/studio/views.rst` on `20.0`.
11. Was: `sudo` counts phrased as calls carrying the mandated comment. Now: 433 `.sudo(` calls in `addons/mail` and 134 `# sudo:` comments in that addon (tests excluded); 220 `.sudo(` in `odoo/addons/base`. Source: independent grep of the 20.0 sparse clone.
12. Was: API key "maximum lifetime three months". Now (refined): docs say keys cannot last more than three months; code enforces a per-group `api_key_duration` (90.0 days on `base.group_user`), and system users are exempt. Source: `base_groups.xml`, `res_users.py` `_check_expiration_date`, `external_api.rst`.
13. Added (missing fact): the `db` service of XML-RPC/JSON-RPC was already removed in Odoo 20 (Online 19.1); `common` and `object` go in Odoo 22 (fall 2028) and Online 21.1 (winter 2027). Source: `external_api.rst`, "Migrating from XML-RPC / JSON-RPC".
14. Added: `v3.0.0-alpha.49` of `odoo/owl` is flagged pre-release, published 2026-09-08. The Owl 2 to Owl 3 layer calls itself a temporary bridge. Source: https://github.com/odoo/owl/tags
15. Added: the latest `paper-muncher` release is `v0.8.0` (2026-09-18), still pre-1.0, and the 20.0 core source has no `paper-muncher` references. Source: https://github.com/odoo/paper-muncher/releases
16. Added: 17.0 standard support ends Sept 2026; SaaS 19.1 to 19.4 release months (Jan, Mar, May, Jul 2026); SaaS gets no extended support. Source: the support-table page.
17. Added: offline mode is documented as a short-term-interruption feature, with oldest data evicted beyond 2 GB. Source: `offline_mode.rst`.
18. Added: Odoo 20 release notes say all AI features need credits bought via IAP. Source: https://www.odoo.com/odoo-20-release-notes
19. Added: crates.io versions and dates for the Rust crates named in the file (see Gaps).
20. Marked `documents_spreadsheet` being Enterprise as unverified beyond its absence from the CE tree.

**Unresolved:**

- The exact release day of Odoo 20.0 was not found. The support table and release notes say only "September 2026".
- The odoo.com release-notes page is truncated in the fetcher, so release-note claims beyond those quoted could not be checked.
- The CE/EE boundary is inferred from absence in the CE git tree. No edition-comparison table could be read, and Enterprise behaviour (Studio internals, MCP and AI modules, `documents_spreadsheet`, grid view) is unverified beyond public docs.
- Pricing is volatile, and the discount terms ("valid for 12 months, for initial users ordered") were not studied further.
- Fitness of the suggested Rust crates for obop (maintenance, licence, API stability) was not assessed, only existence and latest versions.
- Odoo 20.0 production stability and `paper-muncher` maturity were not assessed.
