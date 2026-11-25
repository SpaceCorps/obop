# Rust ecosystem for obop: state of play on 2026-10-01

## Scope & method

Research date is 2026-10-01. Every version and date below was pulled live today from primary sources: the Rust dist manifests (`static.rust-lang.org`), `RELEASES.md` on rust-lang/rust master, the crates.io API and sparse index, GitHub release/changelog/README/Cargo.toml files via `gh api`, the Rust Project Goals repo, the Wasmtime and WASI roadmaps, and the official Rust blog. A few pages were read through a summarising fetch tool, so key figures were re-checked against raw files where possible. There was no general web-search tool in this run, so forum and blog sentiment is largely absent (one official survey is the exception). Nothing was built or benchmarked by me; compile-time guidance is reasoned from project statements, not measurements.

Labels: **[V]** verified today against a primary source; **[I]** inference from verified facts; **[G]** guess or recalled knowledge not re-verified.

---

## 1. Rust toolchain state

### 1.1 Versions and cadence

| Item | Value | Source |
|---|---|---|
| Stable (served by dist channel at check time) | **1.98.1**, published 2026-09-03 (1.98.0 was 2026-08-20) [V] | [channel-rust-stable.toml](https://static.rust-lang.org/dist/channel-rust-stable.toml), [blog index](https://blog.rust-lang.org/) |
| Next stable | **1.99.0**, dated **2026-10-01** in `RELEASES.md`; the `channel-rust-1.99.0.toml` manifest still returned 404 when I checked, so treat as releasing today [V] | [RELEASES.md](https://raw.githubusercontent.com/rust-lang/rust/master/RELEASES.md) |
| Beta / nightly | 1.100.0-beta.1 (commit 2026-09-27) / 1.101.0-nightly (2026-09-30) [V] | [beta](https://static.rust-lang.org/dist/channel-rust-beta.toml), [nightly](https://static.rust-lang.org/dist/channel-rust-nightly.toml) |
| Latest edition | **Rust 2024**, stable since 1.85.0 (2025-02-20) [V]. No announced successor; issues already tag items "Edition 2027" (e.g. [rust-lang/rust#149359](https://github.com/rust-lang/rust/issues/149359)); a 3-year rhythm implies 2027 [I] | [blog index](https://blog.rust-lang.org/) |
| Cadence | 6 weeks. 1.100.0 expected about 2026-11-12 [I] | |
| Recent point releases | 1.96.1 (2026-06-30), 1.97.1 (2026-07-16), 1.98.1 (2026-09-03): **each fixes a miscompilation**. 1.98.1 fixes a vtable bug ([#161441](https://github.com/rust-lang/rust/issues/161441): "vacant vtable slot for a callable boxed async service"); it was a stable-to-stable regression (reporter: works on 1.97.1, segfaults on 1.98.0), so skip 1.98.0 [V] | RELEASES.md, [1.98.1 blog post](https://blog.rust-lang.org/2026/09/03/Rust-1.98.1/) |

### 1.2 Language and Cargo features that matter for a big workspace

| Feature | Stable since | Note for obop |
|---|---|---|
| Workspace lints (`[workspace.lints]`, `[lints] workspace = true`) | 1.74 (2023-11-16) [V] | One lint policy for all module crates |
| `async fn` / return-position `impl Trait` in traits | 1.75 (2023-12-28) [V] | Works, but **no native `dyn` dispatch** and `Send` bounds on the returned future cannot be named on stable (RTN/TAIT and native async-dyn are 2026 goals, see 1.4) |
| MSRV-aware resolver (resolver v3) | 1.84 (2025-01-09) [V] | Default with edition 2024 |
| Async closures, `AsyncFn*` in prelude; Edition 2024 | 1.85 (2025-02-20) [V] | |
| Trait-object upcasting | 1.86 (2025-04-03) [V] | Makes `dyn Module` -> `dyn Any`-style registries cleaner |
| Let chains | 1.88 (2025-06-26), **2024 edition only** [V] | Keep every crate on edition 2024 |
| lld default linker on x86_64-unknown-linux-gnu; multi-package `cargo publish` | 1.90 (2025-09-18) [V] | Free link-time win on Linux CI |
| `build.build-dir` | 1.91 (2025-10-30) [V] | Separate intermediate artifacts from final outputs (cache control) |
| Bundled musl 1.2.5 | 1.93 (2026-01-22) [V] | Static-binary story (see 3.15) |
| `if let` guards; `cfg_select!` | 1.95 (2026-04-16) [V] | |
| `build.warnings` config; v0 symbol mangling default | 1.97 (2026-07-09) [V] | `build.warnings=deny` replaces `-Dwarnings` in CI |
| `debug` profile stub; workspace members may override inherited `default-features` (edition 2024); **incremental compilation off by default when `CI` is set**; LLVM 23 | 1.99 (2026-10-01) [V] | `serde = { workspace = true, default-features = false }` now works |

### 1.3 Build-time levers

| Lever | State on 2026-10-01 | Verdict |
|---|---|---|
| **Parallel front-end** (`-Zthreads=N`) | Still nightly-only. Tracking issue [#113349](https://github.com/rust-lang/rust/issues/113349) open; 2026 goal "Promoting Parallel Front End" is *Accepted*: deadlocks and ICEs mostly fixed, a parallel-incremental race fixed, a dedicated test suite and Cargo integration still pending, stabilisation report not yet filed [V] ([goal](https://github.com/rust-lang/rust-project-goals/blob/main/src/2026/parallel-front-end.md)) | Do not plan on it in 2026; revisit each release |
| **Cranelift backend** | Nightly-only rustup component `rustc-codegen-cranelift-preview` (Linux, macOS, x86_64 Windows) [V] ([README](https://github.com/rust-lang/rustc_codegen_cranelift)). The project's own Feb-2026 numbers: 5% (no debuginfo) to 12% (line tables) faster than LLVM on Zed incremental builds; 10-15% average on single crates; a 2x goal is funded ([goal](https://github.com/rust-lang/rust-project-goals/blob/main/src/2026/improve-cg_clif-performance.md)) [V] | Optional dev-loop toy, not a foundation |
| **Linkers** | lld default on Linux x86_64 since 1.90. **Wild 0.10.0** (2026-08-04): Linux only, incremental linking "not yet implemented" [V] ([README](https://github.com/davidlattimore/wild)). mold 2.42.1 (2026-09-11) [V] | Use default lld; try mold or Wild in Linux CI; Windows devs stay on MSVC/lld-link [I] |
| **Relink-Don't-Rebuild, crate slicing, TPDE backend, shared check/build incremental state** | All 2026 "Fast Builds" roadmap items, not shipped [V] ([roadmap](https://github.com/rust-lang/rust-project-goals/blob/main/src/2026/roadmap-fast-builds.md)) | Watch, do not depend on |
| Incremental-off-in-CI (1.99), `build-dir`, `sccache` 0.18.0, `cargo-hakari` 0.9.39 | Shipped / available [V] | Adopt (see section 4) |

The roadmap itself says CI builds "often take 30+ minutes for medium-sized projects" and that "the frontend remains largely single-threaded" [V]. The 2025 State of Rust survey (7,156 completed responses) again lists slow compiles and storage usage as top productivity problems [V] ([results](https://blog.rust-lang.org/2026/03/02/2025-State-Of-Rust-Survey-results/)).

### 1.4 2026 Project Goals that touch obop

81 goals this year, annual rather than half-yearly ([goals](https://goals.rust-lang.org/2026/goals.html), [highlights](https://goals.rust-lang.org/2026/highlights.html)) [V]. Relevant: stabilise the next-generation trait solver; native `async fn` dynamic dispatch in traits; prepare TAIT + RTN for stabilisation (Help wanted); build-std; stabilise cargo-script; Polonius alpha; const traits and generics; "Wasm Components"; Cargo cross-workspace cache; `cargo-semver-checks` into Cargo; public/private dependencies. **None of the async-dyn items is stable today**, so the `Send`/`dyn` pain remains all of 2026 [I].

### 1.5 Supply-chain events worth a policy

- Rust blog 2026-09-17: an ongoing campaign targets rust-lang members and popular-crate owners via fake video-call lures; `arrayref` was briefly compromised in August [V] ([post source](https://github.com/rust-lang/blog.rust-lang.org/blob/main/content/targeted-attacks.md)).
- 2026-09-21: Miri stores all environment variables in `target/`, leaking secrets through cached CI directories [V] ([post source](https://github.com/rust-lang/blog.rust-lang.org/blob/main/content/github-actions-leaking-secrets-when-miri-output-is-cached.md)).
- Cargo 1.96 (2026-05-28) fixed CVE-2026-5222 (low) and CVE-2026-5223 (medium), both affecting third-party registries only; crates.io users were not affected by 5223 [V] ([5222](https://blog.rust-lang.org/2026/05/25/cve-2026-5222/), [5223](https://blog.rust-lang.org/2026/05/25/cve-2026-5223/)).
- crates.io now shows RustSec "unmaintained" banners and a source-code viewer [V] ([update](https://github.com/rust-lang/blog.rust-lang.org/blob/main/content/crates-io-development-update-2026-07/index.md)).

---

## 2. Recommended stack (summary table)

Versions are latest stable crates.io releases as of today unless marked. "MSRV" is the declared `rust-version`.

| Area | Recommended | Alternatives | Rationale |
|---|---|---|---|
| Toolchain | Rust stable (>= 1.98.1, moving to 1.99), edition 2024, resolver 3 | nightly only for optional dev experiments | Point releases keep fixing miscompilations; do not lag |
| Async runtime | **tokio 1.53.1** (2026-07-20) | smol 2.0.2; compio 0.19.2 / monoio / glommio (io_uring) | De facto standard; LTS lines 1.51.x (to Mar 2027) and 1.53.x (to Sep 2027) |
| HTTP stack | **axum 0.8.9** + hyper 1.11.1 + tower 0.5.3 + tower-http 0.7.1 | actix-web 4.15.0; Loco 1.2.0; Salvo 1.0.0; Poem 3.1.12 | Tower middleware reuse; axum's downloads dwarf the rest |
| DB driver | **sqlx 0.9.0** (PostgreSQL) | tokio-postgres 0.7.18 + deadpool 0.13.1; diesel-async 0.9.2 | Async, runtime-checkable, `QueryBuilder` for dynamic SQL |
| Query building | **sea-query 1.0.2** | sqlx `QueryBuilder` only | Runtime SQL AST, needed for a metadata-driven ORM |
| ORM | Thin in-house layer over sqlx + sea-query (see sibling doc 08) | SeaORM 2.0.4; Diesel 2.3.13; Toasty 0.11.0; Cornucopia 1.0.1 | Static-entity ORMs fight module-extensible models (3.3) |
| Migrations | sqlx migrate for core + in-house runtime DDL engine | sea-orm-migration 2.0.4; refinery 0.9.2; diesel_migrations 2.3.2 | Per-module migration history needed |
| Serialisation | serde 1.0.229, serde_json 1.0.151 | rkyv 0.8.18 (caches); facet 0.46.5 (watch) | |
| Validation | garde 0.23.0 at API edge; registry-driven validation for models | validator 0.21.0 | Model constraints live in metadata, not derives |
| OpenAPI | **utoipa 6.0.0** + utoipa-axum 0.3.0 + utoipa-scalar 0.4.0 | aide 0.15.1 (0.16 alpha); poem-openapi 5.1.16 | Most active; v6 adds OpenAPI 3.2 |
| GraphQL / gRPC | Defer. async-graphql 7.2.1 if needed; tonic 0.14.6 + prost 0.14.4 | juniper 0.17.1; seaography 2.0.0-rc.9 | Not needed for an Odoo-style JSON-RPC/REST core |
| Authn | argon2 0.6.0, jsonwebtoken 11.1.0, openidconnect 4.0.1 + oauth2 5.0.0, webauthn-rs 0.5.5, tower-sessions 0.15.0 | axum-login 0.18.0 | All maintained (latest repo pushes: axum-login 2026-06, tower-sessions 2026-08, openidconnect 2025-11, oauth2 2026-02, webauthn-rs 2026-09) |
| Authz | **Native ACL + record-rule engine compiled to SQL predicates** | cedar-policy 4.13.0; casbin 2.20.0; OpenFGA 1.21.0 + openfga-client 0.6.1 | Row filtering needs SQL pushdown |
| Reports / PDF | **typst 0.15.1** as library | chromiumoxide 0.9.1 (sidecar); krilla 0.8.2; printpdf 0.12.8 | PDF/A + tagged PDF, Apache-2.0 |
| Spreadsheets | calamine 0.36.1 (read), rust_xlsxwriter 0.99.1 (write), csv 1.4.0 | umya-spreadsheet 3.1.0 | |
| Templates (DB-stored) | minijinja 2.24.0 or tera 2.4.0 | askama 0.16.1 / maud 0.27.0 (compile-time only) | Runtime-editable templates needed |
| Email | lettre 0.11.23 (out), mail-parser 0.11.9 + async-imap 0.12.0 (in), mail-builder 1.0.0, mail-auth 0.13.3 | mailparse 0.17.0 | Covers mail-gateway/chatter use cases |
| Search | PostgreSQL FTS + pg_trgm first; tantivy 0.26.2 or Meilisearch 1.54.3 adapter later | ParadeDB (AGPL-3.0) | |
| Jobs / cron | In-house `FOR UPDATE SKIP LOCKED` queue + croner 4.0.0 | apalis 1.0.0-rc.10; pgmq 0.33.7; graphile_worker 0.13.6 | Transactional enqueue, tenant context |
| Decimal / money | rust_decimal 1.43.0 inside an `Amount`/`Money` newtype | bigdecimal 0.4.11; rusty-money 0.5.1 | |
| Time | chrono 0.4.45 at sqlx/API edge; jiff 0.2.37 for time-zone logic | chrono-tz 0.10.4; time 0.3.55 | sqlx has no jiff feature |
| i18n | PO catalogs (polib 0.3.0 / poreader 1.1.2) + icu 2.3.1 | fluent-bundle 0.16.0; rust-i18n 4.2.4; gettext-rs 0.8.0 | Odoo-compatible translation workflow |
| Extensions | Cargo-feature modules + **wasmtime 49.0.1** components for untrusted code; rhai 1.26.1 or CEL (`cel` 0.14.5) for in-DB expressions | extism 1.30.0; mlua 0.12.1; boa_engine 0.22.0; rquickjs 0.14.0; deno_core 0.412.0 | |
| Module registration | Explicit `register()` per module crate + inventory 0.3.24 inside crates | linkme 0.3.37; stabby 72.1.16; abi_stable 0.11.3 | |
| Observability | tracing 0.1.44, tracing-subscriber 0.3.23, opentelemetry 0.33.0 + tracing-opentelemetry 0.34.0 | metrics 0.24.6 + Prometheus exporter 0.18.3 | |
| Testing | cargo-nextest 0.9.146, proptest 1.11.0, insta 1.48.0, testcontainers 0.28.0 | criterion 0.8.2 / divan 0.1.21; cargo-llvm-cov 0.9.1 | |
| Supply chain | cargo-deny 0.20.2, cargo-vet 0.10.2, cargo-audit 0.22.2, cargo-semver-checks 0.50.0 | cargo-shear 1.14.0 | See 1.5 |
| Distribution | Static musl binary, `dist` 0.33.0, container from scratch/distroless, rust-embed 8.12.0 | cargo-zigbuild 0.23.4; cargo-chef 0.1.78 | |

Sources for every version: `https://crates.io/crates/<name>` (crates.io API, queried 2026-10-01) [V]; GitHub release pages where dates are quoted below. Exceptions that are not crates.io crates: `dist` 0.33.0 (GitHub release 2026-09-11; the crates.io `cargo-dist` crate is still at 0.32.0), Wild, mold, Meilisearch, OpenFGA and the Rust toolchain. Re-checked 2026-10-01 by a second pass (crates.io API plus GitHub releases); all crates.io versions above matched.

---

## 3. Findings by area

### 3.1 Async runtime

tokio 1.53.1 (2026-07-20) with LTS branches `1.51.x` (to March 2027) and `1.53.x` (to September 2027); from 1.54 the MSRV becomes 1.85 ([README](https://github.com/tokio-rs/tokio)) [V]. async-std is **discontinued** and its README tells users to switch to smol [V] ([README](https://github.com/async-rs/async-std)); smol's last release is 2.0.2 (2024-09-07) [V]. io_uring runtimes (compio, monoio, glommio) are niche and not compatible with axum/sqlx [I]. Decision: tokio only, enforce with `cargo-deny` bans.

### 3.2 Web framework

| Framework | Version / date | Signals |
|---|---|---|
| **axum** | 0.8.9 (2026-04-14), MSRV 1.80, MIT, 27.3k stars, 123.9M 90-day downloads [V] | `main` holds unreleased **breaking** changes (serve API reshaped, router fallback merge) and a 0.9 milestone with 4 open issues [V] ([CHANGELOG](https://github.com/tokio-rs/axum/blob/main/axum/CHANGELOG.md)); still 0.x after five years |
| actix-web | 4.15.0 (2026-08-21), 24.9k stars, 11.2M 90-day downloads [V] | Active, but actix-http had HTTP/1.1 request smuggling (GHSA-xhj4-vrgc-hr34, April 2026) and a stream of Sept 2026 hardening patches (e.g. reject missing or duplicate `Host`) [V] |
| Loco | 1.2.0 (2026-09-24); **1.0.0 landed late July 2026** [V] | Axum + SeaORM 2 + Tera framework; 1.2 adds opt-in row-level multi-tenancy (`.in_tenant(id)`, explicit at call site) [V] ([release](https://github.com/loco-rs/loco/releases/tag/v1.2.0)). Opinionated "Rails" shape; obop needs its own module system |
| Salvo | 1.0.0 (2026-09-24) [V] | Brand new 1.0; smaller community (4.4k stars) |
| Poem | 3.1.12 (2025-07-28) [V] | Commits continue (Sept 2026) but no release for 14 months [V/I] |

Pick axum: Tower middleware (auth, tracing, limits) is reusable across crates, and hyper 1.11.1 / tower-http 0.7.1 are current. Hedge the 0.x risk by keeping axum behind one `obop-http` crate.

### 3.3 Database access, ORM and migrations

| Option | State | Fit for obop |
|---|---|---|
| **sqlx 0.9.0** | crates.io 2026-05-21 (CHANGELOG header says 2026-05-06); MSRV 1.94; repo moved to `transact-rs/sqlx` because it "has not been owned or maintained by LaunchBadge" for years [V] ([CHANGELOG](https://github.com/transact-rs/sqlx/blob/main/CHANGELOG.md)). New: `sqlx.toml` (renamable `_sqlx_migrations` table, multi-schema **multi-tenant example**; needs the `sqlx-toml` cargo feature, enabled by default in `sqlx-cli` but not in the `sqlx` library), `SqlSafeStr` (runtime SQL must be `&'static str` or wrapped in `AssertSqlSafe`), no jiff support (chrono/time only) [V] | Best driver fit. The `AssertSqlSafe` wrapper is a deliberate speed bump for dynamic SQL built from registry metadata |
| **SeaORM 2.0.4** | 2.0.0 stable **2026-07-19**; on sea-query 1.0 + sqlx 0.9; MSRV 1.94. New entity format, `Entity Loader`, entity-first `schema sync` via `entity-registry` (uses `inventory`), table-scoped **RBAC engine** with `RestrictedConnection`, sync variant on rusqlite [V] ([2.0.0 notes](https://github.com/SeaQL/sea-orm/releases/tag/2.0.0)) | Strongest off-the-shelf option and a real precedent for inventory-based entity registration, but entities are static Rust types. Good for fixed system tables; weak for modules that extend each other's models |
| Diesel 2.3.13 + diesel-async 0.9.2 | 2026-09-04 / 2026-06-19 [V] | Compile-time typed DSL. Default 32-column tables; `64-`/`128-column-tables` features (or the `custom-count-column-tables` feature plus the `DIESEL_MAX_COLUMN_COUNT` env var) needed for wide ERP tables. Diesel's own docs say 64 columns "will increase your compile times" and 128 "significantly" [V] ([Cargo.toml](https://github.com/diesel-rs/diesel/blob/main/diesel/Cargo.toml), [lib.rs docs](https://github.com/diesel-rs/diesel/blob/main/diesel/src/lib.rs)). `diesel-dynamic-schema` 0.2.4 exists but is secondary |
| Toasty 0.11.0 | tokio-rs, MIT, monthly releases (0.8 to 0.11 between 2026-07-10 and 2026-09-25), SQL + DynamoDB, MSRV 1.95 [V] | Pre-1.0 and fast-moving; watch, do not adopt |
| Cornucopia 1.0.1 | SQL-first codegen; **Clorinde was merged back and archived** (2026-05-21) [V] | Viable for hot-path fixed queries only |

Migrations: sqlx's `Migrator` plus per-crate `sqlx.toml` lets each module keep its own history table [V]. A dynamic platform additionally needs runtime DDL for user-defined fields; that design belongs to [08-metadata-platforms-dynamic-orm.md](08-metadata-platforms-dynamic-orm.md).

**Compile-time macros caveat:** `query!` needs `DATABASE_URL` or a committed offline cache, and the schema depends on which modules are installed [I]. Use the macros only in fixed core crates; use runtime builders elsewhere.

### 3.4 Serialisation and validation

serde 1.0.229 / serde_json 1.0.151 are universal [V]. rkyv 0.8.18 (zero-copy) suits in-process caches, not the API. facet is pre-1.0 with a 0.50.0-rc.7 line [V]; watch only. bincode is at 3.0.0 (2025-12-16). YAML: `serde_yaml` was archived upstream (repo archived; last release 0.9.34+deprecated, 2024-03-25) [V]; utoipa 6 switched to `yaml_serde` (0.10.7, 2026-08-18) [V]. validator 0.21.0 and garde 0.23.0 are both maintained; neither replaces registry-driven field constraints.

### 3.5 API layer

- **OpenAPI:** utoipa 6.0.0 (2026-09-22) adds OpenAPI 3.2 support, MSRV 1.88, removes `serde_norway` [V] ([release](https://github.com/juhaku/utoipa/releases/tag/utoipa-6.0.0)). aide's stable line is 0.15.1 (2025-08) with 0.16.0-alpha.4 (2026-04) [V]. For dynamic models, build OpenAPI documents from the registry at runtime using utoipa's builder types [G].
- **GraphQL:** async-graphql 7.2.1 (2026-01-20) is stable; 8.0.0-rc.5 (2026-04-21) is a release candidate, and the repo has had no push since 2026-04-21 [V]. The rc notes mention fixing `MergedObject` hitting the compiler recursion limit at 130+ merged types and codegen rework for compile time [V], which is exactly the shape of a many-module schema. juniper 0.17.1 is quieter. Defer GraphQL.
- **gRPC:** tonic 0.14.6 (2026-05-07); development moved to `grpc/grpc-rust` and master is "preparing breaking changes" [V]. Internal use only, if at all.

### 3.6 Authentication and authorisation

| Crate | Version / state |
|---|---|
| argon2 0.6.0 (2026-08-27), password-hash 0.6.1 | RustCrypto, maintained [V] |
| jsonwebtoken 11.1.0 (2026-09-16) | Active [V] |
| openidconnect 4.0.1 (2025-07), oauth2 5.0.0 (2025-01) | Stable, slow cadence; repos pushed Nov 2025 / Feb 2026 [V] |
| webauthn-rs 0.5.5 (2026-04-30), Kanidm | **MPL-2.0** (file-level copyleft; acceptable as a dependency, note for licence policy) [V] |

Authorisation engines:

| Engine | State | Verdict |
|---|---|---|
| cedar-policy 4.13.0 (2026-09-15) | Apache-2.0, AWS; `partial-eval` and type-aware partial evaluation (`tpe`) are still *experimental* features [V] | Good for discrete allow/deny; not yet a stable way to compile rules into SQL filters |
| casbin 2.20.0 (2026-02-04) | Maintained (repo pushed 2026-08-13) [V] | Request-level RBAC/ABAC matching |
| oso 0.27.3 | Open-source library **deprecated** by its vendor; last release 2024-01 [V] | Avoid |
| OpenFGA 1.21.0 (2026-09-20) | CNCF incubating Go server (Apache-2.0; incubating since 2025-10-28 per [cncf.io](https://www.cncf.io/projects/openfga/)); Rust client `openfga-client` 0.6.1 from a third party (vakamo-labs) [V] | Separate service; overkill for a single-binary product |

Odoo's security is group ACLs plus record rules that are *domain filters applied to list queries*. Point decisions (Cedar, Casbin) cannot cheaply filter a 100k-row list [I]. Build the record-rule engine natively (domain -> sea-query predicate); offer Cedar later as an optional policy layer for API-level decisions.

### 3.7 Reporting, PDF, spreadsheets, templating

| Tool | State | Verdict |
|---|---|---|
| **typst 0.15.1** | 2026-07-17, Apache-2.0, MSRV 1.92. PDF export rewritten on krilla in 0.14 (2025-10-24): PDF/A, PDF/UA-1, tagged PDF; 0.15 can target multiple PDF standards, adds spot colours and variable fonts. HTML export exists and improves (MathML) but is still the weaker path [V] ([releases](https://github.com/typst/typst/releases)) | Best embedded report engine. Library API is not advertised as stable; the wrapper `typst-as-lib` 0.16.0 says its API "is currently not really stable" [V]. Isolate behind an `obop-report` crate |
| krilla 0.8.2 | MIT/Apache; PDF primitives only, "no text layout, tables, page breaking" by design [V] | Underneath Typst; not a report engine |
| printpdf 0.12.8 | MIT; HTML-to-PDF "works for basic layouts ... but is still evolving" [V] | Fallback for simple documents |
| genpdf 0.2.0 | Last release 2021-06-17 [V] | Avoid |
| chromiumoxide 0.9.1 | MIT/Apache, released 2026-02-25, repo pushed 2026-04-03; needs a Chromium binary at runtime [V] | Optional sidecar for user-authored HTML reports (Odoo QWeb parity) |

Spreadsheets: calamine 0.36.1 (2026-07-27, MIT) reads xlsx/xls/ods; rust_xlsxwriter 0.99.1 (2026-09-14) writes xlsx and is one step from 1.0 [V]. Templates: tera 2.4.0 (Tera 2 is now current; Loco 1.x ships it) and minijinja 2.24.0 (3.0.0-alpha.2 exists) are runtime engines suitable for DB-stored email and document templates; askama 0.16.1 and maud 0.27.0 are compile-time only [V].

### 3.8 Email

lettre 0.11.23 (2026-08-03) for SMTP; Stalwart's `mail-parser` 0.11.9 (2026-09-09), `mail-builder` 1.0.0 (2026-09-12), `mail-auth` 0.13.3 for DKIM/SPF/DMARC; `async-imap` 0.12.0 (2026-09-30, async-email org). The classic `imap` crate is stuck at 2.4.1 with a long-lived 3.0 alpha [V]. This is a workable inbound/outbound mail-gateway stack for chatter and helpdesk aliases.

### 3.9 Search

PostgreSQL full-text search and `pg_trgm` cover the Odoo `name_search` and global-search use case without new infrastructure [I]. tantivy 0.26.2 (2026-09-08, MIT) is embeddable but gives you index lifecycle work. Meilisearch 1.54.3 (2026-10-01; 1.54.2 was 2026-09-29) is **MIT plus BUSL-1.1 for Enterprise Edition parts** (`MIT AND BUSL-1.1`) [V] ([LICENSE](https://github.com/meilisearch/meilisearch/blob/main/LICENSE)); use as an external, optional service. ParadeDB `pg_search` is AGPL-3.0 [V]; avoid as a hard dependency.

### 3.10 Job queues

| Option | State |
|---|---|
| apalis | 0.7.4 stable; **1.0.0-rc.10 (2026-09-15)**; Redis, Postgres, SQLite, MySQL, AMQP backends; Tower middleware; repo moved to `apalis-dev` [V] |
| pgmq | 0.33.7 (2026-05-20), PostgreSQL licence, 5.3k stars; client has a 0.34 alpha [V] |
| graphile_worker (Rust port) | 0.13.6 (2026-09-29), 84 stars [V] |
| sqlxmq 0.6.0 | Last push 2025-05-25 [V] |
| fang 0.11.0 (2026-07-02), underway 0.2.0 (2025-07) | Small projects [V] |

Odoo's `ir.cron` runs each cron row as a specific user (its required `user_id` field; the cron model has no company field of its own) [V] ([ir_cron.py](https://github.com/odoo/odoo/blob/master/odoo/addons/base/models/ir_cron.py)), and jobs must be enqueued inside the business transaction. A ~1 kLOC in-house queue over `SELECT ... FOR UPDATE SKIP LOCKED` plus `LISTEN/NOTIFY` (sqlx `PgListener`) fits that better than a generic framework [I]. Revisit apalis when 1.0 is final.

### 3.11 Money, time, i18n

- **Decimal:** rust_decimal 1.43.0 (2026-09-02) has a 96-bit integer mantissa and a scale of at most 28 (`MAX_SCALE = 28`), about 28 significant digits [V] ([README](https://github.com/paupino/rust-decimal), `src/constants.rs`), ample for currency with 6 decimals. bigdecimal 0.4.11 is arbitrary precision (matches PostgreSQL `NUMERIC` fully). sqlx and SeaORM support both. Wrap in a `Money`/`Amount` newtype carrying currency and a rounding policy (half-up, per-currency precision) to mirror Odoo's `float_round`/`decimal.precision`.
- **Time:** jiff 0.2.37 (2026-09-12) is best-in-class for time zones, but the author states in the README (April 2026) that there is no timeline for 1.0 [V] ([README](https://github.com/BurntSushi/jiff)); sqlx 0.9 has only chrono and time features [V]. chrono 0.4.45 (2026-06-04) is the ecosystem default; chrono-tz 0.10.4 dates from 2025-07-11 [V].
- **i18n:** icu (ICU4X) 2.3.1 (2026-08-20) for plurals, number/date formatting, collation [V]. fluent-bundle 0.16.0 (2025-05) with an active repo. Odoo's translation workflow is PO files: polib 0.3.0, poreader 1.1.2 and gettext 0.4.0 exist but are tiny crates [V]. `gettext-rs` 0.8.0 binds C libintl, awkward for static musl [G].

### 3.12 Extension and scripting

| Runtime | Version | Notes |
|---|---|---|
| **wasmtime** | 49.0.1 (2026-09-24), MSRV 1.96, Apache-2.0 + LLVM exception | Monthly majors on the 20th; every 12th release is LTS with 24-month support (v36, v48 now: both received patch releases 36.0.16 and 48.0.3 on 2026-09-24) and security backports [V] ([policy](https://github.com/bytecodealliance/wasmtime/blob/main/docs/stability-release.md), [releases](https://github.com/bytecodealliance/wasmtime/releases)). **WASI 0.3.0 shipped 2026-06-11** with native async (`stream<T>`, `future<T>`); 0.3.1 2026-08-11; 0.3.2 planned 2026-10-13 (bi-monthly train, second Tuesday); Wasmtime 46+ enables it by default [V] ([roadmap](https://wasi.dev/roadmap)). `wasm32-wasip2` is Rust Tier 2 since 1.82 [V]; `cargo-component` looks stale (last release 0.21.1 on 2025-04-07, last push 2025-07-14) next to `wit-bindgen` 0.62.0 (2026-09-10) [V for dates, I for "superseded"] |
| wasmer | 7.5.0 (published today) | Alternative host; smaller Rust community [V] |
| extism | 1.30.0 (2026-06-04), BSD-3-Clause | Friendlier plug-in SDK; verified only version/licence [V] |
| rhai | 1.26.1 (2026-09-10) | Pure-Rust, embed-friendly for formulas and server actions [V] |
| mlua 0.12.1; rquickjs 0.14.0; boa_engine 0.22.0 (pure Rust, MSRV 1.91); deno_core 0.412.0 (V8, heavy, 0.x churn); starlark 0.14.2; pyo3 0.29.3 | | Only if a specific language is mandated |
| `cel` 0.14.5 (MIT), evalexpr 13.1.0 (**AGPL-3.0-only** since 12.0.0 in 2024-10; 11.x was MIT) | | Small expression evaluators for domains and automation conditions [V versions and licences]. Check evalexpr's AGPL against obop's licence policy before using it ([README](https://github.com/ISibboI/evalexpr), crates.io) |

### 3.13 Module registration patterns

- **inventory 0.3.24**: life-before-main registration; Linux, macOS, iOS, FreeBSD, Android, Windows, WebAssembly (with caveats around `__wasm_call_ctors`); order of iteration is unspecified [V] ([README](https://github.com/dtolnay/inventory)). SeaORM's `entity-registry` is built on it [V].
- **linkme 0.3.37**: linker-section "distributed slices", no life-before-main, works on Linux, macOS, Windows, FreeBSD, OpenBSD, illumos [V].
- Both only see crates that actually end up linked, so a module crate that nothing references can silently vanish [G]. Guard with an explicit list in the binary crate and a startup assertion comparing the registry with the enabled-modules manifest.
- **Trait-object registries** (`Box<dyn Module>`): fine, but async methods need `async-trait` 0.1.92, `dynosaur` 0.3.1 or `trait-variant` 0.1.3; mind the 1.98.1 vtable fix (1.1).
- **Dynamic native loading:** Rust has no stable ABI. `abi_stable` 0.11.3 was last released 2023-10-12; `stabby` 72.1.16 (2026-07-20, EPL-2.0 OR Apache-2.0) is active; `libloading` 0.9.0 is the raw mechanism [V]. Sharing a tokio runtime, allocator and tracing dispatcher across a dylib boundary is fragile [I]; avoid for first-party modules.
- **WASM components** (above) are the sandboxed route for third-party and customer code.

### 3.14 Observability and testing

OpenTelemetry Rust: Logs and Metrics API/SDK **Stable**, Traces API/SDK and the OTLP trace exporter **Beta**, OTLP log/metric exporters RC ([README](https://github.com/open-telemetry/opentelemetry-rust)) [V]. Crates are 0.33.0 (2026-09-18); tracing-opentelemetry 0.34.0 followed on 2026-09-23, and Salvo bumped "OpenTelemetry crates to 0.33 together" [V], so pin all `opentelemetry*` crates in lockstep [I]. tracing itself is quiet: 0.1.44 (2025-12-18), subscriber 0.3.23 (2026-03-13) [V].

Testing: nextest 0.9.146 (2026-09-21, MSRV 1.91), proptest 1.11.0, insta 1.48.0, testcontainers 0.28.0 (+ modules 0.15.0), wiremock 0.6.5, cargo-llvm-cov 0.9.1 [V]. sqlx's `#[sqlx::test]` creates a fresh, isolated database per test (needs the `migrate` feature; tests share the server's connection limit) [V] ([docs source](https://github.com/transact-rs/sqlx/blob/main/src/macros/test.md)).

### 3.15 Distribution

Static single-binary is achievable: musl 1.2.5 bundled since 1.93, `riscv64-unknown-linux-musl` promoted to Tier 2 in 1.99 [V]. Practicalities [G]: musl's allocator is slow, so set `mimalloc` 0.1.52 or `tikv-jemallocator` 0.7.0 as global allocator; prefer rustls over OpenSSL, noting some rustls crypto providers need a C toolchain. `dist` (formerly cargo-dist) 0.33.0 (GitHub release 2026-09-11; crates.io `cargo-dist` is still 0.32.0) handles release automation [V] ([releases](https://github.com/axodotdev/cargo-dist/releases)); `cargo-zigbuild` 0.23.4 and `cargo-chef` 0.1.78 help cross-compiling and Docker layer caching. Embed the Vite build output with `rust-embed` 8.12.0 so one binary ships UI and server. `cargo-auditable` 0.7.6 adds dependency metadata to binaries for scanners.

### 3.16 Prior art

A GitHub search for Rust ERPs finds nothing credible (top hits 23-53 stars) [V]. The best large-scale Rust reference is **Hyperswitch** (Apache-2.0, 45k stars, pushed today): a 39-crate workspace (`crates/*` directories; edition 2021, resolver 2) on actix-web + diesel + redis with `[workspace.lints]` (`unsafe_code = "forbid"`, many clippy restrictions) [V] ([repo](https://github.com/juspay/hyperswitch)). It shows a large multi-crate Rust business platform works at scale, on Diesel rather than sqlx.

---

## 4. Sharp edges and mitigations

| Sharp edge | Why it bites obop | Mitigation |
|---|---|---|
| **Compile times in a 100+ crate workspace** | Proc-macro-heavy stack (serde, sqlx, utoipa, async-graphql, SeaORM); parallel front-end and Cranelift are not stable; survey and roadmap name it the top pain [V] | Many small crates with a strict DAG (`*-api` contract crates vs `*-impl`); one top-level `obop` binary crate selecting modules by Cargo feature; `cargo-hakari`; `sccache`; dev profile `debug = "line-tables-only"` and `opt-level` only for hot deps; lld/mold; `cargo build --timings` in CI; avoid monolithic GraphQL schemas; keep generic-heavy code in leaf crates; 1.99 disables incremental in CI by default |
| **Dynamic schema vs static typing** | Odoo modules add fields to other modules' models and users add custom fields at runtime | Static `#[derive(Model)]` core that registers `ModelDef`s, plus registry-driven dynamic records built with sea-query; typed facades for hot paths (details in doc 08). Diesel's column limits and macro-time schemas make it a poor core here |
| **Orphan rules** | Module crates cannot implement foreign traits for foreign types (e.g. a module cannot `impl From<core::Partner> for sales::Lead` if neither is local) | Shared `*-contract` crates owning types and traits; newtype wrappers; extension via registry and events rather than trait impls across modules; blanket impls only in the crate owning the trait |
| **Async ecosystem fragmentation** | async-std dead; `dyn` + `Send` for async traits unresolved until at least 2027 (goals open) [I]; libraries tied to tokio types | tokio-only policy; `trait-variant` for `Send` variants, `dynosaur` or `async-trait` for `dyn`; return boxed futures at module boundaries |
| **0.x API churn** | axum 0.9 pending; utoipa 5 -> 6 landed 9 days ago; apalis 1.0 rc; jiff 0.2; opentelemetry 0.33; tonic master breaking | Wrap volatile crates in facade crates; `[workspace.dependencies]` as single pin point; batch dependency bumps monthly; `cargo-semver-checks` on obop's own public crates |
| **MSRV ratchet** | sqlx 0.9 and SeaORM 2.0 require 1.94; Toasty 1.95; wasmtime 49 requires 1.96 [V] | Track latest stable with `rust-toolchain.toml`; set workspace `rust-version`; do not promise old-compiler support |
| **Compiler miscompilations** | Three consecutive `x.y.1` releases fix codegen bugs; 1.98.1's vtable bug hit boxed async services, the pattern tower-style stacks use | Stay on the newest patch; run the test suite in release mode too; nightly canary job |
| **Linker-based registration** | inventory/linkme can miss unlinked crates; wasm constructors need care | Explicit module list in the binary crate; registry-vs-manifest startup check; tests on each OS |
| **sqlx macro + module-conditional schema** | `query!` validates against one database state | Macros only in core; runtime builders plus integration tests elsewhere; `AssertSqlSafe` only in the single SQL-emitting crate |
| **Windows developer machines** | Wild is Linux-only [V]; lld-default applies to Linux [V] | Dev containers or WSL for fast linking [I]; CI on Linux |
| **Supply chain** | Active 2026 campaigns against crate owners (1.5) | `cargo-deny` (licences, bans, sources), `cargo-vet` or audited lockfile, Dependabot/Renovate with cooldown, no secrets in cached `target/`, review new transitive crates |

---

## 5. Confidence & gaps

- **High [V]:** every version, date, licence and star figure in tables above came from live crates.io, GitHub or dist endpoints on 2026-10-01. Rust 1.98.1 as served stable; 1.99.0 dated today in `RELEASES.md` but not yet on the dist stable channel at my check.
- **Medium:** the project-goal and WASI roadmap summaries were read through a summarising fetch tool; goal texts were additionally read raw from the goals repo for parallel front-end, Cranelift and Fast Builds. The survey post was fetched at a URL I constructed (it resolved).
- **Not verified / guesses [G]:** mimalloc-on-musl advice; runtime OpenAPI building in utoipa; `gettext-rs` and musl friction; loader-order caveats for inventory/linkme; Extism's component-model status; MiniJinja resource-limit features; whether `garde` or `validator` is friendlier for contextual validation. (Resolved in the second pass: rust_decimal precision, `#[sqlx::test]` behaviour, `serde_yaml` archival, and the mail-crate licences: `mail-auth`, `mail-send`, `mail-parser` and `mail-builder` are all Apache-2.0 OR MIT on crates.io [V].)
- **Discrepancies noted:** sqlx 0.9.0 changelog date (2026-05-06) vs crates.io publish (2026-05-21); Loco 1.0.0 body says 2026-07-25 vs GitHub release 2026-07-29; SeaORM 2.0.0 is 2026-07-19 on crates.io and in its CHANGELOG but the GitHub release object is stamped 2026-07-20T00:10Z (UTC).
- **Sparse-index versions** (axum-login 0.18.0, tower-sessions 0.15.0, typst-as-lib 0.16.0, jwt-simple, ring) are "most recently published", which can be a backport, so treat as approximate. crates.io rate-limited me for a few queries.
- **Gaps:** no benchmarks (framework throughput, compile times, ORM overhead); no forum or Reddit sentiment; no check of Windows-specific toolchain behaviour; Vite+ frontend tooling is out of scope (see 07). The actual facts on how PostgreSQL multi-tenancy should work live in the data-layer docs.

---

## Implications for obop

**MUST**

1. **MUST** target Rust stable only, edition 2024 and resolver 3 in every crate, with `rust-toolchain.toml` tracking the newest patch (>= 1.98.1, move to 1.99 immediately). Three recent patch releases were miscompilation fixes.
2. **MUST** use tokio as the only runtime and ban async-std via `cargo-deny`.
3. **MUST** use axum (0.8.9, moving to 0.9) with tower/tower-http, isolated behind one `obop-http` crate so the 0.x break is a one-crate fix.
4. **MUST** use sqlx 0.9 + sea-query 1.0 as the data foundation, and build obop's own registry-driven model layer on it rather than making a static ORM the core abstraction (consistent with doc 08 section 6).
5. **MUST** build Odoo-style ACLs and record rules natively as domain-to-SQL predicate injection; Cedar/Casbin/OpenFGA cannot filter list queries without an experimental partial-evaluation path.
6. **MUST** centralise all dependency versions in `[workspace.dependencies]`, lint policy in `[workspace.lints]`, and enforce supply-chain checks (`cargo-deny` licences/bans/sources, `cargo-audit`, `cargo-vet` or equivalent) in CI from day one.
7. **MUST** design module registration around an explicit, generated module list in the binary crate, with inventory only for intra-crate items and a startup check that registry equals manifest.

**SHOULD**

8. **SHOULD** structure the workspace as many small crates with `*-contract` crates owning shared types and traits, to sidestep orphan rules and keep incremental builds narrow; use Cargo features to choose the module set.
9. **SHOULD** adopt Typst 0.15.x as the embedded PDF engine (templates, PDF/A, tagged PDF), wrapped in `obop-report` because its library API is unstable; add Chromium (chromiumoxide) only as an optional HTML-report sidecar.
10. **SHOULD** use wasmtime components (WASI 0.3, `wit-bindgen`) as the sandboxed extension path for third-party code, with fuel/epoch limits; pin to an LTS major (v48 now) for production and plan a bump policy.
11. **SHOULD** write the job queue in-house on PostgreSQL `SKIP LOCKED` + `LISTEN/NOTIFY` with transactional enqueue and tenant/company context; re-evaluate apalis once 1.0 final ships.
12. **SHOULD** use utoipa 6 for OpenAPI and generate the dynamic-model parts from the registry at runtime.
13. **SHOULD** use `rust_decimal` behind a `Money` newtype with explicit rounding policy; chrono at DB/API edges; jiff for time-zone-sensitive logic, until sqlx gains jiff support.
14. **SHOULD** use PO files as the translation interchange format with ICU4X for formatting.
15. **SHOULD** use PostgreSQL FTS + `pg_trgm` for search first, keeping a `SearchBackend` trait for tantivy or Meilisearch later.
16. **SHOULD** apply the build-time toolkit now: lld/mold on Linux CI, `sccache`, `cargo-hakari`, `line-tables-only` debuginfo, `cargo build --timings` tracked per release, nextest for tests.
17. **SHOULD** ship a static musl binary with mimalloc, embedded frontend assets and `dist` for releases, after confirming TLS/crypto provider build requirements in CI.

**COULD**

18. **COULD** adopt SeaORM 2.0 for fixed system tables or as a fallback if the in-house ORM slips; it is stable (2.0.0, 2026-07-19) and shares sea-query/sqlx.
19. **COULD** add Cedar as an optional API-level policy layer once its partial-evaluation features leave experimental status.
20. **COULD** add GraphQL (async-graphql 8 when final) and gRPC (tonic) later; both are peripheral to an Odoo-style core.
21. **COULD** try the Cranelift backend and Wild linker on Linux dev machines as opt-in experiments; do not make them the default until stable.
22. **COULD** evaluate Toasty when it nears 1.0, and watch facet and rkyv for internal caching.
23. **COULD** use `cel` or rhai for formula, domain and automation expressions, chosen alongside the domain-expression design in doc 08.


## Verification log

Second-pass adversarial check on 2026-10-01 against primary sources: the Rust dist manifests and `RELEASES.md`, the Rust blog, the Rust Project Goals repo, the crates.io API (every crate version, release date, MSRV, licence and 90-day download figure), GitHub releases, changelogs, READMEs and Cargo.toml files via `gh api`/raw files, cncf.io, wasi.dev, and Odoo master source for Odoo-side claims.

**Confirmed as stated: about 250 discrete statements.** This is 141 crate versions compared programmatically against crates.io with zero mismatches (plus the quoted release dates, licences, MSRVs and star counts that were read alongside them), and about 110 other checks. All 13 flagged claims hold, except that Meilisearch's version was stale (correction 1). The flagged claims confirmed were:
- Rust 1.98.1 / 1.98.0 dates and the dist channel still at 1.98.1.
- Rust 2024 and the feature-stabilisation versions.
- Parallel front-end status and the Cranelift Feb-2026 numbers.
- The vtable miscompilation and the 1.96.1 / 1.97.1 fixes.
- sqlx 0.9 (transact-rs move, `AssertSqlSafe`, multi-tenant example, no jiff feature).
- SeaORM 2.0 (2026-07-19 on crates.io, `inventory`-based `entity-registry`, RBAC engine).
- axum 0.8.9 (unreleased breaking changes, 0.9 milestone, 123.9M vs 11.2M downloads, about 11x).
- Typst 0.15.1 (krilla, PDF/A, PDF/UA-1, tagged PDF since 0.14) and the `typst-as-lib` stability quote.
- Wasmtime release and LTS policy, WASI 0.3.0 (2026-06-11) and Wasmtime 46+ default.
- Cedar experimental `partial-eval`/`tpe`, oso deprecated, OpenFGA v1.21.0.
- Diesel 32-column default and the Clorinde archival / Cornucopia 1.0.0.
- ParadeDB AGPL-3.0, webauthn-rs MPL-2.0, jiff's "no timeline for 1.0", and the Meilisearch `MIT AND BUSL-1.1` licence.

### Corrections

| # | Was | Now | Source |
|---|---|---|---|
| 1 | Meilisearch 1.54.2 (2026-09-29) as the current release (summary table and 3.9) | 1.54.3 (2026-10-01) is current; 1.54.2 was 2026-09-29. Licence `MIT AND BUSL-1.1` unchanged | https://github.com/meilisearch/meilisearch/releases |
| 2 | Odoo `ir.cron` "runs per-cron rows under a specific user and company" | A specific user only (required `user_id`); the cron model has no company field | https://github.com/odoo/odoo/blob/master/odoo/addons/base/models/ir_cron.py |
| 3 | Diesel: `DIESEL_MAX_COLUMN_COUNT` listed as an alternative, compile-time cost marked [I] | The env var only works with the `custom-count-column-tables` feature; Diesel's docs state 64 columns increases compile times and 128 "significantly", so now [V] | https://github.com/diesel-rs/diesel/blob/main/diesel/src/lib.rs |
| 4 | sqlx `sqlx.toml` presented as plain new functionality | Needs the `sqlx-toml` cargo feature (on in `sqlx-cli`, off in the `sqlx` library) | https://github.com/transact-rs/sqlx/blob/main/CHANGELOG.md |
| 5 | Hyperswitch "~38-crate workspace" | 39 crate directories under `crates/`; workspace is edition 2021 / resolver 2, so it is prior art for scale but not for edition 2024 | https://github.com/juspay/hyperswitch |
| 6 | "Sources for every version: crates.io" | Not true for `dist` 0.33.0: that is a GitHub release (2026-09-11); the crates.io `cargo-dist` crate is at 0.32.0. Same for Wild, mold, Meilisearch, OpenFGA | https://github.com/axodotdev/cargo-dist/releases ; https://crates.io/crates/cargo-dist |
| 7 | `evalexpr` 13.1.0 listed with no licence note | AGPL-3.0-only since 12.0.0 (2024-10); 11.x was MIT. Flagged for licence-policy review | https://crates.io/crates/evalexpr ; https://github.com/ISibboI/evalexpr |
| 8 | "`cargo-component` is effectively superseded" stated as [V] | Verified facts are last release 0.21.1 (2025-04-07) and last push 2025-07-14; "superseded" is an inference [I] | https://github.com/bytecodealliance/cargo-component |
| 9 | Authn row "All maintained" | Kept, with repo push dates added (openidconnect last pushed 2025-11, the slowest) | `gh api` repo metadata |
| 10 | Cargo CVE line gave no severity | CVE-2026-5222 is low, CVE-2026-5223 is medium; both third-party registries only | https://blog.rust-lang.org/2026/05/25/cve-2026-5222/ ; https://blog.rust-lang.org/2026/05/25/cve-2026-5223/ |
| 11 | [G] items: `#[sqlx::test]` fresh database per test; rust_decimal 96-bit / 28 digits; `serde_yaml` archived; licences of `mail-auth`/`mail-send` | Upgraded to [V]: sqlx `test.md`; rust_decimal README and `MAX_SCALE = 28`; serde-yaml repo archived (last release 0.9.34+deprecated); mail-auth, mail-send, mail-parser, mail-builder are all Apache-2.0 OR MIT on crates.io | https://github.com/transact-rs/sqlx/blob/main/src/macros/test.md ; https://github.com/paupino/rust-decimal ; https://github.com/dtolnay/serde-yaml ; https://crates.io |
| 12 | "io_uring runtimes ... not compatible with axum/sqlx" stated without a label | Labelled [I] (no source checked) | n/a |

### Additions

- 1.98.1 vtable bug (#161441) is a stable-to-stable regression introduced in 1.98.0 (works on 1.97.1), so 1.98.0 should be skipped. Source: https://github.com/rust-lang/rust/issues/161441
- WASI 0.3.2 is planned for 2026-10-13 (bi-monthly train on the second Tuesday). Source: https://wasi.dev/roadmap
- Wasmtime LTS evidence: 36.0.16 and 48.0.3 patch releases shipped on 2026-09-24 alongside 49.0.1. Source: https://github.com/bytecodealliance/wasmtime/releases
- OpenFGA moved to CNCF Incubating on 2025-10-28. Source: https://www.cncf.io/projects/openfga/
- SeaORM 2.0.0 date nuance (crates.io and CHANGELOG 2026-07-19; GitHub release object 2026-07-20T00:10Z UTC) added under Discrepancies.
- The three sparse-index-only versions in section 5 (axum-login 0.18.0 at 2025-07-20, tower-sessions 0.15.0 at 2026-02-01, typst-as-lib 0.16.0 at 2026-06-29) were re-read from the crates.io API and are exact, not approximate.

### Unresolved / not independently verifiable

- Parallel front-end: the goal file lists the stabilisation report as a pending work item and the tracking issue #113349 is open (updated 2026-09-16), but I could not read the compiler-team FCP state. "Report not yet filed" rests on the goal text (unverified beyond that).
- The two currently supported Wasmtime LTS lines (v36 and v48): the official support table is rendered by JavaScript and was not readable; inferred from the policy (multiples of 12, two LTS at a time) and the 2026-09-24 patch releases (I).
- Rust 1.99.0: RELEASES.md on master dates it 2026-10-01, but at check time there was no `channel-rust-1.99.0.toml` (404) and no blog post. Treat as releasing today; the dist stable channel is 1.98.1.
- Odoo record-rule semantics (`ir.rule` domain filters applied to list queries): `ir_rule.py` was not found at its old path in Odoo master, so this was not re-read; the claim matches long-standing documented behaviour (unverified against master). `float_round`, `decimal.precision` and `name_search` were confirmed to exist in master.
- "GitHub search for Rust ERPs finds nothing credible": a repository search for "erp language:rust" sorted by stars gave top hits of 53, 46, 40, 27, 23 stars (several are not ERPs); the "nothing credible" judgement is subjective.
- Remaining [G] items in section 5 (mimalloc-on-musl advice, runtime OpenAPI building in utoipa, `gettext-rs` and musl friction, inventory/linkme loader-order caveats, Extism component-model status, MiniJinja resource limits, garde vs validator ergonomics) were not checked.
- Compile-time guidance and the "about 1 kLOC in-house queue" sizing are reasoned estimates, not measured.
