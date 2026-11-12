---
title: "Apps and modules"
description: "In obop an app is a module: a file that declares models, fields, data classes, permission groups and workflow actions. The API, agent tools and screens are generated from it."
---

An **app** is what people see in the launcher. A **module** is how it is built. In the interface obop says "app"; in the API and in files it says "module".

## What a module declares

A module is described by models. A model has fields, and fields have a kind, a description, a data class and a permission group. A model can also have actions that move a record from one state to another.

From that description obop generates, for every model:

- six commands: `get`, `list`, `search`, `create`, `update` and `delete` (for the model `lead`: `lead.get`, `lead.list` and so on);
- one command per action, for example `lead.qualify`;
- the same commands as REST endpoints, MCP tools and CLI commands;
- screens: list, board, form, status bar and buttons.

Permissions, masking, approvals and audit are applied for free, because they live in the dispatcher and not in each app.

## Built in and installable

| Kind             | Apps                                                                             | Notes                                                                                                                                 |
| ---------------- | -------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| Built in         | Contacts, Products, CRM, Sales, Invoicing, Connectors (the six with an app page) | Written in Rust, always present, cannot be removed. Mileage, the server side of a planned phone app, is built in too and has no page. |
| Bundled packages | 24 more (22 have an app page)                                                    | Plain `obop.toml` files in the `packages/` directory, installed from the marketplace.                                                 |
| Your own         | Anything you write                                                               | Upload a file, or publish your own marketplace.                                                                                       |

A package is one text file. It contains no code, so an administrator can install it at runtime. It can declare computed totals (in a small built-in expression language), numbered fields and actions that change other records. Further code tiers (CEL expressions, WebAssembly components) are planned but not built, so a package runs no code of its own and cannot declare calls to other systems. The [roadmap](/roadmap/) is honest about that.

## Installing and removing

Administrators install, upgrade and uninstall packages from _Customize_, or with the CLI (`obop modules install-from ID`, `obop modules install FILE`, `obop modules uninstall ID`). Install checks the file first: every field must have a description, a data class and a permission group, personal data may not be classed `none`, and dependencies must be installed.

Uninstalling removes the commands and screens but keeps the data in the database. Installing the module again brings the records back.

Every module declares which core versions it runs on (`requires_core`) and, for each module it needs, the range of versions it supports. _Customize_, _Modules_, _Updates_ shows newer versions with their release notes, what changes in the data and which installed modules depend on them; an update is one confirmed, atomic operation, refused when it would leave a dependent's range or break stored records. A package can declare `[[migration]]` steps (rename a field, fill a default, drop a field with confirmation) that run in the same batch, and the last update can be rolled back while it deleted nothing. The instance itself is updated from _Customize_, _Updates_ (see [Instances](/docs/concepts/instances/)).

## Keep reading

- [Browse the apps](/apps/)
- [Module manifest reference](/docs/reference/module-manifest/)
- [Marketplace](/marketplace/)
