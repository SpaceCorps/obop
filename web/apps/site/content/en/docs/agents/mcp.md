---
title: "Connect over MCP"
description: "Connect Claude Desktop, Claude Code, Cursor, VS Code, Windsurf, Codex or Gemini CLI to obop with the Model Context Protocol: exact configuration for each client, using obop mcp over stdio."
lead: "obop ships an MCP server. Every command the token may use becomes an MCP tool. This page gives the configuration for the common clients."
---

## How it works

`obop mcp` is an MCP server that talks JSON-RPC over standard input and output. An MCP client starts it as a child process. It:

- opens the instance's database itself, so it must run on a machine that can reach the database (SQLite file or PostgreSQL URL);
- identifies the agent by an agent token;
- lists one tool per command that token may use, named `<model>__<command>`: `lead__search`, `lead__create`, `invoice__post`. The double underscore stands where the REST path has a slash;
- answers `tools/call` through the same dispatcher as REST, so masking and approvals apply.

> **Hosted instances.** There is no remote (HTTP) MCP transport yet, and `obop mcp` needs the database. You cannot point a desktop MCP client at a hosted instance today. For hosted instances use [REST](/docs/agents/rest/). MCP over HTTP with OAuth is planned.

## Before you start

1. A running instance database, and the `obop` binary on the machine that runs the MCP client (or use its absolute path).
2. An [agent token](/docs/agents/tokens/) with the scopes you want.
3. The database address, as `OBOP_DATABASE_URL` or `--db`. The default is `sqlite://obop.db` in the current directory, so give an absolute path: `sqlite:///home/me/obop.db`.

The command every client runs is:

```bash
obop mcp --db sqlite:///home/me/obop.db --token obop_...
```

It reads `OBOP_TOKEN` and `OBOP_DATABASE_URL` from the environment if the flags are absent. Prefer the environment: it keeps the token out of process listings.

Check it by hand first. This sends an `initialize` and a `tools/list` request and prints the answers:

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  | OBOP_TOKEN=obop_... OBOP_DATABASE_URL=sqlite:///home/me/obop.db obop mcp
```

The server reports protocol version `2025-06-18` and supports `initialize`, `ping`, `tools/list` and `tools/call`. Tool input schemas are only `{"type":"object"}` today, so a client cannot validate arguments; the arguments are the same JSON as the [REST body](/docs/reference/rest-api/).

The snippets below use `/usr/local/bin/obop` and a placeholder token. Replace both. Formats were checked against each client's documentation on 2026-10-16; where something could not be confirmed it is marked.

## Claude Desktop

Open _Settings_, then _Developer_, then _Edit Config_. The file is `~/Library/Application Support/Claude/claude_desktop_config.json` on macOS and `%APPDATA%\Claude\claude_desktop_config.json` on Windows. Add:

```json
{
  "mcpServers": {
    "obop": {
      "command": "/usr/local/bin/obop",
      "args": ["mcp"],
      "env": {
        "OBOP_TOKEN": "obop_your_agent_token",
        "OBOP_DATABASE_URL": "sqlite:///Users/me/obop.db"
      }
    }
  }
}
```

Quit and restart Claude Desktop. Claude asks your approval before it uses a tool.

## Claude Code

Use the command line. Everything after `--` is the server command:

```bash
claude mcp add --transport stdio \
  --env OBOP_TOKEN=obop_your_agent_token \
  --env OBOP_DATABASE_URL=sqlite:///home/me/obop.db \
  obop -- /usr/local/bin/obop mcp
