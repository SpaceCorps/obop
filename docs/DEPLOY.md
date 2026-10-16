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
(cd web/apps/backoffice && vp build) && deploy/build-site.sh
cargo run -- platform serve --static web/site --instance-static web/apps/backoffice/dist
```

Open http://127.0.0.1:8090 (the landing page; the app is under `/app/`), create an account, create an instance, follow its link, sign in with the same credentials, then open Modules.

## Sliplane

The SpaceCorps Sliplane account is `spacecorps`. The `obop` project (`project_2x9d8q51m9lt`) holds, on `spacecorps-main`:

| Service | What | Network |
|---|---|---|
| `obop-postgres` | The `postgres:18` Docker image with volume `obop-pgdata`. The cell: one database and one role per business (D6). The superuser password lives in a secret env and in `~/.config/obop/cell.env` on the machine that created it. | internal only, `obop-postgres.internal:5432` |
| `obop-platform` | The control plane, built from the `deploy` branch of this repository with the root `Dockerfile`; its own database is SQLite on volume `obop-platform-data`. | public, `obop-platform.sliplane.app` |
| `obop-<slug>` | One per customer instance, created by the control plane from the same repository and branch, with auto-deploy off. | public, `*.sliplane.app` |

The control plane provisions through the `sliplane` CLI bundled in the image (v1.0.0, pinned by checksum). **It needs one secret you must set**: `SLIPLANE_API_KEY`, a Sliplane API key with write access. The entrypoint registers it under `SLIPLANE_ACCOUNT` (a plaintext 0600 store inside the container, as the CLI allows) and then drops it from the environment. Without the key the control plane still serves signup and sign-in, but creating an instance fails with a clear error.

```bash
sliplane services set-env --project-id project_2x9d8q51m9lt --service-id <obop-platform id> -a spacecorps --key SLIPLANE_API_KEY --value <key> --secret
sliplane services deploy --project-id project_2x9d8q51m9lt --service-id <obop-platform id> -a spacecorps
```

Settings read by `obop platform serve --provisioner sliplane`:

| Variable | Meaning |
|---|---|
| `SLIPLANE_ACCOUNT` | Account name the CLI is registered under. |
| `OBOP_SLIPLANE_PROJECT`, `OBOP_SLIPLANE_SERVER` | Where instance services go. |
| `OBOP_CELL_ADMIN_URL` | `postgres://` superuser URL of the cell, as the control plane reaches it. |
| `OBOP_CELL_INSTANCE_HOST` | `host:port` instances use for the cell (`obop-postgres.internal:5432`). |
| `OBOP_IMAGE` or `OBOP_REPO` (+ `OBOP_BRANCH`) | Where instance services come from. A repository means Sliplane builds each instance itself; an image (once a registry is in place) is much faster. |
| `OBOP_MARKETPLACE_URL` | Optional extra catalog for every instance. |

### Known limits

- **Build time and memory.** Every instance builds the Rust workspace on `spacecorps-main`. That is slow, and it competes with the other services; a registry image would remove it.
- **TLS** terminates at Sliplane's proxy and the instance speaks plain HTTP behind it. Confirm before relying on it. The control plane reaches the cell over the internal network without TLS.
- **Upgrading instances** is manual (auto-deploy is off): `sliplane services deploy` per instance.
- **Migrations.** There is no schema manager yet: records are JSON documents, and installing a newer package version does not migrate existing records.
- **Backups.** Nothing backs up the cell yet.
