# obop: one business, one platform

An open-source operating system for businesses: accounting, invoicing, sales, CRM, stock and, later, manufacturing, HR, projects, POS, e-commerce and more, as modules on one shared data model. Odoo is the functional reference; obop is a clean-room re-implementation of its concepts, built in Rust with a Vite+ frontend.

- **Completely free software.** Apache-2.0, self-hostable, no paid tier, no enterprise layer, no per-user fee. The one planned paid thing is a hosting service run by SpaceCorps, which restricts nothing in the software (OR-22; planned, not offered or charged yet).
- **Agentic first, CLI for everything.** Every capability is a typed command in the `obop` CLI, the REST API and MCP. The web UI is a client of the same commands.
- **One binary plus PostgreSQL.** Rust backend, React on Vite+, no Node on your server.

## Status

**Early code.** Research and product definition were completed on 2026-10-01 and are Draft 0.1 documents (most decisions are still Proposed). A first working core now exists in this repository: the `obop` binary (server, generated CLI, REST and MCP on one command catalogue), a registry-driven backoffice on Vite+, storage on SQLite locally and PostgreSQL for deployments, an installable package system, and a few modules (base, product, CRM, sales, invoicing). Accounting with tax, German e-invoicing, stock and most of the Odoo-class suite exist only as specifications and as small declarative sample packages (`packages/`), not as the finished modules. To run what exists, see [docs/DEPLOY.md](docs/DEPLOY.md) and [docs/HANDOFF.md](docs/HANDOFF.md).

The repository `SpaceCorps/obop` is public (since 2026-10-26). "obop" is a working name pending trademark clearance. The project is self-funded; the team is the owner plus AI agents. Whether that is enough to deliver on schedule is a named risk, not an assumption.

## What makes it different

These are design commitments of the documents; the Status section says what exists today.

1. **Everything is open and free,** including full accounting, reconciliation, reports and the API.
2. **Agents are first-class.** Agents draft, humans post. Bring your own AI key; everything also works without AI.
3. **The books are protected by the database:** balance, immutability, gapless numbers and a hash chain are enforced in PostgreSQL, not only in application code.
4. **Upgrades are a feature:** migrations are planned and rehearsed before they run (`obop migrate plan|rehearse`), and before 1.0 an N-1 upgrade path and a 12-month deprecation window are promised; from 1.0 the promise is a 2-year security-fix window (the deprecation window after 1.0 is open).
5. **Compliance is signed, tested country packs,** Germany first (XRechnung, Factur-X, DATEV export, GoBD).
6. **A free plugin registry.** From the CLI you can search, install (version-pinned) or copy a plugin's source into your own instance. Only plugins with an OSI-approved licence are listed, and there are no payments.
7. **Leaving is easy:** full export on every install and an Odoo importer.

Honest limits: Odoo Community has 642 addon directories and obop starts with a fraction of that breadth. There is no speed claim until a benchmark exists.

## Tech stack (fixed by the owner)

| Layer | Rule |
|---|---|
| Backend | Latest stable Rust, adopted after a 7 to 14 day cooldown or its `x.y.1` patch |
| Frontend and tooling | Latest Vite+, driven only through the `vp` CLI. Two named exceptions: the nightly exit drill and `pnpm sbom` |
| Package manager | pnpm only. Never `npm`, `npx`, `yarn` or `bun`; only `pnpm-lock.yaml` is committed |
| Database | PostgreSQL 17 and 18 (D8), one database per business; SQLite for local use (D44) |

Details and current versions: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and [CLAUDE.md](CLAUDE.md).

## First delivery

Wave W1, 2027-01-01 to 2027-09-30, cut to the smallest honest loop: the kernel, invoicing, accounting with tax, German e-invoicing, CRM and sales, and one-warehouse stock, with CLI, REST and MCP for all of it and a thin web UI. The 0.5 public beta (2027-07-06, proposed) is meant to open a free hosted sandbox with sample data (OR-20); the sandbox opens with it only if its duties are in place and the 2027-06-01 go/no-go passes, and the beta does not wait for it. The German pack is targeted stable on 2027-09-30, ahead of Germany's all-issuer e-invoicing mandate on 2028-01-01, and 1.0 (the first stable release, with a 2-year security-fix window) on 2028-04-18. Purchase orders, portal, payment providers, Peppol transport, project and timesheet, POS, manufacturing and payroll come later. Full calendar: [docs/ROADMAP.md](docs/ROADMAP.md).

