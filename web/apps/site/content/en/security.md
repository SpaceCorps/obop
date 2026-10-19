---
title: "Security"
description: "Security at obop: how to report a vulnerability, what we do with a report, and a summary of the protections that exist today and the gaps that remain."
lead: "How to report a problem, and what protects your data today."
eyebrow: "Trust"
---

## Report a vulnerability

Please report security problems privately, not in public issues or the forum.

1. Open a **private security advisory** on GitHub: [github.com/SpaceCorps/obop/security/advisories/new](https://github.com/SpaceCorps/obop/security/advisories/new).
2. Describe what you found, how to reproduce it, and what you think the impact is. Do not include real personal data.
3. Give us reasonable time to fix it before you publish.

The same contact is in [`/.well-known/security.txt`](/.well-known/security.txt).

### What to expect

obop is a young project run by a small team, so we cannot promise a response time yet. Our intent is to acknowledge a report, work with you on a fix, credit you if you want, and publish an advisory when a fix is out. There is no bug bounty.

### Please do

- Test only against your own instance or a local build.
- Stop and report if you reach data that is not yours.

### Please do not

- Run denial-of-service tests against the hosted beta.
- Access, change or delete other people's data.
- Use social engineering or physical attacks.

## What protects you today

- Passwords are stored with Argon2id; tokens are stored only as hashes.
- Every field has a data class; agents get masked personal data and no sensitive classes.
- Agents act with scoped tokens and can only propose irreversible steps; a person approves.
- One dispatcher serves every surface, so permissions and masking cannot be skipped by choosing a different one.
- On the hosted service each business has its own database and role.

## What is missing

No audit log, no token revocation, no single sign-on, no backups for the hosted service, no certification and no independent penetration test yet. The [Security and privacy reference](/docs/reference/security/) lists these in full.

## Supply chain

The repository pins its JavaScript dependencies in a single lockfile and its Rust dependencies in `Cargo.lock`. Releases are not signed and no software bill of materials is published yet.
