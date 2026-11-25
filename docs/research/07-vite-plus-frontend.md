# 07 - Vite+ (vite-plus) and the frontend architecture for obop

> **Owner constraint (2026-10-01): vp CLI + pnpm only, no npm.** All obop JavaScript/TypeScript tooling is driven through the `vp` CLI (Vite+) and pnpm is the only package manager; `npm`, `npx`, `yarn`, `bun` and their lockfiles are never used. Where this file says Vite+ supports several package managers, it describes the tool, not obop policy. The public npm registry is still where pnpm installs packages from, so registry facts below (versions, publish times, licenses) stay as written.

Research date: 2026-10-01. Assignment: establish exactly what Vite+ is today, then derive a frontend stack for a metadata-driven, Odoo-class ERP under the "frontend and tooling exclusively Vite+" constraint.

## Scope & method

- Primary sources only where possible: the official docs at viteplus.dev (raw `.md` pages), the `voidzero-dev/vite-plus` repo (git history, releases, issues, ecosystem-ci) read through the GitHub API, VoidZero / Vite / Vitest / TypeScript blog posts, npm registry JSON (versions, publish times, licenses, peer deps, weekly downloads), the crates.io API, and vendor pricing pages.
- No web-search engine was usable in this session (search endpoints are bot-gated and I did not try to bypass that). Discovery was by direct primary URLs. Consequence: community sentiment (HN, Reddit, X) and independent benchmarks are NOT covered. See "Confidence & gaps".
- Some pages were read through a summarising fetch tool. Load-bearing numbers were re-checked against raw JSON or raw markdown where possible.
- Tags: **[V]** verified on a live source today (URL given); **[I]** my inference from verified facts; **[G]** guess or recalled knowledge, not verified today.

---

## 1. What Vite+ is (as of 2026-10-01)

### 1.1 Identity and status

