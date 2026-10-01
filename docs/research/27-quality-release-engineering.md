# 27 - Quality engineering, CI/CD, release cadence, LTS and upgrade policy

## Scope & method

Research date 2026-10-01. Sources read live: GitHub Actions billing/limits docs; Odoo testing and support docs plus its `odoo/runbot` repo; ERPNext workflows and wiki; Tryton docs and the `trytond` 8.0.10 sdist; GitLab, Kubernetes, PostgreSQL, Nextcloud, Mastodon, Ubuntu and Stripe docs; Vite+, Vitest, Playwright, nextest, cargo-mutants and Renovate docs; crates.io, npm, PyPI, `gh api` and the RustSec tree. No general web-search engine was available, so discovery was by direct primary URLs and blog/forum sentiment is absent. **I built and benchmarked nothing**: every obop timing, minute and dollar figure is a model with stated assumptions. Tags: **[V]** verified today at the URL; **[I]** inference or model; **[G]** recalled, unverified. Numbers like "file 18" refer to sibling research files.

## 1. Headline findings

1. **A public repo is nearly free to CI.** Standard GitHub-hosted runners are free for public repos (4 vCPU/16 GB/14 GB SSD there; 2 vCPU/8 GB private); larger runners are always billed [V] https://docs.github.com/en/billing/concepts/product-billing/github-actions, https://docs.github.com/en/actions/reference/runners/github-hosted-runners. The binding limits are the 10 GB cache per repo on every plan, the 14 GB SSD, and 20 (Free) / 40 (Pro) / 60 (Team) concurrent standard-runner jobs (Enterprise 500) [V] https://docs.github.com/en/actions/reference/limits (cache, jobs) and the runners page above (disk). Private-repo included minutes are 2,000 (Free), 3,000 (Pro, Team) and 50,000 (Enterprise Cloud) per month and cannot be spent on larger runners [V] https://docs.github.com/en/billing/concepts/product-billing/github-actions.
2. **If everything ran on GitHub larger runners (private repo), my model gives about USD 1,850/month** for 8 engineers; the same load is about USD 260-350 on spot EC2 (RunsOn), about USD 650 on Namespace, or EUR 552 flat on two Hetzner CCX43 (B2) [I over V prices].
3. **Odoo has no GitHub workflow files** (only issue/PR templates) [V]; its CI is runbot plus a merge bot and a forward-port bot [V] https://github.com/odoo/runbot. Its upgrade path is a commercial service and OpenUpgrade's `20.0` branch has one commit [V file 02].
4. **Best migration-test precedent: ERPNext `patch.yml`** restores a v14 dump, then migrates it through the v15 and v16 hotfix branches to the PR commit, in a 60-minute job [V] https://github.com/frappe/erpnext/blob/develop/.github/workflows/patch.yml. **Best structural-test precedent: Tryton `ModuleTestCase`**, 26 generic test methods every module inherits (counted in the 8.0.10 sdist), plus a template-database cache [V] https://docs.tryton.org/latest/server/ref/tests.html.
5. **A 5-year CRA support period outlives every input except PostgreSQL and Ubuntu LTS**: Rust has no LTS (the project patches only the newest stable [G]), Node 24 ends 2028-04-30, PG 18 ends 2030-11-14 [V]. Each LTS branch needs a pinned toolchain and a dependency-refresh rule.
6. **Recommended release model:** two series a year (April, October), every fourth an LTS (`N.0`) with 5-year support; destructive schema/API removals only at `N.0`; required upgrade stops at `N.0`; monthly patch train; hosted fleet in rings with per-tenant migration state (Part C).

---

# Part A - Test strategy

## A1. What comparable projects actually do

| Project | Verified practice | obop takes |
|---|---|---|
| **Odoo** | Tags `standard`/`at_install`/`post_install`/custom; selector `[-][tag][/module][:class][.method][[params]]`, filtered after each module install and at load end [V] https://github.com/odoo/odoo/blob/20.0/odoo/tools/config.py. Tours in headless Chrome, failure screenshots, `assertQueryCount`; docs warn `at_install` overuse blocks parallelisation on runbot [V] https://raw.githubusercontent.com/odoo/documentation/20.0/content/developer/reference/backend/testing.rst. Runbot: build/batch/bundle/trigger, 60 s batch wait, Docker, workers "max #cpu - 1"; repo also has `runbot_merge` and `forwardport` ("forward ports successful PRs") [V] | Module/tag selection, query-count asserts; fixes land on the oldest branch and are forward-ported |
| **ERPNext** | `setup` job (40 min) bakes site+DB into a tarball, 4 shards run `bench run-parallel-tests --total-builds/--build-number`; coverage only off-PR; nightly; Postgres variant and `backport.yml` exist [V] https://github.com/frappe/erpnext/blob/develop/.github/workflows/server-tests-mariadb.yml. `patch.yml` starts the bench without workers so jobs cannot race DDL [V] | Bake once, fan out; chained upgrade test |
| **Tryton** | Generic tests `test_view`, `test_depends`, `test_field_relation_target`, `test_model_access`, `test_workflow_transitions`, `test_wizards`, `test_buttons_states`, `test_xml_files` and 18 more (26 in all); `DB_CACHE=postgresql://` builds from a server-side template DB; 1,678 `def test_` in `trytond/tests` [V, counted in sdist] | Metadata validation as free inherited tests; template DBs |
| **GitLab** | Predictive selection (Crystalball-based, Jest `--findRelatedTests`) before approval, full after; Knapsack; one retry in a fresh process flags flakes; fast quarantine; merge trains (June 2024) run only `pre-merge-checks`; PG 17 in MRs, 16/18 nightly [V] https://docs.gitlab.com/development/pipelines/. 8 `flaky-test::` labels [V https://docs.gitlab.com/development/testing_guide/unhealthy_tests/]. Migrations tested on Database Lab thin clones [V https://docs.gitlab.com/development/database/database_migration_pipeline/] | Tiered pipelines, retry-once, quarantine, merged-result gating |
| **Kubernetes** | Prow, Tide, TestGrid, Triage (failure clustering), Boskos, kubetest2 [V] https://github.com/kubernetes/test-infra (TestGrid, Triage and the job configs live there; Prow, Tide, Boskos and kubetest2 code now lives in `kubernetes-sigs` repos); cherry-pick deadline ~2 days before each monthly patch [V] | Failure clustering; backport deadlines |
| **PostgreSQL** | Buildfarm status page lists 221 distinct animals on 2026-10-01 (my count of `nm=` links) across Linux distros, BSDs, Windows, AIX, Solaris and 7+ CPU architectures [V] https://buildfarm.postgresql.org/cgi-bin/show_status.pl; pre-merge CI on GitHub Actions per fork via `PG_CI_ENABLED` [V] https://raw.githubusercontent.com/postgres/postgres/master/src/tools/ci/README | Breadth axis = PG version x OS x arch |
| **Rust crater** | Two toolchains in Docker; check-only ~3-4 days, build/test 5-6 days; Linux x86_64 only, many crates skipped [V] https://rustc-dev-guide.rust-lang.org/tests/crater.html | "Crater-lite" for modules, RCs only (C9) |
| **Supabase** | pgTAP via `supabase test db`, `begin; ... rollback;`, RLS via `set local role` + JWT claim, negative cases mandatory [V] https://supabase.com/docs/guides/local-development/testing/overview; `postgres` repo CI: merge queue, dynamic Nix matrix, Blacksmith runners, aggregator gate jobs [V] | RLS tests first-class; single gate job |

Google's flaky-test statistics page did not load (comments only), so I cite none [gap].

## A2. The obop test layers

