---
title: "Security and privacy"
description: "How obop protects data today: Argon2id passwords, hashed tokens, data classes, masking for agents, approvals, one database per business, and the limits and gaps stated plainly."
---

This page describes what the code does today. For how to report a problem, see [Security](/security/).

## What is built

| Area                | What obop does                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| ------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Passwords           | Argon2id hashes. Minimum 12 characters wherever a password is set or changed. Sign-in uses a dummy hash for unknown users so timing does not reveal who exists.                                                                                                                                                                                                                                                                                 |
| Sessions and tokens | Bearer tokens are random 256-bit values stored only as SHA-256 hashes. Sessions expire after 12 hours. Five failed sign-ins lock a username for a minute.                                                                                                                                                                                                                                                                                       |
| Agents              | Scoped tokens that can only narrow their owner's groups. An agent cannot create tokens or approve plans.                                                                                                                                                                                                                                                                                                                                        |
| Data classes        | Every field has a class. A field without one cannot be installed. Personal data cannot be class `none`.                                                                                                                                                                                                                                                                                                                                         |
| Masking             | Agents see personal data as reversible placeholder tokens and sensitive classes not at all. A token sent back is replaced by the real value only when a text value in the request is exactly a token from that agent token's (or MCP session's) own table. The server checks only that the caller holds at least one permission group, not that it can still read the record the token came from; an unknown token is treated as ordinary text. |
| Approvals           | Irreversible actions and deletes are proposals when an agent calls them.                                                                                                                                                                                                                                                                                                                                                                        |
| Command boundary    | One dispatcher serves REST, MCP and the CLI, so no surface skips permissions or masking. Unknown, hidden and forbidden commands look the same.                                                                                                                                                                                                                                                                                                  |
| Isolation           | Each instance has its own database; one that a control plane creates on PostgreSQL also gets its own database role.                                                                                                                                                                                                                                                                                                                             |
| Two-factor          | Optional TOTP (RFC 6238) or passkeys (WebAuthn) with single-use recovery codes, required for administrators by default. Secrets are sealed under a server key, codes cannot be replayed, every step is in an append-only security log. See [Two-factor authentication](/docs/concepts/two-factor-authentication/).                                                                                                                              |
| Auth storage        | Users and tokens sit in storage outside the module registry, so no generated command can read them.                                                                                                                                                                                                                                                                                                                                             |
| Secrets             | Fields of class `secret` are never returned to anyone.                                                                                                                                                                                                                                                                                                                                                                                          |

## Data you hold

obop stores what your business enters in the database of your instance, wherever you run it. SpaceCorps does not host obop for other people yet. See [Privacy](/privacy/) for what this website collects.

## What is not built yet

Honest list, so you can decide:

- **No audit trail of reads and changes to business records** yet, and no marking of who or which agent changed a record. A security log of events such as account, sign-in, security-setting and module events exists.
- **No command to revoke a single agent token** and no expiry. An agent token works until its owner is removed. An app connected through the browser sign-in can be revoked under Customize, Security, Connected apps.
- **Row rules in some apps only.** Everywhere, access is by model and field group. Some apps (for example Expenses, Timesheets, HR leave requests, Documents and Dashboards) also limit which records a person sees; CRM, Sales and Invoicing do not. A rule covers whole records, not fields.
- **No database-level encryption of records.** obop encrypts what it controls: secrets such as two-factor seeds and connection credentials (sealed under `OBOP_SECRET_KEY`, which `obop secrets rotate` changes) and sealed backups. The database itself must sit on encrypted storage that you provide; `obop storage attest` records who attested that.
- **No point-in-time recovery**, and a control plane does not switch on scheduled backups of the instances it creates. Self-hosted instances have `obop backup` and `OBOP_BACKUP_SCHEDULE`; obop does not run point-in-time recovery itself.
- **Single sign-on only with Microsoft Entra, and not yet tried against a real tenant.** It covers one Microsoft tenant and is off until an administrator sets it up. Two-factor is an authenticator app or a passkey (no SMS), passkeys are tested only with software and virtual authenticators, and the server holds the key that seals the authenticator-app secrets.
- **No customer-facing export tool.** A list exports as CSV, and `obop backup --plain` writes every record of a database to an unencrypted file; the planned export job for a whole business is not built.
- **Masking is not free-text aware**, and a placeholder resolves only for the agent token (or MCP session) that received it. Over REST and over MCP on HTTP the list of placeholders is kept sealed in the database and survives a restart; it is forgotten 24 hours after its last change (REST) or two hours after the last request of the session (MCP). Over the command `obop mcp` it lives in the command's memory and ends when the command stops.
- **No certification.** obop has no SOC 2, ISO 27001 or similar attestation, and no independent penetration test yet.
- **TLS** is the responsibility of your proxy or platform.
- **Records are JSON documents.** The database does not enforce field types; the command layer does.

## Data protection notes

- Contacts has an `erase` action that anonymises a party in place and keeps references valid. It needs the `privacy.officer` group; an agent can only propose it, and a person in that group approves. Other apps have no erase action yet.
- Marking `personal_data` correctly is what lets masking and, later, access and erasure requests work. The install gate refuses personal data classed `none`.
- A Data Processing Agreement and sub-processor list do not exist yet. The [Terms](/terms/) and [Privacy](/privacy/) pages are drafts.

## Reporting vulnerabilities

Use a private GitHub security advisory. Details and what to expect are on the [Security](/security/) page and in `/.well-known/security.txt`.
