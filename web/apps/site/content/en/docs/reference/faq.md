---
title: "FAQ"
description: "Frequently asked questions about obop: status, licence, pricing, data, agents and MCP, apps, hosting, languages and how it compares."
---

## General

### What is obop?

An open-source business platform with CRM, sales, invoicing, products and contacts built in, 22 more apps in a marketplace, and a REST API, MCP server and command line generated from the same definitions. See [Platform](/platform/).

### Is it ready for production?

Not yet. obop is in beta. Read the [roadmap](/roadmap/) and the limits on [Security and privacy](/docs/reference/security/) before you rely on it.

### What does it cost?

Self-hosting is free. Hosted plans are planned at €0, €49, €249 and from €899 a month per organisation. Nothing is charged during the beta. See [Pricing](/pricing/).

### Which licence?

The code is on GitHub. The final licence is not decided; the project proposes the AGPL for the core. Check the repository for current terms.

### What is it written in?

Rust for the backend and Vite+ with React for the web apps. This website is built with VitePress.

## Using obop

### How do I add an app?

Open *Customize*, then *Marketplace*, and install one. From the command line: `obop modules install-from helpdesk`. You can also upload your own `obop.toml`. See [Apps and modules](/docs/concepts/apps-and-modules/).

### Can I write my own app?

Yes, as one text file. There is no code tier yet, so apps cannot compute totals, generate numbers or call other systems. See the [module manifest reference](/docs/reference/module-manifest/).

### Why can't my app calculate totals?

Packages are declarative on purpose, so administrators can install them safely. Expressions and WebAssembly extensions are planned.

### What happens to my data if I uninstall an app?

The commands and screens go away and the data stays in the database. Installing the app again brings the records back.

### Can I change an installed app?

Raise its `version` and install the file again. There are no migrations: add optional fields rather than renaming.

## Agents

### Can an agent see my customers' emails?

Not in clear. It sees placeholder tokens such as `EMAIL_1`. See [Masking tokens](/docs/agents/masking/).

### Can an agent post an invoice or delete a record?

It can propose either; a person approves. See [The approval loop](/docs/agents/approvals/).

### Does obop work with Claude, ChatGPT, Cursor or other tools?

obop has an MCP server over stdio and a REST API. Any MCP client that can start a local process works; see [Connect over MCP](/docs/agents/mcp/). There is no remote MCP endpoint yet, so hosted instances are reached over REST.

### How do I stop an agent?

There is no revoke command yet. Remove the owning user, or stop giving the token to the agent. See [Safety](/docs/agents/safety/).

## Hosting and data

### Where is my data?

Hosted: in a database that belongs to your instance, on PostgreSQL. Self-hosted: wherever you run it.

### How do I back up?

Use your database's tools (`pg_dump`, snapshots) or copy the SQLite file while the server is stopped. obop has no built-in backup yet.

### Can I export everything?

Your database is yours. A built-in full export is planned but not built.

### Which languages is the website in?

English, German, French, Polish, Finnish, Swedish and Spanish. The application itself is in English for now. The translations on this site are machine-drafted and are being reviewed by native speakers.

## Contributing and contact

See [About](/about/), [Contact](/contact/) and the [community](/about/community/).
