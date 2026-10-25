# Deploying obop: the operator guide

- **Status:** working document, 2026-10-25. Written against the code in the repository, not a design target. Decision records: D59 (operations) and D73 (backups, restore, migration gates) in [DECISIONS.md](DECISIONS.md). Recovery steps and roles: [RUNBOOK-RECOVERY.md](RUNBOOK-RECOVERY.md).
- **Who this is for:** whoever runs obop in production, on their own server or on Sliplane. Everything here is a command you can run.

## What exists

| Piece | Command | What it does |
|---|---|---|
| Instance | `obop serve` | One business: REST, MCP, backoffice. SQLite locally, PostgreSQL when `OBOP_DATABASE_URL` is `postgres://` (D44). **Several processes may serve one PostgreSQL database** (see "More than one process"). |
| Control plane | `obop platform serve` | Sign up, then create, list and delete your own instances; the forum (`/community`) and status page (`/status`). Runs a provisioner per instance. |
| Provisioners | `--provisioner local\|sliplane\|fake` | `local` starts child `obop serve` processes with SQLite; `sliplane` creates a database on a shared cell PostgreSQL and a Sliplane service per instance; `fake` is for tests. |
| Rolling upgrade | `obop platform upgrade-instances` | Moves every running instance to a published image, one at a time (see "The published image"). |
| Backup and restore | `obop backup [run\|list\|prune]`, `obop backup verify`, `obop restore`, `obop drill`, `deploy/restore-drill.sh` | Consistent logical backups, sealed with a key (format 2), scheduled with retention and an off-host copy, authenticated restores, restore drills (see "Backups"). |
| Key rotation | `obop secrets status`, `obop secrets rotate` | Re-seals every secret under a new server key while serving (see "The server key"). |
| Schema | `obop migrate status\|verify\|plan\|run\|rehearse\|rollback` | The schema version, the migration ledger, and the steps that upgrade it behind backup, plan and rehearsal gates (see "Upgrades"). |
| Storage checks | `obop storage check\|attest\|show` | WAL archiving, TLS and evidence checks of a PostgreSQL host; the storage-encryption attestation (see "Backups"). |
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
| `OBOP_BACKUP_DIR` | instance | Where backups go (mount a volume): the backup before an update, before a migration (`--auto-migrate`, `module.upgrade` that rewrites records) and the scheduled ones. Without it an update needs the administrator's confirmation that they have their own backup, and on PostgreSQL a module update that rewrites records is refused (see "Migration gates"). |
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
| `OBOP_SECRET_KEY` | The current server key, 32 random bytes as 64 hex digits (`openssl rand -hex 32`). Seals two-factor secrets and connector credentials, and the sessions and approval plans kept in the database. See "The server key". Without it a key is generated and stored in the database next to the data, which protects nothing against someone holding a copy of the database. |
| `OBOP_SECRET_KEY_PREVIOUS` | Earlier keys, comma separated: what the current key replaced and that may still protect stored values. Only needed between a key change and the end of `obop secrets rotate`. |
| `OBOP_BACKUP_KEY`, `OBOP_BACKUP_KEY_PREVIOUS` | The key backups are sealed with (format 2), 64 hex digits (`openssl rand -hex 32`), and earlier ones, comma separated. Without it the server key seals them; a key generated into the database never does. See "Backups". |
| `OBOP_BACKUP_SCHEDULE`, `OBOP_BACKUP_COPY_DIR`, `OBOP_BACKUP_COPY_DELETE`, `OBOP_BACKUP_KEEP_DAILY\|WEEKLY\|MONTHLY` | Scheduled backups (`24h`), where each is copied and read back, whether retention may delete there, and retention (default 35 daily, 12 monthly). See "Scheduled backups, retention and an off-host copy". |
| `OBOP_MIGRATION_GATES` | `strict` or `relaxed`: whether the checks before a migration or a data-rewriting module update refuse or only warn. Default strict on PostgreSQL, relaxed elsewhere. |
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

