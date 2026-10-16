---
title: "Roadmap"
description: "An honest roadmap for obop: what is built today, what is partly built, what is not built, and the planned waves from the product plan, with the decisions that are still open."
lead: "What exists, what does not, and what is planned. Dates in the product plan are proposals, not promises."
eyebrow: "Built and planned"
---

obop is in beta. This page separates what runs today from what the plan describes. The source documents are `docs/ROADMAP.md` and `docs/DECISIONS.md` in the repository.

## Built today

- **Kernel:** one command dispatcher behind REST, MCP over stdio and the CLI, with permission groups, field-level access, data classes, masking for agents, approvals and governed state fields.
- **Storage:** SQLite and PostgreSQL.
- **Apps:** five built in (Contacts, Products, CRM, Sales, Invoicing) and 22 declarative packages in the marketplace, installable and removable at runtime.
- **Users:** Argon2id passwords, sessions, scoped agent tokens.
- **Web app:** generated screens: launcher, list, board, record panel, Customize (theme, apps, module builder), light and dark themes.
- **Hosted beta:** a control plane that creates instances with their own database, on Sliplane.
- **This website:** multi-page, static, in seven languages, with Markdown copies of every page.
- **Tests:** Rust tests, and a Playwright end-to-end suite over the built app.

## Partly built or simplified

- Bundled apps record data but compute nothing: totals, balances, stock levels, tax amounts and numbering are entered by people.
- Accounting stores a chart of accounts, entries and periods, but does not check that debits equal credits.
- Knowledge visibility and Sign immutability are conventions, not enforced rules.
- MCP exists over stdio only, with minimal tool schemas.

## Not built yet

- **Code tiers** for modules: expressions (CEL), WebAssembly components, remote apps. Until then packages cannot compute or call out.
- **Remote MCP** (HTTP transport with OAuth), so hosted instances are not reachable by desktop MCP clients.
- **Agent token management:** a screen, listing, expiry and revocation.
- **Audit log**, row-level permissions, single sign-on, two-factor sign-in.
- **Schema manager and migrations**; records are JSON documents.
- **Backups, point-in-time recovery and a full export tool.**
- **Localisation of the application** itself. The website is localised; the web app is English.
- **Country packs** (tax, e-invoicing, charts of accounts for Germany, France, Poland) and the e-invoicing formats the plan names.
- **Importers** from other systems.
- **Billing.** Nothing is charged; plans are planned.

## The plan in waves

The product plan names five waves. All dates are proposals.

| Wave | Window | Aims |
|---|---|---|
| W0 Foundations | 2026-10 to 2026-12 | The kernel, first tagged preview build, security policy. |
| W1 Loop | 2027-01 to 2027-09 | The seven-app loop (contacts, products, CRM, sales, invoicing, accounting, purchase and stock basics), MCP, code tiers X1, X2 and X4, a first country pack, public alpha and a paid beta. |
| W2 Release | 2027-10 to 2028-06 | Services apps, more country packs, WebAssembly host, a 1.0 long-term-support release with five years of support. |
| W3 Breadth | 2028-07 to 2029-06 | More apps (manufacturing, point of sale, website, e-commerce, helpdesk and others), payroll engine, custom domains, partner programme. |
| W4 Ecosystem | from 2029-07 | Marketplace commerce, verticals, 2.0. |

## Decisions that are still open

Several choices are recorded as proposals and wait for the project owner, including the licence (AGPL is proposed), the pricing and the exact calendar. Pricing on this site is therefore marked as planned and not charged.

## How to follow along

The [changelog](/changelog/) is generated from the commit history. The repository has the decision register and the full roadmap.