```

Put at least one other option (here `--transport stdio`) between the last `--env` and the server name. Add `--scope project` to write a shared `.mcp.json` instead, `--scope user` to use it in all projects. A project file looks like this; the `${...}` form reads the value from your environment so the token is not committed:

```json
{
  "mcpServers": {
    "obop": {
      "type": "stdio",
      "command": "/usr/local/bin/obop",
      "args": ["mcp"],
      "env": {
        "OBOP_TOKEN": "${OBOP_TOKEN}",
        "OBOP_DATABASE_URL": "sqlite:///home/me/obop.db"
      }
    }
  }
}
```

Check it with `claude mcp get obop` or `/mcp` inside Claude Code.

## Cursor

Create `.cursor/mcp.json` in a project, or `~/.cursor/mcp.json` for every project:

```json
{
  "mcpServers": {
    "obop": {
      "type": "stdio",
      "command": "/usr/local/bin/obop",
      "args": ["mcp"],
      "env": {
        "OBOP_TOKEN": "${env:OBOP_TOKEN}",
        "OBOP_DATABASE_URL": "sqlite:///Users/me/obop.db"
      }
    }
  }
}
```

Cursor's own examples omit `type` for stdio servers; its field table lists it. `${env:NAME}` reads from the environment Cursor was started in.

## VS Code

Create `.vscode/mcp.json`. VS Code's top-level key is `servers`, not `mcpServers`, and it can prompt for the token so it is never written to disk:

```json
{
  "inputs": [
    {
      "type": "promptString",
      "id": "obop-token",
      "description": "obop agent token",
      "password": true
    }
  ],
  "servers": {
    "obop": {
      "type": "stdio",
      "command": "/usr/local/bin/obop",
      "args": ["mcp"],
      "env": {
        "OBOP_TOKEN": "${input:obop-token}",
        "OBOP_DATABASE_URL": "sqlite:///Users/me/obop.db"
      }
    }
  }
}
```

VS Code's guided _MCP: Add Server_ command may suggest a portable `.mcp.json` with the `mcpServers` format instead; both are accepted according to its documentation.

## Windsurf

Windsurf's documentation has moved to the Devin Desktop site and describes the file as `mcp_config.json`. We could not confirm the current file location for Windsurf itself. Windsurf historically used `~/.codeium/windsurf/mcp_config.json`; the Devin Desktop page gives `~/.config/devin/mcp_config.json` (macOS and Linux) and `%APPDATA%\devin\mcp_config.json` (Windows). Check the location in your version's settings. The format is the same `mcpServers` shape:

```json
{
  "mcpServers": {
    "obop": {
      "command": "/usr/local/bin/obop",
      "args": ["mcp"],
      "env": {
        "OBOP_TOKEN": "${env:OBOP_TOKEN}",
        "OBOP_DATABASE_URL": "sqlite:///Users/me/obop.db"
      }
    }
  }
}
```

## Codex CLI

Add a table to `~/.codex/config.toml` (or `.codex/config.toml` in a trusted project), or run `codex mcp add`:

```toml
[mcp_servers.obop]
command = "/usr/local/bin/obop"
args = ["mcp"]
env_vars = ["OBOP_TOKEN"]

[mcp_servers.obop.env]
OBOP_DATABASE_URL = "sqlite:///home/me/obop.db"
```

`env_vars` forwards `OBOP_TOKEN` from your shell instead of writing it into the file.

## Gemini CLI

Add to `~/.gemini/settings.json`:

```json
{
  "mcpServers": {
    "obop": {
      "command": "/usr/local/bin/obop",
      "args": ["mcp"],
      "env": {
        "OBOP_TOKEN": "$OBOP_TOKEN",
        "OBOP_DATABASE_URL": "sqlite:///home/me/obop.db"
      }
    }
  }
}
```

Gemini CLI's documentation says it was replaced by another tool in 2026, so check that page for your version. We have not run obop against it.

## Any other MCP client

obop is a plain stdio server. Whatever the client's file looks like, it needs a `command` (`obop`), `args` (`["mcp"]`) and two environment variables. If the client only supports remote servers, it cannot connect yet.

## What the agent sees

After connecting, ask the agent to list its tools. You should see only the commands its scopes allow, with the suffix "(agents propose; a human approves)" on those that need approval. Try a read first, for example _"List the three newest leads"_, then see [The approval loop](/docs/agents/approvals/).

If the client shows no tools or an error, go to [Troubleshooting](/docs/agents/troubleshooting/).

## Verified and not verified

| Item                                                                                      | Status                                                                 |
| ----------------------------------------------------------------------------------------- | ---------------------------------------------------------------------- |
| `obop mcp` handshake, `tools/list`, `tools/call`, approval result                         | Run against a real local instance.                                     |
| Configuration formats for Claude Desktop, Claude Code, Cursor, VS Code, Codex, Gemini CLI | Taken from each client's documentation. We did not start every client. |
| Windsurf file location                                                                    | Unconfirmed, see above.                                                |
