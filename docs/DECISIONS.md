# obop decision register

**Status:** Draft 0.1 for owner ratification. **As of:** 2026-10-01, called month 0 (M0). **Scope:** product and architecture. Nothing here is legal advice; items marked "counsel" need a lawyer.

## 0. Front matter and how to read

This register is authoritative for every other obop document. Where a research note (R01-R28, index R00) disagrees with it, this file wins. Where it is silent, the research stands and later notes outrank earlier ones (R23, R24 and R26 outrank the rest). Module-level design rules (the MUST lists of R09-R16 on ledger, stock, pricing, subscriptions, POS, payroll, services) are not repeated here; module design documents must restate them and cite the R-tag. [CONVENTIONS.md](CONVENTIONS.md) fixes the spelling of names that cross documents (events, permission groups, data classes, states, ports); where it differs from this register, this register wins, and it never settles an owner question. Nothing is built yet.

| Status | Meaning | Rule for writers |
|---|---|---|
| FIXED | Owner-mandated | Never changes without the owner |
| BINDING | Research-resolved technical decision | State as fact; changing it needs a new ADR (next free D-ID; the old record is marked "Superseded by") |
| PROPOSED | Needs explicit owner ratification | Write "Proposed (Dn)" wherever you rely on it |
| DEFERRED | Decide later, with a trigger | Do not design around a guess; cite the trigger |

Conventions. "R23 DR-1" means decision record 1 in research note 23 (section 9 maps tags to files). Dates are ISO-8601, money is EUR ex VAT, "M9" is month 9 after M0. Reversibility is "one-way door" (cannot be undone cheaply once data, licences or outsiders depend on it) or "two-way door". The owner ratifies a PROPOSED record by saying so in chat; the record then gains a "Ratified" line and becomes BINDING. Documents must never write "GA" bare; see D26. Wave (W0-W4), release-number and calendar references inside a BINDING record are placements under D25 and D26, which are PROPOSED: the technical rule is BINDING, the placement stays Proposed until those are ratified. Where a record's own clause rests on a Proposed item (D15 and D23 on I-4 and I-3, D24 on its renames), the record says so.

## 1. Fixed constraints

| ID | Constraint |
|---|---|
| F1 | Backend: exclusively the latest stable Rust, edition 2024. |
| F2 | Frontend and tooling: exclusively the latest Vite+ (`vite-plus`). Every JS/TS task runs through the `vp` CLI. pnpm is the only package manager; `npm`, `npx`, `yarn` and `bun` are never used (CLIs, CI, docs, examples) and `package-lock.json`, `yarn.lock`, `bun.lock` are never committed. pnpm still installs from the public npm registry, so naming the registry or a package is fine. Research passages saying Vite+ supports several package managers describe the tool, not policy. |
| F3 | Clean-room relative to Odoo. Odoo is the functional reference. No Enterprise code or assets, no copied Community code, views, translations or assets, no OCA code. Behaviour comes from public docs and Community source read for understanding; charts and rules come from official sources; Odoo may run only as a black-box differential oracle. |
| F4 | Open source: the core product is OSI-approved open source. Which licence is D1. |

Interpretations, all Proposed (Q10):

- **I-1.** "Backend exclusively Rust" governs code obop authors and ships. Programs obop only runs or tests against (PostgreSQL, PgBouncer, pgBackRest, OCR engines, LiveKit, Weblate, and validators such as KoSIT, Mustang, phase4 as CI oracles) are dependencies. obop authors no Java, Python or Node server code; off-the-shelf sidecars stay optional and out of the default install.
- **I-2.** "`vp` only" means every JS/TS build, test, lint, format, task and environment command is `vp`; `vp install`, `add` and `remove` reach pnpm, as do the typed `vp pm` subcommands (`audit`, `approve-builds`, `publish`, `stage` and others). `vp pm` is a closed command set, not raw forwarding (checked against vite-plus v1.0.0, source and the installed `vp`, 2026-10-01). Direct `pnpm` is allowed only where `vp` has no wrapper: `vp pm sbom` and `vp pm licenses` do not exist, and `vp pm audit signatures` is rejected and must be written `vp pm audit -- signatures`. The JavaScript SBOM and licence scan therefore run as `pnpm sbom` and `pnpm licenses list` through the vp-managed pnpm, invoked as `vp env exec pnpm sbom ...` or inside a `vp run` task; whether that counts as "`vp` only" is the owner's ruling (Q10) and stays open. Ad-hoc fetch-and-run is `vp dlx`, never in CI. The CI lint fails on any other JavaScript package-manager lockfile (the Rust `Cargo.lock` is committed and is not in scope), naming `package-lock.json`, `npm-shrinkwrap.json`, `yarn.lock`, `bun.lock` and `bun.lockb` (F2 lists the first, third and fourth; this is the same rule in full).
- **I-3.** "Latest" means the newest release that passes the pin policy below. Until the owner ratifies this (Q10), the project intentionally runs up to about three weeks behind the newest release, which is one release behind the literal F1 and F2 wording (Rust 1.98.1 while 1.99.0 was published on 2026-10-01).
- **I-4.** The nightly exit drill (section 1.1) is a named exception to "`vp` only": a rehearsal on a throwaway copy of the tree that never ships and is never an obop build path. It is the one place upstream Vite, Vitest, Oxlint and Oxfmt run without `vp`, through pnpm scripts. Shipped builds, CI gates and developer commands always use `vp`.

### 1.1 Pin policy: what "latest" means (D23)

**Status.** PROPOSED with I-3 and I-4 (Q10), applied provisionally from 2026-10-01. It turns the word "latest" in F1 and F2 into dated rules and deliberately runs up to about three weeks behind the newest release until the owner ratifies it.

