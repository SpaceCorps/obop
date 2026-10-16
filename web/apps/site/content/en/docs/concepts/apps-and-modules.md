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

| Kind | Apps | Notes |
|---|---|---|
| Built in | Contacts, Products, CRM, Sales, Invoicing | Written in Rust, always present, cannot be removed. |
| Bundled packages | The other 22 apps | Plain `obop.toml` files in the `packages/` directory, installed from the marketplace. |
| Your own | Anything you write | Upload a file, or publish your own marketplace. |

A package is one text file. It contains no code, so an administrator can install it at runtime. Code tiers (expressions, WebAssembly) are planned but not built, which is why packages cannot yet compute totals, generate sequences or call out to other systems. The [roadmap](/roadmap/) is honest about that.

## Installing and removing

Administrators install, upgrade and uninstall packages from *Customize*, or with the CLI (`obop modules install-from ID`, `obop modules install FILE`, `obop modules uninstall ID`). Install checks the file first: every field must have a description, a data class and a permission group, personal data may not be classed `none`, and dependencies must be installed.

Uninstalling removes the commands and screens but keeps the data in the database. Installing the module again brings the records back. There are no data migrations yet: raise the `version` and install again to upgrade, and prefer adding optional fields.

## Keep reading

- [Browse the apps](/apps/)
- [Module manifest reference](/docs/reference/module-manifest/)
- [Marketplace](/marketplace/)
