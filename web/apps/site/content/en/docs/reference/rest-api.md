---
title: "REST API reference"
description: "obop's REST API: sign in, agent tokens, commands, meta, the generated get, list, search, create, update and delete endpoints, workflow actions, approvals, filters, errors and status codes."
---

The REST API is generated from the installed modules. There is no OpenAPI document yet; this page lists everything that exists. All bodies are JSON and all calls except `login` need a bearer token.

```text
Authorization: Bearer obop_...
Content-Type: application/json
```

## Endpoints

| Method and path               | Purpose                                                                                                                                      |
| ----------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| `POST /api/auth/login`        | Body `{"username","password"}`. Returns `{"token"}`, a session token valid for 12 hours. `401` for wrong credentials, `429` when locked out. |
| `POST /api/auth/agent-token`  | Body `{"scopes":[...],"label":"..."}`. A person creates a scoped agent token. Returns `{"token"}` once.                                      |
| `GET /api/commands`           | The commands this caller may use: `{"commands":[{"id","description","needs_approval"}]}`.                                                    |
| `GET /api/meta`               | The modules, models, readable fields and actions this caller may see. Drives the web app.                                                    |
| `POST /api/{model}/{command}` | Runs a command. The body is the command input.                                                                                               |
| `POST /api/approve/{plan_id}` | A person approves a proposed plan.                                                                                                           |

