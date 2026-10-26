---
title: "Self-hosting and operations"
description: "Run obop yourself: Docker image, PostgreSQL, the control plane, Sliplane deployment, serving this website, and what is missing: backups, migrations, upgrades and monitoring."
lead: "What you need to run obop for real, and an honest list of what you have to do yourself."
---

## The pieces

| Piece         | Command                               | What it does                                                                                                                                                            |
| ------------- | ------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Instance      | `obop serve`                          | One business: REST, MCP over the CLI and the web app. SQLite locally, PostgreSQL when `OBOP_DATABASE_URL` starts with `postgres://`.                                    |
| Control plane | `obop platform serve`                 | Sign-up, instances, and the public website. Runs a provisioner per instance.                                                                                            |
| Provisioners  | `--provisioner local\|sliplane\|fake` | `local` starts child `obop serve` processes with SQLite; `sliplane` creates a database on a shared PostgreSQL and a Sliplane service per instance; `fake` is for tests. |

## One instance with Docker and PostgreSQL

The repository has a root `Dockerfile` and a `docker-compose.yml`. The image contains the `obop` binary, the back office build (`/srv/backoffice`) and the website (`/srv/site`). By default it runs one instance:

```bash
docker compose up --build
```

starts PostgreSQL 18 and obop together, with obop published on `127.0.0.1:8080`. For production, run the image against your own database:

```bash
docker run -d --name obop -p 8080:8080 \
  -e OBOP_DATABASE_URL=postgres://obop:secret@db.internal:5432/obop \
  obop
docker exec -it obop obop admin create alice --all-groups
```

Put a reverse proxy with TLS in front. obop speaks plain HTTP.

### Call the API from a browser application on another origin

Set `OBOP_ALLOWED_ORIGINS` to a comma-separated list of exact origins, for example `https://app.example.com`. Only those origins can read answers from `/api` in a browser; the setting is off by default, wildcards are refused when the server starts, and plain `http` is accepted for `localhost` only. The API uses bearer tokens, never cookies, so credentialed requests are not allowed. Not yet tried in a real browser or behind a CDN.

### Encrypt the connection to PostgreSQL

A `postgres://` address connects without TLS unless you ask. On any network you do not fully control, add a mode to the address:

```text
postgres://obop:secret@db.example.com:5432/obop?sslmode=require
postgres://obop:secret@db.example.com:5432/obop?sslmode=verify-full&sslrootcert=/etc/obop/db-ca.pem
```

`require` encrypts but does not check who answers. `verify-full` also checks the certificate chain against the root you give (or against the public roots with `sslrootcert=system`) and that the certificate names the host: use it when you can. `verify-ca` checks the chain only. A server without TLS, an untrusted certificate or a wrong host name makes the connection fail; obop never falls back to a plain connection. The same parameters work in `--db` for `obop backup`, `restore` and `migrate`. Client certificates are not supported. obop uses rustls, so no system OpenSSL is needed. This has been tried against the stock PostgreSQL 17 image with a self-signed certificate only, not against managed services or connection poolers.

## The control plane and this website

The control plane serves the website with a plain static file server and the sign-up app under `/app/`:

```bash
deploy/build-site.sh
cargo run -- platform serve --static web/site --instance-static web/apps/backoffice/dist
```

The image does the same with `--static /srv/site`. Run it with `obop platform serve --addr 0.0.0.0:8080 --static /srv/site --provisioner sliplane`.

## Sliplane

The reference deployment runs on Sliplane, in one project with three kinds of service: a PostgreSQL 18 service on an internal network (one database and one role per business), the control plane (public, its own SQLite on a volume) and one service per customer instance, started from a published image (`ghcr.io/spacecorps/obop`) or built from the repository. The control plane provisions through the bundled `sliplane` CLI and needs `SLIPLANE_API_KEY` as a secret. All settings are in [Environment variables](/docs/reference/environment/).

## Operating it

### Known limits

- **Build time and memory.** An instance created from a repository builds the Rust workspace on the server. Use the published image (`OBOP_IMAGE`): there is no build.
- **TLS** ends at the platform's proxy and the instance speaks HTTP behind it.
- **Upgrading instances** is one command: `obop platform upgrade-instances --image TAG` moves every running instance to a published image, one at a time, each after a health check, and stops at the first failure.
- **Schema versions.** A database carries its schema version. `obop migrate status|verify|plan|run` shows and applies the steps and keeps a ledger of what ran (a step edited after it ran stops the server); on PostgreSQL `migrate run` needs a verified backup, the stored plan, a rehearsal and a free disk (`docs/DEPLOY.md`, "Migration gates"); a server refuses a database newer than itself. Installing a newer package version still does not migrate that package's records. Prefer adding optional fields.
- **Backups.** `obop backup --db URL --out DIR` writes a consistent copy while the server runs, sealed (encrypted and authenticated) when `OBOP_BACKUP_KEY` or `OBOP_SECRET_KEY` is set; `obop backup run` and `OBOP_BACKUP_SCHEDULE` take scheduled backups with an off-host copy and retention; `obop backup verify FILE` checks it; `obop restore --db URL --from FILE` restores it into an empty database; `obop drill` and `deploy/restore-drill.sh` prove a restore end to end; `obop storage check` reports the evidence. The database server itself still needs its own `pg_dump` or WAL archive, and a restore you have not tried is not a backup.
- **Full export.** `obop backup --plain` is a complete, documented export (gzip JSON Lines); a sealed backup is the same stream, encrypted. The customer-facing export tool of the product plan is not built.
- **Monitoring.** Every instance serves `GET /healthz` (the process is up), `GET /readyz` (the database answers and its schema fits; 503 with a short reason otherwise) and `GET /metrics` (Prometheus text). The first two are open and reveal nothing else; `/metrics` stays closed until you set `OBOP_METRICS_TOKEN` (or you run bound to loopback). Not yet tried against a real Prometheus or load balancer. The control plane has no such routes yet. Also watch the database.
- **Audit log.** There is no hash-chained audit trail yet.

### Sizing

An instance is one process by default, and several processes may share one PostgreSQL database (sessions, rate limits, approvals and the rest are kept in the database). SQLite is enough to evaluate. Use PostgreSQL for more than one user or for anything you cannot lose. `list` and `search` read the whole model into memory on the server, so very large tables are slow; there are no indexes yet.

## Security basics

- Run behind TLS. Never expose `--dev-auth`.
- Keep `OBOP_DATABASE_URL`, `SLIPLANE_API_KEY` and agent tokens in a secret store.
- Create administrators with a long password (12 or more characters is enforced) and give day-to-day users only the groups they need.

See [Security and privacy](/docs/reference/security/) and [Getting started](/docs/getting-started/).
