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

Instances also serve the web app from `/`. The control plane serves its own endpoints under `/api/platform/` (sign-up, sign-in, instances); they are used by the sign-up app and are not covered here.

## Command responses

Success: `200` with

```json
{"denied": ["party.tax_id"], "value": { ... }}
```

`denied` lists the field names this caller may not see. Needs approval (for an agent): `202` with `{"approval_required":true,"plan_id":"..."}`.

## Generated commands

Every model gets these. For model `lead`:

| Command       | Input                                         | Result                                                                                |
| ------------- | --------------------------------------------- | ------------------------------------------------------------------------------------- |
| `lead.get`    | `{"id"}`                                      | The record.                                                                           |
| `lead.list`   | optional `filter`, `order`, `limit`, `offset` | An array of records.                                                                  |
| `lead.search` | same as list                                  | `{"rows":[...],"total":N}`, where `total` counts matches before `limit` and `offset`. |
| `lead.create` | the fields                                    | The new record, with its `id`. Governed fields start at their first option.           |
| `lead.update` | `{"id", ...fields}`                           | The updated record. Changing a governed field runs the matching action.               |
| `lead.delete` | `{"id"}`                                      | `{"id"}`. Refused while another record refers to it. Agents only propose it.          |

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

| Code | Meaning                                                                           |
| ---- | --------------------------------------------------------------------------------- |
| 200  | Success.                                                                          |
| 202  | Approval required; nothing changed.                                               |
| 400  | Input refused. `{"error":"..."}` says why.                                        |
| 401  | Missing, unknown, expired or revoked token; or wrong sign-in.                     |
| 404  | Unknown, hidden or forbidden command; unknown plan. Indistinguishable on purpose. |
| 415  | Body not sent as `application/json`.                                              |
| 429  | Sign-in lockout (five failures, one minute).                                      |

## Versioning

There is no version header or deprecation policy yet. The product plan calls for a dated version header; until it exists, treat the API as unstable during the beta and check the [changelog](/changelog/).

See [Use the REST API](/docs/agents/rest/) for worked examples and the [module manifest reference](/docs/reference/module-manifest/) for how fields and actions are declared.
