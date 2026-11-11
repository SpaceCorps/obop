---
title: "Platform"
description: "How the obop platform works: instances with one database per business, modules and the marketplace, permission groups, data classes and masking, approvals, and the MCP, REST and CLI surfaces."
lead: "One program, one data model, three ways in. This page explains how the pieces fit."
eyebrow: "How it works"
---

## One instance per business

Each business runs in its own **instance** with its own database, its own users and its own address. On the hosted service a control plane creates the instance for you after sign-up and gives you its address; the person who signed up becomes its first administrator. You can also run the same program yourself. See [Instances](/docs/concepts/instances/).

Storage is SQLite for trying things out and PostgreSQL for real use. Records are stored as JSON documents for now; the command layer checks field types, and a schema manager is planned.

## Modules are apps

An app is a **module**: a description of models, fields, permission groups and workflow actions. Five modules are built in (Contacts, Products, CRM, Sales, Invoicing). Twenty-two more are bundled as packages. You add apps from the marketplace in a click, or upload your own file.

Because every module goes through one registry, a contact is the same contact in CRM, Sales and Invoicing, and every module gets the same API, screens, permissions and privacy rules. See [Apps and modules](/docs/concepts/apps-and-modules/) and the [list of apps](/apps/).

## Marketplace

The marketplace is a static host serving `index.json` and one `obop.toml` per package. The bundled catalog ships inside the program; you can point an instance at your own host too. Every download is validated like an upload. See [Marketplace](/marketplace/).

## Permissions

Permission groups such as `crm.user` or `invoicing.manager` are named by models, fields and actions. People hold groups; agent tokens hold a subset of their owner's groups. See [Permission groups](/docs/concepts/permission-groups/).

## Data classes and masking

Every field declares a data class (none, public directory, internal, confidential HR, special category, financial, secret) and whether it is personal data. People see values under their permissions. Agents see personal data as stable placeholder tokens and sensitive classes not at all, and can pass tokens back to refer to a value without seeing it. See [Data classes and masking](/docs/concepts/data-classes-and-masking/).

## Workflows and approvals

State changes are named actions. An action can need a stricter group and can need approval. An agent can only propose an action that needs approval: the server returns a plan and changes nothing until a person approves it. Deleting records is also proposed by agents. See [Workflows and approvals](/docs/concepts/workflows-and-approvals/).

## Three surfaces, one set of commands

| Surface | For                       | How                                                                                            |
| ------- | ------------------------- | ---------------------------------------------------------------------------------------------- |
| Web app | People                    | Generated from the module descriptions: lists, boards, forms.                                  |
| REST    | Scripts, services, agents | `POST /api/{model}/{command}` with a bearer token. [REST reference](/docs/reference/rest-api/) |
| MCP     | AI assistants             | `obop mcp` over stdio, one tool per command. [Connect over MCP](/docs/agents/mcp/)             |
| CLI     | Shells and cron           | `obop call model command key=value`. [CLI reference](/docs/reference/cli/)                     |

All of them call the same dispatcher, so permission checks, masking and approvals are identical.

## What it is not (yet)

Be aware before you commit: there are no code tiers beyond declarative packages (no WebAssembly components or remote apps), so an app package runs no code of its own and cannot declare calls to other systems, though it can declare computed totals and actions that change other records in the same database; only some apps limit which records a person sees; the security log records account, sign-in, security-setting and module events but not changes to business records; the planned export job for a whole business is not built (a list exports as CSV, and `obop backup --plain` writes every record of a database to an unencrypted file); and scheduled backups of the hosted service are not built yet. The [roadmap](/roadmap/) lists what is built and what is not.
