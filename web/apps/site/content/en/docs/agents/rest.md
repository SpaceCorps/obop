---
title: "Use the REST API"
description: "Call obop's REST API as an agent with curl: list, search, create and update records, run workflow actions, see masked data and handle approval plans. Examples run against a real local instance."
lead: "Every command is one POST with a JSON body. The examples use a local instance on port 8080 and an agent token in $AGENT_TOKEN."
---

## The shape of every call

```text
POST /api/{model}/{command}
Authorization: Bearer <token>
Content-Type: application/json

{ ...input... }
```

`model` and `command` are the two halves of the command id: `lead.search` is `POST /api/lead/search`. The response is JSON:

```json
{"denied": [], "value": ...}
```

`value` is the result and `denied` lists field names withheld from this caller. Other endpoints: `GET /api/commands` (what this token may call), `GET /api/meta` (models, fields and actions this token may see), `POST /api/approve/{plan_id}` (a person approves a plan) and the two `/api/auth/` endpoints from [Create an agent token](/docs/agents/tokens/). The full list is in the [REST reference](/docs/reference/rest-api/).

## See what the token may do

```bash
curl -s http://127.0.0.1:8080/api/commands -H "Authorization: Bearer $AGENT_TOKEN"
```

```json
{
  "commands": [
    { "id": "lead.create", "description": "Create a record", "needs_approval": false },
    {
      "id": "lead.delete",
      "description": "Delete a record that no other record refers to",
      "needs_approval": true
    },
    {
      "id": "lead.qualify",
      "description": "Move a new lead to the qualified stage",
      "needs_approval": false
    }
  ]
}
```

`needs_approval` tells the agent in advance which commands it can only propose.

## Read, with masking

```bash
curl -s -X POST http://127.0.0.1:8080/api/party/list \
  -H "Authorization: Bearer $AGENT_TOKEN" -H 'content-type: application/json' -d '{}'
```

```json
{
  "denied": ["party.tax_id"],
  "value": [
    {
      "email": "EMAIL_1",
      "id": "01a1464f-b0c8-700e-9f39-0cbf8e848c0b",
      "name": "NAME_2",
      "notes": "NOTES_3"
    }
  ]
}
```

The tax identifier is withheld and named in `denied`. Name, email and notes are personal data and come back as tokens.

## Search and filter

`search` takes the same input as `list` and also returns the total before paging. Filters and sorting only work on fields the agent can read in clear.

```bash
curl -s -X POST http://127.0.0.1:8080/api/lead/search \
  -H "Authorization: Bearer $AGENT_TOKEN" -H 'content-type: application/json' \
  -d '{"filter":[{"field":"stage","op":"eq","value":"new"}],
       "order":[{"field":"expected_revenue_minor","dir":"desc"}],
       "limit":10}'
```

```json
{
  "denied": [],
  "value": {
    "rows": [
      {
        "id": "...",
        "stage": "new",
        "title": "Fleet telematics",
        "expected_revenue_minor": 3125000,
        "contact_email": "CONTACT_EMAIL_7",
        "contact_name": "CONTACT_NAME_8"
      }
    ],
    "total": 1
  }
}
```

Operators: `eq`, `ne`, `contains`, `in`, `gt`, `gte`, `lt`, `lte`, `is_null`. At most 50 conditions, 5 order keys and 5000 rows per call.

## Create and update

```bash
curl -s -X POST http://127.0.0.1:8080/api/lead/create \
  -H "Authorization: Bearer $AGENT_TOKEN" -H 'content-type: application/json' \
  -d '{"title":"Support renewal","contact_name":"NAME_2","contact_email":"EMAIL_1"}'
```

Money is an integer in minor units (`3125000` is 31,250.00). Dates are `YYYY-MM-DD`. The tokens `NAME_2` and `EMAIL_1` were read earlier; the server swaps them for the real values on write and the agent never sees them. See [Masking tokens](/docs/agents/masking/).

`update` takes the record `id` and the fields to change:

```bash
curl -s -X POST http://127.0.0.1:8080/api/lead/update \
  -H "Authorization: Bearer $AGENT_TOKEN" -H 'content-type: application/json' \
  -d '{"id":"01a14650-41ae-7455-9566-1ec1a313bdf3","expected_revenue_minor":3500000}'
```

## Create many at once

`POST /api/{model}/bulk_create` takes up to 1,000 rows (`{"rows": [...]}` or `{"csv": "..."}`), checks each as `create` would, and writes them together or, with `"mode": "partial"`, keeps the good ones. Use `"dry_run": true` first and an `"idempotency_key"` when you might retry. See the [REST API reference](/docs/reference/rest-api/#create-many-records).

## Run a workflow action

```bash
curl -s -X POST http://127.0.0.1:8080/api/lead/qualify \
  -H "Authorization: Bearer $AGENT_TOKEN" -H 'content-type: application/json' \
  -d '{"id":"01a14650-41ae-7455-9566-1ec1a313bdf3"}'
```

An action that needs approval answers `202` instead; see the next section.

## Propose something that needs approval

```bash
curl -si -X POST http://127.0.0.1:8080/api/lead/delete \
  -H "Authorization: Bearer $AGENT_TOKEN" -H 'content-type: application/json' \
  -d '{"id":"01a14650-41ae-7455-9566-1ec1a313bdf3"}'
```

```text
HTTP/1.1 202 Accepted

{"approval_required":true,"plan_id":"01a14650-41d3-7205-b97a-20fa74bb3420"}
```

Nothing has changed. Give the plan id to a person; see [The approval loop](/docs/agents/approvals/).

## Status codes

| Code | Meaning                                                                                                                                                                                                                                                                                                                       |
| ---- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 200  | Done. Read `value` and `denied`.                                                                                                                                                                                                                                                                                              |
| 202  | Approval required. `plan_id` is in the body. Nothing changed.                                                                                                                                                                                                                                                                 |
| 400  | The input was refused. `error` says why, for example `lead.title is required` or `invalid token`.                                                                                                                                                                                                                             |
| 401  | No bearer token, or an unknown, expired or revoked one.                                                                                                                                                                                                                                                                       |
| 404  | The command does not exist, is not visible to this token, or the plan is unknown. All three look the same on purpose.                                                                                                                                                                                                         |
| 415  | The body was not sent as `application/json`.                                                                                                                                                                                                                                                                                  |
| 429  | Too many failed sign-ins (five in a minute lock a username for a minute).                                                                                                                                                                                                                                                     |
| 503  | The database was busy, unreachable or too slow (`storage_busy`, `storage_unavailable`, `storage_timeout`). Read `Retry-After` (seconds) and try again; nothing was changed (after `storage_unavailable` a change in flight may or may not have been applied). A call that had already changed something keeps its own answer. |

## Tips for agents

- Start with `GET /api/commands` and `GET /api/meta` so the agent learns the model instead of guessing.
- Send small, specific calls. A `search` with a `filter` and a `limit` beats listing everything.
- Treat `denied` as information: that field exists but is not for this agent.
