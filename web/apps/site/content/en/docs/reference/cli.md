---
title: "CLI reference"
description: "Every obop command line command and option: serve, platform serve, mcp, call, commands, admin create, agent-token, modules, and operating a database: migrate, backup, restore, secrets rotate and upgrade-instances."
---

The `obop` binary has no subcommand help beyond one usage page (`obop --help`). This page restates it, with what each command does.

```text
obop <command> [options]
```

## Global options

| Option       | Meaning                                                                                                                                                                                          |
| ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `--db URL`   | Where the data lives. `sqlite://obop.db` (default), `sqlite:///absolute/path.db`, `postgres://user:pass@host/db`, or `memory:` (gone when the process ends). Also read from `OBOP_DATABASE_URL`. |
| `--demo`     | Add fake demo data. It is added automatically when a SQLite database is empty.                                                                                                                   |
| `--dev-auth` | Use fixed tokens `dev-admin`, `dev-sales` and `dev-agent` instead of real sign-in. Refuses to bind to anything but localhost. For development only.                                              |
| `--version`  | Print the version.                                                                                                                                                                               |

## Serving

| Command                                                                                                       | What it does                                                                                                                                                               |
| ------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `obop serve [--addr 127.0.0.1:8080] [--static DIR]`                                                           | Runs one instance: the REST API under `/api/` and, with `--static`, a directory of static files (the back office build). Unknown paths fall back to `DIR/index.html`.      |
| `obop platform serve [--addr A] [--static DIR] [--provisioner local\|sliplane\|fake] [--instance-static DIR]` | Runs the control plane: sign-up, sign-in and instance management, plus the static website at `/`. `--instance-static` is the back office build that local instances serve. |
| `obop mcp [--token TOKEN]`                                                                                    | MCP server on standard input and output for one token. Token from `--token` or `OBOP_TOKEN`. See [Connect over MCP](/docs/agents/mcp/).                                    |

## Calling commands

| Command                                                                                 | What it does                                                                                                                                                                                                                                                                                                                                                                                                           |
| --------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `obop commands [--token TOKEN]`                                                         | Lists the commands this token may use, with a note on those that need approval for agents.                                                                                                                                                                                                                                                                                                                             |
| `obop import <model> FILE.csv [--token TOKEN] [--mode partial] [--dry-run] [--key KEY]` | Creates many records from a CSV file through `bulk_create`: the header names the fields, references are ids, numbers are whole numbers, decimals and date-times are text (`12.50`, `2026-03-29T03:30:00+02:00`), at most 1,000 rows and 1 MiB. One bad row creates nothing unless `--mode partial`; `--dry-run` only checks; the same `--key` again returns the first answer. Exits non-zero if any row has a problem. |
| `obop call [--token TOKEN] <model> <command> [key=value ...]`                           | Runs one command and prints the result as JSON. Values are converted by field type. Structured values such as `filter` and `order` are JSON text.                                                                                                                                                                                                                                                                      |

## Users and tokens

| Command                                                 | What it does                                                                                                                                                                                                                                                                                                                                        |
| ------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `obop admin create USER [--groups a,b \| --all-groups]` | Creates a user. The password is read from standard input and must have at least 12 characters. `--all-groups` grants `platform.admin` and every built-in group.                                                                                                                                                                                     |
| `obop admin users`                                      | Lists the accounts: status (active, must change password, disabled, offboarded), two-factor, open sessions and groups.                                                                                                                                                                                                                              |
| `obop admin groups USER [--groups a,b]`                 | Shows the groups of a user, or replaces them. Only groups the installed modules declare are accepted (a group the person already holds may stay). The last administrator keeps `platform.admin`. Recorded in the security log with the groups before and after.                                                                                     |
| `obop admin offboard USER [--note TEXT]`                | Disables the account and ends for good every session, agent token and connected app it had. Enabling it later does not bring them back.                                                                                                                                                                                                             |
| `obop admin disable USER`, `obop admin enable USER`     | Disables or enables an account (an offboarded account is enabled the same way).                                                                                                                                                                                                                                                                     |
| `obop admin import FILE.csv [--dry-run] [--out FILE]`   | Creates many users from a CSV with the columns `name` and `groups` (comma, semicolon, bar or space between groups). All rows are checked first and one bad row creates nothing. Each user gets a generated temporary password to change at the first sign-in, printed once or written to `FILE` (mode 0600, never overwritten). At most 1,000 rows. |
| `obop admin reset-mfa USER --db URL`                    | Break-glass: removes a user's two-factor (authenticator app and passkeys), revokes their sessions and writes an `operator` record to the security log. Needs database access. See [Two-factor authentication](/docs/concepts/two-factor-authentication/).                                                                                           |
| `obop agent-token USER --scopes a,b [--label TEXT]`     | Issues an agent token for an existing user. Shown once. Scopes must be groups the user holds.                                                                                                                                                                                                                                                       |

Example:

```bash
echo "a password of 12+ characters" | obop admin create alice --groups crm.user,sales.user
obop agent-token alice --scopes crm.user --label triage-bot
```

## Modules

