# Data layer, realtime, offline sync, performance and capacity for obop (state of play on 2026-10-01)

## Scope & method

Primary sources only where possible: PostgreSQL 18/19 docs and source code (`async.c`, `lmgr.c`), Odoo 19 docs and Odoo source/tests/issues (read through the GitHub API), GitHub/crates.io/npm metadata for every project named, vendor docs for sync engines, the raw TechEmpower Round 23 result JSON, and Hetzner's price-adjustment page. Web search engines were unusable from this environment (captcha or 403 or 429 walls, which I did not bypass), so blog-level sentiment is thin and marked [S]. Tags: **[V]** verified against a primary source, **[S]** secondary, **[I]** my inference, **[U]** unverified or from memory. Sizing numbers in section 10 are design targets, not measurements. Cross-references: `06-rust-stack.md` (sqlx 0.9 + sea-query, in-house queue) and `08-metadata-platforms-dynamic-orm.md` (database per tenant, RLS as backstop).

## 0. Headline findings

1. PostgreSQL **18 is the current stable major** (18.0 released 2025-09-25; current minor **18.6**, released 2026-08-13; major supported to 2030-11-14); **19 is at Beta 4** (banner dated 2026-09-24) with no GA date yet; 14 reaches EOL on 2026-11-12 [V] ([versioning](https://www.postgresql.org/support/versioning/), [18.6 notes](https://www.postgresql.org/docs/18/release-18-6.html), [19 notes](https://www.postgresql.org/docs/devel/release-19.html)).
2. **NOTIFY commits take a cluster-wide exclusive lock**, in PG18 and in current master. The lock tag has database OID 0, so it serializes NOTIFY-ing commits across *all databases in the cluster*. That matters directly for a database-per-tenant design and for a `SKIP LOCKED + LISTEN/NOTIFY` queue (section 2.2) [V].
3. **`COPY FROM` is not supported on tables with row-level security**, so RLS as a tenant backstop conflicts with COPY-based bulk import unless staging tables or a bypass role are used [V] ([COPY docs](https://www.postgresql.org/docs/18/sql-copy.html)).
4. **Every mainstream sync engine's server is non-Rust** (Electric: Elixir+TS, and Electric joined Databricks/Neon on 2026-08-11; PowerSync: TypeScript, FSL licence; Zero: TypeScript, no offline writes). Odoo 19's offline bar was modest (read-only cache plus POS order capture), but **Odoo 20 (released September 2026) raised it: records can be created, edited, archived, unarchived and deleted offline, queued locally and synced on reconnect** (section 2) [V]. A Rust-native change-feed plus idempotent command queue is the right fit.
5. Odoo's documented capacity is **about 6 concurrent users per worker, workers = 2×CPU+1** [V]. Rust's app-tier CPU advantage is large (about 2.6x to 7x over the fastest Python entries and 16x to 34x over Django on TechEmpower's trivial DB-bound tests, section 3.2) but Postgres stays the same C engine, so realistic end-to-end gains are **3x to 10x per box**, not 50x [I].
6. **MinIO's community repo is archived** (no longer maintained; last release 2025-10-15). RustFS reached 1.0.0 on 2026-09-16 (Apache-2.0); Garage 2.4.1 (AGPL-3.0) and SeaweedFS 4.48 (Apache-2.0) are active [V].

## 1. PostgreSQL: version and feature fit

### 1.1 Version state [V]

| Item | Fact | Source |
|---|---|---|
| Current stable | 18.6 (released 2026-08-13); major 18 first released 2025-09-25; EOL 2030-11-14 | [versioning](https://www.postgresql.org/support/versioning/), [18.6 notes](https://www.postgresql.org/docs/18/release-18-6.html) |
| Still supported | 17.11 (EOL 2029-11-08), 16.15 (2028-11-09), 15.19 (2027-11-11), 14.24 (2026-11-12) | same |
| In development | 19 Beta 4; release notes "as of 2026-09-14"; release date still `2026-??-??` | [devel notes](https://www.postgresql.org/docs/devel/release-19.html) |

### 1.2 Features that matter for obop

| Feature | State (verified) | Use in obop |
|---|---|---|
| Async I/O | PG18: `io_method` = `worker` (default, `io_workers`=3) / `io_uring` / `sync`; release announcement claims "up to 3x" for reads in some scenarios; PG19 autoscales I/O workers ([AIO settings](https://www.postgresql.org/docs/18/runtime-config-resource.html), [announcement](https://www.postgresql.org/about/news/postgresql-18-released-3142/)) | Free win for report scans and vacuum. Leave default `worker` on small VPS; test `io_uring` only on known kernels |
| `uuidv7()` | Native in PG18, timestamp-ordered ([notes](https://www.postgresql.org/docs/18/release-18.html)) | Generate UUIDv7 **in the app** (works on any PG, allows client/offline-generated ids); use `uuidv7()` as column default for SQL-side inserts |
| Temporal constraints | `PRIMARY KEY (id, valid_at WITHOUT OVERLAPS)` uses GiST, needs `btree_gist` for scalar columns; `FOREIGN KEY (..., PERIOD valid_at)`; no `CASCADE`/`SET NULL` actions on temporal FKs ([CREATE TABLE](https://www.postgresql.org/docs/18/sql-createtable.html)). PG19 adds only docs and `range_minus_multi()`; **no `FOR PORTION OF`** | Price-list validity, tax rates, employee contracts, resource bookings, fiscal periods |
| Virtual generated columns | Default in PG18; STORED still available; logical replication can publish only **STORED** generated columns, opt-in via `publish_generated_columns = stored` or a column list ([docs](https://www.postgresql.org/docs/18/logical-replication-gencols.html)) | Derived display columns; use STORED where an index or replication is needed [I] |
| `RETURNING OLD/NEW` | PG18, incl. MERGE | Audit trails and stock-delta reporting in one statement |
| Skip scan | PG18 multicolumn btree | Helps `(tenant/company, date, ...)` indexes when leading column is unrestricted |
| Data checksums | On by default in PG18 `initdb` | Keep on |
| OAuth | New `oauth` auth method in `pg_hba.conf` | Not needed: the app authenticates users, PG sees service roles |
| pg_upgrade stats | Statistics now preserved across major upgrades | Shortens tenant-cluster upgrade windows |
| Logical replication | PG18: `streaming=parallel` default, conflict stats, idle slot timeout. PG19: **sequence replication**, `retain_dead_tuples` conflict detection, `EXCEPT` for `ALL TABLES` publications | Replica/analytics feeds, tenant moves, zero-downtime upgrades |
| PG19 extras | `REPACK CONCURRENTLY`, `WAIT` (standby read-your-writes), parallel autovacuum, SIMD `COPY FROM`, JIT off by default, `log_lock_waits` on, `max_locks_per_transaction` 64 to 128 ([19 notes](https://www.postgresql.org/docs/devel/release-19.html)) | Plan CI on 19 from RC; `WAIT` is handy for read replicas |

**Partitioning** [V] ([docs](https://www.postgresql.org/docs/18/ddl-partitioning.html)): planner copes with "up to a few thousand" partitions only if queries prune to a few; PK/unique must include the partition key; `DETACH PARTITION ... CONCURRENTLY` exists; per-customer LIST partitioning does not survive growth (use HASH). Use it for append-heavy tables only (audit log, bus/outbox, stock-move and journal-item history at large scale), automated with pg_partman 5.5.0 ([repo](https://github.com/pgpartman/pg_partman)).

**Advisory locks** [V] ([docs](https://www.postgresql.org/docs/18/explicit-locking.html)): session-level locks ignore transaction rollback; use `pg_advisory_xact_lock` in app code. The lock table is shared with regular locks (`max_locks_per_transaction`).

### 1.3 Row-level security [V]

- Policy expressions run before user quals except for `LEAKPROOF` functions; sub-SELECT policies are slower and can race under READ COMMITTED; referential-integrity checks bypass RLS; owners bypass unless `FORCE ROW LEVEL SECURITY` ([docs](https://www.postgresql.org/docs/18/ddl-rowsecurity.html)).
- Supabase's practice note: wrap session-variable lookups as `(select current_setting(...))` so the optimizer caches them as an InitPlan, and index every policy column [S] ([guide](https://supabase.com/docs/guides/database/postgres/row-level-security)). No measured numbers on that page.
- **COPY conflict:** "COPY FROM is not supported for tables with row-level security" [V]. Workaround [I]: COPY into an unlogged staging table without RLS, then `INSERT ... SELECT` under the tenant context; or run importers under a `BYPASSRLS` role that sets `company_id` explicitly.
- With doc 08's database-per-tenant model, RLS is only the `company_id` backstop. Set context with `SET LOCAL`/`set_config(..., true)` per transaction, which is safe under transaction pooling.

### 1.4 LISTEN/NOTIFY: documented limits plus source-level finding

Documented [V] ([NOTIFY](https://www.postgresql.org/docs/18/sql-notify.html)): payload under 8000 bytes; queue 8 GB; if full, NOTIFY-ing transactions fail at commit; a session with LISTEN that sits in a long transaction blocks queue cleanup; identical payloads in one transaction are deduplicated; a NOTIFY-ing transaction cannot be `PREPARE`d; delivery is in commit order.

Source-level [V]:
- `PreCommit_Notify` in both PG18 (`REL_18_STABLE`) and master calls `LockSharedObject(DatabaseRelationId, InvalidOid, 0, AccessExclusiveLock)`, held until after commit, to keep queue entries in commit order ([async.c](https://github.com/postgres/postgres/blob/master/src/backend/commands/async.c)).
- `LockSharedObject` builds its lock tag with `InvalidOid` as the database id, i.e. it is **shared across databases** ([lmgr.c](https://github.com/postgres/postgres/blob/master/src/backend/storage/lmgr/lmgr.c)). So all NOTIFY-ing commits in a cluster queue behind one lock, regardless of database [V code reading, I on consequence].
- Recall.ai took three periods of downtime between 2025-03-19 and 2025-03-22 from exactly this pattern with tens of thousands of concurrent writers ([post](https://www.recall.ai/blog/postgres-listen-notify-does-not-scale)) [S].
- PG19 improvements (shared channel map, direct advancement, "wake only backends listening to the specified notifications") reduce wakeup cost but the heavyweight commit lock is **still in master** [V].
- Listeners are per database (`GlobalChannelKey` includes `dboid`), so DB-per-tenant implies one LISTEN connection per tenant per app node unless a shared "bus" database is used [V code, I consequence].

Odoo's approach is the safe one [V] ([bus.py](https://github.com/odoo/odoo/blob/master/addons/bus/models/bus.py)): write a row to `bus_bus` in the business transaction; after commit, open a *separate* connection and `pg_notify('imbus', <JSON list of channels>)` (split recursively to stay under 8000 bytes); clients fetch rows newer than their last id, so a lost NOTIFY only delays delivery; GC after 24 h.

## 2. How Odoo does it (functional reference)

Baseline note [V]: most rows below were read from Odoo 19 docs/source. **Odoo 20.0 was released in September 2026** (branch `20.0`, `odoo/release.py` = 20.0.0 FINAL; [release notes](https://www.odoo.com/odoo-20-release-notes); see also docs 01 and 02), and `master` is now 20.1 alpha. The worker-sizing text in `deploy.rst` is byte-identical in the 19.0 and 20.0 docs branches; the offline-mode docs are not (see the Offline rows).

| Concern | Odoo behaviour | Source |
|---|---|---|
| Isolation | `REPEATABLE READ` for every cursor; "Odoo does its own locking for hot spots"; retry set = `LockNotAvailable`, `SerializationFailure`, `DeadlockDetected` (in 19.0 the tuple lives in `odoo/service/model.py` with up to 5 tries and exponential backoff; in master/20.0 it moved to `sql_db.py`); pool default 64 connections, idle timeout 600 s | [sql_db.py](https://github.com/odoo/odoo/blob/master/odoo/sql_db.py), [19.0 service/model.py](https://github.com/odoo/odoo/blob/19.0/odoo/service/model.py) [V] |
| DB driver | psycopg2; an RFC to move to psycopg3 was opened 2025-12-19 (no decision seen) | [#240721](https://github.com/odoo/odoo/issues/240721) [V] |
| Cron | `FOR NO KEY UPDATE SKIP LOCKED` per job, lock held until commit; `ir_cron_trigger` rows for immediate re-runs; `pg_notify('cron_trigger')` wakeups; disable after 5 consecutive failures and 7 days; each job slice runs at least 10 iterations and at least 120 s unless it completes or fails | [ir_cron.py](https://github.com/odoo/odoo/blob/master/odoo/addons/base/models/ir_cron.py) [V] |
| Standard sequences | native PG sequence, `nextval` (gaps possible) | [ir_sequence.py](https://github.com/odoo/odoo/blob/master/odoo/addons/base/models/ir_sequence.py) [V] |
| `no_gap` sequences | `SELECT number_next ... FOR UPDATE NOWAIT` then `UPDATE`; concurrent users fail fast and retry | same [V] |
| Accounting numbers | `sequence.mixin`: `UPDATE` of a row covered by a unique index to lock the b-tree entry, savepoint plus retry on unique violation, transactional cache so later numbers in the same transaction skip savepoints | [sequence_mixin.py](https://github.com/odoo/odoo/blob/master/addons/account/models/sequence_mixin.py) [V] |
| Reports | `_auto = False` models backed by SQL views (e.g. `sale.report`); computed at query time | [sale_report.py](https://github.com/odoo/odoo/blob/master/addons/sale/report/sale_report.py) [V], "plain view, not materialized" [I] |
| Offline (generic, Odoo 19) | Previously opened views/records readable but **not editable** offline; max 2 GB cache (oldest data evicted; a hard refresh wipes it) | [offline_mode.rst (19.0)](https://github.com/odoo/documentation/blob/19.0/content/applications/general/offline_mode.rst) [V] |
| Offline (generic, Odoo 20) | Records can be **created** (only if a record of that type was created while online), **edited, deleted, archived, unarchived** (if previously opened); changes sit in a local queue, are listed under a "Working offline" button, can be discarded, and sync automatically on reconnect; previously run searches can be repeated; attachments cannot be uploaded or deleted offline; still 2 GB cap; docs say it is for short-term interruptions, not full or long-term offline; a per-view "Data caching" switch disables it | [offline_mode.rst (20.0)](https://github.com/odoo/documentation/blob/20.0/content/applications/general/offline_mode.rst), [20 release notes](https://www.odoo.com/odoo-20-release-notes) [V] |
| Offline (POS) | Products/customers cached in browser; orders created offline stored in browser cache and synced on reconnect; invoices requested offline are generated after reconnect; payment terminals, customer registration, live stock sync unavailable offline | [POS use.rst](https://github.com/odoo/documentation/blob/19.0/content/applications/sales/point_of_sale/use.rst) [V] |
| Multi-tenancy | One process can serve many databases; `--db-filter` by hostname | [deploy.rst](https://raw.githubusercontent.com/odoo/documentation/19.0/content/administration/on_premise/deploy.rst) [V] |
| Realtime | `bus.bus` table + post-commit NOTIFY, WebSocket served by a dedicated gevent worker (`--gevent-port`, path `/websocket/`) | bus.py, deploy.rst [V] |

## 3. Odoo performance limits and realistic Rust gains

### 3.1 Hard data on Odoo

| Datum | Value | Source |
|---|---|---|
| Worker rule | workers = (#CPU × 2) + 1; **1 worker ≈ 6 concurrent users**; cron workers consume CPU | [deploy.rst](https://raw.githubusercontent.com/odoo/documentation/19.0/content/administration/on_premise/deploy.rst) [V] |
| Memory rule | heavy worker ≈ 1 GB, light ≈ 150 MB, 20 % heavy mix; vendor example: 9 workers ≈ 3 GB; sample limits `limit_memory_soft` 629,145,600 and `limit_memory_hard` 1,677,721,600 bytes, `limit_time_cpu` 600 s, `limit_time_real` 1200 s | same [V] |
| Derived | 2 vCPU to 5 workers to about 30 concurrent users; 4 vCPU to 9 workers to about 54; average worker RAM = 0.8×150 + 0.2×1024 ≈ 325 MB | [I] from the formulas above |
| Queries per operation | Odoo's own tests pin **34 SQL queries to create one empty `sale.order`**, 39 for two empty orders, 44 for two orders with note/section lines, and 53 for two orders with one taxless product line each. A variant with taxed product lines (70) and a randomized-pricing batch test (1,593 queries locally/nightly, "1954 sometimes") exist but are **disabled** in the file ("not deterministic", privatized with a `__` prefix) | [test_create_perf.py](https://github.com/odoo/odoo/blob/master/addons/sale_stock/tests/test_create_perf.py) [V] (identical counts in the 19.0 branch) |
| Odoo's own guidance | Profiler collectors, batch creates, prefetch, indexes; warns N+1 and O(n²) patterns are the usual pitfalls | [performance.rst](https://raw.githubusercontent.com/odoo/documentation/19.0/content/developer/reference/backend/performance.rst) [V] |
| Field reports (dated, user-submitted) | Partner complaint 2018: orders with 5,000 lines and receipts with 20,000 serial numbers; 2 s per `action_done` of one serial product, "total time 3 hours" ([#25967](https://github.com/odoo/odoo/issues/25967)). Validating a 5,000-line inventory adjustment took ~6 minutes on 2 CPU/8 GB, Odoo 13, 2020 ([#43153](https://github.com/odoo/odoo/issues/43153)) | [S], old versions |
| Concurrency bugs | `stock.quant` corruption under concurrent transactions ([#55494](https://github.com/odoo/odoo/issues/55494), 13.0, 2020, closed); duplicate invoice/payment under concurrent update ([#90465](https://github.com/odoo/odoo/issues/90465), >=14.0, 2022, closed) | [V] issues exist, [S] severity |

**Gap:** I found **no public, reproducible Odoo load-test report** (only Locust helper repos such as `nseinlet/OdooLocust`, 80 stars, no published results). The vendor's worker formula is the only authoritative throughput figure. obop should build its own Odoo-vs-obop harness on identical flows (quote to order to delivery to invoice to payment) before claiming any multiple [I].

### 3.2 What Rust changes and what it does not

TechEmpower "Round 23" raw data (file labelled "Continuous Benchmarking Run 2025-01-30", run 2025-01-30 to 2025-02-06 per its start/completion timestamps, environment "Citrine", 15 s per level), requests/s derived as `totalRequests/15` at the best concurrency level [V data, I derivation] ([ph.json](https://www.techempower.com/benchmarks/results/round23/ph.json)). Postgres-backed entries only (Django = `django-postgresql`, Rails = `rails`); re-derived 2026-10-01:

| Test | axum-pg | Fastest Python entry | FastAPI | Rails | Django | axum/Django |
|---|---|---|---|---|---|---|
| Fortunes (DB read + template) | ~1,114k | ~174k (fastwsgi-asgi; granian-rsgi ~164k) | ~109k | ~43k | ~33k | ~34x |
| Single query (`db` test) | ~1,270k | ~256k (uvicorn) | ~164k | ~63k | ~44k | ~29x |
| Multiple-queries test at 1 query/request (best interval) | ~1,243k | ~173k (uvicorn) | ~117k | ~57k | ~39k | ~32x |
| Updates at 1 update/request (best interval) | ~421k | ~119k (blacksheep) | ~96k | ~37k | ~27k | ~16x |
| Updates at 20 updates/request (canonical TechEmpower setting) | ~56k | ~22k (granian-rsgi) | ~13k | ~8.5k | ~2.1k | ~27x |

Caveats: trivial handlers, "Citrine" hardware (TechEmpower describes it as a 28-core-class machine, not re-verified here (unverified)), DB on a separate box, hand-tuned entries. The two "best interval" rows take the maximum over the 1/5/10/15/20-query intervals, which is the 1-query point. The TechEmpower repo's last push is 2026-03-24 and the blog's posts through 2026-09-11 announce no Round 24 (a 2026-03-16 post still refers to Round 23), so Round 23 is the latest public data [V]. Interpretation [I]:

- **App-tier CPU per request**: about 2.6x to 7x lower than the fastest Python entries and 16x to 34x lower than Django. Odoo's ORM is heavier than a Fortunes handler, so the gap is probably larger.
- **SQL count**: Odoo issues dozens of statements per simple business operation and over a thousand for pricing-heavy batches (section 3.1; the largest pinned test is disabled). A registry-driven Rust layer that compiles record rules into predicates and uses set-based writes should issue 3 to 10. That shortens latency independent of language.
- **Postgres is unchanged**: once app CPU falls 10x, the database becomes the ceiling. Expect per-box gains to saturate around **3x to 10x** concurrent users per vCPU. Do not publish higher claims until measured.
- **Memory**: Odoo needs about 325 MB per worker on average; one Rust process with shared registry/caches should serve the same users in a few hundred MB [I].

## 4. Postgres extensions and search

| Extension | State | Fit |
|---|---|---|
| `pg_trgm`, `ltree`, `btree_gist`, `uuid`, FTS | Built-in contrib / core. PG18: FTS and pg_trgm indexes need REINDEX after upgrade on non-libc collation clusters ([notes](https://www.postgresql.org/docs/18/release-18.html)). FTS limits: tsvector under 1 MB, no BM25 ([limits](https://www.postgresql.org/docs/18/textsearch-limitations.html)) | MUST baseline search: FTS + trigram for names/SKUs; `ltree` for account/category/location trees |
| PostGIS | GPL-2.0 ([repo](https://github.com/postgis/postgis)); newest source tarball on [download.osgeo.org](https://download.osgeo.org/postgis/source/) is 3.6.4 (release date not checked) [V] | Only for field-service routing/territories; optional module |
| pgvector | 0.8.6 (2026-07-29); HNSW/IVFFlat; `vector` indexable to 2,000 dims, `halfvec` 4,000; iterative scans since 0.8.0 fix post-filter recall ([repo](https://github.com/pgvector/pgvector)) | AI/semantic search; DB-per-tenant removes the tenant-filter recall problem |
| pg_textsearch | Timescale, 1.4.0 (2026-08-18), PostgreSQL licence, BM25, PG17 and 18 ([repo](https://github.com/timescale/pg_textsearch)) | COULD for ranked search without a separate engine |
| ParadeDB `pg_search` | 0.25.11, **AGPL-3.0** ([repo](https://github.com/paradedb/paradedb)) | Avoid as a dependency; licence friction |
| Tantivy / Meilisearch | tantivy crate 0.26.2 (MIT; latest GitHub release tag is 0.26.1); Meilisearch 1.54.3 (2026-10-01; MIT for most of the tree, Business Source License 1.1 variant for the Enterprise Edition parts, per its [LICENSE](https://github.com/meilisearch/meilisearch/blob/main/LICENSE)) | Embedded Rust engine if PG search stops being enough (doc 06 has the `SearchBackend` seam) |
| `pg_cron` | 1.6.8 (2026-09-08); extension installs into **one** database per cluster (`cron.database_name`); jobs can target other databases via `cron.schedule_in_database()` ([repo](https://github.com/citusdata/pg_cron)) | Awkward for DB-per-tenant (job metadata for every tenant lives in one database); schedule in the app |
| `pgmq` | 1.13.0 (2026-09-07), PostgreSQL licence, PG14-18, **SQL-only install works without superuser extension**, Rust client crate 0.33.7 ([repo](https://github.com/pgmq/pgmq)) | Reasonable, but obop still needs transactional enqueue with domain context, so in-house queue per doc 06 |
| `pg_ivm` | 1.16 (2026-09-30), PG13-18; IMMVs via `create_immv`; count/sum/avg/min/max, inner and simple outer joins, simple subqueries and CTEs; not supported on logical-replication subscribers; `create`/`refresh` take `AccessExclusiveLock` ([repo](https://github.com/sraoss/pg_ivm)) | COULD for simple always-fresh aggregates; extension availability on managed PG is a risk |
| `pg_duckdb` | 1.1.1 (2025-12-18), MIT; runs analytics through DuckDB inside PG (`duckdb.force_execution`); Parquet/Iceberg/Delta on S3 ([repo](https://github.com/duckdb/pg_duckdb)). DuckDB itself 1.5.6 (2026-09-28) | COULD on an analytics replica; not on the OLTP primary |
| `pg_mooncake` | last release 0.1.2 (2025-02-12) | Skip (stale) |
| `pgrx` | 0.19.3 (Rust-in-Postgres framework) | COULD, only for hot SQL functions; it ties deployment to extension-capable Postgres |

**Reporting on large data** [I, grounded in the above]: (1) keep ledger-style totals as *rollup tables updated in the same transaction* (`account_balance(company, account, period)` with `INSERT ... ON CONFLICT DO UPDATE SET amount = amount + EXCLUDED.amount`): deterministic, no refresh lag; (2) use plain views plus right indexes for operational reports (what Odoo does); (3) `REFRESH MATERIALIZED VIEW CONCURRENTLY` on schedule for heavy pivots (needs a unique index); (4) pg_ivm for a few simple aggregates; (5) a logical-replication read replica, optionally with DuckDB/pg_duckdb, for free-form analytics. Avoid Citus columnar and Hydra (Hydra's last release 2024-04-01).

## 5. Connection pooling and the Rust database stack

| Pooler | State | Notes |
|---|---|---|
| **PgBouncer** | 1.26.0 (2026-09-23) fixes 3 CVEs (unauthenticated crash via SCRAM nonce; infinite loop on packet growth; unbounded SCRAM iterations), tracks `search_path` on PG18+ and `default_transaction_read_only`, adds `pool_idle_timeout` (disabled by default), per-user/per-database `query_wait_timeout`; **single-threaded**; protocol-level prepared statements via `max_prepared_statements` since 1.21.0 (2023-10-16); old `-R` online restart removed in favour of `so_reuseport` rolling restarts ([NEWS](https://github.com/pgbouncer/pgbouncer/blob/master/NEWS.md)) | Default choice. Transaction mode forbids `SET`/`RESET`, `LISTEN`, `WITH HOLD` cursors, session advisory locks, SQL `PREPARE` ([features](https://www.pgbouncer.org/features.html)) |
| PgCat | 0.2.5 (2024-11-11), last push 2025-02-27, MIT ([repo](https://github.com/postgresml/pgcat)) | Appears dormant [I] |
| Supavisor | 2.9.13 (2026-09-10), Apache-2.0 | Multi-tenant pooler built for Supabase scale; likely heavier to run than PgBouncer [I] |
| PgDog | 0.1.60 (2026-09-24), Rust, **AGPL-3.0**, pooling + load balancing + sharding, with an Enterprise edition | Watch; AGPL and pre-0.2 make it a COULD |

**Rust clients** [V] (crates.io): sqlx 0.9.0 (changelog dated 2026-05-06; MSRV 1.94; repo moving to the `transact-rs` org; new `sqlx.toml` with multi-tenant migration-table renaming) ([changelog](https://github.com/launchbadge/sqlx/blob/main/CHANGELOG.md)); tokio-postgres 0.7.18 (has `binary_copy`); deadpool-postgres 0.14.2; bb8 0.9.1; diesel 2.3.13 / diesel-async 0.9.2; sea-orm 2.0.4; sea-query 1.0.2; tokio 1.53.1; axum 0.8.9; Rust 1.98.1 is the newest tagged stable (2026-09-03; see doc 06 for 1.99). Keep **direct, unpooled connections** for migrations, LISTEN, session advisory locks and replication, and route everything else through the pooler.

Embedded Postgres for demos or solo installs: `postgresql_embedded` 0.21.0 (Apache-2.0, 2026-07-19) downloads or bundles PostgreSQL binaries and runs them as a child process ([repo](https://github.com/theseus-rs/postgresql-embedded)). This makes "single binary" literal without a second database engine.

## 6. Concurrency control, sequences, jobs, events

**Isolation choice** [V facts, I recommendation]. Postgres SERIALIZABLE (SSI) adds no blocking but can fail any transaction with SQLSTATE 40001 and requires whole-transaction retry; it recommends `READ ONLY`, small transactions, fewer connections, and warns sequential scans take relation-level predicate locks ([docs](https://www.postgresql.org/docs/18/transaction-iso.html)). Odoo chose REPEATABLE READ plus hot-spot locks. For obop:

| Situation | Pattern |
|---|---|
| Default write command | READ COMMITTED with **atomic single-statement updates** and explicit row locks on the few hot rows; retry only on 40001/40P01/55P03 (the same three classes Odoo retries) |
| Stock quantities | `INSERT ... ON CONFLICT (product, location, lot) DO UPDATE SET qty = quant.qty + EXCLUDED.qty`; reservation as `UPDATE ... SET reserved = reserved + $n WHERE qty - reserved >= $n RETURNING`; append-only move ledger as source of truth; reconcile quants from the ledger |
| Multi-row invariants (period close, allocation across lots, journal balance) | `SERIALIZABLE` flagged per command in the registry, with a bounded retry wrapper (3 to 5 attempts, jittered) |
| Consistent exports/reports | `SERIALIZABLE READ ONLY DEFERRABLE` or `REPEATABLE READ READ ONLY` |
| User edits | Optimistic `version` column (`WHERE id=$1 AND version=$2`) surfaced as a conflict in the UI |
| Idempotency | `Idempotency-Key` table with unique `(tenant, key)`; same key returns the stored result |

**Gapless sequences.** PG sequences are not rolled back and cannot be gapless; gaps also arise from `ON CONFLICT` ([docs](https://www.postgresql.org/docs/18/functions-sequence.html)) [V]. Use a counter row per `(journal, fiscal-year/prefix)`: `UPDATE doc_counter SET next = next + 1 WHERE id = $1 RETURNING next - 1` inside the *same transaction that posts the document*, taken as late as possible before COMMIT, with `lock_timeout` instead of Odoo's `NOWAIT` so callers queue instead of retry [I]. Throughput per sequence is bounded by commit latency (roughly hundreds of postings/s on NVMe) [I, unmeasured]. Use plain PG sequences for internal ids and non-legal references. Assign legal invoice numbers at post/sync time, as Odoo does for POS invoices requested offline [V].

**Exactly-once jobs.** End-to-end exactly-once does not exist; achieve *effectively-once* = at-least-once delivery + idempotent handlers [I]. Facts: logical-decoding slots can re-send recent changes after a crash and consumers must deduplicate ([docs](https://www.postgresql.org/docs/18/logicaldecoding-explanation.html)) [V]; pgmq's "exactly once" is scoped to a visibility timeout (a lease) [V]. Design [I]:
- `job(id uuidv7, queue, run_at, lease_until, attempts, idempotency_key, payload)` with a unique `(queue, idempotency_key)`; claim with `FOR UPDATE SKIP LOCKED`, lease, heartbeat, dead-letter after N attempts; enqueue inside the business transaction.
- Leader-less scheduling: each node inserts `schedule_fire(job_key, fire_time)` with `ON CONFLICT DO NOTHING`; exactly one insert wins per tick, so no leader election and no `pg_cron`.
- Measured headroom: Graphile Worker reports 15.6k jobs/s unbatched and 184k/s batched on a local i9-14900K, with under 5 ms average latency ([perf page](https://worker.graphile.org/docs/performance)) [S, vendor]; Rust crates: `graphile_worker` 0.13.6, `apalis` 1.0.0-rc.10 (0.7.4 stable), `sqlxmq` 0.6.0 (last update 2025-05-25). An in-house queue (doc 06) is not a throughput risk.
- Wake-ups: do not NOTIFY from the job-enqueuing business transaction (section 1.4). Use an in-process `tokio::sync::Notify` for same-node, poll with backoff (250 ms to 2 s), and optional post-commit NOTIFY from a separate short transaction.

**Outbox and streaming.** Baseline: `event_outbox` table written in the business transaction; a dispatcher claims batches with SKIP LOCKED and publishes in-process, to WebSocket fan-out, and to webhooks. Logical decoding is the upgrade path when a downstream bus is needed, but slots retain WAL indefinitely if the consumer stalls, so set `max_slot_wal_keep_size` and alert on lag [V docs]. Scale-out bus options:

| Bus | State | Licence | Verdict |
|---|---|---|---|
| NATS JetStream | server 2.15.0 (2026-09-17); `async-nats` 0.50.0 | Apache-2.0 | Best optional bus for cell-to-cell events and WS fan-out |
| Kafka | Apache Kafka active | Apache-2.0 | Only for customer-side integration or large analytics |
| Redpanda | v26.2.2 (2026-08-22) | BSL + Redpanda Community Licence files in repo | Avoid as a default dependency |

## 7. Realtime push and local-first sync

### 7.1 Realtime transports

| Transport | Status | Verdict |
|---|---|---|
| WebSocket | Universal; Odoo uses it (dedicated gevent worker) | **Default** (axum built-in) |
| SSE | Universal (`EventSource`); one-way, HTTP/2 friendly | Fallback for simple notification feeds |
| WebTransport | caniuse data: Chrome 97, Edge 98, Firefox 114, **Safari 26.4** / iOS 26.4 now supported ([caniuse data](https://github.com/Fyrd/caniuse)) [V] | Needs HTTP/3 and UDP end to end; COULD after launch |

Protocol [I]: a single multiplexed WebSocket per browser tab, carrying subscriptions by `(model, id|domain-hash)` plus a `last_event_id` resume cursor (as in Odoo's `bus` ids), so a lost connection replays from the outbox rather than relying on NOTIFY.

### 7.2 Sync engines and fit

| Engine | Server language / licence | Status (2026-10-01) | Write path | Offline writes | Fit |
|---|---|---|---|---|---|
| ElectricSQL | Elixir sync service + TS clients, Apache-2.0 | 1.0 announced 2025-03-17 ([post](https://electric.ax/blog/2025/03/17/electricsql-1.0-released)); sync-service 1.8.1 (2026-09-07); docs moved to electric.ax; "read-path sync engine" ([intro](https://electric.ax/docs/intro)); **Electric announced on 2026-08-11 that it is joining Databricks (via Neon)**, saying what it has open-sourced stays open source ([post](https://electric.ax/blog/2026/08/11/electric-joining-databricks)), so roadmap and governance risk is higher than a year ago [V] | Your own API; four documented patterns ([writes](https://electric.ax/docs/guides/writes)) | App-built (patterns 2 to 4) | Read-side model (shapes over a commit-ordered stream) is a good template; the Elixir server is not Rust |
| PowerSync | TypeScript service, **FSL-1.1-ALv2** ([LICENSE](https://github.com/powersync-ja/powersync-service/blob/main/LICENSE)); SDKs for JS, RN, Flutter, Kotlin, Swift, .NET (beta), **Rust (beta)**, Tauri (alpha) | service v1.27.0 (git tag; `@powersync/service-image` published to npm 2026-09-30; last GitHub Release page is v1.26.1, 2026-09-11); Cloud Free/Pro from $49/Team from $599; self-host "Open Edition" is source-available ([pricing](https://www.powersync.com/pricing), [overview](https://docs.powersync.com/intro/powersync-overview)) | Local SQLite with upload queue to your backend | Yes | Best off-the-shelf mobile offline, but Node server plus FSL conflicts with "Rust-only backend" |
| Zero (Rocicorp) | TypeScript `zero-cache`, Apache-2.0 | 1.0 GA March 2026; npm latest 1.9.0; recommended for datasets under ~100 GB; **"Zero doesn't support offline writes"** ([status](https://zero.rocicorp.dev/docs/status), [when-to-use](https://zero.rocicorp.dev/docs/when-to-use)) | Custom mutators | No | Good for connected back-office UIs, wrong for POS/field |
| Automerge | Rust core, MIT | js 3.5.0 (2026-09-16), crate 0.12.0 | Library | Yes | Documents/notes only |
| Loro | Rust, MIT | 1.16.4 (2026-09-30) | Library | Yes | Collaborative rich text/trees, COULD |
| cr-sqlite | C/Rust SQLite extension, MIT | last release v0.16.3 (2024-01-17); only build fixes since | Library | Yes | Not recommended (stalled) |
| Turso Database | Rust SQLite rewrite, MIT | v0.8.1 (2026-09-29); README: "we have not yet reached 1.0"; sync offered via Turso Cloud; Postgres dialect experimental ([repo](https://github.com/tursodatabase/turso)) | Push/pull | Yes | Watch; not a dependency |
| PGlite | WASM Postgres, Apache-2.0 | 0.5.8 (2026-08-26) | with Electric | Yes | Possible in-browser store for Vite+ client |

**Benchmarks for scale reasoning**: Electric reports a single server behind a CDN at 100k to 1M concurrent users with a 960 tx/min write load, and 4,000 to 6,000 row changes/s for shape-affecting writes; every chart was an image and hardware is unstated [V page, weak evidence] ([benchmarks](https://electric.ax/docs/reference/benchmarks)).

**Fit for obop's three offline targets** [I]:
- **POS**: Odoo's own bar is "orders captured offline, synced later, invoices generated after reconnect" [V]. Model orders as **immutable, client-UUIDv7-identified commands** with a per-device idempotency key. The server replays them through the normal domain layer and returns per-command accept/reject. No CRDT is needed; conflicts are business rules (stock, price changes, closed session).
- **Mobile sales/warehouse**: same command queue plus a scoped change-feed (assigned pickings, customers in route, price lists). Local store is SQLite (native) or OPFS SQLite WASM (`@sqlite.org/sqlite-wasm` 3.53.4) in the browser.
- **Field service**: command queue plus **attachments uploaded via resumable object-store presigned URLs**; conflicts resolved per field by server policy (last-writer on technician fields, server-wins on planning fields).

**Change-feed design for a Rust server** [I]. `bigserial` ids are not commit-ordered, so a client cursor on `seq` can skip rows from slow transactions. Two safe options: (a) stamp rows with `pg_current_xact_id()` and serve only rows whose xid is below `pg_snapshot_xmin(pg_current_snapshot())`; (b) consume a logical-decoding slot (what Electric does) into a per-tenant log. Start with (a): no slot to babysit, and it works through PgBouncer.

## 8. Cache, object storage, analytics, alternatives

**Cache.** In-process `moka` 0.12.16 for registry, permissions, hot reference data, with invalidation by `registry_version` and bus events; no external cache in the baseline. If one is needed: Valkey 9.1.2 (BSD-3-Clause) is the clean choice; Redis 8.10.2 is now tri-licensed RSALv2/SSPLv1/AGPLv3; Dragonfly 2.0.0 is BSL 1.1 (change date 2030-11-01, use grant excludes competing services) [V] ([Valkey](https://github.com/valkey-io/valkey), [Redis LICENSE](https://github.com/redis/redis/blob/unstable/LICENSE.txt), [Dragonfly LICENSE](https://github.com/dragonflydb/dragonfly/blob/main/LICENSE.md)).

**Object storage.**

| Store | State | Licence | Note |
|---|---|---|---|
| Local filesystem | n/a | n/a | Baseline for single-binary installs |
| Garage | 2.4.1 (2026-09-08) | AGPL-3.0 | Replication to 3 zones, single binary, 1 GB RAM minimum; no erasure coding mentioned ([site](https://garagehq.deuxfleurs.fr/)) |
| SeaweedFS | 4.48 (2026-09-28) | Apache-2.0 | Mature, feature-rich |
| RustFS | **1.0.0 (2026-09-16)**; `1.0.1-preview.*` pre-releases followed daily through 2026-09-30 | Apache-2.0 | Several features labelled "Preview" (S3 Tables, MinIO on-disk compat, gated behind the `rio-v2` feature); new 1.0, treat as young ([repo](https://github.com/rustfs/rustfs)) |
| MinIO | **Repo archived**, "no longer maintained"; community edition source-only; last release 2025-10-15 | AGPL-3.0 | Do not adopt ([repo](https://github.com/minio/minio)) |

Access through the `object_store` 0.14.2 or `opendal` 0.59.3 crates behind one trait, so local FS, AWS S3, Garage, SeaweedFS and RustFS are configuration choices. Content-address blobs by SHA-256 (doc 08) and keep metadata in Postgres.

**Alternatives.**
- **SQLite/libSQL/Turso** for single-user/edge: libSQL's last server release was 2025-02-14 and Turso's own README says the Rust rewrite replaces it as the intended direction; Turso is pre-1.0. Postgres features obop relies on (RLS, ranges, GiST exclusion, JSONB, LISTEN, SKIP LOCKED) do not exist there. Do **not** dual-target SQL dialects; use embedded Postgres for "single-user" installs [I].
- **Distributed SQL**: CockroachDB is under the proprietary "CockroachDB Software License" (v26.2.7); YugabyteDB core is Apache-2.0 (v2026.1.2.0) but has a Postgres fork with feature gaps; Citus 14.2.0 is AGPL-3.0; Neon is Apache-2.0; Supabase's Multigres ("Vitess for Postgres") is v0.1.0, early stage [V] ([Cockroach LICENSE](https://github.com/cockroachdb/cockroach/blob/master/LICENSE), [Yugabyte](https://github.com/yugabyte/yugabyte-db), [Citus](https://github.com/citusdata/citus), [Neon](https://github.com/neondatabase/neon), [Multigres](https://github.com/multigres/multigres)). None is required: a single primary per cell plus replicas is enough for the targets below. Scale by **tenant cells**, not by distributed SQL [I].
- **Backups**: pgBackRest 2.59.2 (2026-09-27), WAL-G 3.0.9, Barman 3.20.1 (GPL-3.0) are all active [V]; PITR to object storage is part of the baseline.

**Bulk import/export.** COPY is the path: `FORMAT binary|csv`, `ON_ERROR ignore` with `REJECT_LIMIT` (PG18), `LOG_VERBOSITY` ([docs](https://www.postgresql.org/docs/18/sql-copy.html)); PG19 adds SIMD `COPY FROM`, `ON_ERROR SET_NULL`, and `COPY TO` JSON output and partitioned-table support [V]. `tokio-postgres` exposes `binary_copy` (`BinaryCopyInWriter`); sqlx exposes raw copy-in/out. Pattern: parse CSV in Rust, stream to a staging table via COPY, validate with set-based SQL, then `INSERT ... SELECT` into RLS-protected targets (section 1.3). Odoo's import is row-by-row ORM `create`, so this is a large, visible win.

## 9. Recommended baseline architecture

```
                 +-------------------------- obop binary (roles via flags) --------------------------+
Browser (Vite+)  |  web: axum HTTP/JSON + WS/SSE     worker: jobs, outbox dispatch, schedules        |
PWA / POS  <---> |  registry (Arc per tenant) + moka cache      migrate: sqlx migrate / tenant fan-out |
Mobile (SQLite)  +----------------------------------------------------------------------------------+
                        | pooled (PgBouncer txn mode in cloud; direct in single-box)   | direct conns:
                        v                                                               | LISTEN, migrations,
              PostgreSQL 18 (one DB per tenant, RLS backstop on company_id)             | advisory leader locks
              tables: domain, event_outbox, job, schedule_fire, idempotency, change_log  v
                        | logical replication / physical replicas          object_store trait:
                        v                                                   local FS -> S3-compatible
              read replica (reports, optional DuckDB)                       pgBackRest -> object store
   Optional at scale: NATS JetStream (cell-to-cell events), Valkey (only if cache must be shared)
```

Single box: `obop serve --roles=web,worker` plus Postgres (system package, container or `postgresql_embedded`). Scale-out without re-architecture: run more `web` nodes behind a load balancer, split `worker` nodes, add a bus (NATS) for cross-node realtime fan-out, add PgBouncer, add replicas, then add cells.

## 10. Capacity-planning targets (design targets, [I], to be validated by a harness)

Hetzner list prices after the 2026-06-15 adjustment, excl. VAT and excl. IPv4, Germany/Finland (FSN/NBG/HEL) table ([page](https://docs.hetzner.com/general/infrastructure-and-availability/price-adjustment/)) [V]: CX23 €5.49/mo, CX33 €8.49, CPX22 €19.49, CPX32 €35.49, CPX42 €69.49, CCX33 €138.49, CCX43 €275.99, CCX53 €533.49 (the USA and Singapore tables differ). Shapes from Hetzner's plan pages [V] ([cost-optimized](https://www.hetzner.com/cloud/cost-optimized), [regular performance](https://www.hetzner.com/cloud/regular-performance), [general purpose](https://www.hetzner.com/cloud/general-purpose)): CX23 2 vCPU/4 GB/40 GB NVMe; CX33 4/8/80; CPX22 2/4/80; CPX32 4/8/160; CPX42 8/16/320; dedicated CCX33 8 vCPU/32 GB/240 GB, CCX43 16 dedicated vCPU/64 GB/360 GB, CCX53 32/128/600. The plan pages showed "currently unavailable" banners when fetched 2026-10-01, so stock availability at these prices is unverified.

| | Odoo reference (vendor formula) | obop target |
|---|---|---|
| **SMB, one 2 vCPU/4 GB VPS, app + Postgres together** | about 24-30 concurrent users; app RAM ~1.6 GB; no headroom for PG | **100+ concurrent active users** (300 named), app RSS under 300 MB, PG `shared_buffers` 1 GB, `max_connections` 50, pool 16; p95 under 150 ms for reads and under 400 ms for posting; 100 req/s mixed, 20 posting tx/s; 10 POS terminals; 20 GB hot data; nightly full + 5-minute WAL archive, RPO 5 min |
| **SMB, 4 vCPU/8 GB** | about 48-54 users | 300+ concurrent, 500 req/s, 50 posting tx/s |
| **Thousand-tenant cloud** (assume 15 named users/tenant, 20 % peak concurrency, one request per user per 8 s) | 3,000 concurrent users means about 500 workers, roughly 250 vCPU and 160 GB RAM of app tier | Peak about 400 req/s, design for 2,000 req/s. **4 cells of 250 tenants**; per cell: 1 primary (16 dedicated vCPU/64 GB NVMe) + 2 replicas (HA standby, reports) + 3 stateless app nodes (8 vCPU) + PgBouncer; about 3 TB hot data across cells, object storage sized separately. Illustrative infra: 4×3×CCX43 + 12×CPX42 ≈ €4.1k/month (about €4 per tenant) |
| Realtime | n/a | 3-6k sockets total; tokio handles far more per node [U], so limit is fd/ulimit and fan-out CPU |
| Jobs | cron slices of 120 s per job | 1-2k jobs/s per cell (queue headroom is 10x+) |

Catalog arithmetic from doc 08 (600 to 900 tables per tenant) gives ~190k relations per 250-tenant cluster [I]: fine for Postgres, but it makes schema migrations a fan-out job (bounded concurrency, resumable per-tenant state, `sqlx.toml` per-crate migration tables) and makes `pg_dump` per tenant the backup/export unit alongside PITR. Enable PgBouncer `pool_idle_timeout` (off by default) so idle tenants do not hold pools [V 1.26].

**Validation plan (MUST before quoting numbers):** `pgbench` custom scripts on the exact VPS shapes above; `oha`/`k6` on the obop API; the same scenario scripted against Odoo 20 (current release; Odoo 19 as a second baseline) with the built-in profiler to split SQL vs Python time (no public split exists); soak with `pg_stat_statements`; measure the NOTIFY commit lock explicitly by running N tenants' notifier loops in one cluster.

## 11. Confidence & gaps

- **High [V]** (re-verified by the fact-check pass, see the Verification log): PG version/EOL dates and PG18/19 feature lists; NOTIFY commit lock in PG18 and master and its cluster-wide tag; COPY/RLS limitation; Odoo isolation, cron, sequences, bus, offline docs; vendor worker formula; licences and release dates of every repo named (GitHub/crates.io/npm metadata read on 2026-10-01).
- **Medium**: TechEmpower rps are derived from `totalRequests` and the 15 s duration in a file labelled "Continuous Benchmarking Run" (used as the Round 23 dataset); hardware specifics not re-verified. Electric benchmark figures come from images or summaries (the page text does confirm 4,000-6,000 row changes/s and the 100k-1M concurrent users with 960 tx/min claim). Hetzner price list was read via page extraction and re-checked (CCX13 really does jump from €15.99 to €42.99 in the Germany/Finland table; I use only CX/CPX/CCX33+ rows and treat them as list prices).
- **Low / unverified [U]**: TechEmpower "Citrine" hardware spec; tokio socket-per-node limits; SSI retry rates for obop workloads; sequence-counter throughput; the 3x-10x Rust multiplier (an inference from DB-bound arithmetic, not a measurement).
- **Gaps**: no public Odoo load-test report found; no measured RLS overhead, JSONB-vs-column cost, or schema-per-tenant degradation (doc 08 flags the same); PG19 GA date unknown; Zero/PowerSync/Electric Postgres-version support not checked; Postgres HA tooling (Patroni/CloudNativePG) not researched; search-engine access was blocked so community sentiment on Odoo performance is thin.

## Implications for obop

**MUST**

1. **MUST** baseline on **PostgreSQL 18** (18.6 now), run CI against 19 from RC, and track EOL dates (14 dies 2026-11-12). Keep SQL portable to 17 only where free; do not depend on PG19.
2. **MUST NOT** issue `NOTIFY` inside business transactions. The commit lock is cluster-wide (section 1.4). Use outbox rows + in-process wakeups + an optional post-commit notifier on a separate short transaction, or NATS. Amend the "signalling table / LISTEN/NOTIFY" idea in docs 06 and 08 accordingly, and never run one LISTEN connection per tenant database.
3. **MUST** build the sync layer in Rust: client-generated UUIDv7 ids, idempotent command queue (POS/mobile/field), and a commit-safe change-feed (xid8/snapshot cursor). Do not adopt Electric, PowerSync or Zero as runtime dependencies (non-Rust servers; PowerSync is FSL; Zero has no offline writes).
4. **MUST** match at least Odoo 19's offline bar for v1 (read-only cache of visited views, 2 GB cap equivalent, offline POS order capture with post-reconnect invoicing). Odoo 20 (September 2026) now also queues offline create/edit/archive/unarchive/delete for short interruptions, so decide explicitly whether v1 targets 19 or 20 parity; defer long-term general offline editing.
5. **MUST** implement gapless numbering as a counter row updated in the posting transaction (with `lock_timeout`), separate from PG sequences used for internal ids; document the per-sequence throughput ceiling.
6. **MUST** make jobs effectively-once: transactional enqueue, `SKIP LOCKED` claim with lease, unique idempotency keys, leader-less `schedule_fire` scheduling; no reliance on `pg_cron`.
7. **MUST** use atomic SQL for hot rows (quants, counters, balances) and put a bounded retry wrapper for 40001/40P01/55P03 in the data layer; flag the few SERIALIZABLE commands in the registry.
8. **MUST** stage bulk imports through non-RLS staging tables (COPY), because `COPY FROM` is rejected on RLS tables; keep `FORCE ROW LEVEL SECURITY` plus `SET LOCAL` context for the backstop.
9. **MUST** publish no performance multiple over Odoo until the Odoo-vs-obop harness exists (section 10); use the vendor formula (6 users/worker) as the only citable Odoo baseline.
10. **MUST** keep direct connections for migrations, LISTEN, session advisory locks and replication; everything else via PgBouncer 1.26+ in transaction mode (with `max_prepared_statements`).

**SHOULD**

11. **SHOULD** ship single-binary + Postgres as the default (`--roles` flags; system PG, container, or `postgresql_embedded` for solo/demo), with blob storage on local FS behind an `object_store`/`opendal` trait and S3-compatible stores as configuration.
12. **SHOULD** use WebSocket as the only realtime transport at launch with an outbox-backed resume cursor; keep SSE as fallback; defer WebTransport.
13. **SHOULD** use temporal constraints (`WITHOUT OVERLAPS` + `btree_gist`) for price lists, tax rates, contracts and bookings, and partition only append-heavy tables (bus/outbox, audit, ledger history) with pg_partman.
14. **SHOULD** use ledger rollup tables maintained in-transaction for balances and stock valuation, plain views for operational reports, and a logical-replication read replica for heavy analytics.
15. **SHOULD** keep caching in-process (`moka`) and add Valkey only when a shared cache is proven necessary; avoid Redis 8 (RSAL/SSPL/AGPL) and Dragonfly (BSL).
16. **SHOULD** choose Garage, SeaweedFS or (after soak time) RustFS for self-hosted S3; do not adopt MinIO.
17. **SHOULD** plan the cloud as **cells** (about 250 tenant databases per Postgres cluster, PgBouncer per cell, migration fan-out orchestrator, control-plane directory tenant to cell) rather than distributed SQL.
18. **SHOULD** standardise backups on pgBackRest (PITR to object storage) plus per-tenant `pg_dump` for export/restore and erasure.

**COULD**

19. **COULD** add NATS JetStream for cross-node realtime and cell events once more than one app node per cell exists.
20. **COULD** add `pg_textsearch` (BM25) and `pgvector` (0.8.x) as optional modules; avoid AGPL `pg_search`.
21. **COULD** evaluate `pg_ivm` for a few simple always-fresh aggregates and `pg_duckdb` on an analytics replica.
22. **COULD** use Loro or Automerge for collaborative rich-text documents only; not for transactional business data.
23. **COULD** revisit Turso (post-1.0) or PGlite as the in-browser/local store, and PgDog (post-0.x, AGPL caveat) for sharding/pooling.
24. **COULD** publish the capacity harness and results as part of the project's open benchmarks, since Odoo has none.

## Verification log

Fact-check pass of 2026-10-01 (adversarial; primary sources re-read via the GitHub API, crates.io, npm, vendor docs and raw files, using a different route from the author's where possible, e.g. the GitHub API for releases instead of release pages, raw `ph.json` recomputed from scratch, `download.osgeo.org` for PostGIS).

**Confirmed as stated: about 185 individual claims** (approximate tally). All 12 flagged claims were checked; the parts that survived unchanged are:
- PG: 18.6 is the newest 18.x on the versioning page; EOL dates 18 = 2030-11-14, 17.11 = 2029-11-08, 16.15 = 2028-11-09, 15.19 = 2027-11-11, 14.24 = 2026-11-12; 19 Beta 4 (banner 2026-09-24), release notes "AS OF 2026-09-14", release date `2026-??-??`.
- NOTIFY: `LockSharedObject(DatabaseRelationId, InvalidOid, 0, AccessExclusiveLock)` in `PreCommit_Notify` in both `REL_18_STABLE` and `master`; `LockSharedObject` builds the tag with `InvalidOid` as database; `GlobalChannelKey` carries `dboid`. The Recall.ai post and the NOTIFY doc limits (8000 bytes, 8 GB queue, no PREPARE, commit-order delivery) also hold.
- COPY FROM unsupported on RLS tables, COPY TO supported (PG18 docs, verbatim).
- Odoo (19.0 docs): `(#CPU * 2) + 1`, 1 worker ~= 6 concurrent users, light ~150 MB, heavy ~1 GB, 20 % heavy, 9 workers ~= 3 GB example and its limit values; `REPEATABLE READ`; retry set; pool 64; idle 600 s; bus GC 24 h and 8000-byte payload split; cron 5 failures and 7 days; `FOR UPDATE NOWAIT` for `no_gap`; POS offline behaviour; 19.0 offline mode is view-only with a 2 GB cap.
- Zero: GA "As of March 2026", "Zero doesn't support offline writes", "> ~100GB" under "might not be a good fit", Apache-2 client and server, npm 1.9.0.
- PowerSync: FSL-1.1-ALv2 licence text, TypeScript, Rust SDK (beta), .NET (beta), Tauri (alpha), Cloud Free / Pro from $49 / Team from $599. Electric: Elixir sync-service (1.8.1, 2026-09-07), Apache-2.0, "read-path sync engine", 1.0 post dated 2025-03-17, four write patterns.
- Turso v0.8.1 (2026-09-29) with "we have not yet reached 1.0"; cr-sqlite last release v0.16.3 (2024-01-17), MIT (commits continue to 2026-08-10, build fixes).
- MinIO repo archived, README "NO LONGER MAINTAINED", last release RELEASE.2025-10-15; RustFS 1.0.0 (2026-09-16) Apache-2.0; Garage v2.4.1 (2026-09-08) AGPL-3.0, "3 zones", 1 GB, no erasure coding on the home page; SeaweedFS 4.48 (2026-09-28) Apache-2.0.
- TechEmpower: axum-pg Fortunes ~1,114k, FastAPI ~109k, Rails ~43k, Django ~33k (derived `totalRequests/15`); run start 2025-01-30; no Round 24 announced; repo last push 2026-03-24.
- PgBouncer 1.26.0 (2026-09-23), the three CVEs and their descriptions, `search_path` tracking on PG18+, single-threaded, transaction mode "Never" for SET/RESET, LISTEN, WITH HOLD, PREPARE, session advisory locks; `max_prepared_statements` since 1.21.0 (2023-10-16).
- Licences: Redis 8+ RSALv2/SSPLv1/AGPLv3 (8.10.2, 2026-09-17); Valkey 9.1.2 BSD-3-Clause; Dragonfly 2.0.0 BSL 1.1, change date Nov 1, 2030; ParadeDB AGPL-3.0 (0.25.11); CockroachDB Software License (v26.2.7 tag).
- Versions/dates re-read and matching: all crates.io versions named (sqlx 0.9.0 through pgmq 0.33.7 stable), GitHub releases for pg_partman, pg_cron, pgmq, pg_ivm, pg_duckdb, DuckDB, pg_mooncake, PgCat, Supavisor, PgDog, NATS, Redpanda, Automerge, Loro, PGlite, pgBackRest, WAL-G, Barman, libSQL, pgrx, Citus, YugabyteDB, Multigres, Rust 1.98.1, Hetzner prices and plan shapes, caniuse WebTransport (Chrome 97, Edge 98, Firefox 114, Safari/iOS 26.4), Graphile Worker figures (15.6k / 184k jobs/s, ~4 ms latency), Odoo issues #240721, #25967, #43153, #55494, #90465, OdooLocust 80 stars, Electric benchmark figures.

**Corrections (was / now / source):**

| # | Was | Now | Source |
|---|---|---|---|
| 1 | "18.6 ... released 2025-09-25" (headline) | 18.0 was released 2025-09-25; **18.6 was released 2026-08-13** | https://www.postgresql.org/docs/18/release-18-6.html |
| 2 | TechEmpower Fortunes "fastest Python ~164k (granian)" | ~174k (`fastwsgi-asgi`); granian-rsgi ~164k is second | https://www.techempower.com/benchmarks/results/round23/ph.json |
| 3 | "Single query" row = 1,243k / 173k / 117k / 57k / 38k | Those numbers are the multiple-queries test at 1 query. The actual single-query (`db`) test is axum ~1,270k, uvicorn ~256k, FastAPI ~164k, Rails ~63k, Django ~44k (~29x); both rows now shown | ph.json |
| 4 | Updates row: Django ~20k, axum/Django ~21x | Django ~20k was the MySQL entry; Postgres entry `django-postgresql` ~27k, so ~16x; row relabelled "1 update/request (best interval)" and a canonical 20-updates/request row added (axum ~56k, Django ~2.1k, ~27x) | ph.json |
| 5 | Headline and 3.2: Rust "4x to 7x over fastest Python, 20x to 35x over Django" | about 2.6x to 7x over the fastest Python entries, 16x to 34x over Django | ph.json (recomputed) |
| 6 | PowerSync service "v1.26.1 (2026-09-11)" | v1.27.0 exists as a git tag and npm image (published 2026-09-30); v1.26.1 is only the last GitHub Release page | https://github.com/powersync-ja/powersync-service/tags , npm registry metadata of `@powersync/service-image` (`time` field; re-run with `vp info @powersync/service-image time`) |
| 7 | Odoo offline is read-only ("Odoo's own offline bar is modest"; MUST #4 "nothing more") | True for Odoo 19 only. **Odoo 20 (September 2026) allows offline create/edit/archive/unarchive/delete with a local queue synced on reconnect** (docs say short interruptions only). Headline 4, section 2 table, MUST #4 and the validation plan updated | https://www.odoo.com/odoo-20-release-notes , https://github.com/odoo/documentation/blob/20.0/content/applications/general/offline_mode.rst |
| 8 | Odoo tests pin "about 70 for a 2-order batch with lines" and "~1,500" for a complex batch | Active tests pin 34 / 39 / 44 / 53; the 70-query and 1,593-query tests are disabled (`__test_...`, "not deterministic"), the latter "1954 sometimes" | https://github.com/odoo/odoo/blob/master/addons/sale_stock/tests/test_create_perf.py |
| 9 | Retry set cited to `sql_db.py` as Odoo 19 behaviour | In 19.0 the tuple is in `odoo/service/model.py` (5 tries, exponential backoff); it moved to `sql_db.py` in master (20.1 alpha) and 20.0 | https://github.com/odoo/odoo/blob/19.0/odoo/service/model.py |
| 10 | Cron "min 10 runs or 120 s per job slice" | at least 10 iterations and at least 120 s (loop condition is `loop_count < 10 or now < cron_end_time`) | https://github.com/odoo/odoo/blob/master/odoo/addons/base/models/ir_cron.py |
| 11 | Virtual generated columns "replicable via logical replication" | only STORED generated columns can be published (opt-in, `publish_generated_columns = stored`) | https://www.postgresql.org/docs/18/logical-replication-gencols.html |
| 12 | PG19 "COPY of partitioned tables" and "JSON output" | these are `COPY TO` features | https://www.postgresql.org/docs/devel/release-19.html |
| 13 | Electric status without ownership change | Electric announced it is joining Databricks (via Neon) on 2026-08-11 | https://electric.ax/blog/2026/08/11/electric-joining-databricks |
| 14 | "Keep PgBouncer `pool_idle_timeout` on" | the setting is new in 1.26 and disabled by default; it must be enabled | https://github.com/pgbouncer/pgbouncer/blob/master/NEWS.md |
| 15 | pg_cron "tracks jobs in one database per cluster; poor fit" | installs into one database; `cron.schedule_in_database()` can run jobs in others; wording softened | https://github.com/citusdata/pg_cron |
| 16 | PostGIS version [U]; CCX shapes [U]; Meilisearch licence "unresolved" | PostGIS newest source tarball 3.6.4; CCX33 8/32/240, CCX43 16/64/360, CCX53 32/128/600 verified; Meilisearch MIT plus BUSL-based EE parts | https://download.osgeo.org/postgis/source/ , https://www.hetzner.com/cloud/general-purpose , https://github.com/meilisearch/meilisearch/blob/main/LICENSE |
| 17 | Recall.ai "three outages in March 2025" | three periods of downtime 2025-03-19 to 2025-03-22 | https://www.recall.ai/blog/postgres-listen-notify-does-not-scale |
| 18 | Hetzner prices "excl. VAT" | also excl. IPv4 and specific to the Germany/Finland table (USA/Singapore differ) | https://docs.hetzner.com/general/infrastructure-and-availability/price-adjustment/ |
| 19 | RustFS 1.0.0 as newest | 1.0.0 is the newest stable; `1.0.1-preview.*` builds were published daily through 2026-09-30 | https://github.com/rustfs/rustfs/releases |

**Added facts:** Odoo 20.0 is released (docs 01 and 02 agree) and `master` is 20.1 alpha; the Odoo 20 `deploy.rst` worker rule is identical to 19; Odoo master and 20.0 still use psycopg2 (psycopg3 RFC #240721 still open); Electric/Databricks; RustFS preview flags (`rio-v2`); Hetzner plan pages showed "currently unavailable" banners.

**Unresolved / unverified:**
- Exact day Odoo 20.0 shipped: the release notes say "September 2026" and the 20.0 branch and docs exist, but no exact date was found.
- Stock availability of the Hetzner plans at the listed prices (plan pages showed "currently unavailable" banners when fetched; may be a page artefact).
- TechEmpower "Citrine" hardware description (28-core-class) not re-verified (unverified); "Round 23" is the file's label for `round23/ph.json`, whose internal title is "Continuous Benchmarking Run 2025-01-30".
- PostGIS 3.6.4 release date; CockroachDB v26.2.7 release date (git tag only, no GitHub Release page).
- The `sqlx.toml` multi-tenant migration-table renaming feature of sqlx 0.9 is not independently confirmed beyond the changelog mention; the sqlx raw copy-in/out API was not checked.
- All section 10 capacity targets, the 3x to 10x Rust multiplier, cell sizing and sequence-counter throughput remain design inferences [I], not measurements.
- Numeric ratios in section 3.2 depend on picking the Postgres-backed Django/Rails entries and the best-concurrency point; other reasonable picks shift them by up to about 25 percent.

- Owner-constraint pass (2026-10-01): the owner fixed `vp` (Vite+) plus pnpm as the only JavaScript tooling (no npm, npx, yarn or bun). The only command example using the npm CLI was the `npm view` cell in correction #6; it now names the registry metadata and the equivalent `vp info @powersync/service-image time` (`vp info <pkg> [field]` resolves to `pnpm view`, checked in the vite-plus source `crates/vp_pm_cli/.../view.rs`). The original cell cited the npm CLI command; the data point itself (the registry `time` field, v1.27.0 published 2026-09-30) is unchanged and was not re-fetched in this pass. All other "npm" mentions in this file (npm registry metadata for Zero, PowerSync and others) are registry-level facts and are unchanged.
