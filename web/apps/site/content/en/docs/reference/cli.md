---
title: "CLI reference"
description: "Every obop command line command and option: serve, platform serve, mcp, call, commands, admin create, agent-token, modules check, install, uninstall and market."
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

| Command                                                       | What it does                                                                                                                                      |
| ------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| `obop commands [--token TOKEN]`                               | Lists the commands this token may use, with a note on those that need approval for agents.                                                        |
| `obop call [--token TOKEN] <model> <command> [key=value ...]` | Runs one command and prints the result as JSON. Values are converted by field type. Structured values such as `filter` and `order` are JSON text. |

## Users and tokens

| Command                                                 | What it does                                                                                                                                                    |
| ------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `obop admin create USER [--groups a,b \| --all-groups]` | Creates a user. The password is read from standard input and must have at least 12 characters. `--all-groups` grants `platform.admin` and every built-in group. |
| `obop admin reset-mfa USER --db URL` | Break-glass: removes a user's two-factor, revokes their sessions and writes an `operator` record to the security log. Needs database access. See [Two-factor authentication](/docs/concepts/two-factor-authentication/). |
| `obop agent-token USER --scopes a,b [--label TEXT]`     | Issues an agent token for an existing user. Shown once. Scopes must be groups the user holds.                                                                   |

Example:

```bash
echo "a password of 12+ characters" | obop admin create alice --groups crm.user,sales.user
obop agent-token alice --scopes crm.user --label triage-bot
```

## Modules

| Command                        | What it does                                                                                                                                                                                         |
| ------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `obop modules check [FILE...]` | Runs the install gate. Without files, over the built-in modules. With files, over each `obop.toml`, without a database. Prints `ok  FILE` or `FAIL  FILE: reason` and exits non-zero on any failure. |
| `obop modules list`            | Lists installed modules with their source (built in, marketplace or upload).                                                                                                                         |
| `obop modules market`          | Lists the marketplace: the bundled catalog plus any extra catalog from `OBOP_MARKETPLACE_URL`.                                                                                                       |
| `obop modules install-from ID` | Installs a package from the marketplace.                                                                                                                                                             |
| `obop modules install FILE`    | Installs or upgrades from a local `obop.toml`. The version must be higher to upgrade.                                                                                                                |
| `obop modules uninstall ID`    | Removes the package's commands and screens. Its data stays in the database. Refused while another installed module depends on it.                                                                    |

These are operator tools: they write to the database directly. Inside an instance, administrators do the same from _Customize_.

## Exit status

`0` on success. Anything else prints a line starting with `error:` on standard error. A command that needs approval is reported as `error: ApprovalRequired { plan_id: ... }`.

## Defaults worth knowing

- The instance listens on `127.0.0.1:8080` unless you pass `--addr`.
- Sign-in sessions last 12 hours. Five wrong passwords lock a username for a minute.
- The CLI prints `database: sqlite` (or the backend name) on standard error at start.
