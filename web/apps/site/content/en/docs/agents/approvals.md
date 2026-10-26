---
title: "The approval loop"
description: "How obop keeps a person in charge: an agent proposes an irreversible step, the server returns a plan id and changes nothing, and a person approves it. Examples over REST and MCP."
lead: "Agents propose. Humans approve. This page shows the loop end to end."
---

## Which steps need approval

- Every action marked `approval = true` in its module. In the built-in apps that is `invoice.post` and `party.erase`; in bundled packages it includes posting accounting entries, refunds, sending campaigns, publishing website content, signing, and paying expenses. Each [app page](/apps/) lists them.
- Every `delete`.
- Installing, updating, rolling back and removing modules (`module.install`, `module.upgrade`, `module.upgrade_all`, `module.rollback`, `module.uninstall`, `marketplace_listing.install`), and updating the instance itself (`instance_update.start`, `instance_update.rollback`).

`GET /api/commands` marks them with `"needs_approval": true`, and the MCP tool description ends with "(agents propose; a human approves)".

## The loop

1. **The agent calls the command.** Same call as any other.
2. **The server answers with a plan.** Nothing is changed. Over REST it is `202 Accepted` with `{"approval_required":true,"plan_id":"..."}`. Over MCP the tool result is `{"status":"approval_required","plan_id":"..."}`. The CLI prints `ApprovalRequired { plan_id: ... }`.
3. **The agent tells a person.** The plan id and a plain description of what it wanted to do and why. The agent cannot approve its own plan: an agent token asking to approve gets `404`.
4. **A person approves.** They sign in and call `POST /api/approve/{plan_id}` with their own session token.
5. **The action runs.** The server checks again that the approver holds the action's group, that the proposing agent was allowed to do it, and that the record still allows the change. The result comes back to the approver.

## Example: posting an invoice

An agent with `invoicing.manager` finds a draft invoice and asks to post it.

```bash
curl -si -X POST http://127.0.0.1:8080/api/invoice/post \
  -H "Authorization: Bearer $AGENT_TOKEN" -H 'content-type: application/json' \
  -d '{"id":"01a1464f-b0cb-7689-be17-2311cd62d24c"}'
```

```text
HTTP/1.1 202 Accepted

{"approval_required":true,"plan_id":"01a14662-bb99-74fe-bc64-1b98f21d855f"}
```

The agent reports: _"Draft invoice 01a1464f... for 1,200.00 is ready to post. Waiting for approval: plan 01a14662-bb99..."_ A person with `invoicing.manager` approves:

```bash
curl -si -X POST http://127.0.0.1:8080/api/approve/01a14662-bb99-74fe-bc64-1b98f21d855f \
  -H "Authorization: Bearer $SESSION_TOKEN"
```

```text
HTTP/1.1 200 OK

{"denied":[],"value":{"id":"01a1464f-b0cb-7689-be17-2311cd62d24c","number":"INV-00001","state":"posted"}}
```

The invoice is posted and has its legal number. Approving the same plan again is `404`: a plan runs once.

Approving something that no longer applies, or that the approver may not run, fails with `404` or a `400` that says why. An unknown or already-used plan id is `404`.

If the database refuses the approval before anything was changed (`503` with `"applied":"none"`), the plan is put back and the same approval can be repeated. After `"applied":"unknown"` (the connection broke while a change was in flight) the plan is not put back, so that a repeat cannot run it twice: read the records, and ask the agent to propose again if the change is not there.

## Over MCP

```json
{
  "jsonrpc": "2.0",
  "id": 4,
  "method": "tools/call",
  "params": {
    "name": "lead__delete",
    "arguments": { "id": "01a1464f-b0c9-7689-8fe1-ec6283086c76" }
  }
}
```

```json
{
  "jsonrpc": "2.0",
  "id": 4,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "{\"plan_id\":\"01a14660-1971-733a-8a1e-19e32cc341b2\",\"status\":\"approval_required\"}"
      }
    ],
    "isError": false
  }
}
```

`isError` is `false` because a proposal is not a failure. Tell the agent in its instructions to treat `approval_required` as "stop and ask a person".

## Limits, honestly

- Pending plans are kept in the server's memory. A restart forgets them; the agent has to propose again.
- Plans do not expire on their own.
- There is no list-pending-plans call. The plan id travels from the agent to the approver by whatever channel you set up (a chat message, a ticket).
- A person approving should read what the plan does. The plan id alone does not show it; ask the agent to describe the change in words before you approve.

See [Safety best practices](/docs/agents/safety/).
