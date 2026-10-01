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
- Status: pre-code. Nothing is built yet.
