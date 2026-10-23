---
title: "Permission groups"
description: "Permission groups in obop are names like crm.user and invoicing.manager. Models, fields and actions each name the group that may use them, and agent tokens can only narrow their owner's groups."
---

A **permission group** is a name of the form `module.role`, for example `crm.user`, `helpdesk.agent` or `invoicing.manager`. A group exists as soon as something refers to it. Users hold groups, and so do agent tokens.

## Three places that name a group

1. **Model:** `read_group` allows `get`, `list` and `search`. `write_group` allows `create`, `update` and `delete`.
2. **Field:** `group` says who sees the field. It defaults to the model's read group. A field a caller cannot read is not even described to them.
3. **Action:** `group` says who may run a workflow step. It defaults to the model's write group. This is how posting or paying can need a stricter group than editing.

## Conventions

`<module>.user` is the day-to-day role and `<module>.manager` handles configuration and approvals. A third group such as `approver` appears only when someone who enters a thing must not be the person who releases it.

`platform.admin` satisfies every check for a person. The first user of a hosted instance holds it, and it is what lets someone install modules.

## Users and agents

A person's groups are set when the user is created (`obop admin create NAME --groups a,b`, or `--all-groups`) and changed later by an administrator in Customize > Security > People, with `obop admin groups NAME --groups a,b`, or over REST. Only groups the installed modules declare can be given, a change reaches the person's open sessions at their next request, and every change is in the security log with the groups before and after. An agent token is created with a list of scopes, which are groups, and it can only narrow its owner's groups: asking for a group the owner does not hold is refused. An agent cannot create tokens itself.

An unknown command, a hidden command and a forbidden command all answer `404 not found`, so a caller cannot probe which commands exist.

## Limits

There are no row-level permissions yet. Access is by model and field group only, so everyone who can read a model reads every record in it. A `visibility` field in an app such as Knowledge is a label, not a rule.

See [Create an agent token](/docs/agents/tokens/) and [Safety best practices](/docs/agents/safety/).