## Licence and contributing

Apache-2.0 for the whole project ([LICENSE](LICENSE), [NOTICE](NOTICE)), with no copyleft layer, no `ee` tree and nothing paid in the software (OR-1, OR-2); SpaceCorps is the publisher, copyright holder and trademark holder (OR-10), and Apache-2.0 grants no trademark rights. Contributions carry a DCO sign-off and no CLA: the owner signs off every commit, and agents are recorded with a `Co-Authored-By` trailer. Nothing here is legal advice; open legal items are listed for counsel in [docs/BUSINESS.md](docs/BUSINESS.md).

## Where to start

| You are | Read |
|---|---|
| Anyone | [docs/PRODUCT.md](docs/PRODUCT.md), the master product description |
| The owner | [docs/DECISIONS.md](docs/DECISIONS.md): your rulings so far, every decision, and the questions still open |
| An agent or script author | [docs/spec/cli.md](docs/spec/cli.md), [docs/spec/ai.md](docs/spec/ai.md), [docs/spec/integrations.md](docs/spec/integrations.md) |
| An engineer | [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), [docs/CONVENTIONS.md](docs/CONVENTIONS.md), [docs/ENGINEERING.md](docs/ENGINEERING.md) |
| An accountant or compliance officer | [docs/LOCALIZATION.md](docs/LOCALIZATION.md), [docs/spec/finance.md](docs/spec/finance.md) |

## All documents

| Document | Covers |
|---|---|
| [CLAUDE.md](CLAUDE.md) | Hard constraints and working rules |
| [docs/PRODUCT.md](docs/PRODUCT.md) | Master product description |
| [docs/DECISIONS.md](docs/DECISIONS.md) | Owner rulings, decision register, open questions |
| [docs/CONVENTIONS.md](docs/CONVENTIONS.md) | Names and shared contracts (events, permission groups, data classes, ports) |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | System design and stack |
| [docs/ROADMAP.md](docs/ROADMAP.md) | Waves, calendar, capacity, gates |
| [docs/BUSINESS.md](docs/BUSINESS.md) | Licensing, governance, sustainability, risks |
| [docs/LOCALIZATION.md](docs/LOCALIZATION.md) | Country packs and compliance |
| [docs/ENGINEERING.md](docs/ENGINEERING.md) | Quality, CI/CD, release and support policy, legal pack |
| [docs/GLOSSARY.md](docs/GLOSSARY.md) | Terms |
| [docs/spec/finance.md](docs/spec/finance.md) | Accounting, invoicing, tax, e-invoicing |
| [docs/spec/sales-crm.md](docs/spec/sales-crm.md) | Catalog, pricing, CRM, sales, subscriptions |
| [docs/spec/supply-chain.md](docs/spec/supply-chain.md) | Inventory, purchase, manufacturing |
| [docs/spec/retail-commerce-marketing.md](docs/spec/retail-commerce-marketing.md) | POS, eCommerce, website, marketing |
| [docs/spec/people.md](docs/spec/people.md) | HR, expenses, payroll strategy |
| [docs/spec/services.md](docs/spec/services.md) | Projects, timesheets, helpdesk, planning |
| [docs/spec/platform.md](docs/spec/platform.md) | Kernel and platform apps |
| [docs/spec/ux.md](docs/spec/ux.md) | Views, design system, accessibility |
| [docs/spec/ai.md](docs/spec/ai.md) | Agents and MCP |
| [docs/spec/cli.md](docs/spec/cli.md) | The `obop` CLI, CLI/API/MCP parity, the plugin registry |
| [docs/spec/integrations.md](docs/spec/integrations.md) | API, connectors, portability |
| [docs/spec/security-privacy.md](docs/spec/security-privacy.md) | Identity, audit, privacy |
| [docs/spec/mobile.md](docs/spec/mobile.md) | React Native apps, background location, the mileage package |
| [docs/research/](docs/research/README.md) | 28-file research corpus (predates the owner rulings; the rulings win) |
| [docs/HANDOFF.md](docs/HANDOFF.md) | Handoff and operations guide for the next agent or a new machine |
| [docs/DEPLOY.md](docs/DEPLOY.md) | Operator guide: run, back up, upgrade and update obop |
