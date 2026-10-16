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

## The control plane and this website

The control plane serves the website with a plain static file server and the sign-up app under `/app/`:

```bash
deploy/build-site.sh
cargo run -- platform serve --static web/site --instance-static web/apps/backoffice/dist
```

The image does the same with `--static /srv/site`. Run it with `obop platform serve --addr 0.0.0.0:8080 --static /srv/site --provisioner sliplane`.

## Sliplane

The reference deployment runs on Sliplane, in one project with three kinds of service: a PostgreSQL 18 service on an internal network (one database and one role per business), the control plane (public, its own SQLite on a volume) and one service per customer instance built from the same repository. The control plane provisions through the bundled `sliplane` CLI and needs `SLIPLANE_API_KEY` as a secret. All settings are in [Environment variables](/docs/reference/environment/).

## Operating it

### Known limits

- **Build time and memory.** Each instance builds the Rust workspace on the server. That is slow and competes with other services; a registry image would remove it.
- **TLS** ends at the platform's proxy and the instance speaks HTTP behind it.
- **Upgrading instances is manual.** Auto-deploy is off; redeploy each instance.
- **No schema migrations.** Records are JSON documents, and installing a newer package version does not migrate existing records. Prefer adding optional fields.
- **Backups.** Nothing backs up the database server for you. Use `pg_dump` or your provider's snapshots, and test a restore. A single SQLite file can be copied while the server is stopped.
- **Full export.** The product plan promises a full export on every plan; the tool is not built. Your database is your export.
- **Monitoring.** There is no metrics endpoint yet. Watch the process and the database.
- **Audit log.** There is no hash-chained audit trail yet.

### Sizing

An instance is one process. SQLite is enough to evaluate. Use PostgreSQL for more than one user or for anything you cannot lose. `list` and `search` read the whole model into memory on the server, so very large tables are slow; there are no indexes yet.

## Security basics

- Run behind TLS. Never expose `--dev-auth`.
- Keep `OBOP_DATABASE_URL`, `SLIPLANE_API_KEY` and agent tokens in a secret store.
- Create administrators with a long password (12 or more characters is enforced) and give day-to-day users only the groups they need.

See [Security and privacy](/docs/reference/security/) and [Getting started](/docs/getting-started/).
