---
title: "Agents"
description: "How to set up AI agents to work with obop: the agent model, scoped tokens, masked personal data, and approvals that keep a person in charge of anything irreversible."
lead: "obop treats AI agents as first-class users that are limited on purpose. This page explains the model; the next pages show how to set one up."
---

## The agent model

An **agent** is anything that acts on obop with an agent token: an MCP client such as Claude Desktop or Cursor, a script that calls the REST API, or the command line. Agents work on the same records and through the same commands as people. Four rules make that safe.

1. **Scoped.** An agent token carries a list of permission groups (its _scopes_). It can only narrow what its owner may do, never widen it. Asking for a group the owner does not hold is refused.
2. **Masked.** Personal data comes back as stable placeholder tokens such as `EMAIL_1`. Financial, HR and special-category fields are withheld. An agent can still reason about records and pass tokens back, but it never sees the real values. See [Masking tokens](/docs/agents/masking/).
3. **Propose, don't commit.** An agent can run ordinary actions within its scopes, but for anything irreversible (posting an invoice, paying, refunding, sending, publishing, deleting) it can only _propose_. The server returns a plan id and changes nothing. A person approves. See [The approval loop](/docs/agents/approvals/).
4. **Same door.** REST, MCP and the CLI go through one dispatcher, so an agent has exactly the same permissions, masking and approvals whichever way it connects.

## Three ways to connect

| Surface        | Use it when                                                                                                  | Page                                   |
| -------------- | ------------------------------------------------------------------------------------------------------------ | -------------------------------------- |
| MCP over stdio | An MCP client (Claude Desktop, Claude Code, Cursor, VS Code, Windsurf and others) should call obop as tools. | [Connect over MCP](/docs/agents/mcp/)  |
| REST           | A script or service calls HTTP endpoints.                                                                    | [Use the REST API](/docs/agents/rest/) |
| CLI            | A shell or a cron job calls `obop call`.                                                                     | [Use the CLI](/docs/agents/cli/)       |

## Set-up in five steps

1. Decide what the agent should do, and which groups that needs. Less is better.
2. [Create an agent token](/docs/agents/tokens/) with only those scopes.
3. Connect the agent over [MCP](/docs/agents/mcp/), [REST](/docs/agents/rest/) or the [CLI](/docs/agents/cli/).
4. Try a harmless read, then a change that needs approval, and approve it as a person.
5. Read [Safety best practices](/docs/agents/safety/) before you give the token to anything unattended.

## What exists today, and what does not

- Built: scoped tokens, masking and restore, approval plans, MCP over stdio, REST and the CLI.
- Not built yet: an HTTP (remote) MCP transport with OAuth, so hosted instances cannot yet be reached over MCP; a screen for creating or revoking agent tokens; a list of pending approvals in the interface. Agent tokens do not expire and there is no revoke command yet. These are called out again where they matter.

See [Example workflows](/docs/agents/workflows/) for prompts and flows to start from, and [Troubleshooting](/docs/agents/troubleshooting/) when something does not work.
