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

A token has a label, but you cannot list or revoke tokens yet. Separate tokens limit damage and make an investigation possible:

- Create a fresh token for each agent and each unattended job.
- Create short-lived work with a token you will stop using, and stop using it.
- If you share one token between agents, you cannot tell their actions apart and you cannot cut one off.

## Keep tokens secret

- Use environment variables or a secret store. Never commit a token, paste it into a chat, or put it in a prompt.
- In an MCP client config, reference the environment where the client allows it (`${env:OBOP_TOKEN}`, `${input:...}`) so the file holds no secret.
- Tokens do not expire. A leaked token works until its owner is removed.

## If a token leaks

There is no revoke command yet. The ways to stop a token today are:

1. Delete or disable the owning user so that the token stops resolving (a token stops working when its owner is deleted).
2. Rotate the whole instance database credentials if the leak is wider than one token.

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

- No row-level permissions: an agent with `crm.user` can read every lead.
- Actions change one field and cannot send email or call other systems, so an agent cannot cause those side effects through obop.
- Masking works on classified fields, not on free text.
- Rate limiting applies to failed sign-ins only. There are no per-token quotas yet.