**Rust.** `rust-toolchain.toml` carries an exact `channel = "x.y.z"` on every branch, with rustfmt and clippy; builds use `--locked`; release builds set `overflow-checks = true`. A release is eligible when it is x.y.1 or later, or x.y.0 at least 14 days old with no open stable-to-stable regression. Adopt within 7 days of eligibility, so the maximum lag is about three weeks. Reason: 1.96.1, 1.97.1 and 1.98.1 each fixed a miscompilation. Skip list: 1.98.0 (vtable regression, rust-lang/rust#161441). A compiler or std advisory overrides the waiting period on every branch. LTS branches keep a per-branch toolchain in a digest-pinned builder image and change it only for such an advisory.

- **Today (2026-10-01): pin 1.98.1 (Proposed, D23; Q10).** Rust 1.99.0 was published on 2026-10-01 (blog post dated that day; `channel-rust-stable.toml` serves it, manifest Last-Modified 12:38 UTC, after a 404 at about 10:10 UTC). The pin stays 1.98.1 because of the rule above (x.y.0 plus 14 days), not because the channel lacks 1.99.0. Adopt 1.99.x between 2026-10-15 (1.99.0 plus 14 days) and 2026-10-22, or at 1.99.1 if that comes first, once debug and release-mode CI pass. 1.99 turns incremental compilation off when `CI` is set. Expect 1.100.0 about 2026-11-12 (eligible 2026-11-26, or at 1.100.1); it brings `wasm32-wasip3` to stable (Tier 3 in 1.99.0; promoted to Tier 2 by rust#161940, milestone 1.100.0).
- **Re-check:** a weekly Renovate toolchain group, a nightly beta canary, and a manual look every six-week release Thursday.

**Vite+.** `vite-plus` is a project-local dependency at an exact version (no range). The exact pnpm is pinned with `vp env pin pnpm@<x.y.z> --target package-manager` (without `--target`, Vite+ 1.0 writes `devEngines.packageManager` instead of the top-level `packageManager` field that the CI guardrails expect), Node in `.node-version`, and `pnpm-lock.yaml` is the only JavaScript lockfile committed (the Rust `Cargo.lock` is committed as well); CI runs `vp install --frozen-lockfile` with `voidzero-dev/setup-vp` at an exact release or SHA (never `v1`) or a digest-pinned image. "Latest Vite+" means the newest `vite-plus` that is at least 7 days old and passes: the exit drill, a LICENSE/NOTICE diff, exactly one Vite and one Vitest copy (`vp why`), and the test suite. One Renovate group moves `vite-plus`, the `vite` alias to `@voidzero-dev/vite-plus-core` and the `vitest` pin in lockstep.

- **Today (Proposed, D23; Q10):** `vite-plus` 1.0.0 (2026-09-28, MIT) becomes eligible 2026-10-05. Its bundle is the toolchain of record: Vite 8.3.1, Rolldown 1.2.11, Vitest 5.0.1, Oxlint 1.85.0, oxlint-tsgolint 7.0.2003, Oxfmt 0.70.0, tsdown 0.23.0. Upstream is already newer (Vite 8.3.2, Rolldown 1.2.12, Vitest 5.0.3, Oxlint 1.86.0, Oxfmt 0.71.0); that lag is accepted and the bundle is not overridden piecemeal. Exception: a published advisory in a bundled component with no Vite+ release within 7 days permits a logged, temporary override.
- **Node:** 24 LTS now; move to Node 26 (LTS from 2026-10-28) by 0.2 (2027-02-02) and before any LTS branch is cut. Node 24 ends 2028-04-30.
- **pnpm version:** one exact `x.y.z` (never a range or dist-tag), pinned with `vp env pin pnpm@<x.y.z> --target package-manager`, under the same 7-day age rule; a new major is adopted only after the settings below are verified on it. Registry state 2026-10-01 14:07 UTC: `latest` and `latest-12` = 12.8.1, `next-12` = 12.8.2, `latest-11` = 11.28.2, `next-11` = 11.28.3. pnpm 12 is the Rust port and the current major; pnpm 11 is the JS line and is still released. The six workspace keys of the settings bullet below were read without warning by pnpm 12.8.1 (checked 2026-10-01, a parse check only); behavioural verification is still on 11.x only. Under the 7-day rule the newest eligible version today is 12.6.0 or 11.27.1; 12.8.1 and 11.28.2 become eligible on 2026-10-05. Which major is the owner's call (Proposed; Q15).
- **pnpm settings:** `minimumReleaseAge` of at least 7 days (strict), `trustPolicy: no-downgrade`, `blockExoticSubdeps`, `strictDepBuilds` with an `allowBuilds` allow-list, and a scanner that reads both YAML documents of `pnpm-lock.yaml`.
- **Exit drill (Proposed exception, I-4; Q10):** a nightly job builds a throwaway copy of the tree with plain Vite 8, Vitest 5, Oxlint and Oxfmt, outside `vp`; a human reviews it quarterly. Until the owner records the exception, the drill is PROPOSED, not BINDING (D15, ruling 26). Tripwires that reopen the Vite+ risk (not the constraint): a licence or NOTICE change; remote cache or core `vp` features behind a paid service; a 1.x minor that breaks a CLI flag or config key; plan issue #2405 slipping a quarter.

**Other dependencies.** Versions live in `[workspace.dependencies]`. Renovate cooldowns: crates 7 days, JS packages 7 days, GitHub Actions pinned by SHA 3 days; security updates skip the cooldown but are reviewed by hand. cargo-deny blocks PRs; pre-1.0 vendor crates are pinned exactly; wasmtime follows D11.

## 2. Decision register

### 2.1 Summary

| ID | Title | Status | Decision in one line |
|---|---|---|---|
| D1 | Licence, CLA, governance | PROPOSED | AGPL-3.0-or-later core, s7 exception, Apache-2.0 SDKs, licence-grant CLA with OSI-forever pledge, company plus EU non-profit |
| D2 | Open/paid boundary, pricing unit | PROPOSED | Everything open; paid = operation and assurance; closed ratcheting list; per-organisation plans |
| D3 | Name and trademark | PROPOSED | "obop" is a working name until counsel clears it |
| D4 | First market, ICP, MVP | PROPOSED | Free invoicing into a seven-app loop; Germany, then France, then Poland |
| D5 | Funding and hiring gates | PROPOSED | Gated plan; at most 17 FTE before EUR 1M ARR |
| D6 | Tenancy unit | BINDING | One PostgreSQL database per business; cells of at most 250 |
| D7 | Realtime and queue signalling | BINDING | No NOTIFY in business transactions; outbox plus coalescing notifier |
| D8 | PostgreSQL floor | BINDING | 17 and 18 supported, 18 is the reference |
| D9 | Keys and money | BINDING | App-generated UUIDv7; minor-unit bigint ledger money; no floats |
| D10 | Authorization algebra | BINDING | Odoo-20 algebra, default-deny, RLS as backstop |
| D11 | Extension and scripting model | BINDING | Five code tiers, CEL only in data, Wasm LTS, no third-party UI in the main realm |
| D12 | Expression storage | PROPOSED | Domains as JSON AST; all else CEL source plus compiled form |
| D13 | Offline scope | PROPOSED | 1.0 at Odoo-19 parity for the apps shipped by then; POS capture with `pos` (W3); queued writes later |
| D14 | Storefront rendering | PROPOSED | Rust-rendered blocks plus Vite islands; no Node in production |
| D15 | Frontend stack, Vite+ risk | BINDING; exit-drill clause PROPOSED (I-4) | React 19 and TypeScript 7 on pinned Vite+ behind one config factory |
| D16 | Release, LTS, deprecation | PROPOSED | Two series a year, every fourth an LTS; 24-month deprecation floor |
| D17 | Compliance operating model | PROPOSED | Manufacturer plus steward; certified intermediaries; signed pack channel |
| D18 | AI posture | BINDING | BYOK, one typed command surface, agents are principals, drafts not posts |
| D19 | Payroll strategy | BINDING | Engine in core, packs, provider connectors; no filing in core |
| D20 | Report and PDF engine | PROPOSED | Typst only in W1-W2 |
| D21 | Performance claims | BINDING | No multiple over Odoo until a harness exists |
| D22 | Odoo-compatibility surface | BINDING | Domain syntax, external ids and importer; no XML-RPC |
| D23 | Pin policy | PROPOSED (with I-3, I-4; applied provisionally) | Section 1.1 |
| D24 | Canonical vocabulary | BINDING; renames and X, L, W, T labels PROPOSED | Section 4 |
| D25 | Module tiers | PROPOSED | T0-T3 with wave placement (section 5) |
| D26 | Release waves and calendar | PROPOSED | W0-W4; one calendar (section 5) |
| D27 | Integration contract | BINDING | Ports, TxEvents, outbox, dependency law |
| D28 | Kernel model and group structure | BINDING | Flat company, branch, ownership edges |
| D29 | Registry, ORM, Rust stack | BINDING | Metadata-first registry; in-house ORM on sqlx and sea-query |
| D30 | Database-enforced integrity | BINDING | Balance, immutability, locks, numbering in PostgreSQL |
| D31 | Public API and versioning | BINDING | Generated REST, MCP, dated version header, free on every plan |
| D32 | Extension packaging and trust | BINDING | One `obop.toml`, signed OCI artefacts, trust levels, sideloading |
| D33 | Security invariants | BINDING | The R18 P0 list |
| D34 | i18n and localisation mechanics | BINDING | PO plus ICU MessageFormat 1; signed effective-dated packs |
| D35 | UI architecture, accessibility | BINDING | Versioned JSON views; WCAG 2.2 AA is a release gate |
| D36 | Quality and release engineering | BINDING | Template-database tests, migration chain tests, generated IDOR matrix |
| D37 | Runtime topology and edge store | BINDING | One binary with roles; SQLite only as fixed-schema edge store |
| D38 | Docs, disclosure and legal pack | BINDING | VitePress in Vite+; equal-time advisories; Data Act terms; export everywhere |
| D39 | Collaboration and messaging services | BINDING | First-party chatter, OAuth-only mail gateway, honest e-signature |

Proposed and awaiting the owner: D1, D2, D3, D4, D5, D12, D13, D14, D16, D17, D20, D23, D25, D26, interpretations I-1 to I-4, the exit-drill clause of D15, and the renames and labels of D24.

### 2.2 Records

#### D1 Core licence, contributor terms, governance
**Status** PROPOSED. **Reversibility** one-way door. **Owner action** yes (Q1): ratify by 2026-10-20 and instruct counsel.

**Decision.** Core and first-party apps under AGPL-3.0-or-later (counsel may choose `-only`) plus an AGPL s7 Application Exception; Apache-2.0 for SDKs, WIT worlds, UI SDK and templates; a commercial `ee` tree in separate artefacts. The CLA is a licence grant (not an assignment) with a patent licence and an irrevocable "OSI licence forever" pledge. A company (CRA manufacturer, cloud, `ee`) plus an EU non-profit steward that holds the trademark and can veto leaving OSI licences.

**Why.** Passes NLnet, Sovereign Tech Fund and public-sector gates; protects against proprietary forks; keeps CRA free-software treatment. **Rejected.** FSL, BUSL, SSPL, ELv2 (not OSI); Odoo-style LGPL plus proprietary EE; permissive core; DCO only; assignment CLA. **Consequences.** Nothing public until counsel covers exception drafting, CLA text, mixed-licence repository, CRA roles, steward form, trademark, patents, dependency compatibility and funder terms (R17 s12). The default Q5 answer date (2027-06-01) falls after the latest public flip (2027-04-06, D26), so the owner must settle in Q16 whether "covers" means written counsel advice or formed entities, or move Q5 earlier. In-process third-party Rust is unsupported. Pledge, veto and investor covenant precede any priced round. **Evidence.** R17 s11-12, R24 s8, R26 C2-C3, R05, R00 D1.

#### D2 Open/paid boundary and pricing unit
**Status** PROPOSED. **Reversibility** one-way door once published, because the ratchet only moves paid to open. **Owner action** yes (Q3): ratify section 6 before the first public commit; marketplace and white-label terms follow in Q13.

**Decision.** Every first-party app and every security outcome is open; paid means operator convenience, assurance and operated or warranted services (section 6). The list is published, closed and ratcheting; `ee` code converts to open 24 months after release. Price per organisation (one business), banded by full users; portal users, up to 3 accountant seats and light users (fair use, up to 5x full users) are free. Free, EUR 49, EUR 249 and from EUR 899 are price-test hypotheses. Marketplace: 0% at launch, cap 15%, any rate announced 12 months ahead, never on free packages or sideloads.

**Why.** Odoo's gated accounting and API are the main grievance; R17's gating of SSO, SCIM, audit and white-label as written contradicts the one-edition promise. **Rejected.** Per-user pricing with a moving line; everything free plus services; R17 s11.2 as written. **Consequences.** OIDC, TOTP, passkeys, audit log and export can never be sold; ARPA shortfall is a top-8 risk (R26 risk M9); price tests are needed. **Evidence.** R26 A2-A3, R25 B7, R24 s10, R17 s11.2, R03, R04.

#### D3 Name and trademark
**Status** PROPOSED. **Reversibility** one-way door once announced or filed. **Owner action** yes (Q2).

**Decision.** "obop" is a working name. No announcement, spend or filing until counsel clears EUIPO, UKIPO, WIPO, DPMA and INPI in classes 9, 35, 42 (and 41, 45) and opines on OBOPP (pending, class 35) and OBOL (two live, classes 9 and 42). Reserve the crate, PyPI, GitHub org variant, GHCR and Matrix handles now (the npm registry name `obop` is taken; publish `@obop/*`). Keep screened fallbacks (kernelbiz, monobiz, mercora). File in the steward's name.

**Why.** Clearance risk is medium; US intent-to-use marks cannot be assigned early. **Rejected.** Rename now; proceed and file. **Consequences.** A public repository is an announcement, so the public flip waits for clearance or a fallback name (gate G0, D26). **Evidence.** R28 s2, A.5.

#### D4 First market, ICP and MVP shape
**Status** PROPOSED. **Reversibility** two-way door, but costly (hiring, partners). **Owner action** yes (Q8).

**Decision.** A free compliant-invoicing on-ramp into the seven-app loop (Contacts/Products, CRM, Sales, Inventory plus Purchase, Invoicing plus Accounting, Payments, Portal with PDF and email documents and attachments; the `documents` DMS app is W3, D25), one company, ICP 5-50 person services and light trade. Germany first (pack stable 2027-09-30), France second (2028-03), Poland third (2028-08), re-scored in 2027-Q2. Preconditions: written SKR03/04 and DATEV confirmation; Peppol via a partner access point. Out of the first delivery: POS, payroll, manufacturing, multi-country tax.

**Why.** Germany scores 84 of 100, has about 281k firms of 10-249 staff without ERP and the 2028-01-01 all-issuer mandate. **Rejected.** Vertical wedge; platform first; UK/US (no compliance wedge); BE/NL (small base). **Consequences.** Flip France and Poland if no French compliance lead or PA partner by 2027-03-31; SKR/BAS licences may force data-pack delivery. **Evidence.** R25 A7-A8, R20 s9, R09.

#### D5 Funding path and hiring gates
**Status** PROPOSED. **Reversibility** one-way door for covenants. **Owner action** yes (Q4).

**Decision.** Gated plan: about EUR 3M seed for 24 months, then about EUR 5M against EUR 1M ARR (about EUR 8M total). No more than 17 FTE before EUR 1M recurring ARR (base case about M29). If paying organisations are under 60 at M24 (2028-10-01), cut to 6 FTE and bridge with services and partners. Pledge, veto and investor covenant before any priced round. Apply to NLnet Restack (2026-11-03 12:00 CET) for discrete crates.

**Why.** The ungated base model peaks at about EUR 10.8M (raised as about EUR 12M); gating cuts the peak to about EUR 7.1M. A six-month paid-cloud slip adds about EUR 5.7M; grants cover under 6% of burn. The model is not independently reproducible (a rebuild gave about EUR 12.7M and M71). **Rejected.** Ungated hiring; lean services-funded as default (kept as fallback). **Consequences.** W3 breadth is paced by ARR, not the calendar. **Evidence.** R26 B5-B7, C1-C3, R00 s7.

#### D6 Tenancy unit
**Status** BINDING. **Reversibility** two-way door (pool mode can be added); very high cost if `company_id` discipline is skipped. **Owner action** no for the mode; the Free-tier default is confirmed at the spike readout (Q6).

**Decision.** One PostgreSQL database per business is the only mode. `company_id` on every row, a router trait (`TenantRouter`, Proposed rename `BusinessRouter` in D24), no cross-business SQL. Cells hold at most 250 business databases, with PgBouncer per cell and a control-plane directory business-to-cell. Pool mode is not built (if built: `tenant_id` leads every key, composite FKs, RLS primary). Spike: 250 databases of 800 tables on PG 18; pass means fan-out migration under 30 minutes and idle memory under 4 GB; readout 2026-11-27. The Free tier stays database-per-business (Proposed) unless the spike fails, Free costs exceed 10% of COGS or conversion falls under 2% (R26 risk M7); then the owner decides on a pooled Free tier (DEFERRED).

**Why.** Odoo and Frappe parity; per-business backup, erasure and residency. **Rejected.** Schema per business; pool default; silo and pool both first-class. **Consequences.** Migrations are fleet jobs (about 190k tables per cell). **Evidence.** R23 DR-2, R22 s10, R08, R18 s4, R26 risks T6 and M7.

#### D7 Realtime and queue signalling
**Status** BINDING. **Reversibility** two-way door (low to medium). **Owner action** no.

**Decision.** `NOTIFY` is forbidden in business transactions; CI allows it only in `obop-bus-notifier`. Durable events are outbox rows plus per-subscriber delivery rows. Wake-up ladder: in-process `tokio::sync::Notify`, then a coalescing notifier (at most 10 per second per node, `SET LOCAL synchronous_commit = off`) on one bus database per cluster, then polling at 250 ms to 2 s. Each node keeps one LISTEN connection, on the bus database only. Browser realtime is a WebSocket resuming from `(xid8, event_id)` below the snapshot xmin. Single-business installs use the same path.

**Why.** Source reading shows the NOTIFY commit lock is cluster-wide and LISTEN is per database. **Rejected.** NOTIFY in transactions; plain polling; NATS now (COULD). **Consequences.** The pgbench spike (32 databases; coalesced at least 90% of plain) may relax the ban to SHOULD, never the one-LISTEN rule. **Evidence.** R23 DR-1, R22 s1.4.

#### D8 PostgreSQL floor
**Status** BINDING. **Reversibility** two-way door to raise the floor, hard to lower it. **Owner action** no.

**Decision.** Support 17 and 18 at 1.0; 18 is the reference. CI runs 17, 18 and 19 from RC. Kernel DDL uses 17-level features; 18-only features (temporal foreign keys, virtual columns, `uuidv7()` default, `RETURNING OLD/NEW`, `NOT NULL ... NOT VALID`) sit behind `requires_pg = 18`. Raise the floor to 18 when 17 is 12 months from EOL (2028-11) or Debian stable and Supabase ship 18.

**Why.** 16 has under two years left at a 2027 release and lacks incremental backup; Debian 13 and Supabase do not yet ship 18. **Rejected.** Floor 16; 18 only. **Consequences.** Overlap guarantees use `EXCLUDE ... gist` on 17. **Evidence.** R23 DR-3.

#### D9 Keys and money
**Status** BINDING. **Reversibility** one-way door once data exists; the key freeze waits for the spike (2026-11-27). **Owner action** no; the spike result decides the key type (Q6).

**Decision.** Every model has an application-generated UUIDv7 primary key (`Id<M>`); ISO catalogues use natural codes; module records carry external ids `module.name`; mutating commands carry a unique `command_id`; `created_at` is explicit. A per-model `IdType` flag lets ledger, stock-move and log tables switch to `bigint` plus a public uuid if the 300M-row spike exceeds +25% size or 15% slowdown. Ledger money is `bigint` minor units plus `currency_code` (exponent as data); document totals `bigint`; unit prices `NUMERIC(24,8)`; quantities `NUMERIC(24,6)` in base UoM; FX rates `NUMERIC(28,14)`; valuation `value_minor bigint`. Per-company rounding `per_line` or `per_group` (mandatory for EN 16931). Floats are banned.

**Why.** Stable ids across databases; exact arithmetic. **Rejected.** `bigint` identity everywhere; `bigint` plus public uuid as default. **Consequences.** About +20% ledger row width. **Evidence.** R23 DR-5, DR-6.

#### D10 Authorization algebra and RLS
**Status** BINDING. **Reversibility** one-way door in practice (every module's seed access data). **Owner action** no.

**Decision.** Odoo 20's algebra: OR of permission rows, AND of restriction rows, compiled from the Domain AST to SQL, one path for list, by-id, export, webhook and AI reads. Default-deny at model, field and action level; every permission declares a scope (`all`, `own`, `company`, `domain`); an implicit TRUE is a lint error. Elevation is a typed, audited `Env<Elevated>`. Field security is enforced in the projection and the query compiler, which rejects filter, sort and group on unreadable fields; sensitive classes live in separate tables or columns behind `obop_cls_*` roles. RLS is a fixed row-local backstop (FORCE, non-owner non-BYPASSRLS role, `set_config(..., true)`). Bulk import stages through non-RLS tables.

**Why.** RLS cannot express the algebra; Cedar cannot filter lists. **Rejected.** Default-allow `ir.rule`; ABAC engine; RLS primary. **Consequences.** A property test must give identical answers from list, by-id, export and RLS paths; benchmark the restrictive policy on 10M ledger rows. **Evidence.** R23 DR-4, R18 s3, R02.

#### D11 Extension and scripting model
**Status** BINDING; the Wasm/AGPL boundary needs counsel. **Reversibility** two-way door except the licence boundary. **Owner action** no.

**Decision.** Code tiers X1 declarative pack, X2 CEL, X3 Wasm component, X4 remote app, X5 first-party native crate (Proposed labels, renamed from R24's T1-T5, D24). CEL is the only expression language stored in data for conditions, modifiers, formulas, validation and policies; Rhai is not shipped in W1-W2 (COULD later, only inside a signed pack). Template languages are a separate, bounded category (Proposed with D14 and D20): minijinja templates and Typst markup are allowed only as sandboxed render-time templates with fuel, recursion and no-I/O limits, and never express conditions, modifiers, formulas, validation or policies. CEL needs save-time limits (4 KB, AST depth 32), a step budget and a deadline, added in a W1 spike. X3 pins a wasmtime LTS (v48; next v60 about 2027-08-20) behind `obop-wasm-host`, patches within 7 days (24 h for High), never relies on fuel alone, and runs in the `ext` role. UI tiers F0 declarative, F1 trusted ESM (first-party and business-own only), F2 remote-dom, F3 sandboxed iframe; third-party UI never enters the main realm.

**Why.** In-process plugins are AGPL derivatives and have the worst security record. **Rejected.** In-process crates; Rhai in data; main-realm marketplace UI. **Consequences.** W1 cannot promise Wasm; one internal Wasm connector spike in months 10-12; the exception needs counsel before third parties are solicited. **Evidence.** R24 s3-4, s8, R17, R21.

#### D12 Expression storage
**Status** PROPOSED. **Reversibility** two-way door early, costly after data exists. **Owner action** yes (Q9).

**Decision.** Domains are a JSON AST. Modifiers, visibility, conditions and formulas store CEL source plus a type-checked compiled form, run by one Rust evaluator (server and Wasm client) under conformance tests. A CEL-like authoring syntax for domains may compile to the AST.

**Why.** Domains must compile to SQL; CEL fits everything else. **Rejected.** JSON AST for all (R19); CEL text for all. **Consequences.** One evaluator kept in lockstep across server and client; the W1 spike adds the cost limits. **Evidence.** R24 s3.2, R19.

#### D13 Offline scope
**Status** PROPOSED. **Reversibility** two-way door. **Owner action** yes (Q11).

**Decision.** 1.0 targets Odoo-19 parity for the apps that ship by then: a read cache of visited views and an explicit degraded-feature list. POS order capture with post-reconnect invoicing ships with `pos` and `obop-edge` in W3 (D25), after the 1.0 LTS (2028-04-18), unless the owner pulls a POS-capture slice into W2 (Q11). Queued writes for allow-listed models come later (W3+). No Electric, PowerSync or Zero. Architecture is D37.

**Why.** Odoo 20 adds queued writes, but a sync engine is expensive and none fits (non-Rust, FSL, no offline writes). **Rejected.** Odoo-20 parity at 1.0; local-first engine. **Consequences.** POS and warehouse get an edge command outbox from W3; the degraded-feature list is published; 1.0 makes no POS offline promise. **Evidence.** R22 MUST 3-4, R19, R12.

#### D14 Storefront rendering
**Status** PROPOSED. **Reversibility** two-way door until W3. **Owner action** yes (Q14).

**Decision.** Rust-rendered block snapshots (minijinja, also built to Wasm for editor preview) with Vite islands; no Node in production; islands and the manifest are built only with `vp build`. Astro, Leptos or Dioxus would each need an explicit owner waiver of the Vite+ rule (F2), because none is part of Vite+.

**Why.** Keeps the runtime Rust-only and cacheable; vite-ssg is broken under Vite+ (#1315). **Rejected.** Node SSR sidecar; separate Astro app. **Consequences.** App-like checkout flows stay Rust plus islands. **Evidence.** R12 s9, R07 s3.9.

#### D15 Frontend stack and Vite+ risk
**Status** BINDING, except the nightly exit drill, which is PROPOSED as a named exception to "`vp` only" (I-4, Q10). **Reversibility** two-way door, moderately costly. **Owner action** no.

**Decision.** React 19 and TypeScript 7, TSX only; the React Compiler via `@rolldown/plugin-babel`; TanStack Query, Router, Table and Virtual; no `baseUrl`; no `.vue` or `.svelte` in core. No commercially licensed runtime dependency (CI licence allow-list). Rust owns contracts: OpenAPI 3.1 from utoipa, meta-model types via ts-rs or specta, generated files diff-gated. All `vite-plus` imports pass through one config factory and one test re-export module. Nightly exit drill on a throwaway copy of the tree (section 1.1; Proposed, I-4) and a named engineer who triages Vite+ issues.

**Why.** The thin alias layer and a days-old 1.0 are the risk, not the components beneath. **Rejected.** Vue, Svelte, Solid. **Consequences.** TanStack Start stays out until it leaves RC; AG Grid Enterprise, Handsontable and similar are excluded. **Evidence.** R07 s2-3, R26 T1-T2, R27 B9.

#### D16 Release, LTS and deprecation
**Status** PROPOSED. **Reversibility** one-way door once customers rely on it. **Owner action** yes (Q7).

**Decision.** Two series a year, released on the third Tuesday of April and October. `N.0` is an LTS with five years of support (years 0-2 full, 2-5 security and legal-critical); `N.1`-`N.3` get 9 months. Contract migrations and API removals happen only at `N.0`. Monthly patch train on the first Tuesday; hosted fleet in rings 0-3; required upgrade stops at `N.0`. Public contracts (API, WIT, manifest, pack schema, UI SDK F2/F3) are deprecated for at least 24 months, then removed at the first `N.0` after that (effective window 24-48 months); F1 gets 12 months. Counting rule (Proposed, Q7): months are release months, so April 2028 to April 2030 counts as 24; counted in days, 2.0 (2030-04-16) is two days short of 24 months after 1.0 (2028-04-18) and nothing deprecated at 1.0 could go at 2.0. Under the release-month rule anything deprecated at 1.0 is removable at 2.0, and anything deprecated at 1.1 to 1.3 or at 2.0 is removable at 3.0 (April 2032, estimate).

**Why.** CRA five-year support; Odoo's yearly breakage is the grievance. **Rejected.** Annual majors; rolling releases; 12-month deprecation. **Consequences.** Two to three LTS lines plus one or two regular ones stay live; if LTS upkeep exceeds 20% of engineering for two quarters, stretch LTS spacing to 2.5 years; `compat.json` per release; telemetry opt-in. **Evidence.** R27 C1-C8, R24 s5.3, R21.

#### D17 Compliance operating model
**Status** PROPOSED. **Reversibility** partly one-way (legal roles). **Owner action** yes (Q5, counsel).

**Decision.** The company is CRA manufacturer and the non-profit the steward; the Art. 14 runbook (24 h, 72 h, 14 days) is live before any commercial offer. Peppol through a certified access point (own access point only near 250 businesses); France by integrating with a PA, not accrediting; Spain gated behind counsel (EUR 150,000 per year per system type). Country packs are signed bundles with an oracle-gated ladder (D34). Law-defined content is open; operated, warranted or certified services are paid (section 6). Warranty is disclaimed by default and capped only on the paid Assurance service.

**Why.** Mandates move faster than releases. **Rejected.** Owning every regime; partner-only. **Consequences.** Counsel before any pack claims conformance; E&O cover; standing feeds for code lists. **Evidence.** R25 B, R17 s7, R09.

#### D18 AI posture
**Status** BINDING. **Reversibility** two-way door. **Owner action** no.

**Decision.** Bring your own key, no obop credit wall; hosted credits are an optional paid convenience at cost plus 20%. The typed command API is the only agent surface; MCP 2026-07-28 via `rmcp`. Agents are principals (permissions are the intersection of user and token scopes), use dry-run with a digest-bound `plan_id` and idempotency keys, and create drafts but never post or pay without human approval. A governed semantic layer answers questions; a fully self-hosted profile exists; every AI feature can be switched off per module; no non-commercial weights by default.

**Why.** Odoo 20 meters AI through IAP credits; agents need the same permission path as users. **Rejected.** Metered credits; a parallel AI API; raw text-to-SQL. **Consequences.** An approval policy engine (auto, confirm, approve-by-role) is a W2 deliverable (placement per D25, Proposed); HR and credit-scoring features are AI-optional for the EU AI Act high-risk date 2027-12-02. **Evidence.** R16, R21, R26 A2.

#### D19 Payroll strategy
**Status** BINDING; reference country DEFERRED. **Reversibility** two-way door. **Owner action** no.

**Decision.** A generic rule engine in core (CEL, decimals, explainable and reproducible runs), separately versioned country packs, and provider connectors. No statutory filing in core, no arbitrary code in the database, no AI in hiring decisions by default, effective-dated employee versions. A pack without golden cases cannot be stable. The first reference pack (France via `modele-social`, or US federal) is decided on 2029-04-01 (M30) and need not match the first accounting country.

**Why.** Payroll is the moat and the liability; Odoo's open payroll left Community in 13.0. **Rejected.** Country engines in core; connectors only. **Consequences.** Warranty applies only to the paid payroll service. **Evidence.** R13, R25 B7, R26.

#### D20 Report and PDF engine
**Status** PROPOSED. **Reversibility** two-way door. **Owner action** yes (Q12).

**Decision.** Typst behind `obop-report` (PDF/A-3b, PDF/UA-1) for all documents in W1-W2; user-designed templates through minijinja (fuel and recursion limits) feeding Typst. No wkhtmltopdf. A browser renderer only ever runs in an isolated worker with no network or file access.

**Why.** One renderer, archival output, no headless-browser attack surface. **Rejected.** Typst plus pooled Chromium; paper-muncher. **Consequences.** User-designed HTML templates are limited to the minijinja-to-Typst path. **Evidence.** R15, R18, R24, R06.

#### D21 Performance claims
**Status** BINDING. **Reversibility** two-way door. **Owner action** no.

**Decision.** Publish no multiple over Odoo until the Odoo-versus-obop harness runs on fixed hardware; cite Odoo only by its documented worker formula (about 6 users per worker). Position Rust on correctness and operations.

**Why.** Gains are unmeasured: the research infers 3-10x per box (design inference [I], R22 s3.2; not for publication) and models infrastructure at about 6% of year-3 revenue (estimate, R26; D5 notes the model is not independently reproducible). **Rejected.** Publishing 5-10x targets now. **Consequences.** Running the harness monthly from M9 (calendar per D26, Proposed) and publishing driver and results is the way to lift the ban. **Evidence.** R22 MUST 9, R27 A7, R26 T8.

#### D22 Odoo-compatibility surface
**Status** BINDING; endpoint DEFERRED. **Reversibility** two-way door. **Owner action** no.

**Decision.** Odoo's domain syntax as the AST, `module.name` external ids, and an importer (JSON-2 and exports, dry-run in a transaction, identity ledger). No XML-RPC. Clean-room (F3). A JSON-2-compatible `/json/2/` endpoint is DEFERRED until three design partners ask or Odoo's RPC removal (fall 2028) creates migration demand.

**Why.** Switching cost is the main acquisition lever. **Rejected.** Clean break; XML-RPC compatibility. **Consequences.** The importer is a product with a CI round-trip test. **Evidence.** R08, R21, R03, R04.

#### D23 Pin policy
**Status** PROPOSED (applied provisionally from 2026-10-01); it becomes BINDING under F1 and F2 when the owner ratifies I-3 and I-4 in Q10. **Reversibility** two-way door. **Owner action** yes (Q10, by 2026-10-15): ratify, or choose literal newest-on-release-day.

**Decision.** The rules are section 1.1. From 2026-10-01 until Rust 1.99.x is adopted the project intentionally runs one release behind the literal F1 and F2 wording. **Why.** Three recent Rust patch releases fixed miscompilations and Vite+ 1.0 is days old. **Rejected.** Floating `stable`; adopting 1.99.0 on day zero (R06 MUST 1). **Consequences.** A dated adoption log lives in the repository. **Evidence.** R06, R07, R27 B8.

#### D24 Canonical vocabulary
**Status** BINDING for the terms and the ambiguous words they forbid (business, company, party, pack, module, app, port and the rest of section 4); PROPOSED for the renames and labels marked "(Proposed)" there. **Reversibility** two-way door. **Owner action** yes for the Proposed part (Q8 for the T and W labels, Q10 for the renames and the X and L prefixes).

**Decision.** Section 4 defines terms and forbids ambiguous ones. The renames there (`BusinessTx`, `BusinessRouter`, and the X, L and W prefixes) are Proposed naming changes; nothing is built, so they cost nothing today. **Why.** "Tenant", "business", "company", "group" and "T" labels carried several meanings across R01-R28. **Rejected.** Keeping R23's "T0 (21) and T1 (19)" sets next to R01's tiers. **Evidence.** R23 DR-2.

#### D25 Module tiers and delivery order
**Status** PROPOSED (the T0 app list needs the owner). **Reversibility** two-way door. **Owner action** yes (Q8).

**Decision.** Tiers T0-T3 and wave placement as in section 5. R20's cut lines govern W1; R23's graph is the dependency law, not the delivery list. Tier answers "is a business OS complete without it"; wave answers "when". **Why.** R01, R20 and R23 scoped the first delivery differently. **Rejected.** R23's 40-module sets as the delivery list; R01's full T1 as the MVP. **Consequences.** HR, assets and expenses are T1 but ship in W3, except the employee core that `timesheet` needs in W2. **Evidence.** R01 s6, R20 s9, R23 D.3.

#### D26 Release waves and calendar
**Status** PROPOSED. **Reversibility** two-way door except regulatory anchors. **Owner action** yes (Q8).

**Decision.** Waves W0-W4 and one calendar (section 5). Three milestones once conflated as "GA" are separate: "pack stable" (DE 2027-09-30), "paid cloud" (opens 2027-07-06 as a paid beta; SLA plan from 2027-10-01) and "1.0 LTS" (2028-04-18). CRA duties apply from the first commercial offer. The repository stays private until gate G0 passes (D1-D3 ratified or cleared, CLA bot live, paid list published), then goes public with full history, at the latest at 0.3 public alpha (2027-04-06). D1's counsel condition reaches CRA roles and steward form, whose default Q5 answer date (2027-06-01) is later than that flip; until Q16 is answered, G0 has no settled date.

**Why.** R20, R25, R26 and R27 dated "GA" and "paid cloud" differently. **Rejected.** One "GA" word; shipping a "1.0" before 2027-12-11 to dodge CRA. **Consequences.** A six-month paid-cloud slip costs about EUR 5.7M, so instrument launch and conversion first. **Evidence.** R20 s10, R25 A8, R26 B5-B7, R27 C10.

#### D27 Integration contract
**Status** BINDING. **Reversibility** one-way door. **Owner action** no.

**Decision.** Mechanisms in order: port call (`Arc<dyn Port>` from the owner's `*-api` crate; optional ports are `Option<Arc<...>>` and need no bridge), transactional `TxEvent`, extension point, durable event (outbox plus per-subscriber delivery, `SKIP LOCKED`, lease). Rules R1-R7: one command one transaction; per-aggregate ordering with `aggregate_version`; at-least-once with `inbox_processed`; compensation by reversal documents, sagas only across businesses; an `xtask` dependency law (implementation crates depend only on kernel crates and `*-api` crates; no implementation-to-implementation edges); handler traits owned by `obop-events`; `auto_install` per business. Hub ports `DemandPort`, `QtyContributionPort`, `PostingPort`, `InvoiceableSource` keep bridges linear. Implementers (CONVENTIONS 7.1; those beyond the first are Proposed until this record is amended): `stock` for `DemandPort`; `sales` for `QtyContributionPort` (`purchase` for purchase lines, Proposed); `account`, by account role, for `PostingPort`; `sales` and `subscriptions` for `InvoiceableSource` (`purchase`, `pos` and `expenses`, Proposed). `timesheet` is not an `InvoiceableSource` implementer, so time-only invoicing is an open product question (ARCHITECTURE 16.3 item 7, decided with Q8), and whether `payment` publishes a hub port instead of the `sale_payment` bridge would amend this record and is open (CONVENTIONS 9.1).

**Why.** Odoo's 400 auto-install bridges create gravity wells (`account` has 398 transitive dependents). **Rejected.** `_inherit` glue; Frappe-style stringly `doc_events`; microservices. **Consequences.** Build-time graph check; async ports use boxed futures until native dyn async stabilises. **Evidence.** R23 D, R01 s2.

#### D28 Kernel model and group structure
**Status** BINDING. **Reversibility** one-way door, mitigated by expand-only growth. **Owner action** no.

**Decision.** Party is person or organisation; customer and vendor are per-company roles in `party_profile`. Principals (user, portal, guest, API client, agent, system) are not parties. `company` is flat and tied to one legal-entity party; `branch` is separate; groups are `ownership_interest` edges; VAT groups are `fiscal_unit`. Documents follow a header-line pattern with state machines as registry data, a three-value kernel lifecycle and derived progress fields. W1 ships one company with `company_id NOT NULL`, composite FKs and a dormant `branch`; multi-company arrives in W2, intercompany in W3, consolidation in W4 (wave placement per D25 and D26, Proposed).

**Why.** Odoo overloads one company tree (a parent cannot later become a branch). **Rejected.** A single company tree; customer and vendor as separate tables. **Consequences.** Later work rewrites no keys. **Evidence.** R23 B-C.

#### D29 Registry, ORM and Rust stack
**Status** BINDING. **Reversibility** one-way door. **Owner action** no.

**Decision.** One runtime registry (typed Rust models plus business rows) drives ORM, API, OpenAPI, MCP, views, access, webhooks, imports, docs and the semantic layer. Field storage has three tiers: typed columns, promoted custom columns, JSONB properties; no EAV. In-house ORM on sqlx 0.9 and sea-query 1.0; SeaORM only for fixed system tables. Domain AST compiled to SQL with bound parameters. Stack: tokio only; axum isolated in `obop-http`; utoipa 6; `Money` over rust_decimal; jiff for time-zone logic.

**Why.** Studio-class customisation, APIs and agents are cheap only if built-in behaviour is metadata. **Rejected.** A static ORM as the core; EAV. **Consequences.** Compile times in a 100-plus crate workspace need a strict crate DAG and sccache. **Evidence.** R08, R06, R02, R23.

#### D30 Database-enforced integrity
**Status** BINDING. **Reversibility** one-way door. **Owner action** no.

**Decision.** PostgreSQL enforces a deferred balanced-entry trigger, posted-row immutability, lock dates, gapless legal numbers assigned at posting from a row-locked counter, a per-journal hash chain, a unique quant key with `ON CONFLICT`, and immutable done stock moves. Corrections are reversal documents. Stock valuation serialises per company and product.

**Why.** Odoo checks balance only in the ORM and tolerates duplicate quants. **Rejected.** Rust-only invariants. **Consequences.** Triggers and the hash chain ship in migrations; posting assigns numbers late in the transaction. **Evidence.** R08, R09, R10, R23.

#### D31 Public API and versioning
**Status** BINDING. **Reversibility** two-way door until stable endpoints exist. **Owner action** no.

**Decision.** The API is generated from the registry: REST/JSON with action endpoints, OpenAPI 3.1 (3.2 later), RFC 9457 errors, cursor pagination with `updated_since` and tombstones, atomic `POST /$batch`, `Idempotency-Key`, Standard Webhooks, an OAuth 2.1 provider, MCP. URL major `/api/v1` plus a date-versioned `Obop-Version` header; stable endpoints are never removed within a major. Free on every plan. No XML-RPC; no GraphQL in W1.

**Why.** Odoo gates its API to the Custom plan, removes RPC in 2028 and has no atomic multi-step call. **Rejected.** Plan-gated API; OData or GraphQL first. **Consequences.** Per-business OpenAPI includes custom fields and installed packages; 24-month windows (D16). **Evidence.** R21, R27 C2, R08.

#### D32 Extension packaging and trust
**Status** BINDING. **Reversibility** two-way door. **Owner action** no.

**Decision.** One `obop.toml` for every package kind (pack, wasm, remote, native, theme), replacing R02's `module.toml` and R21's `obop.app.toml`. OCI artefacts with publisher ed25519 plus cosign keyless signatures, CycloneDX SBOM and a capability manifest diffed per upgrade. Trust levels private, community, reviewed, partner, core. Sideloading is always allowed for self-hosters; on shared cloud it is limited to X1-X4 (R24 s9). Nightly compatibility runs; a registry kill switch. Three data layers (module, business overlay, user); business customisation exports as a package.

**Why.** Compatibility is a product; Odoo's lack of it created OCA and OpenUpgrade. **Rejected.** Twenty-style no-review; raw SQL for third parties. **Consequences.** A package registry service and review tooling ship in W2 (placement per D25, Proposed). **Evidence.** R24 s5, s9.

#### D33 Security invariants
**Status** BINDING. **Reversibility** one-way door. **Owner action** no.

**Decision.** The R18 P0 list is design-time law: a business-scoped transaction handle (`BusinessTx`, a Proposed rename of `TenantTx`, D24); one query path with default-deny; typed elevation; append-only hash-chained audit log; Argon2id, TOTP and passkeys with revocable sessions; one SSRF-hardened egress client and an isolated report renderer; integer money with `overflow-checks`; personal-data flags driving erasure; expand/contract migrations; cargo-deny, cargo-audit and a frozen pnpm lockfile from the first commit.

**Why.** These cannot be retrofitted. **Rejected.** Hardening later. **Consequences.** P1 items (signed releases, SBOM, SECURITY.md, OIDC, SCIM subset) precede the first public release; P2 (cargo-vet, audit checkpoints, pentest) precede audits. **Evidence.** R18 s12.

#### D34 i18n and localisation mechanics
**Status** BINDING. **Reversibility** two-way door. **Owner action** no.

**Decision.** PO catalogues with an ICU MessageFormat 1 subset (Lingui on TypeScript; PO parser and `icu` in Rust); Weblate; 12 target locales at first pack stable; machine translation is a suggestion and banned for statutory labels; RTL and CJK fixtures in CI; search is capability-detected (`pg_trgm`, `unaccent`, ICU; PGroonga for CJK). Packs are signed, effective-dated bundles on `stable`, `preview` and `edge` channels; gate ladder draft, preview, stable, certified; oracles CEN artefacts, KoSIT, KSeF DEMO. BAS data never enters the public repository. MessageFormat 2 is reviewed in 2027-H2.

**Why.** Odoo itself uses Weblate; law moves faster than releases. **Rejected.** Fluent (doubles catalogues); MessageFormat 2 now (no mature Rust implementation). **Consequences.** Data, pack and core cadences differ; extension availability on managed PostgreSQL is unverified. **Evidence.** R25 B-C.

#### D35 UI architecture and accessibility
**Status** BINDING. **Reversibility** two-way door. **Owner action** no.

**Decision.** Views are versioned JSON ASTs with named slots and an effective-view resolver; view-type and field-renderer registries. WCAG 2.2 AA is a release gate (React Aria, axe, every drag has a non-drag path). Logical CSS only; DTCG 2025.10 tokens as runtime CSS variables; four saved-view scopes; imports with dry-run and batch rollback. The v1 catalogue includes views Odoo gates (report viewer, reconciliation split view, approvals inbox).

**Why.** Avoids Odoo's multi-era UI and makes accessibility structural. **Rejected.** XPath-style patching; commercial grids. **Consequences.** CI bans physical margins; a 100k-row performance budget is enforced. **Evidence.** R19, R07.

#### D36 Quality and release engineering
**Status** BINDING. **Reversibility** two-way door. **Owner action** no.

**Decision.** Template-database test harness; append-only migration ledger, never squashed inside a line; N-1 chain test from 0.3, N-2 plus a 1,000-business fan-out from 0.5 (release numbers per D26, Proposed); a registry-generated IDOR matrix before any multi-business deployment; proptest invariants for ledger, tax, pricing and stock; a Tryton-style inherited structural suite; expand/backfill/contract tags enforced in CI; reproducible double builds for LTS; artefacts, SBOMs and attestations kept 10 years; once the repository is public (G0, D26) it runs on free runners with a budget of USD 300-800 per month. While it is private, free runners do not apply and the runner plan is open (Q17).

**Why.** It is what 4-12 people can sustain, with ERPNext and Tryton precedent. **Rejected.** Migrate-per-test; hand-made fixtures. **Consequences.** A quality-platform engineer (0.5-1 FTE); benchmark the three clone strategies in sprint one. **Evidence.** R27 A-B.

#### D37 Runtime topology and edge store
**Status** BINDING. **Reversibility** two-way door (low to medium). **Owner action** no.

**Decision.** One binary with roles (`http`, `cron`, `bus`, `ext`, `hub`, `all`), static musl with embedded frontend assets, PostgreSQL behind PgBouncer 1.26+ (direct connections for migrations, LISTEN and session locks), blobs behind an `object_store` trait on S3-compatible storage (not MinIO), in-process cache, pgBackRest for PITR. Solo and demo installs use embedded PostgreSQL. SQLite is only the fixed-schema edge store `obop-edge` (at most 15 tables, `PRAGMA user_version`), never a projection of the server schema. The store hub is the same binary on rusqlite; POS shells are Tauri 2 plus a Rust hardware agent, with a PWA baseline; browser-only mode uses sqlite-wasm. No Node on customer servers.

**Why.** One binary plus PostgreSQL is the operations differentiator; two SQL dialects would double the domain layer. **Rejected.** Dual-target SQL; Electric, PowerSync, Zero; Redis 8. **Consequences.** The NOTIFY lock stays bounded by cell size. **Evidence.** R23 DR-7, R22, R24, R12.

#### D38 Docs, disclosure and legal pack
**Status** BINDING. **Reversibility** two-way door. **Owner action** no.

**Decision.** Docs are plain Markdown on VitePress 2 inside Vite+ (pinned; Starlight is a fallback only with an explicit owner waiver of the Vite+ rule (F2), as for Astro in D14, because it is an Astro framework with its own CLI that `vp` does not drive; the waiver is not assumed, Q14), with reference pages generated from the registry. Security advisories reach everyone at once (no customer-tiered embargo). Full data export ships in every edition, and the hosted contract carries Data Act switching terms (2 months' notice, 30-day transition, 30-day retrieval, erasure, no switching charges from 2027-01-12). Legal-compliance warranties are disclaimed by default; every tax or payroll computation stores the rule-pack version and inputs. Default storefront, checkout, portal and POS are WCAG 2.2 AA and ship the consumer withdrawal function.

**Why.** Odoo's Enterprise-first disclosure and paid upgrades are grievances; the Data Act makes export a duty. **Rejected.** Tiered embargoes; capped statutory carve-outs. **Consequences.** VitePress 2 is still alpha, so content stays plain Markdown. **Evidence.** R28 s2-6.

#### D39 Collaboration and messaging services
**Status** BINDING. **Reversibility** two-way door. **Owner action** no.

**Decision.** Chatter is first-party and record-bound. The email gateway is built on Stalwart's permissive crates (`mail-parser`, `mail-builder`, `mail-auth`), `lettre` and `async-imap`, never the AGPL Stalwart server. Mailboxes are OAuth-only (Gmail API, Microsoft Graph): no EWS, no basic SMTP AUTH. Collaborative documents use Yjs 13.6 with `yrs` 0.28, pinned together. E-signature is honest SES with QTSP connectors for qualified signatures. One generic approvals engine. No copyleft-only components in core (HyperFormula, BlockNote XL, Documenso).

**Why.** Microsoft begins default-disabling EWS in Exchange Online tenants from 2026-10-01 (rolling, admin-reversible) and turns it off permanently on 2027-04-01; it disables basic SMTP AUTH by default for existing tenants at the end of December 2026. **Rejected.** LISTEN/NOTIFY bus; self-labelled "advanced" signatures. **Consequences.** A CI licence gate; a cross-implementation Yjs/yrs test. **Evidence.** R15, R21.

### 2.3 Deferred items

| Item | Trigger |
|---|---|
| Pooled Free tier (D6) | Spike fails, Free cost over 10% of COGS, or conversion under 2% |
| Reference payroll country (D19) | 2029-04-01 |
| JSON-2 compatible endpoint (D22) | Three design partners ask, or fall 2028 |
| Field work as task subtype or planning shift | Before `planning` design starts (2028-07-01) |
| Rhai inside signed packs (D11) | Measured demand in W3 |
| MessageFormat 2 review (D34) | 2027-H2, if Lingui and a Rust implementation ship it |
| NATS JetStream (D7) | Measured: bus-database wake-up lag or polling-fallback rate above the threshold set at the D7 pgbench spike readout (2026-11-27), at the reference cell size. Node count alone is not a trigger: a cell always has several app nodes |
| Own Peppol access point (D17) | About 250 businesses or M31, on measured volume |
| Marketplace take rate (D2) | A year of verified paid-listing actuals |

## 3. Contradiction rulings

| # | Topic | Ruling | Authority | Status |
|---|---|---|---|---|
| 1 | NOTIFY | Forbidden in business transactions; outbox, in-process wake-up, coalescing notifier on one bus database | R23 DR-1, D7 | BINDING |
| 2 | Tenancy | Database per business only; pool exit prepared; Free tier stays unless D6 triggers | R23 DR-2, D6 | BINDING; Free-tier default PROPOSED, pooled Free tier DEFERRED |
| 3 | PostgreSQL floor | 17 and 18; `requires_pg = 18` for 18-only features | R23 DR-3, D8 | BINDING |
| 4 | Authorization | Odoo-20 algebra, default-deny | R23 DR-4, D10 | BINDING |
| 5 | Scripting in data | CEL only for conditions, modifiers and formulas; render-time templates are a bounded category (D11); no Rhai in W1-W2; cost-limit spike | R24 s3.2, D11 | BINDING |
| 6 | Modifier storage | Domains JSON AST; the rest CEL source plus compiled form | D12 | PROPOSED |
| 7 | Wasmtime LTS | v36 and v48 are LTS; pin v48; next v60 about 2027-08-20; R21's claim is wrong | R24 s3.3, D11 | BINDING |
| 8 | Frontend extension | F1 only for first-party and business-own code; marketplace is F2 or F3 | R24 s4, D11 | BINDING |
| 9 | Edge datastore | PostgreSQL server; SQLite only as fixed-schema edge store | R23 DR-7, D37 | BINDING |
| 10 | Storefront and Node | No Node in production; build-time only | D14 | PROPOSED |
| 11 | One edition versus paid security | Security outcomes open; paid is operation and assurance; closed ratcheting list | R26 A2, R25 B7, D2 | PROPOSED |
| 12 | CLA | Licence grant, OSI-forever pledge, steward veto, investor covenant before a priced round | R26, R17, D1 | PROPOSED |
| 13 | Pricing unit | Per organisation banded by full users; price-test hypotheses | R26 A3, D2 | PROPOSED |
| 14 | Deprecation window | At least 24 months, removal only at the next `N.0` after that | D16 | PROPOSED |
| 15 | Timeline | One calendar; pack stable, paid cloud and 1.0 LTS separated | D26, section 5 | PROPOSED |
| 16 | First country | Germany, then France, then Poland; conditions in D4 | R25, D4 | PROPOSED |
| 17 | MVP breadth | R20 s9.3 cut lines govern W1; R23 graph is the dependency law | D25 | PROPOSED |
| 18 | Odoo 20 Field Service | Merged into Planning (R14); task subtype versus shift stays open | R14, section 2.3 | DEFERRED |
| 19 | Odoo 20 release and MCP | "September 2026 (release commit 2026-09-17)"; `/mcp` with API-key auth, server actions as tools | R16, R21 | BINDING |
| 20 | Odoo offline | Odoo 20 queues writes; obop 1.0 targets Odoo-19 parity for the apps shipped by then; POS capture ships with `pos` in W3 | D13 | PROPOSED |
| 21 | Odoo price presentation | 19.90 and 35.90 are 12-month introductory yearly prices; list 24.90 and 44.90; monthly list 31.10 and 56.90; Light User 7.90 (unit unverified) | R26 A1 | BINDING |
| 22 | PDF engine | Typst only in W1-W2; isolated worker if a browser is ever added | D20 | PROPOSED |
| 23 | Payroll reference country | Decide 2029-04-01 | D19 | DEFERRED |
| 24 | Performance messaging | No multiple before a harness exists | D21 | BINDING |
| 25 | Rust adoption speed | R27's x.y.1 or x.y.0 plus 14 days beats R06's "move to 1.99 immediately" | D23 | PROPOSED (I-3, Q10; applied provisionally) |
| 26 | Exit drill cadence | Nightly automated job plus quarterly human review (R07 said quarterly); runs outside `vp` on a throwaway copy, so it needs the named exception | R27 B9, D15 | PROPOSED (I-4, Q10) |
| 27 | "Paid cloud at M9" | Paid beta opens 2027-07-06 (M9); SLA plan from 2027-10-01 | R26, R20, D26 | PROPOSED |
| 28 | "T" labels | R01, R23, R24, R25, R26 each use T0-T13 differently; see section 4 | D24 | BINDING; the T0-T3 meaning is PROPOSED (D25) |
| 29 | White-label | Partner white-label is paid: trademark licence in the partner programme plus a reseller console in `ee`; business branding is open | R24 s10, R26 A2 | PROPOSED |
| 30 | Repository visibility | Private until G0, then public; at latest at 0.3 public alpha 2027-04-06 (R20 "day one" versus R27); counsel timing against D1 open (Q16) | D26 | PROPOSED |
| 31 | Importers | CSV at 0.3; Odoo and QuickBooks at 0.6; Xero and ERPNext in W2 (R20 s9.3 versus s10) | D26 | PROPOSED |
| 32 | Assets, deferrals, budgets | R01 says T1, R20 says out of the MVP: tier T1, wave W3 | D25 | PROPOSED |
| 33 | Multi-company timing | R26 lists "5 entities" at paid launch, R20 and R23 ship one company: multi-company (selector, per-company books) in W2; plan tables say "entities" only from then | D25, D28 | PROPOSED |
| 34 | Payroll packs open or paid | Rule content is open; the warranted payroll service is paid | R25 B7, R26 | PROPOSED |

## 4. Canonical vocabulary

| Term | Meaning | Avoid |
|---|---|---|
| obop | Product and project; lower case | OBOP, Obop |
| business | Isolation and deployment unit: one PostgreSQL database, module set, user directory, object-store prefix, backup unit | "tenant", "instance", "workspace" in the data plane |
| tenant | Commercial and billing principal in the control plane; owns one or more businesses; absent from business databases | In schemas, APIs or data-plane code. Proposed renames: `TenantTx` to `BusinessTx`, `TenantRouter` to `BusinessRouter`, `tenant_release_state` to `business_release_state` |
| organisation (org) | Customer-facing name and billing unit for one business | For a legal person (use "legal entity") |
| legal entity | A juridical person, stored as an organisation `party` | "entity" alone |
| company | Ledger-owning scope of one legal entity: books, functional currency, fiscal calendar, journals, sequences, lock dates | For a counterparty or a branch |
| branch | Subdivision of a company without own books | "subsidiary" (that is a company) |
| company group | Holding and subsidiaries, modelled as `ownership_interest` edges | "group" alone |
| permission group | Named bundle of permission rows (Odoo `res.groups`) | "group" alone; "role" outside UI copy |
| permission row, restriction row | Rows of the D10 algebra | "ACL", "record rule", "sudo" |
| party | Person or organisation in the data model | "partner" in data (Odoo's `res.partner`) |
| partner | Channel, implementation, hosting or localisation partner (commercial) | For a contact or customer |
| principal | User, portal user, guest, API client, agent or system | "bot" for agents |
| cell | One PostgreSQL cluster of at most 250 business databases, with PgBouncer and app nodes | |
| control plane, data plane | Fleet, billing and releases (`ee`), versus business databases and app nodes | |
| module | First-party unit with manifest, `*-api` and implementation crates, installed per business | "addon", "plugin" |
| app | A module shown as a top-level menu entry | For a package |
| package | Distribution unit in the package registry (`obop.toml`); kinds pack, wasm, remote, native, theme | |
| extension | Any third-party or business-own package | "plugin" |
| remote app | Package of kind `remote` (code tier X4) | "app" alone |
| pack | Package of kind `pack` (declarative data, optionally plus code); a localisation pack is a country pack | For a paid service: say "Assurance service" or "payroll service" |
| registry | The runtime model registry (`Arc<Registry>`) | For the package registry (say "package registry"), crates.io or the npm registry |
| port | A Rust trait in a `*-api` crate implemented by its owner; a "hub port" (`DemandPort`) is a convergence port | A network port, unqualified |
| TxEvent, durable event | In-transaction event; outbox-backed event | "event" alone in architecture text |
| outbox | `outbox_event` plus `outbox_delivery` rows | The edge `command_outbox` (say "edge command outbox") |
| bus database | The one database per cluster hosting the coalescing notifier | "bus" alone |
| edge store | SQLite store of `obop-edge` | A replica of the server schema |
| store hub | The `hub` role serving a shop's tablets | "hub" alone |
| open edition, `ee` | Everything AGPL; the commercial code tree | "Community", "Enterprise", "Studio" (Odoo words) |
| X1-X5 (Proposed) | Code tiers: pack, CEL, Wasm, remote, native | T1-T5 (R24) |
| F0-F3 | UI extension tiers; write "UI tier F2", never bare, because F1-F4 are the owner constraints | Bare "F2" |
| L1-L3 (Proposed) | Localisation pack maintenance tiers: core team, paid partner, community | T1-T3 (R25) |
| T0-T3 (Proposed, D25) | Module tiers (section 5) | R23's "T0 (21) and T1 (19)" sets; R26 risk ids read "risk T7" |
| W0-W4 (Proposed, D26) | Release waves (section 5) | "Phase", "wave" for country order, bare "GA" |
| pack stable, 1.0 LTS | Pack gate "stable"; the first LTS series | "DE GA" |
| series, LTS | Release `N.M`; the `N.0` series with five years of support | |
| operator, implementer, publisher, host, steward, manufacturer | Runs an installation; configures it; authors a package; resells hosting; non-profit owner of trademark; company with CRA duties | "Foundation" for the steward (except inside quoted research) |

**Identifier namespaces.** Each prefix has one meaning in this register. Where a companion document or research note uses the same letters for something else, write the qualifier every time ("UI tier F2", "constraint F2", "R26 risk M9", "month M9", "integration rule R5", "research note R05"). Companion documents may define their own families (gates, hiring gates, schedule risks, spikes, goals, open questions, counsel items); those are read as defined there and never cited in this register without the document name.

| Family | Meaning in this register | Defined in |
|---|---|---|
| D1-D39 | Decision records | Section 2 |
| Q1-Q20 | Owner questions | Section 8 |
| F1-F4 | FIXED owner constraints | Section 1 |
| I-1 to I-4 | Proposed interpretations of F1 and F2 | Section 1 |
| R00-R28 | Research notes (two digits; "R23 DR-1" is decision record 1 of note 23) | Section 9 |
| R1-R7 | Integration rules (one digit) | D27 |
| X1-X5, F0-F3, L1-L3 | Code tiers, UI tiers, pack maintenance tiers | This section |
| T0-T3, W0-W4 | Module tiers, release waves | Section 5 |
| G0 | Public-repository gate | D26 |
| M0-M36 | Months counted from 2026-10-01 (never R26 risk ids such as M7 or M9) | Section 0 |
| P0-P2 | Security priority classes of R18 | D33 |
| [V], [I], [E] | Research evidence marks: verified, inference, estimate | R00 |

**Style.** British spelling in prose (licence, localisation, organisation, behaviour); US spelling only in identifiers, file names and quoted sources (`LOCALIZATION.md`, `l10n`). One document status label across the set ("Draft 0.1") until the owner ratifies the register. A decision's status (FIXED, BINDING, PROPOSED, DEFERRED) lives in section 2.1; other documents write "Proposed (Dn)" only for the record a sentence relies on. The non-profit is the "steward" throughout.

## 5. Module tiers and release waves

### 5.1 Tiers

T0 kernel: platform, present in every business. T1 must-have: the product is not a business OS without it. T2 second wave: large segments needing vertical depth or compliance. T3 long tail: niche, or best delivered through the extension API. Tier is R01's meaning; wave is when it ships. Any module not listed defaults to T3.

| Tier | Wave | Modules | Note |
|---|---|---|---|
| T0 | W0 | Kernel crates `obop-types`, `-registry`, `-orm`, `-security`, `-events`, `-jobs`, `-i18n`, `-storage`, `-http`, `-bus-notifier`, `-testkit`; module `base` (the Contacts app: party, principal, company, currency, sequences, external ids, audit, attachments, properties, import/export) | Nothing ships before this |
| T0 | W1 | `product`, `mail` (chatter, activities, followers, aliases, email gateway, templates), `portal`, `analytic`, `automation` (rules, webhooks), `pricing`, `analytics` (semantic layer), `obop-report`; `customize` custom fields (admin screen, overlay packs, export as a package); registry-generated REST, OpenAPI, MCP; form, list, kanban, pivot, graph, calendar views; search | R23 foundations and engines, less `account` and `tax`. `customize` is the no-code customisation app (Proposed T0; R01 had it as T2, and R20 keeps declarative custom fields in the first delivery but visual editors out, so only fields are W1) |
| T0 | W2 | Approvals engine (inside `automation`), `obop-wasm-host`, `obop-ext-sdk`, `obop-compat`, package registry service; `resource` (working calendars, leaves, public holidays); `customize` custom models and layout editor | Approvals is an engine, not an app. `resource` and the `customize` additions are Proposed; project, timesheet, helpdesk, planning and time off need `resource` |
| T0 | W3 | `obop-edge`; gantt, map and cohort views; `customize` form builder and print-template designer | `obop-edge` with POS; gantt follows `project` |
| T1 | W1 | `account` (GL, AR/AP, period locks, hash chain, reconciliation), `tax`, `invoicing`, `payment` (manual, SEPA, Stripe plus one), `bank` (camt.053 and MT940 import), `edi` (EN 16931 UBL/CII, Factur-X, XRechnung, Peppol via partner access point), `l10n` framework and `l10n_de`, `crm`, `sales`, `purchase`, `stock` (one warehouse), `sale_stock`, `purchase_stock`, `stock_account` | R20's seven-app loop |
| T1 | W2 | `project`, `timesheet`, `sale_project`, `sale_timesheet` (W1 only if the loop is on schedule at 0.5), `hr` employee core (employee, version, department, job, calendar; Proposed), `calendar`, `discuss` channels, multi-company (selector, per-company books), stock lots, serials and routes, `l10n_fr`, aggregator bank feeds | Services variant. `timesheet` needs an employee record and `resource` calendars, so the employee core ships ahead of the rest of `hr` (otherwise `timesheet` moves to W3) |
| T1 | W3 | `hr` (employees beyond the W2 core, time off), `expenses`, `documents`, `delivery` (`CarrierProvider` trait), `l10n_pl`, fixed assets, deferrals, budgets, to-do | |
| T2 | W3 | `mrp` (with Shop Floor, quality, maintenance), `pos` (retail, DE TSE via cloud signer), `website` (with its block page builder), `ecommerce` (with `loyalty`), `helpdesk`, `planning` (with field work), `subscriptions`, `payroll` engine, `recruitment`, `attendances`, `sign`, `knowledge`, `appointments`, email and SMS marketing, `livechat`, dashboards, `ai` agents, AT and CH packs, `intercompany` | Demand order; `mrp` and `pos` first (after the 1.0 LTS, so D13's POS capture ships here); each needs a design partner. The CH pack is unresearched, so its wave is open (LOCALIZATION 7.3, question 19) |
| T2 | W4 | `consolidation`, barcode and WMS depth, payroll reference pack, BE/NL/ES/IT/Nordic packs (L2 partners), WhatsApp, Shopify and WooCommerce connectors as X3 or X4 packages | Other marketplace connectors are T3 |
| T3 | W4+ or community | Rental, PLM, IoT and VoIP, social and automation marketing, eLearning, forum and blog, events, surveys, fleet, lunch, frontdesk, referrals, appraisals, ESG, equity, marketing card, Amazon/Shopee/Lazada/TikTok connectors, Gelato, repairs, industry bundles, an approvals app | Extension API and community |

### 5.2 Waves and calendar (Proposed, D26)

M0 is 2026-10-01. Only the German dates have a hard external driver (2028-01-01 all-issuer mandate); if M0 slips, compress W1 rather than move 2027-09-30. Fixed external anchors: CEN code lists 2026-11-16, 2027-05-17 and 2027-11-15; Product Liability Directive 2026-12-09; Data Act switching-charge ban 2027-01-12; Microsoft EWS off 2027-04-01; Veri*Factu others 2027-07-01; France SMEs 2027-09-01; CRA main obligations 2027-12-11; German all issuers 2028-01-01.

| Wave | Name | Window | What ships | Exit criteria |
|---|---|---|---|---|
| W0 | Foundations | 2026-10-01 to 2026-12-31 | CI baseline; T0 kernel; three spikes read out by 2026-11-27; migration ledger starts; 0.1 preview 2026-12-01 (no stability promise); security policy and CRA runbook v0 | G0 passed, or dated no later than 2027-04-06 if Q16 leaves counsel coverage open (D26); D6, D7, D9 frozen; pins per D23; 3-5 design partners signed; counsel retained; DATEV and SKR confirmation requested |
| W1 | Loop | 2027-01-01 to 2027-09-30 | 0.2 (02-02); DE pack alpha 03-31; 0.3 public alpha 04-06 (N-1 guarantee, `obop upgrade --dry-run`); 0.4 05-04; DE beta 06-30; 0.5 public beta and paid beta plans 07-06; 0.6 09-07; X1, X2, X4; CLI v0; Peppol via partner; MCP; CSV then Odoo and QuickBooks importers | DE pack stable 2027-09-30 (oracles, golden cases, accountant sign-off); at least 10 paying and 50 production organisations; CRA runbook drilled; ledger proptests green; N-1 and N-2 chain tests; no known trial-balance bug |
| W2 | Release | 2027-10-01 to 2028-06-30 | SLA plan and Assurance DE service 10-01; 0.7 11-02; CRA technical file 11-30 and sign-off by 2027-12-10; cut `release/1.0` 2028-03-07, RC 03-21 and 04-04; **1.0 LTS 2028-04-18**, 1.0.1 05-02; FR pack stable 03-31; PL start 2027-10, KSeF beta 2028-05; X3 host, F2, registry alpha; accountant and certified-partner programmes; SOC 2 Type I; multi-company; services variant | 1.0 LTS signed with SBOM and attestation; CRA sign-off; FR stable; SOC 2 Type I; at least 40 paying organisations (geometric midpoint of R20's 10 at M12 and 150 at M24) |
| W3 | Breadth | 2028-07-01 to 2029-06-30 | PL pack stable 2028-08-31; 1.1 2028-10-17; 1.2 2029-04-17; T2 apps in demand order (`mrp`, `pos` first); F3, custom domains, partner white-label; payroll engine; own Peppol access point if volume supports it (about M31); SOC 2 Type II and ISO 2029-04 | ARR at least EUR 1M (base about M29); at least two T2 apps stable; M24 check (2028-10-01): 150 paying organisations targeted, under 60 triggers the D5 cut; hiring above 17 FTE only after the ARR gate |
| W4 | Ecosystem | 2029-07-01 onward | 1.3 2029-10-16; 2.0 LTS 2030-04-16; marketplace commerce; verticals; public-sector track once the EU strategy is law | Governed by the LTS cadence, not an exit date |

Alignment rules. Paid beta plans open at M9 (R26's revenue start, R20's beta). The SLA plan waits for the DE pack to be stable. 1.0 LTS falls after the CRA date and is fully subject to it; CRA duties apply from the first commercial offer (2027-07-06), so the Art. 14 runbook is drilled by 2027-06-01. The hosted contract carries the Data Act switching terms before the first paid plan. Mail integration uses OAuth and Graph only.

## 6. Open/paid boundary

**Free forever (publish before the first public commit).**

- All first-party apps: accounting, inventory, manufacturing, CRM, HR, payroll engine, POS, helpdesk, documents.
- REST, webhooks, MCP, OpenAPI, importers and full-fidelity export on every plan, with no switching or egress charges (Data Act, 2027-01-12).
- Multi-company, custom fields and models, Wasm extensions, the package registry client and sideloading.
- The code of every operated service below: the package registry server, the mail OAuth broker, bank-feed, SMS and sending-relay adapters, and routing and tile adapters, with bring-your-own credentials (Proposed addition; Q19).
- Authentication (password, TOTP, passkeys, OIDC, LDAP bind), RBAC, record rules and field permissions; the hash-chained audit log with UI and export (13-month default); envelope encryption with local or OpenBao keys; the backup and PITR recipe; a single-business install.
- BYOK AI, MCP server, usage ledger and budget caps.
- Security fixes, SBOMs, signed releases and CRA reporting.
- Law-defined pack content: charts, taxes, fiscal positions, VAT-return and audit-file exports, e-invoice syntaxes, validators.
- Business branding, email and PDF templates, portal and website themes, portal custom domain; community support.

**Paid (closed list).**

- Native SAML (multi-IdP, JIT); full SCIM (Bulk, advanced filters, group-to-role policies).
- WORM or object-lock checkpoint publication, SIEM streaming, legal hold, multi-year audit tiering, auditor evidence packs.
- Cloud-KMS and HSM connectors, customer-managed keys, region pinning (hosted Scale).
- Fleet DR and cross-region failover; the multi-business control plane (`ee`, partner-hosting licence); the supported air-gap bundle.
- Partner white-label (trademark licence plus reseller console).
- Hosted AI credits; hosting; support and SLAs; the per-country Assurance service (legal-change SLA, filing connectors, capped warranty); the payroll service; certification exam fees; hosted gateways (Peppol, KSeF, Veri*Factu).
- Operated services over open code (Proposed addition; Q19): the hosted package registry, the hosted OAuth broker for mailbox providers such as Gmail, the hosted bank-feed broker, hosted sending relay, managed IP reputation and SMS credits, and hosted routing and tiles. "Filing connectors" in the Assurance service means an operated service over open code, never closed code.

**Guard rails.** Never gate security fixes, data export, the API, multi-company, ledger integrity or the existence of an audit log. Sell response time, never availability of fixes (CRA Annex I Part II(8)). Every security outcome is reachable in the open edition.

**Ratchet and conversion.** The list is published, closed, and moves only from paid to open. `ee` code converts to open 24 months after release. The licence-grant CLA carries the OSI-forever pledge; the steward can veto any move off OSI licences; an investor covenant precedes any priced round. A hosting competitor is answered with trademark policy, partners and service levels, never a relicence. No Odoo-style contract levers (7% uplift, 25% old-version fee, 300% breach fee, per-100-line fees, usage beacon).

## 7. Carry-forward corrections

Do not repeat these superseded statements.

- **NOTIFY.** R02 SHOULD 14, R06 SHOULD 11, R08, R15 MUST 1, R18, R26 T7 ("post-commit notify") plan LISTEN/NOTIFY as the bus or queue wake-up: use D7.
- **Wasmtime.** R21's "no LTS policy stated": v36 and v48 are LTS lines (24-month support).
- **Rhai.** R05, R15 s11, R06 COULD 23 propose Rhai: CEL only (D11).
- **Odoo offline.** "Read-only cache with 2 GB cap" (R02, R12, R19, R22 first drafts): Odoo 20 queues writes.
- **Field Service.** R01 lists a separate Enterprise app: Odoo 20 merged it into Planning.
- **Prices.** 19.90 and 35.90 are 12-month introductory prices, not plain list prices.
- **Database.** R08 "database per tenant with RLS backstop", R18 "silo and pool first-class", R26 "190k relations": one database per business, about 190k tables per cell.
- **PostgreSQL.** R02 "16+", R18 "16/17/18", R08 and R22 "18 minimum or baseline": 17 and 18.
- **Authorization.** R08, R18 default-allow `ir.rule` with global AND and group OR: Odoo-20 algebra.
- **Edge.** R12 "SQLite everywhere", R18 "Tauri local mode with SQLite": PostgreSQL server, SQLite only as edge store, embedded PostgreSQL for solo.
- **Frontend.** R07 s3.12 and MUST 6 load third-party ESM into the main realm: F1 is trusted code only.
- **PDF.** R15 Chromium pool, R06 SHOULD 9 chromiumoxide: Typst only.
- **Enterprise tier.** R17 s11.2 and SHOULD 9 gate SSO, SCIM, audit, control plane, air-gap, white-label as written: section 6.
- **CLA and pricing.** R05 "avoid a relicensing CLA" and R03 SHOULD 7 "one per-user price": D1 and D2.
- **Deprecation.** R26 T4 and R27 C4 "12 months": 24-month floor.
- **Rust.** R06 MUST 1 "move to 1.99 immediately": D23.
- **Rust channel.** "1.99.0 is not yet on the channel", "its manifest returned 404", "1.98.1 as served stable" (research and documents written before 12:38 UTC on 2026-10-01): the stable channel is 1.99.0 (2026-10-01) and 1.98.1 (2026-09-03) was the previous stable. The pin stays 1.98.1 only because of the D23 waiting rule.
- **pnpm versions.** A bare 11.28.2, 11.28.3 or 12.8.1 as "the" pnpm version (R00, R07, ENGINEERING): each belongs to a dist-tag (`latest-11`, `next-11`, `latest`); name the tag, and pin an exact version with `--target package-manager` (section 1.1; Q15).
- **`vp pm`.** "`vp pm` forwards raw package-manager commands", `vp pm sbom` as an unverified forward, `vp pm audit signatures` (R07, R24, R27, ENGINEERING, security-privacy): `vp pm` is a typed command set; `vp pm sbom` and `vp pm licenses` do not exist in vite-plus 1.0.0; write `vp pm audit -- signatures` (I-2; the "`vp` only" ruling is Q10).
- **EWS.** "Microsoft turns EWS off on 2027-04-01" without the start: default-disabling begins on 2026-10-01, tenant by tenant and admin-reversible, and EWS is permanently off from 2027-04-01 (D39). On-premises Exchange Server keeps EWS.
- **Names.** The engine is the "approvals engine" (D39), and the approval policy engine of D18 is its policy evaluator; the carrier provider trait is written `CarrierProvider`; event, permission-group, data-class, state and port spellings follow [CONVENTIONS.md](CONVENTIONS.md).
- **Labels.** R24 T1-T5, R25 T1-T3, R26 T1-T13, R23 "T0 (21) and T1 (19)", R20 "Phase": section 4.
- **Manifests.** R02 `module.toml` and R21 `obop.app.toml`: one `obop.toml`.
- **Country order.** R09 "DE, BE, FR, US, NL first": D4.
- **Sidecars.** R10 Karrio sidecar and R15 OCR sidecar are optional, never default (I-1).
- **Odoo dates.** Write "September 2026 (release commit 2026-09-17)"; 19.0 is "September 2025".

## 8. Open questions for the owner

Q1-Q14 are ranked by irreversibility; Q15 onward were added after that ranking and are listed in order of addition. Dates are the latest useful answer. Ratified so far: none. This section is the owner's single list: questions that live in companion documents are folded in below or listed after the table.

| Rank | Question | Door | Recommended default | Answer by |
|---|---|---|---|---|
| Q1 | Licence suffix, CLA grant and pledge, company plus steward (D1) | One-way | AGPL-3.0-or-later, s7 exception, licence-grant CLA with pledge, counsel instructed | 2026-10-20 |
| Q2 | Keep "obop"; when may the repository go public (D3, G0) | One-way | Working name; reserve handles by 2026-10-15; counsel opinion by 2026-12-15; else a fallback name | 2026-10-20 |
| Q3 | Ratify the free and paid lists and the ratchet, including the operated-service lines (D2, section 6, Q19) | One-way | Ratify as written | 2026-10-20 |
| Q4 | Funding path, hiring gate, covenant, NLnet application, and the bridge between the seed and the ARR-gated round (D5; BUSINESS BQ12) | One-way | Gated plan; apply to NLnet; bridge per BQ12 | 2026-10-23 |
| Q5 | Steward legal form and CRA roles (D1, D17) | One-way | Counsel opinion before the paid beta; the part that gates the public flip is Q16 | 2027-06-01 |
| Q6 | Spike readout: Free-tier economics and key type (D6, D9) | One-way once data exists | Keep database per business; UUIDv7 if thresholds pass | 2026-11-27 |
| Q7 | Release, LTS and 24-month deprecation commitments, with months counted as release months (D16; ENGINEERING Q-E3; ROADMAP 9.3 items 4 and 9, the numbering of a slipped 1.0) | One-way once public | Ratify; `N.0` removals only; count release months | 2027-03-31 |
| Q8 | Germany first, ICP, T0 app list (including `customize` and `resource`), calendar (D4, D25, D26) | Costly | Confirm all | 2026-11-13 |
| Q9 | Expression storage (D12) | Costly | Domains JSON AST, CEL for the rest | 2026-12-31 |
| Q10 | Interpretations I-1 to I-4 (I-2 includes whether `vp env exec pnpm sbom` or a `vp run` task around it counts as "`vp` only"; I-4 is the exit-drill exception; ENGINEERING Q-E2), the D23 pin policy, the D24 renames and X and L prefixes, and the Rust 1.99 adoption window | Two-way | Ratify; adopt 1.99.x in 2026-10-15 to 2026-10-22 | 2026-10-15 |
| Q11 | Offline scope at 1.0 and the wave of POS order capture (D13, D25) | Two-way | Odoo-19 parity for the apps shipped by 1.0: read cache plus degraded-feature list; POS capture with `pos` in W3 | 2027-09-30 |
| Q12 | Typst only, no browser renderer (D20) | Two-way | Typst only | 2027-09-30 |
| Q13 | Marketplace terms and white-label licence (D2) | Two-way | 0% at launch, cap 15% | 2028-03-31 |
| Q14 | Storefront without Node (D14), and whether Starlight (Astro) may be the docs fallback in D38, which needs the same F2 waiver | Two-way | No Node in production; no Astro without a waiver, and no Starlight fallback without one (VitePress stays the docs generator) | 2028-03-31 |
| Q15 | Which pnpm major to pin, 11.x or 12.x (D23; ENGINEERING Q-E6) | Two-way | Newest 11.x that is at least 7 days old (11.27.1 today; 12.6.0 if the owner picks 12.x) until the settings are behaviourally verified on 12.x (a parse check passed on 12.8.1) | 2026-10-15 |
| Q16 | What "counsel covers" means for the public flip, and when Q5 is answered (D1, D26, Q5) | One-way | Written counsel advice on all nine R17 s12 items, due by 2027-03-15; formed entities, E&O cover and signed CRA roles gate the paid beta (Q5), not the flip | 2026-12-15 |
| Q17 | Runner plan while the repository is private (D36, D26; ENGINEERING Q-E1) | Two-way | One Hetzner CCX43 for Rust builds plus standard runners, no merge queue (ENGINEERING proposal) | 2026-10-23 |
| Q18 | Identifier families that share letters (section 4): keep and qualify, or rename (UI tiers, integration rules, spikes, goals) | Two-way | Keep and qualify now; rename only on the owner's request, before the first public commit | 2026-11-13 |
| Q19 | Operated services on the paid list, and later additions to the closed list (D2, section 6; BUSINESS BQ2, BQ11) | One-way | Open code with paid operation for the registry server, brokers, relays, SMS and routing; new capabilities join the paid list only by a published amendment with steward-board consent and 12 months' notice, for capabilities never shipped open | 2026-10-20 |
| Q20 | Where the company is incorporated, which decides the CRA manufacturer role and the dogfood ledger (ROADMAP section 9.3) | Costly | No default in the research; ask counsel with Q5 | 2026-10-23 |

**Companion lists.** Questions that stay in their own documents are two-way details owned there: [BUSINESS.md](BUSINESS.md) BQ1 and BQ3-BQ10, [ENGINEERING.md](ENGINEERING.md) Q-E4, Q-E5 and Q-E7 onward, [ROADMAP.md](ROADMAP.md) section 9.3 items 1, 6, 7 and 8, [PRODUCT.md](PRODUCT.md) section 14.3 and the open questions at the end of each spec. Folded in above: Q-E1 (Q17), Q-E2 (Q10), Q-E3 (Q7), Q-E6 (Q15), BQ2 and BQ11 (Q19), BQ12 and ROADMAP 9.3 item 3 (Q4), ROADMAP 9.3 item 2 (Q20), items 4 and 9 (Q7), item 5 and BQ13 (Q16). Time-only invoicing stays in [ARCHITECTURE.md](ARCHITECTURE.md) section 16.3 (item 7) and is decided with Q8. [CONVENTIONS.md](CONVENTIONS.md) section 9.1 collects the owner decisions that affect names; each points back to Q2, Q8, Q10, Q11 or Q18, to D27 above, or to an open question of its own spec.

## 9. Sources

All paths are relative to this file.

| Tag | File |
|---|---|
| R00 | [research/README.md](research/README.md) |
| R01 | [research/01-odoo-app-catalog.md](research/01-odoo-app-catalog.md) |
| R02 | [research/02-odoo-architecture.md](research/02-odoo-architecture.md) |
| R03 | [research/03-odoo-business-model.md](research/03-odoo-business-model.md) |
| R04 | [research/04-odoo-voice-of-customer.md](research/04-odoo-voice-of-customer.md) |
| R05 | [research/05-competitive-landscape.md](research/05-competitive-landscape.md) |
| R06 | [research/06-rust-stack.md](research/06-rust-stack.md) |
| R07 | [research/07-vite-plus-frontend.md](research/07-vite-plus-frontend.md) |
| R08 | [research/08-metadata-platforms-dynamic-orm.md](research/08-metadata-platforms-dynamic-orm.md) |
| R09 | [research/09-accounting-finance-compliance.md](research/09-accounting-finance-compliance.md) |
| R10 | [research/10-supply-chain-manufacturing.md](research/10-supply-chain-manufacturing.md) |
| R11 | [research/11-crm-sales-subscriptions.md](research/11-crm-sales-subscriptions.md) |
| R12 | [research/12-retail-pos-ecommerce-marketing.md](research/12-retail-pos-ecommerce-marketing.md) |
| R13 | [research/13-hr-payroll.md](research/13-hr-payroll.md) |
| R14 | [research/14-services-project-helpdesk-field.md](research/14-services-project-helpdesk-field.md) |
| R15 | [research/15-platform-collaboration-documents.md](research/15-platform-collaboration-documents.md) |
| R16 | [research/16-ai-native-erp.md](research/16-ai-native-erp.md) |
| R17 | [research/17-oss-licensing-business-models.md](research/17-oss-licensing-business-models.md) |
| R18 | [research/18-security-compliance-deployment-ops.md](research/18-security-compliance-deployment-ops.md) |
| R19 | [research/19-ux-design-system.md](research/19-ux-design-system.md) |
| R20 | [research/20-market-personas-mvp-gtm.md](research/20-market-personas-mvp-gtm.md) |
| R21 | [research/21-integrations-interoperability-api.md](research/21-integrations-interoperability-api.md) |
| R22 | [research/22-data-layer-sync-realtime-performance.md](research/22-data-layer-sync-realtime-performance.md) |
| R23 | [research/23-kernel-model-architecture-decisions.md](research/23-kernel-model-architecture-decisions.md) |
| R24 | [research/24-extension-platform-sdk.md](research/24-extension-platform-sdk.md) |
| R25 | [research/25-localization-operating-model.md](research/25-localization-operating-model.md) |
| R26 | [research/26-business-model-pricing-risk.md](research/26-business-model-pricing-risk.md) |
| R27 | [research/27-quality-release-engineering.md](research/27-quality-release-engineering.md) |
| R28 | [research/28-docs-community-support-brand.md](research/28-docs-community-support-brand.md) |
| Names | [CONVENTIONS.md](CONVENTIONS.md) (spelling of cross-document names; subordinate to this register) |
