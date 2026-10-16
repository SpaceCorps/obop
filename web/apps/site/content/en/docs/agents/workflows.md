---
title: "Example workflows"
description: "Example prompts and agent workflows for obop: triage new helpdesk tickets, draft quotations, prepare invoices for approval, and keep a CRM pipeline tidy, each with the token scopes it needs."
lead: "Starting points you can adapt. Each one lists the scopes, a prompt, the calls the agent makes and where a person steps in."
---

These examples assume the agent is connected over [MCP](/docs/agents/mcp/) or [REST](/docs/agents/rest/). Tool names are `model__command` over MCP and `/api/model/command` over REST.

## Triage new helpdesk tickets

**Needs:** the Helpdesk package installed. **Scopes:** `helpdesk.agent`.

**Prompt**

> You help the support team. List helpdesk tickets in state `open` that have no category. For each, read the subject and description, set `category` (question, incident, problem or feature_request) and `priority` (low, normal, high or urgent) and give me a one-sentence summary in your final report. Do not change the state or any other field. Personal data is masked; never try to find out who the customer is. At the end, list what you changed.

**What the agent does**

1. `helpdesk_ticket__search` with `{"filter":[{"field":"state","op":"eq","value":"open"},{"field":"category","op":"is_null"}],"limit":25}`.
2. For each row, `helpdesk_ticket__update` with `{"id":"...","category":"incident","priority":"high"}`.
3. Reports the list.

**Where a person steps in:** replying to customers. Helpdesk has no sending action, and this agent has no way to contact anyone.

## Draft a quotation

**Scopes:** `crm.user`, `sales.user`, `base.user`, `product.user`.

**Prompt**

> The lead "Warehouse pilot" has reached the qualified stage. The customer is the party with id 01a1464f-b0c8-700e-9f39-0cbf8e848c0b. Look up the list prices of the products I name below and create a draft sale order for that party with the matching total and currency EUR. Leave it in draft. Tell me the order id.

**What the agent does**

1. `lead__search` to confirm the lead and its stage.
2. `product__search` for list prices (`list_price_minor`, an integer in minor units).
3. `sale_order__create` with `party_id`, `total_minor` and `currency`. The state starts at `draft` and the agent cannot set it to anything else.

The prompt gives the party id because masked contact details on a lead cannot be matched to a party: tokens differ per field, and an agent cannot search masked fields.

**Where a person steps in:** confirming the order. The agent could run `sale_order__confirm` too, because Sales has no approval step; if you want a person to confirm, do not put `confirm` in the prompt, or keep the agent's scope to a read-only group.

## Prepare invoices for approval

**Scopes:** `invoicing.user`, `invoicing.manager`.

**Prompt**

> List invoices in draft. For each, check that total_minor is greater than zero and that it refers to a sale order. Propose posting the ones that pass and give me a table of invoice id, total and plan id. Do not try to post anything yourself.

**What the agent does:** `invoice__search` for drafts, then `invoice__post` for each that passes. Every call returns `approval_required` with a plan id. The agent lists the plans. A person with `invoicing.manager` approves the ones they agree with, using [the approval loop](/docs/agents/approvals/). The legal number is assigned only when the plan runs.

## Keep a CRM pipeline tidy

**Scopes:** `crm.user`.

**Prompt**

> Find leads in `qualified` with no expected revenue and list them for me. Do not edit them. Then find leads in `new` that have no contact name and suggest which to archive. Do not delete anything; if you think a lead should go, say which and why.

An agent can only propose a delete, so even a mistaken instruction does no immediate harm. The plan appears as `approval_required`.

## A scheduled daily summary over the CLI

**Scopes:** read-only groups such as `crm.user`.

```bash
export OBOP_TOKEN=obop_read_only_token
obop call lead search 'filter=[{"field":"stage","op":"eq","value":"won"}]' limit=50 > won.json
```

A script feeds `won.json` to a model to write the summary. Because leads come back masked, the summary refers to tokens, not names. Resolve names yourself afterwards if you need them.

## Writing good agent instructions

- State the goal and the boundary: what to do, and what never to do.
- Name the states and fields you mean (`state`, `stage`, `category`), so the agent does not guess.
- Say what to do when something is withheld or needs approval: report it and stop.
- Ask for a short report at the end. It is what the approving person reads.

See [Safety best practices](/docs/agents/safety/) and [Troubleshooting](/docs/agents/troubleshooting/).
