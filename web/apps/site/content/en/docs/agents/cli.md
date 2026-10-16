---
title: "Use the CLI"
description: "Use the obop command line as an agent: obop commands lists what a token may do, obop call runs a command with key=value arguments, and approvals are reported without running."
lead: "The CLI is the third surface over the same commands. It is the simplest way for a shell script or a scheduled job to work with obop."
---

The CLI opens the database directly, like `obop mcp`, so it runs where the database is reachable. For a hosted instance use [REST](/docs/agents/rest/).

## Set the token and database once

```bash
export OBOP_TOKEN=obop_your_agent_token
export OBOP_DATABASE_URL=sqlite:///home/me/obop.db
```

Or pass `--token` and `--db` on every call.

## List what the token may do

```bash
obop commands
```

```text
lead.create              Create a record
lead.delete              Delete a record that no other record refers to  [needs approval for agents]
lead.get                 Read one record by id
lead.list                List records; optional filter, order, limit and offset
lead.qualify             Move a new lead to the qualified stage
lead.search              Like list, but returns {rows, total} with the count before limit and offset
lead.update              Update fields of a record
```

## Call a command

`obop call <model> <command> [key=value ...]`. Numbers, money and booleans are converted from text using the model's field types, so `expected_revenue_minor=5000` is an integer.

```bash
obop call lead search limit=1
obop call lead create title="Spring campaign" expected_revenue_minor=5000
obop call lead qualify id=01a14650-c0c4-72e3-bb4d-36d1f28fa20d
```

The result is printed as indented JSON. Masking applies exactly as over REST:

```json
{
  "rows": [
    {
      "contact_email": "CONTACT_EMAIL_1",
      "contact_name": "CONTACT_NAME_2",
      "expected_revenue_minor": 500000,
      "id": "01a1464f-b0c9-7689-8fe1-ec6283086c76",
      "stage": "qualified",
      "title": "Annual support contract"
    }
  ],
  "total": 4
}
```

## Approvals and errors

A command that needs approval is not run. The CLI prints the plan and exits with an error:

```text
error: ApprovalRequired { plan_id: "01a14650-d1b3-74c4-91b6-9cc0b09d082d" }
```

Other errors print the reason, for example `error: Handler("no lead nope")`. There is no `approve` command in the CLI: a person approves over REST or in the web app. See [The approval loop](/docs/agents/approvals/).

Each `obop call` is a separate process, so masking tokens do not carry over from one call to the next. To use a token you read earlier, stay inside one MCP or REST session.

## Using it from a script

```bash
#!/bin/sh
set -e
# The newest 50 leads that are still in the "new" stage, as JSON
obop call lead search 'filter=[{"field":"stage","op":"eq","value":"new"}]' limit=50
```

Structured arguments such as `filter` are JSON text. For anything beyond a few fields, REST with `jq` is usually easier to read than the CLI.

## Flags

| Flag            | Meaning                                                                                                                 |
| --------------- | ----------------------------------------------------------------------------------------------------------------------- |
| `--token TOKEN` | Bearer token. Default: `OBOP_TOKEN`.                                                                                    |
| `--db URL`      | `sqlite://file.db`, `postgres://user:pass@host/db` or `memory:`. Default: `OBOP_DATABASE_URL`, then `sqlite://obop.db`. |
| `--dev-auth`    | Use the fixed dev tokens (`dev-admin`, `dev-sales`, `dev-agent`). Local development only.                               |

The full list is in the [CLI reference](/docs/reference/cli/).
