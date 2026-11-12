---
title: "Safety best practices"
description: "Practical rules for running AI agents against obop: least privilege, one token per task, keeping tokens secret, reviewing approvals, and what to do when a token leaks."
lead: "obop limits what an agent can do, but you choose the limits. These are the habits that matter."
---

## Give the least that works

- **Scopes are the main control.** An agent that triages tickets needs `helpdesk.agent`, not `platform.admin`. If you are unsure whether a group is needed, leave it out and add it when a call fails.
- **Do not hand an agent an administrator's groups.** Installing and removing modules needs `platform.admin`; agents can only propose it, but there is no reason to widen the blast radius.
- **Check the result.** `GET /api/commands` with the new token lists exactly what it can call. Read that list before you use the token.

## One token per agent and per task

A token made with `obop agent-token` or the REST API has a label, but you cannot list or revoke those tokens yet. Separate tokens limit damage and make an investigation possible:

- Create a fresh token for each agent and each unattended job.
- Create short-lived work with a token you will stop using, and stop using it.
- If you share one token between agents, you cannot tell their actions apart and you cannot cut one off.

## Keep tokens secret

- Use environment variables or a secret store. Never commit a token, paste it into a chat, or put it in a prompt.
- In an MCP client config, reference the environment where the client allows it (`${env:OBOP_TOKEN}`, `${input:...}`) so the file holds no secret.
- Agent tokens do not expire. A leaked one works until its owner is removed.

## If a token leaks

There is no command to revoke a single token made with `obop agent-token` or the REST API yet. The ways to stop one today are:

1. Delete or disable the owning user so that the token stops resolving (a token stops working when its owner is deleted).
2. Rotate the whole instance database credentials if the leak is wider than one token.

An app that connected through the browser sign-in (see [MCP](/docs/agents/mcp/)) is listed under _Customize, Security, Connected apps_, where its owner can revoke it at once.

Then review what the token did: records changed by an agent are not yet marked as such in the interface, so compare against your backups and the approvals you granted. A revoke command and an audit view are on the [roadmap](/roadmap/).

## Review every approval

Approval exists so that a person looks. Make it a real look:

- Ask the agent to describe the change in plain words, with the record it concerns, before you approve.
- Approve in a separate step from the conversation with the agent, so you are not pressed into saying yes.
- Do not approve a plan you did not expect. Nothing is lost by asking again.

## Keep personal data out of the loop

- Do not paste real customer data into prompts. Let the agent read tokens.
- Classify custom fields honestly. A free-text field that may contain personal data should be marked as personal data, or the agent will see it in clear.
- Be careful with `notes` fields: people type anything in them.

## Test on a copy

Point a new agent at a throwaway SQLite instance (`obop serve --demo` makes fake data) before you let it near a real business. Use the [CLI](/docs/agents/cli/) or [REST](/docs/agents/rest/) to rehearse the flow.

## Know the limits

- Row rules exist only in some apps, and CRM has none: an agent with `crm.user` can read every lead. Where an app has them (for example Expenses, Timesheets, HR leave requests), an agent never sees more than its owner.
- An action usually moves one state field and can also create or change other records in the same database, for example paying a sale in Point of Sale can prepare draft stock moves. An action does not send email. The Connectors actions `test` and `sync` reach the other system of a connection (`sync` needs approval). In the eCommerce app, when the shop takes card payments through Stripe, the order actions `refund` and `refund_delivered` ask Stripe to refund the payment first (both need approval, so an agent can only propose them), and `cancel` asks Stripe to close the open payment and to return a payment that had already arrived (it needs no approval, so an agent that holds `ecommerce.user` can run it). An administrator can also connect a [connector](/docs/concepts/connectors/) so that a webhook goes out when a command succeeds, and that includes commands an agent runs; check the connections before you give an agent write access.
- Masking works on classified fields, not on free text.
- Failed sign-ins are rate limited. So are requests on `/mcp`, per token (600 a minute by default, `OBOP_MCP_RATE_PER_MINUTE`), bulk imports, per person and shared with that person's agents (12 a minute by default, `OBOP_BULK_PER_MINUTE`), and file uploads (40 in ten minutes per principal and server process). Other REST calls have no per-token limit. See the [environment variables](/docs/reference/environment/).
