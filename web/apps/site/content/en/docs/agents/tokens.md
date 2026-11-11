---
title: "Create an agent token"
description: "Create a scoped obop agent token over REST (POST /api/auth/agent-token) or with the CLI (obop agent-token). The token is shown once and can only narrow its owner's permission groups."
lead: "An agent token is a bearer token that belongs to a person and carries a list of permission groups. Create one per agent and per task."
---

## What a token is

- It belongs to the person who created it (its _owner_) and acts with a subset of that person's groups.
- It is shown **once**. obop stores only a hash, so a lost token cannot be recovered; create a new one.
- It starts with `obop_`. Treat it like a password.
- Only a person can create tokens. An agent cannot mint another token.
- It does not expire. It stops working while its owner's account is disabled, and for good when the owner is offboarded. There is no command to revoke a single token yet (see [Safety](/docs/agents/safety/)).

## Choose the scopes

Scopes are [permission groups](/docs/concepts/permission-groups/). Look at what the agent should do and give it only those groups. Common groups of the built-in apps:

| Group               | Lets the agent                                          |
| ------------------- | ------------------------------------------------------- |
| `base.user`         | read and write Contacts (personal data stays masked)    |
| `crm.user`          | read and write leads and move them through the pipeline |
| `sales.user`        | read and write quotations and orders                    |
| `invoicing.user`    | read and write invoices and cancel drafts               |
| `invoicing.manager` | propose posting an invoice (a person still approves)    |
| `product.user`      | read products and prices                                |

Installed packages add their own groups, for example `helpdesk.agent`. The [app pages](/apps/) list the groups of each app.

## Over REST

Sign in as a person, then ask for a token. This needs a running instance; the examples use a local one on port 8080.

```bash
curl -s -X POST http://127.0.0.1:8080/api/auth/login \
  -H 'content-type: application/json' \
  -d '{"username":"alice","password":"your password"}'
```

The answer is `{"token":"obop_..."}`: a session token that lasts 12 hours. If two-factor sign-in is on for the account, the answer is `{"mfa_required":true,"mfa_token":"..."}` instead; send `{"mfa_token":"...","code":"123456"}`, with a code from your authenticator app, to `POST /api/auth/login/mfa` to get the session token. Use the session token as a bearer token to create the agent token:

```bash
curl -s -X POST http://127.0.0.1:8080/api/auth/agent-token \
  -H "Authorization: Bearer $SESSION_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"scopes":["base.user","crm.user"],"label":"triage-bot"}'
```

```json
{ "token": "obop_1e00c3..." }
```

`scopes` is required, must not be empty and may name only groups that are listed on your own account; `platform.admin` does not stand in for the other groups here. The first account of an instance that a control plane created holds only `platform.admin`, so give it the groups you want to hand over first, under Customize > Security > People. `label` is free text. On such an instance use its address instead of `127.0.0.1:8080`, and sign in with your sign-up email as the username.

## With the CLI

If you run obop yourself you can issue a token straight from the command line. It talks to the database directly, so run it where the database is reachable:

```bash
obop agent-token alice --scopes base.user,crm.user --label triage-bot
```

```text
obop_1e00c32430532fdcef...
shown once; store it like a password
```

Add `--db postgres://...` (or set `OBOP_DATABASE_URL`) if the instance does not use the default `sqlite://obop.db`.

## Use the token for MCP

The same token connects an MCP client to the instance's remote MCP server, `https://YOUR-INSTANCE/mcp`: send it as `Authorization: Bearer obop_...` (clients have a `headers` field or a bearer-token variable for it). A token created with the CLI or over REST works for both; there is nothing MCP-specific to create. Keep these points in mind:

- Create a separate token per client and give it only the scopes that client needs. The MCP tool list is exactly the commands the scopes allow.
- A session token from sign-in also works, but it expires after 12 hours (an agent token does not), and it acts as you: personal data is not masked and commands that need approval run without one. Use an agent token for an agent.
- The token must be sent on every request, not only the first; clients do this once it is in their configuration.
- Clients that support a browser sign-in (OAuth) do not need a pasted token: they connect with a sign-in and get a one-hour token that works on `/mcp` only. We have not yet tried this with Claude Desktop, claude.ai or Claude Code; see [Connect over MCP](/docs/agents/mcp/#connect-with-a-browser-sign-in-oauth).

## Store it safely

- Put it in an environment variable or a secret store, not in a repository.
- Prefer one token per agent and per task, each with only the scopes that agent needs.
- Tokens in a client config file are readable by anything that can read the file.

## Errors

| Answer                                                                             | Meaning                                                                                        |
| ---------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| `400 {"error":"an agent token needs at least one scope"}`                          | `scopes` was empty.                                                                            |
| `400 {"error":"you do not hold invoicing.manager, so you cannot delegate it"}`     | You asked for a group you do not hold.                                                         |
| `400 {"error":"only a human user can issue agent tokens"}`                         | You used an agent token to ask for another.                                                    |
| `401`                                                                              | Missing, wrong or expired session token.                                                       |
| `403 {"error":"two-factor authentication must be set up before you can continue"}` | Your account must set up two-factor sign-in first; until then this session can only set it up. |

Next: [connect over MCP](/docs/agents/mcp/) or [use the REST API](/docs/agents/rest/).
