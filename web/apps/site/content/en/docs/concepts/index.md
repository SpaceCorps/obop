---
title: "Concepts"
description: "The building blocks of obop: apps and modules, instances, data classes and masking, permission groups, workflows and approvals."
---

obop has a small number of ideas. Once you know them, every app works the same way.

- [Apps and modules](/docs/concepts/apps-and-modules/): an app is a module that declares models, fields and actions. Everything else is generated.
- [Instances](/docs/concepts/instances/): one business, one database, one address.
- [Data classes and masking](/docs/concepts/data-classes-and-masking/): every field says how sensitive it is, and agents see masked values.
- [Permission groups](/docs/concepts/permission-groups/): who may read, write and run steps.
- [Workflows and approvals](/docs/concepts/workflows-and-approvals/): state changes are named actions, and some need a person.

## The one rule behind all of it

Every surface goes through one command dispatcher. The web app, the REST API, the MCP server and the command line all call the same commands, with the same permission checks, the same masking and the same approval rules. A module cannot add a side door, and a field without a data class cannot be installed.
