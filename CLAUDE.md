# obop — one business, one platform

Open-source operating system for businesses: an Odoo-class modular suite (ERP, CRM, accounting, inventory, manufacturing, HR, projects, POS, e-commerce, website, marketing, helpdesk, documents) on one shared data model. Clean-room re-implementation of Odoo's concepts and behaviour, not a code port.

## Hard tech constraints (set by the owner)

- **Backend:** exclusively the latest stable Rust.
- **Frontend and tooling:** exclusively the latest Vite+ (`vite-plus`).
  - All JS/TS tooling goes through the **`vp` CLI** (dev, build, test, lint, format, tasks, environment).
  - The package manager is **pnpm**. Never use `npm`, `npx`, `yarn` or `bun`. The only JavaScript lockfile that may exist is `pnpm-lock.yaml` (`Cargo.lock` is committed as usual); `package-lock.json`, `npm-shrinkwrap.json`, `yarn.lock`, `bun.lock` and `bun.lockb` must never be committed (CI fails on them). "No npm" means the CLI; pnpm still installs from the public npm registry.
  - Research files that describe Vite+ as supporting several package managers are describing the tool, not obop policy. obop uses pnpm only.
- Pending owner ruling: whether two narrow cases are allowed as named exceptions to "vp only": the nightly exit drill, which runs upstream Vite tools without `vp` (I-4), and the JavaScript SBOM and licence scan (`pnpm sbom`, `pnpm licenses list`, run through `vp env exec` or a `vp run` task, because `vp pm sbom` and `vp pm licenses` do not exist in vite-plus 1.0.0; I-2). See `docs/DECISIONS.md` (I-2, I-4, Q10). Until ruled, do not add other exceptions.
- Pin policy (D23, I-3, PROPOSED, Q10): "latest" is provisionally the newest release that passes the pin policy, so the toolchain intentionally runs up to about three weeks behind the newest release until the owner ratifies it. See `docs/DECISIONS.md` section 1.1.

## Layout

- `README.md` — entry point and document map.
- `docs/PRODUCT.md` — master product description. `docs/DECISIONS.md` — binding decision register (wins over the research where they differ).
- `docs/CONVENTIONS.md` — naming and contract authority (identifiers, events, permission groups, data classes, ports); it ranks below `docs/DECISIONS.md` and above every other document.
- `docs/ARCHITECTURE.md`, `docs/ROADMAP.md`, `docs/BUSINESS.md`, `docs/LOCALIZATION.md`, `docs/ENGINEERING.md`, `docs/GLOSSARY.md`.
- `docs/spec/` — per-domain product specifications.
- `docs/research/` — the fact-checked research corpus (index: `docs/research/README.md`).
- Status: early code. A Cargo workspace (`crates/kernel/*`, `crates/modules/*`, `crates/obop`) and a Vite+ backoffice (`web/apps/backoffice`) exist. Storage is SQLite locally and PostgreSQL for deployments (D44), via `obop-storage`; records are JSON documents until the schema manager exists.
- Code map: `obop-registry` (model/field/command metadata and the D41 install gate), `obop-commands` (the one `Dispatcher` behind REST, MCP and CLI: permissions, masking, approvals, generated CRUD), `obop-masking` (D40), `obop-http` (axum), `obop-mcp` (stdio), modules `base`, `product`, `crm`, `sales`, `invoicing`. A module declares models in the registry and gets all surfaces generated; every field needs a data class, permission group and description (`obop modules check`).
- Modules and platform: `obop-pkg` (obop.toml X1 packages: parse, gate, runtime install/uninstall, marketplace; admin commands `module.*` and `marketplace_listing.*`), `obop-platform` (control plane: accounts, instances, provisioners local/sliplane/fake). `packages/*.toml` is the bundled catalog. See `docs/DEPLOY.md`; Sliplane project `obop` (`project_2x9d8q51m9lt`, account `spacecorps`) exists and is empty.
- UI: `web/apps/backoffice` renders from `/api/meta` (D46): app launcher, control panel, list/kanban, record side panel, Customize app (theme, apps, modules, module builder). `web/apps/platform` is the sign-up app, `web/apps/landing` the landing page; `deploy/build-site.sh` assembles them.
- Tests: `cargo test` (Rust, add `OBOP_TEST_POSTGRES_URL` for PostgreSQL), and `web/packages/e2e` (Playwright over the real built app, 50 tests; see its README; CI runs it). 12 bundled marketplace packages live in `packages/*.toml` (typed fields, D47).
- Auth: `obop-auth` (Argon2id passwords, SHA-256-hashed bearer tokens, 12 h sessions, login lockout, scoped agent tokens that can only narrow the owner's groups). First user: `echo "<12+ chars>" | cargo run -- admin create NAME --all-groups`; `cargo run -- serve --static web/apps/backoffice/dist` after `cd web/apps/backoffice && vp build`; agents: `cargo run -- agent-token NAME --scopes base.user`. `--dev-auth` swaps in fixed dev tokens (`dev-admin`, `dev-sales`, `dev-agent`) and refuses non-loopback binds. Tests: `cargo test` (set `OBOP_TEST_POSTGRES_URL` to include PostgreSQL).