Administrators also manage people over REST (a signed-in administrator's session; an agent token gets `404`, and proposes the same changes through the `user_account` commands instead):

| Method and path                                                   | Purpose                                                                                                                                                                                                                       |
| ----------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `GET /api/auth/groups`                                            | The permission groups that can be given: `{"groups":[...]}`.                                                                                                                                                                  |
| `GET /api/auth/users`                                             | The accounts, with groups, status, two-factor and sessions. Never a hash.                                                                                                                                                     |
| `POST /api/auth/users`                                            | Create a person: `{"username","groups":[...],"password"?,"must_change_password"?}`. Without a password the answer carries `temporary_password`, once. The person must choose their own at the first sign-in.                  |
| `POST /api/auth/users/{name}/groups`                              | `{"groups":[...]}` replaces the groups. Answers `{"before","after","added","removed","changed"}`. Takes effect at the person's next request. Refused: unknown groups, one's own administrator access, the last administrator. |
| `POST /api/auth/users/{name}/offboard`                            | `{"note"?}` disables the account and revokes its sessions, agent tokens and connected apps for good: `{"revoked_sessions","revoked_agent_tokens","revoked_apps"}`.                                                            |
| `POST /api/auth/users/{name}/disable`, `enable`, `reset-password` | Disable, enable, or set a temporary password (`{"password"}`).                                                                                                                                                                |

Instances also serve the web app from `/`. The control plane serves its own endpoints under `/api/platform/` (sign-up, sign-in, instances); they are used by the sign-up app and are not covered here. It also publishes `GET /api/releases` (the core releases an instance's _Updates_ page reads) and three endpoints that only an instance's update token opens (`/api/platform/update/status`, `start`, `rollback`); the token can ask for a published version and nothing else.

On an instance, administrators update modules with `POST /api/module/upgrade` (`{"id": ...}`), `/api/module/upgrade_all` and `/api/module/rollback`, and the instance itself with `POST /api/instance_update/check`, `status`, `start` (`{"version": ...}`) and `rollback`. Agents only propose the changing ones (a `202` with a `plan_id`).

## Command responses

Success: `200` with

```json
{"denied": ["party.tax_id"], "value": { ... }}
```

`denied` lists the field names this caller may not see. Needs approval (for an agent): `202` with `{"approval_required":true,"plan_id":"..."}`.

## Generated commands

Every model gets these. For model `lead`:

| Command            | Input                                         | Result                                                                                |
| ------------------ | --------------------------------------------- | ------------------------------------------------------------------------------------- |
| `lead.get`         | `{"id"}`                                      | The record.                                                                           |
| `lead.list`        | optional `filter`, `order`, `limit`, `offset` | An array of records.                                                                  |
| `lead.search`      | same as list                                  | `{"rows":[...],"total":N}`, where `total` counts matches before `limit` and `offset`. |
| `lead.create`      | the fields                                    | The new record, with its `id`. Governed fields start at their first option.           |
| `lead.bulk_create` | `rows` or `csv`, see below                    | A report: how many were created, which rows failed, and the ids.                      |
| `lead.update`      | `{"id", ...fields}`                           | The updated record. Changing a governed field runs the matching action.               |
| `lead.delete`      | `{"id"}`                                      | `{"id"}`. Refused while another record refers to it. Agents only propose it.          |

## Create many records

`POST /api/{model}/bulk_create` creates up to 1,000 records in one call (an agent token needs the same scope as for `create`, and approval exactly where `create` would). Give `rows` or `csv`:

```json
{
  "rows": [{ "title": "First" }, { "title": "Second", "stage": "new" }],
  "mode": "all_or_nothing",
  "dry_run": false,
  "idempotency_key": "import-2026-10-24"
}
```

| Input             | Meaning                                                                                                                                                                                                                                                                            |
| ----------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `rows`            | A list of objects of field names and values, as for `create`. At most 1,000.                                                                                                                                                                                                       |
| `csv`             | Instead: the text of a CSV file (UTF-8, `,` or `;`, at most 1 MiB, a header of field names). References are record ids; numbers are whole numbers (money in minor units, no decimal point); an empty cell leaves the field out; image and file fields are not accepted.            |
| `mode`            | `all_or_nothing` (default): one bad row creates nothing and the call answers `400` naming the first rows. `partial`: the good rows are created and the answer lists the bad ones.                                                                                                  |
| `dry_run`         | Check every row and write nothing. The answer is the full report, even in `all_or_nothing` mode.                                                                                                                                                                                   |
| `idempotency_key` | Up to 128 visible characters you make up. Repeating the call with the same key and the same rows returns the first answer (`replayed: true`) for 24 hours instead of creating the rows again; other rows with the same key are refused. The key belongs to the person who used it. |

Every row is checked exactly as `create` checks a record, and everything is written in one transaction. The answer:

```json
{
  "value": {
    "ok": true,
    "dry_run": false,
    "mode": "all_or_nothing",
    "source": "rows",
    "total": 2,
    "created": 2,
    "failed": 0,
    "errors": [],
    "errors_truncated": false,
    "ids": ["01...", "01..."],
    "replayed": false
  }
}
```

`ids` follows the order of the rows (`null` for a row that was not created); with `dry_run` the count is `valid` and there are no ids; `errors` lists at most 100 problems as `{row, line?, error}`, `line` being the line of a CSV file. A person may start `OBOP_BULK_PER_MINUTE` calls a minute (default 12). Limits: no remapping of ids and no reference from one row to another row of the same call, no decimal or date-time kinds yet, no images or files.

Each workflow action is one more command: `lead.qualify`, `invoice.post`. Input is `{"id"}`, plus any `required` fields it needs.

## Filters, order and paging

```json
{
  "filter": [
    { "field": "stage", "op": "in", "value": ["new", "qualified"] },
    { "field": "expected_revenue_minor", "op": "gte", "value": 100000 }
  ],
  "order": [{ "field": "expected_revenue_minor", "dir": "desc" }, { "field": "title" }],
  "limit": 50,
  "offset": 100
}
```

- Conditions are combined with AND; at most 50.
- Operators: `eq`, `ne`, `contains` (case-insensitive substring, text only), `in` (up to 1000 values), `gt`, `gte`, `lt`, `lte` (not for `bool` or `ref`), `is_null`. `ne` also matches records where the field is empty; other operators never match an empty value.
- `order`: up to 5 keys, `dir` `asc` (default) or `desc`; empty values sort last.
- `limit` 1 to 5000 (default: no limit), `offset` 0 or more.
- `id` can be used as a field. Unknown keys, operators or malformed values are `400`.
- Filtering and sorting only work on fields the caller can read in clear. Other fields fail with `unknown field <model>.<field>`.

## Field values

| Kind               | JSON                                             |
| ------------------ | ------------------------------------------------ |
| `text`, `longtext` | string                                           |
| `number`           | integer (no floats)                              |
| `money`            | integer in minor currency units: 12.34 is `1234` |
| `date`             | `"YYYY-MM-DD"`, a real calendar date             |
| `bool`             | `true` or `false`                                |
| `select`           | one of the field's options                       |
| `ref`              | the id of an existing record of the target model |

## Status codes

| Code | Meaning                                                                                                                                                                                                         |
| ---- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 200  | Success.                                                                                                                                                                                                        |
| 202  | Approval required; nothing changed.                                                                                                                                                                             |
| 400  | Input refused. `{"error":"..."}` says why.                                                                                                                                                                      |
| 401  | Missing, unknown, expired or revoked token; or wrong sign-in.                                                                                                                                                   |
| 404  | Unknown, hidden or forbidden command; unknown plan. Indistinguishable on purpose.                                                                                                                               |
| 415  | Body not sent as `application/json`.                                                                                                                                                                            |
| 429  | Sign-in lockout (five failures, one minute).                                                                                                                                                                    |
| 503  | The database was busy, unreachable or too slow: `{"error":"storage_busy"}`, `storage_unavailable` or `storage_timeout`, with a `Retry-After` header (seconds). Nothing was started after `storage_busy`; retry. |

## MCP

The same commands are also exposed as MCP tools at `POST /mcp` (Streamable HTTP, protocol revisions 2025-11-25, 2025-06-18 and 2025-03-26), with the same bearer token, permissions, masking and approvals. A command `lead.search` is the tool `lead__search`; its input is the REST body, described by a JSON Schema generated from the model. An approval-required step is a normal result with `"status": "approval_required"` and a `plan_id` instead of HTTP 202; a refused input is a tool result with `isError: true` instead of HTTP 400; a hidden or unknown tool is a JSON-RPC error instead of 404. The OpenAPI document advertises the endpoint in `x-mcp`, and `GET /api/meta` in a `Link` header (`rel="mcp"`). Setup, limits, discovery documents and client configuration: [Connect over MCP](/docs/agents/mcp/).

## Versioning

There is no version header or deprecation policy yet. The product plan calls for a dated version header; until it exists, treat the API as unstable during the beta and check the [changelog](/changelog/).

See [Use the REST API](/docs/agents/rest/) for worked examples and the [module manifest reference](/docs/reference/module-manifest/) for how fields and actions are declared.
