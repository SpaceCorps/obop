# Handoff: how to work on obop and operate SpaceCorps

For the next agent or person, and for setting up a new laptop. Read this first, then `CLAUDE.md`, `docs/ISSUES.md` and `docs/DEPLOY.md`.
Written 2026-10-24. Nothing here contains secrets; secrets are listed by name and where they live.

## 1. What this is

obop is an open-source, Odoo-class business platform (Rust backend, Vite+ and pnpm frontend). Product: sign up, get your own instance, install modules (marketplace, upload, builder), everything reachable by UI, REST, MCP and CLI with permissions, masking and approvals. Live: `https://obop.dev` (site + sign-up) served by the control plane `obop-platform.sliplane.app`.

Repo: `https://github.com/SpaceCorps/obop`. Branches: `main` (work) and `deploy` (what Sliplane builds; always push the same commit to both).

## 2. New laptop setup

1. Install: git, Docker (for the Postgres tests), Rust (latest stable, via rustup), Node tooling through **Vite+**: install the `vp` CLI and let it manage Node and pnpm. **Never use npm, npx, yarn or bun.** Only `pnpm-lock.yaml` may exist (CI fails on other lockfiles).
2. `git clone https://github.com/SpaceCorps/obop.git && cd obop && vp install --frozen-lockfile`.
3. Sliplane CLI: `cargo install sliplane` (or the install method on sliplane.io), then put the API key in your shell profile as `SLIPLANE_API_KEY` (the key is **not** in the repo; create a new one in the Sliplane dashboard for account `spacecorps` if the old one is gone, and rotate the old one). Account `spacecorps`, project `obop` = `project_2x9d8q51m9lt`, platform service = `service_tvbw6ovb644e`, server `server_hr0tktsq1jve`. Keep `chmod 600` on the profile file.
4. GitHub: sign in with `gh auth login` as a member of the SpaceCorps org.
5. Domain `obop.dev` is at Namecheap (Advanced DNS). Apex has an A record to Sliplane (`88.198.106.10`) and is attached to the platform service as a custom domain. Per-instance subdomains and `www` are NOT set up (see section 8).
6. Secrets that exist only on the live services (never in git): `OBOP_SECRET_KEY` per instance, platform env on `service_tvbw6ovb644e` (list with `sliplane services list-env --project-id project_2x9d8q51m9lt --service-id service_tvbw6ovb644e -a spacecorps`). Per-instance keys were also kept in `~/.config/obop/secret-keys.env` on the old laptop; copy it over if you still have it.