`OBOP_TEST_POSTGRES_URL` adds PostgreSQL to `cargo test`, including the multi-process tests; `OBOP_BIN` points the end-to-end suite at a binary.

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
| `obop_db_up`, `obop_db_ping_seconds` | a database ping at scrape time |
| `obop_active_sessions` | people's valid sign-in sessions (read at most every 15 seconds) |
| `obop_job_queue_depth{queue="connectors",state}` | connector deliveries waiting, leased, dead (read at most every 15 seconds) |
| `obop_backup_age_seconds`, `obop_backup_size_bytes`, `obop_backup_duration_seconds`, `obop_backup_copy_age_seconds`, `obop_restore_drill_age_seconds`, `obop_backup_last_run_failed` | the newest verified backup, its copy off the host, the newest passed restore drill and whether the last scheduled run failed (from the backup catalogue; absent until the first one; D73) |
| `obop_wal_archive_lag_seconds`, `obop_wal_archive_failed_total` | on PostgreSQL only: seconds since a WAL segment was last archived and the failed attempts, from `pg_stat_archiver` (read at most every 15 seconds; absent until a segment has been archived) |

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
6. **Backups sealed with the old key** (format 2 without an `OBOP_BACKUP_KEY` of its own, see "Backups") still need it: keep it in `OBOP_SECRET_KEY_PREVIOUS` for as long as any such backup is kept, or give backups a key of their own (`OBOP_BACKUP_KEY`) before you rotate, so that rotating the server key never touches them. `obop backup verify FILE` tells you by key id which key a backup needs.

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

`obop backup` writes a consistent logical copy of every record: JSON Lines (gzip-compressed) plus a manifest with checksums, the schema version and the core version. It reads from one snapshot (a repeatable-read transaction on PostgreSQL, a read transaction on SQLite), a batch at a time, so a large database never sits in memory and a running server is not stopped. Uploaded pictures and files (D60) follow the records as `blob` lines, one at a time, wherever their bytes live (`OBOP_BLOB_DIR`, a `<file>.blobs` directory beside a SQLite file, or the database); `obop restore` puts them back and checks that each hashes to its id, so a restore onto a new host brings the pictures too. Set the same `OBOP_BLOB_DIR` for backup and restore.

```bash
obop backup --db postgres://... --out /backups/            # a timestamped file in the directory
obop backup --db postgres://... --out /backups/nightly.obbk
obop backup verify /backups/nightly.obbk                    # opens and authenticates a sealed backup (needs the key)
obop restore --db postgres://.../new --from FILE --dry-run # check, report, write nothing
obop restore --db postgres://.../new --from FILE           # into an EMPTY database
obop restore --db postgres://.../live --from FILE --force  # replace what is there
```

### Two formats (D73)

| | Format 1 (plain) | Format 2 (sealed) |
|---|---|---|
| Written | when no key is configured, or with `--plain` | when `OBOP_BACKUP_KEY` or `OBOP_SECRET_KEY` is set (the default then); `--encrypted` refuses to write without a key |
| File | `obop-backup-<time>.jsonl.gz` | `obop-backup-<time>.obbk` |
| Records readable by whoever holds the file | yes | no: the stream is encrypted with ChaCha20-Poly1305 in 64 KiB chunks under a random key per backup |
| Detects damage | checksums | every chunk is authenticated, and so are the order, truncation (even at a chunk boundary) and extension |
| Authenticates who wrote it | no (anyone who can rewrite both files can forge a pair) | the manifest carries an HMAC under a subkey of the backup key, and the data file cannot be sealed without it: a data file and manifest both rewritten without the key are refused |
| Restore | needs no key | needs the key; without it `obop restore` and `obop backup verify` stop with the code `restore_key_required`, **exit status 3**, and write nothing |

Format 1 backups, including every backup made before D73, restore as they always did. A sealed file carries a clear header (format, obop version, schema version, time, the id of the key) that is authenticated but not secret; the manifest names the models and how many records each holds. Nothing else about the records is outside the cipher: `obop backup verify FILE --canary TEXT` fails if the text is found in the file as stored.

