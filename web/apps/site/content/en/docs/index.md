---
title: "Documentation"
description: "Documentation for obop: get started, learn the concepts, set up AI agents over MCP, REST and the CLI, and look up the reference."
lead: "How to run obop, build on it and connect AI agents to it. Everything here was checked against the code and, where there is a command, run."
---

obop is an open-source business platform. One program serves the web app, a REST API, an MCP server and a command line from the same definitions, on one data model.

## Where to start

- **New to obop?** Read [Getting started](/docs/getting-started/). It covers the hosted beta, self-hosting with Docker and PostgreSQL, and local development with SQLite.
- **Want an AI agent to work with obop?** Start with [Agents](/docs/agents/), then [create an agent token](/docs/agents/tokens/) and [connect over MCP](/docs/agents/mcp/).
- **Building your own app?** The [module manifest reference](/docs/reference/module-manifest/) explains how to describe models, fields, workflows and permissions in one `obop.toml` file.
- **Looking something up?** See the [CLI](/docs/reference/cli/), [REST API](/docs/reference/rest-api/), [environment variables](/docs/reference/environment/) and the [FAQ](/docs/reference/faq/).

## How the documentation is organised

| Section | What it answers |
|---|---|
| Start here | How do I get an instance and use it? |
| Concepts | What are apps, data classes, permission groups, workflows and instances? |
| Agents | How do I set agents up safely, and what can they do? |
| Reference | Exact commands, endpoints, settings and limits. |

## Status

obop is in beta. Where a feature is not built yet, the page says so plainly. The [roadmap](/roadmap/) lists what exists and what does not, and the [changelog](/changelog/) lists what changed.

Every page here is also available as plain Markdown: add `.md` instead of the trailing slash, or read the whole documentation in one file at [llms-full.txt](/llms-full.txt).
