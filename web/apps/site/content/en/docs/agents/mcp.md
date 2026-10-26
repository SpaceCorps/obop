---
title: "Connect over MCP"
description: "Connect Claude Code, Cursor, VS Code, Windsurf, Codex or Gemini CLI to an obop instance by URL with the remote MCP server (Streamable HTTP, bearer token), or run obop mcp over stdio. Exact configuration per client."
lead: "Every obop instance has an MCP server at /mcp. Point a client at the URL with an agent token and every command the token may use becomes an MCP tool: no local binary needed."
---

## Remote MCP: connect by URL

An instance serves the [Model Context Protocol](https://modelcontextprotocol.io/) over **Streamable HTTP** at `https://YOUR-INSTANCE/mcp`. A hosted instance (for example on Sliplane) and a local `obop serve` both have it; there is nothing to switch on.

- **Authentication:** a browser sign-in (OAuth, [below](#connect-with-a-browser-sign-in-oauth)) for Claude Desktop, claude.ai and Claude Code, or `Authorization: Bearer <token>` on every request with an agent token. Either way the token is checked as for REST. A missing or bad token gets `401` with `WWW-Authenticate: Bearer resource_metadata="https://YOUR-INSTANCE/.well-known/oauth-protected-resource/mcp"`, as the MCP authorization specification asks; that document names the authorization server of the instance (`/.well-known/oauth-authorization-server`). A token issued by the sign-in works on `/mcp` only, not on `/api`.
- **Protocol revisions:** `2025-11-25`, `2025-06-18` and `2025-03-26`, negotiated in `initialize`. The newest revision, `2026-07-28`, is stateless and works differently; obop does not implement it yet. A client that speaks both falls back to `initialize` when it sees a plain `400`, which is what obop sends for an unsupported version header.
- **Sessions:** `initialize` returns an `Mcp-Session-Id`; send it on every later request. The session keeps the masking vault, so a mask token such as `NAME_1` means the same thing in every call of the session and you can send it back. A different session starts a new vault. `DELETE /mcp` ends a session; an unknown or expired one answers `404`, and the client starts again with `initialize`. A session belongs to the token that opened it and expires after two hours without a request.
- **Answers are plain JSON.** There is no server-to-client SSE stream (`GET /mcp` answers `405`), and JSON-RPC batches are accepted only on `2025-03-26`, the one revision that has them.
- **Limits:** request bodies up to 1 MiB (`413`), 600 requests per minute per token (`429` with `Retry-After`, tunable with `OBOP_MCP_RATE_PER_MINUTE`), 25 open sessions per token.
- **When the database is busy:** a `tools/call` that meets a refusal of the database is answered HTTP `503` with `Retry-After` and a JSON-RPC error `-32000` whose `data.applied` says what became of the call (`none`: nothing was changed, repeat it; `unknown`: after `storage_unavailable`, read before you repeat a write; `partial`: part of the call was applied and a step it needed was refused, do not repeat it blindly, read the records it touched). It is never an HTTP 200 tool error, so a client that retries only 503s retries the right things. The local stdio server (`obop mcp`) has no HTTP status: it answers the same refusal as a tool error whose structured content has `code` (`storage_busy`, `storage_unavailable` or `storage_timeout`), `applied`, `hint` and `retry_after` (seconds).
- **Browsers:** a request with an `Origin` header is refused with `403` unless the origin is the instance's own public URL (`OBOP_PUBLIC_URL`), the same origin on a loopback host, or listed in `OBOP_ALLOWED_ORIGINS` (comma separated). This protects local instances against DNS rebinding. Clients that are not browsers send no `Origin`. Behind a proxy set `OBOP_PUBLIC_URL` (for example `https://acme.example.com`) so the discovery documents name the public address.

### Connect with a browser sign-in (OAuth)

Claude Desktop, claude.ai and Claude Code can connect to an instance without a pasted token. In the app add a custom connector with the address `https://YOUR-INSTANCE/mcp` (Claude Code: `claude mcp add --transport http obop https://YOUR-INSTANCE/mcp`, then `/mcp` and choose to authenticate). The app finds the sign-in by itself and opens a page of **your instance** in the browser. There you:

1. sign in with your obop account, and enter your two-factor code if the account has one (a passkey works too, as the second step, or on its own if you allowed that in Security);
2. see which application asks and to which address the browser will return;
3. tick the permission groups to hand over, choose how long the access lasts (1, 7, 30 or 90 days) and press **Allow**.

What this means:

- **The app can only narrow you.** You are offered the groups you hold, and the app acts as you with the ones you tick, never with more ([agent tokens](/docs/agents/tokens/) work the same way). Its actions are recorded as those of your agent.
- **Approvals still need a person.** A step that needs approval waits for a human in obop; the app cannot approve its own proposals.
- **Short-lived tokens.** The app receives an access token that lasts one hour and a refresh token that is used once and replaced each time. If a refresh token is ever used twice, the whole connection is ended. An access token from this sign-in is for `/mcp` only.
- **You stay in control.** _Customize, Security, Connected apps_ lists every connection with its groups and end date, and **Revoke** ends it at once. Connecting, refusing, revoking and a reused token are in the security log.
- **Nothing is trusted in advance.** Any application may register itself, so the page shows the name it chose and the address it will return to, and says that obop cannot verify who runs it. Redirect addresses must be `https` (or `http` on `localhost`, `127.0.0.1` or `[::1]`), registrations are limited and unused ones expire.

Protocol, for client authors: RFC 9728 and RFC 8414 metadata, dynamic client registration (RFC 7591, public clients), authorization code with PKCE **S256** (mandatory), the `resource` parameter (RFC 8707, it must be the `/mcp` address of the instance), refresh token rotation with reuse detection, revocation (RFC 7009), and the `iss` parameter on the redirect (RFC 9207). Endpoints: `/oauth/register`, `/oauth/authorize`, `/oauth/token`, `/oauth/revoke`. A loopback redirect (`http://127.0.0.1:PORT/...`) may use any port, as RFC 8252 asks; every other redirect must match exactly. An instance behind a proxy needs `OBOP_PUBLIC_URL` so that the addresses in these documents are the public ones. We built this against the specification and tested it with our own client in automated tests; we have **not yet connected the real Claude apps** to it.

### Tools

`tools/list` shows one tool per command the token may use, named `<model>__<command>`: `party__search`, `lead__create`, `invoice__post`. Each has a JSON Schema `inputSchema` generated from the model: money and number fields are integers, dates are `format: date`, selects are an `enum` of their options, a ref field is a string that names its target model, and required fields are in `required`. `list` and `search` take `filter`, `order`, `limit` and `offset`; `get`, `delete` and actions take an `id`. Annotations mark reads (`readOnlyHint`) and delete (`destructiveHint`). One extra read-only tool, `obop__meta`, returns what `GET /api/meta` returns: start there. A tool the token may not use is not listed and answers like a tool that does not exist.

Results have `structuredContent` and the same JSON as text:

```json
{ "status": "ok", "value": [{ "id": "...", "name": "NAME_1" }], "denied": ["party.tax_id"] }
```

- A step that needs a human (every delete, posting an invoice) does not run for an agent. The result is a normal one with `"status": "approval_required"`, a `plan_id` and an instruction to ask a human, who approves it in the back office or with `POST /api/approve/<plan_id>`. See [The approval loop](/docs/agents/approvals/).
- A refused input (a bad value, a missing field) is a tool error: `isError: true` and the server's message in `structuredContent.error`.
- `initialize` also returns `instructions`, a short text that tells the model how to use the instance.

`tools/list` is paged with a `cursor` only when a token sees more than 200 tools. The list does not announce changes (`listChanged` is `false`): after installing a module, start a new session.

### Check it with curl

```bash
export OBOP_URL=https://YOUR-INSTANCE
export OBOP_TOKEN=obop_...
curl -si -X POST "$OBOP_URL/mcp" \
  -H "Authorization: Bearer $OBOP_TOKEN" \
  -H 'Content-Type: application/json' -H 'Accept: application/json, text/event-stream' \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"curl","version":"0"}}}'
```

Copy the `mcp-session-id` header of the answer, then send `{"jsonrpc":"2.0","method":"notifications/initialized"}` (answer `202`), then `{"jsonrpc":"2.0","id":2,"method":"tools/list"}`, both with `-H "Mcp-Session-Id: <id>"`.

### Remote client configuration

Replace `https://YOUR-INSTANCE/mcp` and keep the token in an environment variable or a secret store. The formats below were taken from each client's documentation on 2026-10-17. We ran obop only against the MCP Inspector command line client (`vp dlx @modelcontextprotocol/inspector --cli URL --transport http --header "Authorization: Bearer ..." --method tools/list`) and our own Playwright tests. We did not start the other clients, so each snippet is **documented, not tried**.

#### Claude Code

```bash
claude mcp add --transport http obop https://YOUR-INSTANCE/mcp \
  --header "Authorization: Bearer $OBOP_TOKEN"
```

Add `--scope project` to write a shared `.mcp.json` or `--scope user` to use it in all projects. In a shared file read the token from the environment so it is not committed:

```json
{
  "mcpServers": {
    "obop": {
      "type": "http",
      "url": "https://YOUR-INSTANCE/mcp",
      "headers": { "Authorization": "Bearer ${OBOP_TOKEN}" }
    }
  }
}
```

If the server rejects the header, Claude Code reports a failed connection instead of starting OAuth. Check with `/mcp`. Without a header, `/mcp` offers the browser sign-in described above.

#### Claude Desktop and claude.ai

The apps add remote servers as _custom connectors_ (Settings, Connectors, Add custom connector) and authenticate them with OAuth. Enter `https://YOUR-INSTANCE/mcp` and follow the browser sign-in described [above](#connect-with-a-browser-sign-in-oauth); no token is pasted. The instance must be reachable from the internet over `https` for claude.ai, because Anthropic's servers make the requests. The stdio setup [below](#claude-desktop-stdio) works in Claude Desktop with a local binary. A bridge such as the community `mcp-remote` package can add a header to a remote server for clients that only speak stdio; we have not tested it with obop.

#### Cursor

`.cursor/mcp.json` (project) or `~/.cursor/mcp.json` (all projects):

```json
{
  "mcpServers": {
    "obop": {
      "url": "https://YOUR-INSTANCE/mcp",
      "headers": { "Authorization": "Bearer ${env:OBOP_TOKEN}" }
    }
  }
}
```

#### VS Code

`.vscode/mcp.json`. The top-level key is `servers`, and an input prompts for the token so it is not written to disk:

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
      "type": "http",
      "url": "https://YOUR-INSTANCE/mcp",
      "headers": { "Authorization": "Bearer ${input:obop-token}" }
    }
  }
}
```

VS Code's documentation did not show one complete example with headers and an input, so this snippet is assembled from its field tables.

#### Windsurf

Windsurf's documentation now lives on the Devin Desktop site. It names `~/.config/devin/mcp_config.json` (macOS and Linux) and `%APPDATA%\devin\mcp_config.json` (Windows); older Windsurf installs used `~/.codeium/windsurf/mcp_config.json`. Check which one your version reads. Remote servers use `serverUrl` (or `url`) and `headers`, and `${env:NAME}` is expanded:

```json
{
  "mcpServers": {
    "obop": {
      "serverUrl": "https://YOUR-INSTANCE/mcp",
      "headers": { "Authorization": "Bearer ${env:OBOP_TOKEN}" }
    }
  }
}
```

#### Codex CLI

In `~/.codex/config.toml`. `bearer_token_env_var` names the environment variable that holds the token; Codex sends it as an `Authorization` bearer token:

```toml
[mcp_servers.obop]
url = "https://YOUR-INSTANCE/mcp"
bearer_token_env_var = "OBOP_TOKEN"
```

`codex mcp add` documents `--url` with OAuth options only, so use the file for a bearer token.

#### Gemini CLI

The documentation we read says Gemini CLI was replaced by Antigravity CLI on 2026-06-18; check the page of the tool you use. The Gemini CLI format is `httpUrl` for Streamable HTTP (`url` is for the older SSE transport), in `~/.gemini/settings.json`:

```json
{
  "mcpServers": {
    "obop": {
      "httpUrl": "https://YOUR-INSTANCE/mcp",
      "headers": { "Authorization": "Bearer your-agent-token" }
    }
  }
}
```

or `gemini mcp add --transport http --header "Authorization: Bearer $OBOP_TOKEN" obop https://YOUR-INSTANCE/mcp`. We could not confirm that this file expands environment variables in `headers`, so the token is written out here.

### Discovery

An instance publishes these without a token:

| URL                                       | What                                                                                                                                                                                |
| ----------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `/.well-known/oauth-protected-resource`   | OAuth protected resource metadata (RFC 9728) for `/mcp`, naming the authorization server. Also at `/.well-known/oauth-protected-resource/mcp`, which the `401` challenge points to. |
| `/.well-known/oauth-authorization-server` | OAuth authorization server metadata (RFC 8414): the endpoints, PKCE `S256` only, public clients only.                                                                               |
| `/.well-known/mcp/server-card.json`       | MCP server card: name, version, the `streamable-http` endpoint, protocol versions, the `Authorization` header. Also at `/mcp/server-card`.                                          |
| `/.well-known/ai-catalog.json`            | A one-entry AI Catalog that points at the server card.                                                                                                                              |
| `/api/openapi.json`, `/api/meta`          | The OpenAPI document has an `x-mcp` field with the endpoint, transport and protocol versions; `GET /api/meta` carries a `Link` header with `rel="mcp"`.                             |

The server card is an extension of the MCP specification that is still being developed (SEP-2127, kept in an incubation repository), so its shape may change; the file says so in its `$comment`. It also carries plain `serverInfo`, `transport`, `endpoint` and `capabilities` fields from an earlier draft, for scanners that look for them.

## MCP over stdio (local binary)

Use this when the client can only start a local program, or when you run obop on the same machine. The remote server above needs none of it.

### How it works

`obop mcp` is an MCP server that talks JSON-RPC over standard input and output. An MCP client starts it as a child process. It:

- opens the instance's database itself, so it must run on a machine that can reach the database (SQLite file or PostgreSQL URL);
- identifies the agent by an agent token;
- lists the same tools, with the same schemas, as the remote server (both transports run one code path): one tool per command that token may use, named `<model>__<command>`. The double underscore stands where the REST path has a slash;
- answers `tools/call` through the same dispatcher as REST, so masking and approvals apply. The mask tokens are stable for as long as the process runs.

### Before you start

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
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  | OBOP_TOKEN=obop_... OBOP_DATABASE_URL=sqlite:///home/me/obop.db obop mcp
```

The server answers with the revision the client asks for if it is one of `2025-11-25`, `2025-06-18` and `2025-03-26`, and supports `initialize`, `ping`, `tools/list` and `tools/call`. The arguments of a tool are the same JSON as the [REST body](/docs/reference/rest-api/).

The snippets below use `/usr/local/bin/obop` and a placeholder token. Replace both. Formats were checked against each client's documentation on 2026-10-16; where something could not be confirmed it is marked.

### Claude Desktop (stdio)

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

### Claude Code (stdio)

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

### Cursor (stdio)

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

### VS Code (stdio)

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

### Windsurf (stdio)

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

### Codex CLI (stdio)

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

### Gemini CLI (stdio)

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

### Any other MCP client

obop is a plain stdio server. Whatever the client's file looks like, it needs a `command` (`obop`), `args` (`["mcp"]`) and two environment variables. If the client only supports remote servers, use the remote server above.

## What the agent sees

After connecting, ask the agent to list its tools. You should see only the commands its scopes allow, with a note on those that need approval. Try a read first, for example _"List the three newest leads"_, then see [The approval loop](/docs/agents/approvals/).

If the client shows no tools or an error, go to [Troubleshooting](/docs/agents/troubleshooting/).

Every model the agent may create also has a `<model>__bulk_create` tool: up to 1,000 rows in one call (`rows`, or `csv` text), all or nothing by default, `mode: "partial"` to keep the good rows and get the bad ones listed, `dry_run: true` to check first, and an `idempotency_key` to make a retry safe. It is subject to the same masking, scope and approval as `create`.

An agent whose owner is an administrator also sees the `user_account` tools (`user_account__list`, `__get`, `__add`, `__set_groups`, `__offboard`, `__disable`, `__enable`). The ones that change an account always return `approval_required`: an administrator approves the plan, and it runs as that administrator's current account, so a plan outlives nothing. `__add` needs a password a person chose; the server never hands a generated one to an agent.

## Verified and not verified

| Item                                                                                         | Status                                                                                 |
| -------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| `obop mcp` handshake, `tools/list`, `tools/call`, approval result                            | Run against a real local instance.                                                     |
| Remote `/mcp`: handshake, tools, masking, approval loop, sessions, 401, origin check, limits | Automated tests (Rust over HTTP, Playwright) against a real instance.                  |
| Remote `/mcp` with a real MCP client                                                         | MCP Inspector command line client 2.11.0: `initialize` and `tools/list`, `tools/call`. |
| Official MCP conformance suite                                                               | Not meaningful: it needs a server without authentication and its own fixture tools.    |
| Remote configuration of Claude Code, Cursor, VS Code, Windsurf, Codex CLI, Gemini CLI        | Taken from each client's documentation; the clients were not started.                  |
| Stdio configuration formats                                                                  | Taken from each client's documentation. We did not start every client.                 |
| Windsurf file location                                                                       | Unconfirmed, see above.                                                                |
| Browser sign-in (OAuth): registration, PKCE, consent, tokens, refresh reuse, revocation      | Automated tests (Rust, and Playwright in a real browser) with our own client.          |
| Claude Desktop and claude.ai connectors, Claude Code sign-in                                 | **Not tried** with the real apps; built from the specification.                        |
