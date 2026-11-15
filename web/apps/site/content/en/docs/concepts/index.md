---
title: "Concepts"
description: "The building blocks of obop: apps and modules, instances, data classes and masking, permission groups, workflows and approvals."
---

obop has a small number of ideas. Once you know them, every app works the same way.

- [Apps and modules](/docs/concepts/apps-and-modules/): an app is a module that declares models, fields and actions. Everything else is generated.
- [Instances](/docs/concepts/instances/): one business, one database, one address.
- [Data classes and masking](/docs/concepts/data-classes-and-masking/): every field says how sensitive it is, and agents see personal data masked and sensitive data not at all.
- [Permission groups](/docs/concepts/permission-groups/): who may read, write and run steps.
- [Workflows and approvals](/docs/concepts/workflows-and-approvals/): state changes are named actions, and some need a person.

## The one rule behind all of it

The web app, the REST API, the MCP server and the command line all call the same commands through one dispatcher, with the same permission checks, the same masking and the same approval rules. A few routes sit outside it, such as the fields an app declares public for anonymous visitors and the page behind a signing link. A field without a data class cannot be installed.
