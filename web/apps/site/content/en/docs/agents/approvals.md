---
title: "The approval loop"
description: "How obop keeps a person in charge: an agent proposes a step that needs approval, the server returns a plan id and changes nothing, and a person approves it on the Approvals page. Examples over REST and MCP."
lead: "Agents propose. Humans approve. This page shows the loop end to end."
---

## Which steps need approval

- Every action marked `approval = true` in its module. In the built-in modules that is `invoice.post`, `party.erase`, `connection.connect`, `pause`, `resume`, `retry`, `revoke` and `sync`, `consent.withdraw`, `egress_policy.purge`, and `mileage_trip.confirm`, `reject`, `discard` and `discard_stuck`; in bundled packages it includes posting accounting entries, refunds, sending campaigns, publishing website content, signing, and paying expenses. Each [app page](/apps/) lists those of its app.
- Every `delete`.
- Installing, updating, rolling back and removing modules (`module.install`, `module.upgrade`, `module.upgrade_all`, `module.rollback`, `module.uninstall`, `marketplace_listing.install`), and updating the instance itself (`instance_update.start`, `instance_update.rollback`).
- Administering accounts (`user_account.add`, `user_account.set_groups`, `user_account.offboard`, `user_account.disable`, `user_account.enable`, `user_account.bind_sso`, `user_account.unbind_sso`, `user_account.set_sso_only`) and revoking a signed-in device (`device.revoke`).

`GET /api/commands` lists the commands your token may use and marks each one that needs approval with `"needs_approval": true`; the MCP description of such a tool says "Needs human approval".

## The loop

1. **The agent calls the command.** Same call as any other.
2. **The server answers with a plan.** Nothing is changed. Over REST it is `202 Accepted` with `{"approval_required":true,"plan_id":"..."}`. Over MCP the tool result is `{"status":"approval_required","plan_id":"...","instruction":"..."}`. The CLI prints `error: ApprovalRequired { plan_id: "..." }` on stderr and exits with status 1.
3. **The agent tells a person.** A plain description of what it wanted to do and why. The agent cannot approve its own plan: an agent token asking to approve gets `404` (a token from the MCP browser sign-in gets `401`, because it works on `/mcp` only). It can read the state of its own plan with `plan.get` (MCP: `plan__get`): waiting, approved, rejected, failed or expired, and the reason a person gave when they rejected it.
4. **A person decides on the Approvals page.** The back office shows a button with the number of requests waiting for the person in its top bar (and an Approvals item in the account menu on a phone). The page lists what waits, oldest first. Opening a request shows what changes (each field from and to; a field the person may not read shows only "Set, not shown to you"), who asked and when, and when it runs out. The filled button names the act ("Approve and save", "Approve and delete"); Reject takes an optional reason of up to 300 characters, which the agent will read. Nothing is decided from the list, the badge or a notice: one request is on screen, with its change, when a person presses a button. A link in the notice "Sent for approval." opens that request. Scripts can do the same with a person's token: `POST /api/plan/list`, `/api/plan/get`, `/api/plan/approve` (with `seen_version` to approve only what was shown) and `/api/plan/reject`, or `POST /api/approve/{plan_id}` as before.
5. **The action runs.** The server checks again that the approver holds the action's group, that the proposing agent was allowed to do it, and that the record still allows the change. If the record changed after the person opened the request, the approval is refused and the request stays waiting, so a person only ever approves what they saw. The result comes back to the approver.

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

{"denied":[],"value":{"id":"01a1464f-b0cb-7689-be17-2311cd62d24c","number":"INV-2026-00001","state":"posted"}}
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
        "text": "{\"instruction\":\"Nothing has run. A person must approve plan 01a14660-1971-733a-8a1e-19e32cc341b2 in the back office, on the Approvals page. Check it later with plan__get (plan.get with id). Do not call this tool again for the same step.\",\"plan_id\":\"01a14660-1971-733a-8a1e-19e32cc341b2\",\"status\":\"approval_required\"}"
      }
    ],
    "structuredContent": {
      "instruction": "Nothing has run. A person must approve plan 01a14660-1971-733a-8a1e-19e32cc341b2 in the back office, on the Approvals page. Check it later with plan__get (plan.get with id). Do not call this tool again for the same step.",
      "plan_id": "01a14660-1971-733a-8a1e-19e32cc341b2",
      "status": "approval_required"
    },
    "isError": false
  }
}
```

`isError` is `false` because a proposal is not a failure. Tell the agent in its instructions to treat `approval_required` as "stop and ask a person".

## Limits, honestly

- A pending plan is kept sealed in the instance's database for 7 days, so a restart does not lose it, and any server process on that database can approve it. After 7 days it is forgotten: approving it answers 404 and the agent has to propose again.
- A person sees the requests they may decide (the action's group and the right to read the record) and those of their own agents; a request they may not decide does not exist for them. The back office shows the count when the page is opened, when the window gets focus and once a minute: it does not push a message to a phone. Tell the person by whatever channel you set up (a chat message, a ticket) that something waits.
- A decided request stays visible to the people who could decide it, and to the proposing agent through `plan.get`, for 30 days.
- One agent can have 100 requests waiting; the 101st is refused until some are decided.

See [Safety best practices](/docs/agents/safety/).