**Which key.** `OBOP_BACKUP_KEY` (64 hex digits: `openssl rand -hex 32`; earlier keys in `OBOP_BACKUP_KEY_PREVIOUS`, comma separated) is a key of its own that you can keep apart from the server's. Without it the server key `OBOP_SECRET_KEY` (and its previous keys) is used, which means a backup is exactly as readable as the sealed secrets inside it, and that rotating the server key (below) leaves older backups needing the previous key. A key that was generated into the database (an instance with no `OBOP_SECRET_KEY`) is **never** used: it is inside every backup, so a backup sealed with it could not be opened after the database is gone; such an instance writes format 1 and says so. The server holds the key it seals with, so this protects the *file* (a stolen copy, a leaked bucket, a shared volume), not a compromised server. A key held only as a public key, so that the server could not read its own backups, is not built (D73, "Not delivered").

**What to keep apart from the backups, and what is lost if it is lost.**

| Keep | Where | If it is lost |
|---|---|---|
| The backup key (`OBOP_BACKUP_KEY`, or `OBOP_SECRET_KEY` when that is what sealed) | Your secret store, in at least two places, never on the volume of the backups | **Every backup sealed with it is unreadable.** There is no backdoor and no recovery. Keep a previous key for as long as any backup sealed with it is kept |
| `OBOP_SECRET_KEY` | Your secret store | The data restores (when a separate backup key sealed it); the sealed secrets inside it (two-factor seeds, connection credentials) cannot be opened (see "If the key is lost"). If it was also the backup key, see the row above |
| The manifest (`<file>.manifest.json`) | Beside the data file, copied with it | A sealed data file without its manifest cannot be verified or restored by `obop restore`: copy both (the manifest is a few kilobytes) |
| The database password, `OBOP_PUBLIC_URL`, DNS | Wherever you keep deployment settings | A restore into a new database needs a new password and address; passkeys are bound to the address (D65) |

Format 2 uses no public-key signature, so there is no signing key or private key to custody (OBR-DAT-046 asked for both; see D73).