| Layer | Tooling (verified version) | Runs | Budget [I] |
|---|---|---|---|
| Unit | `cargo-nextest` 0.9.146 (2026-09-21), `insta` 1.48.0 [V]; affected crates via `-E 'rdeps(crate)'` [V https://nexte.st/docs/filtersets/reference/] | every push | PR p50 < 4 min |
| Property | `proptest` 1.11.0 [V]; regression seeds committed | PR 256 cases, nightly 20k+ | - |
| DB | `sqlx` 0.9.0 + own template harness (A3) | PR, PG service container | < 6 min, 4 shards |
| API contract | utoipa 6.0.0 [V]; `oasdiff` v1.32.1 (Apache-2.0) diff; `schemathesis` 4.28.0 (MIT) live fuzz [V gh api] | PR diff, nightly fuzz | diff < 1 min |
| Metadata/view | Tryton-style inherited suite over the model/view/ACL registry | PR | < 2 min |
| Migration | A4 | PR/MQ/nightly | - |
| Localization | KoSIT validator 1.6.3 (2026-08-20, Apache-2.0) + `xrechnung-testsuite` as oracle [V]; golden files per pack; payroll packs need regulator worked examples to be "stable" (file 13) | PR touched packs, nightly all | 5 min |
| Security | cross-tenant + IDOR matrix (file 18); `cargo-fuzz` 0.13.2 (nightly + sanitizers, Unix x86-64/aarch64 only [V https://github.com/rust-fuzz/cargo-fuzz]) | matrix nightly; fuzz 60 s PR/10 min nightly/1 h weekly | corpus artifact |
| UI | Vitest 5 Browser Mode, Playwright 1.63.0, `@axe-core/playwright` 4.13.0 (MPL-2.0) [V npm]; `toMatchScreenshot`, with baselines generated only in one pinned container because file names embed OS/browser and rendering differs by GPU/fonts [V https://vitest.dev/guide/browser/visual-regression-testing]; `--shard` + blob merge [V https://playwright.dev/docs/test-sharding] | PR smoke, nightly full | smoke < 8 min |
| Performance | file 19 s12 and file 22 s10 budgets; macro gates on dedicated boxes (shared runners are noisy [I]) | nightly | +/-10% alarm |
| Restore drill | pgBackRest restore to scratch, checksums, ledger hash-chain verify (file 18) | monthly | RTO logged |
| Failure injection | failpoints in job/outbox/webhook paths; `shuttle` 0.9.4, `loom` 0.7.2 [V versions] | nightly | - |
| AI evals | file 16; promptfoo is MIT but "now part of OpenAI" [V https://github.com/promptfoo/promptfoo] | PR replay; weekly live | live < USD 50 [G] |

**Property invariants [I].** Ledger: posted moves balance per currency, posted lines immutable, hash chain verifies, reconciled amounts never exceed residual. Stock: on-hand+reserved never negative, quantity conserved along move chains, valuation layers equal the inventory account. Pricing: resolution deterministic and idempotent. Tax: tax-included/excluded round trip within one minor unit; per-line and global rounding reconcile with EN 16931 arithmetic (the `en16931` crate carries 317 rules, file 09). Workflows via a state-machine proptest. **Security matrix:** generated from the registry (model x operation x role x relation), expected outcomes from ACL/record-rule metadata, non-leaking 404; about 18,000 cases for 300 models x 5 ops x 6 roles x 2 tenants, sampled on PR. **Fuzz targets:** UBL/CII XML, MT940/CAMT, MIME, CSV import, Factur-X embedded XML, the domain-expression parser.

## A3. DB-per-test: speed and cost

`#[sqlx::test]` makes a fresh database per test, applies `./migrations` or a `Migrator`, supports ordered SQL fixtures, drops on success, keeps on failure, shares one connection limit; it says nothing about template databases [V] https://docs.rs/sqlx/latest/sqlx/attr.test.html. sqlx 0.9.0 (changelog date 2026-05-06; published on crates.io 2026-05-21) adds per-crate `sqlx.toml` incl. relocating `_sqlx_migrations`; the repo moves to the `transact-rs` org and stops tracking `Cargo.lock`, so `cargo install --locked sqlx-cli` breaks [V https://github.com/launchbadge/sqlx/blob/main/CHANGELOG.md]. `CREATE DATABASE ... TEMPLATE` needs zero other sessions on the template; `WAL_LOG` (default) suits small templates; `FILE_COPY` forces checkpoints before and after; PG 18 `file_copy_method=clone` lets the kernel share blocks on some filesystems [V https://www.postgresql.org/docs/current/sql-createdatabase.html, https://www.postgresql.org/docs/current/runtime-config-resource.html].

Cost model [I]: with 600-900 tables per tenant (file 22), migrating per test plausibly costs 1.5-4 s [G]; for 4,000 DB tests that is 1.7-4.4 CPU-hours versus ~20 CPU-minutes at an assumed 0.3 s per template clone. **Benchmark migrate-per-test, `WAL_LOG` template and `FILE_COPY`+`clone` on the real runner before committing.** Policy [I]: (1) default `#[obop::test(tx)]`: one transaction, rolled back (pgTAP style); (2) `#[obop::test(db)]`: clone of a per-shard migrated template, for commits, jobs, outbox, `LISTEN/NOTIFY`, advisory locks, RLS role switches; (3) `testcontainers` 0.28.0 [V] only for tests needing their own server; nextest runs each test in its own process [G], so use one Postgres service per CI job; (4) test cluster with `fsync=off`, tmpfs data.

## A4. Migration tests

- **Fixtures are built, not hand-made:** each release pipeline runs the released binary over the seed generator and publishes `fixture-<series>-<profile>.dump.zst` (ERPNext keeps a `v14-baseline` dump as a release asset [V]). Profiles: tiny 50 partners, small 5k, medium 500k, large 10M move lines [I].
- **Sources:** N-1, N-2, previous LTS, oldest supported LTS, each to HEAD. PR: N-1 tiny; merge queue: N-1/N-2 medium; nightly: all plus fan-out.
- **Expand/contract compatibility:** run N-1's API suite against the N schema after the expand phase; run contract only in the "fleet fully on N" test. CI rejects a contract in the release of its expand; lint `lock_timeout`, concurrent indexes, `NOT VALID`+`VALIDATE` (lock facts in file 18 s9.3). GitLab's recipe (ignore in M, drop in M+1, never both in one release) is the model [V https://docs.gitlab.com/development/database/avoiding_downtime_in_migrations/].
- **Fan-out:** clone 1,000 tenant DBs from the N-1 template, `obop migrate --fleet` at concurrency 32; assert zero failures, p95 duration, and no `ACCESS EXCLUSIVE` held over 1 s (sampled from `pg_locks`). ~190k relations per 250-tenant cluster make this a fan-out problem [V file 22].
- **Data migrations are SQL or a sealed SDK with no domain-crate dependency**, so any chain runs at any later version.

## A5. Test data

Factories: an `obop-testkit` crate of seeded typed builders (`fake` 5.1.0 [V]). Seed/demo data: declarative datasets loaded through the production importer; Odoo ships `demo` data and, on 20.0, a `populate` addon of generators (`addons/populate/generators/`) plus declarative XML datasets (`addons/account/populate/populate_demo.xml`, `benchmarks.xml`); 18.0 and 19.0 only have `odoo/tools/populate.py` and no `addons/account/populate` [V, git trees 2026-10-01]. Anonymised fixtures: none in the public repo; partner data stays private, pseudonymised with format-preserving fakes that still pass IBAN/VAT checksums. Odoo's `obfuscate` is reported imperfect [S file 03].

## A6. Mutation, coverage, flakes, impact selection

- **cargo-mutants** 27.1.0 (2026-06-02): `--in-diff` tests only mutants overlapping the diff (ignores test-only diffs); `--shard k/n` needs identical args per shard; `--baseline=skip` after a green step [V https://mutants.rs/in-diff.html, https://mutants.rs/shards.html]. Scope: ~15 core crates (ledger, tax, pricing, stock, ACL, migration runner). Model: 3,000 mutants x 20 s = 17 runner-hours, ~USD 22 on GitHub 8-core (17 h x 60 x USD 0.022) [I]. Gate: `--in-diff` nightly, full weekly, score >= 80%.
- **Coverage:** `cargo-llvm-cov` 0.9.1 [V]. No global percentage; diff coverage >= 80% on core crates, informational elsewhere.
- **Flakes:** nextest `--retries 1`; a retry pass is reported `FLAKY` (JUnit `flakyFailure`), and `flaky-result = "fail"` (since 0.9.131) can fail it [V https://nexte.st/docs/features/retries/]. Two flakes in 7 days auto-quarantine the test, open an issue with the CODEOWNERS owner, 14-day fix-or-delete SLA, cap of 20 quarantined tests. Nightly runs with `flaky-result = "fail"` [I].
- **Impact selection:** `rdeps()` on PR; always the full suite in the merge queue and on release branches (GitLab: predictive before approval, full after [V]).

## A7. What 4-12 people can sustain

| Team | Keep | Defer |
|---|---|---|
| 4-5 | unit + proptest (ledger/tax), template harness, OpenAPI drift, metadata suite, N-1 chain test, Playwright smoke + axe, cargo-deny/audit, nightly PG matrix | mutation, full fuzz, visual regression, Odoo scenario, live AI evals |
| 8 | + fuzz on 3 parsers, nightly IDOR matrix, N-2 + fan-out, KoSIT goldens, weekly core mutants, perf box, monthly restore drill | crater-lite |
| 12 | + visual regression, failure-injection suite, monthly Odoo-vs-obop, ecosystem runs, live AI evals | - |

One "quality platform" engineer (0.5-1 FTE at 8 people) owns CI, the flake ledger and fixtures [I]. **Odoo-vs-obop scenario [I]:** one Rust driver runs the same scripted day against Odoo 20 (JSON-2; Odoo 19 second baseline) and obop: order to delivery to invoice to payment to reconciliation, 10 POS terminals, a 10k-row list, aged receivable, 100 concurrent users; report p50/p95/p99, throughput, RSS and PG CPU on fixed hardware (Hetzner CCX33, EUR 138.49/mo [V file 22]). k6 is AGPL-3.0 [V gh api], so a custom driver sidesteps the question.

---

# Part B - CI/CD

## B1. Recommended pipeline

```
push/PR ─► T0 FAST GATE (free 4-vCPU runners, <=10 min)
           fmt · clippy affected+rdeps · cargo-deny · vp check · OpenAPI drift · migration lint
        ─► T1 REQUIRED (p50 <=20 min)
           rust-test x4 (nextest archive, PG service, template DB) · web (`vp install --frozen-lockfile`, `vp test`: Vitest unit+browser)
           contract (OpenAPI diff, registry/view validation) · migrate-smoke N-1 · e2e-smoke (Playwright+axe)
        ─► label ci:full: fuzz changed targets · mutants --in-diff
merge_group ─► MERGE QUEUE (<=30 min, merged result)
           T1 · release-mode smoke · migrate N-1/N-2 medium · IDOR sampled · l10n goldens
main ─► edge build ─► hosted ring 0
nightly ─► PG 16/17/18 · release-mode · full Playwright+visual+axe · fuzz 10 min/target · IDOR full
           N-1/N-2 fan-out 1,000 tenants · KoSIT · perf (bare metal) · AI replay · mutants in-diff
           Vite+ exit drill · cargo-audit/vet · rustc beta canary
weekly ─► core mutants (16 shards) · 1 h fuzz · Odoo-vs-obop · ecosystem compat (top-100 modules)
monthly ─► restore drill · fleet upgrade rehearsal
tag release/* ─► x86_64+aarch64 musl builds · 2x reproducible compare · SBOM · cosign+attestation
           upgrade matrix from all supported sources · publish · compat.json · advisory feed
```

One aggregator "CI ok" job per stage so branch protection names a single check (Supabase pattern [V]).

## B2. Minute and dollar budgets

Prices [V] https://docs.github.com/en/billing/reference/actions-runner-pricing (post 2026-01-01 cut, "up to 39%" [V https://github.com/resources/insights/2026-pricing-changes-for-github-actions]): Linux x64 USD/min 2-core 0.006, 4-core 0.012, 8-core 0.022, 16-core 0.042, 32-core 0.082, arm64 32-core 0.050, rounded up per job. Cost per vCPU-minute is ~USD 0.0026-0.003 at every size, so bigger runners buy wall-clock, not savings. GitHub's self-hosted USD 0.002/min charge (announced for 2026-03-01) was postponed on 2025-12-15; billing docs today show self-hosted free [V]. Cache over 10 GB: USD 0.07/GB-month [V].

**Model [I]:** 8 engineers, 220 merged PRs/month x 3 runs, 240 merge-queue runs, warm caches.

| Item | Composition (vCPU x min) | Per run | Monthly |
|---|---|---|---|
| PR | lint 8x7, rust-test 4x(8x9), contract 4x6, web 4x8, migrate 4x6, e2e 4x8 = 456 vCPU-min | 1.28 | **846** |
| Merge queue | 1.5x PR | 1.92 | **462** |
| Nightly | PG matrix, release-mode, Playwright, fuzz, fan-out, mutants-diff... = 3,524 vCPU-min | 9.63 | **289** |
| Weekly | core mutants 16x(8x60), fuzz, restore drill | 27 | **116** |
| Release | 32-vCPU, 2x reproducible build, ~5 branch releases | 6.6-8.6 | **43** |
| Ecosystem | top-100 modules x 10 min, 8-core, weekly | 22 | **95** |
| **Total (GitHub larger runners, private repo)** | ~658k vCPU-min, ~93k billable min | | **~USD 1,850** |

| Option | Monthly | Notes |
|---|---|---|
| A. All GitHub larger runners | ~USD 1,850 | zero ops; 10 GB cache cap |
| B. **Public repo, free standard runners** | ~USD 0-100 [I] | wall-clock ~1.6-1.8x [I]; 20 (Free) / 60 (Team) jobs; disk pressure |
| C. Namespace (8 vCPU/16 GB USD 0.008/min prepaid) | ~USD 650 | [V https://namespace.so/pricing] |
| D. RunsOn, own AWS, spot (8 cpu USD 0.0027/min; license from EUR 300/yr) | ~USD 260-350 | [V https://runs-on.com/pricing/]; spot interruption; single-vendor [I] |
| E. Hetzner 2x CCX43 (16 dedicated vCPU, EUR 275.99 each) | EUR 552 flat, ~47% utilisation; 3 boxes EUR 828 | [V file 22; list prices excl. VAT and IPv4, in force since 2026-06-15, CCX43 was EUR 124.99 before: https://docs.hetzner.com/general/infrastructure-and-availability/price-adjustment/; the 16-vCPU figure is (unverified)]; you run autoscaling and patching |

**Recommendation [I]:** B while PR wall-clock stays under 20 min, then a hybrid: lint/web/contract stay free, Rust build/test on D or E, perf on a dedicated CCX33, mutants/fuzz on E; target USD 300-800/month. On public repos never run self-hosted runners on fork PRs; use ephemeral runners without secrets for `merge_group` and maintainer-labelled PRs [G]. Miri stores env vars in `target/`, so never cache `target/` from jobs holding secrets [V file 06].

## B3. Caching

- **sccache** 0.18.0 (crates.io 2026-09-16; GitHub release 2026-09-14): `RUSTC_WRAPPER`; S3/R2/GCS/Azure/GHA backends; cannot cache `bin`, `dylib`, `cdylib`, `proc-macro` or incremental crates; paths must match (`SCCACHE_BASEDIRS`) [V https://github.com/mozilla/sccache]. Rust 1.99 disables incremental when `CI` is set [V file 06]. Use an S3-compatible backend to escape the 10 GB cap [I].
- **Swatinem/rust-cache**: `~/.cargo` and dependency `target/` only, `CARGO_INCREMENTAL=0`, bound by the 10 GB cap [V https://github.com/Swatinem/rust-cache]. Hybrid [I]: rust-cache for registry, sccache for workspace rlibs.
- **nextest:** `cargo nextest archive` once, then `--partition hash:m/n` (stable membership) or `slice:m/n` (balanced, since 0.9.127) [V https://nexte.st/docs/ci-features/partitioning/].
- **Vite+ cache is local only (no remote cache is documented):** `node_modules/.vite/task-cache`, keyed on args, opted-in env (`cache.env`) and tracked inputs; only tasks run through `vp run` use it; restoring it in CI is an "Experimental" `actions/cache` recipe and entries are never evicted individually [V https://viteplus.dev/guide/cache, https://viteplus.dev/guide/github-actions-cache]. File 07 R1 lists open cache-correctness bugs, so use `--no-cache` for codegen/release. Pin `voidzero-dev/setup-vp` by exact version or SHA; `v1` is no longer updated [V https://viteplus.dev/guide/ci]. `setup-vp` with `cache: true` caches only package-manager (pnpm) data, separately from Vite Task results [V same page]; CI installs with `vp install --frozen-lockfile`.

## B4. Compile-time budgets (file 06 s4)

Targets, not measurements [I]: cold `cargo check --workspace` (8 vCPU) <= 6 min; cold dev build <= 15 min; warm PR build (sccache hit >= 85%) <= 5 min, fail at 8; release LTO `obop` binary <= 40 min on 16 vCPU; leaf-module edit check <= 10 s, test link <= 30 s; CPU-time regression warn +10%, fail +25% vs main; any single crate over 60 s self-time triggers a split review. `cargo build --timings` only writes HTML, and `--timings=json` is noted as removed from unstable in 1.94 nightly [V https://doc.rust-lang.org/cargo/reference/timings.html, https://doc.rust-lang.org/cargo/reference/unstable.html], so gate on wall-clock on a fixed runner and archive the HTML. Precedent: rustc-perf benchmarks every bors commit [V https://github.com/rust-lang/rustc-perf].

## B5. Merge queue

Available for public repos owned by an organization and for private repos on Enterprise Cloud; workflows need the `merge_group` trigger; settings: build concurrency 1-100, group size 1-100, wait time, check timeout, "only merge non-failing pull requests" [V https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/configuring-pull-request-merges/managing-a-merge-queue]. Start at group size 1-3, concurrency 2 [I]; the queue carries the heavier tier so T1 stays fast.

## B6. Reproducible builds, SBOM, signing

- **Reproducible:** pinned `rust-toolchain.toml`, `--locked`, `--remap-path-prefix`, `SOURCE_DATE_EPOCH` from the tag commit [V https://reproducible-builds.org/docs/source-date-epoch/], builder image pinned by digest, two builds compared by hash per release; Cargo `trim-paths` is still unstable [V file 18]. Blocker for LTS, warning for regular series [I].
- **SBOM:** `cargo-cyclonedx` 0.5.9 emits only spec 1.3-1.5 (CycloneDX is at 1.7); pnpm lockfiles are two-document YAML that some scanners misread [V file 18]. Emit 1.5 for Rust, `pnpm sbom --sbom-format cyclonedx --sbom-spec-version 1.5` for the JS/TS packages (pnpm >= 11.1, also 1.6 and 1.7; run as `vp pm sbom` [I]; the npm-CLI-based `@cyclonedx/cyclonedx-npm` is not applicable to obop, pnpm-only) [V https://pnpm.io/cli/sbom], merge, and embed `cargo-auditable` 0.7.6 data. CRA Annex I only requires top-level dependencies [V file 17].
- **Attestations:** GitHub artifact attestations give SLSA v1.0 Build L2 (L3 via reusable workflow); public repos use Sigstore public-good; useless unless verified with `gh attestation verify`; sign releases, not test builds [V https://docs.github.com/en/actions/concepts/security/artifact-attestations]. crates.io publishing via OIDC trusted publishing only [V file 18]. Packages obop publishes to the npm registry (for example the `@obop/*` SDK packages) go out with `pnpm publish` using the same OIDC trusted-publishing and provenance flow (pnpm >= 11 publishes natively; verified in the pnpm source, not on the pnpm.io publish page) and may use staged publishing, where CI uploads without 2FA and a maintainer approves with 2FA (`pnpm stage` or `vp pm stage`, pnpm >= 11.3) [V https://pnpm.io/cli/stage, https://viteplus.dev/guide/install; details in file 24 s6]. npm-CLI publishing controls (npm 2FA at `npm publish`, minimum npm CLI versions) are not applicable to obop (pnpm-only); OIDC plus staged approval replaces them.

## B7. CRA Art. 14 runbook wiring

Reporting applies since 2026-09-11 (24 h early warning, 72 h notification, final report 14 days after a fix for exploited vulnerabilities); ENISA's platform went live the same day; full obligations 2027-12-11; support period >= 5 years; updates available 10 years; security fixes free of charge to business users outside tailor-made products [V files 17, 18; reporting dates and the ENISA platform: https://digital-strategy.ec.europa.eu/en/policies/cra-reporting]. Wiring [I]: intake (GitHub private advisories, `security@`, nightly `cargo-audit`/`cargo-deny`, OSV) opens a ticket in a private repo whose Action stamps T0 = awareness and schedules reminders at T0+12 h, +24 h, +60 h, +72 h and fix+10 d. SLAs: exploited/critical 7 days, high 30 days, medium next patch train. Each fix ships an OSV/CSAF advisory and lands on every supported branch in one train. The ENISA submission stays manual with two trained people on rota; quarterly tabletop. Entity/steward split is a lawyer item (file 17 s12).

## B8. Dependency update policy

- **Renovate** 44.127.1 (2026-10-01): `minimumReleaseAge` waits per version and adds a `renovate/stability-days` pending status; the built-in presets `security:minimumReleaseAgeNpm` and `security:minimumReleaseAgeCrate` both use 3 days (the crate preset cites crates.io's 72-hour deletion window), and the best-practice docs use 14 days as the cooldown before automerging third-party updates; it needs registry timestamps and the crate datasource reads index `pubtime` or the crates.io API [V https://docs.renovatebot.com/key-concepts/minimum-release-age/, https://github.com/renovatebot/renovate/blob/main/lib/config/presets/internal/security.preset.ts, https://docs.renovatebot.com/upgrade-best-practices/, https://github.com/renovatebot/renovate/blob/main/lib/modules/datasource/crate/index.ts]. Cargo's `min-publish-age` is nightly-only [V https://doc.rust-lang.org/cargo/reference/unstable.html], so Renovate is the crate cooldown; also set pnpm's own `minimumReleaseAge` in `pnpm-workspace.yaml` (Renovate recommends both) [V]. pnpm >= 11 defaults to 1440 minutes and is non-strict unless the value is set explicitly, so set 10080 (7 days) to match the proposal below, together with `trustPolicy: no-downgrade` and `blockExoticSubdeps` [V https://pnpm.io/settings/dependency-resolution, https://pnpm.io/supply-chain-security; file 07 s3.13]. Renovate's docs state that security updates bypass `minimumReleaseAge` [V https://docs.renovatebot.com/key-concepts/minimum-release-age/], so review those PRs by hand; pnpm's own gate would still hold such a version unless it is listed in `minimumReleaseAgeExclude`, and `pnpm audit --fix` adds the patched version there [V https://pnpm.io/cli/audit]. Proposal [I]: crates 7 d, npm registry packages (via pnpm) 7 d, Actions pinned by SHA 3 d.
- **Groups:** `vite-plus` as one group with exact pins (file 07 R3); toolchain; tokio/axum/tower; wasmtime (pin an LTS major); sqlx/sea-query. No automerge for request-path crates.
- **Toolchain rule:** adopt `x.y.1`, or `x.y.0` + 14 days, since 1.96.1, 1.97.1 and 1.98.1 each fixed a miscompilation [V file 06]; nightly beta canary.
- **cargo-deny** 0.20.2 (advisories, bans, licenses, sources) blocks PRs [V https://embarkstudios.github.io/cargo-deny/checks/index.html]; **cargo-vet** 0.10.2 (relative/shared audits, shrinking exemptions [V https://mozilla.github.io/cargo-vet/]) for request-path crates only, ~1-2 person-days/month [I]; its last release is 2026-01-13, so watch for stagnation [V].
- **Load:** RustSec holds 126 (2023), 139 (2024), 172 (2025) and 317 (2026 so far) advisory files under `crates/` [V, counted from https://github.com/rustsec/advisory-db]; includes unmaintained/unsound notices.

## B9. Vite+ exit-drill job (file 07 R13)

No eject is documented: the `vp migrate` page describes no undo or rollback (so treat it as one-way; "one-way" itself is my inference [I]) and `vp implode` removes the Vite+-managed global `vp` installation and its user data (runtimes, global packages, config, shims) but leaves the `vite-plus` dependencies inside projects [V https://viteplus.dev/guide/migrate, https://viteplus.dev/guide/implode]. Vite+ 1.0.0 was published 2026-09-28 [V npm, https://github.com/voidzero-dev/vite-plus/releases]. Nightly job [I]: copy the tree; codemod `vite-plus` to `vite` (and `vitest/config`, which the documented rewrite maps to `vite-plus`), `vite-plus/test` to `vitest`, `vite-plus/test/browser-playwright` to `@vitest/browser-playwright`, `vite-plus/test/browser/context` to `@vitest/browser/context` (inverse of the documented rewrite [V]); install (with pnpm) upstream Vite 8.3.1, Vitest 5.0.1, Oxlint 1.85.0, Oxfmt 0.70.0, tsdown 0.23.0 (the Vite+ 1.0.0 pins, file 07; [V npm `vite-plus@1.0.0` and `@voidzero-dev/vite-plus-core@1.0.0` bundledVersions]; npm `latest` today is already vite 8.3.2, vitest 5.0.3, oxlint 1.86.0, oxfmt 0.71.0 [V], so the pins lag upstream); run lint, format check, tests, build with plain pnpm scripts (a throwaway rehearsal copy: the one job where upstream tools run without `vp`; pnpm is still the only package manager and shipped builds always go through `vp`). Metrics: files touched, pass/fail. ~USD 0.12/night. It proves code portability, not Vite Task config.

---

# Part C - Release, LTS and upgrade policy

## C1. Comparison

| Project | Cadence | Support | Upgrade rule | Source |
|---|---|---|---|---|
| **Odoo** | one major a year (Sept/Oct) + Online-only SaaS minors every 2-3 months | 3 years standard (20.0 to Sept 2029 planned), extended for a fee; Online forced forward ~2 years | "upgrade from any version to any major version still under standard support" (or one whose standard support ended less than six months ago) via upgrade.odoo.com; free only with Enterprise; Online databases on a major must upgrade every two years | [V] https://raw.githubusercontent.com/odoo/documentation/20.0/content/administration/standard_extended_support.rst, https://raw.githubusercontent.com/odoo/documentation/20.0/content/administration/upgrade.rst; `SECURITY.md` on 20.0 still lists 16-19 [V] |
| **ERPNext** | major every ~1.3-2.3 years (v14 2022-08-01, v15 2023-10-20, v16 2026-01-12 [V GitHub releases]) | two majors at a time, 2.5-3 years: v14 EOL 2026-01-31, v15 end-2027, v16 end-2029 (planned); fix highest version first, backport | CI chains v14-v15-v16-HEAD | [V] https://github.com/frappe/erpnext/wiki/Supported-Versions |
| **Tryton** | 6 months nominal; actual gaps 7.6 to 7.8 = 7.6 months, 7.8 to 8.0 = 4.1 months (2025-12-15 to 2026-04-20, 126 days) [V PyPI] | 1 year/series; x.0 LTS 5 years; 6.0 last patched 2026-04-16; 7.0.57 and 8.0.10 on 2026-09-16; patches every ~2-4 weeks | may skip, "cannot skip more than 2 series ending with .0"; per-series Before/After steps | [V] https://www.tryton.org/download, https://docs.tryton.org/latest/migration.html |
| **Nextcloud** | majors ~every 4 months (33 Feb, 34 Jun, 35 Sep 2026) | ~1 year each, monthly maintenance | "You cannot skip major releases"; no downgrade | [V] https://github.com/nextcloud/server/wiki/Maintenance-and-Release-Schedule, https://docs.nextcloud.com/server/latest/admin_manual/maintenance/upgrade.html |
| **GitLab** | monthly minor (3rd Thursday), patches twice a month, yearly major (20.0 on 2027-05-20) | bug fixes current only; security current + previous two | required stops x.2/x.5/x.8/x.11; wait for background migrations | [V] https://docs.gitlab.com/policy/maintenance/, https://docs.gitlab.com/update/upgrade_paths/ |
| **Mastodon** | 4.6.0 2026-06-17, 4.7.0 2026-08-20 | 4.7, 4.6, 4.5.x to 2027-02-20, 4.4.x to 2026-12-17 | skip patches not minors; pre-deploy migrations with `SKIP_POST_DEPLOYMENT_MIGRATIONS=true`, restart, post-deploy migrations; phased `reload` | [V] https://github.com/mastodon/mastodon/releases/tag/v4.7.0, https://github.com/mastodon/mastodon/blob/main/SECURITY.md |
| **PostgreSQL** | yearly major; minors >= quarterly (2nd Thursday Feb/May/Aug/Nov) | 5 years; 18 to 2030-11-14; 14 EOL 2026-11-12 | pg_upgrade / dump | [V] https://www.postgresql.org/support/versioning/, https://www.postgresql.org/developer/roadmap/ |
| **Ubuntu LTS** | LTS every 2 years (26.04); interim 9 months | 5 years standard (to May 2031), ESM 2036, Legacy 2041 (15 total) | release to release | [V] https://ubuntu.com/about/release-cycle |
| Kubernetes | 1.34 to 1.37 in ~4-month steps [I] | ~14 months (12 + 2), monthly patches | apiserver cannot skip minors | [V] https://kubernetes.io/releases/patch-releases/, https://kubernetes.io/releases/version-skew-policy/ |

Patterns: long-support projects define a required stop or a skip limit; small teams promising 6-month cadence drift by months (Tryton); only GitLab and Mastodon formalise two-phase migration for self-hosters; Odoo's skip-anything upgrade is a hosted service, not a code property.

## C2. Four version axes

| Axis | Scheme | Rule |
|---|---|---|
| Product series | pre-1.0 `0.N`; from 1.0 `N.M.P`, `N.0` = LTS, `N.1-N.3` regular | contract migrations/removals only at `N.0`; `N.1-N.3` are API-, module-API- and schema-compatible (expand-only) |
| Public API | URL major `/api/v1` + dated compatibility header, like Stripe's monthly non-breaking and twice-yearly breaking releases with account/webhook pinning [V https://docs.stripe.com/api/versioning] | GA endpoints never removed within an API major |
| Module ABI | declarative `manifest_version`; WIT world `obop:module@x.y.z` for sandboxed WASM; `obop-sdk` crate semver for native modules (no stable Rust ABI, file 06), gated by `cargo-semver-checks` 0.50.0 [V] | module declares `obop_requires = "N.M"` |
| Schema | per-module ordered ledger (`module`, `id`, `kind`, `checksum`, `applied_at`, `app_version`); sqlx 0.9 can relocate the table per crate [V] | append-only inside an LTS line; squash only at `N.0` into a baseline that is a required stop |

## C3. Cadence, branching, LTS, backports

- **Cadence:** two series a year, GA on a Tuesday in week 3 of April and October (Ubuntu rhythm, clear of PG's September GA); LTS every 4th series = every 2 years.
- **Branching:** trunk-based `main`; cut `release/N.M` 6 weeks before GA (RC1 T-4 wk, RC2 T-2 wk); LTS branches live for life. Fix on `main`, bot-backport (ERPNext `backport.yml` [V]; Kubernetes cherry-pick deadlines [V]). Odoo forward-ports from the oldest branch [V], which suits slow-moving stable lines; obop's `main` will move faster than any stable line for two years, so main-first is cheaper [I].
- **Support:**

| Track | Lifetime | Content |
|---|---|---|
| edge (`main`) | none | nightly builds |
| Regular `N.1-N.3` | 9 months (3 months after the next GA) | bugfix, security, compliance data |
| **LTS `N.0`** | **5 years**: years 0-2 full; years 2-5 security + legal-critical only | CRA Art. 13(8) (support period at least 5 years); security updates stay available 10 years (Art. 13(9)) and technical documentation is kept 10 years (Art. 13(13)), both "or the support period, whichever is longer"; treating SBOMs as part of that documentation is my reading [I] [V https://www.cyberresilienceact.eu/the-cyber-resilience-act/, file 17] |

- **Patch train:** first Tuesday monthly for branches in "full" phase; security-only LTS branches roll up quarterly plus out-of-band for CVE SLAs. ~USD 7 CI per branch release (B2).
- **Capacity [I].** Steady state is 2-3 LTS lines + 1-2 regular = 4-6 live branches; Tryton carries three live lines today [V]. Controls: (a) security-only phase takes CVSS >= 7, data-integrity and legal-critical fixes only; (b) per-branch pinned toolchain, dependency patch bumps only when an advisory applies; (c) locale/tax data ships as separate packs on their own cadence (the EC/CEN EN 16931 code-list and validation-artefact cycle takes effect 15 May / 15 Nov, moved to the next Monday on a weekend: next 2026-11-16 with artefacts published 2026-10-09, then 2027-05-17 [V https://ec.europa.eu/digital-building-blocks/sites/spaces/DIGITAL/pages/467108974/Registry+of+supporting+artefacts+to+implement+EN16931]; Peppol BIS Billing follows its own cycle: 3.0.21 published 2026-05-20, mandatory 2026-08-17 [V https://docs.peppol.eu/poacc/billing/3.0/release-notes/]); (d) weekly reduced CI on old branches. Estimated cost 0.3-0.5 FTE [G]. **Tripwire:** if LTS upkeep exceeds 20% of engineering time for two quarters, stretch LTS spacing to 2.5 years (Tryton) or cut regular support to 6 months.
- **Dependency lifetimes vs LTS 1.0 (GA 2028-04-18, support to 2033):** PG 15 ends 2027-11-11 and PG 18 2030-11-14, so 1.0 must support PG 17-20; Node 24 ends 2028-04-30, Node 26 LTS 2029-04-30 [V https://github.com/nodejs/Release/blob/main/schedule.json], so build the 1.0 frontend on Node 26; with assets embedded Node is build-time only (file 18). Rust has no LTS: rebuild an LTS with a newer compiler only when a compiler/std advisory lands [I].

## C4. Deprecation windows

Kubernetes: GA APIs are never removed within a major; beta persists 9 months or 3 minor releases after deprecation (whichever longer); alpha has no promise; deprecated calls return `Warning` and a metric [V https://kubernetes.io/docs/reference/using-api/deprecation-policy/]. Odoo: XML-RPC/JSON-RPC deprecated in 19, `common` and `object` services removed in 22 (fall 2028), the `db` service already in 20 [V https://raw.githubusercontent.com/odoo/documentation/20.0/content/developer/reference/external_api.rst, 19.0 docs, file 02]. obop rule [I]: a GA API, event, WIT world or manifest key is deprecated at series N.x, kept for >= two series **and** 12 months, removed only at the next `N.0`; use emits `Deprecation`/`Sunset` headers [G, RFC numbers unverified], an audit-log entry, and a `deprecations.json` entry; experimental features sit behind flags.

## C5. Skip-level and schema migration

Supported paths [I]: any `N.x` to later `N.y`; `N.x` to `(N+1).0`; `N.0` to `(N+2).0` (skip one LTS) with the `(N+1).0` stop recommended, not mandatory; further jumps stop at an intermediate `N.0`. Safe because contract phases occur only at `N.0`, data migrations are sealed from domain code (A4), and a batched background migration started in one series is finalised at the next required stop before its old path is deleted (GitLab's practice [V]). CI proves it with the ERPNext-style chain oldest LTS to HEAD nightly. Tryton's skip-with-manual-steps is the honest fallback [V]; obop targets zero manual steps and publishes any remaining ones as structured data.

## C6. Self-hoster upgrade UX

`obop upgrade` [I]: (1) `--dry-run` prints the path with required stops, migrations by kind with duration and lock estimates, disk headroom, PG/OS/browser compatibility, installed modules' "verified on" status, manual steps (ideally none) and a downtime estimate. (2) **Backup gate:** refuses without a verified backup newer than N hours; override is logged. (3) **Signature gate:** verifies the attestation and cosign signature. (4) `--rehearse` restores the latest backup to a scratch database and runs the chain locally, answering Odoo's upload-a-dump service [V file 03]. (5) **Two phases like Mastodon:** `--phase=pre` (expand, old app running), swap, `--phase=post` (backfills as jobs; contract only at `N.0`) [V]. (6) **Rollback:** before contract, run the previous binary; after contract, restore the gate's backup (Nextcloud also forbids downgrade [V]). (7) **Downtime budget:** patch 0 (phased reload or < 10 s drain); minor < 60 s; `N.0` with contract < 5 min p95 for a 20 GB tenant; longer backfills run online.

## C7. Hosted fleet rollout

File 22 proposes cells of ~250 tenants, one DB per tenant. Rings [I]: **0** own tenants (24 h bake), **1** one canary cell of opt-in tenants (48 h), **2** 25% of cells (72 h), **3** the rest. Control-plane row `tenant_release_state(tenant_id, cell, app_version, schema_epoch, phase idle|expanding|backfilling|contracting|paused|failed, last_error, pinned_until)`. Expand before deploy; contract only after 100% of the fleet has been on N+1 for the 7-day rollback window; bounded concurrency per cell primary; per-tenant `lock_timeout`, resumable; halt a ring above 2% tenant failures; failed tenants stay on N-1 (works by the expand rule). SLO: 95% of tenants migrated in < 5 min. Tenants may pin up to 60 days inside the same LTS line.

## C8. Compatibility matrix, changelog, telemetry

`compat.json` per release (also rendered as a page): tested PG versions, OS/arch, browsers, upgrade sources, API versions, deprecations with removal series, module ABI versions, toolchain fingerprint. Changelog: Markdown + JSON, entries typed `breaking|security|migration|deprecation|feature|fix`, security entries carry GHSA/CVE IDs, plus a per-series upgrade-notes file consumed by `--dry-run` (Tryton publishes per-series steps; Mastodon puts them in release notes [V]). **Telemetry:** usage telemetry opt-in, default off, with `obop telemetry preview` printing the payload (GitLab has "Preview payload" but Service Ping is opt-out on free self-managed [V https://docs.gitlab.com/administration/settings/usage_statistics/]). Keep the advisory/update check separate, on by default, switchable; it sends only the version (Mastodon checks every 30 min and lets you blank `UPDATE_CHECK_URL`, discouraged [V https://docs.joinmastodon.org/admin/upgrading/]). Never collect record content; bucket counts.

## C9. Third-party module compatibility and "Verified on vX" (04 #8)

Precedent: WordPress.org "Tested up to" is author-declared and major-only [V https://developer.wordpress.org/plugins/wordpress-org/how-your-readme-txt-works/]; crater is machine-run but takes days [V]. obop's badge should be machine-issued [I]: an `ecosystem-ci` repo pulls each registered module at its declared commit and on every RC and weekly against `main` runs (1) manifest and `obop_requires` check; (2) build (native `cargo check` against the series SDK; WASM load in the pinned wasmtime LTS; declarative validation); (3) the inherited structural suite; (4) install, upgrade from the previous series, uninstall; (5) capability-manifest audit; (6) the module's tests. A pass yields a sigstore-signed attestation "verified on 1.1.0-rc.1 at <commit>" rendered as a badge plus `verified.json`; it lapses when a new series ships until re-verified. "Verified" means compatible and sandbox-clean, not audited. Cost: top-100 modules ~USD 22 per run; 1,000 modules ~USD 220 per RC.

## C10. Two-year calendar (2026-10 to 2028-10)

Plans, not promises; obop milestones [I], external dates [V] unless marked. Phases follow file 20 (foundations to month 3, beta by month 12, GA months 12-20).

| Date | obop | External clock |
|---|---|---|
| 2026-10-06 | policy v0, CI baseline, nightly edge | Rust 1.99 (10-01); PG 19 targeted Oct (Beta 4 on 09-24); Node 26 LTS 10-28 |
| 2026-11-03 | toolchain to 1.99.x | **CEN EN 16931 artefact cycle effective 11-16** (artefacts 10-09; this is the EC/CEN registry cycle, not a Peppol BIS release): refresh KoSIT goldens by 11-09 [V file 09, EC registry]; 11-12 Rust 1.100, PG minors, **PG 14 EOL** |
| 2026-12-01 | **0.1 preview**; migration ledger starts, never squashed again | Rust 1.101 (12-24) |
| 2027-02-02 | 0.2; PG 19 joins matrix after 19.1-19.2 | DE issue mandate (> EUR 800k) and Veri*Factu corporates 01-01 [V file 09]; PG minors 02-11 |
| 2027-04-06 | **0.3 public alpha**; N-1 guarantee; `--dry-run` MVP | Node 22 EOL 04-30 |
| 2027-05-04 | 0.4; crater-lite on top-20 modules | CEN EN 16931 cycle 05-17; PG minors 05-13; GitLab 20.0 05-20 |
| 2027-07-06 | **0.5 public beta**; hosted private beta | Veri*Factu others 07-01 |
| 2027-09-07 | 0.6; canary-ring rehearsal | France SMEs 09-01; Odoo 21 and PG 20 expected Sept-Oct [I] |
| 2027-11-02 | 0.7 (RC quality); CRA technical file due 11-30 | PG 15 EOL and minors 11-11 |
| **2027-12-11** | internal CRA conformance sign-off | **CRA main obligations apply** [V file 17] |
| 2028-03-07 | cut `release/1.0`; RC1 03-21, RC2 04-04 | Rust 1.112 (03-30) likely 1.0 toolchain [I]; DE all-B2B mandate 01-01 |
| **2028-04-18** | **1.0 LTS GA** (support to 2033-04) | Node 24 EOL 04-30 |
| 2028-05-02 | first 1.0.1 patch train | PG minors 05-11 |
| 2028-09-05 | cut `release/1.1`; RCs 09-19, 10-03 | ViDA platform deemed-supplier rules 07-01 (Member States may postpone to 2030-01-01) [V file 09, https://taxation-customs.ec.europa.eu/taxation/vat/vat-digital-age-vida_en]; Odoo 22 (fall) removes XML-RPC/JSON-RPC [V file 02] |
| **2028-10-17** | **1.1 GA** (regular, 9-month support) | - |

Rust follows a six-week train from 1.99: 1.100 Nov 12 2026; 1.102 Feb 4 2027; 1.104 Apr 29; 1.106 Jul 22; 1.108 Oct 14; 1.110 Jan 6 2028; 1.112 Mar 30; 1.114 Jun 22; 1.116 Sep 14 [I, computed]. Patch train: first Tuesday of each month (2026-10-06, 11-03, 12-01, 2027-01-05 ... 2028-10-03). **CRA note [I, lawyer item]:** 1.0 GA falls after 2027-12-11, so it is clearly subject to the full CRA; shipping a "1.0" earlier to use the Art. 69(2) carve-out depends on a substantial-modification reading nobody should bet an LTS on.

---

## Confidence & gaps

- **High [V]:** GitHub prices, limits, free-for-public rule, merge-queue availability; Odoo tag grammar, support table and runbot modules; ERPNext workflows and support table; Tryton `ModuleTestCase`, `DB_CACHE`, PyPI dates and skip rule; GitLab, Kubernetes, PostgreSQL, Nextcloud, Mastodon and Ubuntu policies; tool versions.
- **Modelled [I], not measured:** every obop build time, test count, cache hit rate, CI minute and dollar total, template-clone timing, Hetzner utilisation, the 0.3-0.5 FTE LTS cost. Costs scale roughly linearly with PR volume; re-run with real `cargo build --timings` and nextest durations in month one.
- **Not retrievable:** Google flaky statistics; Blacksmith per-size prices; Tryton's full release-policy text (the 5-year LTS rests on a one-line summary plus PyPI dates); GitHub's self-hosted fee status after the 2025-12-15 postponement beyond today's billing pages; Nextcloud app-store compatibility docs (404); CCX43's vCPU count and CRA Art. 69(2)/Art. 71 wording (not re-fetched in the fact-check; EUR-Lex blocked automated access). Resolved by the fact-check: Hetzner CCX33/CCX43 prices (docs.hetzner.com price-adjustment page), Renovate's security-update bypass (its docs), and the runbot licence (the `runbot`, `runbot_merge` and `forwardport` manifests declare LGPL-3; the repo has no root LICENSE file and GitHub reports no licence).
- **Judgement calls:** series naming, the 9-month regular window, ring bake times and SLAs are proposals. The CRA steward split and Art. 69(2) timing need counsel.

## Implications for obop

**MUST**

1. **MUST** keep the public repo's PR tier on free standard runners, treat the 10 GB cache and 14 GB disk as the real limits, and put sccache on an S3-compatible backend from day one; budget USD 300-800/month at 8 people, not USD 1,850.
2. **MUST** build a template-database test harness (`tx` and `db` modes), benchmark the three clone strategies in sprint one, and run one Postgres per CI job.
3. **MUST** start the append-only migration ledger with the first schema, never squash inside a line, run an ERPNext-style N-1 chain test from 0.3 and N-2 plus the 1,000-tenant fan-out from 0.5.
4. **MUST** ship a Tryton-style inherited structural suite for every module (views, depends, ACL, workflows, manifests) so third parties get it free and badges can rely on it.
5. **MUST** write proptest invariants for ledger, tax, pricing and stock before those crates leave alpha, and generate the cross-tenant/IDOR matrix from the registry before any multi-tenant deployment.
6. **MUST** permit contract migrations and API removals only at `N.0`, with `expand|backfill|contract` tags enforced in CI (file 18).
7. **MUST** build LTS releases from pinned, archived builder images, double-build for reproducibility, and retain artefacts, SBOMs and attestations 10 years.
8. **MUST** wire the CRA Art. 14 clock (T0 stamp, 24 h/72 h/14 d reminders, rota) before any commercial offer, and never gate security fixes behind an enterprise edition.
9. **MUST** keep `vite-plus` in one Renovate group with exact pins, use `--no-cache` for codegen/release tasks, and run the nightly exit drill.
10. **MUST** pin the Rust toolchain per branch and adopt `x.y.1` (or `x.y.0` + 14 days).

**SHOULD**

11. **SHOULD** adopt two series a year, LTS every fourth with 5-year support, 9-month regular support, first-Tuesday patch train and the C3 tripwires.
12. **SHOULD** move Rust build/test to ephemeral self-hosted (RunsOn spot or 2 x Hetzner CCX43) once PR p50 exceeds 20 min; perf and mutants on dedicated hardware; never on fork PRs.
13. **SHOULD** publish `compat.json` and a structured changelog per release, and deliver `obop upgrade --dry-run`, backup gate, attestation check and `--rehearse` within 0.3-0.5.
14. **SHOULD** make usage telemetry opt-in with payload preview, and keep the advisory check separate, on by default and switchable.
15. **SHOULD** run cargo-mutants (`--in-diff` nightly, full weekly, 80% score) on ~15 core crates only.
16. **SHOULD** use Renovate cooldowns (7 d crates / npm registry via pnpm, 3 d Actions by SHA) plus pnpm's own (`minimumReleaseAge`, `trustPolicy: no-downgrade`, `blockExoticSubdeps`), cargo-deny as a blocker, cargo-vet on request-path crates.
17. **SHOULD** issue "Verified on vX" badges from a machine run with signed attestation on RCs and weekly, starting with 20 modules at 0.4.
18. **SHOULD** fix API stability Stripe/Kubernetes-style: GA never removed within a major, >= two series and 12 months deprecation, headers and audit entries on use.
19. **SHOULD** gate the first 1.0 GA on a signed CRA conformance review (2027-11-30) and build its frontend on Node 26.

**COULD**

20. **COULD** run AI evals on an in-house harness rather than depend on promptfoo (now part of OpenAI): recorded-response replay on PRs, weekly live runs under ~USD 50.
21. **COULD** add Database Lab-style thin-clone migration tests on production-size data for hosted cells.
22. **COULD** stretch LTS spacing to 2.5 years or shorten regular support to 6 months if the tripwire trips.
23. **COULD** run the Odoo-vs-obop scenario monthly from month 9 and publish the driver and results.
24. **COULD** sell extended support past year 5 (Ubuntu ESM/Legacy analogue) once a commercial entity exists.

## Verification log

Fact-check pass dated 2026-10-01, run against live primary sources (docs pages, `gh api`, crates.io/npm/PyPI registries, the Tryton sdist, EC and Hetzner pages). All 13 flagged load-bearing claims were confirmed in substance; the Odoo upgrade rule, Vite+ migrate/implode wording, Hetzner provenance and the GitHub disk-limit source needed fixes, and 20 corrections or clarifications in all were applied (list below). Arithmetic in the B2 model (PR 456 vCPU-min = USD 1.28/run, 660 x 1.28 = 846, total ~1,850; A3 CPU-hour figures; 18,000 security cases; EUR 552/828; ~47% utilisation; Namespace ~650) was recomputed and holds.

**Confirmed count:** about 190 individual facts (approximate tally): all GitHub prices, limits, plan minutes and runner specs; the 2025-12-15 self-hosted postponement; ERPNext `patch.yml` (60 min, v14 dump to v15, v16, PR commit, no-worker bench), `server-tests-mariadb.yml` (40 min setup, 4 shards), Postgres variant and `backport.yml`, wiki EOL table; Tryton 26 `ModuleTestCase` tests, 1,678 `def test_`, `DB_CACHE`, skip rule, PyPI dates (6.0.76 on 2026-04-16, 7.0.57/8.0.10 on 2026-09-16, 7.6 to 7.8 = 7.6 months); GitLab cadence, support, required stops, 20.0 on 2027-05-20, pipeline practices; Odoo support table, no workflow files, runbot modules, `SECURITY.md`, OpenUpgrade single commit, test-tag grammar; PostgreSQL EOL dates, minor schedule, PG 19 target; sqlx docs, 0.9.0 changelog and `transact-rs`; `file_copy_method`; Node 24/26 dates; Vite+ 1.0.0 pins; RunsOn and Namespace prices; RustSec counts 126/139/172/317; every crate and npm version and licence named in A2/A6/B3/B6/B8/C2 (nextest 0.9.146, insta 1.48.0, proptest 1.11.0, utoipa 6.0.0, cargo-fuzz 0.13.2, shuttle 0.9.4, loom 0.7.2, testcontainers 0.28.0, fake 5.1.0, cargo-mutants 27.1.0, cargo-llvm-cov 0.9.1, sccache 0.18.0, cargo-deny 0.20.2, cargo-vet 0.10.2, cargo-cyclonedx 0.5.9, cargo-auditable 0.7.6, cargo-semver-checks 0.50.0, Playwright 1.63.0, axe 4.13.0, Renovate 44.127.1, oasdiff 1.32.1, schemathesis 4.28.0, KoSIT 1.6.3; k6 AGPL-3.0, promptfoo MIT and "part of OpenAI"); Rust 1.99 on 2026-10-01, the six-week train dates and 1.96.1/1.97.1/1.98.1 miscompilation fixes; Cargo incremental-off-in-CI, `--timings=json` removal, unstable `min-publish-age` and `trim-paths`; Kubernetes, Nextcloud, Mastodon, Ubuntu, Stripe, Supabase policies and dates; CRA Art. 13(8), Art. 14 clocks, 2026-09-11 and 2027-12-11 dates, ENISA platform live 2026-09-11; EC/CEN artefact cycle dates; Node 22 EOL; calendar weekdays and first-Tuesday patch dates.

**Corrections (was / now / source):**

1. Buildfarm size: "~230 animals (my count)" / 221 distinct animals on the status page today / https://buildfarm.postgresql.org/cgi-bin/show_status.pl
2. Tryton 7.8 to 8.0 gap: "4.2 months" / 4.1 months (126 days) / https://pypi.org/pypi/trytond/json
3. Odoo upgrade rule: "any major version still under standard support" / also majors whose standard support ended less than six months ago; Online majors must upgrade every two years / https://raw.githubusercontent.com/odoo/documentation/20.0/content/administration/standard_extended_support.rst and upgrade.rst
4. ERPNext cadence: "major ~every 2 years" / 2022-08-01, 2023-10-20, 2026-01-12 (about 1.3 to 2.3 years) / https://github.com/frappe/erpnext/releases
5. Odoo populate: "`populate` factories (`addons/account/populate` on 20.0)" / 20.0 has an `addons/populate` generator addon and XML datasets in `addons/account/populate`; 18.0/19.0 have neither / https://github.com/odoo/odoo (git trees)
6. mutants.rs sharding link and unit: `/sharding.html` (404) and "17 CPU-hours" / `/shards.html` and 17 runner-hours (the USD 22 figure prices runner time) / https://mutants.rs/shards.html
7. Renovate cooldown: "docs suggest 3 days for npm (72 h unpublish window)" / presets `security:minimumReleaseAgeNpm` and `...Crate` use 3 days (72 h deletion window is in the crate preset text); 14 days is the best-practice automerge example / https://github.com/renovatebot/renovate/blob/main/lib/config/presets/internal/security.preset.ts, https://docs.renovatebot.com/upgrade-best-practices/
8. Renovate security bypass: "exemption config unverified [G]" / verified: security updates bypass `minimumReleaseAge` / https://docs.renovatebot.com/key-concepts/minimum-release-age/
9. Vite+ exit: "`vp migrate` is one-way; `vp implode` only removes the global CLI" / no documented undo (one-way is an inference); `vp implode` removes the global installation plus runtimes, global packages, config and shims, leaving project dependencies / https://viteplus.dev/guide/migrate, https://viteplus.dev/guide/implode
10. Vite+ cache: "fingerprinted env" / only env opted in through `cache.env`; "local only" now qualified as "no remote cache documented" / https://viteplus.dev/guide/cache
11. Cycle attribution: "Peppol/CEN cycle effective 11-16" and "Peppol cycle 05-17" / the EC/CEN EN 16931 registry cycle (15 May / 15 Nov, weekend moves to Monday); Peppol BIS 3.0.21 was published 2026-05-20, mandatory 2026-08-17 / https://ec.europa.eu/digital-building-blocks/sites/spaces/DIGITAL/pages/467108974/Registry+of+supporting+artefacts+to+implement+EN16931, https://docs.peppol.eu/poacc/billing/3.0/release-notes/
12. CRA retention: "artefacts/SBOMs kept 10 years (Art. 13(9))" / Art. 13(9) is security-update availability, Art. 13(13) is technical documentation, both 10 years or the support period if longer; SBOM inclusion is an inference / https://www.cyberresilienceact.eu/the-cyber-resilience-act/
13. GitHub disk limit: 14 GB cited to the limits page / the limits page has no disk figure; disk comes from the runners reference; added plan minutes and concurrency (Free 20, Pro 40, Team 60, Enterprise 500) / https://docs.github.com/en/actions/reference/runners/github-hosted-runners, https://docs.github.com/en/actions/reference/limits
14. Hetzner: "[V file 22]" with prices listed as not retrievable / CCX33 EUR 138.49 and CCX43 EUR 275.99 (DE/FI, excl. VAT and IPv4, from 2026-06-15; CCX43 was EUR 124.99) / https://docs.hetzner.com/general/infrastructure-and-availability/price-adjustment/
15. Runbot licence: "none shown" / `runbot`, `runbot_merge`, `forwardport` manifests declare LGPL-3; no root LICENSE file / https://github.com/odoo/runbot
16. Kubernetes test-infra tools: all six listed as in that repo / TestGrid, Triage and job configs are there; Prow, Tide, Boskos, kubetest2 live in `kubernetes-sigs` repos / https://github.com/kubernetes/test-infra
17. Dates clarified: sqlx 0.9.0 changelog date 2026-05-06 vs crates.io publish 2026-05-21; sccache 0.18.0 crates.io 2026-09-16 vs GitHub release 2026-09-14 / https://crates.io/crates/sqlx, https://github.com/mozilla/sccache/releases
18. ViDA platform rules 2028-07-01 now noted as postponable to 2030-01-01 by Member States / https://taxation-customs.ec.europa.eu/taxation/vat/vat-digital-age-vida_en
19. Odoo RPC removal: added that only `common` and `object` go in 22 and the `db` service was already removed in 20 / https://raw.githubusercontent.com/odoo/documentation/20.0/content/developer/reference/external_api.rst
20. ERPNext `patch.yml` wording clarified: restores v14, migrates through the v15 and v16 hotfix branches to the PR commit (v14 is restored, not migrated) / https://raw.githubusercontent.com/frappe/erpnext/develop/.github/workflows/patch.yml

**Unresolved (left as stated, flagged):**

- CCX43 "16 dedicated vCPU" is (unverified); Hetzner's price page lists prices only.
- CRA Art. 69(2) (substantial-modification carve-out) and Art. 71 wording were not re-fetched (EUR-Lex blocks automated access); the 2026-09-11 and 2027-12-11 dates are confirmed from the Commission pages.
- Germany EUR 800k / 2028, Veri*Factu 2027-01-01 and 2027-07-01, and France mid-size vs SME split were not re-verified here (AEAT and EC pages returned 404 or no body); they rest on file 09's own verification. The Peppol France page confirms only 2026-09-01 (largest companies) and 2027-09-01 (small companies).
- Every obop timing, cost, FTE and SLA figure is a model [I] and cannot be fact-checked; only the arithmetic was rechecked.
- Cross-file claims cited as "[V file NN]" (Miri env vars, crates.io OIDC-only publishing, Odoo `obfuscate`, Vite+ R1/R3/R13, Postgres relation counts) were not re-opened.
- "Rust has no LTS; only the newest stable is patched" stays [G] (consistent with 1.96.1/1.97.1/1.98.1 being point releases of the then-newest stable, but no policy page was read).
- Tryton's 5-year LTS still rests on the download page's one-line policy plus PyPI history; Google flaky statistics and Blacksmith prices remain unretrieved.

- Owner-constraint pass (2026-10-01): no npm, npx, yarn or bun command existed in this file; changes are limited to making the JS side explicitly vp + pnpm. B1 names `vp install --frozen-lockfile` and `vp test` in the web stage; B3 notes that `setup-vp` `cache: true` caches only pnpm data (viteplus.dev CI guide); B6 replaces "a pnpm-aware generator for npm" with `pnpm sbom` (CycloneDX 1.5 to 1.7, pnpm >= 11.1, pnpm.io docs source) and adds the pnpm equivalent of npm trusted publishing and 2FA (`pnpm publish` OIDC and provenance, verified in the pnpm source; `pnpm stage` / `vp pm stage`, pnpm >= 11.3), marking npm-CLI publishing controls not applicable; B8 adds pnpm's `minimumReleaseAge` (default 1440 minutes since pnpm 11, non-strict unless set), `trustPolicy: no-downgrade`, `blockExoticSubdeps` and the `minimumReleaseAgeExclude` interplay with Renovate security bypass (pnpm audit docs), and rewords "npm" cooldowns to "npm registry packages (via pnpm)"; B9 states the exit drill is a throwaway rehearsal, the one place upstream tools run without `vp`, with pnpm still the only package manager; MUST/SHOULD 16 reworded likewise. Registry facts (Renovate preset names such as `security:minimumReleaseAgeNpm`, package versions, licences) were left as written. Open point for the owner: whether the B9 exit drill (upstream Vite/Vitest/Oxlint/Oxfmt run without `vp`) is acceptable under "all JS tooling through vp". Not verified: `vp pm sbom` forwarding (tagged [I]).