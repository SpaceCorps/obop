# Metadata-driven platforms and an Odoo-style dynamic model layer in Rust on PostgreSQL

Research note 08 for obop. Research date: 2026-10-01. Labels: **[V]** verified against a live primary source this session; **[S]** secondary source or opinion; **[I]** my inference or arithmetic; **[U]** unverified or from memory.

## Scope & method

Primary sources read: Odoo source on GitHub (`master`, `19.0`, `20.0`: `odoo/orm/*.py`, `ir_model.py`, `ir_rule.py`, `ir_sequence.py`, `account_move.py`, `ir_attachment.py`, `addons/web/models/models.py`), Frappe, Twenty, Directus, Baserow, NocoDB, PocketBase, PostgREST, Hasura and pg_graphql source and release metadata (GitHub API), PostgreSQL 18 docs, crates.io, and Notion/Salesforce engineering pages. No web-search tool was available to the original author, so discovery was by direct URL and GitHub API; projects I did not think of may be missing. A later adversarial verification pass (2026-10-01; see "Verification log") re-checked the claims against primary sources and resolved the Salesforce architect page (fetched) and EU VAT Art. 226 (via a legislation.gov.uk copy; EUR-Lex itself still returned empty). Odoo Enterprise (including Studio internals) was not read and must not be copied.

---

## 1. Landscape snapshot (2026-10-01)

