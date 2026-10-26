# Deploying obop: the operator guide

- **Status:** working document, 2026-10-17. Written against the code in the repository, not a design target. Decision record: D59 in [DECISIONS.md](DECISIONS.md).
- **Who this is for:** whoever runs obop in production, on their own server or on Sliplane. Everything here is a command you can run.

## What exists

| Piece | Command | What it does |
|---|---|---|
| Instance | `obop serve` | One business: REST, MCP, backoffice. SQLite locally, PostgreSQL when `OBOP_DATABASE_URL` is `postgres://` (D44). **Several processes may serve one PostgreSQL database** (see "More than one process"). |
| Control plane | `obop platform serve` | Sign up, then create, list and delete your own instances; the forum (`/community`) and status page (`/status`). Runs a provisioner per instance. |
| Provisioners | `--provisioner local\|sliplane\|fake` | `local` starts child `obop serve` processes with SQLite; `sliplane` creates a database on a shared cell PostgreSQL and a Sliplane service per instance; `fake` is for tests. |
| Rolling upgrade | `obop platform upgrade-instances` | Moves every running instance to a published image, one at a time (see "The published image"). |
| Backup and restore | `obop backup`, `obop backup verify`, `obop restore`, `deploy/restore-drill.sh` | Consistent logical backups, checked restores (see "Backups"). |
| Key rotation | `obop secrets status`, `obop secrets rotate` | Re-seals every secret under a new server key while serving (see "The server key"). |
| Schema | `obop migrate status\|plan\|run` | The schema version of a database and the steps that upgrade it (see "Upgrades"). |
| Modules | Modules page, or `obop modules ...` | Admins install, upgrade and remove declarative packages (`obop.toml`) from an upload or the marketplace. |

The owner's Argon2 hash is copied into the new instance as its first admin (`platform.admin`), so the signup email and password also sign in to the instance. The control plane never sees the password after signup.

## Try the whole thing on one machine

```bash
(cd web/apps/backoffice && vp build) && deploy/build-site.sh
cargo run -- platform serve --static web/site --instance-static web/apps/backoffice/dist
```

Open http://127.0.0.1:8090 (the website; the sign-up app is under `/app/`), create an account, create an instance, follow its link, sign in with the same credentials, then open Modules.

## Updating from inside the instance (D63)

Administrators update modules and the instance itself from Customize, without a shell.

**Modules.** Customize, Modules, Updates lists the packages with a newer version in the bundled catalogue or the configured marketplace, with the release notes (`changelog`), what changes in the data, the data steps and the installed modules that depend on it, each with the range it supports. Update one (a plan dialog first), update all that fit, or roll back. Same from the CLI: `obop modules updates`, `modules update ID`, `modules update-all`, `modules rollback ID`. See docs/MODULES.md 7 and 7.1 for what is refused and what a package declares.

**The instance.** Customize, Updates (`obop update check|status|start|rollback`) reads the release feed, judges every installed module against the new core (their declared `requires_core` and ranges on built-in modules; incompatible ones block the release), takes a backup, and for a *managed* instance asks the control plane to run the upgrade.

| Variable | Where | Meaning |
|---|---|---|
| `OBOP_UPDATE_URL` | instance | The control plane's public address. Set by the provisioner. |
| `OBOP_UPDATE_TOKEN` | instance (secret) | The instance's update token, set by the provisioner (Sliplane: `--secret-env`). Sealed into the database at start-up; the variable only hands it over. |
| `OBOP_UPDATE_FEED` | instance | The release feed. Default `<OBOP_UPDATE_URL>/api/releases`, else `https://obop.dev/api/releases`; `off` disables. Only `https`, or `http` to localhost. Read only when an administrator checks. |
| `OBOP_BACKUP_DIR` | instance | Where the backup before an update goes (mount a volume). Without it an update needs the administrator's confirmation that they have their own backup. |
| `OBOP_CONTROL_URL` | control plane | The address handed to new instances (falls back to `OBOP_PUBLIC_URL`). |

Control plane, operator side: `obop platform release publish --version V --image REF [--notes T|--notes-file F] [--min-from V]`, `release list`, `release withdraw V`; `obop platform register-instance SLUG --url URL [--external-id ID] [--image REF] [--token-stdin]` registers an instance this control plane did not create and prints (or takes) its token; `obop platform update-token SLUG [--revoke]` rotates or revokes a token.

How it is kept safe: the token is 256 random bits, stored on the control plane only as a SHA-256 hash with a one-year expiry, and good for three things for its own instance: read the update state, ask for a *published version* (no field for an image or URL), ask to go back to the previous image. It opens no account API, requests are rate limited (12 per hour per instance), a wrong, expired, revoked or replaced token is refused alike, and every use is written to the control plane's audit log (`instance_update_started|succeeded|rolled_back|failed`, `release_published`, `update_token_minted`). The instance writes `instance_update_requested|finished|failed` and `module_*` to its own `security_event`. The feed is integrity-checked (a digest per release) but not signed; it never decides what runs, because the control plane deploys the image it published itself. When the new version does not answer its health check the control plane puts the previous image back and reports `rolled_back`. Schema steps are forward-only (D59): going back restores the old image, not the data; the backup path is shown on the page.

**Self-hosted** (no control plane): the page, and `obop update check`, show the exact commands: `obop backup`, `docker pull <image>`, restart with `OBOP_AUTO_MIGRATE=1` (or `obop migrate plan` then `run`), `obop update check`.

## Configuration

Command line flags win over variables. Secrets are set as secret environment variables of the service (Sliplane: `--secret-env`), never in the database, the repository or a plain variable.

### Every instance and the control plane

| Variable | Meaning |
|---|---|
| `OBOP_DATABASE_URL` | Where the data lives (`--db`): `sqlite://obop.db` (the default for an instance; `sqlite://obop-platform.db` for the control plane), `postgres://user:pass@host/db?sslmode=require` (see "Database connections and TLS"), `memory:`. |
| `OBOP_DB_POOL_SIZE` and the other `OBOP_DB_*` variables, `OBOP_LOCK_DATABASE_URL` | The PostgreSQL connection pool: size, acquire timeout, recycling, statement time limits, pooler mode, direct address for locks. See "Connection pool and poolers". |
| `OBOP_SECRET_KEY` | The current server key, 32 random bytes as 64 hex digits (`openssl rand -hex 32`). Seals two-factor secrets and connector credentials, and the sessions and approval plans kept in the database. See "The server key". Without it a key is generated and stored in the database next to the data, which protects nothing against someone holding a copy of the database. |
| `OBOP_SECRET_KEY_PREVIOUS` | Earlier keys, comma separated: what the current key replaced and that may still protect stored values. Only needed between a key change and the end of `obop secrets rotate`. |
| `OBOP_AUTO_MIGRATE` | `1` lets a starting server run pending schema steps itself, `0` makes it refuse and print the command. Default: on for SQLite, **off for PostgreSQL**. `--auto-migrate` and `--no-auto-migrate` override it. |
| `OBOP_REQUIRE_MFA_FOR_ADMINS` | `0` changes the *default* of the setting `security.require_mfa_for_admins` to off (the production default is on). A value an administrator stored under Customize > Security wins. |
| `OBOP_DEV_INSECURE_KEY` | `1` selects a public, fixed key and logs a warning. Development and tests only. |
| `OBOP_MFA_ISSUER` | The name shown in the authenticator app (default `obop`). |
| `OBOP_PUBLIC_URL` | The external URL behind a proxy (`https://acme.example.com`): MCP discovery documents, the issuer, endpoints and resource identifier of the OAuth sign-in (D64), webhook and OAuth redirect addresses, the community pages, and the origin and relying party id of passkeys (D65). **Set it behind any proxy**: the resource a client asks for must equal `<OBOP_PUBLIC_URL>/mcp`, and a passkey works only at the address it was made for. |
| `OBOP_ALLOWED_ORIGINS` | Comma-separated exact browser origins (`https://app.example.com`; `http://localhost:5173` on loopback only) that may call `/api` and `/mcp` from another origin. Off when unset. A wildcard or any other invalid entry stops the server at start. See "Browser clients and allowed origins". |
| `OBOP_METRICS_TOKEN` | Opens `GET /metrics` to a scraper that sends `Authorization: Bearer <token>` (at least 16 characters, for example `openssl rand -hex 24`; a shorter one stops the server at start). Without it `/metrics` is served only when the server is bound to a loopback address, and then only to direct loopback callers without proxy headers; on any other bind it answers 404. See "Health and metrics". |
| `OBOP_ALLOWED_ORIGINS` | Comma-separated browser origins allowed to call `/mcp`. |
| `OBOP_BULK_PER_MINUTE` | `bulk_create` calls (and `obop import`) a person may start per minute, counted across all processes, dry runs included (default 12; `0` turns the limit off). A call takes at most 1,000 rows and 1 MiB, so the default is at most 12,000 rows a minute per person (D70). |
| `OBOP_MCP_RATE_PER_MINUTE` | Requests per token per minute on `/mcp`, counted across all processes (default 600; `0` is off). |
| `OBOP_TRUST_PROXY`, `OBOP_PUBLIC_SITE`, `OBOP_ADMIN_PREFIX` | Public site settings (`--trust-proxy`, `--public-site`, `--admin-prefix`). Behind a proxy set `OBOP_TRUST_PROXY=1`, or every visitor shares the proxy's address in the rate limits. |
| `OBOP_MARKETPLACE_URL` | An extra module catalogue. |
| `OBOP_CONNECTORS_ALLOW_LOOPBACK` | `1` lets connections reach loopback addresses. Tests only. |
| `OBOP_TOKEN` | The bearer token for `obop mcp`, `obop call`, `obop commands`. |

