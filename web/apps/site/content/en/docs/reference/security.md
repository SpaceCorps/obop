---
title: "Security and privacy"
description: "How obop protects data today: Argon2id passwords, hashed tokens, data classes, masking for agents, approvals, one database per business, and the limits and gaps stated plainly."
---

This page describes what the code does today. For how to report a problem, see [Security](/security/).

## What is built

| Area                | What obop does                                                                                                                                                    |
| ------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Passwords           | Argon2id hashes. Minimum 12 characters when created from the command line. Sign-in uses a dummy hash for unknown users so timing does not reveal who exists.      |
| Sessions and tokens | Bearer tokens are random 256-bit values stored only as SHA-256 hashes. Sessions expire after 12 hours. Five failed sign-ins lock a username for a minute.         |
| Agents              | Scoped tokens that can only narrow their owner's groups. An agent cannot create tokens or approve plans.                                                          |
| Data classes        | Every field has a class. A field without one cannot be installed. Personal data cannot be class `none`.                                                           |
| Masking             | Agents see personal data as reversible placeholder tokens and sensitive classes not at all. Tokens are restored on write only if the agent could read the record. |
| Approvals           | Irreversible actions and deletes are proposals when an agent calls them.                                                                                          |
| Command boundary    | One dispatcher serves REST, MCP and the CLI, so no surface skips permissions or masking. Unknown, hidden and forbidden commands look the same.                    |
| Isolation           | On the hosted service each business has its own database and database role.                                                                                       |
| Auth storage        | Users and tokens sit in storage outside the module registry, so no generated command can read them.                                                               |
| Secrets             | Fields of class `secret` are never returned to anyone.                                                                                                            |

## Data you hold

obop stores what your business enters. The hosted beta stores it in a PostgreSQL database that is yours alone. See [Privacy](/privacy/) for what the website and the sign-up service collect.

## What is not built yet

Honest list, so you can decide:

- **No audit log** of reads and changes yet, and no marking of who or which agent changed a record.
- **No revoke command** for agent tokens and no expiry. A token works until its owner is removed.
- **No row-level permissions.** Access is by model and field group.
- **No encryption at rest or key management** beyond what your database provider gives you.
- **No backups or point-in-time recovery** for the hosted service.
- **No single sign-on, two-factor sign-in or passkeys.** Password sign-in only.
- **No data export tool.** Self-hosted data is in your database.
- **Masking is not free-text aware** and tokens live in server memory (not stable across restarts or sessions).
- **No certification.** obop has no SOC 2, ISO 27001 or similar attestation, and no independent penetration test yet.
- **TLS** is the responsibility of your proxy or platform.
- **Records are JSON documents.** The database does not enforce field types; the command layer does.

## Data protection notes

- Contacts has an `erase` action that anonymises a party in place and keeps references valid. It needs approval and the `privacy.officer` group. Other apps have no erase action yet.
- Marking `personal_data` correctly is what lets masking and, later, access and erasure requests work. The install gate refuses personal data classed `none`.
- A Data Processing Agreement and sub-processor list do not exist yet. The [Terms](/terms/) and [Privacy](/privacy/) pages are drafts.

## Reporting vulnerabilities

Use a private GitHub security advisory. Details and what to expect are on the [Security](/security/) page and in `/.well-known/security.txt`.