| Command                                 | What it does                                                                                                                                                                                         |
| --------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `obop modules check [FILE...]`          | Runs the install gate. Without files, over the built-in modules. With files, over each `obop.toml`, without a database. Prints `ok  FILE` or `FAIL  FILE: reason` and exits non-zero on any failure. |
| `obop modules list`                     | Lists installed modules with their source (built in, marketplace or upload).                                                                                                                         |
| `obop modules market`                   | Lists the marketplace: the bundled catalog plus any extra catalog from `OBOP_MARKETPLACE_URL`.                                                                                                       |
| `obop modules install-from ID`          | Installs a package from the marketplace.                                                                                                                                                             |
| `obop modules install FILE`             | Installs or upgrades from a local `obop.toml`. The version must be higher to upgrade.                                                                                                                |
| `obop modules uninstall ID`             | Removes the package's commands and screens. Its data stays in the database. Refused while another installed module depends on it.                                                                    |
| `obop modules updates`                  | Lists installed packages with a newer version on offer and whether each can be updated here.                                                                                                         |
| `obop modules update ID` / `update-all` | Updates one package (or every one that can be) in one atomic operation, with the data steps the new version declares. `obop modules plan ID` shows the changes first.                                |
| `obop modules rollback ID`              | Puts a package back to the version before its last update, when the update did not delete data.                                                                                                      |

These are operator tools: they write to the database directly. Inside an instance, administrators do the same from _Customize_.

## Updating the instance

| Command                                                                                        | What it does                                                                                                                                                                                                                                                  |
| ---------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `obop update check`                                                                            | Reads the release feed (`OBOP_UPDATE_FEED`), says which releases are newer, whether every installed module fits each (their declared core ranges), and for a self-hosted instance prints the commands to run. Exits non-zero when a newer release is blocked. |
| `obop update status`                                                                           | The state of the last update request.                                                                                                                                                                                                                         |
| `obop update start --to VERSION --yes [--skip-backup]`                                         | Managed instances: takes a backup (`OBOP_BACKUP_DIR`), then asks the control plane to upgrade this instance to a published release.                                                                                                                           |
| `obop update rollback --yes`                                                                   | Asks the control plane to put the previous version back.                                                                                                                                                                                                      |
| `obop platform release publish\|list\|withdraw`                                                | Control plane: the releases instances read at `/api/releases`.                                                                                                                                                                                                |
| `obop platform register-instance SLUG --url URL`, `obop platform update-token SLUG [--revoke]` | Control plane: register an instance it did not create, issue or revoke an update token.                                                                                                                                                                       |

Inside an instance the same is _Customize_, _Updates_, and the commands `instance_update.check`, `status`, `start` and `rollback` (an agent may only propose the last two).

## Operating a database

These commands work on a database given by `--db` (or `OBOP_DATABASE_URL`) and need no running server. The operator guide, with the procedures around them, is `docs/DEPLOY.md`.

| Command                                                                                          | What it does                                                                                                                                                                                                                                                                                |
| ------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `obop migrate status`                                                                            | Prints the schema version in the database, the version this build writes and the steps still pending.                                                                                                                                                                                       |
| `obop migrate plan` / `obop migrate run [--dry-run]`                                             | `plan` (the same as `run --dry-run`) says what each pending step would do; `run` applies them in order, under a database lock. Take a backup first.                                                                                                                                         |
| `obop backup --db URL --out DIR\|FILE`                                                           | Writes a consistent copy of every record while the server keeps running: gzip JSON Lines plus a manifest with checksums, the schema version and the core version. A directory gets a timestamped file; a path ending in `.gz` is the file. Secrets stay sealed as stored. Never overwrites. |
| `obop backup verify FILE`                                                                        | Re-reads a backup: checksum of the file, gzip integrity, record shape, counts per model, checksum of the records. Exits non-zero on any damage.                                                                                                                                             |
| `obop restore --db URL --from FILE [--force] [--dry-run]`                                        | Verifies, then restores in one transaction into an empty database. A non-empty target is refused unless `--force` (which replaces its contents). A backup from a newer schema or an unknown format is refused. `--dry-run` checks everything and writes nothing.                            |
| `obop secrets status`                                                                            | Lists the key ids in `OBOP_SECRET_KEY` and `OBOP_SECRET_KEY_PREVIOUS` and how many stored secrets each protects. Never prints a key.                                                                                                                                                        |
| `obop secrets rotate [--dry-run] [--batch N] [--max-batches N] [--skip-unopenable]`              | Re-seals every stored secret under the current key. Safe while serving, resumable, stops before writing anything if some value opens with no key. Needs `OBOP_SECRET_KEY` (and `OBOP_SECRET_KEY_PREVIOUS` for the earlier keys).                                                            |
| `obop platform upgrade-instances --image TAG [--only SLUG] [--dry-run] [--provisioner sliplane]` | Control plane: moves every running instance to a published image, oldest first, one at a time, each after the previous one answers a health check; stops at the first failure. `TAG` is a commit tag of `OBOP_IMAGE`'s repository or a full image reference; `--only` picks one instance.   |

`obop serve` refuses a database that needs migration steps unless it is started with `--auto-migrate` (or `OBOP_AUTO_MIGRATE=1`); the default is on for SQLite and off for PostgreSQL. It always refuses a database newer than itself.

## Exit status

`0` on success. Anything else prints a line starting with `error:` on standard error. A command that needs approval is reported as `error: ApprovalRequired { plan_id: ... }`.

## Defaults worth knowing

- The instance listens on `127.0.0.1:8080` unless you pass `--addr`.
- Sign-in sessions last 12 hours. Five wrong passwords lock a username for a minute.
- The CLI prints `database: sqlite` (or the backend name) on standard error at start.
