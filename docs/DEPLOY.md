# Deploying obop: control plane, instances and Sliplane

- **Status:** working draft, 2026-10-16. Written against the code in the repository, not a design target.

## What exists

| Piece | Command | What it does |
|---|---|---|
| Instance | `obop serve` | One business: REST, MCP, backoffice. SQLite locally, PostgreSQL when `OBOP_DATABASE_URL` is `postgres://` (D44). |
| Control plane | `obop platform serve` | Sign up, then create, list and delete your own instances. Runs a provisioner per instance. |
| Provisioners | `--provisioner local\|sliplane\|fake` | `local` starts child `obop serve` processes with SQLite; `sliplane` creates a database on a shared cell PostgreSQL and a Sliplane service per instance; `fake` is for tests. |
| Modules | Modules page, or `obop modules ...` | Admins install, upgrade and remove declarative packages (`obop.toml`) from an upload or the marketplace. |

The owner's Argon2 hash is copied into the new instance as its first admin (`platform.admin`), so the signup email and password also sign in to the instance. The control plane never sees the password after signup.

## Try the whole thing on one machine

```bash
(cd web/apps/backoffice && vp build) && (cd web/apps/platform && vp build)
cargo run -- platform serve --static web/apps/platform/dist --instance-static web/apps/backoffice/dist
```

Open http://127.0.0.1:8090, create an account, create an instance, follow its link, sign in with the same credentials, then open Modules.

## Sliplane

The SpaceCorps Sliplane account is `spacecorps`; the `obop` project (`project_2x9d8q51m9lt`) exists and is empty. The pieces the Sliplane provisioner needs:

| Variable | Meaning |
|---|---|
| `SLIPLANE_ACCOUNT` | Account name the `sliplane` CLI is configured with (`spacecorps`). |
| `OBOP_SLIPLANE_PROJECT` | Project id for instance services (`project_2x9d8q51m9lt`). |
| `OBOP_SLIPLANE_SERVER` | Server id the services run on. |
| `OBOP_CELL_ADMIN_URL` | `postgres://` URL of a shared PostgreSQL with rights to create roles and databases (one database per business, D6). Create it with `sliplane postgres create`. |
| `OBOP_IMAGE` or `OBOP_REPO` (+ `OBOP_BRANCH`) | Where instance services come from. The image built from the root `Dockerfile` serves both roles. |
| `OBOP_MARKETPLACE_URL` | Optional extra catalog (`index.json` and `packages/<id>.toml`) for every instance. |

Deploy the control plane as one Sliplane service from the same image with the command `obop platform serve --addr 0.0.0.0:8080 --static /srv/platform --provisioner sliplane`, a persistent volume (or a PostgreSQL `--db`) for its own database, and the variables above.

### Before the first real deployment

- **Server capacity.** `spacecorps-main` is 2 vCPU and 2 GB and already runs five services. A Rust release build and every extra instance compete with them. Use a separate server for obop, or build the image elsewhere and pull it with `OBOP_IMAGE`.
- **Cost.** The cell PostgreSQL and any new server are billed resources; none has been created.
- **TLS** terminates at Sliplane's proxy; the instance speaks plain HTTP behind it. Confirm that before relying on it.
- **Migrations.** There is no schema manager yet: records are JSON documents, and installing a newer package version does not migrate existing records.
- **Backups.** Nothing backs up the cell PostgreSQL yet.
