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

## Configuration

Command line flags win over variables. Secrets are set as secret environment variables of the service (Sliplane: `--secret-env`), never in the database, the repository or a plain variable.

### Every instance and the control plane

| Variable | Meaning |
|---|---|
| `OBOP_DATABASE_URL` | Where the data lives (`--db`): `sqlite://obop.db` (the default for an instance; `sqlite://obop-platform.db` for the control plane), `postgres://user:pass@host/db`, `memory:`. |
| `OBOP_SECRET_KEY` | The current server key, 32 random bytes as 64 hex digits (`openssl rand -hex 32`). Seals two-factor secrets and connector credentials, and the sessions and approval plans kept in the database. See "The server key". Without it a key is generated and stored in the database next to the data, which protects nothing against someone holding a copy of the database. |
| `OBOP_SECRET_KEY_PREVIOUS` | Earlier keys, comma separated: what the current key replaced and that may still protect stored values. Only needed between a key change and the end of `obop secrets rotate`. |
| `OBOP_AUTO_MIGRATE` | `1` lets a starting server run pending schema steps itself, `0` makes it refuse and print the command. Default: on for SQLite, **off for PostgreSQL**. `--auto-migrate` and `--no-auto-migrate` override it. |
| `OBOP_REQUIRE_MFA_FOR_ADMINS` | `0` changes the *default* of the setting `security.require_mfa_for_admins` to off (the production default is on). A value an administrator stored under Customize > Security wins. |
| `OBOP_DEV_INSECURE_KEY` | `1` selects a public, fixed key and logs a warning. Development and tests only. |
| `OBOP_MFA_ISSUER` | The name shown in the authenticator app (default `obop`). |
| `OBOP_PUBLIC_URL` | The external URL behind a proxy (`https://acme.example.com`): MCP discovery documents, webhook and OAuth redirect addresses, the community pages. |
| `OBOP_ALLOWED_ORIGINS` | Comma-separated browser origins allowed to call `/mcp`. |
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
| `OBOP_UPGRADE_SETTLE_SECS` | Seconds to let a redeploy settle before asking Sliplane whether it is live (default 30). |
| `SLIPLANE_ACCOUNT`, `SLIPLANE_API_KEY`, `OBOP_SLIPLANE_PROJECT`, `OBOP_SLIPLANE_SERVER`, `OBOP_CELL_ADMIN_URL`, `OBOP_CELL_INSTANCE_HOST` | The Sliplane provisioner (see "Sliplane"). |

### Tests

`OBOP_TEST_POSTGRES_URL` adds PostgreSQL to `cargo test`, including the multi-process tests; `OBOP_BIN` points the end-to-end suite at a binary.

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
| Order idempotency keys | Put-if-absent records, so a double submit that lands on two processes makes one order. |
| Governed writes | One database lock around the guard and the write of every mutating call, so two processes cannot race between check and write. |
| Forum and status page | The database is the truth; each process keeps a copy that it reloads when a shared version number moves (checked at most every 250 ms). A post made on one process shows on the others within that. Writes take a database lock first, so ids are never handed out twice. One process per interval runs the status checks. |
| Connector retry queue and dead letters | Rows with a lease: a worker claims due deliveries for 120 seconds; if it dies, another takes them. Delivery is at-least-once, and the delivery id is stable, so a receiver can deduplicate. Due pulls and the daily purge run once across processes. |

What is *not* shared: small per-process caches (site settings for a few seconds, the forum and status copies between version checks). SQLite supports several processes on **one machine** too (file locks next to the database), but not on a network file system; use PostgreSQL to scale out.

The locks are PostgreSQL session advisory locks (released by the server when a process dies) or, for SQLite, operating-system file locks named `<database file>.lock-<key>` (harmless to leave behind).

Test it: `OBOP_TEST_POSTGRES_URL=postgres://... cargo test -p obop --test multi_process` starts two real servers on a scratch database and checks that a login on one works on the other, an MCP session continues across them, a plan proposed on one is approved on the other, parallel writes all land once, and rate limits and the lockout are counted across both.

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

`obop backup` writes a consistent logical copy of every record: gzip-compressed JSON Lines plus a manifest with checksums, the schema version and the core version. It reads from one snapshot (a repeatable-read transaction on PostgreSQL, a read transaction on SQLite), a batch at a time, so a large database never sits in memory and a running server is not stopped.

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

It removes the second factor, revokes the person's sessions and writes an `operator` record to the security log (`security_event`). The next sign-in is password-only, and if the policy covers the person they are asked to set two-factor up again.

**Accounts, sessions and passwords.** In Customize > Security everyone can change their own password (the current password, and a code when two-factor is on), see their sessions and sign out the others or everywhere. Turning two-factor off or changing the password signs out the person's other sessions. Administrators see the people of the instance, disable and enable accounts (a disabled account's sessions end at once and its agent tokens stop working until it is enabled again; the last enabled administrator cannot be disabled) and set a temporary password (the person must choose a new one at the next sign-in; their sessions end). The equivalent API: `GET /api/auth/sessions`, `POST /api/auth/sessions/revoke-all`, `POST /api/auth/password`, `GET /api/auth/users`, `POST /api/auth/users/{name}/disable|enable|reset-password` (administrators). Every one of these, and every change to the `security.*` instance settings (who, old value, new value; values that look like secrets are never written), is a row in the security log.

## Known limits

- **TLS** terminates at Sliplane's proxy and the instance speaks plain HTTP behind it. Confirm before relying on it. The control plane reaches the cell over the internal network without TLS.
- **Scheduled backups of hosted instances** are not built (they need object storage); run `obop backup` or `pg_dump` from a job (see "Backups").
- **Instances created before D53** have no `OBOP_SECRET_KEY` unless an operator set one; without it a generated key lives in their database.
- **Migrations** exist for the two steps above; installing a newer version of a package still does not migrate existing records of that package.
- **The published image** has not been built by GitHub from this history yet, and `upgrade-instances` against live Sliplane has not been run; both are covered by tests against stand-ins (the Docker build, the runner fakes), not against the real services.
