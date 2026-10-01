# 23. obop reference architecture v1: kernel model, integration contract and binding decisions

Research date 2026-10-01. Tags: **[V]** verified today at the cited source; **[I]** inference or design judgement; **[G]** guess or training memory, not verified.

## Scope & method

I read docs 02, 06, 08, 18, 22 and the entity sections of 09-14 (plus 12, 15, 16, 20), then re-checked contested facts on 2026-10-01 against primary sources: PostgreSQL source (`async.c`, `lmgr.c`, `copy.c`, `rls.c` on `master`, `REL_18_STABLE`, `REL_19_BETA4`) and docs; Odoo 20.0 source, docs and the 641 addon manifests (642 directories, one has no manifest; parsed by me); ERPNext, Tryton, Dolibarr, Spring Modulith, ABP, Medusa and Business Central docs (GitHub API); cloud PostgreSQL docs. **Not done:** no PostgreSQL runtime was available (no local PG, Docker stopped, and pulling binaries needs explicit permission), so the DR-1 lock is verified in source and its spike is specified, not executed. NetSuite docs returned 403/404 (so [G]); IFRS standard texts are paywalled. **T0** = first shippable tier (single-company invoicing-to-stock loop); **T1** = second tier (services, manufacturing, POS, HR, web).

---

## Part A: binding decisions

### DR-1 Realtime and queue wake-up (NOTIFY versus outbox)

**Context.** Docs 02, 06, 08, 15 and 18 plan `LISTEN/NOTIFY` for bus, queue wake-up and registry signalling; doc 22 (1.4) says NOTIFY commits serialise cluster-wide and LISTEN is per database.

**Options.** (A) `pg_notify` inside business transactions. (B) Outbox plus polling. (C) Outbox plus in-process wake-up plus a coalescing post-commit notifier on one bus database. (D) External broker (NATS) or logical decoding.