- **What is in it:** every record, in insertion order (including the schema version record, the migration ledger and the generated key if the instance has no `OBOP_SECRET_KEY`). **What is not:** sessions, pending approval plans and rate-limit counters (people sign in again; agents propose again). **Secrets stay sealed exactly as stored**; the server key is not in the backup.
- **Verification** reads every record (gzip integrity, shape, counts per model, a checksum over the records, and for a sealed backup every chunk, the hash chain over the chunk tags and the manifest's MAC). A truncated, edited or damaged file fails. A format 1 file's checksums detect damage; they do not authenticate: keep those where only the operator can write. `obop backup verify FILE --structure-only` checks a sealed file's size, checksum, framing and hash chain without the key and says it proved nothing about who wrote it.
- **Restore** refuses a non-empty target unless `--force`; checks the format (an unknown format is refused), refuses a backup from a newer schema than the binary knows, and writes in one transaction, so a failure leaves the target as it was. A backup from an older schema restores and tells you to run `obop migrate run`. `--force` also clears the target's sessions and counters. The restore is **one transaction**: its duration at your data size is what the drill measures (below); above about 10 GB a resumable chunked restore is not built (D73, "Not delivered").
- SQLite and PostgreSQL are both supported, and a backup moves between them (it is how you move a local database to PostgreSQL).
- **Memory** stays at a few megabytes (one chunk, one batch of records and one uploaded file at a time); a 50 GB database has not been backed up here, so the figure is by construction, not measured.

### Scheduled backups, retention and an off-host copy (D73)

`obop backup run` is the whole job once, for cron or a platform's scheduled job; `obop serve` runs the same job on its own timer. Either way: write the backup (sealed when a key is set), read it back and authenticate it, record it in the database (`obop backup list` shows them), copy it to every sink and prove each copy by reading it back and comparing its hash, then apply retention.

```bash
obop backup run --db postgres://... --out /var/backups/obop --copy-to /mnt/offsite
OBOP_BACKUP_DIR=/var/backups/obop OBOP_BACKUP_SCHEDULE=24h OBOP_BACKUP_COPY_DIR=/mnt/offsite obop serve ...
```

| Variable / flag | Meaning |
|---|---|
| `OBOP_BACKUP_DIR` / `--out` | Where backups are written on this host: a volume that outlives the container. Also where the backup before an update or a migration goes |
| `OBOP_BACKUP_SCHEDULE` | `24h`, `12h`, `90m`, `1d` ... (at least five minutes). The server checks every minute whether the newest verified scheduled backup is older than this; the decision lives in the database, so it survives restarts, and with several processes on one database one of them (a lease in the shared store) takes each backup. Needs `OBOP_BACKUP_DIR`. Off when unset |
| `OBOP_BACKUP_COPY_DIR` / `--copy-to DIR` | Directories to copy each backup to (comma separated; repeat the flag). A copy counts only when it was read back and its SHA-256 matched. Objects are write-once: a different file under an existing name is refused |
| `OBOP_BACKUP_COPY_DELETE=1` / `--copy-delete` | Let retention delete in the copy too. Off by default: the credential that writes backups should not be able to delete them, and expiry in the store belongs to its lifecycle rule |
| `OBOP_BACKUP_KEEP_DAILY`, `_WEEKLY`, `_MONTHLY` / `--keep-daily N` ... | Retention. Default **35 daily, 0 weekly, 12 monthly**: for each of the last N UTC days (ISO weeks from Monday, calendar months) that has a backup, the newest backup of that period is kept; a backup kept by any rule is kept; the newest backup is never deleted |

**Retention is a promise to put in your privacy notice.** It bounds how long a person's erased data survives in backups: with the default, up to 12 months. It applies to *scheduled* backups only; a backup taken by hand, before a migration or before an update stays until a person removes it (so does its data). A scheduled backup is never deleted from this host until every configured sink holds a verified copy of it.

**The copy in another failure domain.** Shipped: a directory sink (`FsSink`): a mounted share, a second disk, or an object store mounted with `rclone mount` or `s3fs`. **An S3-compatible sink is not built**: a signed multipart upload client that can be tested against a real store is a project of its own, and an operator-run uploader does the same job today. Run `obop backup run` with `--out` on a local disk, upload both files of each backup with your tool (`aws s3 cp`, `rclone copy`) to a bucket with **object lock (compliance or governance mode), versioning and a lifecycle rule** in another account and region, give the uploader's credential write but not delete or lock-shortening rights, and prove the copy the way obop does: download it to a scratch directory and run `obop backup verify` on it (or, better, the drill below on it). A copy you have not restored is not a backup.

**Alerts.** The metrics (`GET /metrics`, see "Health and metrics") carry `obop_backup_age_seconds`, `obop_backup_size_bytes`, `obop_backup_duration_seconds`, `obop_backup_copy_age_seconds`, `obop_restore_drill_age_seconds` and `obop_backup_last_run_failed`. Prometheus rules (not run against a real Prometheus):

```yaml
- alert: ObopBackupOverdue          # nightly job: older than 26 hours, or none at all
  expr: obop_backup_age_seconds > 93600 or absent(obop_backup_age_seconds)
  for: 5m
- alert: ObopBackupNotOffHost
  expr: obop_backup_copy_age_seconds > 93600 or absent(obop_backup_copy_age_seconds)
  for: 5m
- alert: ObopRestoreDrillOverdue    # the last drill that passed is older than 35 days
  expr: obop_restore_drill_age_seconds > 3024000 or absent(obop_restore_drill_age_seconds)
  for: 5m
- alert: ObopBackupRunFailed
  expr: obop_backup_last_run_failed == 1
  for: 15m
- alert: ObopWalArchiveLate          # a recovery point of five minutes needs the archiver to keep up
  expr: obop_wal_archive_lag_seconds > 300 or increase(obop_wal_archive_failed_total[10m]) > 0
  for: 1m
```

PostgreSQL switches to a new WAL segment every `archive_timeout` **only when there was activity**, so on an idle database `obop_wal_archive_lag_seconds` grows without anything being wrong; the failure counter is the sure sign, and the lag rule is for a database that is in use. The age of the last *base* backup is the business of your WAL tool (`pgbackrest info`), not an obop metric.

`obop storage check` asks the same questions from the command line (below).

### The restore drill

`obop drill BACKUP` restores a backup into a scratch database (a temporary SQLite file, or an empty database you made: `--scratch postgres://.../scratch`) and checks the copy; `deploy/restore-drill.sh BACKUP` does that on a scratch PostgreSQL database it makes and drops, and then starts a server on the copy:

```bash
DRILL_ADMIN_URL=postgres://postgres:PASSWORD@HOST:5432/postgres \
OBOP_BACKUP_KEY=... OBOP_SECRET_KEY=... DRILL_RECORD_IN=postgres://.../live \
  deploy/restore-drill.sh /backups/nightly.obbk
```

The checks: the backup verifies (a sealed one is authenticated with its key); it restores; the records per model equal the manifest; **a SHA-256 over every restored record in order equals the backup's record digest**; the schema version is one this build serves (a backup from an older schema fails the drill unless you pass `--migrate` / `DRILL_MIGRATE=1`, which migrates the copy: the drill of an upgrade); the migration ledger of the copy is clean; the accounts came back; no sealed secret is unopenable with the keys given; uploaded files came back and hash to their ids; a fixed write, change, read and delete works on the copy. The script then starts `obop serve` on the copy and asks `GET /api/openapi.json` and `GET /readyz`. It exits non-zero on the first failed check; **a sealed backup without its key exits 3**.

`--record-in URL` (`DRILL_RECORD_IN`) writes the result into your real database as a restore drill record with the **measured** recovery point (the age of the backup when it was drilled) and recovery time (how long the restore and the checks took on that machine with that data). Those are measurements, not promises. CI runs the drill on every change against a PostgreSQL service: a sealed backup, a plain one, the wrong key (must exit 3) and a corrupt copy (must fail). Run it from a schedule against your real backups (monthly at least; docs/RUNBOOK-RECOVERY.md has the roles and the quarterly and half-yearly drills). Checks that do not apply say "not checked" instead of passing silently. Not covered: the history, freeze-hash, chain and guard-trigger checks of OBR-DAT-049 (those models do not exist yet, D75 to D77), the erasure replay (no erasure ledger yet), and a loss-of-primary drill on a new host, which is yours to run and time.

### PostgreSQL itself: WAL archiving and point-in-time recovery

`obop backup` is the portable, version-independent copy. A database server also needs its own, for point-in-time recovery and for losing the volume: a nightly backup loses up to a day; WAL archiving loses minutes. obop does not run it: you configure PostgreSQL, and `obop storage check` tells you when the settings or the evidence do not support the promise. **None of this has been run against the Sliplane service or any managed PostgreSQL**; it was written from the PostgreSQL and pgBackRest documentation and the checks were run against the stock `postgres:17` image and a fake server.

The settings (`postgresql.conf`; `archive_mode` needs a restart):

```conf
wal_level = replica                  # or logical
archive_mode = on
archive_command = 'pgbackrest --stanza=obop archive-push %p'   # or wal-g wal-push %p, or archive_library
archive_timeout = 60s                # a segment leaves the host at least every minute: a recovery point of minutes
data_checksums on at initdb          # silent disk corruption is then an error, not data
```

A pgBackRest sketch (repository on an object store with object lock, encrypted with its own passphrase; check the options against your version):

```conf
# /etc/pgbackrest/pgbackrest.conf
[global]
repo1-type=s3
repo1-s3-endpoint=s3.eu-central-1.example.com
repo1-s3-bucket=obop-wal-archive         # a bucket with object lock + versioning, another failure domain
repo1-cipher-type=aes-256-cbc
repo1-cipher-pass=...                    # from the secret store, not this file in git
repo1-retention-full=35                  # a PITR window of 35 days or more
start-fast=y
[obop]
pg1-path=/var/lib/postgresql/data
```

Take a base backup at least daily (`pgbackrest --stanza=obop --type=full backup`, or incremental with a weekly full), and `pgbackrest check` / `verify` after changes. **A migration creates a named recovery point**: `obop migrate run` calls `pg_create_restore_point('pre-migration-<schema version>')` before its first step (it needs the privilege; with WAL archiving on, strict mode refuses to run without it) and writes the name and position to the ledger rows. Point-in-time recovery to it is `recovery_target_name = 'pre-migration-3'` with `recovery_target_action = promote`.

`obop storage check --db postgres://...` verifies, on the real server: `wal_level`, `archive_mode`, an `archive_command` or `archive_library` that is not a placeholder (the command itself is never printed: it can hold credentials), `archive_timeout` of 1 to 60 seconds, that `pg_stat_archiver` has archived a segment in the last 5 minutes and that the last attempt did not fail, `fsync` and `full_page_writes`, data checksums, and TLS (D66, above: a host that is not this machine or a private name must be on `sslmode=require` at least, `verify-full` recommended); then the evidence: a verified backup under 26 hours old, sealed, with a verified off-host copy; a restore drill that passed in the last 35 days; and the storage attestation. It exits non-zero on any failure. It checks configuration and evidence, not the world: that `archive_command` points at an encrypted, locked store in another failure domain is what you attest and what the drills prove.

**Host gate.** Before real personal data is loaded onto a host: perform a point-in-time restore on that host and keep the result. Procedure: write a marker row at T0; write more rows; restore the base backup plus WAL to T0 plus one second on a fresh server (`recovery_target_time`); the marker is there and the later rows are not; then `obop migrate status` and the drill checks on the result. Measure the time of a restore of your base backup plus WAL at your data size: it is the number the runbook asks for. This has not been done in this repository.

### Encryption at rest

obop encrypts what it controls: sealed secrets (the server key), backups (format 2, above), and nothing else by itself. The database files, the WAL, temporary files, swap, the WAL archive, the backup volume and the blob directory sit on encrypted storage that you provide: a volume or provider-managed encryption (AES-256 or stronger) whose keys are held outside the reach of ordinary database operators (the provider's key service, or a separate custodian) and never on the same volume. obop adds no database-level encryption for this (field-level encryption is a separate step).

Record that, per plane, with a person's name on it, before load and again every 12 months and after any host change:

```bash
obop storage attest --db postgres://... --plane main --host db1.example.com --provider "Acme Cloud" \
  --mechanism "provider-managed volume encryption, AES-256-XTS" --custody "provider key service" \
  --readers "the provider's key service only; no database operator" --evidence "ticket 1234, provider test result 2026-10-01"
obop storage show        # attestations, restore drills
```

`obop storage check` fails when there is no attestation or it is more than 12 months old, and warns in the last 30 days. Verify the claim with canary scans: plant a string in a record, take a backup (`obop backup verify FILE --canary TEXT` finds 0 occurrences in a sealed one), and look for it with `strings` in a WAL segment, a base backup and a blob object of your own stack; a provider snapshot mounted on another machine without the key must be unreadable (record the provider's test result as the evidence). Not verified here: any provider.

## Upgrades

### Schema versions

A database carries its schema version in one record (`obop_meta/schema`). Today that is the JSON-document layout plus the shapes the modules store in it (version 1 is the layout of today, step 2 re-seals secrets from before the key ring). Each change to a stored shape is a numbered, ordered, idempotent step in `obop-migrate`; a step either returns its writes, which are applied together with its version record and its ledger row in **one transaction**, or does its own batched, resumable writes.

```bash
obop migrate status --db URL     # the version in the database, the version this build writes, pending steps, the ledger
obop migrate verify --db URL     # ledger, code and version compared; exits non-zero on drift
obop migrate plan --db URL       # what each pending step would do, with its cost; stores the plan
obop migrate run --db URL        # apply them, in order, under a database lock (two runners apply each step once)
```

- A fresh, empty database is stamped with the current version. A database from before versioning counts as version 0.
- **A server refuses to run against a newer schema than it knows** ("run a newer obop, or restore a backup taken before the upgrade"). A downgrade cannot be done safely, so there is none.
- When pending steps exist, a starting server runs them only with `--auto-migrate` (default **on for SQLite, off for PostgreSQL**); otherwise it prints the exact command and exits. Hosted instances set `OBOP_AUTO_MIGRATE=1` (one process per instance); the control plane's own SQLite database upgrades itself. A server that migrates itself takes a **verified backup first** when `OBOP_BACKUP_DIR` is set (sealed when a key is, recorded in the ledger) and stops without migrating if that backup fails; without a directory it migrates and says so. It does not demand a stored plan or a rehearsal: that is the choice of `--auto-migrate`; for the full gate turn it off and run `obop migrate run`.

### The migration ledger (D73)

Every step that runs leaves a row in the model `obop_migration`, written in the same transaction as the step: the module (`core` or a package id), the kind (`rust`, `package`), the step, its class (`expand`, `backfill`, `contract`), the cost class of its DDL (`none`, `instant`, `concurrent`, `blocking`, `rewrite`), the **SHA-256 of its definition**, the status (`applied`, `failed`, `partial`, `adopted`), when, how long, who (`operator`, `server`, or the administrator of `module.upgrade`), the database role (the one application role today), the obop version, the backup and the restore point that preceded the run. `obop migrate status` prints it.

- **A step that was edited after it ran stops the server from starting** and names the step: the database in front of you was not made by the code in front of you. For a Rust step the checksum covers its declared identity and the text of its source file with comments, blank lines and indentation ignored. An applied step is never edited: to change what a step does, add a step. The checksums of the shipped steps are pinned in a test.
- A database from before the ledger is **adopted**: its steps get rows with status `adopted` (not replayed), the time taken from the old history line; so does a fresh database (`created at this version`).
- A step that fails inside its transaction leaves the database as it was and a `failed` row (a separate row per attempt, with the reason); a step that works in batches writes a `partial` row before it starts, and an interrupted run leaves it `partial` and the next run resumes it.
- `module.upgrade` and `module.rollback` write one `package` row per `[[migration]]` step, in the same atomic batch as the package record and the data.
- The integer in `obop_meta/schema` stays: servers compare it. The ledger is the record of what ran. Making the rows append-only in the database (a trigger, a separate owner role) is D77.
- **DDL.** Nothing issues DDL on the serving path today except schema creation and the unique indexes of D57. The typed layout and the guard triggers of D75 to D77 will, and must go through **one DDL component** that classifies each statement with `obop_migrate::ddl::classify` (a table of known PostgreSQL 17 and 18 forms; an unknown form is refused, not guessed), sets `lock_timeout` 5 s and `statement_timeout` 30 min, retries a lock timeout with a bounded, jittered backoff, and refuses a `blocking` or `rewrite` step on a table above 1,000,000 records unless the operator passes `--window`. The classifier, the policy and the window rule exist and are tested; the component that executes statements does not. `obop migrate verify` does not yet compare the live catalogue (a hand-made index, a missing trigger): that needs the typed layout.
- A step declares `compat_n_minus_1` (the previous release keeps working against the schema it leaves) or `downtime`; `obop migrate plan` prints which, and a test fails a step that declares neither. The check that binary N-1 serves requests against schema N needs a published N-1 image and is not built.

### Migration gates (D73)

`obop migrate run` on PostgreSQL is **strict**: it refuses unless

1. a verified backup no older than 24 hours exists (`--backup FILE`, opened with its key when sealed; or one is taken into `--backup-dir` / `OBOP_BACKUP_DIR` first, read back and authenticated) and its name goes into the ledger rows;
2. `obop migrate plan` stored the plan for exactly this database state and this build;
3. there is room: twice the database's size where the backup goes and (SQLite) beside the file; a PostgreSQL host's own disk cannot be read from a client, so you check it is at least twice the largest table the plan touches and pass `--disk-ok`;
4. a **rehearsal** of the same plan passed: `obop migrate rehearse --from BACKUP --out receipt.json` restores a backup into a scratch database (a temporary SQLite file, or `--scratch postgres://.../empty`; never the production database, which it refuses by name), runs the plan there, runs the drill's checks and a fixed read and write set, and reports the time, the longest single transaction and the records before and after; the receipt is signed with the server key (a hand-written, edited or other-key receipt is refused) and counts for a week; a table-locking step (DDL class `blocking` or `rewrite`) that holds its lock longer than one second fails the rehearsal;
5. the ledger is clean, the schema is not newer than the build, no other migration holds the lock, and a long-lock step on a big table has its `--window`.

On SQLite (and in memory) the same checks run and a failing one is a warning (`--gates strict|relaxed` or `OBOP_MIGRATION_GATES` overrides). With WAL archiving on, strict mode also needs `pg_create_restore_point('pre-migration-<version>')` to succeed. `module.upgrade` and `module.rollback` that rewrite records (a `rename_field`, `set_default` or `drop_field` step) take the same backup first, into `OBOP_BACKUP_DIR`: **on PostgreSQL they are refused with nothing changed when no directory is set** (set it; or `OBOP_MIGRATION_GATES=relaxed` if you take your own); an additive upgrade needs nothing.

**Rollback is forward-only: it is a restore.** `obop migrate rollback --from BACKUP --reason "..." --yes` verifies the backup, replaces the database with it in one transaction, and records the reason, the backup and what the discarded database's ledger said in the ledger and in the security log. Stop every process first; start the obop version that matches the backup's schema; with WAL archiving, point-in-time recovery to `pre-migration-<version>` is the other way back. How long it takes is the restore time your last drill measured (docs/RUNBOOK-RECOVERY.md). An old binary refuses a newer schema, as before.

### The compatibility promise

While the core is below 1.0 a minor version may break (D55). Within that: (1) a build never reads a database newer than itself and says so; (2) a build upgrades any database of an older schema, step by step, without data loss, and a step that fails changes nothing of itself; (3) migrations are forward-only, so the way back from an upgrade is the backup you took before it (`obop migrate rollback`); (4) every backup restores into a build of the same or a newer schema and is migrated on the way. From 1.0 the destructive-change rules of ENGINEERING 4.3 apply.

### Procedure (self-hosted)

1. `obop migrate plan --db URL` with the **new** binary shows the steps with their cost and stores the plan; read the release notes.
2. `obop backup --db URL --out DIR` and `obop backup verify FILE` (or let `migrate run` take it); on PostgreSQL also check WAL archiving is healthy (`obop storage check`).
3. Rehearse: `obop migrate rehearse --from FILE --out receipt.json` on a machine that is not the database host.
4. Stop the old processes (or, with several processes and only additive steps, roll them one by one: old processes keep running until they meet a schema they do not know, so finish the migration quickly).
5. `obop migrate run --db URL --backup FILE --rehearsal receipt.json --disk-ok`, then start the new processes.
6. Roll back with `obop migrate rollback --from FILE --reason "..." --yes` and the **old** binary.

### Procedure (Sliplane instances)

See "The published image": build, then `obop platform upgrade-instances --image <commit>`. Hosted instances run with `OBOP_AUTO_MIGRATE=1`, so the redeploy migrates; set `OBOP_BACKUP_DIR` on a volume so the migration is preceded by a verified backup. Take a backup of the cell (or of the instance with `obop backup`) before a release that has migration steps; the command stops at the first instance that fails its health check and names it.

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
- **Backups off the host:** scheduled backups, retention and a directory sink exist (D73); an S3-compatible sink is not built (run your own uploader with object lock and drill from the copy), asymmetric keys (a server that cannot read its own backups) are not built, and a restore is one transaction (no resumable chunked restore above 10 GB). Nothing about WAL archiving, point-in-time recovery or encryption at rest has been run against the Sliplane service or a managed PostgreSQL.
- **Instances created before D53** have no `OBOP_SECRET_KEY` unless an operator set one; without it a generated key lives in their database.
- **Migrations** of the core exist for the two steps above, each with a ledger row and a checksum (D73); a newer version of a package migrates its records only through the `[[migration]]` steps it declares (D63), each with a ledger row. The DDL component, the live-catalogue comparison of `migrate verify`, the check that binary N-1 serves schema N, and the history/freeze/chain/guard checks of the drill wait for D75 to D77.
- **Updating from inside** has run against the fake provisioner and a loopback control plane, not against live Sliplane; the release feed is unsigned (a digest per release only).
- **The published image** has not been built by GitHub from this history yet, and `upgrade-instances` against live Sliplane has not been run; both are covered by tests against stand-ins (the Docker build, the runner fakes), not against the real services.