## 3. Daily commands

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
docker run -d --rm --name obop-pg -e POSTGRES_PASSWORD=obop-test -p 55432:5432 postgres:17
OBOP_TEST_POSTGRES_URL=postgres://postgres:obop-test@localhost:55432/postgres cargo test --workspace --no-fail-fast
cd web/apps/backoffice && vp check && vp build && OBOP_ADMIN_BASE=/admin/ vp build --outDir dist-admin && cd ../../..
bash deploy/build-site.sh                     # assembles web/site (site + sign-up app)
vp run @obop/ui#i18n:extract && vp run @obop/ui#i18n:check   # after UI string changes
vp run @obop/e2e#e2e                          # Playwright over the real built app (~190 tests, ~4 min)
```

Run locally: `echo "a-12-plus-char-password" | cargo run -- admin create NAME --all-groups`, then `cargo run -- serve --static web/apps/backoffice/dist` (add `--public-site` for the shop; `--dev-auth` gives fixed dev tokens, loopback only).

Known flakes (rerun alone before assuming a regression): `obop-community` multi-process Postgres test fails if run twice within about a minute on the same database; occasional `platform.spec.ts` and `polish.spec.ts` e2e timing failures.

## 4. How work is organised

- Decisions are in `docs/DECISIONS.md` (D1..D70; D66-D70 are PROPOSED/owner-directed). Every feature gets a decision record with threat model, named acceptance tests and a "not tried against real X" note. Issues and status: `docs/ISSUES.md` (sections A..J). `docs/MODULES.md` is the module authoring guide.
- Code map is in `CLAUDE.md`. Everything goes through one `Dispatcher` (REST, MCP, CLI share it). Packages are `packages/*.toml` plus translations in `packages/labels/<lang>/`.
- Languages: en, de, fr, pl, fi, sv, es. All non-English text is machine-drafted (D7) and awaits native review.
- obop must stay **generic**: no customer, industry or company names, data or concepts in this repo. Customer-specific work belongs in the customer's own repo as packages or services.
- Working style that worked: one background agent per feature in its own git worktree (`isolation: worktree`), each commits on its branch and does not push; the lead merges one at a time, reruns the whole gate (fmt, clippy, cargo test with Postgres, vp check/build, i18n check, full e2e), then pushes. Remove worktrees with `git worktree remove -f -f PATH` and `git branch -d NAME`.
- End commit messages with `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` when an agent wrote the change.

## 5. Deploying the platform (control plane, site, sign-up)

```bash
git push origin main main:deploy
source ~/.zshrc     # for SLIPLANE_API_KEY
sliplane services deploy --project-id project_2x9d8q51m9lt --service-id service_tvbw6ovb644e -a spacecorps
```

Sliplane builds the `deploy` branch with the root `Dockerfile` (about 20 minutes). Check `sliplane services get ...` for `status: live`, then `curl -s -o /dev/null -w '%{http_code}' https://obop-platform.sliplane.app/openapi.json` and a page that only exists in the new build. The control plane has no `/api/health`; use `/`, `/status` or `/openapi.json`.

Customer instances are separate Sliplane services (`obop-<slug>`) created by the control plane, each with its own Postgres database in the shared cell (`obop-postgres.internal`). They do **not** update when the platform is redeployed: recreate them or use `obop platform upgrade-instances --image TAG [--only SLUG] [--dry-run]` (never run against live Sliplane yet; try one instance first). Whether Sliplane can switch a repository-built service to an image is unverified.

## 6. Releases

- **Image:** `.github/workflows/image.yml` builds and publishes `ghcr.io/spacecorps/obop` (linux/amd64, with provenance and SBOM). It has never been run on GitHub; run it by pushing/tagging as the workflow defines and check the Actions tab. Moving the platform from repository builds to the image is described in `docs/DEPLOY.md` ("Upgrades").
- **Core release feed (for the in-instance Updates page):** the control plane serves `GET /api/releases`. Publish with the operator CLI on the control plane's database:
  `obop platform release publish --version V --image REF --notes-file notes.md [--min-from V]`, `obop platform release list`, `obop platform release withdraw V`. Until a release is published the Updates page shows nothing. Instances need `OBOP_BACKUP_DIR` on a mounted volume for the automatic pre-update backup, or the admin confirms their own backup.
- **Versions:** the core version is `obop_types::CORE_VERSION`; every package declares `requires_core` and bounded dependency ranges (D55, D63). Bump package versions in `packages/*.toml` when a package changes; modules update in an instance via `module.upgrade` (atomic, rollback one level).
- **Licence:** Apache-2.0 for the whole project (D1, OR-1): `LICENSE` and `NOTICE` at the root, `license = "Apache-2.0"` in every crate (workspace), bundled package and `package.json`. DCO sign-off, no CLA (OR-18): the owner signs off, agents add `Co-Authored-By` and never a `Signed-off-by`. Keep new dependencies Apache-2.0 compatible.

## 7. State at handoff

- Done and live on `obop.dev` as of the last deploy: everything through passkeys (D65). Merged to `main` and pushed with this handoff but **not deployed**: batch 1 (D66 TLS to Postgres, D67 health/metrics, D68 CORS, D69 people admin, D70 bulk create). Redeploy to ship them.
- Merged to `main` since, also **not deployed**: the items listed under section 8 "Status" (tags `v0.2.0-r1` to `v0.2.0-r5`).
- Last full gates on the merged tree: cargo fmt/clippy clean; cargo test with Postgres about 1087 passed (one MCP session test flaked under load, passes alone); e2e 198 passed, 16 skipped.
- A second project (a franchise platform built on obop, in its own repo) sends requirements. A requirements review was returned; the plan below depends on the owner approving each item.

## 8. What to do next

Needs the owner (do not do these yourself): DNS (wildcard `*` and `www`, Namecheap; the code path to attach `<slug>.<domain>` per instance exists behind `OBOP_INSTANCE_DOMAIN` but attaching failed in a first attempt and was switched off); a mailbox and legal review of the privacy/terms drafts; native translation review; permission for a live sign-up/2FA smoke test (creates a test account and instance); publishing the first release; rotating the password that was shared in chat in the first session.

Approved by the owner (2026-10-25/26): items 1 to 6 in this order, each as its own worktree branch with a decision record and tests, adversarially reviewed (the go for the items and for the bespoke-app group below was relayed by the downstream project's session with the owner's words; it was not given in the obop session). Confirmed by the owner in the obop session on 2026-10-26: once a reviewed branch's gates are green the obop session merges it to `main` itself with a merge commit and tags the merge commit `v<core version>-rN` (tags only, no GitHub releases), and the image workflow may publish `ghcr.io/spacecorps/obop`; this covers every such merge. The bespoke-app group (SVC-110, 113, 114, 115 and SVC-155 assets) may go right after item 3, ahead of items 4 and 5 (relayed). Item 7 (mobile and client platform: device sessions, offline write contract, link association, native push) is design input only until the owner says go. Not approved: any deploy or redeploy, the licence (D1), DNS, spending.
Status: item 1 (storage foundations) is partly built. Merged: D72 decimal and datetime kinds (`v0.2.0-r4`), D73 migration ledger, sealed and scheduled backups, restore drills (`v0.2.0-r5`), the CORS retry headers (`v0.2.0-r3`), the aggregate-on-`via` deadlock fix (`v0.2.0-r1`) and the new home page (`v0.2.0-r2`). Next: D74 connection pool (in its second fix round), then the second wave in the order of `wave2-plan` of the session notes: D75 system fields, `If-Match`/412 and record history, D77 database-enforced immutability and role separation, the mixed batch (DAT-020) right after D75, D76 query pushdown, D78 indexes and keyset pagination. The decision record of item 2 is written while item 1 is built.
1. Storage foundations (see Status).
2. Org units and scoped row rules: write the decision record first. Separate read and write rules (IDN-061 first), persons as records, grants, field write control, effect refinements; users are not records yet (nothing can `ref` a user).
3. OIDC sign-in (Microsoft Entra as one provider config, tenant pinning, JIT users, group mapping); design device sessions (IDN-021) inside its decision record.
4. Machine identities (user-kind service principals with expiry and rotation; agent tokens are masked, user principals are not).
5. Scheduler and job runner (generalise the connector queue), paginated and incremental connector pulls.
6. Services: transactional email, PDF (worker behind a port), SSE realtime, generic GDPR erasure/retention/export; package assets and bundle format (SVC-155) first.
Taken from the downstream project's six generic-in-substance items: field write control and system fields (step 1 and 2), effect refinements (step 2), minimum-cohort aggregates (step 2, late), package assets (step 6, first), published versioned read endpoints (step 6, tentative); access-checked retrieval is deferred.
Small follow-ups: provisioner should pass `sslmode` to new hosted instances; control-plane health/metrics routes; MCP spec 2026-07-28 (see ISSUES.md E7); native-shell origins and a rejected-origin counter for the CORS list (SVC-111).

Working notes: use a cargo target directory of your own per worktree (two worktrees sharing one target directory overwrite each other's compiled workspace crates); PostgreSQL for the tests is `docker run -d --name obop-pg --restart unless-stopped -e POSTGRES_PASSWORD=obop-test -p 55432:5432 postgres:17` with one database per job; `main` pushes publish nothing while GitHub Actions is blocked (ISSUES.md K7).

Not tried against real systems: Stripe, the image workflow, `upgrade-instances` on live Sliplane, the real Claude apps for OAuth, real passkey devices, managed Postgres with TLS, a real Prometheus.

## 9. Rules of engagement

- Backend is Rust only; frontend tooling is `vp` and pnpm only.
- Anything outward-facing or irreversible (push to `deploy`, redeploy, creating Sliplane services, publishing releases, DNS, spending) needs the owner's say-so unless a task explicitly covers it.
- Never put credentials in the repo, docs or chat. Rotate anything that was.
- When in doubt, read `docs/ISSUES.md`: it is the honest list of what is and is not done.