| Project | What it is | License [V] | Latest release [V] | User-defined schema storage |
|---|---|---|---|---|
| Odoo | Python ERP, ORM with `ir.model*` reflection | LGPLv3 ([LICENSE](https://raw.githubusercontent.com/odoo/odoo/master/LICENSE)) | `20.0` branch = `(20, 0, 0, FINAL, 0, '')` ([release.py](https://raw.githubusercontent.com/odoo/odoo/20.0/odoo/release.py)); `master` = `(20, 1, 0, ALPHA, 1, '')`; 19.0 also FINAL. Odoo 20 was released in September 2026 and is the recommended version; Odoo 19 is still supported ([nightly.odoo.com](https://nightly.odoo.com/)) | Real columns via DDL (`x_` models/fields) + Properties (JSONB) |
| Frappe / ERPNext | Python/JS framework, DocType metadata | Frappe MIT; ERPNext GPL-3.0 | Frappe v16.36.1 (2026-09-30); ERPNext v16.37.0 (2026-09-30), v15 maintenance line v15.121.6 ([releases](https://github.com/frappe/erpnext/releases)) | Real columns (`tab<DocType>`), `Custom Field` + `Property Setter` overlays |
| Twenty | TypeScript CRM, workspace metadata engine | AGPLv3 + commercial `/* @license Enterprise */` files; MIT for `twenty-sdk`, `twenty-client-sdk`, `create-twenty-app`, `twenty-shared`, `twenty-ui` and `packages/twenty-apps`; AGPL carries a "Twenty Application Exception" additional permission ([LICENSE](https://raw.githubusercontent.com/twentyhq/twenty/main/LICENSE)) | v2.44.0 (2026-10-01) | Schema per workspace, real DDL |
| Directus | Node data platform over SQL | Monospace Sustainable Core License 1.0 (MSCL-1.0-GPL), source-available: any purpose except a "Competing Use"; each version converts to GPL-3.0 on its fourth anniversary ([license](https://raw.githubusercontent.com/directus/directus/main/license)) | v12.4.1 (2026-09-23) | Real DDL + `directus_*` metadata |
| Baserow | Django Airtable clone | MIT ("Baserow OSE") outside `premium/` and `enterprise/`; `docs/` CC BY-SA 4.0 ([LICENSE](https://raw.githubusercontent.com/baserow/baserow/develop/LICENSE)) | 2.4.0 (2026-09-30) | One real PG table per user table |
| NocoDB | Node Airtable alternative | Sustainable Use License, updated 2026-01-29 ([LICENSE.md](https://raw.githubusercontent.com/nocodb/nocodb/develop/LICENSE.md)) | 2026.09.1 (2026-09-29) | Metadata in `nc_models_v2`, `nc_columns_v2`, `nc_col_lookup_v2`, `nc_col_rollup_v2`, `nc_col_formula_v2` ([globals.ts](https://raw.githubusercontent.com/nocodb/nocodb/develop/packages/nocodb/src/utils/globals.ts)) |
| PocketBase | Go BaaS on SQLite | MIT | v0.40.4 (2026-09-12), still 0.x | Collections = tables, filter-DSL rules |
| Hasura v2 | Haskell GraphQL over PG | Apache-2.0 | v2.50.3 (2026-09-09); commit tags v2.51.0 on 2026-09-21 | Reads existing schema; boolean-expression permissions |
| PostgREST | Haskell REST over PG | MIT | v16.4 (2026-09-24), even majors only ([CHANGELOG](https://raw.githubusercontent.com/PostgREST/postgrest/main/CHANGELOG.md)) | Authorisation delegated to PG roles/RLS |
| pg_graphql | Rust pgrx extension | Apache-2.0 | v1.6.2 (2026-08-26) | GraphQL reflected inside the DB |
| Salesforce, Airtable, Notion | SaaS | proprietary | n/a | See §2.4, §2.6 |

Hasura v3's Rust connector `ndc-postgres` is Apache-2.0 but "early development, not accepting contributions" (v3.1.0, 2025-08-01, [README](https://github.com/hasura/ndc-postgres)); no v3 engine repo appears in the `hasura` GitHub org listing (checked 2026-10-01), so the engine is presumably closed [I].

**Headline finding [V]:** there is no mature Rust prior art for an Odoo-style dynamic ORM. A GitHub search on 2026-10-01 (`erp language:rust`, `topic:erp language:rust`, re-run in the verification pass) found nothing above 53 stars, and the 53-star top hit (`print-bridge`) is a print agent, not an ERP; the actual Rust ERP projects are `forgeerp` 40, `idEAS` 27, `rerp` 23, `obvia` 16. "metadata-driven" Rust repos were all ≤14 stars. Rust low-code/workflow platforms exist (`windmill-labs/windmill`, ~18k stars) but are not business-suite/ORM prior art. The ORM layer is a core research-and-build item, not a library choice.

**Licensing consequence [V]:** Directus (MSCL, source-available), NocoDB (Sustainable Use License) and Twenty (AGPLv3 plus commercial Enterprise files) code cannot be ported wholesale into a permissively or differently licensed open-source product without licence review; use them as design references. Baserow is different: its non-`premium/`, non-`enterprise/` code is MIT and could be reused with attribution, but it is Python/Django so there is little to port to Rust. Frappe is MIT; ERPNext is GPL-3.0. Odoo Community is LGPLv3; stay clean-room.

---

## 2. Prior art

### 2.1 Odoo: the functional reference

**Reflection [V]** ([ir_model.py](https://raw.githubusercontent.com/odoo/odoo/master/odoo/addons/base/models/ir_model.py)). `ir.model`: `model` (default `x_`), `state` (`manual`/`base`), `abstract`, `transient`, `order`. `ir.model.fields`: `name`, `ttype`, `state`, `store`, `compute`, `depends`, `relation`, `on_delete`, `translate`, `related`, `company_dependent`, `index`, m2m `relation_table/column1/column2`. Custom names need an `x_` prefix (SQL check) and `check_pg_name` (63 chars).

**Custom-field lifecycle [V].** `create` → `flush_all` → `pool._setup_models__(cr, models)` (incremental re-setup of affected models and `_inherit/_inherits` descendants) → `pool.init_models(..., update_custom_fields=True)` (the DDL). `write` allows renaming one column at a time (`RENAME COLUMN`, index renamed) and forbids type/model changes. `unlink` refuses if other manual fields depend on it, then `DROP COLUMN ... CASCADE` and a registry reload. Manual models/fields reload at startup from `state='manual'` rows (`_add_manual_models/_add_manual_fields`, [model_classes.py](https://raw.githubusercontent.com/odoo/odoo/master/odoo/orm/model_classes.py)).

**Registry and multi-worker reload [V]** ([registry.py](https://raw.githubusercontent.com/odoo/odoo/master/odoo/orm/registry.py)). One `Registry` per database (LRU by db name). Cross-worker signalling uses insert-only tables `orm_signaling_registry` plus one per cache key (`default, assets, stable, templates, routing, groups`); workers compare remembered max(id). A code comment says PG sequences were avoided because they break with replication. Loading takes PG advisory locks (`registry_loading`, exclusive on module update).

**Odoo's own trajectory away from EAV [V].** `ir.translation` (file absent on 16.0) and `ir.property` (present on 17.0, absent on 18.0) were folded into in-row JSONB: a field's column type becomes `('jsonb','jsonb')` when `company_dependent` or `translate` is set ([fields.py L891](https://raw.githubusercontent.com/odoo/odoo/master/odoo/orm/fields.py)). Strongest real-world signal on EAV vs JSONB for a mature ERP.

**Properties fields [V]** ([fields_properties.py](https://raw.githubusercontent.com/odoo/odoo/master/odoo/orm/fields_properties.py)). Values: one `jsonb` column on the child (flat name→value map, `store`, `precompute`). *Definitions*: a `PropertiesDefinition` jsonb list on a **container** record (`definition = "<record_field>.<definition_field>"`), so each container (project, team) owns its user-defined schema. Validation: allowed keys/types, unique names ≤512 chars, option shapes. Reads reconcile values with the definition and silently drop removed options or deleted targets. Search compiles to `(col -> 'key')`, JSONB containment for `in`, `->>` for strings; group-by on tags/many2many uses `LEFT JOIN jsonb_array_elements` (source warns it can duplicate rows); HTML properties cannot be grouped. Properties are deliberately second-class: no FK, no unique, no index unless added manually.

**Studio [V/U].** Customises fields, views, models, automation rules, webhooks, PDF reports, approval and security rules; installing it on the Standard plan triggers an upsell to the Custom plan ([docs](https://www.odoo.com/documentation/19.0/applications/studio.html)). There is no `web_studio` module in the `odoo/odoo` tree (only its icon), consistent with Studio being Enterprise-only [V/I]; I did not read Studio code. It presumably writes ordinary manual `ir.model*` rows plus view inheritance [I].

**Domains [V]** ([domains.py](https://raw.githubusercontent.com/odoo/odoo/master/odoo/orm/domains.py), 88 KB). Immutable tree `DomainBool | Not | And | Or | Condition | Custom`; legacy prefix lists (`'&','|','!'`) parsed with a stack. Four optimisation levels (`NONE, BASIC, DYNAMIC_VALUES, FULL`) iterated to a fixed point: negation pushdown, `=`→`in`, merging `a in {1} or a in {2}`, date→range, `child_of/parent_of` expansion, dotted paths rewritten `company_id.name = x` → `company_id any (name = x)` (EXISTS-style), and `any!` (bypasses record rules; internal only, rejected in list-form domains). Non-stored computed fields participate via their `search` method at FULL level. `_to_sql` requires a fully optimised domain, supports `any, not any, in, not in, <, >, <=, >=, like, ilike, =like, =ilike`, and renders negation as `(...) IS NOT TRUE` (NULL-safe).

**Record rules [V]** ([ir_rule.py@19.0](https://raw.githubusercontent.com/odoo/odoo/19.0/odoo/addons/base/models/ir_rule.py)). Rules per (model, mode ∈ read/write/create/unlink), global or group-linked. **Global rules AND together; group rules OR together; the OR is ANDed with globals.** `_inherits` parents add `parent_field any (parent_domain)`. `domain_force` is `safe_eval`-ed with `user`, `company_ids`, `company_id`, `time`. Cached by `ormcache` on (uid, su, model, mode, allowed_company_ids); any rule write clears the registry cache. Superuser bypasses rules.

**Computed stored fields [V]** ([models.py L6265-6500](https://raw.githubusercontent.com/odoo/odoo/master/odoo/orm/models.py)). At registry setup each field's `depends` paths are inverted into a `TriggerTree`: for source field F, nodes list fields to recompute, edges are inverse relations to walk to find affected records (the source's diagram: G depends on F; H on X.F; I on W.X.F). `modified(fnames)` walks the tree (`_modified_triggers`) via inverse many2one/one2many or a `search([(field,'in',ids)])` fallback and queues work in `transaction.tocompute`; `_recompute_field` computes in batches at flush. On create it skips inverse walks. A comment in `_get_unbalanced_moves` warns integrity queries "MUST NOT depend on computed stored fields" because `create()` can defer recomputation.

**Onchange [V]** (in the `web` addon, [models.py L2189+](https://raw.githubusercontent.com/odoo/odoo/master/addons/web/models/models.py)). Build an unsaved `new()` record from form values, take `snapshot0`, apply changes, run onchange methods and recompute in a loop until no viewed field changes, take `snapshot1`, return `snapshot1.diff(snapshot0)` as `value` plus optional `warning`; x2many as command lists. The ORM core leaves `onchange()` as `NotImplementedError`. One server round trip per edit.

**Concurrency [V].** 16.0 had `CONCURRENCY_CHECK_FIELD = '__last_update'`; it is absent from `odoo/models.py` on 17.0, 18.0 and 19.0 and from master's `odoo/orm/models.py` (grep for `__last_update` returns nothing). Transactions run at `REPEATABLE READ` with retry on `SerializationFailure` ([sql_db.py](https://raw.githubusercontent.com/odoo/odoo/master/odoo/sql_db.py)): last-writer-wins at business level.

**Sequences [V]** ([ir_sequence.py](https://raw.githubusercontent.com/odoo/odoo/19.0/odoo/addons/base/models/ir_sequence.py)). `standard` = one PG `SEQUENCE` per record, fast, gappy. `no_gap` = `SELECT number_next FROM ir_sequence WHERE id=%s FOR UPDATE NOWAIT` then `UPDATE number_next+incr`; a concurrent transaction fails immediately rather than queueing. Date ranges are separate rows. Invoice names actually come from `account.move.sequence_prefix` + `sequence_number`, derived from the last posted move in the same journal/prefix.

**Ledger integrity [V]** ([account_move.py](https://raw.githubusercontent.com/odoo/odoo/19.0/addons/account/models/account_move.py)). Balance is enforced **in Python, not the database**: `_check_balanced` runs `SUM(balance)` per move at the end of `create/write` and raises `UserError`; the `check_move_validity` context flag disables it. Lock dates: `_check_fiscal_lock_dates`. Tamper evidence: `secure_sequence_number` plus an `inalterable_hash` chain per journal (`hash_version` 1-4). No DB guarantee that a posted entry is balanced or immutable.

**Audit / archive / attachments [V].** `mail.tracking.value` stores old/new in typed columns (`old_value_integer/float/char/text/datetime` and the `new_value_*` twins; on master the model lives in the new `mail_tracking` addon, on 19.0 in `mail`). Archive = `active` + `active_test` context key. `ir.attachment` uses a SHA-1 content-addressed filestore; `_file_write` records the name in a `checklist` dir "in case the transaction aborts" and `_gc_file_store` deletes unreferenced files ([ir_attachment.py](https://raw.githubusercontent.com/odoo/odoo/19.0/odoo/addons/base/models/ir_attachment.py)); access is checked via the linked `res_model/res_id` record.

**API [V].** `/json/2/<model>/<method>` with `Authorization: bearer <API key>`, optional `X-Odoo-Database`, and a `/doc` page ([external_api.rst](https://raw.githubusercontent.com/odoo/documentation/19.0/content/developer/reference/external_api.rst)). XML-RPC/JSON-RPC are deprecated since 19.0: the `db` service was removed in Odoo 20 (fall 2026; `odoo/service/db.py` exists on 19.0 but is absent on 20.0) and in Online 19.1; `common`/`object` are scheduled for removal in Odoo 22 (fall 2028) ([external_rpc_api.rst](https://raw.githubusercontent.com/odoo/documentation/master/content/developer/reference/external_rpc_api.rst)). The API-parity target is now JSON-2.

### 2.2 Frappe DocTypes

- A DocType is model and view; meta is itself DocTypes; table `tab<DocType>`; the Frappe README says ERPNext has "more than 700 object types" ([README](https://github.com/frappe/frappe)); counting `<name>/<name>.json` DocType definitions on `develop` (2026-10-01) gives 534 in `erpnext` plus 305 in `frappe` = about 839 for a full install, including child tables [V/I]. Frappe's `frappe/database/` now has `mariadb`, `postgres`, `sqlite` and `duckdb` backends [V].
- Custom fields are `Custom Field` rows whose `on_update` calls `frappe.db.updatedb(doctype)` (DDL) ([custom_field.py](https://raw.githubusercontent.com/frappe/frappe/develop/frappe/custom/doctype/custom_field/custom_field.py)); customer overrides of standard metadata are `Property Setter` rows, so shipped JSON is never edited [V]. Cleanest upgrade story seen: shipped metadata immutable, customer deltas stored separately.
- Optimistic concurrency: `check_if_latest()` compares `_original_modified` to DB `modified`, raising `TimestampMismatchError` ([document.py](https://raw.githubusercontent.com/frappe/frappe/develop/frappe/model/document.py)); `docstatus` 0/1/2 (draft/submitted/cancelled) with validated transitions [V].
- Naming series: `tabSeries(name, current)`, `SELECT ... FOR UPDATE` then `current + 1`; `revert_series_if_last` decrements on delete ([naming.py](https://raw.githubusercontent.com/frappe/frappe/develop/frappe/model/naming.py)) [V].
- Audit: `Version` stores a JSON diff; `Deleted Document` archives `deleted_doctype`, `deleted_name`, `data`, `restored` on hard delete [V].
- Postgres is secondary to MariaDB (unverified as a current project-status statement); a code comment notes "Postgres DDL is transactional" ([postgres/database.py](https://raw.githubusercontent.com/frappe/frappe/develop/frappe/database/postgres/database.py)) [V]. Site-per-database multitenancy: `bench new-site` takes `--db-name` and otherwise generates a random `_<hash>` database name per site ([installer.py](https://raw.githubusercontent.com/frappe/frappe/develop/frappe/installer.py)) [V/I].

### 2.3 Twenty's workspace metadata engine (source: `packages/twenty-server/src/engine`)

- **Metadata** tables such as `objectMetadata` (unique on `(nameSingular, workspaceId)` and `(namePlural, workspaceId)`, `overrides jsonb`) and `fieldMetadata` (25 field types incl. `ACTOR, ADDRESS, CURRENCY, EMAILS, FULL_NAME, MORPH_RELATION, RATING, RAW_JSON, RICH_TEXT, TS_VECTOR`); further metadata for views, roles, row-level-permission predicates, page layouts, logic functions [V].
- **Data** in one PG **schema per workspace**, `workspace_<base36(uuid)>`, via `createSchema/dropSchema`. A `WORKSPACE_SCHEMA_DDL_LOCKED` flag blocks DDL "during hot upgrades" [V].
- **Real DDL:** `workspace-schema-column-manager` emits `ALTER TABLE "<schema>"."<table>" ADD COLUMN / DROP COLUMN IF EXISTS / RENAME COLUMN / SET DEFAULT` with identifier escaping. Composite fields expand to several columns (`CURRENCY` = `amountMicros NUMERIC` + `currencyCode TEXT`) [V].
- **Pipeline:** `workspace-migration-builder` (validate/plan) → `workspace-migration-runner` (+ deferred actions for slow work) → cache invalidation (`ORMEntityMetadatas`, `graphQLResolverNameMap`, metadata GraphQL schema version) [V].
- **Generated API:** per-workspace GraphQL schema built from metadata; code-first `defineObject` in `twenty-sdk` published via CLI [V].

Lesson: schema-per-tenant + real DDL works at SaaS scale when a tenant has tens of tables; an ERP has an order of magnitude more (§5).

### 2.4 Salesforce

Architects page ([link](https://architect.salesforce.com/fundamentals/platform-multitenant-architecture)) [V]: one shared schema; objects/fields are rows in a Universal Data Dictionary (`MT_Objects`, `MT_Fields`); all data in a generic `MT_Data` heap with string "flex columns" (`Value0..N`) converted with `TO_NUMBER/TO_DATE`; long text out of line (`MT_Clob`, ≤32,000 chars); typed pivot tables for indexes (`MT_Indexes`, `MT_Unique_Indexes`, `MT_Relationships`); physical partitioning by OrgID; a custom optimiser with tenant/user-level statistics and "pre-queries"; governor limits prevent one tenant monopolising resources; recycle bin 15 days. Per-edition limits and Metadata API details could not be fetched [U].

Verdict [I]: EAV-at-scale, needed because millions of tiny orgs share one schema without per-tenant DDL. On PostgreSQL, with transactional DDL and per-tenant databases, copying MT_Data would sacrifice typing, FKs and planner statistics for nothing.

### 2.5 Directus, Baserow, NocoDB, PocketBase, Hasura, PostgREST, pg_graphql

| Project | Pattern worth stealing | Pattern to avoid | Evidence |
|---|---|---|---|
| Directus | `directus_collections/fields/relations` metadata over real tables; `createField` runs `alterTable` in a transaction but `CREATE INDEX CONCURRENTLY` outside it; mutations emit `directus_activity` + `directus_revisions` (`delta`) | Introspection-first: schema is truth, metadata decoration | [fields.ts](https://raw.githubusercontent.com/directus/directus/main/api/src/services/fields.ts), [items.ts](https://raw.githubusercontent.com/directus/directus/main/api/src/services/items.ts) |
| Baserow | Runtime-generated model per table (`managed=False`, cached), real columns, `MAX_FIELD_LIMIT` default 600, DDL via `schema_editor.add_field` | Per-table DDL churn | [models.py](https://gitlab.com/baserow/baserow/-/raw/develop/backend/src/baserow/contrib/database/table/models.py), [base.py](https://raw.githubusercontent.com/baserow/baserow/develop/backend/src/baserow/config/settings/base.py) |
| NocoDB | Lookup / rollup / formula as first-class column kinds | Metadata-only layer over foreign DBs weakens invariants | globals.ts |
| PocketBase | Rules are a filter language compiled to SQL via a `FieldResolver`; `maxExpressions` budget | SQLite only; no global-vs-group composition | [filter.go](https://raw.githubusercontent.com/pocketbase/pocketbase/master/tools/search/filter.go) |
| Hasura v2 | Permission = boolean expression per (table, role, operation), session variables, relationship traversal, `_exists`, separate pre/post-update checks | GraphQL façade only | [docs](https://hasura.io/docs/2.0/auth/authorization/permissions/row-level-permissions/) |
| PostgREST | Role per request, JWT claims via `current_setting('request.jwt.claims', true)`, RLS `USING`/`WITH CHECK` | Needs real PG roles (cluster-wide names) | [db_authz](https://raw.githubusercontent.com/PostgREST/postgrest/main/docs/explanations/db_authz.rst) |
| pg_graphql | GraphQL reflected from SQL schema in-DB; Rust+pgrx viable | Extension deployment constraints | [README](https://raw.githubusercontent.com/supabase/pg_graphql/master/README.md) |

### 2.6 Airtable and Notion

- **Notion [V]** ([data model](https://www.notion.com/blog/data-model-behind-notion)): everything is a *block* with UUID, `type`, free-form `properties` (independent of type), ordered `content` child pointers and an upward `parent` pointer used only for permission inheritance. Writes are operations in transactions: load "before", apply to a copy, validate permissions and coherence, commit. Sharding ([post](https://www.notion.com/blog/sharding-postgres-at-notion)): key = workspace ID; **480 logical shards, one PG schema each, 15 per DB on 32 physical DBs**; later **96 databases × 5 schemas** ([re-shard](https://www.notion.com/blog/the-great-re-shard)); 480 chosen for divisibility. Best public evidence of "row-level tenant key + schema-as-shard".
- **Airtable [V]:** Free 1,000 records/base, Team 50,000, Business self-serve 125,000; API 5 req/s per base, 50 req/s per user token, 429 then 30 s penalty ([plans](https://support.airtable.com/docs/airtable-plans), [rate limits](https://airtable.com/developers/web/api/rate-limits)). Internal storage not public [U]. Lesson: explicit per-base ceilings make a generic model tractable.

### 2.7 Rust ecosystem today [V unless noted]

Versions and dates from the [crates.io API](https://crates.io/api/v1/crates/sqlx) (checked 2026-10-01). Licences per crates.io: `sqlx`, `sea-orm`, `sea-query`, `seaography`, `async-graphql`, `inventory`, `utoipa` are `MIT OR Apache-2.0`; `object_store` is `MIT/Apache-2.0` [V].

| Crate | State | Relevance |
|---|---|---|
| `sqlx` | 0.9.0 (2026-05-21) | Async PG, `QueryBuilder`; compile-time macros irrelevant to dynamic models |
| `sea-query` | 1.0.2 (2026-08-12) | Runtime SQL AST: natural Domain-compiler backend |
| `sea-orm` | 2.0.4 (2026-09-27), MIT OR Apache-2.0 ([crates.io](https://crates.io/crates/sea-orm)) | "Entity First" schema sync (`db.get_schema_registry("my_crate::entity::*").sync(db)`, features `entity-registry` + `schema-sync`), RBAC; static entities |
| `seaography` | 1.1.4 stable, 2.0.0-rc.9 | GraphQL from static SeaORM entities |
| `async-graphql` | 7.2.1 stable (2026-01-20); 8.0.0-rc.5 prerelease (2026-04-21) is the newest version and the last repo commit; has `dynamic` module ([docs.rs](https://docs.rs/async-graphql/7.2.1/async_graphql/dynamic/index.html)) | Runtime GraphQL schema; slow cadence, no 8.0 stable after five RCs in 2026 |
| `inventory` | 0.3.24 | Link-time plugin registration: derive→registry mechanism |
| `object_store` | 0.14.2 (2026-09-15) | S3/GCS/Azure/local blobs |
| `utoipa` | 6.0.0 (2026-09-22) | OpenAPI; runtime building suitability [U] |
| `prisma-engines` | Apache-2.0, pushed 2026-09-29 | Its Rust *query compiler* emits executable plans (SQL + orchestration) run by JS drivers: compile-to-plan is a sound split |

---

## 3. EAV vs JSONB vs real columns

| Dimension | EAV | JSONB column (Odoo Properties) | Real column via DDL |
|---|---|---|---|
| Constraints | None without triggers | App-validated; no cross-key FK/CHECK | NOT NULL, CHECK, FK, UNIQUE |
| Read one record | N joins / pivot | One row | One row |
| Indexing | Typed side tables (Salesforce `MT_Indexes`) | GIN `jsonb_ops`/`jsonb_path_ops` or expression indexes ([docs](https://www.postgresql.org/docs/current/datatype-json.html)) | B-tree etc. |
| Planner stats | Poor | Docs say nothing of per-key stats [V]; expect default selectivity [I] | Full |
| Update cost | Row per attribute | Whole value rewritten; TOAST above ~2 kB ([docs](https://www.postgresql.org/docs/current/storage-toast.html)) [V+I] | Changed tuple only |
| Schema change | Insert rows | Edit definition | `ALTER TABLE` (§4) |
| Hard limits | None | 1 GB/value | 1,600 columns/table, 63-byte identifiers ([limits](https://www.postgresql.org/docs/current/limits.html)) [V] |
| Aggregation | Painful | Casts; array group-by may duplicate rows [V] | Natural |

PG docs also warn "any update acquires a row-level lock on the whole row", so JSONB documents should be small and atomic [V].

**Recommendation [I], three tiers:** (1) module-declared fields = typed real columns; (2) user fields needing index/unique/FK/stored compute/money/reporting = real columns created by the schema manager; (3) sparse per-container "properties" = one JSONB column with definitions on a container record, with an explicit "promote to column" path. No EAV.

---

## 4. Migrating user-defined schema safely

PG 18 facts [V] ([ALTER TABLE](https://www.postgresql.org/docs/current/sql-altertable.html)):
- `ADD COLUMN` with a non-volatile default is metadata-only; volatile defaults, stored generated columns, identity columns and constrained domains force a rewrite; virtual generated columns never do.
- Most `ALTER TABLE` forms take `ACCESS EXCLUSIVE`; `VALIDATE CONSTRAINT` takes `SHARE UPDATE EXCLUSIVE`; `ADD FOREIGN KEY` takes `SHARE ROW EXCLUSIVE` on both tables.
- `ALTER COLUMN TYPE` rewrites table and indexes unless binary-coercible, needs up to 2× disk, drops stats, and is not MVCC-safe for old snapshots.
- `SET NOT NULL` scans unless a valid `CHECK (col IS NOT NULL)` exists: add the check `NOT VALID` → `VALIDATE` → `SET NOT NULL` → drop the check. PG 18 adds `NOT VALID` for `NOT NULL` constraints themselves (release notes: "Allow ALTER TABLE to set the NOT VALID attribute of NOT NULL constraints"; `VALIDATE CONSTRAINT` accepts them), which shortens this dance on 18+ ([release notes](https://www.postgresql.org/docs/18/release-18.html)).
- `DROP COLUMN` is instant; dropped columns still count toward the 1,600 limit.
- `CREATE INDEX CONCURRENTLY` cannot run in a transaction (Directus comment) and DDL is transactional (Frappe comment) [V].

**Design [I]:** a `SchemaManager` diffs registry vs `pg_catalog` into a typed plan (Twenty builder/runner split, Odoo `_auto_init`), classifies steps `Instant | Rewrite | Blocking | Concurrent`, runs under `SET LOCAL lock_timeout` with retry (common practice, not in PG docs), and refuses `Rewrite` steps on large tables except via expand → batched backfill → contract jobs. Each user field has a **stable immutable id** separate from its label; physical name `x_<slug>` (63 bytes); drops are two-phase (rename to `_dropped_<id>`, hard drop later); a `DDL_LOCKED` flag during upgrades (Twenty); an append-only `meta_migration_log`.

---

## 5. Multi-tenancy with numbers

| Model | Published scale data | Label |
|---|---|---|
| Database per tenant | "10s of tenants"; 5 fine, 10 probably ok, 50+ steer clear | Crunchy Data [S] |
| Schema per tenant | "100s of tenants"; Citus 12 + PgBouncer "generally not to exceed several thousand of tenants" | Crunchy [S] ([link](https://www.crunchydata.com/blog/designing-your-postgres-database-for-multi-tenancy); post dated 2023-11-14, so Citus 12-era guidance, not re-dated for PG 18) |
| Row-level `tenant_id` | "millions of tenants"; composite keys lead with tenant id | Crunchy + [Citus docs](https://docs.citusdata.com/en/stable/sharding/data_modeling.html) [S/V] |
| Notion | 480 schemas over 32 DBs (post of 2021-10-06) → 96 DBs × 5 schemas (post of 2023-07-17; current state unknown); limits 500 GB/table, 10 TB/DB; ~100 PgBouncers × 6 conns per shard | [V] |
| Twenty | Schema per workspace in SaaS | [V] |
| Odoo | Database per tenant | [V] |

Hard PG limits [V]: 4,294,950,911 databases; 1,431,650,303 relations per database (not binding). Practical, derived [I]:
- `max_locks_per_transaction` defaults to 64; the lock table holds about that × (`max_connections` + `max_prepared_transactions`) ≈ 6,400 locks at 100 connections. Whole-schema operations like `pg_dump` lock every table (the docs page does not mention pg_dump; the inference is mine).
- An ERP tenant is big: the Frappe README says ERPNext has >700 object types and a DocType-JSON count on `develop` gives ~839 with the framework's own [V/I], so assume ~600-900 tables and thousands of indexes per obop tenant [I]. 1,000 schemas in one DB ≈ 0.7-0.9 M tables plus millions of index relations, each costing catalog rows, per-backend relcache memory and autovacuum attention [I]. PG partitioning docs call "a few thousand partitions" the planner's comfortable ceiling and legacy inheritance "up to perhaps a hundred" [V]: a proxy for relation-count sensitivity.
- RLS [V] ([docs](https://www.postgresql.org/docs/current/ddl-rowsecurity.html)): policy expressions run before user quals unless functions are `LEAKPROOF`; row-local policies are fastest; subselects carry performance and race-condition risk; RI/unique checks bypass RLS; owners bypass unless `FORCE ROW LEVEL SECURITY`. Supabase: index policy columns, wrap calls as `(select fn())` for an initPlan, always specify `TO role` ([guide](https://supabase.com/docs/guides/database/postgres/row-level-security)); its [RLS performance guide](https://supabase.com/docs/guides/troubleshooting/rls-performance-and-best-practices-Z5Jjwv) does publish before/after timings (e.g. 171 ms → under 0.1 ms with an index on `user_id`; 179 → 9 ms wrapping `auth.uid()` in `(select ...)`; 173,000 → 16 ms for `team_id = any(array(select user_teams()))`), but these are vendor micro-benchmarks of badly written policies on test tables, not a general RLS overhead figure, so no reliable generic RLS overhead number exists [V/I].

**Recommendation [I]:** isolation unit = **one database per business** ("one business, one platform"; Odoo and Frappe model): per-tenant DDL freedom, trivial export/restore/erasure, no `tenant_id` tax on every table, no cross-tenant blast radius. A control plane packs tens-to-low-hundreds of tenant DBs per cluster; treat Crunchy's numbers as hypotheses to benchmark. Inside a tenant, multi-company = row-level `company_id` + record rules. Keep a `TenantRouter` seam so pooled row-level or schema-shard modes can be added later. RLS is a backstop for `company_id`, not the rule engine (§8).

---

## 6. Static Rust core with runtime-extensible models [I]

- **Static side:** `#[derive(Model)]` on a struct emits a `ModelDef` (name, table, fields with type/nullability/index/unique/relations, `compute`/`depends` names, access flags) registered at link time with `inventory::submit!` (no central list), plus typed accessors and `TryFrom<Record>` for first-party paths (posting, stock moves) that need compile-time safety. SeaORM 2.0 shows the derive→registry→schema-sync pipeline is practical [V], but static-only.
- **Dynamic side:** `meta_model` / `meta_field` / `meta_selection` rows (the `ir.model*` analogue, `origin = code|custom`) merge at tenant start into the same `Registry`. Dynamic records: `Record { model, values: SmallVec<(FieldId, Value)> }`, `enum Value { Null, Bool, Int, Numeric, Text, Date, Timestamp, Json, Ref, Many }`.
- **Lifecycle:** `Arc<Registry>` per tenant with monotonically increasing `registry_version`, swapped atomically after schema change; peers learn via a signalling table or `LISTEN/NOTIFY` (Odoo: insert-only table + max(id) polling [V]). Extend incrementally (affected models only), as Odoo's `_setup_models__(cr, model_names)` does [V].
- **Extension without inheritance:** Odoo's `_inherit`/MRO has no Rust analogue. Modules add fields to other modules' models (`ADD COLUMN` at install) and register **method hooks** (`before_create`, `compute:<field>`, `constrains`, `onchange`) into a per-model chain ordered by module dependency; "super" is explicit `next(ctx)`.
- **Trade-off:** dynamic records cost allocation and dispatch; fine for CRUD, wrong for bulk posting/MRP, where typed structs or set-based SQL should be used.

---

## 7. Domain expression to SQL compiler [I, informed by Odoo V and PocketBase V]

1. **AST:** `enum Domain { True, False, Not, And(Vec), Or(Vec), Cond { path, op, value } }`, immutable, `Hash + Eq`. `Op` ⊇ Odoo's standard set plus `child_of/parent_of` (expanded via a materialised `parent_path`).
2. **Front ends:** Odoo prefix lists (API parity), a JSON object form (`{_and:[...]}`, Directus/Hasura style), optionally text. All parse to one AST.
3. **Type-check against the registry:** resolve path segments, coerce values by field type, reject unknown fields; relation paths become `Any(rel, subdomain)` (EXISTS), as in Odoo; `props.key` resolves via the container definition.
4. **Optimise to fixed point** with a data-driven pass list: NULL-safe negation pushdown, flatten/merge `in`, dedupe, date→range, `=`→`in`.
5. **Inject security:** AND the record-rule domain for (user, model, operation) *before* SQL generation; the `any!`-style bypass is a separate `Op` not constructible from external input.
6. **Emit** via `sea-query` or `sqlx::QueryBuilder`: values always bound, identifiers only from the registry. Budgets: max nodes (PocketBase `maxExpressions`), path depth, `in` list size (PG allows 65,535 query parameters [V]; beyond that bind one `= ANY($1::bigint[])`).
7. **Non-stored computed fields** supply `search(op, value) -> Domain` (Odoo `determine_domain`), else unsearchable.
8. **Plan caching:** evaluate `user`, `company_ids`, `now` into bind values before compile so SQL text is stable per (model, rules, domain shape).

---

## 8. Record rules and RLS pushdown

Copy from Odoo [V]: per (model, mode) rule sets; global AND, group OR, combined by AND; superuser bypass; parent propagation; per-(uid, model, mode, company) caches invalidated on rule writes.

PG combines policies the same way [V] ([CREATE POLICY](https://www.postgresql.org/docs/current/sql-createpolicy.html)): permissive OR, restrictive AND, the two groups ANDed. So **Odoo global rule ≈ `RESTRICTIVE` policy; group rule ≈ `PERMISSIVE` policy.** Caveat [V]: PG docs say that if only restrictive policies exist, no rows are accessible, so a `RESTRICTIVE` policy needs at least one `PERMISSIVE` policy (e.g. `USING (true)`) beside it, whereas Odoo with only global rules grants access to whatever they allow. Full mapping is a trap [I]: Odoo domains use `any` subqueries and `_inherits`; PG warns about subselect policies (performance, READ COMMITTED races); RI/unique checks bypass RLS (covert channels); policies would need per-tenant, per-table DDL on every rule edit versus one cached in-memory domain.

**Recommendation [I]:** app-level pushdown is primary (one plan, full feature set). A small fixed set of row-local RLS policies is the backstop: `company_id = ANY (string_to_array(current_setting('obop.company_ids', true), ',')::bigint[])` as RESTRICTIVE (plus a trivial `PERMISSIVE ... USING (true)` policy, otherwise no row is visible), app role not owner, `FORCE ROW LEVEL SECURITY`, `set_config(name, value, true)` in every transaction (survives transaction pooling). Writes need a `WITH CHECK` equivalent: evaluate the rule on the *new* values as well as the old (Hasura separates pre- and post-update checks [V]).

---

## 9. Computed stored fields and onchange

**Dependency graph [I, following Odoo V]:**
- At registry build, resolve each computed field's `depends` paths (`"line_ids.price_total"`) and invert them into a trigger tree per source field (node = fields to recompute, edge = inverse relation). Detect cycles at build (reject unless `recursive`).
- The unit of work tracks `dirty: (ModelId, FieldId) -> IdSet`; at flush, walk the tree (`SELECT id FROM child WHERE parent_id = ANY($1)` per edge), process in topological order in batches inside the same transaction, write back with one `UPDATE ... FROM (VALUES ...)` per field.
- Compute functions take a *batch* and return values; a `sql = "..."` escape allows set-based aggregates. Skip inverse walks on create [V].
- **Rule:** DB-level invariants are stated on *base* columns only (Odoo's own warning [V]).
- PG 18 virtual generated columns compute on read and never rewrite on add [V]: good for cheap single-row expressions, not indexable [I].

**Onchange [I, following Odoo V]:** a pure function `onchange(model, values, changed, spec) -> { value, warning, domain }` over a `VirtualRecord` implementing the same `RecordView` trait as DB-backed records, so compute and onchange hooks are written once. Snapshot0 → apply changes → loop {hooks + recompute} to fixed point (bounded, ~10 iterations) → snapshot1 → diff; x2many as commands. Later, compile simple expressions to client-side to avoid round trips.

---

## 10. Concurrency, audit, soft delete, attachments

**Optimistic concurrency [I]:** `row_version bigint NOT NULL DEFAULT 1` on every table; `UPDATE ... SET row_version = row_version + 1 WHERE id = $1 AND row_version = $2`, 0 rows → 409/412; ETag/`If-Match` on REST. Frappe's `modified` comparison is the precedent [V]; a counter avoids clock skew. Add a retry wrapper for SQLSTATE `40001`/`40P01` (Odoo retries serialization failures [V]). Use `SELECT ... FOR UPDATE` only in short domain services (sequences, ledger).

**Audit [I]:** (1) ORM field-level change log, opt-in per field: typed old/new columns (Odoo) or JSON diff (Frappe `Version`, Directus `delta`) [V]; (2) DB trigger history for non-ORM writes (`supa_audit` is archived, last pushed 2024-01-03; `pgaudit` 18.0 (2025-09-24; a `19beta4` tag followed on 2026-09-26) logs statements, not row history [V]), so write a small obop-owned trigger into a partitioned append-only table; (3) hash chain on posted accounting documents (Odoo `inalterable_hash` [V]).

**Temporal [V/I]:** PG 18 adds `PRIMARY KEY/UNIQUE ... WITHOUT OVERLAPS` and `FOREIGN KEY ... PERIOD` ([release notes](https://www.postgresql.org/docs/18/release-18.html)): use for price lists, contracts, tax rates, fiscal positions. No native system-versioned tables; `temporal_tables` (BSD-2-Clause) last released v1.2.2 on 2023-09-24 (repo pushed 2026-01-12), so prefer trigger-written history. XTDB 2.1.0 (MPL-2.0, Clojure, 2025-12-01, latest stable; 2.2.0 betas were in progress as of 2026-09-28) is bitemporal but a second datastore. Model bitemporality only where law needs it (pgledger keeps `created_at` and `event_at` [V]). **Not** event sourcing as system of record: state tables + append-only ledgers (journal lines, stock moves) + transactional outbox.

**Soft delete [I]:** `active` (archive; Odoo `active`/`active_test` [V]) is distinct from hard delete with a `deleted_record` archive copy (Frappe `Deleted Document` [V]); partial unique indexes `WHERE active`; GDPR erasure needs real deletion plus an audit tombstone.

**Attachments [I]:** `attachment(id, model, record_id, name, mime, size, checksum_sha256, storage_key, ...)`; blobs content-addressed in `object_store`; `pending` → `committed` state with timeout GC (Odoo checklist idea [V]); downloads authorised by the parent record's read rule (Odoo [V]); signed URLs for bulk.

---

## 11. Gapless sequences and double-entry invariants

**Sequences [V]:** PG `nextval` is not reclaimed on abort or crash; sequences "cannot be used to obtain gapless sequences" ([docs](https://www.postgresql.org/docs/current/functions-sequence.html)). Odoo `no_gap` and Frappe `tabSeries` use a row-locked counter (Odoo `NOWAIT`, Frappe blocking).

**Design [I]:**
- `document_sequence(id, scope (journal, fiscal_year, prefix), next_value)`; allocate **at posting, not at draft creation**, in the transaction that flips state to `posted`: `UPDATE document_sequence SET next_value = next_value + 1 WHERE id = $1 RETURNING next_value - 1`. The row lock serialises posting per sequence until commit; rollback returns the number. Keep posting transactions short; one sequence per journal per fiscal year.
- Block (not `NOWAIT`) with `lock_timeout`; backstop `UNIQUE (journal_id, sequence_prefix, sequence_number)` and a nightly gap detector.
- Plain PG sequences for internal ids and non-legal numbers (quotes, pickings).
- EU VAT Directive 2006/112/EC Art. 226(2) requires "a sequential number, based on one or more series, which uniquely identifies the invoice" ([legislation.gov.uk copy](https://www.legislation.gov.uk/eudr/2006/112/article/226), text as retained at the end of the UK transition; EUR-Lex itself was not retrievable, so later EU amendments such as ViDA were not checked) [V]; that wording asks for uniqueness in series, not strict gaplessness; "some states require strictly gapless series" is [U]. Make gaplessness a per-journal flag, default on for sales journals.

**Double-entry in the database [I], from PG facts [V]:**
- `CHECK` and `NOT NULL` **cannot** be deferred; only `UNIQUE`, `PRIMARY KEY`, `REFERENCES`, `EXCLUDE` and constraint triggers can ([SET CONSTRAINTS](https://www.postgresql.org/docs/current/sql-set-constraints.html)). Constraint triggers must be `AFTER ... FOR EACH ROW` on plain tables, may be `DEFERRABLE INITIALLY DEFERRED`, cannot use transition tables, and `WHEN` is evaluated immediately ([CREATE TRIGGER](https://www.postgresql.org/docs/current/sql-createtrigger.html)). Statement-level triggers with `REFERENCING NEW TABLE` see the whole statement but fire at statement end, not commit.
- Schema: `journal_entry(id, company_id, state, date, number, ...)`; `journal_line(entry_id, account_id, debit_minor bigint CHECK (debit_minor >= 0), credit_minor bigint CHECK (credit_minor >= 0), CHECK (debit_minor = 0 OR credit_minor = 0), currency_id, amount_currency_minor, ...)`. Minor-unit integers remove rounding drift (Odoo rounds `SUM(balance)` to `currency.decimal_places` [V]).
- **Balanced guard:** one `CONSTRAINT TRIGGER ... AFTER INSERT OR UPDATE OF state ... DEFERRABLE INITIALLY DEFERRED` on `journal_entry`; when `state='posted'` it checks `SUM(debit_minor) = SUM(credit_minor)` over the entry's lines. It fires once per posting (O(1)), unlike a per-line deferred trigger (n firings × n-line sums).
- **Immutability guard:** immediate `BEFORE INSERT/UPDATE/DELETE` trigger on `journal_line` that raises if the parent entry is `posted` or period-locked (DB twin of Odoo's `_check_fiscal_lock_dates`); `REVOKE UPDATE, DELETE` from the app role; corrections are reversal entries; state transitions limited to `draft → posted → reversed` by a trigger.
- Composite FK `(company_id, account_id)` for same-company integrity; `WITHOUT OVERLAPS` for non-overlapping fiscal periods.
- Precedents: `pgledger` (MIT, 494 stars, v0.7.0 2025-12-03) is a ledger entirely in PG functions; it locks accounts in sorted id order to avoid deadlocks, raises on disallowed negative balances, and the author reports 10,637 transfers/s at 1.9 ms each (low contention; 7,559/s at 2.6 ms with hot accounts) on an M3 MacBook Air with PG 17.5 ([repo](https://github.com/pgr0ss/pgledger), [bench](https://pgrs.net/2025/05/16/pgledger-in-postgresql-is-fast/)) [V/S]. Formance Ledger (Go, MIT, v2.4.14 stable; v3.0.0 betas since 2026-09) and TigerBeetle (Zig, Apache-2.0, 0.17.9) add a second system; keep the GL in the primary PG for atomicity with documents.

---

## 12. Auto-generated REST / GraphQL from the registry

Precedents [V]: Odoo JSON-2 + `/doc`; Directus REST+GraphQL from schema; Twenty per-workspace GraphQL from metadata with explicit cache invalidation; PostgREST and pg_graphql reflect the DB.

Design [I]:
- **One generic REST router** in axum (`/api/v1/{model}`, `/{model}/{id}`, `/{model}/{id}/{action}`, plus `/json/2/{model}/{method}` for Odoo parity) dispatching through the registry: no per-model routes, so a new custom model needs no restart. Flow: authn → tenant → `Arc<Registry>` → ACL + record rule → Domain compile → SQL.
- **Field spec** modelled on Odoo's `web_read` specification (`{field: {fields: {...}}}`) [V], shared with onchange.
- **OpenAPI 3.1 built at runtime** per tenant, cached by `registry_version`, served at `/doc`.
- **GraphQL** via `async-graphql::dynamic`, `Arc<Schema>` keyed by `registry_version` and rebuilt lazily (Twenty bumps a schema version [V]); second phase. Seaography is entity-static.
- Both surfaces call the **same service layer** so ACL, rules, audit and hooks cannot be bypassed.

---

## 13. Proposed layered architecture for obop

```
L8  Clients         Vite+ web app (list/form/kanban views driven by metadata), CLI, integrations
L7  API surface     axum: generic REST (/api/v1), JSON-2-style call endpoint, dynamic GraphQL,
                    runtime OpenAPI, webhooks/SSE; auth (API keys, sessions, OIDC); rate limits
L6  Application     Business services (posting, reconciliation, MRP, pricing), automation/workflow,
                    onchange engine, reports, import/export, job runner + transactional outbox
L5  Model runtime   Unit of Work + Recordset API; CRUD pipeline (defaults -> hooks -> constraints ->
                    write -> computed flush -> audit -> outbox); ACL + record rules; sequences;
                    row_version concurrency; retry on 40001/40P01
L4  Query layer     Domain AST -> type-check -> optimise -> + record rules -> sea-query/sqlx -> PG;
                    read_group; properties JSONB operators; search on computed fields
L3  Registry        ModelDef/FieldDef/HookChain from (a) derive statics via inventory, (b) module
                    manifests + dependency order, (c) tenant meta_model/meta_field rows;
                    Arc<Registry> per tenant, registry_version, signalling table / NOTIFY
L2  Schema manager  diff(registry, pg_catalog) -> classified DDL plan -> runner (lock_timeout,
                    concurrent index steps, batched backfill); module migrations + custom-field DDL;
                    DDL_LOCKED flag; migration log
L1  Storage         PostgreSQL 18.x, one DB per tenant (RLS backstop on company_id; ledger triggers);
                    object_store blobs; control-plane DB (tenants, modules, licences)
L0  Kernel crates   obop-types (Id, Money minor units, Decimal, Value), obop-macros (derive Model),
                    obop-sql (quoting, bind helpers), obop-testkit (PG per test)
```

Invariant placement: structure/typing in L2 + PG; authorisation in L5 (+RLS backstop); ledger balance, immutability and period locks in PG triggers surfaced as domain errors; numbering in a PG row-lock counter; dependency recompute in L5 inside the transaction; API shape generated L3 → L7.

---

## Confidence & gaps

- **High [V]:** Odoo mechanics (reflection, registry signalling, domains, rules, trigger tree, onchange, sequences, balance check, attachments, JSON-2, version status); Frappe concurrency/naming/custom-field DDL; Twenty schema-per-workspace and DDL; PG 18 behaviour quoted from official docs; licences and release metadata from GitHub.
- **Medium [S/I]:** multi-tenancy thresholds (Crunchy is a vendor blog; opinion); all design recommendations; catalog-size arithmetic; pgledger throughput is the author's laptop benchmark.
- **Unverified [U]:** Salesforce per-edition limits and Metadata API; whether any EU member state mandates strictly gapless invoice series, and post-2020 EU amendments to Art. 226 (ViDA); Odoo Studio internals (Enterprise-only status is only inferred from the absence of `web_studio` in the Community tree); Hasura v3 engine (presumed closed); Airtable internals; `utoipa` 6 runtime suitability; Notion's current shard layout after 2023. Resolved in the verification pass: Frappe site-per-database, crate licences (all `MIT OR Apache-2.0`), Salesforce multitenant architecture page.
- **No benchmarks gathered:** JSONB-vs-column performance, RLS overhead, schema-per-tenant degradation and DDL lock impact are reasoned from docs (see MUST-3).
- **Tooling limits:** no web search for the original author; the verification pass used the GitHub API, direct raw-file fetches and page fetches; "no concurrency check in Odoo master" means not found in `odoo/orm/models.py`, not exhaustively proven; Twenty's large tree was read only in the relevant directories.

---

## Implications for obop

**MUST**
1. **Three-tier field storage:** typed real columns for module and promoted custom fields; one JSONB "properties" column with definitions on a container record for sparse no-DDL fields; **no EAV** (Odoo removed `ir.translation` and `ir.property`). Ship a "promote to column" migration from day one.
2. **Registry as single source of truth:** `#[derive(Model)]` → static `ModelDef` via `inventory`, merged at tenant start with `meta_model/meta_field` rows into an `Arc<Registry>` with `registry_version`; query compiler, API generation, schema manager, onchange and ACL read only from it.
3. **Benchmark tenancy before fixing numbers:** synthetic tenant with obop's projected table count; compare schema-per-tenant vs database-per-tenant at 100/500/1,000 tenants (catalog size, relcache memory, `pg_dump`, migration fan-out, `max_locks_per_transaction`). Default to **database per tenant** until data says otherwise.
4. **Domain AST + optimiser + SQL compiler** on `sea-query`/`sqlx`: bind parameters, registry-only identifiers, node/depth/list budgets, NULL-safe negation, `any`-style relational subqueries; record rules injected as Domains before SQL generation.
5. **Ledger integrity in PostgreSQL, not only Rust:** deferred constraint trigger on the entry header for debit = credit at commit, immutability and period-lock triggers for posted lines, `REVOKE UPDATE/DELETE` from the app role, integer minor units. Odoo's Python-only check is what to improve on.
6. **Legal document numbers from a row-locked counter at posting time** (not PG sequences), with a unique-index backstop and per-journal gapless flag.
7. **`row_version` optimistic concurrency** (ETag/If-Match) and a retry wrapper for SQLSTATE `40001/40P01`.
8. **Custom-field DDL through a diff/plan/run schema manager:** step classification, `lock_timeout`, concurrent index builds outside the transaction, stable field ids separate from labels, two-phase drops, migration log.
9. **Treat Directus, NocoDB and Twenty as design references only** (licences above; Baserow's MIT portion is reusable but is Python); keep the Odoo work clean-room.

**SHOULD**
10. **Overlay customisation on immutable shipped metadata** (Frappe `Custom Field`/`Property Setter`) so upgrades never clobber customer changes.
11. **Generic REST plus an Odoo-JSON-2-compatible call endpoint** (`/json/2/<model>/<method>`, bearer API keys) and runtime OpenAPI at `/doc`; do not target XML-RPC (removal: `db` in 20, rest in 22).
12. **Odoo-style trigger-tree recomputation** (inverse-relation paths, batch compute, flush before commit, cycle detection); DB invariants on base columns only.
13. **Onchange as a pure function over a `RecordView` trait** shared by DB and virtual records, bounded fixed-point loop, snapshot diff.
14. **RLS only as a backstop** (`company_id`, restrictive policy, `FORCE RLS`, per-transaction `set_config`); app-level rule pushdown stays primary (global AND, group OR, superuser bypass, per-mode caches).
15. **Three audit layers:** ORM field-level change log, DB trigger history for non-ORM writes, hash chain on posted accounting documents.
16. **Separate `active` (archive) from hard delete + `deleted_record` archive**, with partial unique indexes.
17. **`object_store` attachments** with sha256 addressing, pending→committed state and GC, authorisation via the parent record.
18. **Use PG 18 deliberately:** `uuidv7()` ids, application-time temporal constraints for validity periods, virtual generated columns for cheap derived values, `RETURNING OLD/NEW` for audit/outbox. Minimum PG 18 (18.6 current, released 2026-08-13, PG 18 first released 2025-09-25, final release 2030-11-14; PG 19 is beta 4 as of 2026-09-24, GA date not announced on [postgresql.org/support/versioning](https://www.postgresql.org/support/versioning/)).

**COULD**
19. **GraphQL** via `async-graphql::dynamic`, rebuilt on `registry_version` bumps, after REST stabilises (watch its slow release cadence).
20. **Pooled row-level tenancy mode** (tenant_id + RLS, Notion-style schema shards) behind the `TenantRouter` seam for very small SaaS tenants.
21. **Compile simple compute/onchange expressions to the client** to cut round trips.
22. **Borrow SeaORM 2.0's entity-registry/schema-sync ideas** or use its introspection, but only behind obop's own registry, since its entities are static.
23. **Evaluate XTDB or an event-sourced core** only if a concrete bitemporal reporting requirement appears; default to state tables + append-only ledgers + outbox.

---

## Verification log

Adversarial fact-check run 2026-10-01 (sources: GitHub API and raw files, crates.io API, postgresql.org docs, odoo.com docs and nightly.odoo.com, supabase.com, notion.com, airtable.com, architect.salesforce.com, legislation.gov.uk). No WebSearch tool was available; discovery was by direct URL and GitHub API.

**Confirmed as stated: about 115 discrete claims**, including all 12 author-flagged load-bearing claims (with the corrections below where a sub-claim was off): Odoo 20.0 FINAL / master 20.1 ALPHA, LGPLv3, `db` service removed in 20 (`odoo/service/db.py` present on 19.0, absent on 20.0), `common`/`object` removal in Odoo 22 (fall 2028), JSON-2 bearer API; JSONB for `company_dependent`/`translate` (`fields.py` L891) and the `ir_property`/`ir_translation` file-presence history; Properties storage; `_check_balanced` in Python plus `check_move_validity` flag; `no_gap` `FOR UPDATE NOWAIT`; PG 18.6 / 2025-09-25 / PG 19 Beta 4 (2026-09-24) and the PG 18 features; sequence gap wording; deferrable-constraint rules and constraint-trigger restrictions; Twenty v2.44.0 (2026-10-01), `workspace_<base36>` schema names, runtime DDL, `WORKSPACE_SCHEMA_DDL_LOCKED`; Directus/NocoDB/Baserow/Frappe/ERPNext licences; Crunchy tier numbers; Notion 480 shards / 32 DBs / 96 x 5; sqlx 0.9.0, sea-orm 2.0.4 (Entity First, RBAC), sea-query 1.0.2, inventory 0.3.24, object_store 0.14.2; Frappe `TimestampMismatchError` and `tabSeries ... FOR UPDATE`; Odoo master lacks `__last_update` and runs `REPEATABLE READ`. Also confirmed: release versions/dates for Frappe, Twenty, Directus, Baserow, NocoDB, PocketBase, Hasura, PostgREST, pg_graphql, ndc-postgres; Odoo registry signalling tables, domain optimiser levels, record-rule composition, trigger tree, onchange location, attachment filestore, hash chain; Twenty metadata uniqueness, 25 field types, currency composite; Frappe `Custom Field` DDL, `Deleted Document`, `Version`; Directus/Baserow/NocoDB/PocketBase code evidence; Salesforce multitenant tables, flex columns, 32,000-char CLOB, 15-day recycle bin, pre-queries; Airtable record and rate limits; PG limits, ALTER TABLE lock and rewrite behaviour, TOAST 2 kB, partition guidance, `max_locks_per_transaction` default 64, RLS semantics; pgaudit, supa_audit, temporal_tables, XTDB, pgledger (494 stars, v0.7.0), Formance, TigerBeetle metadata; Supabase RLS advice.

**Corrections (was / now / source)**

| # | Was | Now | Source |
|---|---|---|---|
| 1 | ERPNext "v15.121.6" as latest | ERPNext v16.37.0 (2026-09-30) is latest; v15.121.6 is the v15 maintenance line | https://github.com/frappe/erpnext/releases |
| 2 | async-graphql "7.2.1 (2026-04-21)" | 7.2.1 stable was 2026-01-20; 2026-04-21 is 8.0.0-rc.5, the newest (pre)release and last commit | https://crates.io/api/v1/crates/async-graphql |
| 3 | sea-orm licence "Apache-2.0"; crate licences "unverified" | `MIT OR Apache-2.0` for sea-orm and the other listed crates (`object_store`: MIT/Apache-2.0) | https://crates.io/api/v1/crates/sea-orm |
| 4 | "Directus, NocoDB, Twenty and Baserow code cannot be ported" (incl. MUST 9) | Baserow outside `premium/` and `enterprise/` is MIT ("Baserow OSE"), so reusable with attribution (but Python); the other three stand | https://raw.githubusercontent.com/baserow/baserow/develop/LICENSE |
| 5 | Crunchy quote "generally not beyond several thousand" | Actual wording "generally not to exceed several thousand of tenants"; post is dated 2023-11-14 | https://www.crunchydata.com/blog/designing-your-postgres-database-for-multi-tenancy |
| 6 | Supabase "only published timing is one 171 ms example" | Supabase's RLS performance guide publishes a before/after table (171 to <0.1 ms, 179 to 9 ms, 173,000 to 16 ms, ...) | https://supabase.com/docs/guides/troubleshooting/rls-performance-and-best-practices-Z5Jjwv |
| 7 | Twenty "SDK packages MIT" | MIT covers `twenty-sdk`, `twenty-client-sdk`, `create-twenty-app`, `twenty-shared`, `twenty-ui`, `packages/twenty-apps`; AGPL has a "Twenty Application Exception" | https://raw.githubusercontent.com/twentyhq/twenty/main/LICENSE |
| 8 | Rust ERP "nothing above 53 stars" | Still true as a search result, but the 53-star hit (`print-bridge`) is not an ERP; real ERPs are forgeerp 40, idEAS 27, rerp 23, obvia 16 | GitHub search `erp language:rust`, 2026-10-01 |
| 9 | `ERPNext alone ships >700 DocTypes [V]` | README says "more than 700 object types"; DocType JSON count on `develop` is 534 (erpnext) + 305 (frappe) | https://github.com/frappe/frappe (README), repo trees |
| 10 | RLS backstop: a `RESTRICTIVE` policy alone | A restrictive-only table exposes no rows; a `PERMISSIVE ... USING (true)` policy is required alongside | https://www.postgresql.org/docs/current/sql-createpolicy.html |
| 11 | Odoo `__last_update` check "16.0 had, master lacks" | Absent from 17.0, 18.0, 19.0 and master | raw `odoo/models.py` per branch |
| 12 | `version_info` shown as a 5-tuple | 6-tuple with trailing `''` | https://raw.githubusercontent.com/odoo/odoo/20.0/odoo/release.py |
| 13 | pgledger ">10,000 transfers/s at ~1.9 ms" | 10,637/s at 1.9 ms low contention; 7,559/s at 2.6 ms with hot accounts; M3 MacBook Air, PG 17.5 | https://pgrs.net/2025/05/16/pgledger-in-postgresql-is-fast/ |
| 14 | Frappe site-per-DB `[U]`; EU VAT Art. 226 `[U]`; Hasura v3 engine `[U]`; Studio Enterprise-only "not verifiable" | Verified from `installer.py` (random `_<hash>` db per site); Art. 226(2) wording confirmed via legislation.gov.uk; no v3 engine repo in the hasura org (presumed closed, inferred); no `web_studio` module in the Community tree (inferred) | see §2.1, §2.2, §11 |

**Facts added (missing but important):** Odoo 20 was released September 2026 and is the recommended version (nightly.odoo.com); PG 18 `NOT NULL ... NOT VALID` support (release notes); Directus MSCL converts to GPL-3.0 after four years and only bars "Competing Use"; Notion figures are from 2021 and 2023 posts; `mail.tracking.value` moved to the `mail_tracking` addon on master; Frappe now has sqlite and duckdb backends; async-graphql has had no stable 8.0 after five 2026 RCs; XTDB 2.2 betas and Formance v3 betas are in progress; `supa_audit` is archived; PG 19 GA date not announced.

**Unresolved**
- Salesforce per-edition limits and Metadata API details (not fetched).
- Whether any EU member state mandates strictly gapless invoice numbering, and whether post-2020 EU amendments (ViDA) change Art. 226 (EUR-Lex returned empty; only a 2020-era copy was read).
- Odoo Studio internals and its Enterprise-only status (inferred, not proven).
- Hasura v3 engine being closed-source (inferred from an org repo listing).
- Airtable internal storage model; `utoipa` 6 suitability for runtime OpenAPI building.
- Notion's shard layout after July 2023.
- "Postgres is secondary to MariaDB" for Frappe (current project status, marked unverified in the text).
- All `[I]` design recommendations, catalog-size arithmetic and tenancy thresholds remain reasoned, not benchmarked; Crunchy's guidance is a vendor blog from 2023.
