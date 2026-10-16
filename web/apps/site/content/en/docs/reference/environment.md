---
title: "Environment variables"
description: "The environment variables obop reads: database URL, agent token, marketplace URL, and the settings of the control plane and Sliplane provisioner."
---

obop reads a small number of environment variables. Command line flags win over variables.

## Instance and CLI

| Variable | Meaning | Default |
|---|---|---|
| `OBOP_DATABASE_URL` | Where the data lives: `sqlite://obop.db`, `sqlite:///abs/path.db`, `postgres://user:pass@host/db` or `memory:`. Same as `--db`. | `sqlite://obop.db` |
| `OBOP_TOKEN` | Bearer token for `obop mcp`, `obop call` and `obop commands`. Same as `--token`. | none |
| `OBOP_MARKETPLACE_URL` | An extra marketplace: a host serving `index.json` and `packages/<id>.toml`. Shown after the bundled catalog. | none |

## Control plane

Used by `obop platform serve`.

| Variable | Meaning |
|---|---|
| `OBOP_DATA_DIR` | Where the `local` provisioner keeps instance data. Default `obop-instances`. |
| `OBOP_IMAGE` or `OBOP_REPO` (with `OBOP_BRANCH`, default `main`) | Where instance services come from with the `sliplane` provisioner. A repository means Sliplane builds each instance; an image is faster. |
| `OBOP_MARKETPLACE_URL` | Optional extra catalog for every instance. |

## Sliplane provisioner

`--provisioner sliplane` needs:

| Variable | Meaning |
|---|---|
| `SLIPLANE_ACCOUNT` | Account name the bundled Sliplane CLI is registered under. |
| `SLIPLANE_API_KEY` | A Sliplane API key with write access. The container entrypoint registers it with the CLI and removes it from the environment. Set it as a secret. |
| `OBOP_SLIPLANE_PROJECT`, `OBOP_SLIPLANE_SERVER` | Where instance services go. |
| `OBOP_CELL_ADMIN_URL` | `postgres://` superuser URL of the shared PostgreSQL server, as the control plane reaches it. |
| `OBOP_CELL_INSTANCE_HOST` | `host:port` that instances use to reach that server, for example `obop-postgres.internal:5432`. |

## Build time (the website)

| Variable | Meaning |
|---|---|
| `OBOP_SITE_URL` | Public origin used for canonical links, the sitemap and social cards when building this website. Default: `https://obop-platform.sliplane.app`. |

## Tests

`OBOP_TEST_POSTGRES_URL` adds PostgreSQL to `cargo test`. `OBOP_BIN` points the end-to-end suite at a built binary.
