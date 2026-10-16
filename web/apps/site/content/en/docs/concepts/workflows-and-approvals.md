---
title: "Workflows and approvals"
description: "State changes in obop are named actions such as qualify or post. Irreversible actions need approval: an agent only proposes them, and a person with the right group approves."
---

## Workflows are actions

A workflow is a set of **actions** on a model. An action moves one `select` field, usually `state` or `stage`, from one option to another: `lead.qualify` moves `stage` from `new` to `qualified`. An action may name a stricter group than the model's write group and may require approval.

The field an action moves becomes **governed**. You cannot set it freely with `create` or `update`:

- On `create`, leave it out (it starts at the first option) or send exactly the first option.
- On `update`, changing it is accepted only as an implicit run of a declared action, and only if you hold that action's group. The web app's board drag and status bar rely on this, so they succeed exactly when the person could have pressed the matching button.
- A move with no matching action is refused with a message that lists where the record can go.

## Approval

An action can be marked `approval = true`. Mark anything irreversible or money-moving: posting an invoice, paying, refunding, sending to customers, publishing, closing a period.

| Who runs it                      | What happens                                                                                            |
| -------------------------------- | ------------------------------------------------------------------------------------------------------- |
| A person with the action's group | It runs at once.                                                                                        |
| An agent                         | Nothing changes. The server answers with a **plan id** (`202` over REST, `approval_required` over MCP). |
| A person approving the plan      | The action runs, as that person.                                                                        |

Deleting a record is also only proposed by agents. A proposal is checked again when it is approved, because the record may have changed in between.

## Why approval is separate from permission

Permission says what an agent may try. Approval says what it may do without a person. An agent can hold the permission to post invoices and still only propose each posting, because the steps that cannot be undone are the ones where a human should decide.

## What actions cannot do

An action changes one field. It does not compute anything, create records, send email or call other systems. Apps that need those wait for the planned expression and WebAssembly tiers.

See [The approval loop](/docs/agents/approvals/) for the exact calls.