### Control plane only

| Variable | Meaning |
|---|---|
| `OBOP_PLATFORM_ADMINS` | Staff emails (forum moderation, status console). |
| `OBOP_STATUS_INTERVAL_SECS` | How often the status checks run (default 60). With several control-plane processes one takes each round. |
| `OBOP_DATA_DIR` | Where the `local` provisioner keeps instance data. |
| `OBOP_IMAGE` or `OBOP_REPO` (+ `OBOP_BRANCH`) | Where `sliplane` instance services come from. **`OBOP_IMAGE` is preferred and wins when both are set.** |
| `OBOP_REGISTRY_CREDENTIAL` | A Sliplane registry credential id, for a private image. |
| `OBOP_INSTANCE_DOMAIN` | A domain with a wildcard DNS record (`*` A record to the Sliplane server's IP), for example `obop.dev`. Each new instance is also given `<slug>.<domain>` and uses it once it resolves; otherwise it keeps its `*.sliplane.app` address. |
| `OBOP_UPGRADE_SETTLE_SECS` | Seconds to let a redeploy settle before asking Sliplane whether it is live (default 30). |
| `SLIPLANE_ACCOUNT`, `SLIPLANE_API_KEY`, `OBOP_SLIPLANE_PROJECT`, `OBOP_SLIPLANE_SERVER`, `OBOP_CELL_ADMIN_URL`, `OBOP_CELL_INSTANCE_HOST` | The Sliplane provisioner (see "Sliplane"). |

### Tests

`OBOP_TEST_POSTGRES_URL` adds PostgreSQL to `cargo test`, including the multi-process tests and the connection pool tests (`cargo test -p obop-storage pg_pool`, `cargo test -p obop --test pool`); `OBOP_BIN` points the end-to-end suite at a binary. The tests that restart a server, put PgBouncer in front of it (`edoburu/pgbouncer`) or enable TLS start containers of their own and are skipped, with a message, when Docker is not available.

## Database connections and TLS (D66)

By default a `postgres://` URL connects **without TLS**, exactly as before: the password and every record cross the network in the clear. That is acceptable only on a private network you control (same host, a private overlay between containers). For any other path put the mode in the URL; every PostgreSQL connection obop opens uses it (the main connection, the lock connections, the export snapshot of `obop backup`, the blob store, the control plane's cell connections and the status check of the cell).

| `sslmode` | Encrypted | Certificate checked | Use |
|---|---|---|---|
| (absent) or `disable` | no | no | same host or a private network |
| `prefer` | when the server offers it | no | not recommended: an active attacker can answer "no TLS" |
| `require` | yes, or the connection fails | no (but see below) | the minimum for any other network: stops eavesdropping |
| `verify-ca` | yes | chain and validity, not the host name | when the host name is not in the certificate |
| `verify-full` | yes | chain, validity and host name | **recommended** when you know the server's CA |

```text
postgres://obop:PASSWORD@db.internal:5432/obop?sslmode=require
postgres://obop:PASSWORD@db.example.com/obop?sslmode=verify-full&sslrootcert=/etc/obop/db-ca.pem
postgres://obop:PASSWORD@db.example.com/obop?sslmode=verify-full&sslrootcert=system
```

- `sslrootcert=PATH` is a PEM file with one or more CA certificates (a self-signed server certificate may be given as its own root, provided it is not marked as a CA). `sslrootcert=system` (also the default for the two `verify-*` modes) means the bundled Mozilla roots, for a server with a certificate from a public CA. The file is read when the URL is read, so a wrong path stops the server at start-up. Give `sslrootcert` only with `require`, `verify-ca` or `verify-full`; **`require` with a root certificate is checked like `verify-ca`**, as in libpq.
- Refused with a clear message at start-up: `sslmode=allow`, a misspelt mode, `sslrootcert` without a mode that uses it, `sslcert`, `sslkey` and `sslcrl` (client certificates are not built), and any address that is not a `postgres://` or `postgresql://` URL. A failed verification (untrusted chain, expired certificate, wrong name, a server without TLS) is an error, never a fall back to a plain connection. The password is not in any of these messages.
- Turn TLS on at the server (`ssl = on`, `ssl_cert_file`, `ssl_key_file`), and to forbid plain connections use `hostssl` lines in `pg_hba.conf`. obop's TLS is rustls (TLS 1.2 and 1.3, no system OpenSSL); the server's certificate key must be RSA, ECDSA or Ed25519.
- Put the same parameters in `OBOP_CELL_ADMIN_URL`, and in `--db` for `obop backup`, `restore`, `migrate` and `secrets`. The database URLs the control plane gives to **new** hosted instances do not carry `sslmode` yet; keep the cell on a private network until they do.
- **Not tried** against a managed PostgreSQL service, a connection pooler in TLS mode, a certificate chain with intermediates, or a public CA: only against the stock `postgres:17` image with a self-signed certificate (`cargo test -p obop-storage tls` starts one when Docker is available).

## Connection pool and poolers (D74)

Every process that uses PostgreSQL keeps a few small pools of connections. A request takes a connection for the length of one database call and gives it back; nothing waits behind a long read, and a slow database call never blocks the others.

- **The main pool** (`OBOP_DB_POOL_SIZE`, default 8, at most 64) serves reads and writes. Connections are opened when needed, kept while they are used, closed after 5 minutes idle and after 30 minutes of life, and a connection that sat idle for a second or more is checked (a no-op round trip) before it is handed out, so a connection the server or a firewall dropped is replaced, not used. A connection that failed, or whose holder panicked, is never put back.
- **When it is full.** A call that finds no free connection waits at most `OBOP_DB_ACQUIRE_TIMEOUT_MS` (2 s), and only if fewer than `OBOP_DB_MAX_WAITERS` (256) calls already wait; then the call is refused (`storage_busy`). The queue is bounded in length and in time, so a flood costs a bounded number of parked threads, not memory that grows with the load. The other storage faults are `storage_unavailable` (the database cannot be reached or the connection broke) and `storage_timeout` (a statement ran into its time limit and was rolled back).
- **What a 503 means (and when it is not sent).** The pool is leased per database call, and a request makes several calls. The rule, in full: **a request that hits a storage fault before it has committed any change is answered `503` with a `Retry-After` header (1 s for `storage_busy`, 5 s for the other two) and `{"error":"storage_busy"|"storage_unavailable"|"storage_timeout","code":...,"hint":...}`, and it has changed nothing**, whatever the code above the store made of the refused call ("not signed in", "not found", an empty list). Once a call of the request was refused, every further change of that request is refused too, so a command that carries on after a swallowed refusal cannot write what it derived from it. **A request that had already committed a change keeps its own answer** even if a later step was refused (the read-back of what was written, a log entry, a notification): a 503 there would make the client repeat a change that was made. A create or update whose read-back is refused answers with what was written (computed fields missing). Such faults are counted (`obop_storage_faults_total{answer="kept"}`) and logged; the security-log write that follows a change is tried four times before it is given up. **Retrying after a 503 is therefore safe** for `storage_busy` and `storage_timeout`; after `storage_unavailable` a change that was in flight when the connection broke may or may not have been applied (single creates and updates have no idempotency key; `bulk_create` has one). The 503 has the shape of its route: the REST error, an OAuth `temporarily_unavailable` on `/oauth/token`, `/oauth/register` and `/oauth/revoke`, a JSON-RPC error on `/mcp`, a small page where a page was coming; CORS and security headers the route set are kept. The control plane (`obop platform serve`) and the community pages answer the same way. `/healthz`, `/readyz` and `/metrics` decide for themselves: a full pool does not make `/readyz` fail (the database is there, it is only busy; if the probe itself finds every connection in use it reports ready), and `obop_db_up` counts a busy pool as up. Threads that have nobody to answer (the connector worker, a provisioning in progress) are not refused for being busy: a lookup that was refused leaves a queued delivery queued, the instance policy then reads as closed (nothing leaves) until it can be read, and the last write of a provisioning is waited out for up to a minute.
- **Long reads** (a scan that folds a whole model, `obop backup`'s export, a restore) take one connection each for as long as they run, inside one repeatable-read read-only transaction, so they see one snapshot and block no writer. At most `OBOP_DB_LONG_READS` of them (default half the pool) run at once, so a few heavy reads cannot take every connection from interactive requests. A scan fetches 500 rows at a time; a consumer that stalls for more than `OBOP_DB_IDLE_IN_TRANSACTION_MS` (2 minutes) between two fetches loses its snapshot.
- **Locks.** Cross-process locks are PostgreSQL session advisory locks and need a session that lasts, so they use a pool of their own (`OBOP_DB_LOCK_POOL_SIZE`, at most 8) that does not count against the main pool. The locks one thread holds at the same time share one connection, so a request that holds the write lock and asks for a second lock never waits for another connection. A ninth thread that wants a lock while eight others hold theirs waits the acquire timeout and then gets `storage_busy`.
- **Blobs** (images and files kept in the database) use a pool of their own (`OBOP_DB_BLOB_POOL_SIZE`, 4).
- **Time limits.** Every statement runs under a `statement_timeout` chosen by what it is: a point read or write, a counter, a short-lived value 5 s; a list, search or lookup 10 s; a scan or a count of everything 15 s; a transaction of several statements, a restore, an export, an index 60 s. A scan or export fetch has the limit per fetch, not for however long the consumer takes. Set a limit to `0` to leave the server's own `statement_timeout` (for example one set with `ALTER ROLE`) in place: single statements `RESET` it, transactions and scans set it `TO DEFAULT` for their own length, whatever limit another call left on that connection. A cancelled statement is recognised by its SQLSTATE (57014), not by its message, so a server whose messages are in another language (`lc_messages`) behaves the same.
- **Batches that name the same records.** Several writes of one process now run at the same moment on different connections (before, one connection serialised them). An atomic batch (`apply`, which `create`, actions, `bulk_create` and checkout use) first takes a transaction-level advisory lock for each record it names, in sorted order, so two batches that touch the same records in opposite order queue instead of deadlocking and two that create the same record cannot both think it is missing; a transaction the server still rolls back for a deadlock (40P01) or a serialisation failure (40001) is run again, up to three more times. Database error texts now carry the SQLSTATE and the server's message (no detail, which may hold values).

| Variable | Default | Meaning |
|---|---|---|
| `OBOP_DB_POOL_SIZE` | 8 (1 to 64) | Connections of the main pool, per process. |
| `OBOP_DB_ACQUIRE_TIMEOUT_MS` | 2000 (50 to 60000) | How long a request waits for a free connection (this includes opening a new one) before `storage_busy`. |
| `OBOP_DB_MAX_WAITERS` | 256 | Requests allowed to wait at once; more are refused immediately. `0` never waits. |
| `OBOP_DB_LONG_READS` | half the pool, at least 1 | Scans, exports and restores allowed at once. |
| `OBOP_DB_IDLE_TIMEOUT_SECS` | 300 | An idle connection is closed after this long. |
| `OBOP_DB_MAX_LIFETIME_SECS` | 1800 | A connection is closed after this long (at the latest when it comes back). |
| `OBOP_DB_HEALTH_CHECK_IDLE_MS` | 1000 | A connection idle this long or longer is checked on checkout; `0` checks every checkout (one more round trip per call). A read that finds a dead connection is repeated once on a fresh one; a write is not repeated (it may have been applied) and fails once with `storage_unavailable`. |
| `OBOP_DB_CONNECT_TIMEOUT_MS` | 10000 | Opening one connection (TCP, TLS, login). |
| `OBOP_DB_STATEMENT_TIMEOUT_POINT_MS`, `_LIST_MS`, `_AGGREGATE_MS`, `_BATCH_MS` | 5000, 10000, 15000, 60000 | The limits above; `0` leaves the server's value. |
| `OBOP_DB_IDLE_IN_TRANSACTION_MS` | 120000 | How long a scan or export transaction may sit idle between two fetches; `0` is no limit. |
| `OBOP_DB_LOCK_POOL_SIZE` | 8 (1 to 8) | Connections for advisory locks. |
| `OBOP_DB_BLOB_POOL_SIZE` | 4 (1 to 64) | Connections of the blob store. |
| `OBOP_DB_POOLER` | `direct` | `transaction` when the database address is a pooler in transaction mode (below). **Refused at start-up without `OBOP_LOCK_DATABASE_URL`.** |
| `OBOP_LOCK_DATABASE_URL` | not set (the lock connections then use the database URL) | A direct address (same `sslmode` rules) for the lock connections. **Required** with `OBOP_DB_POOLER=transaction` and refused there when it is the same address as the database URL. |

A value that is not a number in range, an unknown pooler, or a pooler in transaction mode without a lock address stops the server at start-up with a message that names the variable (and never echoes a value that looks like an address). Each of the three pools is opened lazily; a process with no traffic holds one to three connections. A `connect_timeout`, `keepalives_idle` or `tcp_user_timeout` in the database URL wins over `OBOP_DB_CONNECT_TIMEOUT_MS` and the pool's keep-alive defaults (60 s idle, 30 s user timeout). **The `OBOP_DB_*` and lock variables describe the database the process serves from.** A command that targets another database with `--db` (`obop backup`, `restore`, `migrate`, `secrets`) reads the same variables: unset `OBOP_LOCK_DATABASE_URL` for it (it would be refused as pointing at another database) or point it at the matching direct address. The control plane opens the databases of new instances with one connection and none of these settings.

**Sizing.** Count what PostgreSQL must allow: for every obop process up to `OBOP_DB_POOL_SIZE` + `OBOP_DB_LOCK_POOL_SIZE` + `OBOP_DB_BLOB_POOL_SIZE` (20 with the defaults) plus one for each connection being opened, summed over all processes, plus your own tools and `superuser_reserved_connections`, must stay under `max_connections` (100 by default). Raise `max_connections`, lower the pool sizes, or put a pooler in front. Connections are named `obop-main`, `obop-lock` and `obop-blob` in `pg_stat_activity` (`application_name`, unless your URL sets one).

**Supported topologies.**

| Topology | Setting | Status |
|---|---|---|
| obop processes straight to PostgreSQL (one or several) | nothing | Tested (`cargo test -p obop-storage`, `-p obop --test multi_process`, `--test pool`). |
| PgBouncer in **session** mode | nothing | The same as direct for the application; **not tried**. |
| PgBouncer in **transaction** mode | `OBOP_DB_POOLER=transaction` and `OBOP_LOCK_DATABASE_URL=<direct address>` (both required) | Tested against PgBouncer 1.26.1 (the image `edoburu/pgbouncer@sha256:8638aa56...`, pinned by digest in the test, which asks it `SHOW VERSION`; `pool_mode = transaction`, two server connections shared by eight client connections, protocol-level prepared statements on or off). |
| PgBouncer in statement mode, other poolers (Supavisor, RDS Proxy, Odyssey, pgcat), managed poolers | | Not supported / not tried. |

Behind a pooler in **transaction mode** the server connection behind a client connection changes between transactions, so anything set on the session is lost or lands on somebody else's session. obop therefore does two things only in this mode. It runs every database call in a transaction of its own and sets the time limit with `SET LOCAL` (which ends with the transaction); the price is about twice the round trips per call. And it must not take its advisory locks through the pooler: a lock belongs to a server session, and the unlock would reach another one. Give `OBOP_LOCK_DATABASE_URL` a **direct** address of the same database (a PgBouncer in session mode should also work, untried). obop refuses to start in this mode without it, or with the pooler's own address in it, and connects both ways at start-up and refuses to start if they do not reach the same server and database (the name may differ: the pooler may alias it), because locks taken in the wrong database exclude nobody. What cannot be detected is a lock address that is another pooler in transaction mode in front of the same database; do not do that. The connection settings of D66 (`sslmode`, `sslrootcert`) are read from each URL separately. PgBouncer settings that matter: `pool_mode = transaction`; `default_pool_size` at least as large as the number of connections obop may use at once, or requests queue inside PgBouncer where obop's timeouts do not see them; `ignore_startup_parameters` needs nothing for obop (it sends `application_name`, `client_encoding` and the standard ones); `server_idle_timeout` below your firewall's idle limit. `OBOP_DB_ACQUIRE_TIMEOUT_MS` also covers the time PgBouncer takes to accept a new client connection. PostgreSQL's `idle_in_transaction_session_timeout` on the role should stay above `OBOP_DB_IDLE_IN_TRANSACTION_MS` or off.

**Limits.** The lock pool is capped at 8 connections per process (the requirement says at most 8), and a lock-holding request holds one for its whole duration (an MCP request keeps its session lock until it finishes): a ninth concurrent lock-holder in one process waits the acquire timeout and gets `storage_busy`. Per-person fairness is not built: one person with many parallel requests can occupy the pool; run a reverse proxy with connection and request limits in front of a public instance. Requests beyond the runtime's blocking-thread limit queue in the runtime before they reach the pool.

**Not tried:** a managed PostgreSQL service or pooler, PgBouncer in session mode or any version other than 1.26.1, PostgreSQL 18, Linux, a network partition that drops packets silently (obop sets TCP keep-alive of 60 s and a user timeout of 30 s on its connections, which helps on Linux only), more than one hour of continuous load, a sustained 503 storm of minutes. Migrations, `obop backup` and `obop restore` run through the same pools; if you use a transaction-mode pooler, point those commands at the direct address with `--db` (and see the note on the lock variables above).

## Health and metrics (D67)

Three read-only routes on every instance, served next to the API (so a `--static` backoffice or the public site never hides them):

| Route | Answers | Use |
|---|---|---|
| `GET /healthz` | 200 `{"status":"ok"}` while the process serves requests. No database, no login. | liveness probe: restart the container when it fails |
| `GET /readyz` | 200 `{"status":"ready"}` when the database answers and its schema is one this build serves; else 503 `{"status":"unavailable","reason":"..."}` with one of `database unreachable`, `database did not answer in time`, `database is newer than this server`, `database needs migration`. No login. | readiness probe: take the process out of the load balancer, do not restart |
| `GET /metrics` | Prometheus text. **Closed unless you open it** (below). | scrape every 15 to 60 seconds |

`/healthz` and `/readyz` are open on purpose and reveal nothing but those words (no versions, host names or driver messages); `/readyz` asks the database at most once every two seconds however often it is called. With several processes on one database each process answers for itself.

**`/metrics` access.** Set `OBOP_METRICS_TOKEN` and scrape with the bearer token (Prometheus: `authorization: { type: Bearer, credentials: ... }` in `scrape_configs`). Without a token the route exists only when `obop serve` is bound to a loopback address, and then only a direct loopback caller that sends no `X-Forwarded-*`, `Forwarded`, `X-Real-IP` or `Via` header gets it; **if a reverse proxy on the same host strips those headers, set a token**. On any other bind (a container on `0.0.0.0`) without a token it answers 404. Series (no per-path, per-user or per-model labels):

| Series | Meaning |
|---|---|
| `obop_build_info{core_version}` | always 1 |
| `obop_process_uptime_seconds` | since start |
| `obop_http_requests_total{class="2xx"...}` | responses by status class, every route including the public site |
| `obop_http_request_duration_seconds` | histogram, 5 ms to 10 s, until the response headers are ready |
| `obop_db_up`, `obop_db_ping_seconds` | a database ping at scrape time (a ping refused only because the pool is full counts as up) |
| `obop_db_pool_max_connections{pool}`, `obop_db_pool_in_use{pool}`, `obop_db_pool_idle{pool}`, `obop_db_pool_waiting{pool}` | the connection pools (D74; `pool` is `main`, `lock` or `blob`): size, checked out, open and free, callers waiting right now. Read from the pool itself, so they work when it is full |
| `obop_db_pool_acquire_seconds{pool}` | histogram, 0.5 ms to 2.5 s: how long getting a connection took, refusals included |
| `obop_db_pool_acquire_timeouts_total{pool}`, `obop_db_pool_acquire_rejected_total{pool}` | callers refused after the whole acquire timeout / at once because the wait queue was full (both are `storage_busy`) |
| `obop_db_pool_connect_failures_total{pool}`, `obop_db_pool_connections_opened_total{pool}`, `obop_db_pool_connections_closed_total{pool}` | connection churn |
| `obop_db_statement_timeouts_total` | statements cancelled by their time limit |
| `obop_storage_faults_total{fault="busy"\|"unavailable"\|"timeout",answer="503"\|"kept"}` | requests that hit a storage fault: answered 503 (nothing had been changed) or `kept` (a change had already committed, so the request kept its own answer; see "What a 503 means") |
| `obop_active_sessions` | people's valid sign-in sessions (read at most every 15 seconds) |
| `obop_job_queue_depth{queue="connectors",state}` | connector deliveries waiting, leased, dead (read at most every 15 seconds) |

Counters are per process and restart with it; sum them across processes in the scraper. The text format is written by hand to the exposition format and has not been checked with a real Prometheus. The control plane (`platform serve`) has no such routes yet. The upgrade check of `obop platform upgrade-instances` still asks `/api/openapi.json` so that it also works for an image that predates `/readyz`.

## Browser clients and allowed origins (D68)

By default only pages served from the instance itself can call `/api` from a browser (same origin); scripts, agents and servers are not affected by browsers' cross-origin rules. To let a browser application on **another** origin call the API, list that origin:

```text
OBOP_ALLOWED_ORIGINS=https://app.example.com,http://localhost:5173
```

- **Exact origins only:** scheme, host and optional port, nothing else. `https` for every real host; plain `http` only for `localhost`, `127.0.0.1`, `[::1]` and `*.localhost`. **Wildcards are refused** (`*`, `https://*.example.com`), as are `null`, paths, user names and bad ports: the server prints `OBOP_ALLOWED_ORIGINS has an invalid entry "...": reason` and exits before it listens.
- **What a listed origin gets**, on paths under `/api/`: the answer carries `Access-Control-Allow-Origin` with that origin, `Access-Control-Expose-Headers: Link, Retry-After, WWW-Authenticate`, and every `/api/` response carries `Vary: Origin`. A preflight (`OPTIONS`) is answered 204 for `GET` and `POST` with the request headers `Authorization`, `Content-Type`, `Accept` and `Accept-Language`, and a max-age of 600 seconds. Anything else (an unlisted origin, another method or header) is refused with 403 and no CORS headers. An unlisted origin's ordinary requests get no CORS headers, so its page cannot read the answer.
- **Bearer tokens only.** The API has no cookies and `Access-Control-Allow-Credentials` is never sent, so a page cannot use the visitor's signed-in browser against the API; the page must hold a token (a session token from `/api/auth/login` or an agent token) and send it in `Authorization`. A listed origin is trusted with its tokens: keep that application free of script injection.
- `/mcp` uses the same list (and, as before, the instance's own public URL and same-origin on loopback). The OAuth sign-in and consent pages and the OAuth token endpoints are not changed by this setting.
- Set the headers in one place: a reverse proxy that adds its own CORS headers as well produces duplicates that browsers reject.
- **Not tried in a real browser or behind a CDN**; verified with raw HTTP requests and the Fetch standard's rules.

## More than one process

Several `obop serve` (or `obop platform serve`) processes may share one **PostgreSQL** database, behind any load balancer, without sticky sessions. Give them the same `OBOP_SECRET_KEY` (and `OBOP_SECRET_KEY_PREVIOUS`) and the same `OBOP_DATABASE_URL`.

What is shared through the database, and how:

| State | How it is shared |
|---|---|
| Sign-in sessions and agent tokens | Records (hashed tokens), as always. Revoking on one process revokes on all. |
| Mask-token vaults of REST sessions | Sealed, expiring records keyed by a hash of the bearer token. An agent's call holds a database lock on its vault for its duration. A vault is forgotten 24 hours after its last change. |
| MCP sessions (`Mcp-Session-Id`) | Sealed, expiring records; any process serves any session; a session belongs to the token that opened it. Closing on one process closes it everywhere. |
| Proposed approval plans | Sealed records kept 7 days; any process can approve a plan any process proposed; a plan is approved once. |
| Login lockout, second-factor attempts | Atomic counters in the database: five wrong passwords across any processes lock the account. |
| Rate limits (MCP, public site, checkout, webhooks) | Fixed-window counters in the database, counted on the database's clock. (The forum's posting limit is computed from the stored posts, so it is shared too.) |
| Installed modules | The packages are records; every install, upgrade or removal bumps a shared version number, and each process compares it (at most once a second, whenever it handles a call or builds the metadata) and registers, replaces or unregisters the commands to match. A module installed through one process is on the others within about a second. |
| Order idempotency keys | Put-if-absent records, so a double submit that lands on two processes makes one order. |
| Governed writes | One database lock around the guard and the write of every mutating call, so two processes cannot race between check and write. |
| Forum and status page | The database is the truth; each process keeps a copy that it reloads when a shared version number moves (checked at most every 250 ms). A post made on one process shows on the others within that. Writes take a database lock first, so ids are never handed out twice. One process per interval runs the status checks. |
| Connector retry queue and dead letters | Rows with a lease: a worker claims due deliveries for 120 seconds; if it dies, another takes them. Delivery is at-least-once, and the delivery id is stable, so a receiver can deduplicate. Due pulls and the daily purge run once across processes. |
| OAuth sign-in (D64): clients, grants, refresh tokens, codes, flows | Registered clients, grants and refresh tokens are records (hashed refresh tokens, so a rotation is a compare-and-set and a reused token is seen by whichever process gets it); codes (60 s) and sign-in flows (10 min) are expiring values, spent by whoever deletes them first; registration, authorize, sign-in and token rate limits are counters. Revoking on one process revokes on all. |

What is *not* shared: small per-process caches (site settings for a few seconds, the forum and status copies between version checks). SQLite supports several processes on **one machine** too (file locks next to the database), but not on a network file system; use PostgreSQL to scale out.

The locks are PostgreSQL session advisory locks (released by the server when a process dies) or, for SQLite, operating-system file locks named `<database file>.lock-<key>` (harmless to leave behind).

Test it: `OBOP_TEST_POSTGRES_URL=postgres://... cargo test -p obop --test multi_process` starts two real servers on a scratch database and checks that a login on one works on the other, an MCP session continues across them, a plan proposed on one is approved on the other, parallel writes all land once, and rate limits and the lockout are counted across both.

## Connecting Claude Desktop, claude.ai and Claude Code (D64)

An instance is its own OAuth authorization server for its `/mcp` endpoint, so MCP clients that sign in through a browser (the custom connector of the Claude apps, `/mcp` in Claude Code) connect without a pasted token. There is nothing to switch on; what the operator owns is the address and the proxy.

- **Set `OBOP_PUBLIC_URL`** (for example `https://acme.example.com`) on every instance behind a proxy. It is the issuer, the base of every endpoint and the resource a token is bound to (`<OBOP_PUBLIC_URL>/mcp`); without it the instance derives them from the `Host` header and `X-Forwarded-Proto`, which is fine on a laptop and wrong behind a proxy that rewrites the host. Set `OBOP_TRUST_PROXY=1` when a proxy you control sets `X-Forwarded-For`, or every client shares the proxy's address in the registration, sign-in and token rate limits.
- **Forward these paths**: `/mcp`, `/oauth/*` (`register`, `authorize`, `token`, `revoke`), `/.well-known/oauth-authorization-server`, `/.well-known/oauth-protected-resource` (and its `/mcp` variant) and `/.well-known/mcp/server-card.json`. claude.ai connects from Anthropic's servers, so the instance must be reachable from the internet over `https`; a local instance works with Claude Code and with clients on the same machine through a loopback redirect.
- **What a person sees**: the instance's own sign-in (with the two-factor step when the account has it), then a consent page that names the application and where the browser returns to, lists the groups the person holds, and asks how long the access lasts (1, 7, 30 or 90 days). An account that must still enrol two-factor, or must change a temporary password, is sent to the back office first.
- **What the token is**: a scoped agent token (D18) of one hour, bound to `<public URL>/mcp`: it works on `/mcp` and is refused on `/api`. It can hold only groups the person holds and never more; approvals still wait for a person. The refresh token is single use; presenting a used one, or a used code, ends the whole connection and writes `oauth_refresh_reuse` or `oauth_code_reuse` to the security log.
- **Managing it**: every person lists and revokes their own connected apps under Customize, Security, Connected apps (`GET /api/auth/oauth/grants`, `POST /api/auth/oauth/grants/{id}/revoke`); `oauth_authorize`, `oauth_denied`, `oauth_revoked` and the reuse events are in `security_event`. Disabling an account stops its apps at once (its tokens stop resolving). A person's password change does not end them, as with other agent tokens; revoke them in the list.
- **Registration is open**, as MCP clients expect (RFC 7591), and therefore limited: public clients only, 5 redirect URIs of `https` (or `http` on `localhost`, `127.0.0.1`, `[::1]`; no wildcards), 20 registrations per address and hour and 300 in all, at most 2000 clients, a client that never completed a connection is forgotten after a day and an idle one after 180 days. Registration is not authentication: the consent page says that obop cannot verify who runs an application.
- **Nothing here needs `OBOP_SECRET_KEY`**: codes, refresh tokens and the binding of a sign-in are stored as SHA-256 hashes, so `obop secrets rotate` has nothing to re-seal for it. Clients, grants and refresh records are in `obop backup`; codes and sign-in flows (seconds to minutes) are not.
- **Not tested against the real Claude apps**; see docs/ISSUES.md E1.

## The server key

`OBOP_SECRET_KEY` seals four kinds of secret: two-factor seeds, connection credentials and OAuth tokens (and the keys behind pseudonyms), and the transient sessions, vaults and plans above. Every sealed value names the key that sealed it (an id: the first eight hex digits of a SHA-256, which says nothing usable about the key), so opening picks the right key at once and a missing key is reported by id, never by content.

- **Do not lose it.** Without it enrolled people cannot use their authenticator codes until an operator runs `obop admin reset-mfa USER`, and connection credentials must be entered again. Keep it in the place where you keep other production secrets, apart from backups.
- A generated key (no `OBOP_SECRET_KEY` set) lives in the database and therefore in every backup. That is fine for evaluation, not for production. When you later set `OBOP_SECRET_KEY`, the stored key stays in the ring as a previous key until you rotate.

### Rotating the key (runbook)

Rotation needs no downtime and can be done in steps. Rehearse it on a restored copy first (`deploy/restore-drill.sh`).

1. **Back up**, and verify: `obop backup --db URL --out DIR`, `obop backup verify FILE`.
2. **Make a new key:** `openssl rand -hex 32`.
3. **Deploy with both keys:** `OBOP_SECRET_KEY=<new>` and `OBOP_SECRET_KEY_PREVIOUS=<old>` on every process. From now on new values are sealed with the new key and everything the old key sealed still opens. Check with `obop secrets status --db URL` (set the same variables): it lists the current and previous key ids and how many values each protects, without printing any key.
4. **Re-seal:** `obop secrets rotate --db URL` with the same variables. Add `--dry-run` first for the counts. It re-seals in batches (`--batch N`, default 200), each batch one transaction of compare-and-set swaps, so a value the running server changed in the meantime is left alone and picked up by the next run. `--max-batches N` stops early (rotate gently, resume later). Re-running is always safe and finishes the job.
5. **Remove the old key** from `OBOP_SECRET_KEY_PREVIOUS` at the next deploy, once the report says everything is under the current key (`obop secrets status` shows only the current id).

Rules the tool follows: it never prints, logs or returns a key, a plaintext or a sealed value, only places (`connection/c1.credentials`) and key ids. **A value that no key of the ring opens stops the run before anything is written** and names the first one; put the missing key in `OBOP_SECRET_KEY_PREVIOUS`, or accept losing those values with `--skip-unopenable`. It refuses to run without `OBOP_SECRET_KEY` set. The stored pseudonym key of every connection is made to exist before re-sealing, so pseudonyms already given to receivers do not change.

Secrets from before the key ring (formats `v1.` and `obs1:`) are opened with every key in the ring and re-sealed by `obop migrate run` (step 2) or by a rotation.

### If the key is lost

There is no backdoor: sealed values cannot be opened without the key. Recover in this order:

1. Look for the key in backups of your secret store, in earlier deploy configurations and in any process still running with it (`/proc/<pid>/environ` of a live server, a running container's environment). A live server still holds it.
2. If a database from before the loss has a generated key (`connector_key`), that one is in the database.
3. Otherwise: set a new `OBOP_SECRET_KEY`; run `obop secrets rotate --skip-unopenable` to see what is lost (the report names each place); reset two-factor for the affected people (`obop admin reset-mfa USER --db URL`, which revokes their sessions and writes an audit record); enter connection credentials again and reconnect OAuth accounts. Passwords, records and everything that is not a sealed secret are unaffected.

### If a key leaks

Treat it as the loss of confidentiality of every secret sealed under it: rotate as above (the old key goes into `OBOP_SECRET_KEY_PREVIOUS` only for the time the run takes), then change the secrets themselves where they matter (rotate the third-party API keys that connections hold, and have people re-enrol two-factor).

## Backups

`obop backup` writes a consistent logical copy of every record: gzip-compressed JSON Lines plus a manifest with checksums, the schema version and the core version. It reads from one snapshot (a repeatable-read transaction on PostgreSQL, a read transaction on SQLite), a batch at a time, so a large database never sits in memory and a running server is not stopped. Uploaded pictures and files (D60) follow the records as `blob` lines, one at a time, wherever their bytes live (`OBOP_BLOB_DIR`, a `<file>.blobs` directory beside a SQLite file, or the database); `obop restore` puts them back and checks that each hashes to its id, so a restore onto a new host brings the pictures too. Set the same `OBOP_BLOB_DIR` for backup and restore.

```bash
obop backup --db postgres://... --out /backups/            # a timestamped file in the directory
obop backup --db postgres://... --out /backups/nightly.jsonl.gz
obop backup verify /backups/nightly.jsonl.gz               # checksums, gzip, format, counts
obop restore --db postgres://.../new --from FILE --dry-run # check, report, write nothing
obop restore --db postgres://.../new --from FILE           # into an EMPTY database
obop restore --db postgres://.../live --from FILE --force  # replace what is there
```

- **What is in it:** every record, in insertion order (including the schema version record and the generated key if the instance has no `OBOP_SECRET_KEY`). **What is not:** sessions, pending approval plans and rate-limit counters (people sign in again; agents propose again). **Secrets stay sealed exactly as stored**; the backup is as safe as `OBOP_SECRET_KEY`, which is not in it. Store the key apart from the backups.
- **Verification** recomputes the checksum of the file as stored, then reads every record (gzip integrity, shape, counts per model, a checksum over the records). A truncated, edited or damaged file fails. The checksums detect damage; they do not authenticate: keep backups where only the operator can write.
- **Restore** refuses a non-empty target unless `--force`; checks the format (an unknown format is refused), refuses a backup from a newer schema than the binary knows, and writes in one transaction, so a failure leaves the target as it was. A backup from an older schema restores and tells you to run `obop migrate run`. `--force` also clears the target's sessions and counters.
- SQLite and PostgreSQL are both supported, and a backup moves between them (it is how you move a local database to PostgreSQL).
- **Scheduling:** `obop backup` is a command; run it from cron, a systemd timer or a scheduled job of your platform, and ship the file to storage you control. Scheduled backups for hosted instances are not part of the product yet (they need object storage); operate them as below.

### The restore drill

`deploy/restore-drill.sh BACKUP` restores a backup into a scratch PostgreSQL database, checks it and drops it again:

```bash
DRILL_ADMIN_URL=postgres://postgres:PASSWORD@HOST:5432/postgres \
OBOP_SECRET_KEY=... deploy/restore-drill.sh /backups/nightly.jsonl.gz
```

It verifies the file, creates `obop_drill_<time>`, restores, reads the schema version (`DRILL_MIGRATE=1` also drills the migration of an older backup), counts the restored records against the manifest, checks that user accounts came back and that no sealed secret is unopenable with the keys given, starts a server on the copy and asks it `GET /api/openapi.json`, then drops the database. It exits non-zero on the first failed check. CI runs it against a PostgreSQL service on every change; run it on a schedule against your real backups (monthly is the minimum; the pipeline plan lists it as a monthly job, ENGINEERING 3.1).

### PostgreSQL itself (the Sliplane cell)

`obop backup` is the portable, version-independent copy. A database server also needs its own, for point-in-time recovery and for losing the volume. With standard PostgreSQL tooling (these are the usual commands; they are not run against the Sliplane service from this repository, so try them once):

```bash
# one business: a logical dump with pg_dump (run from any host or job that reaches obop-postgres)
pg_dump --format=custom --no-owner "postgres://obop_acme:PASSWORD@obop-postgres.internal:5432/obop_acme" -f acme.dump
createdb obop_drill && pg_restore --no-owner -d obop_drill acme.dump   # the drill of a pg_dump
# the whole cell, roles included
pg_dumpall --globals-only -h obop-postgres.internal -U postgres > globals.sql
```

For point-in-time recovery run WAL archiving: set `archive_mode = on` and an `archive_command` (or use `pgBackRest` / `wal-g`) that copies segments to storage outside the server's volume, take a base backup regularly, and rehearse a recovery to a new server. Sliplane's volume snapshots and PostgreSQL backup guarantees are unverified (D43); do not count on them until you have restored from one. A scheduled job service in the same Sliplane project (the `postgres:18` image running `pg_dump` to an object store) is the straightforward way to run the commands above on a timer.

Whichever you use: a copy you have not restored is not a backup. After restoring a dump, `obop migrate status --db URL` and `deploy/restore-drill.sh`'s server check tell you whether the database is whole.

## Upgrades

### Schema versions

A database carries its schema version in one record (`obop_meta/schema`). Today that is the JSON-document layout plus the shapes the modules store in it (version 1 is the layout of today, step 2 re-seals secrets from before the key ring). Each change to a stored shape is a numbered, ordered, idempotent step in `obop-migrate`; a step either returns its writes, which are applied together with its version record in **one transaction**, or does its own batched, resumable writes.

```bash
obop migrate status --db URL     # the version in the database, the version this build writes, pending steps
obop migrate plan --db URL       # what each pending step would do (same as run --dry-run)
obop migrate run --db URL        # apply them, in order, under a database lock (two runners apply each step once)
```

- A fresh, empty database is stamped with the current version. A database from before versioning counts as version 0.
- **A server refuses to run against a newer schema than it knows** ("run a newer obop, or restore a backup taken before the upgrade"). A downgrade cannot be done safely, so there is none.
- When pending steps exist, a starting server runs them only with `--auto-migrate` (default **on for SQLite, off for PostgreSQL**); otherwise it prints the exact command and exits. Hosted instances set `OBOP_AUTO_MIGRATE=1` (one process per instance); the control plane's own SQLite database upgrades itself.

### The compatibility promise

While the core is below 1.0 a minor version may break (D55). Within that: (1) a build never reads a database newer than itself and says so; (2) a build upgrades any database of an older schema, step by step, without data loss, and a step that fails changes nothing of itself; (3) migrations are forward-only, so the way back from an upgrade is the backup you took before it; (4) every backup restores into a build of the same or a newer schema and is migrated on the way. From 1.0 the destructive-change rules of ENGINEERING 4.3 apply.

### Procedure (self-hosted)

1. `obop backup` and `obop backup verify`; keep the file.
2. Read the release notes; `obop migrate plan --db URL` with the **new** binary shows the steps.
3. Stop the old processes (or, with several processes and only additive steps, roll them one by one: old processes keep running until they meet a schema they do not know, so finish the migration quickly).
4. `obop migrate run --db URL` with the new binary, then start the new processes.
5. Roll back by stopping, restoring the verified backup with the **old** binary (`obop restore --force`) and starting it.

### Procedure (Sliplane instances)

See "The published image": build, then `obop platform upgrade-instances --image <commit>`. Hosted instances run with `OBOP_AUTO_MIGRATE=1`, so the redeploy migrates. Take a backup of the cell (or of the instance with `obop backup`) before a release that has migration steps; the command stops at the first instance that fails its health check and names it.

## The published image

`.github/workflows/image.yml` builds the root `Dockerfile` for `linux/amd64` on every push to `main` or `deploy`, with the BuildKit layer cache (the Rust dependencies are a separate, cached `cargo-chef` layer, so a source-only change rebuilds only the workspace) and publishes it to GitHub Container Registry as

- `ghcr.io/spacecorps/obop:<git commit sha>` (immutable: what the control plane deploys),
- `ghcr.io/spacecorps/obop:main` or `:deploy` (moving).

The build attaches SLSA provenance (`mode=max`) and an SBOM as attestations, and the workflow runs `obop --version` from the pushed image. It authenticates with the repository's `GITHUB_TOKEN` (`packages: write`); no other credential exists. Actions are pinned by commit SHA. arm64 is not built: under emulation the Rust build takes hours; add a matrix job on a native arm64 runner and merge the manifests when it is needed. The workflow has never been run from this repository's history (it needs GitHub); the first run is the test, so watch it.

Build the image locally with `docker build -t obop:local .`. `docker build --target web .` builds only the website and back office (it needs `Cargo.toml` in the build context for the core version, which the Dockerfile copies).

### Switching the control plane from repository builds to the image

Today the control plane runs with `OBOP_REPO` and Sliplane builds the Rust workspace on `spacecorps-main` for every instance: slow, and it competes with the other services. With an image there is no build.

1. **Make the package pullable.** Either set the GHCR package `ghcr.io/spacecorps/obop` to public (Package settings, Change visibility), or keep it private and give Sliplane a read credential: create a GitHub personal access token with `read:packages`, register it with Sliplane (`sliplane credentials create --name ghcr-obop --type ghcr --username <github user> --token <the token> -a spacecorps`; `sliplane credentials list` shows the id), and note the credential id.
2. **Pick a commit tag** that the workflow published, for example `ghcr.io/spacecorps/obop:<sha>` (the Actions summary prints it).
3. **Set on the control-plane service** (`obop-platform`): `OBOP_IMAGE=ghcr.io/spacecorps/obop:<sha>` and, for a private package, `OBOP_REGISTRY_CREDENTIAL=<credential id>`; remove `OBOP_REPO` and `OBOP_BRANCH` (when both are set the image wins). Redeploy the control plane. New instances are created from the image (`services create --image ... --registry-credential-id ...`), start with `OBOP_AUTO_MIGRATE=1`, and record the image they run.
4. **Existing instances** were created from the repository. `obop platform upgrade-instances` moves a service to an image with `sliplane services update --image` and redeploys it; whether Sliplane accepts changing a repository-based service into an image-based one has not been verified here. Try it on one instance first (`--only SLUG`); if it is refused, delete and recreate that instance (its database is separate, so recreate only the service and keep the secrets: `OBOP_SECRET_KEY` must be set again to the same value, see below).

### Rolling out an upgrade

```bash
# on the control plane host (same settings as platform serve: SLIPLANE_*, OBOP_*, the sliplane CLI registered)
obop platform upgrade-instances --image <commit tag> --dry-run          # who would move
obop platform upgrade-instances --image <commit tag> --only <slug>      # one instance first
obop platform upgrade-instances --image <commit tag>                    # all, oldest first
```

`--image` takes a bare tag (resolved against the repository of `OBOP_IMAGE`) or a full reference. For each **running** instance, in creation order, it checks that the Sliplane service is the instance's own (by name), points it at the image (`services update --image`, merging the environment: no secret is dropped; adds `OBOP_AUTO_MIGRATE=1`), triggers `services deploy`, waits for the service to be live, checks that it reports the requested image when it says which it runs, and then asks the instance itself: `GET https://<domain>/api/openapi.json` must answer 200 (a version that cannot open its database, or whose migration failed, never listens). Only then does it start the next. **The first failure stops the run**: the rest are untouched and reported as not attempted, and the failed instance keeps its error in the control plane's record. Fix the cause and run the same command again; instances already on the image are skipped.

To go back for one instance: `obop platform upgrade-instances --image <old tag> --only <slug>`. A schema migration that already ran is not undone; if the older build then refuses the database ("newer than this build"), restore the instance's backup. Take one first.

`OBOP_UPGRADE_SETTLE_SECS` (default 30) is how long to wait after the deploy before asking whether the service is live, because a service reports the old deployment as live for a moment.

## Sliplane

The SpaceCorps Sliplane account is `spacecorps`. The `obop` project (`project_2x9d8q51m9lt`) holds, on `spacecorps-main`:

| Service | What | Network |
|---|---|---|
| `obop-postgres` | The `postgres:18` Docker image with volume `obop-pgdata`. The cell: one database and one role per business (D6). The superuser password lives in a secret env and in `~/.config/obop/cell.env` on the machine that created it. | internal only, `obop-postgres.internal:5432` |
| `obop-platform` | The control plane, from the root `Dockerfile` (a repository build of the `deploy` branch today; the published image once switched, above); its own database is SQLite on volume `obop-platform-data`. | public, `obop-platform.sliplane.app` |
| `obop-<slug>` | One per customer instance, created by the control plane, with auto-deploy off. | public, `*.sliplane.app` |

The control plane provisions through the `sliplane` CLI bundled in the image (v1.0.0, pinned by checksum). **It needs one secret you must set**: `SLIPLANE_API_KEY`, a Sliplane API key with write access. The entrypoint registers it under `SLIPLANE_ACCOUNT` (a plaintext 0600 store inside the container, as the CLI allows) and then drops it from the environment. Without the key the control plane still serves signup and sign-in, but creating an instance fails with a clear error.

```bash
sliplane services set-env --project-id project_2x9d8q51m9lt --service-id <obop-platform id> -a spacecorps --key SLIPLANE_API_KEY --value <key> --secret
sliplane services deploy --project-id project_2x9d8q51m9lt --service-id <obop-platform id> -a spacecorps
```

The Sliplane provisioner generates a fresh random `OBOP_SECRET_KEY` for each instance and passes it with `--secret-env` when it creates the service; the control plane does not store it, so it exists only in that service's secret environment. **If you recreate an instance's service by hand, set the same key again** (`sliplane services set-env ... --key OBOP_SECRET_KEY --value <hex> --secret`) or enrolled people lose their authenticator. Keep a copy of each instance's key in your secret store (read it from the Sliplane service environment). The control plane itself (`obop-platform`) needs its own `OBOP_SECRET_KEY` secret for its optional account two-factor and for its sessions; without it the Security button reports that two-factor is not available. The `local` provisioner passes the environment of the control plane to its child processes, so one key serves them all on a developer machine.

Settings read by `obop platform serve --provisioner sliplane`: `SLIPLANE_ACCOUNT`, `OBOP_SLIPLANE_PROJECT`, `OBOP_SLIPLANE_SERVER`, `OBOP_CELL_ADMIN_URL` (the superuser URL of the cell, as the control plane reaches it), `OBOP_CELL_INSTANCE_HOST` (`host:port` instances use for the cell, `obop-postgres.internal:5432`), `OBOP_IMAGE` or `OBOP_REPO` (+ `OBOP_BRANCH`), `OBOP_REGISTRY_CREDENTIAL`, `OBOP_MARKETPLACE_URL` (see Configuration).

## Two-factor authentication and accounts (D53, B7)

Every instance and the control plane need `OBOP_SECRET_KEY` to protect the two-factor secrets (TOTP seeds) stored in their database. Without it nobody can turn two-factor on, and the "require for administrators" policy is not applied (a policy nobody could satisfy would lock the admins out).

**Break-glass.** A person who lost the device and the recovery codes is reset by an administrator in Customize > Security, or, when no administrator can sign in, by the operator with database access:

```bash
obop admin reset-mfa alice --db postgres://...      # or sqlite://obop.db
```

It removes the second factor (the authenticator app **and every passkey**, and the passkey sign-in setting), revokes the person's sessions and writes an `operator` record to the security log (`security_event`). The next sign-in is password-only, and if the policy covers the person they are asked to set two-factor up again.

**Passkeys (D65).** Passkeys (WebAuthn) are a second factor that satisfies the same policy as an authenticator app, and, when a person allows it, a way to sign in without the password. They need no `OBOP_SECRET_KEY` (the database holds only public keys, names and counters) and no other setting, with these conditions:

- **The address.** The browser binds every passkey to the host name of the instance, and the server derives the relying party id and the origin itself: `OBOP_PUBLIC_URL` if set; otherwise, with `--trust-proxy` (`OBOP_TRUST_PROXY=1`), the proxy's `X-Forwarded-Host` and `X-Forwarded-Proto`; otherwise `Host` and `X-Forwarded-Proto`. **Set `OBOP_PUBLIC_URL` to the address people type, over https**, and forward `/api/auth/*` and `/oauth/*` unchanged. An IP address and plain http (except `localhost`) cannot hold passkeys; the Security page says so and the API answers `503 passkeys_unavailable`.
- **Changing the address.** Passkeys made for the old host name stop working (passwords and authenticator apps are unaffected); people add new ones. There is no related-origins support. Moving a database to another host name needs the same.
- **The proxy.** Do not let a proxy rewrite `Host` without `OBOP_PUBLIC_URL`; the origin check would then fail for everyone, and nothing is accepted that was made for a different address.
- **Several processes.** A ceremony may start on one server process and finish on another: challenges (two minutes, single use) and the short-lived proofs live in the shared store (D59), and the rate limits are counters there.
- **Backups.** `auth_passkey` is in every backup and restore like the other tables; it holds no secret.
- **Lost passkeys.** Recovery codes, an administrator's reset in Customize > Security, or `obop admin reset-mfa` above. The log (`security_event`) records `passkey_register`, `passkey_rename`, `passkey_remove`, `passkey_used`, `passkey_login`, `passkey_failed`, `passkey_clone` (a counter that went backwards) and `passkey_passwordless`.
- **Not verified on real devices.** The implementation has been exercised with a software authenticator and Chrome's virtual authenticator only. Try a real phone, platform authenticator and security key on your instance before you require passkeys of administrators, and keep an authenticator app or the break-glass command at hand.

**Accounts, sessions and passwords.** In Customize > Security everyone can change their own password (the current password, and a code when two-factor is on), see their sessions and sign out the others or everywhere. Turning two-factor off or changing the password signs out the person's other sessions. Administrators see the people of the instance, disable and enable accounts (a disabled account's sessions end at once and its agent tokens stop working until it is enabled again; the last enabled administrator cannot be disabled) and set a temporary password (the person must choose a new one at the next sign-in; their sessions end). The equivalent API: `GET /api/auth/sessions`, `POST /api/auth/sessions/revoke-all`, `POST /api/auth/password`, `GET /api/auth/users`, `POST /api/auth/users/{name}/disable|enable|reset-password` (administrators). Every one of these, and every change to the `security.*` instance settings (who, old value, new value; values that look like secrets are never written), is a row in the security log.

**People administration (D69).** Administrators also create people, edit their permission groups and offboard them, in Customize > Security > People, over REST, as proposals an agent can make (MCP), and from the command line:

```bash
obop admin users                                    # accounts, status, two-factor, sessions, groups
obop admin groups alice                             # show alice's groups
obop admin groups alice --groups crm.user,sales.user  # replace them (checked against the registered groups)
obop admin offboard alice --note "left the company" # disable alice and end every credential for good
obop admin enable alice                             # the person may sign in again; revoked credentials stay revoked
obop admin import users.csv --dry-run               # name,groups ; check every row, create nothing
obop admin import users.csv --out passwords.csv     # create them; temporary passwords go to a file only you can read
```

- **Groups.** The groups an account can be given are the ones the installed models, fields and commands name (`GET /api/auth/groups`); anything else is refused, except that a group the person already holds may stay when its module is gone. A change applies to the person's open sessions and agent tokens at their next request (nothing is revoked); the log row `user_groups` records who, and the groups added, removed and now held. Nobody can take `platform.admin` from themselves, and the last enabled administrator keeps it. If the instance requires two-factor for administrators and the person has none, a new administrator's open sessions are limited to setting it up at once.
- **Creating a person.** REST `POST /api/auth/users` with `{"username","groups","password"?}`: with a password the person must still choose their own at the first sign-in (`must_change_password: false` opts out); without one the server returns a random temporary password, once. It is never written to the log. An agent can only propose `user_account.add` with a password a person chose.
- **Offboarding.** `POST /api/auth/users/{name}/offboard` (or the button) disables the account and revokes its sessions, agent tokens, connected apps (OAuth grants and their refresh tokens) and the passkey proof or half-finished sign-in it had. Enabling the account later lets the person sign in; none of those credentials comes back. Not yourself, not the last enabled administrator. Anonymising or erasing the person's display data is not built (issue I-1 in docs/ISSUES.md).
- **Import.** `obop admin import` reads a UTF-8 CSV (a header with `name` and optionally `groups`; `,` or `;`; at most 1,000 rows and 1 MiB), checks every row first and creates all accounts in one transaction or none. Each gets a generated temporary password to change at the first sign-in, printed once or written with `--out FILE` (created with mode 0600, never overwritten).
- **Who may.** A signed-in administrator whose session is not limited to two-factor enrolment; the operator with database access (the CLI); agents only propose (an administrator approves, and the proposal is checked again against the administrator's stored account when it runs). No step-up re-authentication is asked for these actions, as for the other administrator actions (issue E7f).
- **Not tried** against a multi-process PostgreSQL deployment under concurrent edits (the read-modify-write sits under the same cross-process lock as two-factor changes), or with real OAuth clients (only our own, in Rust).

## Known limits

- **TLS** terminates at Sliplane's proxy and the instance speaks plain HTTP behind it. Confirm before relying on it. The control plane reaches the cell over the internal network without TLS.
- **Scheduled backups of hosted instances** are not built (they need object storage); run `obop backup` or `pg_dump` from a job (see "Backups").
- **Instances created before D53** have no `OBOP_SECRET_KEY` unless an operator set one; without it a generated key lives in their database.
- **Migrations** of the core exist for the two steps above; a newer version of a package migrates its records only through the `[[migration]]` steps it declares (D63).
- **Updating from inside** has run against the fake provisioner and a loopback control plane, not against live Sliplane; the release feed is unsigned (a digest per release only).
- **The published image** has not been built by GitHub from this history yet, and `upgrade-instances` against live Sliplane has not been run; both are covered by tests against stand-ins (the Docker build, the runner fakes), not against the real services.