| Item | Finding |
|---|---|
| What it is | A single toolchain and entry point for web development, driven by one CLI, `vp`. It manages the Node.js runtime and the package manager, and bundles Vite, Rolldown, Vitest, Oxlint, Oxfmt, tsdown and a task runner (Vite Task). The authors say it is "neither a framework, nor a package manager and definitely not a replacement for Vite" [V] https://voidzero.dev/posts/announcing-vite-plus-1-0 |
| Name | No rename. "Vite+" / `vite-plus` / `vp`. The npm name `vite-plus` previously held an unrelated 2022 ISC package (0.0.1, other maintainer); the project's `0.0.0-<sha>` builds start 2026-01-15 [V] https://registry.npmjs.org/vite-plus |
| Repo | https://github.com/voidzero-dev/vite-plus - created 2025-03-06, 5.9k stars, 270 forks, 1,601 commits, about 121 open / 584 closed issues, 61 open PRs; code is roughly half Rust (2.95 MB) and half TypeScript (2.89 MB) [V] GitHub API |
| Docs | https://viteplus.dev (installers at https://vite.plus); machine-readable index https://viteplus.dev/llms.txt [V] |
| Registry packages (npm registry, installed via pnpm) | `vite-plus` (project-local CLI, bins `vp` and `vpr`), `@voidzero-dev/vite-plus-core` (the Vite/Rolldown/tsdown build that `vite` is aliased to), eight platform packages `@voidzero-dev/vite-plus-{darwin,linux,win32}-*` [V] npm registry |
| Latest | **1.0.0, published 2026-09-28** (npm 05:37Z; GitHub release 05:38Z; "promotes rc.1 to stable with no code changes"). rc.0 2026-09-22, rc.1 2026-09-26 [V] https://github.com/voidzero-dev/vite-plus/releases/tag/v1.0.0 |
| History | First npm release 0.1.0 on 2026-03-06 (0.1.0-alpha.0 a few hours earlier the same day); alpha announced 2026-03-13; beta 2026-07-02; 1.0 2026-09-28. 94 npm versions since 0.1.0-alpha.0 (40 non-prerelease) in about 7 months, roughly one release a week; the registry also holds 49 `0.0.0-<sha>` snapshot builds [V] npm `time` field, https://voidzero.dev/posts/announcing-vite-plus-alpha, https://voidzero.dev/posts/announcing-vite-plus-beta |
| Stability | "Stable" at 1.0, but the authors say Vite+ is "far from feature-complete" and tell production users to read the migration guide first [V] 1.0 post. Pre-1.0 plan issue states the 1.0 contract: semver applies to the `vp` CLI and the config schema; command names, flags, config keys and generated-file locations will not change in 1.x; bundled tools keep their own version numbers [V] https://github.com/voidzero-dev/vite-plus/issues/2405 (plan text; I searched the 1.0 post and the viteplus.dev guide pages and did not find it restated) |
| Adoption | npm `vite-plus` 2.36M downloads in 2026-09-23..29 (vite: 218.6M); more than 2,600 public repos depend on it; named users Tiptap, Dify, vinext (Cloudflare), BlockNote, Inkline, npmx, hono [V] npm downloads API + 1.0 post. CI traffic inflates the download count [I] |
| Steward | VoidZero Inc. (Evan You). VoidZero announced on 2026-06-04 that it is joining Cloudflare (Evan You's post calls it "this acquisition"; all VoidZero staff join Cloudflare). Both posts say Vite, Vitest, Rolldown, Oxc and Vite+ stay open source and MIT with the VoidZero team still leading them. Neither post says whether the deal has formally closed (closing status unverified) [V] https://voidzero.dev/posts/voidzero-cloudflare, https://blog.cloudflare.com/voidzero-joins-cloudflare/ |

### 1.2 License, paid tiers and the license history

Current license: **MIT** ("Copyright (c) 2026-present, VoidZero Inc."); GitHub API reports `MIT`; every npm version from `0.1.0-alpha.0` onward declares MIT. viteplus.dev lists no paid tier [V].

The repo's license was not always MIT. This matters for a 10-year project:

| Date | Event (from `git log` on `LICENSE`) |
|---|---|
| 2025-03-06 | `init repo`: MIT, "Copyright 2024-present VoidZero Inc. & Contributors" |
| 2025-10-27 | `chore(LICENSE): BUSL-1.1 (#259)` - Business Source License 1.1, Additional Use Grant "TBA", Change License "TBA" |
| 2026-02-27 | `chore: Switch to MIT License. (#648)` |
| 2026-03-06 | First public npm release 0.1.0, already MIT |

Sources: https://api.github.com/repos/voidzero-dev/vite-plus/commits?path=LICENSE and raw LICENSE at commits a19e5a36 / f6c223d2 [V]. The alpha post says a paid license for companies was first considered and dropped because debating which features to gate "only creates friction" (https://voidzero.dev/posts/announcing-vite-plus-alpha, dated 2026-03-13) [V]. Evan You's Cloudflare post calls it a "mixed-licensing experiment" that did not feel right and says Vite, Vitest, Rolldown, Oxc and Vite+ stay MIT [V]. I could not establish whether the repo was public during the BUSL window. No published npm version declares BUSL [V]: the `0.0.0-<sha>` snapshot builds published 2026-01-15 to 2026-02-26 (the BUSL window) carry no `license` field at all, and snapshots from 2026-02-27 onward declare MIT. The BUSL text also had a Change Date of "four years from the date the Licensed Work is published" (Additional Use Grant and Change License were "TBA") [V] raw LICENSE at commit a19e5a36.

Business model: not Vite+ licensing. The alpha post says the sustainable business will come from Void (void.cloud); Cloudflare says it intends to open-source the Void platform over time [V] https://blog.cloudflare.com/voidzero-joins-cloudflare/. Vite+ "remote caching" is listed as planned and unreleased, which is the obvious place a paid service could appear [I, speculation].

### 1.3 What 1.0.0 bundles (exact pins)

| Tool | Bundled version | Latest upstream today | Upstream maturity |
|---|---|---|---|
| Vite | 8.3.1 | 8.3.1 | Vite 8 GA 2026-03-12 (Rolldown replaces esbuild+Rollup); 8.1 (2026-06-23) added experimental Bundled Dev Mode and `.wasm` ESM integration [V] https://vite.dev/blog/announcing-vite8, https://vite.dev/blog/announcing-vite8-1 |
| Rolldown | 1.2.11 | 1.2.12 | 1.0 GA 2026-05-07; `^1` API locked, output behaviour may still change [V] https://voidzero.dev/posts/announcing-rolldown-1-0 |
| Vitest | 5.0.1 | 5.0.3 | Vitest 5 announced 2026-09-03; Browser Mode left "experimental" in 4.0 (2025-10-22) [V] https://vitest.dev/blog/vitest-5, https://vitest.dev/blog/vitest-4 |
| Oxlint | 1.85.0 | 1.86.0 | 1.x; about 865-870 rules; JS plugins still **alpha**; `.vue/.svelte/.astro` linted only inside `<script>` blocks [V] https://oxc.rs/docs/guide/usage/linter.html |
| oxlint-tsgolint | 7.0.2003 | 7.0.2003 | Type-aware rules and type-checking on the native TypeScript port; TypeScript 7.0 GA 2026-07-08 [V] https://devblogs.microsoft.com/typescript/ |
| Oxfmt | **0.70.0** | 0.71.0 | Still 0.x; claims 100% of Prettier's JS/TS conformance tests, about 30x faster than Prettier; no stable label [V] https://oxc.rs/docs/guide/usage/formatter.html |
| tsdown | **0.23.0** | 0.23.0 | 0.x library bundler (`vp pack`) |
| Vite Task | (bundled, unversioned in notes) | - | Task runner / cache; separate MIT repo voidzero-dev/vite-task |

Bundled versions lag upstream by days to weeks (Vitest 5.0.1 vs 5.0.3) [V]; because Vite+ pins exactly, you receive upstream patches only when Vite+ ships [I].

### 1.4 CLI surface (`vp`)

Nine headline commands: `create`, `install`, `dev`, `check` (format + lint + type-check in one pass), `test`, `build`, `pack`, `run`, `env` [V 1.0 post]. Full set per https://viteplus.dev/guide/ and the README [V]:

- Start: `create`, `migrate`, `config`, `hooks`, `staged`, `install`, `env`
- Develop: `dev`, `check`, `lint`, `fmt`, `test`
- Execute: `run` (alias `vpr`), `exec`, `node`, `dlx` (`vpx`), `cache`
- Build: `build`, `pack`, `preview`
- Dependencies (wrap pnpm/npm/Yarn/Bun, chosen from `packageManager` and lockfiles; **obop uses pnpm only (owner constraint, 2026-10-01)**): `add`, `remove`, `update`, `dedupe`, `outdated`, `list`, `why`, `info`, `link`, `rebuild`, `pm`
- Maintain: `toolchain`, `upgrade`, `implode`
- Package-manager selection and pinning [V https://viteplus.dev/guide/install, https://viteplus.dev/guide/env, both read as raw markdown on 2026-10-01]: `vp` reads top-level `packageManager`, then `devEngines.packageManager`, then marker files (`pnpm-workspace.yaml`, `pnpm-lock.yaml`, `yarn.lock`, `package-lock.json`, `bun.lock`, and others), and falls back to **pnpm** when nothing matches; it downloads the matching manager and never rewrites `package.json`. `vp env pin node@24 pnpm@<x>` writes the project pin and `VP_PACKAGE_MANAGER` overrides it per command. Verified command names for obop's use: `vp install` (`--frozen-lockfile`, `--lockfile-only`, `--ignore-scripts`, `--filter`, `-w`), `vp add` (`-D`, `-O`, `--save-peer`), `vp remove`, `vp update`, `vp dev`, `vp build`, `vp test`, `vp check`, `vp lint`, `vp fmt`, `vp run` (`vpr`), `vp exec` (binary from `node_modules/.bin`), `vp dlx <pkg>` and `vpx` (one-off package binaries; `vpx` tries a local binary first), `vp env ...`, `vp pm <cmd>` (raw forwarding, e.g. `vp pm audit`; `vp pm stage` for staged publishing, pnpm >= 11.3).

Built-in commands cannot be overridden by `package.json` scripts; use `vp run <script>` for scripts [V] https://viteplus.dev/guide/run. Two CLIs exist: a global standalone binary (manages Node and package managers, no Node needed to install) and the project-local `vite-plus` package (toolchain version pinned in the lockfile) [V] https://viteplus.dev/guide/global-cli, https://viteplus.dev/guide/local-cli.

### 1.5 Configuration format

One `vite.config.ts` using `defineConfig` from `vite-plus`. Standard Vite keys (`server`, `build`, `preview`, `plugins`) plus `test` (Vitest), `lint` (Oxlint), `fmt` (Oxfmt), `check`, `pack` (tsdown), `run.tasks` (Vite Task), `staged`, `create`, and `defaultPackage` (lets a non-JS repo root delegate to `./frontend`) [V] https://viteplus.dev/config/. `lazyPlugins()` avoids evaluating heavy plugins when tools only read metadata. Docs advise against a separate `vitest.config.ts`. rc.1 broke task config: `env`, `untrackedEnv`, `input`, `output` moved under `cache` (`vp migrate` rewrites them) [V] rc.1 release notes.

### 1.6 Scaffolding, monorepo, task running

- `vp create`: built-ins `vite:monorepo`, `vite:application`, `vite:library`, `vite:generator` (Bingo-based); framework templates via shorthands (`vite`, `@tanstack/start`, `svelte`, `next-app`, `nuxt`, `react-router`, `vue`); org template packs via `@org/create` manifests [V] https://viteplus.dev/guide/create.
- Monorepo: one root `vite.config.ts`; per-package lint/format differences via `lint.overrides` / `fmt.overrides`; **nested per-package lint/format configs are deliberately not applied** by `vp check` (the docs say they are "holding off ... for now" and ask for use cases in discussion #2669; `vp lint`/`vp fmt` run from inside a package do read that package's own `lint`/`fmt` block, which the docs do not recommend). Root `vp dev/build` opens a package picker [V] https://viteplus.dev/guide/monorepo, https://viteplus.dev/guide/troubleshooting.
- Task caching: tasks in `vite.config.ts` are cached by default, `package.json` scripts are not (opt in with `--cache`). Cache is keyed by file-system access tracking (fspy); `vp build` also reports env vars cooperatively. Flags `-r`, `-t`, `--filter` (pnpm syntax), default concurrency 4. Remote cache is **not available**; reusing the cache in GitHub Actions is marked experimental [V] https://viteplus.dev/guide/run, https://viteplus.dev/guide/automatic-data-tracking, https://viteplus.dev/guide/github-actions-cache.
- CI: `voidzero-dev/setup-vp` for GitHub Actions, GitLab CI and Azure Pipelines; docs say pin an exact release or SHA, not the dead `v1` tag; Docker image `ghcr.io/voidzero-dev/vite-plus:<version>`; Homebrew formula [V] https://viteplus.dev/guide/ci, v1.0.0 release notes.
- Migration: `vp migrate` folds Vite/Vitest/ESLint/Prettier/tsup/tsdown/lint-staged settings into `vite.config.ts`; requires Vite 8+ and Vitest 4.1+ first; Husky/lefthook are left in place with a warning; "most projects need manual adjustments" [V] https://viteplus.dev/guide/migrate. I found no documented eject path back to plain Vite [V absence in llms.txt].

### 1.7 Framework support

Officially framework-agnostic: anything that is a Vite project (React, Vue, Svelte, Solid, "20+ more") and meta-frameworks that ship as Vite plugins; deploy targets Vercel, Netlify, Cloudflare, Render [V] https://viteplus.dev. Evidence it actually works, from the repo's ecosystem-CI matrix [V] https://github.com/voidzero-dev/vite-plus/blob/main/ecosystem-ci/repo.json: Dify (large React app), Skeleton (Svelte), vue-mini, vite-plugin-react, VitePress, a TanStack Start hello-world, vinext, a Vue+Vercel app, plus Tiptap, nuxt-devtools, reactive-resume, sqlocal and about 20 others (repo.json lists 33 entries, several of them minimal bug reproductions). Known breakage: Vite SSG (`vite-ssg`) crashes under Vite+ (issue #1315, open; listed in the 1.0 plan) [V]; a dev-server restart wedge under `@cloudflare/vite-plugin` (#2481) [V]; duplicate Vite copies made TanStack Start return 404s (#1391, mitigated by the `vite` alias) [V].

### 1.8 Performance numbers (all vendor claims, none independently replicated)

| Claim | Source |
|---|---|
| Oxlint 50-100x faster than ESLint; Oxfmt up to 30x faster than Prettier; builds "up to 40x" faster than webpack | https://viteplus.dev |
| Vite 8 (Rolldown) production builds 1.6x-7.7x faster than Vite 7 | https://voidzero.dev/posts/announcing-vite-plus-alpha |
| Rolldown 10-30x faster than Rollup, on par with esbuild; customer build-time cuts of 38-64% (Mercedes-Benz.io, Ramp, Beehiiv); Linear 46s to 6s | Vite 8 and Rolldown 1.0 posts |
| `vp check` about 2x faster than running type-aware lint and type-check separately | https://viteplus.dev/guide/why |
| Vite 8.1 Bundled Dev Mode: about 15x faster startup, 10x faster reload on a 10,000-component test (experimental) | https://vite.dev/blog/announcing-vite8-1 |

### 1.9 Roadmap signals

Shipped since beta: GitLab/Azure CI, Homebrew, Docker, `vp toolchain`, `vp hooks`, `vp staged`, tsup migration. **Planned, not shipped:** remote caching, deeper monorepo diagnostics, `vp release`, `vp docs` (the 1.0 post lists it as planned; the Q3 plan issue strikes `vp doc` as "on hold"), Biome migration (plan issue only) [V] 1.0 post (first four) and issue #2405. **Explicitly not planned for Q3 2026:** an MCP server, `vp deploy` (waits on a Vite Deploy RFC), IntelliJ extension pack, monorepo dependency-graph visualisation, deeper Nuxt/Astro integration, auto-accessor transpilation [V] issue #2405 only (the 1.0 post does not mention `vp deploy`, an MCP server or Q3; the plan dates from 2026-08-11 and Q3 ended 2026-09-30, so re-check for a Q4 plan). Cloudflare's stated direction: a `cf` CLI rebuilt on Vite, and provider-agnostic full-stack and agent primitives in Vite; $1M Vite ecosystem fund [V] Cloudflare post.

### 1.10 Platform and runtime requirements

Node `^22.18.0 || ^24.11.0 || >=26.0.0` (Node 20 dropped in rc.0; `vp staged` needs `^22.22.1 || ^24.11.0 || >=26` and Git >= 2.32). Prebuilt binaries: Tier 1 Linux x64/arm64 glibc, Windows x64, macOS x64/arm64; Tier 2 Windows arm64; Linux x64 musl experimental; Alpine needs `libstdc++`; FreeBSD unsupported; the Windows `vp-setup.exe` is unsigned; standalone installers verify SLSA provenance since rc.0 [V] https://viteplus.dev/guide/, rc.0 notes. Node 24 "Krypton" is LTS (end of life 2028-04-30) and Node 26 becomes LTS 2026-10-28 (end of life 2029-04-30) [V] https://github.com/nodejs/Release/blob/main/schedule.json.

---

## 2. Vite+ risk register for a long-lived project

| # | Risk | Evidence | Severity | Mitigation |
|---|---|---|---|---|
| R1 | **Immaturity.** Stable for 3 days; CLI contract only from 1.0; about 121 open issues incl. task-cache correctness bugs | 1.0.0 on 2026-09-28; #2636 cache-key failure exits 0 with "0/0 cache hit"; #2635 task with no declared `output` still cached and silently drops files; #2518 cache continuously invalidated [V] | High (first 6-12 months) | Pin exact; treat `vp run` caching as opt-in for correctness-critical tasks (codegen, release); keep plain commands (`cargo`, `tsc`) runnable without `vp` |
| R2 | **Single-vendor control, then acquisition.** License flipped MIT to BUSL-1.1 (2025-10-27) and back to MIT (2026-02-27), a BUSL window of about 4 months, which ended before the first tagged npm release (0.1.0-alpha.0, 2026-03-06); VoidZero announced it is joining Cloudflare (2026-06-04; closing unconfirmed) | git log of LICENSE; Cloudflare post [V] | Medium-High | MIT grants on released versions are irrevocable [I]; fork path exists; monitor license and NOTICE on every upgrade in CI |
| R3 | **Alias / indirection layer.** `vite` must be overridden to `@voidzero-dev/vite-plus-core`; `vitest` pinned exactly; test imports come from `vite-plus/test`, not `vitest` | https://viteplus.dev/guide/local-cli; #1391 (two Vite copies: 404s), #2401 (no `vitest` package for integrations), #2356 (bots bump alias and pin out of lockstep), #2806 (strict pnpm peer validation), #2854 (types import undeclared `playwright`), #2023 (alias strategy concern) [V] | High | Centralise imports; one Renovate group; CI check that exactly one Vite and one Vitest copy exist (`vp why`) |
| R4 | **Bundled tools are 0.x or alpha.** Oxfmt 0.70, tsdown 0.23, Oxlint JS plugins alpha; bumps "can flag or format code that passed before" | rc.0 notes; oxc.rs docs [V] | Medium | Format/lint drift absorbed by upgrade PRs; avoid JS lint plugins in core; use Oxfmt output only after a full `vp fmt` commit |
| R5 | **Type-check coverage depends on tsgo/tsgolint.** No `compilerOptions.baseUrl` support; Vue/Svelte template type-checking is not what tsgo does | https://viteplus.dev/guide/troubleshooting; Oxlint lints only `<script>` of SFCs [V]; template-check gap is [I] | Medium (framework-dependent) | Favour TSX (fully covered); ban `baseUrl`, use `paths` |
| R6 | **Monorepo config model.** No nested lint/format configs; root-only with `overrides` | troubleshooting + monorepo docs; discussion #2669 [V] | Low-Medium | Fine for a single-repo monorepo; module authors cannot ship their own lint rules per package |
| R7 | **Supply chain.** `install.sh` resolves CLI dependencies at install time, not from your lockfile (#2850, CI broke on a `std-env` 404); `curl | bash` culture; unsigned Windows installer; the JS dependency tree itself comes from the npm registry (via pnpm) | #2850, global-cli docs [V] | Medium | Prefer project-local `vite-plus` + `pnpm-lock.yaml`; pinned Docker image by digest in CI; pin `setup-vp` by SHA; pnpm supply-chain settings (section 3.13) |
| R8 | **Upstream security-patch latency.** Vite's support policy has three tiers: regular patches only for the current minor (8.3); important fixes plus security patches for the previous minor (8.2) and the previous major's last minor (7.3); security-only patches for the second-to-last minor (8.1) and second-to-last major's last minor (6.4). Vite+ pins an exact Vite | https://vite.dev/releases [V]; latency is [I] | Medium | Track Vite+ releases weekly; automate upgrade PRs |
| R9 | **Fast majors.** Vite 6 (2024-11), 7 (2025-06), 8 (2026-03); Vitest 5 changed defaults (`clearMocks`, awaited async assertions); Node floor already raised twice | https://vite.dev/blog [V] | Medium | Budget an upgrade sprint per quarter; `vp migrate` handles part of it |
| R10 | **Missing features we may need.** Remote cache, `vp release`, `vp deploy`, SSG (#1315), Nuxt/Astro deep integration | 1.0 post; #2405 [V] | Medium | Design CI without remote cache; storefront SSR/SSG evaluated separately (section 3.9) |
| R11 | **Platform gaps.** musl experimental, WSL2 file-tracking bug (#1643), macOS shim/inode bug (#2263), no StackBlitz (#2598), no FreeBSD (#2537) | docs + issues [V] | Low-Medium | Standardise on glibc Linux, Windows x64, macOS |
| R12 | **Maintainer concentration.** Top contributors fengmk2 (576 commits), Brooooooklyn (180), wan9chi (127) | GitHub contributors API [V]; employer status [G] | Medium | Allocate obop engineer time to upstream fixes; Rust-heavy codebase suits a Rust shop |
| R13 | **No eject documented.** `vp migrate` is one-way as documented | migrate docs, llms.txt index [V absence] | Medium | Keep an "exit drill" CI job that runs plain Vite 8, Vitest 5, Oxlint, Oxfmt against the same config keys (keys mirror upstream [I]) |
| R14 | **Marketing numbers.** Headline speedups are vendor-measured | section 1.8 | Low | Benchmark on obop's own tree before planning around them |

Net assessment [I]: the constraint is workable. The components under Vite+ (Vite 8, Rolldown 1.x, Vitest 5, Oxlint 1.x, TypeScript 7) are each stable and independently governed; the risky layer is the thin `vp`/alias/pinning layer, and that layer is MIT, Rust-and-TypeScript, and replaceable at moderate cost if isolated from day 1.

---

## 3. Frontend architecture under the Vite+ constraint

### 3.1 What the Odoo reference demands

- Odoo 20.0's web client (branch `20.0`, `release.py` says FINAL) ships these view directories in `addons/web/static/src/views`: `calendar`, `form`, `graph`, `kanban`, `list`, `pivot` (plus `card`, `fields`, `widgets`) [V] GitHub API. Gantt, map, cohort and Studio are not in that tree [V absence]; they are Enterprise features [G]. obop must still deliver Gantt-class views.
- Bundled third-party JS in `addons/web/static/lib`: Chart.js, FullCalendar, Luxon, Bootstrap, DOMPurify, Ace, pdf.js, Popper, Prism, signature_pad, zxing [V]. Odoo did not need a commercial grid; its list view is a bespoke component.
- Odoo's own UI framework, Owl, is LGPL-3.0, about 30 kB gzipped, now in 3.0 alpha (49th alpha on 2026-09-08) with signals; stable npm is 2.8.3 [V] https://github.com/odoo/owl. Its ecosystem is essentially Odoo itself; not a candidate [I].
- Key architectural fact [I]: a metadata-driven ERP cannot statically type per-business-model data on the client. Static codegen covers the meta-model (field descriptors, view architectures, domain AST, actions, errors) and framework endpoints; business records are dynamic, typed by the model registry at runtime.

### 3.2 UI framework evaluation

| Candidate | State today [V unless noted] | Vite 8 / Vite+ fit | ERP-relevant ecosystem | Verdict |
|---|---|---|---|---|
| **React 19.3.0** (2026-09-09) | 211.7M weekly downloads; React Compiler 1.0.0 | `@vitejs/plugin-react` 6.1.1 requires Vite 8, uses Oxc for Refresh; Compiler via `@rolldown/plugin-babel` (stable path) or the experimental native Rust port (`react({ compiler: true })` plus optional peer `oxc-transform-react`, which the plugin README and its package both label experimental; the Vite+ 1.0 post claims it is 10x faster than the Babel plugin, vendor claim) [V] https://registry.npmjs.org/@vitejs%2Fplugin-react; Oxlint has native React and jsx-a11y plugins; TSX fully covered by tsgolint | Deepest: TanStack Table/Virtual/Form/Router, AG Grid, Glide, dnd-kit, Tiptap, Lexical, FullCalendar, ECharts/visx, React Aria, Radix/Base UI, Univer, SVAR Gantt | **Recommended** |
| **Vue 3.5.43** (3.6.0-rc.10 on 2026-09-30, Vapor Mode) | 18.9M weekly; 3.6 still RC after 10 RCs, so Vapor not GA | Best political fit (Evan You), plugin-vue 6.0.9 supports Vite 8; but Oxlint checks only `<script>`, and template type-checking needs vue-tsc [I] | Good: PrimeVue (license file not reviewed), Vuetify 4, Element Plus, Quasar, Reka UI, TanStack Table/Query adapters, AG Grid Vue | Runner-up; revisit when Vapor is GA |
| **Svelte 5.57.1** / SvelteKit 2.70 | Stable; plugin-svelte 7.3.1 needs Vite 8 | Oxfmt formats Svelte; same `<script>`-only lint and template-check caveat | Thinner: Bits UI, SVAR Gantt (Svelte), dnd-kit Svelte adapter, TanStack Table adapter; no mainstream pro grid | Viable for storefront, risky for back-office breadth |
| **Solid 1.9.15** (2.0.0-rc.13 on `next`) | 2.0 not GA | vite-plugin-solid 2.11.14 (Vite up to 9) | Fine-grained reactivity, excellent for huge forms; small ecosystem (Kobalte, TanStack adapters, dnd-kit Solid) | Not for v1; watch 2.0 |
| **Preact 11.0.0** (2026-09-30) | Brand-new major | Works via compat layer [G] | React libraries work through compat but with edge cases [G] | COULD for storefront islands |
| **Lit 3.3.3** | Stable web components | Framework-neutral | Web Awesome / Shoelace / Vaadin Grid / Spectrum WC; weak for app-scale state and metadata-driven forms [I] | Only for embeddable widgets |

Performance note: the framework benchmark (krausest) could not be retrieved; it is widely reported that signal-based frameworks beat React on raw DOM update benchmarks [G]. For an ERP the dominant costs are list virtualization, schema size and network round trips, not framework diffing [I]. React Compiler removes most manual memoization [V existence; effect on real forms unmeasured].

**Recommendation: React 19 + TypeScript 7, TSX only.** Reasons, in order: (1) only React has every heavyweight component class we need under a permissive license; (2) TSX is the one syntax fully covered by Oxlint plus tsgolint, which is the `vp check` path; (3) hiring and longevity; (4) `@vitejs/plugin-react` 6 was rebuilt around Vite 8 / Oxc, so it is the most-exercised path through the Vite+ toolchain [I]. What would flip it: Vue 3.6 Vapor reaching GA with an ERP-grade grid and gantt story, or an owner preference for Vue; keep the metadata layer framework-neutral (3.11) so a flip costs the renderers only.

### 3.3 Component and library matrix

| Need | Recommended | Why / caveats | Alternatives and licensing traps |
|---|---|---|---|
| Data grid (list views, x2many editors) | **TanStack Table 9.2.4 + TanStack Virtual 3.14** (MIT; 25.6M weekly downloads for react-table; 9.0.0 published to npm 2026-08-04 after 80 betas, 9.2.4 is `latest` as of 2026-08-28) | Headless: cell renderers come from the field-widget registry; adapters exist for React, Vue, Solid, Svelte, Angular, Lit, Preact [V repo packages] | **AG Grid**: Community 36.2 is MIT, but row grouping, pivoting, aggregation, tree data, master/detail, Excel export, clipboard, range selection, fill handle, set filters, server-side row model are **Enterprise** (the pricing page lists every item except the server-side row model, which is flagged Enterprise only in the docs page metadata; basic server-side sort/filter/pagination is in Community): $999 per developer (perpetual licence, 1 year of updates), bundle with AG Charts Enterprise $1,498 [V] https://www.ag-grid.com/license-pricing/, https://www.ag-grid.com/javascript-data-grid/server-side-model/. Per-developer commercial licensing is a poor match for an open-source core [I]. **Handsontable 18.1.1**: free "Hobby" license is non-commercial only; commercial from $999 per developer (Standard plan; billing model not stated on the page) [V] https://handsontable.com/pricing - exclude |
| Huge canvas grid (data explorer, spreadsheet-like) | COULD: Glide Data Grid 6.0.3 (MIT, canvas, "millions of rows", React only) | Last stable release 2024-02-03, alpha 2025-10-08, last push 2026-01-21; the README says none of the primary developers are accessibility users, so implementation flaws are likely [V] | Keep out of the main list view (a11y) |
| Pivot | Server-side aggregation in Rust (SQL `GROUP BY`/`ROLLUP`), client renders with TanStack Table | Same model as Odoo `read_group` [I] | **Perspective 5.5** (Apache-2.0, OpenJS Foundation, WASM query engine, pivot + WebGL charts) for an "analytics explorer" COULD [V]; AG Grid pivot is Enterprise |
| Charts | **ECharts 6.1.0** (Apache-2.0) | Tree-shakeable; covers bar/line/pie/heatmap/treemap/sankey | Chart.js 4.5.1 (MIT, what Odoo bundles) as lighter option; uPlot 1.6.32 for dense time series; visx 4.0 (low-level); AG Charts Enterprise is paid |
| Calendar | **FullCalendar 7.1.0 core** (MIT) | What Odoo uses. Timeline and vertical-resource views are **Premium**, "starting at $480" for 1-10 developers, yearly renewal (you keep the last version released before a lapse) [V] https://fullcalendar.io/pricing | Build resource views on TanStack Virtual |
| Gantt | SPIKE: **SVAR React Gantt 2.7.3** (MIT; React and Svelte builds) | Young (260 stars) and unverified at ERP scale | DHTMLX Gantt 10.0.3 "Community edition" (MIT on npm; feature cut-off not reviewed); frappe-gantt 1.2.2 (basic); Bryntum / Syncfusion commercial [G] |
| Rich text (chatter, notes, HTML fields) | **Tiptap 3.31 core** (MIT, ProseMirror-based; a named Vite+ adopter) | Collaboration, comments, DOCX import/export, AI are paid cloud features starting $49/month (annual) [V] https://tiptap.dev/pricing - do not depend on them. ProseMirror's GitHub repos are **archived**; development moved to code.haverbeke.berlin [V] | Lexical 0.52 (MIT, pre-1.0); BlockNote 0.55 (MPL-2.0); self-host Yjs 13.6 + Hocuspocus 4.7 (MIT) for collaboration |
| Drag and drop (kanban, reorder) | **@dnd-kit/react 0.5.0** behind an internal facade | Pointer, mouse, touch and keyboard sensors with ARIA live regions; adapters for React/Vue/Svelte/Solid [V]; still 0.x, the legacy `@dnd-kit/core` 6.3.1 is unchanged since 2024-12-05 | Atlassian pragmatic-drag-and-drop 4.0.0 (Apache-2.0); @hello-pangea/dnd 18.0.1; provide a non-drag "Move to..." path regardless (WCAG 2.5.7) [G] |
| Headless UI primitives | **React Aria Components 1.21.1** (Apache-2.0) for complex, keyboard-heavy widgets (combobox, date fields, tables) | Strong i18n (locale-aware dates/numbers, RTL) and keyboard behaviour [G on depth] | Base UI 1.8.0 (MIT, MUI), Radix 1.6.7 (MIT; last repo push 2026-08-08), Ark UI 5.39 (multi-framework), shadcn CLI 4.21 (copy-in pattern) |
| Styling | Tailwind CSS 4.3.3 (`@tailwindcss/vite` supports Vite 8) | Use logical utilities for RTL | - |
| Spreadsheet | COULD: Univer 1.0.3 (Apache-2.0 core); pivot tables, charts, real-time collaboration and import/export are **Pro** (the repo README says "import/export" and does not name xlsx explicitly; Pro pricing is not published there) [V] https://github.com/dream-num/univer | o-spreadsheet (Odoo, LGPL-3.0) is another option [V] | Evaluate only when the accounting/reporting module needs it |
| Command palette, hotkeys | cmdk 1.1.1 + tinykeys 4.0.1 (both MIT) | Keyboard-first UX; chord maps as data | react-hotkeys-hook 5.3.3; @tanstack/hotkeys 0.10.1 (pre-1.0) |

Rule [I]: an automated license allow-list (MIT, Apache-2.0, BSD, ISC, MPL-2.0 file-level) blocks any package whose license is "SEE LICENSE" or commercial. Several candidates fail at the npm metadata level today (AG Grid Enterprise = "Commercial", Handsontable, FullCalendar premium, PrimeVue/PrimeReact "SEE LICENSE" [V npm], which I did not review).

### 3.4 Routing, data and state

- Server state: **TanStack Query 5.104** (MIT) with a normalised record cache keyed by (model, id, field set).
- Router: **TanStack Router 1.170** (type-safe search params; the URL encodes action, view type, domain, group-by, breadcrumbs - mirrors Odoo's action stack). `react-router` 8.4.0 is the fallback. TanStack Start is labelled **Release Candidate** in its own docs today, so do not use it for the back-office SPA [V] https://github.com/TanStack/router/blob/main/docs/start/framework/react/overview.md.
- UI state: **Zustand 5.0.15** for small cross-cutting stores (command palette, dialogs); form state local. Jotai 3.0.1 and Redux Toolkit 2.13 are unnecessary [I].
- Forms: a schema-driven renderer on **TanStack Form 1.33.5** (headless), validators via Standard Schema (Zod 4.6, Valibot 1.5, ArkType 2.2), with constraints compiled from server-sent field metadata rather than hand-written schemas [I]. React Hook Form 7.89 is the alternative.
- Offline / local-first: TanStack DB 0.11 and Zero 1.9 are pre-1.0 or new; PGlite 0.5.8, `@sqlite.org/sqlite-wasm` 3.53, Dexie 4.4.6 and RxDB 17.5 are available [V versions]. Recommend an outbox pattern (idempotency keys, replay) for POS only, not full local-first sync [I].

### 3.5 Internationalisation, RTL, time

- Messages: **Lingui 6.8** (ICU MessageFormat, `.po` catalogs - the same interchange format Odoo translators use; `@lingui/vite-plugin` compiles catalogs on the fly, needs `@rolldown/plugin-babel`) or FormatJS 12 / `@formatjs/vite-plugin`; i18next 26 as third option [V versions]. Unicode MessageFormat 2 is specified (UTS #35 Part 9, LDML 48.2, document dated 2026-03-03; CLDR tag `release-48-2` 2026-03-17) and implemented by `messageformat` 4.0 [V] https://www.unicode.org/reports/tr35/, https://github.com/unicode-org/cldr/releases; too young to be the catalog format today [I]. Open question: how the Rust side formats the same ICU strings (Fluent via `fluent-templates` 0.15 vs ICU4X `icu` 2.3; `icu_messageformat_parser` is stale at 0.0.2) [V versions].
- RTL: CSS logical properties and Tailwind 4 logical utilities, `dir` on `<html>`, React Aria for bidi-aware widgets, mirrored icons by rule; Oxlint cannot enforce "no physical margins", so add a stylelint-equivalent check via an Oxlint JS plugin (alpha) or a CI grep [I].
- Dates: `Temporal` ships in Chrome 144 and Firefox 139 but Safari only in preview (MDN compat data) [V]; use `temporal-polyfill` 1.0.5 (MIT). Money and decimals: see WASM below.

### 3.6 Testing

- Unit and component: `vp test` (Vitest 5.0.1). Browser Mode (stable since 4.0) with the Playwright provider and official `vitest-browser-react`; `toMatchScreenshot` for visual regression; Trace View in v5 [V] https://vitest.dev/guide/browser/. Imports come from `vite-plus/test`; `@vitest/browser-webdriverio` is community-only.
- E2E: Playwright 1.63 (`@playwright/test`) is not bundled by Vite+; run it as a `vp run e2e` task. Accessibility: `@axe-core/playwright` 4.13 (MPL-2.0) [V versions].
- Lint-time a11y: Oxlint's jsx-a11y plugin [V].
- Rust-side contract tests belong to the Rust workspace; the TS side consumes the generated OpenAPI only.

### 3.7 Rust-to-TypeScript contract sharing

| Layer | Pick | Status today [V] |
|---|---|---|
| REST surface | **utoipa 6.0.0** (OpenAPI 3.1, released 2026-09-22; 50M all-time downloads) + `utoipa-axum` 0.3; `aide` 0.15.1 is the alternative (0.16.0-alpha.4 exists) | axum 0.8.9 |
| TS client from OpenAPI | `openapi-typescript` 7.13 + `openapi-fetch` 0.17 (types only, no runtime); Hey API `openapi-ts` 0.99 (pre-1.0), Orval 8.39, Kubb 5.4 are generators with more features | - |
| Meta-model types (view arch, field descriptors, domain AST, actions, errors) | **ts-rs 12.0.1** (released 2026-01-31; 16.7M all-time downloads) or **specta** (crates.io's newest stable, 1.0.5, dates from 2023-07-17 and predates `specta-typescript`; the live line is **2.0.0-rc.25**, 2026-05-07, which `specta-typescript` 0.0.12 pins exactly as `=2.0.0-rc.25`) | ts-rs is the lower-risk pick: specta 2 is still a release candidate and its TypeScript exporter is pre-0.1. specta pairs with `tauri-specta` **2.0.0-rc.25** for typed IPC (the 1.0.2 stable is the old 2023 line) |
| JSON Schema | `schemars` 1.2.2 (503M all-time downloads) | Feed form validators and docs |

Design [I]: Rust is the single source of truth; CI regenerates `openapi.json` and the TS types, commits them, and fails on `git diff`. Generation runs as a `vp run` task with explicit `cache.input` globs (fspy cannot see across `cargo`'s target dir reliably, so do not trust automatic tracking for cargo tasks - unverified, spike it).

### 3.8 Rust compiled to WASM

- Toolchain: `wasm-bindgen` 0.2.129 (released 2026-09-25, repo active), `wasm-pack` 0.15.0 now under the `wasm-bindgen` GitHub org, `tsify` 0.5.8 for TS types, `serde-wasm-bindgen` 0.6.5 [V]. Vite side: Vite 8.1 added direct `.wasm` ESM imports [V]; `vite-plugin-wasm` 3.6.0 declares support for Vite 8 [V]; Vite 8 also lists Wasm SSR support [V].
- Best candidates for WASM [I]: (1) the **domain / modifier expression evaluator** (`invisible`, `readonly`, `required` conditions, computed defaults) shared bit-for-bit with the server; (2) **decimal money arithmetic and rounding** (`rust_decimal`-style) so POS totals, taxes and currency rounding match the server exactly, instead of JS floats; (3) import-file parsing/validation of large CSV/XLSX in a Worker; (4) barcode (GS1) and tax-ID validators. Not candidates: anything that mainly touches the DOM.
- Costs [I]: async init, bundle size, no direct DOM, SSR complications, debugging. Keep the boundary coarse (pass batches, not per-cell calls).

### 3.9 Desktop, mobile, offline shells

- **PWA baseline**: `vite-plugin-pwa` 1.3.0 (declares Vite 8) + Workbox 7.4.1 [V]. Service worker caches the app shell and metadata; data stays online-first except POS.
- **Tauri 2.12.1** (Apache-2.0 OR MIT, Rust core, Windows/macOS/Linux/iOS/Android, Vite guide, plugins for updater, SQL, store, deep-link, barcode scanner; minimal app under 600 KB) [V] https://tauri.app/start/. Best fit for POS hardware (printers, scanners, cash drawer) because plugins are Rust [I]. Mobile maturity not verified.
- **Capacitor 8.5.2** (MIT): mature plugin ecosystem for camera/barcode on phones; adopt only if Tauri mobile falls short [G].
- Electron is out of scope (JS-heavy, not Rust) [I].

### 3.10 SSR / SSG for storefront and website

- Vite's Environment API was not yet stable at the Vite 8 announcement ("We are working to make the Environment API stable"), the 8.1 post does not mention it, and the current guide still says some of its APIs are experimental, with stabilisation planned for a future major [V] https://vite.dev/blog/announcing-vite8, https://vite.dev/guide/api-environment; SSR frameworks sit on that moving part.
- Options under Vite 8 [V versions]: **Astro 7.3.5** (requires `vite ^8.0.13`; static-first with React islands), SvelteKit 2.70.3, Nuxt 4.5.2 (`@nuxt/vite-builder` needs Vite ^8.2), React Router 8.4.0 framework mode (Vite 7 or 8), TanStack Start RC (React/Solid), Vike 0.4.267, vinext 1.0.0 (Next.js API on Vite, Cloudflare), Nitro 3.0.260903-beta.
- Vite+ gaps here: `vite-ssg` broken (#1315); Nuxt/Astro integration explicitly deprioritised (#1506) [V].
- Recommendation [I]: the storefront/website is a **separate app** from the back-office SPA. Static-first (Astro 7 or SSG), with product data pulled from the Rust API at build time and revalidated by webhook-triggered rebuilds; dynamic islands (cart, checkout) are React components from the shared design system. This avoids a Node SSR process in production. If true per-request SSR is needed, that is an owner decision because it adds a Node runtime to a "Rust backend only" deployment.

### 3.11 Keeping the metadata layer framework-neutral

Keep these as plain TS packages with no React import [I]: `@obop/meta` (generated types + view/field/action descriptors), `@obop/rpc` (typed client, record cache protocol), `@obop/domain` (domain AST evaluation; WASM-backed), `@obop/i18n-core`. Only `@obop/ui-*` packages import React. Registries: view-type registry (list, form, kanban, calendar, pivot, graph, gantt, map, activity, search), field-widget registry keyed by `(fieldType, widgetName)`, action-handler registry, client-action registry, service registry. Views are described as **JSON ASTs**, not XML strings, so they are diffable and validated in Rust.

### 3.12 Frontend extension (module) model

Odoo rebuilds asset bundles on the server when modules install. obop cannot run Vite+/Node on customer servers [I]. Therefore: core modules' UIs are built together with code-splitting per module, and third-party modules ship **prebuilt ES-module chunks** compiled against a versioned "obop UI SDK", loaded at runtime via `import()` with shared dependencies (React, SDK) provided through an import map. Vite 8.1's experimental chunk import map and Rolldown's chunking are relevant but experimental [V]. This is the highest-impact open design question in the frontend.

### 3.13 Repository layout under Vite+ [I]

```
/ (repo root: Cargo workspace + pnpm workspace)
  Cargo.toml, crates/...            # Rust backend (not touched by vp)
  pnpm-workspace.yaml               # pnpm only: package globs, catalog, overrides (vite alias, vitest pin), supply-chain settings
  pnpm-lock.yaml, .node-version     # committed; package-lock.json / yarn.lock / bun.lock never exist (CI fails if they do) [I]
  web/                              # vp defaultPackage target
    vite.config.ts                  # single root config (lint/fmt overrides, run.tasks)
    apps/backoffice  apps/storefront  apps/pos-shell
    packages/meta rpc domain i18n-core ui-kit ui-views ui-widgets
    generated/openapi.json, generated/meta/*.ts   # committed, CI-diffed
```

Package manager: **pnpm only (owner constraint, 2026-10-01)**. Vite+ falls back to pnpm when nothing is detected [V https://viteplus.dev/guide/install]; obop pins it explicitly with `vp env pin node@24 pnpm@<x>` (pnpm 12.8.1 is the registry `latest` tag on 2026-10-01, 11.28.2 is `latest-11` [V https://registry.npmjs.org/pnpm]). Catalogs (`catalog:` in `pnpm-workspace.yaml`) and `overrides` are supported by pnpm [V https://pnpm.io/catalogs] and used by `vp migrate` for the `vite` alias and `vitest` pin [V https://viteplus.dev/guide/migrate-rules]. Tasks: `vp install --frozen-lockfile` (CI), `vp run gen` (cargo -> openapi -> TS types), `vp check`, `vp test`, `vp build`, `vp run e2e`.

pnpm supply-chain settings (all in `pnpm-workspace.yaml`) [V https://pnpm.io/supply-chain-security, https://pnpm.io/settings/dependency-resolution, https://pnpm.io/settings/build, read as raw docs on 2026-10-01]:

- `minimumReleaseAge` in minutes: default 1440 since pnpm 11 (0 before), applies to transitive dependencies too, and the built-in default is non-strict. Proposal [I]: set it explicitly to 10080 (7 days, matching the Renovate cooldown in file 27 B8), which also turns `minimumReleaseAgeStrict` on; security fixes go through `minimumReleaseAgeExclude` (`pnpm audit --fix` adds the patched version there).
- `trustPolicy: no-downgrade` (default `off`) fails an install when a package's trust evidence (trusted publisher, provenance) is weaker than an earlier release's; exceptions via `trustPolicyExclude`, old versions via `trustPolicyIgnoreAfter`.
- `blockExoticSubdeps` (default `true` in the current docs): transitive dependencies may not come from git or tarball URLs.
- Dependency build scripts: not run by default since pnpm 10; `strictDepBuilds` (default `true`) fails the install on unreviewed scripts; approve explicitly in the `allowBuilds` map (`onlyBuiltDependencies` was removed in pnpm 11); `vp pm approve-builds` is the forwarded `pnpm approve-builds` [I].
- Audit: `vp pm audit` (forwarded to `pnpm audit`, which queries the registry's bulk advisories endpoint since pnpm 11); `audit signatures` (pnpm >= 11.1) verifies registry signatures, run as `vp pm audit signatures` [I].

---

## Confidence & gaps

- **High confidence [V]:** Vite+ 1.0.0 identity, date, MIT license, bundled versions, CLI surface, config shape, requirements, license history, the Cloudflare announcement (but not whether the deal has closed), library versions and licenses (all read from npm/GitHub/crates.io raw JSON or raw docs today).
- **Medium:** the exact 1.x stability contract (taken from the pre-1.0 plan issue); star and issue counts (they move daily); Oxlint/Oxfmt/Vitest behaviours taken from summarised fetches.
- **Not researched or not retrievable:**
  - Community sentiment and third-party reviews of Vite+ (no search engine).
  - Independent benchmarks; all speedups are vendor claims. The krausest framework benchmark could not be retrieved, so no framework performance ranking is asserted.
  - Vue 3.6 Vapor GA date, Solid 2.0 GA date, Tauri mobile maturity, SVAR/DHTMLX Gantt quality at ERP scale, pragmatic-drag-and-drop accessibility, PrimeVue/PrimeReact license text, Cloudflare's governance details beyond its blog post, whether the repo was public during the BUSL window, whether `vp` has any undocumented eject.
  - Whether fspy-based caching works reliably around `cargo` (needs a spike).
- Volatile facts (versions, star counts, issue totals) are accurate to 2026-10-01 and will drift.

---

## Implications for obop

**MUST**

1. **Adopt Vite+ 1.0.x as the only JS toolchain, pinned.** Project-local `vite-plus` dependency + `pnpm-lock.yaml` + `.node-version`; `vite` alias and `vitest` pin updated in lockstep (one Renovate group); CI via `setup-vp` pinned to an exact release or SHA (never `v1`, never `@latest`) or the versioned Docker image pinned by digest, installing with `vp install --frozen-lockfile`. Node 24 LTS (or 26 after 2026-10-28). **pnpm is the only package manager (owner constraint, 2026-10-01)**: `npm`, `npx`, `yarn`, `bun` and their lockfiles are banned (CI fails on `package-lock.json`, `yarn.lock`, `bun.lock`/`bun.lockb`), and the pnpm supply-chain settings in section 3.13 are on from the first commit.
2. **Isolate Vite+-specific surface.** All `vite-plus` and `vite-plus/test` imports flow through one config factory and one test re-export module. Add a quarterly "exit drill" CI job running plain Vite 8 + Vitest 5 + Oxlint + Oxfmt on the same config, so leaving Vite+ (license change, abandonment, Cloudflare strategy shift) stays a bounded task (R2, R3, R13).
3. **React 19 + TypeScript 7, TSX only, React Compiler via `@rolldown/plugin-babel` (the native Rust port in `@vitejs/plugin-react` is still experimental).** Ban `baseUrl` (breaks tsgolint); forbid `.vue`/`.svelte` in core.
4. **No commercially licensed runtime dependency in core.** CI license allow-list; explicitly excluded: AG Grid Enterprise, Handsontable, FullCalendar Premium, Univer Pro, Tiptap Cloud/Pro extensions, Bryntum. Optional "enterprise adapters" may live outside core.
5. **Rust is the single source of truth for contracts.** OpenAPI 3.1 from utoipa, meta-model types via ts-rs (or specta), generated files committed and diff-gated in CI. Business-model record types stay dynamic.
6. **Decide the frontend extension model now:** prebuilt ESM module chunks + versioned UI SDK + import-map shared deps; no bundling on customer servers.
7. **Accessibility, keyboard and RTL are structural:** React Aria primitives for complex widgets, logical CSS only, Oxlint jsx-a11y + axe in Playwright, every drag interaction has a keyboard path.
8. **Define the view/domain/modifier languages as serialisable JSON ASTs** owned by Rust crates, from the first commit.

**SHOULD**

9. Data layer: TanStack Query + Router + Table + Virtual + Form, Zustand for UI state, URL as the view/action state. Keep TanStack Start out of the SPA until it leaves RC.
10. Grid: TanStack Table/Virtual with a field-widget registry; server-side pagination, grouping and pivot aggregation in Rust.
11. Charts: ECharts; calendar: FullCalendar 7 core; rich text: Tiptap core; DnD: `@dnd-kit/react` behind a facade pinned to an exact 0.x; Gantt: time-boxed spike of SVAR vs DHTMLX Community vs custom.
12. i18n: Lingui + ICU MessageFormat + `.po` catalogs; `temporal-polyfill`; resolve how Rust shares the same message catalogs before module translations are written.
13. Shared Rust-to-WASM crate for the expression evaluator and decimal/money arithmetic, loaded in a Worker via Vite 8.1 `.wasm` ESM imports (spike bundle size and init cost first).
14. Shells: PWA (vite-plugin-pwa) as baseline; Tauri 2 for POS and desktop; POS-only offline outbox.
15. Testing: Vitest Browser Mode (Playwright provider) for components incl. `toMatchScreenshot`; Playwright E2E as a `vp run` task.
16. Storefront/website as a separate static-first app (Astro 7 or plain SSG) fed by the Rust API; raise "Node SSR in production?" with the owner before committing.
17. Fund upstream: a named engineer triages Vite+ issues that hit obop (#2635/#2636 cache correctness, #1315 SSG, #2806/#2356-class alias and lockstep issues) and reviews every Vite+ release note.
18. Treat `vp run` caching as opt-in for correctness-critical tasks; for cargo-driven tasks declare explicit `cache.input` globs.

**COULD**

19. Perspective (WASM pivot/analytics explorer); Glide Data Grid for a spreadsheet-like explorer; Univer core or o-spreadsheet for a spreadsheet module.
20. Capacitor for phone-only features if Tauri mobile is insufficient.
21. TanStack DB / sqlite-wasm for broader offline support once past 1.0.
22. Revisit Vue 3.6 Vapor and Solid 2.0 at GA for the storefront islands.
23. Vite Task remote cache when (if) it ships, and only after reading its terms.

---

## Verification log

Adversarial re-check on 2026-10-01 by a second pass, using different primary routes where possible: npm registry JSON (`time`, `dist-tags`, `engines`, `peerDependencies`, `license` per version), crates.io API (including dependency lists), GitHub API via `gh` (releases, LICENSE history, issues, contributors, languages, repo metadata, Odoo 20.0 tree), raw viteplus.dev `.md` docs, raw VoidZero / Cloudflare / Vite / Vitest / TypeScript / Oxc pages, Node `schedule.json`, MDN browser-compat-data 8.1.3, vendor pricing pages, Unicode LDML page and CLDR tags.

**Confirmed as stated: about 200 atomic claims** (each version pin, date, price, licence name, issue number and state, repo statistic and status statement re-checked individually; the count is approximate). This includes all four Vite+ 1.0.0 facts in claim 1 (npm 2026-09-28 05:37Z, GitHub release 05:38Z promoting rc.1 "with no code changes", 0.1.0 on 2026-03-06), the whole LICENSE history (MIT 2025-03-06, BUSL-1.1 2025-10-27, MIT 2026-02-27, every npm version since 0.1.0-alpha.0 declares MIT), all seven bundled tool pins and the Node range, the exact `vite`/`vitest` override requirement and issue #1391, the vendor prices (AG Grid $999/$1,498, Handsontable $999 and non-commercial Hobby, FullCalendar $480, Tiptap $49/month), TypeScript 7.0 on 2026-07-08, Vitest 4 (2025-10-22) and 5 (2026-09-03) facts, Vite 8 / 8.1 / Rolldown 1.0 dates and claims, Vue/Solid/React/Preact/TanStack Start statuses, npm download statistics, all Vite+ repo statistics and issue states cited in the risk register, Node LTS dates, Temporal browser support, Odoo 20.0 view directories and bundled libs, and the npm/crates.io versions of every library named in sections 3.2 to 3.10 (checked against `dist-tags.latest`).

**Corrections**

| # | Was | Now | Source |
|---|---|---|---|
| 1 | Vite "supports only the current minor, plus the previous minor and previous major" | Three tiers: regular patches for current minor (8.3); important fixes + security for previous minor (8.2) and previous major's last minor (7.3); security-only for second-to-last minor (8.1) and major (6.4) | https://vite.dev/releases |
| 2 | "VoidZero was acquired by Cloudflare" / "VoidZero now inside Cloudflare" / "Cloudflare acquisition" verified | Announced 2026-06-04 that VoidZero "is joining" Cloudflare; neither post states that the deal has closed | https://voidzero.dev/posts/voidzero-cloudflare, https://blog.cloudflare.com/voidzero-joins-cloudflare/ |
| 3 | "License flipped MIT to BUSL to MIT within 5 months" | BUSL-1.1 window was 2025-10-27 to 2026-02-27 (about 4 months); the full MIT-BUSL-MIT cycle spans about 12 months | `gh api repos/voidzero-dev/vite-plus/commits?path=LICENSE` |
| 4 | "no published npm version carries BUSL" tagged [I] | Verified [V]: 0 of 150 npm versions declare BUSL; snapshot builds published 2026-01-15 to 2026-02-26 have no `license` field, those from 2026-02-27 declare MIT | https://registry.npmjs.org/vite-plus |
| 5 | Quote "not a framework, not a package manager, and definitely not a replacement for Vite" | Exact text: "neither a framework, nor a package manager and definitely not a replacement for Vite" | https://voidzero.dev/posts/announcing-vite-plus-1-0 |
| 6 | "Explicitly not planned for Q3 2026" (MCP server, `vp deploy`, ...) sourced to "1.0 post and issue #2405" | Only issue #2405 (dated 2026-08-11) says this; the 1.0 post lists only remote caching, deeper monorepo diagnostics, `vp release`, `vp docs` as planned and never mentions `vp deploy`, an MCP server or Q3. The plan issue marks `vp doc` (not `vp docs`) "on hold" | https://github.com/voidzero-dev/vite-plus/issues/2405 |
| 7 | specta "1.0.5" and tauri-specta "1.0.2" presented as the current versions | Those are crates.io's newest non-prerelease but date from 2023; the live line is specta / tauri-specta 2.0.0-rc.25 (2026-05-07) and `specta-typescript` 0.0.12 pins `specta =2.0.0-rc.25`. The recommendation to prefer ts-rs is strengthened | https://crates.io/api/v1/crates/specta, https://crates.io/api/v1/crates/specta-typescript/0.0.12/dependencies |
| 8 | MessageFormat 2 "LDML 48.2, 2026-04-21" | LDML/UTS #35 48.2 document is dated 2026-03-03; CLDR tag `release-48-2` is 2026-03-17 (2026-04-21 not found) | https://www.unicode.org/reports/tr35/, https://github.com/unicode-org/cldr/releases |
| 9 | TanStack Table "v9 reached the npm `latest` tag by 2026-08-28" | 9.0.0 published 2026-08-04 (after beta.80); 9.2.4 was `latest` on 2026-08-28 | https://registry.npmjs.org/@tanstack%2Freact-table |
| 10 | Glide: "authors state no a11y experts on the team" | README: "none of the primary developers are accessibility users" | https://github.com/glideapps/glide-data-grid |
| 11 | FullCalendar Premium "from $480" | "Starting at $480" for 1-10 developers, yearly renewal | https://fullcalendar.io/pricing |
| 12 | AG Grid server-side row model listed as Enterprise on the pricing page | Not on the pricing page; flagged Enterprise only in the docs page metadata; basic server-side ops are Community. Bundle is AG Grid + AG Charts Enterprise | https://www.ag-grid.com/license-pricing/ |
| 13 | Univer Pro gates "xlsx import/export" | README says "import/export" only; xlsx is not named | https://github.com/dream-num/univer |
| 14 | Nested lint/format configs "deliberately not applied" | Docs: "holding off ... for now", `vp check` uses the root block; `vp lint`/`vp fmt` run inside a package do read its block | https://viteplus.dev/guide/troubleshooting |
| 15 | Downloads "50M", "16.7M", "503M" for utoipa, ts-rs, schemars | Those are crates.io all-time totals, now labelled | crates.io API |
| 16 | React Compiler only "via `@rolldown/plugin-babel`" | Also an experimental native Rust port via `react({ compiler: true })` + `oxc-transform-react` (plugin-react 6.1.1); Babel path stays the stable choice | https://registry.npmjs.org/@vitejs%2Fplugin-react |
| 17 | Node 24 "LTS to 2028-04-30" | 2028-04-30 is end of life; Node 26 EOL 2029-04-30 | https://github.com/nodejs/Release/blob/main/schedule.json |
| 18 | Environment API "8.1 did not announce stabilisation" (no further source) | Added: current guide still marks some APIs experimental, stabilisation planned for a future major | https://vite.dev/guide/api-environment |
| 19 | "94 npm versions" | Clarified: 94 since 0.1.0-alpha.0; the registry holds 150 total incl. 49 `0.0.0-<sha>` snapshots | npm `time` |
| 20 | ecosystem-CI list (8 projects) | Clarified: 33 entries in `repo.json`, incl. Tiptap, nuxt-devtools, reactive-resume, sqlocal and several bug repros | https://github.com/voidzero-dev/vite-plus/blob/main/ecosystem-ci/repo.json |

**Unresolved / unverified**

- Whether the Cloudflare acquisition has formally closed (no source states it).
- Whether the vite-plus repo was public during the BUSL window.
- The 1.x stability contract (semver on CLI and config schema) exists only in the pre-1.0 plan issue #2405; not restated in the 1.0 post or guide pages I searched.
- Vite+ roadmap after Q3 2026: the Q3 plan issue is still open on 2026-10-01 and no Q4 plan was found.
- Univer Pro pricing and whether xlsx specifically is Pro; AG Grid server-side row model licence is inferred from docs metadata.
- Maintainer employer status (R12), community sentiment, independent benchmarks, krausest framework ranking: not checkable with the tools available; all speedups remain vendor claims.
- Vue 3.6 Vapor GA date, Solid 2.0 GA date, Tauri mobile maturity, SVAR/DHTMLX Gantt quality, PrimeVue licence text, Lit-ecosystem component claims (Web Awesome, Shoelace, Vaadin, Spectrum), Bryntum/Syncfusion status, Capacitor plugin maturity: left as the author tagged them ([G] or not reviewed).
- Whether fspy-based task caching is reliable around `cargo` (needs a spike).

- Owner-constraint pass (2026-10-01): added the "vp CLI + pnpm only, no npm" note under the title; appended "obop uses pnpm only" to the `vp` dependency-command line in 1.4 and added a verified package-manager selection and pinning bullet (detection order, pnpm fallback, `vp env pin`, exact `vp` command names; read from the raw viteplus.dev install and env guides); renamed the "npm packages" row to registry packages installed via pnpm; R7 now names the npm registry (via pnpm) as a supply-chain surface and points to pnpm's controls; the 3.13 layout gained `pnpm-workspace.yaml`, `pnpm-lock.yaml` and `.node-version` with a ban on other lockfiles, and lists the pnpm settings (`minimumReleaseAge`, `trustPolicy`, `blockExoticSubdeps`, `strictDepBuilds`/`allowBuilds`, `audit`) with defaults read from the pnpm docs sources on GitHub; MUST 1 restates pnpm-only and `vp install --frozen-lockfile`. No npm, npx, yarn or bun command was present in this file; registry facts (versions, publish dates, licenses, downloads) were left as written. Not verified: that `vp pm approve-builds` and `vp pm audit signatures` forward to the pnpm commands of the same name (the docs say `vp pm` forwards raw commands; tagged [I]).
