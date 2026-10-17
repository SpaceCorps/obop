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

Open http://127.0.0.1:8090 (the website; the sign-up app is under `/app/`), create an account, create an instance, follow its link, sign in with the same credentials, then open Modules.

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
| `OBOP_MARKETPLACE_URL` | Optional extra catalog for every instance: `index.json` plus `packages/<id>.toml` (format and the D55 metadata a listing may carry: docs/MODULES.md section 8). |

### Two-factor authentication and `OBOP_SECRET_KEY` (D53)

Every instance and the control plane need one server secret to protect the two-factor secrets (TOTP seeds) stored in their database. Without it nobody can turn two-factor on, and the "require for administrators" policy is not applied (a policy nobody could satisfy would lock the admins out).

| Variable | Meaning |
|---|---|
| `OBOP_SECRET_KEY` | **Required for two-factor.** 32 random bytes as 64 hex digits (or base64). `openssl rand -hex 32`. Never put it in the database, the repository or a plain env var; set it as a secret env. Changing or losing it makes enrolled users unable to use their authenticator codes until an operator runs `obop admin reset-mfa USER` for them (recovery codes cannot be verified either, because the account's second factor cannot be read). There is no rotation yet. |
| `OBOP_REQUIRE_MFA_FOR_ADMINS` | `0` changes the *default* of the setting `security.require_mfa_for_admins` to off (the production default is on). A value an administrator stored under Customize > Security wins. The test suite sets it to `0`. |
| `OBOP_DEV_INSECURE_KEY` | `1` selects a public, fixed key and logs a warning. Development and tests only. |
| `OBOP_MFA_ISSUER` | The name shown in the authenticator app (default `obop`). |

The Sliplane provisioner generates a fresh random `OBOP_SECRET_KEY` for each instance and passes it with `--secret-env` when it creates the service; the control plane does not store it, so it exists only in that service's secret environment. If you recreate an instance's service by hand, set the same key again (`sliplane services set-env ... --key OBOP_SECRET_KEY --value <hex> --secret`) or enrolled people lose their authenticator. The control plane itself (`obop-platform`) needs its own `OBOP_SECRET_KEY` secret for its optional account two-factor; without it the Security button reports that two-factor is not available. The `local` provisioner passes the environment of the control plane to its child processes, so one key serves them all on a developer machine.

**Break-glass.** A person who lost the device and the recovery codes is reset by an administrator in Customize > Security, or, when no administrator can sign in, by the operator with database access:

```bash
obop admin reset-mfa alice --db postgres://...      # or sqlite://obop.db
```

It removes the second factor, revokes the person's sessions and writes an `operator` record to the security log (`security_event`). The next sign-in is password-only, and if the policy covers the person they are asked to set two-factor up again.

### Known limits

- **Build time and memory.** Every instance builds the Rust workspace on `spacecorps-main`. That is slow, and it competes with the other services; a registry image would remove it.
- **TLS** terminates at Sliplane's proxy and the instance speaks plain HTTP behind it. Confirm before relying on it. The control plane reaches the cell over the internal network without TLS.
- **Upgrading instances** is manual (auto-deploy is off): `sliplane services deploy` per instance.
- **Migrations.** There is no schema manager yet: records are JSON documents, and installing a newer package version does not migrate existing records.
- **Backups.** Nothing backs up the cell yet.
