---
title: "FAQ"
description: "Frequently asked questions about obop: status, licence, pricing, data, agents and MCP, apps, hosting, languages and how it compares."
---

## General

### What is obop?

An open-source business platform with CRM, sales, invoicing, products, contacts and connectors built in, 24 more apps bundled as packages in a marketplace (22 of them have a page on this site), and a REST API, MCP server and command line generated from the same definitions. See [Platform](/platform/).

### Is it ready for production?

Not yet. obop is below version 1.0, so a release can still change things. The six built-in apps are marked Stable, the other apps Beta or Experimental. Read the [roadmap](/roadmap/) and the limits on [Security and privacy](/docs/reference/security/) before you rely on it.

### What does it cost?

obop is free to use and to self-host. Dedicated hosting on a server of its own is planned from €29 a month, plus €15 for each extra 50 GB of storage; it is not offered or charged yet. See [Pricing](/pricing/).

### Which licence?

The code is on GitHub under the Apache-2.0 licence for the whole project: free to use, change and self-host, with no paid edition. The full text is in the [LICENSE](https://github.com/SpaceCorps/obop/blob/main/LICENSE) file.

### What is it written in?

Rust for the backend and Vite+ with React for the web apps. This website is built with VitePress.

## Using obop

### How do I add an app?

Open _Customize_, then _Marketplace_, and install one. From the command line: `obop modules install-from helpdesk`. You can also upload your own `obop.toml`. See [Apps and modules](/docs/concepts/apps-and-modules/).

### Can I write my own app?

Yes, as one text file. A package can declare computed totals, numbered fields and actions that change other records. There is no code tier yet, so it runs no code of its own and cannot declare calls to other systems. See the [module manifest reference](/docs/reference/module-manifest/).

### Can my app calculate totals?

Yes, simple ones. A field can be computed from the record's own fields and its lines, for example `sum(lines.amount)` or `quantity * unit_price`, and cannot then be edited. Packages are declarative on purpose, so administrators can install them safely: the expression language is deliberately small and cannot call anything. CEL expressions and WebAssembly extensions are planned.

### What happens to my data if I uninstall an app?

The commands and screens go away and the data stays in the database. Installing the app again brings the records back.

### Can I change an installed app?

Raise its `version` and install the file again. Adding optional fields needs nothing more. To rename or drop a field, or to fill a value into existing records, declare a `[[migration]]` step in the new version; see [Apps and modules](/docs/concepts/apps-and-modules/).

## Agents

### Can an agent see my customers' emails?

Not in clear. It sees placeholder tokens such as `EMAIL_1`. See [Masking tokens](/docs/agents/masking/).

### Can an agent post an invoice or delete a record?

It can propose either; a person approves. See [The approval loop](/docs/agents/approvals/).

### Does obop work with Claude, ChatGPT, Cursor or other tools?

obop has an MCP server over stdio, an MCP endpoint at `/mcp` on every instance (Streamable HTTP, with an agent token or a browser sign-in) and a REST API; see [Connect over MCP](/docs/agents/mcp/). The remote endpoint was tried with the MCP Inspector and our own automated tests. The browser sign-in for Claude Desktop, claude.ai and Claude Code has not been tried with those apps yet.

### How do I stop an agent?

There is no command to revoke a single token made with `obop agent-token` or the REST API yet: remove or disable the owning user (their tokens stop working at once), or stop giving the token to the agent. An app that connected through the browser sign-in is listed under _Customize, Security, Connected apps_, where its owner can revoke it at once. See [Safety](/docs/agents/safety/).

## Hosting and data

### Where is my data?

Hosted: in a database that belongs to your instance, on PostgreSQL. Self-hosted: wherever you run it.

### How do I back up?

Run `obop backup --db URL --out DIR`: a consistent copy taken while the server runs, sealed (encrypted) when a key is set, which `obop restore` reads back; see the [CLI reference](/docs/reference/cli/) and [Self-hosting and operations](/docs/reference/self-hosting/). `OBOP_BACKUP_SCHEDULE` makes `obop serve` take backups on a timer. Point-in-time recovery is not run by obop; your database server needs its own for that. The hosted service does not switch on scheduled backups of its databases yet.

### Can I export everything?

Your database is yours. A list exports as CSV from the web app, and `obop backup --plain` writes every record of a database as gzip JSON Lines. The planned export job for a whole business is not built.

### Which languages is the website in?

English, German, French, Polish, Finnish, Swedish and Spanish. The web app is available in the same seven languages. The translations on this site and the translated labels in the apps are machine-drafted, and no native speaker has reviewed the ones on this site yet; [you can help review one](/about/community/).

## Contributing and contact

See [About](/about/), [Contact](/contact/) and the [community](/about/community/).