**Evidence.**
- [V] `PreCommit_Notify` calls `LockSharedObject(DatabaseRelationId, InvalidOid, 0, AccessExclusiveLock)` and keeps it until after commit: line 1309 on `master` and `REL_19_BETA4`, line 921 on `REL_18_STABLE`. The source comment calls writer serialisation a cluster-wide heavyweight lock on "database 0" (https://github.com/postgres/postgres/blob/master/src/backend/commands/async.c).
- [V] `LockSharedObject` builds its tag with `InvalidOid` as database (https://github.com/postgres/postgres/blob/master/src/backend/storage/lmgr/lmgr.c), so every database in the cluster shares the lock. Listener keys are `(dboid, channel)`, so LISTEN is per database.
- [V] The 2026-01-15 commit "Optimize LISTEN/NOTIFY via shared channel map and direct advancement" cuts wake-ups but leaves the lock. Recall.ai (secondary) had three outages on 2025-03-19..22 with COMMIT waiters above 1 s; its page (first published 2025-07-01, updated 2026-05-08) later says core fixed it, which today's source contradicts for the commit lock (https://www.recall.ai/blog/postgres-listen-notify-does-not-scale).
- [V] `synchronous_commit` can be set per transaction (https://www.postgresql.org/docs/18/runtime-config-wal.html).
- [I] The lock spans the synchronous WAL flush, so cluster-wide NOTIFY-commit throughput is about 1/commit-latency (roughly 500-2,000 per second at 2-0.5 ms), and a waiter also holds its earlier row locks (for example a gapless counter row) longer.

**Decision (binding).**
1. `NOTIFY`/`pg_notify` is forbidden in business transactions; CI allows it only in crate `obop-bus-notifier`.
2. Durable events are outbox rows plus per-subscriber delivery rows (B.1, D.2).
3. Wake-up ladder: same node `tokio::sync::Notify` after commit; cross-node a coalescing notifier; fallback polling with 250 ms to 2 s backoff.
4. The notifier runs one tiny transaction with `SET LOCAL synchronous_commit = off`, at most 10 per second per node, on one **bus database** per cluster. Each app node holds one LISTEN connection, on the bus database only, never on business databases. Cluster NOTIFY commits are then capped at nodes x 10 per second whatever the tenant load, and the lock is held for microseconds.
5. Browser realtime is a WebSocket fed by the outbox dispatcher, resuming from `(xid8, event_id)` restricted to `xid < pg_snapshot_xmin(pg_current_snapshot())`, because UUIDv7 ids are not commit-ordered (doc 22, 7.2).
6. Single-business installs use the same path, with bus database = business database. NATS JetStream stays a COULD.

**Reversal cost.** Low to medium: a `WakeBus` trait hides the transport and the outbox schema is unchanged.

**Validation spike (specified, not executed).** On PG 18 (then 19 RC) create `t00..t31`, each with `outbox(id bigserial, payload jsonb)`, and run 32 pgbench processes (`-c2 -j1 -T60 -r`, one per database):
```
-- plain.sql
INSERT INTO outbox(payload) VALUES ('{}');
-- notify.sql
BEGIN; INSERT INTO outbox(payload) VALUES ('{}'); SELECT pg_notify('wake',''); COMMIT;
-- coalesced: plain.sql plus one session doing pg_notify every 100 ms on database bus (synchronous_commit=off)
```
Pass [I]: `notify.sql` summed TPS stays near a single-database run while `plain.sql` scales with I/O, and `coalesced` reaches at least 90% of plain. If `notify.sql` shows no contention, downgrade rule 1 to SHOULD; rule 4 stays because per-database LISTEN is independently fatal. Add a Rust harness (tokio-postgres, HDR histogram) before GA.

### DR-2 Tenancy unit and vocabulary

**Context.** Doc 08 says database per tenant with RLS as backstop; doc 18 says silo plus pool; doc 22 assumes cells of about 250 databases.

**Options.** (A) Database per business. (B) Schema per business. (C) Pool: shared schema, `tenant_id`, RLS. (D) Silo default, pool reserved.

**Evidence.**
- [V] Odoo: database per tenant; Frappe: per site; Twenty: schema per workspace (doc 08; Twenty still has a `workspace-schema-manager` in `packages/twenty-server`, github.com/twentyhq/twenty). [V, secondary] Crunchy Data, 2023-11-14: database-per-tenant suits tens of tenants (it advises avoiding it at about 50 or more), schema-per-tenant hundreds, shared tables with a tenant column millions (https://www.crunchydata.com/blog/designing-your-postgres-database-for-multi-tenancy). The 250-database cell is therefore outside that guidance and rests on the spike below [I].
- [I] At 600-900 tables per ERP tenant (doc 22), 250 databases is about 190k tables per cluster (`pg_class` holds several times that once indexes and TOAST tables count).
- [V] The autovacuum launcher starts one worker per database every `autovacuum_naptime` (a launch every naptime/N seconds) (https://www.postgresql.org/docs/18/routine-vacuuming.html). The NOTIFY lock is cluster-wide (DR-1), which also bounds a cell's blast radius.

**Decision.** Database per **business** is the only GA mode. Pool mode is not built but kept open by three rules: `company_id` on every row, a `TenantRouter` trait, and no cross-business SQL. If pool mode is built, `tenant_id` leads every key, foreign keys are composite and RLS is primary (doc 18, 4). Cells start at no more than 250 business databases per cluster [I].

| Term | Definition in obop |
|---|---|
| Tenant | Commercial/billing principal in the control plane; owns one or more businesses; absent from business databases. |
| Business | Isolation and deployment unit: one PostgreSQL database, one module set, one user directory, one object-store prefix, one backup unit. |
| Legal entity | A juridical person, stored as an organisation `party` with tax registrations. |
| Company | The ledger-owning scope of one legal entity: own books, functional currency, fiscal calendar, journals, sequences, lock dates. Counterparty entities are parties only. |
| Branch | Subdivision of a company with no books of its own: own address, warehouses, POS, defaults; an optional dimension on documents and journals. |

**Reversal cost.** Very high if `company_id` discipline is skipped; medium to add pool mode later (column plus policies).

**Validation spike.** Generate 250 databases of 800 tables from registry DDL on PG 18; measure `CREATE DATABASE` time, catalog size, idle memory, autovacuum lag, `pg_dump`, migration fan-out and PgBouncer pools. Pass if fan-out migration takes under 30 minutes and idle memory stays under 4 GB [thresholds I].

### DR-3 PostgreSQL minimum (16 versus 18)

**Context.** Odoo requires 16 (`MIN_PG_VERSION = 16` [V]); docs 08 and 22 say 18, doc 18 says 16/17/18, doc 02 says 16.

**Evidence [V unless noted].**

| Platform | PostgreSQL 18 today | Source |
|---|---|---|
| AWS RDS | yes, since 2025-11-14 | docs.aws.amazon.com/AmazonRDS/latest/PostgreSQLReleaseNotes/postgresql-release-calendar.html |
| Azure Flexible Server | yes, minor 18.6 (page updated 2026-08-26) | learn.microsoft.com/en-us/azure/postgresql/configure-maintain/concepts-supported-versions (canonical URL; the old `flexible-server/` path is the one first cited) |
| Google Cloud SQL | yes, default (18.6, regular support from 2025-09-25) | docs.cloud.google.com/sql/docs/db-versions (cloud.google.com 301-redirects here) |
| Neon | 14 to 18 | github.com/neondatabase/website (postgres-version-support.md) |
| Supabase | no: 15, 17, OrioleDB-17 in `ansible/vars.yml` on the default branch `develop`; Supabase's upgrade guide mentions only 15 and 17 (hosted previews not checked) | github.com/supabase/postgres; supabase.com/docs/guides/platform/upgrading |
| Debian 13 stable | 17 (testing, forky: 18) | packages.debian.org/trixie/postgresql |
| Ubuntu 26.04 LTS | 18 (`18+290ubuntu1`) | packages.ubuntu.com/resolute/postgresql |

- PG 18.6 is current (2026-08-13; 18.5 was never released, so 18.6 follows 18.4, https://www.postgresql.org/docs/18/release-18-6.html); PG 19 is at Beta 4 (2026-09-24), no GA (https://www.postgresql.org/support/versioning/). The Beta 4 announcement plans a release candidate for early October 2026 and says GA "may also occur in October" (https://www.postgresql.org/about/news/postgresql-19-beta-4-released-3386/). EOL (https://www.postgresql.org/support/versioning/): 16 on 2028-11-09, 17 on 2029-11-08, 18 on 2030-11-14.
- `WITHOUT OVERLAPS` and `VIRTUAL` generated columns are in the 18 `CREATE TABLE` docs and absent from 17's. `btree_gist` supports `uuid` (postgresql.org/docs/17/btree-gist.html), so `EXCLUDE USING gist (company_id WITH =, product_id WITH =, valid_during WITH &&)` gives the same overlap guarantee on 17; only the temporal foreign key (`PERIOD`) is 18-only.

**Decision.** Support **17 and 18** at 1.0; 18 is the reference. CI runs 17, 18, and 19 from RC. Kernel DDL uses 17-level features only; 18-only features (temporal FKs, virtual columns, `uuidv7()` default, `RETURNING OLD/NEW`, `NOT NULL ... NOT VALID`; each is in the PG 18 docs and absent from PG 17's) sit behind a `requires_pg = 18` capability flag. Raise the floor to 18 when PG 17 is 12 months from EOL (2028-11) or Debian stable and Supabase ship 18, whichever is first. Floor 16 is rejected: assuming a 2027 GA it has under two years of support left, and it lacks the PG 17 incremental backup that doc 18 relies on.

**Reversal cost.** Low to raise the floor; high to lower it after 18-only features are used, hence the flag.

**Validation spike.** Run migrations and the integration suite on 17 and 18; `EXPLAIN`-compare pricing/tax validity queries (EXCLUDE index versus temporal key).

### DR-4 Authorization algebra, field-level security, RLS

**Context.** Odoo 20 merged ACLs and record rules into `ir.access`; doc 18 wants default-deny; doc 08 wants RLS as backstop only.

**Evidence.**
- [V] Odoo 20 `_access_domain` returns `Domain.OR(permissions) & Domain.AND(restrictions)`: group-bound rows are permissions, group-less rows restrictions, `_inherits` parents add `any` restrictions. The empty OR is `_FALSE_DOMAIN`, so no matching permission means denial (odoo/orm/models.py L3603; odoo/orm/domains.py L740, branch 20.0). The developer docs still describe the old pair (doc 02).
- [V] Odoo HR field groups do not propagate to related fields (doc 13), so enforce in the data layer. Cedar partial evaluation is still experimental (docs 06, 18).
- [V] `COPY FROM` fails with a row-level-security error only when `check_enable_rls` returns `RLS_ENABLED`; `BYPASSRLS` roles and owners without `FORCE` pass (`copy.c` L230, `rls.c`). `COPY TO` applies SELECT policies (postgresql.org/docs/18/sql-copy.html).
- [V] RLS quals run before user quals unless `LEAKPROOF`, and RI checks bypass RLS (https://www.postgresql.org/docs/18/ddl-rowsecurity.html). pgvector filters after the ANN scan with approximate indexes; since 0.8.0 `hnsw.iterative_scan` keeps scanning until enough rows pass (https://github.com/pgvector/pgvector#filtering).

**Decision.**
1. The algebra is Odoo 20's (OR of permissions, AND of restrictions), compiled from the Domain AST to SQL; one path serves list, by-id, export, webhook payload and AI tool reads.
2. Default-deny at model, field and action level. Every permission row declares a scope (`all | own | company | domain`); an implicit TRUE is a lint error. Restrictions only narrow (company, soft-delete, embargo, residency).
3. Elevation is a typed, audited `Env<Elevated>`, never a global sudo.
4. **Field-level security** is enforced in the registry projection (API, export, report, AI output) and in the query compiler, which rejects filter, sort and group-by on unreadable fields (closing the inference channel). Classes `confidential_hr`, `special_category`, `financial` also live in separate tables or columns granted only to a fixed set of DB roles (`obop_cls_*`), chosen per request by `SET LOCAL ROLE` in `TenantTx`.
5. **RLS is a backstop in silo mode and primary only in pool mode.** The policy set is fixed and row-local: `company_id = ANY(nullif(current_setting('obop.company_ids', true), '')::uuid[]) OR company_id IS NULL` as RESTRICTIVE plus a PERMISSIVE `true`, `FORCE ROW LEVEL SECURITY`, a non-owner non-BYPASSRLS runtime role, `set_config(..., true)` per transaction.
6. **HR:** RLS gives only the coarse gate (company plus class role). Manager-hierarchy visibility stays in the app, since hierarchy subselect policies are slow and racy under READ COMMITTED.
7. **AI retrieval:** candidates come from `ai_chunk` (RLS on `company_id`) with over-fetch, then parent records are re-checked through the normal path.
8. **Bulk import:** `COPY` into an unlogged staging schema without RLS, validate set-based, then `INSERT ... SELECT` under `TenantTx`. No BYPASSRLS importer role: it works but widens blast radius.

**Reversal cost.** High for the algebra (every module's seed access data); low for the RLS policy set.

**Validation spike.** A property test over random (principal, record, operation) triples must give identical answers from the list query, by-id check, export path and an RLS-enabled connection. Benchmark the restrictive policy on 10M `journal_line` rows using the `(select current_setting(...))` InitPlan form.

### DR-5 Money and quantity storage (a rule per table family)

**Evidence.**
- [V] Odoo stores `Monetary` as `numeric` but rounds floats (`float_round`), defaults "Product Price", "Discount" and "Product Unit" precision to 2 digits, and leaves `rate` digits unconstrained (`product_data.xml`, `uom_data.xml`, `res_currency.py`).
- [V] rust_decimal has a 96-bit mantissa and scale at most 28 (doc 06).
- [V] CLDR 48 (cldr-json 48.2.3, 2026-09-30; CLDR 49 is in beta since 2026-09-23) shows currencies with 0 digits (JPY, KRW, ...), 3 (BHD, JOD, KWD, LYD, OMR, TND) and 4 (CLF, UYW) (unicode-org/cldr-json, `cldr-core/supplemental/currencyData.json`). CLDR also encodes display conventions (for example HUF at 0, with cash rounding 5) that differ from ISO 4217, which lists HUF at 2 [V, secondary: en.wikipedia.org/wiki/ISO_4217], so load the ISO exponent as data.
- [V] EN 16931 BR-CO-17: VAT category tax amount = taxable amount x rate/100, rounded to two decimals, per VAT breakdown. BR-DEC rules cap totals, VAT breakdown amounts and line net amount (BT-131) at 2 decimals; unit price (BT-146) has no cap (ConnectingEurope/eInvoicing-EN16931, `EN16931-model.sch`). The executable UBL Schematron (`ubl/schematron/preprocessed/EN16931-UBL-validation-preprocessed.sch` on `master`) accepts a VAT amount within 1.00 currency unit of the rounded product, so the validator is looser than the rule text; obop should still emit the exact rounded value [V].

**Decision.**

| Table family | Storage | Rule |
|---|---|---|
| Ledger: `journal_line`, payments, bank and tax lines, balance rollups | `bigint` minor units plus `currency_code`; exponent from `currency.decimal_places`, immutable once used | Never read minor units without the currency. `debit >= 0 AND credit >= 0 AND debit*credit = 0`. The balanced-entry trigger sums integers (doc 08). |
| Document totals (untaxed, tax, total, residual) | `bigint` minor, persisted | Computed in Rust with `Decimal`, rounded once per the company profile, equal to ledger postings. |
| Unit price, cost, pricelist amounts | `NUMERIC(24,8)` | More decimals than the currency allowed; line net rounds to the currency when persisted. |
| Quantities | `NUMERIC(24,6)` in base UoM, authoritative; entry qty and UoM kept | UoM `rounding` applies at entry; conversion rounds half-up. |
| FX rates | `NUMERIC(28,14)` with `rate_type` (spot, average, closing, historical) and source | The applied rate is stored on each foreign line; history is never re-derived. |
| Tax rates / percentages | `NUMERIC(12,8)` / `NUMERIC(9,6)` | Fixed per-unit taxes `NUMERIC(24,8)`. |
| Valuation | `value_minor bigint` authoritative; unit cost derived | AVCO residual goes to the last outbound entry. |
| Analytic / timesheet | `amount_minor bigint`, `unit_amount NUMERIC(24,6)` | |

Rounding profile per company: `per_line` or `per_group`. `per_group` is mandatory for EN 16931 output: totals by (tax, rate), rounded once, and the EDI serializer asserts BR-CO-10..16 before emit. Cash rounding is an explicit line. Default is half-up [G: jurisdictions differ]. Floats are banned in money/quantity paths (`clippy::disallowed_types`) and release builds set `overflow-checks = true` (doc 18). Assets needing more than 18 significant digits (crypto) are not GA ledger currencies.

**Reversal cost.** High (ledger storage); mitigated by one `Money` newtype and one ledger crate.

**Validation spike.** Property tests: Rust totals equal SQL `SUM`s of persisted integers; generated invoices pass the KoSIT validator; a 10M-line replay compares `per_line` and `per_group`.

### DR-6 Primary keys and external identifiers

**Options.** (A) `bigint` identity (Odoo, Tryton). (B) UUIDv7 everywhere. (C) `bigint` PK plus public UUID.

**Evidence.** [V] PG 18 has native `uuidv7()`; RFC 9562 (May 2024) defines version 7; `uuid` crate 1.26.1 (2026-09-10, crates.io), whose `Cargo.toml` has a `v7` feature (`v7 = ["rng"]`). [V] Odoo POS already uses a client UUID with `unique(uuid)` for idempotent offline orders (doc 12). [V] UUIDv7 embeds a timestamp, so ids are never capabilities (doc 18). [I] Benefit: ids stay stable across databases, so tenant moves, interco mirrors, imports and consolidation need no remapping, and client generation needs no mapping tables. Cost: 16-byte keys against 8 (about +20% ledger row width).

**Decision.**
1. Every registry model has an application-generated `uuid` PK (monotonic UUIDv7); DB default `uuidv7()` on 18, a SQL polyfill on 17; Rust type `Id<M>` (phantom-typed newtype).
2. ISO catalogs (currency, country, language) use natural text codes (`currency.code char(3)`).
3. Module-shipped records are addressed across environments by **external id** `module.name`.
4. Mutating commands carry a client `command_id uuid` with a unique index; human references come from sequences.
5. `created_at` is an explicit column; the timestamp inside a UUID is not trusted.
6. `IdType` is a per-model registry flag: if the spike fails, append-heavy tables (ledger, stock moves, logs) switch to `bigint` plus a public `uuid` without touching module code.

**Reversal cost.** Very high once data exists, hence the spike first.

**Validation spike.** Load 300M synthetic `journal_line` rows with `bigint` and UUIDv7 keys; compare table/index size, insert TPS and buffer-hit ratio on partner-ledger and product-move queries. Accept at most +25% size and 15% slowdown on the OLTP mix [thresholds I].

### DR-7 Edge/POS datastore (doc 12 SQLite hub versus doc 22 no SQL dual-targeting)

**Options.** (A) SQLite as a purpose-built edge store. (B) Dual-target domain SQL. (C) Embedded PostgreSQL on the hub, PGlite in browser. (D) PowerSync, Electric or Zero.

**Evidence.** [V] The sync engines run non-Rust servers (PowerSync service under FSL-1.1-ALv2, github.com/powersync-ja/powersync-service; Zero without offline writes, zero.rocicorp.dev/docs/offline; doc 22). [V] Odoo POS mirrors models into IndexedDB with client-UUID orders; the `opfs-sahpool` SQLite VFS needs no COOP/COEP headers (doc 12; sqlite.org/wasm/doc/trunk/persistence.md recommends it for clients unable to set them, at the cost of multi-tab concurrency). [V] SQLite 3.53.4 (2026-07-24, sqlite.org); `rusqlite` 0.40.2 (2026-08-08, crates.io); Turso is pre-1.0 (latest tag v0.8.1, 2026-09-29, github.com/tursodatabase/turso; doc 22). [I] Kernel SQL uses PG-only features (jsonb, exclusion constraints, RLS, `RETURNING OLD/NEW`), so (B) means two domain SQL layers.

**Decision.** The docs conflict only if "SQLite" means the domain database. So:
1. **Server and kernel are PostgreSQL only.**
2. **SQLite is only the edge store**: a fixed schema of at most 15 tables owned by crate `obop-edge` (catalog, price and tax snapshots, open orders, `command_outbox`, device state). It is not a projection of the server schema and has no registry, ORM or dynamic DDL; migrations use `PRAGMA user_version`.
3. Shared logic is pure Rust with no SQL (pricing, tax, promotions, rounding), compiled native and to WASM. The wire contract is the `pos-api` command/snapshot schema with UUIDv7 command ids, replayed server-side through normal domain services.
4. The store hub is the same binary in role `hub` on `rusqlite`; browser-only mode uses sqlite-wasm with `opfs-sahpool` in a SharedWorker.
5. Solo and demo installs use embedded PostgreSQL, never SQLite.

**Reversal cost.** Low to medium: the edge store is a leaf and the protocol is the asset.

**Validation spike.** Two tablets plus a hub run one hour offline, queue 10k orders, sync with `kill -9` mid-sync; check exactly-once on `unique(command_id)` and measure sahpool write latency.

---

## Part B: kernel entity model

**Principles.** (1) Every business row has `id uuid`, `company_id uuid` (NULL only on business-wide masters), `row_version bigint`, audit stamps and `active`. (2) The registry owns the shape (doc 08). (3) Documents never share tables with ledger rows; posting is an explicit command producing immutable entries. (4) Progress statuses are derived from link tables. (5) Polymorphic references exist only as `RecordRef(model_id, record_id)` for thread, attachment, audit and external id, with orphan GC (Odoo's `Many2oneReference` has no integrity checks [V doc 02]).

### B.1 Canonical ownership

| Concept | Owner | Canonical tables and rules | Precedent |
|---|---|---|---|
| Party | `base` | `party(kind person/org)`, typed `party_address`, `party_identifier`, `party_bank_account`. Customer/vendor is a **role** in per-company `party_profile` (receivable/payable account, terms, pricelist, credit limit, fiscal position). | Odoo one table [V doc 11]; typed addresses avoid `_fields_sync` |
| Principal: user, portal, guest, API client, agent, system | `base`, `portal` | `principal(kind, party_id NULL, status)`, `credential`, `principal_company`, `group_membership`, `api_key`, `session`, `agent_delegation(agent, on_behalf_of, scopes, expires)`, `share_grant(token_hash, record_ref, scope, expires)` (capability tokens, not ids). Not a party subtype. | Odoo `res.users` `_inherits` partner [V]; Tryton splits them |
| Employee | `hr` | `employee(party_id NOT NULL, principal_id NULL, company_id)`, effective-dated `employee_version`, classified `employee_private`, `hr_employee_public` view | Tryton `Employee.party` required [V]; Odoo `hr.version` [V doc 13] |
| Company / legal entity | `base` | `company(party_id UNIQUE NOT NULL, currency, country, fiscal calendar, tax registrations)`, flat; `branch(company_id, address_id, ...)` separate; group ownership in `ownership_interest` (Part C) | Tryton `company.party` [V]; Odoo overloads one tree [V] |
| Currency, rates, address | `base` | `currency(code PK, exponent, rounding)`, `currency_rate(company_id NULL, code, rate_type, valid_on, rate)` with EXCLUDE on overlap; typed address rows referenced by `address_id` | Odoo keeps rates on the root company [V res_currency.py L186] |
| Product, UoM, packaging | `product` | `product_template`/`product_variant`, per-company `product_profile` (cost method, accounts, default taxes), `uom(relative_uom_id, relative_factor, rounding)`, `product_uom` (packaging as a per-product UoM with barcode) | Odoo 20 has no `product.packaging` model [V: `addons/product/models/` has `product_uom.py` and no `product_packaging.py`] |
| Attachments, chatter | `base`, `mail` | `attachment(record_ref, blob_id)` with content-addressed `blob(sha256, storage_key, state)` and pending-to-committed GC; lazily created `thread(record_id PK, model_id, company_id)` gives messages, followers, activities real FKs | doc 08; Odoo `mail.thread` is 5,446 lines [V doc 02] |
| Analytic | `analytic` | `analytic_plan`, `analytic_account`, `analytic_line`, `analytic_line_tag(line_id, plan_id, account_id)`, `analytic_distribution jsonb` on document lines; timesheets are analytic lines | Odoo adds one column per plan [V `analytic_line.py`]: avoid |
| Sequences, properties, external ids | `base` | `sequence(company_id, scope, pattern, next_value, gapless, reset)`, legal numbers assigned at post time by `UPDATE ... RETURNING`, PG sequences for internal refs; `properties jsonb` plus `property_definition` on the container; `external_id(module, name, record_ref, origin, module_version, content_hash)` replaces `noupdate` | docs 02, 08, 22 |
| Audit, outbox, translations | `base`, kernel | `audit_change` (field diffs, partitioned), append-only `security_event`, journal hash chain; `outbox_event(id, company_id, aggregate_type, aggregate_id, aggregate_version, type, schema_ver, payload, xid8, trace_id)`, `outbox_delivery`, `inbox_processed`; translations as `jsonb {lang: value}` per column plus PO import/export | docs 08, 18, 22; Odoo [V doc 02] |

Document headers and lines belong to each document module (B.2).

### B.2 Document header-line pattern and state machines

- **Header:** `company_id, branch_id?, number NULL until confirm/post, state (draft | confirmed | cancelled), locked, party_id, currency_code, date, origin_ref, rounding_profile, fiscal_position_id, payment_term_id, totals_*_minor, row_version`.
- **Line:** `doc_id, position, display_type (product | section | subsection | note), parent_line_id` (combo/optional lines as a tree in one table, doc 11), `product_id, qty, uom_id, price_unit, discount_pct, tax_ids`, minor-unit amounts, `analytic_distribution`, `source_line_ref`.
- **State machines are data**: `StateMachine{states, transitions[{from, to, command, guards, emits}]}` in the registry, run only by command handlers. Posted ledger documents get a DB trigger limiting transitions to `draft -> posted -> reversed`.
- **Two tiers:** a three-value kernel lifecycle (like Frappe `docstatus` 0/1/2 [V doc 08]) plus module **progress** fields (`invoice_status`, `delivery_status`, `payment_state`) stored-computed from link tables (`sale_line_qty_contribution`, `invoice_line_source`, `document_link`), never user-written. Posted or done documents are corrected only by reversal documents.

### B.3 Semantic and reporting layer

- The registry declares `Measure{model, expr, agg, unit: Money(currency_field) | Qty(uom_field) | Ratio | Count, additivity, filter: Domain}`, `Dimension{model, path, kind: categorical | time(grains) | hierarchy | geo}` and `Cube` views.
- Pivot/graph views, dashboards, financial reports (Odoo's declarative `account.report` engines over account tags are Community source [V doc 09]) and AI queries all submit one `SemanticQuery{measures, dimensions, filters, grain, target_currency, rate_type, company_scope}`, compiled through the Domain AST plus the caller's access domain (DR-4). Multi-currency aggregation needs an explicit target currency and rate type; fan-out joins pre-aggregate each fact. Doc 16 cites preprints that governed semantic layers avoid raw text-to-SQL's row-level-security leaks.
- Interchange: Apache Ossie (incubating, formerly Open Semantic Interchange; Apache-2.0; draft spec 0.2.0.dev0; github.com/apache/ossie) models datasets, relationships, fields and metrics; import/export is a COULD. Cube (v1.7.48) and dbt MetricFlow (Apache-2.0, v0.213.0) are non-Rust, so not runtime dependencies.

---

## Part C: group structures

### C.1 Comparison

| System | Entity model | Masters | Intercompany | Consolidation | VAT group |
|---|---|---|---|---|---|
| **Odoo 20** [V] | `res.company` tree (`parent_id`, `parent_path`, `root_id`); branches are child companies; docs say independent subsidiaries belong in separate companies | Accounts: `company_ids` many2many with per-root company-dependent code. Taxes: one `company_id`, `parent_of` check. Products: shared or company-restricted, company-dependent `standard_price`. Rates on root company. | Docs: auto-create bills, SOs, POs, sync stock moves, optional auto-validate. `inter_company_rules` is absent from the Community tree; `account_payment_interco` (LGPL-3) is present. | Account mapping by code, multi-ledger (adjustment journals excluded from statutory ledger), horizontal groups, CTA (equity historical, P&L weighted-average, balance sheet closing). No elimination or minority-interest text in `consolidation.rst`. | Tax units: same country and currency, representative company, own tax id for returns |
| **ERPNext v16** [V] (v16.37.0, 2026-09-30) | `Company` tree (`parent_company`, `is_group`) | `Account.company` required; `Item Default` rows per company | `is_internal_customer` + `represents_company`, `inter_company_invoice_reference`, `Allowed To Transact With` | Consolidated Financial Statement and Trial Balance reports, presentation currency | not checked |
| **Tryton 8** [V] (trytond 8.0.10, 2026-09-16; `main` is 8.1) | `company.company` flat, requires a `party`; employees require a party | Per company | no interco module in `modules/` | `account_consolidation` (GPL-3): `account.consolidation` groups account types across companies; `consolidation_company` on Move/Invoice tags the counterparty | not checked |
| **NetSuite OneWorld** [G] | Subsidiary tree (unverified); OneWorld is an add-on at extra cost (Wikipedia: "Multi-entity and global accounting and consolidation functionality is available at additional cost") | Shared records restricted per subsidiary (unverified) | Paired intercompany documents (unverified) | Elimination subsidiary, consolidated rates (unverified) | not verified |
| **Business Central** [V] | Company = separate ledger in an environment | Per company; IC partner codes, IC chart of accounts, IC dimensions | IC outbox/inbox: enter once, counterpart created, receiver may reject | Separate **consolidated company** as container; business units; per-account rates; full or percentage consolidation; eliminations are a manual process (journal lines plus the G/L Consolidation Eliminations report) | not read |
| **Dolibarr** [V] | `entity` integer column on core tables (`llx_societe.sql`); Multicompany is an external module sold on Dolistore (360 EUR excl. tax on 2026-10-01, dolistore.com/product.php?id=1619) | Row discriminator | n/a | n/a | n/a |

Sources: Odoo 20.0 `general/companies.rst` (branches; "Independent subsidiaries should be created as additional companies, not branches"; a parent cannot later become a branch), `multi_company.rst`, `consolidation.rst`, `tax_units.rst`, `account_account.py`; ERPNext `company.json` (branch `version-16`); Tryton `account_consolidation`; MicrosoftDocs `intercompany-manage.md`, `finance-consolidated-company-reporting.md`; wiki.dolibarr.org Module_MultiCompany.

### C.2 Recommended model

- **Business, company, branch** per DR-2. `company` is flat. Holding and group structure are **edges**: `ownership_interest(parent_company, child_company, pct_bp, method full/proportional/equity, valid_at range, EXCLUDE overlap)`. A holding is simply a company owning others.
- **Avoid Odoo's single tree for branch and subsidiary**; its docs show the strain (a parent cannot later become a branch [V]).
- **Fiscal unit** (VAT group): `fiscal_unit(country, representative_company, vat_id)` plus `fiscal_unit_member(company_id, valid_at)`. Returns are produced per unit; intra-unit supplies get an automatic out-of-scope fiscal position. EU VAT grouping is a Member State option after consulting the VAT Committee (Directive 2006/112/EC Art. 11 [V, legislation.gov.uk/eudr/2006/112/article/11, the UK-retained copy of the original text; EUR-Lex is the authoritative consolidated version]); Odoo's unit needs one country and currency [V]; treatment of branches is national [G] (unverified).

### C.3 Shared versus company-dependent masters

| Master | Scope and mechanism |
|---|---|
| Accounts, journals, taxes, fiscal positions, sequences, lock dates, fiscal calendar | **Per company**, instantiated from templates; composite FK `(company_id, id)`; `group_account_map` for consolidation; a branch uses its company's rows |
| Partners, products, pricelists, analytic plans | **Business-wide**, optional `company_id` restriction; company-dependent data in side tables (`party_profile`, `product_profile`), not jsonb maps |
| Warehouses, locations, employees | Company-owned; the inter-company transit location is business-level |
| Currencies | Business-wide catalog; rates business-wide with optional company override |
| Users, groups | Business-wide, granted per company via `principal_company` |
| Attachments, chatter | Inherit the record's company |

**Visibility.** `Env.companies` is an explicit set validated against grants, never ambient. Reads span it plus shared rows; writes need exactly one active company. Restriction: `company_id IS NULL OR company_id = ANY(env.companies)` (no tree, so no `parent_of`). Composite FKs guard company-owned masters; shared masters get a trigger plus an integrity job.

### C.4 Intercompany, eliminations, consolidation, franchise

- **Mirrored documents are durable events, not shared rows.** `InvoicePosted` in company A to a party that represents company B creates a draft bill in B, tracked in `interco_link(src_ref, mirror_ref, state, idempotency_key)`; orders, receipts and payment clearing follow the same pattern. It copies BC's inbox model with per-pair auto-validate and reject [V BC]. It is deliberately asynchronous: B's lock date must not block A's posting, and failures stay visible and retryable. Odoo's docs say counterparts are generated automatically; its module is Enterprise code I did not read.
- **Eliminations** live in a separate consolidation ledger. Posted lines carry `counterparty_company_id` (Tryton's tag idea [V]); an `elimination_run` matches `interco_link` pairs and posts only into a `ledger_kind = 'consolidation'` ledger (Odoo's multi-ledger idea [V]). Unmatched differences are reported, never auto-posted.
- **Translation and CTA:** balance sheet at closing rate, P&L at average or transaction rate, equity at historical rate (Odoo documents these classes [V]; IAS 21 paragraph rules and OCI treatment are [G] (unverified)). CTA goes to a dedicated equity account.
- **Minority interests:** consolidate 100% of controlled subsidiaries and show the NCI share of equity and profit, computed in the consolidation ledger from `ownership_interest` (IFRS 10 [G] (unverified)). BC offers percentage consolidation [V]; neither Odoo's docs nor ERPNext's report code mention NCI [V].
- **Franchise:** franchisor and franchisees are separate **businesses** joined by a "business link" (catalog syndication, royalty invoices over API or Peppol), not companies in one database, because ownership and confidentiality differ [I]. A single-controller group uses multi-company.

### C.5 MVP cut (doc 20: single company) that does not foreclose this

T0 ships one `company` row; `company_id NOT NULL` and composite FKs on every company-owned table; a one-element `Env.companies`; `principal_company`; the RLS backstop; a dormant `branch` table with nullable `branch_id`; nullable `currency_rate.company_id`; outbox and `external_id`; no selector UI. **Deferred:** `interco_link`, `ownership_interest`, `fiscal_unit`, `consolidation_*`. All later work is expand-only, so no key or FK is rewritten.

---

## Part D: integration contract

### D.1 What the comparators teach

| System | Mechanism | Lesson |
|---|---|---|
| Odoo 20 [V] | `_inherit` plus `auto_install` glue: 400 of the 641 addons with a manifest are auto-install (642 directories; `mrp_subcontracting_repair` has no `__manifest__.py`), 216 of 412 non-l10n manifests, at `20.0` tip `9df43e7` (2026-09-10). `sale_stock` depends on `stock_account`, so sale-to-stock drags in `account` (27 transitive deps); `account` has 156 direct and 398 transitive dependents, `mail` 592 transitive. | Glue works, but hubs become gravity wells and pair-wise bridges multiply |
| Tryton [V] | `depends` plus `extras_depend`, with an optional `[register account_invoice]` section in the same module | Optional integration without a bridge module |
| Frappe [V] | `doc_events` in each app's `hooks.py`: dotted-path handlers per doctype and event (`validate`, `on_submit`, `*`), synchronous, same transaction | Simple, but stringly typed and unordered across apps |
| Medusa 2.21.2 [V] | Isolated modules, **Module Links** (link tables of ids), workflows of steps with **compensation functions**, workflow hooks | Isolation plus sagas for cross-module and external work |
| Spring Modulith 2.1.1 [V] | Synchronous in-process events by default; `@ApplicationModuleListener` (async, transactional); an **event publication registry** written in the business transaction with a lifecycle (published, processing, completed, failed, resubmitted); `verify()` and `allowedDependencies` | Per-listener durable rows and a build-time graph check |
| ABP 10.6.1 [V] | Local bus handlers run **in the same unit of work** and roll it back on error; distributed events use **outbox plus inbox** (dedup) | Two tiers: atomic in-process, durable cross-process |
| Business Central AL [V] | Publisher/subscriber events; publishers do not know their subscribers | Extension without editing the owner |

### D.2 The contract

Mechanisms, in order of preference:
1. **Port call** (synchronous request/response): a trait in the owner's `*-api` crate, implemented by the owner, resolved from the registry as `Arc<dyn Port>`. Optional calls use `Option<Arc<dyn Port>>` and need **no bridge module**.
2. **Transactional event** (`TxEvent`, past tense): handlers run in the publisher's transaction, in deterministic order (module depth, then priority); an `Err` aborts the command. For atomic invariants.
3. **Extension point** (mutable pre-event such as `BeforeInvoicePost`): a port with a chain of implementers.
4. **Durable event**: `outbox_event` written in the transaction, one `outbox_delivery(event_id, subscriber, state, attempts, lease_until, next_run_at)` row per subscriber (Spring Modulith registry shape), claimed with `FOR UPDATE SKIP LOCKED` and a lease. External I/O lives only here.

**Rules.**
- **R1 Transaction boundary:** one command, one DB transaction, one `Tx` handle; only durable-event handlers open their own.
- **R2 Ordering:** per (subscriber, aggregate) only: a delivery is claimable when no earlier incomplete delivery exists for that pair, and events carry `aggregate_version` so consumers detect gaps. No cross-aggregate order.
- **R3 Idempotency:** at-least-once delivery, `inbox_processed(subscriber, event_id)` unique, natural-key handlers (`sale_line_qty_contribution` unique on `(line, source)`), and `Idempotency-Key` on API commands.
- **R4 Compensation:** inside a business, rollback; after commit, a reversal document (credit note, return) issued by an explicit command. Only cross-business or external flows use a `process_instance` saga with steps and compensations (Medusa model). No 2PC.
- **R5 Dependency law** (an `xtask` over `cargo metadata`): an impl crate may depend only on kernel crates and the `*-api` crates of its own module and its manifest `depends`. No impl-to-impl edges; api crates hold types, traits and events only, with no sqlx or logic. This is Spring Modulith's `verify()` applied to Cargo.
- **R6 Orphan rule** (an impl needs a local trait or type [V Rust reference]): handler traits are owned by `obop-events` and bridges implement them on their own unit structs; ports are implemented by their owner only. Async ports use boxed futures (`async-trait`/`dynosaur`) since native `dyn` async is unstable (doc 06; still feature-gated as `async_fn_in_dyn_trait`, tracking issue rust-lang/rust#133119 open, with Rust stable at 1.98.1 as of 2026-09-03).
- **R7 auto_install:** manifest `auto_install = ["sale", "stock"]` (Odoo's list form: all must be installed [V]). Install state is per business in `module_state`. Crates are always linked in the release binary (slim builds via Cargo features); handlers and migrations register only for installed modules when the per-business `Arc<Registry>` is built.
- **Hub ports keep bridges O(N), not O(N squared):** `DemandPort` (sale, pos, project, repair, mrp raise demand), `QtyContributionPort` (stock, timesheet, milestones feed delivered quantity), `PostingPort` (stock, payroll, POS request entries by **account role**, not account id) and `InvoiceableSource`. A bridge crate exists only to add storage or UI.

### D.3 Module dependency graph for T0/T1 (40 modules)

Kernel crates (not modules): `obop-types, -registry, -orm, -security, -events, -jobs, -i18n, -storage, -http`.

| Layer | T0 (21) | T1 (19) |
|---|---|---|
| Base | `base` | |
| Foundations | `mail`, `portal`, `analytic`, `product`, `automation` | |
| Engines | `pricing`, `tax`, `account`, `analytics` (semantic layer) | |
| Apps | `invoicing`, `payment`, `edi`, `l10n_<cc>` (data pack), `crm`, `sales`, `purchase`, `stock` | `project`, `timesheet`, `mrp`, `pos`, `subscriptions`, `hr`, `helpdesk`, `website`, `ecommerce`, `documents`, `intercompany`, `consolidation`, `ai` |
| Bridges | `sale_stock`, `purchase_stock`, `stock_account` | `sale_project`, `sale_timesheet`, `sale_mrp`, `mrp_account`, `pos_stock`, `pos_account` |

Edges (impl to api crates only; `?` = optional port):
```
mail, portal, analytic, product, automation -> base     pricing -> base, product
account -> base, analytic, product                      tax -> base, account-api, product
invoicing -> account, tax, product, pricing             payment -> account, invoicing
edi -> invoicing, tax     l10n_cc -> account, tax, edi  analytics -> base
crm -> base, mail, sale-api                             sales -> product, pricing, tax, invoicing
purchase -> product, invoicing                          stock -> base, product
bridges <a>_<b> -> a-api, b-api, for example:
  sale_stock -> sale-api, stock-api    stock_account -> stock-api, account-api (PostingPort)
  sale_timesheet -> sale-api, timesheet-api            timesheet -> analytic, hr?
```

### D.4 Worked quote-to-cash, with crate graph

```
 kernel:      obop-types  obop-events  obop-registry  obop-orm
 api crates:  base-api  product-api  pricing-api  sale-api  stock-api  account-api
 impl crates: base      product      pricing      sales     stock      account + invoicing
                                                    \         |          /
 bridges:                                    sale_stock   stock_account      (api crates only)
```
`sales` never references `stock` and vice versa: a stock move carries a generic `origin_ref` and `sale_stock` maps it to sale lines. `crm` subscribes to `sale-api` events directly, with no bridge.

| Step | Command (transaction) | In-transaction handlers (TxEvent) | Durable events |
|---|---|---|---|
| 1 Quote | `CreateQuote`: `PricingPort::price` (pure) | | |
| 2 Confirm | `ConfirmSaleOrder` (Tx-1): state to confirmed, internal number | `sale_stock` on `SaleOrderConfirmed` calls `DemandPort::raise`, so stock creates and reserves moves; `crm` marks the opportunity won | `SaleOrderConfirmed`: webhook, email, interco mirror, semantic cache, AI index |
| 3 Deliver | `ValidateTransfer` (Tx-2): quants updated by `INSERT ... ON CONFLICT`, lines done | `sale_stock` on `TransferDone` calls `QtyContributionPort::set(('transfer', id), line, qty)`; `stock_account` posts valuation entries via `PostingPort` (perpetual per transfer) or nothing until invoicing | `TransferDone` |
| 4 Invoice | `CreateInvoice` (Tx-3) from `InvoiceableSource`, linked in `invoice_line_source`; `PostInvoice` (Tx-4): lock dates, hash chain, legal number from the counter row late in the transaction, deferred balanced trigger | `BeforeInvoicePost`: `stock_account` adds COGS lines; `InvoicePosted`: `sales` updates `qty_invoiced` | `InvoicePosted`: e-invoice transmission (external I/O, retried, receipt stored), email, interco mirror |
| 5 Pay | `RegisterPayment` or bank-statement/PSP webhook | `PaymentReconciled` updates `payment_state` | `PaymentReconciled` |

A locked period makes the whole transaction roll back and the user see the error; that coupling is intended.

**Other flows, same pattern.** *Project to timesheet to invoice:* a timesheet is an `analytic_line` (as in Odoo [V doc 14]); `sale_timesheet` turns `TimesheetChanged` into `QtyContributionPort::set(source = timesheet line)` via its own `project_sale_line_map`, gated by an approved-only predicate; step 4 follows. *POS to stock to account:* `SyncPosOrder(command_id)` is idempotent; `ClosePosSession` (one transaction) emits `PosSessionClosed`; `pos_stock` raises one consolidated picking via `DemandPort`, `pos_account` posts the session entry via `PostingPort`; a locked period keeps the session open.

---

## Confidence & gaps

**High [V]:** the NOTIFY lock and COPY/RLS behaviour in source; PG 18.6 current and 19 at Beta 4; PG 18 availability on platforms and distros; Odoo 20's access algebra, company/consolidation docs and auto-install statistics; the ERPNext, Tryton, Dolibarr and BC facts cited; EN 16931 rounding rules; Spring Modulith, ABP and Medusa mechanics.

**Medium:** Supabase's 17-only state comes from repo config, not its hosted catalog. The Enterprise status of `inter_company_rules` and consolidation modules is inferred from absence in the Community tree. Auto-install counts are my own parse of 641 manifests, re-run independently and reproduced exactly (`mrp_subcontracting_repair` has no manifest, only i18n files).

**Gaps:**
- **The DR-1 spike was not executed** (no PostgreSQL runtime without downloads). Throughput ceilings, convoy effects and all spike thresholds are [I].
- NetSuite OneWorld, IAS 21/IFRS 10 paragraph rules and national VAT-group treatment are [G].
- UUID size overhead, the 250-database cell and the 10/s coalescing rate are design estimates.
- Recall.ai says core fixed the bottleneck, but today's source still has the lock; the likely explanation is the wake-up optimisation. Retest at PG 19 GA.
- The `uuid` crate's `v7` feature exists in 1.26.1 [V, re-checked]. Still open (unverified): NetSuite OneWorld details, IAS 21 and IFRS 10 paragraph rules, national VAT-group treatment of branches, and the internal doc 02-22 citations that this file relies on but did not re-derive.

## Implications for obop

**MUST**
1. Forbid `NOTIFY` in business transactions; build the outbox, per-subscriber deliveries and the coalescing notifier on a bus database (DR-1).
2. Make database-per-business the only GA tenancy mode, with `company_id` on every row and the DR-2 vocabulary.
3. Support PostgreSQL 17 and 18 (18 reference), gate 18-only features behind `requires_pg`, and run CI on 17, 18 and 19 RC (DR-3).
4. Implement Odoo 20's permission-OR/restriction-AND algebra with default-deny and mandatory scopes; enforce field-level security in the projection and query compiler; keep RLS a backstop in silo mode (DR-4).
5. Stage bulk imports through non-RLS tables; never run the runtime role as owner or BYPASSRLS.
6. Store ledger money as `bigint` minor units with a currency, use `NUMERIC` for prices, quantities and rates, and implement `per_line` and `per_group` rounding (DR-5).
7. Use application-generated UUIDv7 keys and `module.name` external ids, frozen only after the size spike (DR-6).
8. Keep the server PostgreSQL-only, with SQLite only as the fixed-schema edge store (DR-7).
9. Keep `company` flat, `branch` separate and group structure as `ownership_interest` edges; ship the T0 single-company cut with expand-only growth (Part C).
10. Enforce the dependency law (no impl-to-impl edges) in CI and ship ports, Tx events, extension points and durable events under R1-R7 (D.2).
11. Enforce balanced entries, posting immutability and lock dates with DB triggers, and assign gapless numbers at post time (docs 08, 09).

**SHOULD**
12. Build the hub ports (`DemandPort`, `QtyContributionPort`, `PostingPort`, `InvoiceableSource`) in T0 so T1 bridges stay thin.
13. Mirror intercompany documents as durable events with an inbox and reject step, with eliminations in a separate consolidation ledger (T2).
14. Route all analytics through the registry-declared semantic layer; give AI principals delegated scopes (doc 16).
15. Add a Medusa-style `process_instance` saga crate for cross-business and external flows (T1).
16. Run the three spikes (NOTIFY contention, 250-database cell, UUID size) in month one, before freezing DR-1, DR-2 and DR-6.

**COULD**
17. Export/import the semantic layer as Apache Ossie.
18. Add NATS JetStream for multi-node realtime fan-out.
19. Build pool-mode tenancy behind `TenantRouter`, and "business links" for franchise syndication.
20. Add Cedar for approval-style policy decisions.

## Verification log

Adversarial re-check on 2026-10-01 against primary sources (raw GitHub files fetched with `curl`, a sparse clone of `odoo/odoo` branch `20.0`, the crates.io API, postgresql.org and vendor docs).

**Confirmed as stated: 60 atomic claims.** All 11 author-flagged claims hold, apart from the source and count corrections below. Confirmed items:
- NOTIFY lock: `LockSharedObject(DatabaseRelationId, InvalidOid, 0, AccessExclusiveLock)` at L1309 on `master` and `REL_19_BETA4`, L921 on `REL_18_STABLE`; the source comments say "cluster-wide heavyweight lock" and "database 0"; `lmgr.c` builds the tag with `InvalidOid`; `GlobalChannelKey` holds `dboid`; the 2026-01-15 commit exists and the lock is still there; `synchronous_commit` is settable per transaction; Recall.ai outages 2025-03-19..22 and its "fixed in core" update.
- PostgreSQL: 18.6 current (2026-08-13); PG 19 Beta 4 (2026-09-24), no GA or RC yet; EOL dates for 16, 17, 18.
- Platforms: RDS 18 since 2025-11-14; Azure 18.6; Cloud SQL default 18; Neon 14-18; Supabase 15/17/orioledb-17; Debian 13 ships 17 (forky 18); Ubuntu 26.04 LTS (`resolute`) ships 18.
- `copy.c` L222-L233 (COPY FROM error only on `RLS_ENABLED`); `rls.c` BYPASSRLS and owner-without-FORCE exemptions; COPY TO applies SELECT policies; RLS quals run before user quals except leakproof; RI checks bypass RLS.
- Odoo 20: `_access_domain` is `Domain.OR(permissions) & Domain.AND(restrictions)` at `models.py` L3603; `DomainOr.ZERO = _FALSE_DOMAIN` at `domains.py` L740; `MIN_PG_VERSION = 16`; developer docs still describe `ir.model.access` plus `ir.rule`; `Monetary` is `numeric`; Product Price, Discount and Product Unit precision 2; `rate` has `digits=0`; rates keyed to `company.root_id` (`res_currency.py` L186); no `product.packaging` (packaging is `product.uom`); `mail_thread.py` is 5,446 lines; `hr_version.py` exists; analytic plan fields are one column per plan; POS `unique (uuid)`; `account_report.py` is in CE.
- PG 17 docs lack `WITHOUT OVERLAPS`, `VIRTUAL`, `PERIOD`, `uuidv7()`, `RETURNING OLD/NEW` and `NOT NULL ... NOT VALID`; `btree_gist` lists `uuid`; PG 17 `pg_basebackup` has incremental backup.
- Odoo 20 manifests: 400 auto_install; 216 non-l10n; `account` 156 direct and 398 transitive dependents; `sale_stock` 27 transitive dependencies; `mail` 592 transitive dependents; no `inter_company_rules`; `account_payment_interco` is LGPL-3.
- Odoo docs: interco auto-creation and stock sync; consolidation (account mapping, multi-ledger, horizontal groups, CTA rates; no elimination or minority-interest text); tax units (same country and currency, representative company, own tax id).
- Tryton: flat party-based `company.company`; `account_consolidation` GPL-3.0-or-later, `consolidation_company` on Move and Invoice, `depends`/`extras_depend`/`[register account_invoice]`; no interco module in `modules/`; `company.employee.party` required.
- ERPNext v16 fields (`parent_company`, `is_group`, `represents_company`, `Allowed To Transact With`, `inter_company_invoice_reference`, consolidated reports with presentation currency, no NCI terms); Dolibarr `entity` column and external Multicompany module; Business Central IC inbox/outbox, reject, consolidated company and business units, percentage consolidation.
- EN 16931: BR-CO-17 text; BR-DEC-23 caps BT-131 at 2 decimals; no cap on BT-146; BR-CO-10..16 exist.
- Versions and statuses: Spring Modulith 2.1.1 (publication registry lifecycle since 2.0, states as listed); ABP 10.6.1 (same-UoW local bus, outbox plus inbox); Medusa 2.21.2; Apache Ossie (incubating, Apache-2.0, 0.2.0.dev0, formerly OSI); Cube v1.7.48; MetricFlow v0.213.0 (Apache-2.0); `uuid` 1.26.1 (2026-09-10); `rusqlite` 0.40.2 (2026-08-08); SQLite 3.53.4 (2026-07-24); `rust_decimal` 96-bit mantissa with scale 0-28; CLDR 48 fraction-digit lists (0, 3, 4); RFC 9562 (May 2024); PG 18 `uuidv7()`; Turso pre-1.0; PowerSync service under FSL-1.1-ALv2; Zero has no offline writes; `opfs-sahpool` works without COOP/COEP; Cedar partial evaluation experimental (cedar-policy 4.13.0); Crunchy 2023 tenancy guidance; autovacuum launcher behaviour; Directive 2006/112/EC Art. 11 wording; Twenty schema-per-workspace (partial: schema manager present); Frappe `doc_events` with `*`.

**Corrections (was / now / source):**
1. Scope and D.1 counts: "all 642 addon manifests", "400 of 642 addon dirs", "216 of 413 non-l10n", "`mrp_subcontracting_repair` did not parse" / now 641 manifests in 642 directories; `mrp_subcontracting_repair` has no `__manifest__.py` (i18n only); 400 of 641 and 216 of 412 non-l10n manifests (the dependency figures were reproduced exactly) / sparse clone of `odoo/odoo` `20.0`, tip `9df43e7` (2026-09-10).
2. DR-2: "about 190k relations per cluster" / now "about 190k tables" (indexes and TOAST tables add many more `pg_class` rows) / arithmetic on 250 x 600-900 tables.
3. Azure source URL `.../flexible-server/concepts-supported-versions` / now canonical `.../configure-maintain/concepts-supported-versions`; Cloud SQL `cloud.google.com/sql/docs/db-versions` / now `docs.cloud.google.com/sql/docs/db-versions` (301 redirect) / the two pages themselves.
4. C.1 sources: "Independent subsidiaries should be separate companies, not branches" and "a parent cannot later become a branch" were attributed to `multi_company.rst` / now `general/companies.rst` (branches section) / https://github.com/odoo/documentation/blob/20.0/content/applications/general/companies.rst
5. C.2 Art. 11 source path on `legislation.gov.uk` (returns 404) / now `legislation.gov.uk/eudr/2006/112/article/11` (UK-retained copy; EUR-Lex is authoritative) / fetched 2026-10-01.
6. DR-5: HUF ISO 4217 exponent was [G] / now verified at 2 while CLDR shows 0 / https://en.wikipedia.org/wiki/ISO_4217 (secondary) and cldr-json 48.2.3 `currencyData.json`.
7. Gaps: "`uuid` crate v7 feature not re-checked [G]" / now verified, `v7 = ["rng"]` in `Cargo.toml` of 1.26.1 / https://github.com/uuid-rs/uuid/blob/main/Cargo.toml
8. DR-2: Crunchy tag "[S]" (undefined in the legend) / now "[V, secondary]" with its 50-tenant caution added; the 250-database cell is outside that guidance and depends on the spike / https://www.crunchydata.com/blog/designing-your-postgres-database-for-multi-tenancy
9. C.1 NetSuite row: unsupported details now explicitly "(unverified)"; the only verified line is Wikipedia's add-on-at-extra-cost statement / https://en.wikipedia.org/wiki/NetSuite

**Facts added:** PG 19 RC planned for early October 2026 with GA possibly in October (Beta 4 announcement); 18.5 was never released; the executable EN 16931 UBL Schematron accepts a BR-CO-17 VAT amount within 1.00 currency unit; Business Central eliminations are manual; pgvector 0.8.0 iterative scans; Dolistore Multicompany price 360 EUR excl. tax; Rust stable 1.98.1 with native `dyn` async still unstable (rust-lang/rust#133119); CLDR 49 is in beta; Turso latest tag v0.8.1; ERPNext v16.37.0 and trytond 8.0.10 version pins.

**Unresolved (left as unverified or [G]):**
- NetSuite OneWorld subsidiary, interco and consolidation behaviour (Oracle docs 404, netsuite.com 403).
- IAS 21 translation paragraph rules, OCI treatment and IFRS 10 NCI presentation (ifrs.org pages do not expose the text).
- National VAT-group treatment of branches.
- Supabase hosted catalog (only repo config and the upgrade guide were checked).
- Internal citations to docs 02, 06, 08, 09-14, 16, 18, 20 and 22 (for example "Odoo HR field groups do not propagate", the semantic-layer preprints in doc 16, Odoo POS IndexedDB mirroring in detail) were not re-derived.
- The DR-1 lock throughput, convoy effects, the 250-database cell, the 10/s coalescing rate, UUID overhead and all spike thresholds remain [I]; no PostgreSQL runtime was used.
- Whether `inter_company_rules` and the consolidation modules are Enterprise-only is inferred from their absence in the Community tree.
