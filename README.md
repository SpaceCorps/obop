# obop: one business, one platform

- **Status:** Draft 0.1
- **Date:** 2026-10-01
- **Scope:** Root pitch and navigation; detail lives in the documents in section 8.

**One business, one platform.**

## Answers in 60 seconds

Everything below is Proposed unless a binding decision is cited (section 2); prices are hypotheses.

| Question | Short answer |
|---|---|
| What is it? | An open-source operating system for businesses: sales, stock, invoicing, accounting, CRM and more as modules on one shared data model, with Odoo as the functional reference (section 1). |
| For whom? | Owner-led firms of 5 to 50 people in services and light trade, one company and one country at a time, Germany first (Proposed, D4). See [PRODUCT.md section 3](docs/PRODUCT.md#3-who-it-is-for). |
| Why would it win? | One open edition, a free generated API, books enforced by the database, upgrades with a local dry-run, signed country packs (section 3). No speed claim until a benchmark exists (D21). |
| What does it cost? | Nothing. The whole project is Apache-2.0 and completely free to use and to self-host: no paid tier, no enterprise layer, no price list (RATIFIED, D1, D2, OR-1, OR-2). The hosted plans, prices and paid list that [PRODUCT.md](docs/PRODUCT.md) and [BUSINESS.md](docs/BUSINESS.md) still describe are superseded. |
| When? | Nothing can be installed today. First delivery is wave W1, 2027-01-01 to 2027-09-30: paid beta 2027-07-06, German pack stable 2027-09-30, 1.0 LTS 2028-04-18 (Proposed, D26). |
| How is it built? | Rust backend, Vite+ frontend driven only through `vp`, pnpm as the only package manager, one binary plus PostgreSQL (F1, F2, D37). |
| Where does the project stand? | Pre-code and pre-formation: no seed round closed (about EUR 3M planned), no counsel retained, no design partner signed ([ROADMAP.md section 5.2](docs/ROADMAP.md#52-dependency-register)). |
| Top risks? | The funding gap (BR-01), the accounting, localisation and payroll burden (BR-02), customisation without recompiling (BR-03), and an uncleared name (section 7). Full register: [BUSINESS.md section 9](docs/BUSINESS.md#9-consolidated-risk-register). |
| What must the owner decide? | The licence is settled (D1, D2: OR-1, OR-2, OR-10, OR-18 of 2026-10-01). Sixteen other decisions and four Proposed interpretations (I-1 to I-4) are not ratified; the owner questions that remain open are listed in [DECISIONS.md section 8](docs/DECISIONS.md#8-open-questions-for-the-owner) (Q1, Q3, Q13 and Q19 are closed). |

## 1. What obop is

obop is an open-source operating system for businesses: sales, purchasing, stock, invoicing, accounting, payments, projects, support, HR and more, as modules on one shared data model. Odoo is the functional reference and obop re-implements its concepts and behaviour clean-room (F3), not as a code port, while changing the delivery contract its users complain about most: one open edition, a free generated API, upgrades as a product feature and compliance as signed country packs (Proposed, D2, D16). The backend is Rust, the frontend and tooling are Vite+, and the runtime is one binary plus PostgreSQL.

## 2. Status

**Pre-code: research and product definition complete as of 2026-10-01; nothing is built yet.** There is nothing to install or run.

- A 28-file fact-checked research corpus exists; the product, architecture, business and domain documents are Draft 0.1.
- The licence decisions D1 and D2 are RATIFIED (OR-1, OR-2, OR-10, OR-18). Sixteen decisions (D3, D4, D5, D12, D13, D14, D16, D17, D20, D23, D25, D26, D40 to D43) and interpretations I-1 to I-4 await the owner, as do the exit-drill clause of D15 and the renames and labels of D24; none is ratified. Unless a binding decision is cited, everything here is Proposed; figures marked (target), (estimate) or (hypothesis) are not measurements.
- The repository is public (since 2026-10-26). Counsel advice on the name, the CRA role and the DCO wording is still open (Q16); it is not a condition of the licence.

## 3. Headline differentiators (design commitments; nothing built yet)

These are commitments of the design, not shipped features. Where no "Proposed" mark appears, the cited decision is binding.

1. **Completely free:** one open edition under Apache-2.0, accounting, reconciliation and reports included; no paid list and no paid tier (D2, OR-2).
2. **Books protected by the database:** balance, immutability, gapless numbers and a hash chain enforced in PostgreSQL (D30).
3. **Upgrades as a product:** a local dry-run, contract migrations only at each `N.0` (Proposed, D16).
4. **A free generated API:** REST, OpenAPI 3.1, webhooks and MCP on every plan (D31).
5. **Compliance as signed, tested country packs,** Germany first (D34; Proposed, D4).
6. **Customisation as data,** exportable as packages; third-party code stays outside the trust domain (D11, D32).
7. **One binary plus PostgreSQL;** no Node on customer servers (D37).
8. **Agents draft, humans post;** bring your own AI key (D18).
9. **Leaving is easy:** full export in every edition (D38); Odoo importer (D22); QuickBooks importer (Proposed, D26).

Honest limits: Odoo Community has 642 addon directories [R01 s1.3]; obop starts with a fraction of that breadth, three country packs by 2028-08-31, and claims no speed multiple until a benchmark harness exists (D21).

## 4. Tech stack (fixed constraints)

Rust, Vite+ and pnpm are owner-mandated; PostgreSQL is a binding register decision.

| Layer | Constraint | State as of 2026-10-01 |
|---|---|---|
| Backend | Exclusively the latest stable Rust, edition 2024 (F1) | Pinned 1.98.1, intentionally one release behind the literal F1 wording until the owner ratifies the pin policy (Proposed, D23; Q10); 1.99.x adoption planned 2026-10-15 to 2026-10-22 [R06] |
| Frontend, tooling | Exclusively the latest Vite+ (`vite-plus`); every JS/TS task runs through the `vp` CLI (F2) | `vite-plus` 1.0.0, MIT, published 2026-09-28 [R07]; React 19, TypeScript 7 (D15); the nightly exit drill as a named exception to `vp` only is Proposed (I-4, Q10) |
| Package manager | pnpm only; `npm`, `npx`, `yarn` and `bun` are never used; only `pnpm-lock.yaml` is committed (F2) | pnpm still installs from the public npm registry |
| Database | PostgreSQL 17 and 18, one database per business (D6, D8) | 18 is the reference; 18.6 is current [R22] |

## 5. First delivery

Wave W1, 2027-01-01 to 2027-09-30 (Proposed, D25, D26). Three milestones replace the bare word "GA" (D26): paid beta 2027-07-06, German pack stable 2027-09-30, 1.0 LTS 2028-04-18.

- A free, compliant e-invoicing on-ramp (EN 16931, XRechnung, Peppol via a partner) for 5 to 50 person services and light-trade firms, Germany first (Proposed, D4).
- The seven-app loop: Contacts and Products, CRM, Sales, Inventory with Purchase (one warehouse), Invoicing with Accounting, Payments, Portal.
- Generated REST, OpenAPI and MCP; declarative packs, CEL, remote apps; Odoo and QuickBooks importers.
- Not in it: POS, payroll, manufacturing, multi-country tax, multi-company. France and Poland packs follow (2028-03-31, 2028-08-31).

## 6. Licence and contributing (RATIFIED, D1)

The whole project is **Apache-2.0** ([LICENSE](LICENSE), [NOTICE](NOTICE)): the core, first-party apps, SDKs and templates alike, with no copyleft layer, no `ee` tree and nothing paid (OR-1, OR-2). SpaceCorps is the publisher, copyright holder and trademark holder (OR-10); Apache-2.0 grants no trademark rights.

Contributions carry a DCO sign-off (`git commit -s`) and no CLA, so inbound equals outbound. The owner signs off every commit, and agents are recorded with a `Co-Authored-By` trailer and never add a sign-off (OR-18). Nothing here is legal advice; open legal items are listed for counsel in [docs/BUSINESS.md](docs/BUSINESS.md).

## 7. The name

"obop" is a working name pending trademark clearance (Proposed, D3). Clearance is incomplete and rated medium risk: the US register shows no OBOP record but a pending near-identical application and two live similar registrations, and the EU, UK, WIPO, German and French registers are unsearched [R28 A.1-A.4]. Nothing is announced, spent or filed until counsel reports. See [docs/ENGINEERING.md](docs/ENGINEERING.md) section 10.

## 8. Navigation

Start with [docs/PRODUCT.md](docs/PRODUCT.md), or jump in by reader.

| You are | Start with |
|---|---|
| An owner or decision-maker | [DECISIONS.md section 8](docs/DECISIONS.md#8-open-questions-for-the-owner), then [ROADMAP.md section 5.1](docs/ROADMAP.md#51-owner-decisions-on-the-calendar) |
| A customer or accountant | [PRODUCT.md section 3](docs/PRODUCT.md#3-who-it-is-for), [section 7.6](docs/PRODUCT.md#76-if-you-run-odoo-today) (if you run Odoo today) and [section 9](docs/PRODUCT.md#9-editions-and-pricing-summary) |
| An investor or funder | [BUSINESS.md section 6.7](docs/BUSINESS.md#67-funding-plan-and-hiring-gates-proposed-d5), [section 6.8](docs/BUSINESS.md#68-investor-case-briefly) and [section 9](docs/BUSINESS.md#9-consolidated-risk-register) |
| An implementer or partner | [BUSINESS.md section 7](docs/BUSINESS.md#7-partners-and-marketplace-terms), then [PRODUCT.md section 5](docs/PRODUCT.md#5-the-product) |
| An engineer or contributor | [ARCHITECTURE.md](docs/ARCHITECTURE.md), [CONVENTIONS.md](docs/CONVENTIONS.md) (names and shared contracts) and [ENGINEERING.md](docs/ENGINEERING.md) |
| Compliance or counsel | [LOCALIZATION.md](docs/LOCALIZATION.md), [ENGINEERING.md section 11](docs/ENGINEERING.md#11-legal-pack-items-for-counsel) and [BUSINESS.md section 10](docs/BUSINESS.md#10-legal-and-regulatory-items-for-counsel) |

| Document | Covers |
|---|---|
| [README.md](README.md) | This file |
| [CLAUDE.md](CLAUDE.md) | Hard constraints |
| [docs/PRODUCT.md](docs/PRODUCT.md) | Master product description |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Architecture and stack |
| [docs/DECISIONS.md](docs/DECISIONS.md) | Decision register (25 binding, 18 proposed; D15 and D24 are binding with Proposed parts) |
| [docs/CONVENTIONS.md](docs/CONVENTIONS.md) | Naming and contract authority: identifiers, events, permission groups, data classes, ports |
| [docs/ROADMAP.md](docs/ROADMAP.md) | Waves, calendar, team, gates |
| [docs/BUSINESS.md](docs/BUSINESS.md) | Licensing, pricing, economics, risks |
| [docs/LOCALIZATION.md](docs/LOCALIZATION.md) | Country packs, compliance |
| [docs/ENGINEERING.md](docs/ENGINEERING.md) | Quality, CI/CD, release, legal |
| [docs/GLOSSARY.md](docs/GLOSSARY.md) | Terms |
| [docs/spec/finance.md](docs/spec/finance.md) | Accounting, invoicing, tax |
| [docs/spec/sales-crm.md](docs/spec/sales-crm.md) | CRM, sales, subscriptions, portal |
| [docs/spec/supply-chain.md](docs/spec/supply-chain.md) | Inventory, purchase, manufacturing |
| [docs/spec/retail-commerce-marketing.md](docs/spec/retail-commerce-marketing.md) | POS, eCommerce, website, marketing |
| [docs/spec/people.md](docs/spec/people.md) | HR, expenses, payroll strategy |
| [docs/spec/services.md](docs/spec/services.md) | Projects, helpdesk, planning |
| [docs/spec/platform.md](docs/spec/platform.md) | Kernel and platform apps |
| [docs/spec/ux.md](docs/spec/ux.md) | Views, design system, accessibility |
| [docs/spec/ai.md](docs/spec/ai.md) | Agents and MCP |
| [docs/spec/integrations.md](docs/spec/integrations.md) | API, connectors, portability |
| [docs/spec/security-privacy.md](docs/spec/security-privacy.md) | Identity, audit, privacy |
| [docs/research/](docs/research/README.md) | 28-file research corpus |

## 9. Sources

Links are relative to `README.md`; D-, F-, I- and Q-IDs refer to [docs/DECISIONS.md](docs/DECISIONS.md).

| Tag | File |
|---|---|
| R01 | [docs/research/01-odoo-app-catalog.md](docs/research/01-odoo-app-catalog.md) |
| R06 | [docs/research/06-rust-stack.md](docs/research/06-rust-stack.md) |
| R07 | [docs/research/07-vite-plus-frontend.md](docs/research/07-vite-plus-frontend.md) |
| R17 | [docs/research/17-oss-licensing-business-models.md](docs/research/17-oss-licensing-business-models.md) |
| R22 | [docs/research/22-data-layer-sync-realtime-performance.md](docs/research/22-data-layer-sync-realtime-performance.md) |
| R28 | [docs/research/28-docs-community-support-brand.md](docs/research/28-docs-community-support-brand.md) |

- Handoff and operations guide for the next agent or a new machine: [`docs/HANDOFF.md`](docs/HANDOFF.md).
