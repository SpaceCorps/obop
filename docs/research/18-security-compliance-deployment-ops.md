# obop research 18: Security, compliance, deployment and operations

Date of research: 2026-10-01. Author: research agent (security/compliance/ops track).

## Scope & method

Only WebFetch was available (no WebSearch), so I fetched about 95 primary pages directly: Postgres, Kubernetes, Odoo (docs source and GitHub), OWASP, NIST, W3C, EU Commission, CNCF, Sigstore, SLSA, crates.io and GitHub release pages. GitHub's REST API returned 403; I used github.com release pages and raw files instead. Tags used throughout: **[V]** verified in this session from the cited URL; **[I]** my inference or design judgement; **[M]** from memory and not verified today (treat as a lead, not a fact). Version numbers are point-in-time readings from 2026-10-01. One fetch of a third-party attack-technique write-up on SAML was refused by the harness safeguard, so SAML library risk below rests on the library README and general knowledge only.

---

## 1. Threat model for a multi-tenant ERP

**Assets.** Ledger and tax data, inventory/BOM/pricing (trade secrets), HR/payroll PII, customer and vendor PII, credentials/API keys/integration secrets, backups, the release binary itself, and (if POS/e-commerce is used) card data, which obop should never hold.

**Actors.** Anonymous internet; authenticated low-privilege users (portal customers, vendors, employees); a malicious or compromised tenant admin (tenant-vs-tenant); insiders committing fraud; third-party extension authors (the Odoo-style app model means module code runs with the platform's privileges); the hosting operator/DBA; compromised dependencies or CI.

**Trust boundaries.** Browser to API; API to Postgres; API to object storage; API to outbound networks (webhooks, email, payment providers, IdPs, LDAP); background workers; report/PDF renderer; admin CLI; and in Tauri local mode the IPC layer between Rust core (full system access) and the WebView (only via IPC and capabilities) [V] https://v2.tauri.app/security/.

| # | Threat | ERP-specific shape | Primary controls (sections) |
|---|---|---|---|
| T1 | Broken object/record-level authz (IDOR/BOLA) | Fetch-by-id endpoints skip the record rules that list views apply | One query builder for list and by-id; non-leaking 404 (3, 10) |
| T2 | Cross-tenant leak | Missing `tenant_id` filter, pooled connection with stale tenant context | Tenant-scoped transaction type + RLS backstop (4) |
| T3 | Privilege-bypass code paths | "sudo"-style elevation, raw SQL, public RPC methods | Typed elevation token, no raw SQL outside audited module (3) |
| T4 | Injection | Dynamic domains/filters, report builders, user templates, formula/expression fields | Typed query AST, sandboxed expression language (10) |
| T5 | SSRF | Webhooks, "import from URL", avatar/logo fetch, HTML-to-PDF rendering | Egress proxy + allowlist, no file/network access in renderer (10) |
| T6 | Supply-chain compromise | Crates, npm registry packages (installed via pnpm), CI, release signing | Section 7 |
| T7 | Insider fraud / record tampering | Back-dating entries, editing posted documents, missing segregation of duties | Immutable posting, hash-chained audit, approval rules (6) |
| T8 | Secret/backup theft, curious DBA | Integration tokens, bank details, payroll in dumps | Envelope + field encryption, encrypted backups (5, 9) |
| T9 | Abuse/availability | Heavy reports, imports, zip/XML bombs, API scraping, noisy tenant | Quotas, concurrency limits, body limits (11) |
| T10 | Privacy failure | Unanswered DSAR, over-retention, unlawful transfer | Section 8 |
| T11 | Bulk exfiltration | Export/API/report engines ignore field-level ACLs | Field ACL enforced in serializer and export path (3) |
| T12 | Failed upgrade/restore | Destructive migration, untested backup | Expand/contract, restore drills (9) |
| T13 | Untrusted extensions | Automation "code" fields and third-party modules | No arbitrary code in data; capability-scoped modules (3, 10) |

OWASP's Top 10:2025 ordering confirms where to spend effort: A01 Broken Access Control, A02 Security Misconfiguration, A03 Software Supply Chain Failures (new emphasis), A04 Cryptographic Failures, A05 Injection, A06 Insecure Design, A07 Authentication Failures, A08 Software or Data Integrity Failures, A09 Security Logging and Alerting Failures, A10 Mishandling of Exceptional Conditions [V] https://top10.owasp.org/2025. ASVS 5.0.0 is the current verification standard (released 2025-05-30, confirmed from the GitHub release timestamp; the release page itself shows only "30 May"), restructured into 17 chapters V1-V17 (Encoding, Validation/Business Logic, Web Frontend, API, File Handling, Authentication, Session, Authorization, Self-contained Tokens, OAuth/OIDC, Cryptography, Secure Communication, Configuration, Data Protection, Secure Coding/Architecture, Logging/Error Handling, WebRTC). Level 1 shrank to 70 of 345 requirements (20%), and CWE/NIST direct mappings were dropped [V] https://github.com/OWASP/ASVS/releases, https://github.com/OWASP/ASVS/tree/master/5.0/en, https://raw.githubusercontent.com/OWASP/ASVS/master/5.0/en/0x05-For-Users-Of-4.0.md.

---

## 2. Authentication

| Mechanism | State of the art (verified) | Rust options and status | Odoo Community parity |
|---|---|---|---|
| Passwords | NIST SP 800-63B-4 (page stamped 26 Aug 2025): min 15 chars if sole factor (8 if inside MFA), accept 64+, no composition rules, no periodic rotation, mandatory blocklist check, rate-limit failed attempts (cap 100 consecutive) [V] https://pages.nist.gov/800-63-4/sp800-63b.html. OWASP: Argon2id minimum m=19456 KiB, t=2, p=1 (equivalents listed), PHC string format, pepper optional and stored outside the DB [V] https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html | `argon2` 0.6.0 (2026-08-27) [V] https://crates.io/api/v1/crates/argon2 | `auth_password_policy` |
| Passkeys/WebAuthn | WebAuthn Level 3 became a W3C Recommendation on 25 Aug 2026 (related origins, JSON serialization helpers, signal methods, `prf`/`largeBlob`) [V] https://www.w3.org/TR/webauthn-3/. NIST: syncable authenticators allowed at AAL2, not AAL3 (derived from: syncable keys are inherently exportable; exportable keys are usable at AAL2 or below; AAL3 needs non-exportable) [V] https://pages.nist.gov/800-63-4/sp800-63b/authenticators/ | `webauthn-rs`: stable 0.5.5, dev line 0.6.1-dev (both published 2026-04-30), MPL-2.0, README says it passed a security audit by SUSE product security and explicitly makes no standards-compliance claim (no FIDO certification claimed) [V] https://github.com/kanidm/webauthn-rs, https://crates.io/api/v1/crates/webauthn-rs | `auth_passkey` exists in master [V] https://github.com/odoo/odoo/tree/master/addons |
| TOTP | Baseline second factor; NIST/GitHub direction is to prefer FIDO (GitHub plans to deprecate TOTP for 2FA on npm registry accounts; a registry-account policy, not an npm-CLI feature) [V] https://github.blog/security/supply-chain-security/our-plan-for-a-more-secure-npm-supply-chain/ | RFC 6238 is trivial to implement on `hmac`/`sha1`; no crate verified | `auth_totp` |
| OIDC | Standard SSO path | `openidconnect` 4.0.1, last release 2025-07-06 (slow cadence, high usage: 14.2M downloads) [V] https://crates.io/api/v1/crates/openidconnect | `auth_oauth` |
| SAML 2.0 | Enterprise requirement, highest implementation risk (XML signature handling) [M] | `samael` 0.0.22 (2026-07-07): README calls itself work in progress, MIT, signing/verification needs the `xmlsec1` C library via a modified rust-xmlsec, encrypted-assertion algorithm support is narrow [V] https://github.com/njaremko/samael | none in Community listing (no `auth_saml` directory) [V] |
| SCIM 2.0 | RFC 7644: `/Users`, `/Groups`, optional PATCH (add/remove/replace), filters, Bulk, ETags; auth left to TLS plus OAuth bearer, basic discouraged [V] https://www.rfc-editor.org/rfc/rfc7644 | No server crate verified; expect to build a subset | none |
| LDAP | Bind-auth against AD/OpenLDAP | `ldap3` 0.12.1 pure Rust (2025-09-18) [V] https://crates.io/api/v1/crates/ldap3 | `auth_ldap` |
| Idle timeout | Session hygiene | n/a | `auth_timeout` |

**Recommendations [I].** Ship local password + TOTP + passkeys + OIDC first. Treat SAML as an isolated component (separate process or external bridge such as Keycloak/Authentik) because the Rust ecosystem option is pre-1.0 and C-linked; never hand-roll XML-DSig. Use opaque, server-side, revocable sessions in Postgres for browsers (ASVS 5.0 separates session management V7 from self-contained tokens V9), hashed and prefix-identifiable API keys for machines, and step-up re-auth for sensitive actions. Postgres 18 itself gained an `oauth` auth method and deprecates `md5` passwords [V] https://www.postgresql.org/about/news/postgresql-18-released-3142/; require `scram-sha-256` for the app role.

---

## 3. Authorization

### 3.1 What Odoo does (the behaviour to clone) [V] https://raw.githubusercontent.com/odoo/documentation/19.0/content/developer/reference/backend/security.rst

- **Access rights** (`ir.model.access`): per model, per group, create/read/write/unlink; additive across groups; a row with empty group applies to everyone including portal/public.
- **Record rules** (`ir.rule`): domain expressions per model, evaluated after access rights. **Default-allow**: if ACL passes and no rule applies, access is granted. Global rules intersect; group rules union; the two sets then intersect, so adding the first group rule to a global ruleset restricts access.
- **Field access**: `groups` on a field removes it from views and `fields_get` and errors on explicit read/write.
- **Pitfalls Odoo documents**: any public method is RPC-callable with attacker-chosen arguments (underscore-private methods are not), raw cursors bypass ACLs, concatenated SQL, domain injection when lists are built by hand, unescaped `Markup`, `eval`/`safe_eval`, `getattr` on user-controlled names.

Design defects to **not** clone [I]: default-allow record rules, implicit public RPC surface, and unrestricted `sudo`-style elevation (the doc page did not describe `sudo`; this is [M]).

### 3.2 Engine options

| Option | Fit | Evidence |
|---|---|---|
| Native RBAC + record rules + field groups in Rust, rules compiled to SQL predicates | Required for list views, search, reporting, exports (filtering must happen in the query) | [I] |
| **Cedar** (AWS-originated, Rust, Apache-2.0) | Good for action-level ABAC ("approve PO over X if role and amount"), schema-validated, analyzable. Needs entities and context supplied per request; list-filtering needs partial evaluation (`tpe`, "type-aware partial evaluation"), still labelled an experimental feature in the v4.13.0 (2026-09-15) changelog; CNCF Sandbox since 2025-10-08; v4.12/4.13 changed experimental APIs and JSON output (4.13.0 changed `has a.b.c` chains to a single JSON `has` node) | [V] https://github.com/cedar-policy/cedar/releases, https://raw.githubusercontent.com/cedar-policy/cedar/main/cedar-policy/CHANGELOG.md, https://www.cncf.io/projects/cedar/, license per https://crates.io/api/v1/crates/cedar-policy |
| **OpenFGA** (Go, Zanzibar-style ReBAC) | Best for sharing graphs (documents, portals). External Go service breaks "single Rust binary + Postgres". CNCF Incubating since 2025-10-28; v1.21.0 (2026-09-20). 2026 releases include fixes for authz-result bugs in StreamedListObjects (v1.19.0, GHSA-5278-rrxc-mgf7) and ListUsers (v1.20.0, GHSA-h7w8-xr72-cv4r), a reminder that policy engines add their own vulnerability surface | [V] https://www.cncf.io/projects/openfga/, https://github.com/openfga/openfga/releases |
| `casbin` crate 2.20.0 (ACL/RBAC/ABAC) | Possible but no advantage over native | [V] https://crates.io/api/v1/crates/casbin |

**Recommendation [I].** Native engine with Odoo semantics but default-deny for both ACLs and record rules; rules expressed as a typed predicate AST that compiles to SQL and is also used by the by-id fetch path (IDOR becomes structurally impossible if there is no separate by-id code path). Optionally embed Cedar later for approval/ABAC decisions that do not filter lists. Do not depend on OpenFGA in core.

**Mass assignment and field leakage [I].** Separate input and output DTOs per operation; `#[serde(deny_unknown_fields)]`; field-level read/write ACL enforced in one serializer used by API, export, report and webhook payloads. IDs: Postgres 18's `uuidv7()` embeds a timestamp, so ids are guessable in part and must never be treated as capabilities [V] https://www.postgresql.org/about/news/postgresql-18-released-3142/.

---

## 4. Tenant isolation

| Model | Isolation | Ops cost | Notes |
|---|---|---|---|
| Database per tenant (Odoo model: `dbfilter` selects DB by hostname; Odoo advises containers/jails when hosting several customers) | Strongest | Highest (migrations fan out) | Per-tenant PITR, erasure, residency are trivial [V] https://raw.githubusercontent.com/odoo/documentation/19.0/content/administration/on_premise/deploy.rst |
| Schema per tenant | Moderate | Catalog bloat, complex migrations | AWS calls this "bridge" |
| Shared schema + `tenant_id` + RLS ("pool") | Weakest unless enforced by DB | Lowest | AWS pattern uses a session variable and policy `current_setting(...)` [V] https://aws.amazon.com/blogs/database/multi-tenant-data-isolation-with-postgresql-row-level-security/ |

**Facts that constrain an RLS design [V]** https://www.postgresql.org/docs/current/ddl-rowsecurity.html:
- Superusers, `BYPASSRLS` roles and (unless `FORCE ROW LEVEL SECURITY`) table owners bypass policies; the app must connect as a non-owner, non-bypass role.
- Policies are default-deny once enabled; `USING` filters silently, `WITH CHECK` errors.
- Referential-integrity checks bypass RLS, so FK/unique errors can leak existence of hidden rows. Policies that read other tables suffer race conditions under READ COMMITTED and performance cost. Keep policies to a single-column tenant predicate.
- `row_security = off` makes queries error instead of silently filtering: use it for backups.
- `set_config(name, value, true)` is transaction-local; `current_setting(name)` errors on unknown names, `missing_ok=true` returns NULL [V] https://www.postgresql.org/docs/current/functions-admin.html. After a local setting is reverted it may read as empty string rather than NULL [general PG behaviour, flagged by the fetch as not on that page, so test it]; write policies as `tenant_id = nullif(current_setting('obop.tenant', true), '')::uuid` so an unset context yields no rows.
- PgBouncer transaction pooling: session `SET` of untracked parameters can leak between clients unless `server_reset_query_always` is on; `max_prepared_statements` (default 200) tracks protocol-level prepared statements only [V] https://www.pgbouncer.org/config.html. Therefore use `SET LOCAL`/`set_config(...,true)` inside every transaction, never session-level. Run PgBouncer >= 1.26.0: released 2026-09-23, it fixes three CVEs (CVE-2026-19888 unauthenticated remote crash via a SCRAM client-final-message without a nonce, CVE-2026-6668 packet-buffer overflow hang, CVE-2026-6669 unbounded SCRAM iteration count) and now tracks `search_path` (PG18+) by default [V] https://www.pgbouncer.org/changelog.html. The AWS RLS article itself warns that session variables may not work behind server-side pooling such as PgBouncer [V].

**Recommendation [I].** Support two deployment modes behind one code path: *silo* (DB per company, default for self-host and regulated customers) and *pool* (shared schema, `tenant_id` first column of every PK and index, composite FKs `(tenant_id, id)` so cross-tenant references are impossible, RLS with `FORCE`). In Rust, make the only way to obtain a DB handle a `TenantTx` that begins a transaction, drops to the restricted role (`SET LOCAL ROLE`), and sets the tenant; forbid raw pool access via module visibility and a clippy/`cargo-deny`-style CI lint. Add a CI test that runs the whole suite as tenant A and asserts zero rows of tenant B through every endpoint.

---

## 5. Secrets and encryption

- **In transit.** `rustls` (0.23.45 stable; 0.24 in development) [V] https://crates.io/api/v1/crates/rustls; `aws-lc-rs` 1.18.1 is the ring-compatible provider [V] https://crates.io/api/v1/crates/aws-lc-rs (rustls's default feature set enables `aws_lc_rs`, so it is the default provider [V] https://docs.rs/crate/rustls/latest/features). Postgres connections `verify-full`. Postgres 18 added `ssl_tls13_ciphers` and FIPS-mode validation [V].
- **At rest, database.** Core PostgreSQL has **no TDE**; its docs list pgcrypto column encryption (key and plaintext visible to the server), filesystem/block encryption (protects only stolen media) and client-side encryption [V] https://www.postgresql.org/docs/current/encryption-options.html. Percona's `pg_tde` (2.2.2, 2026-08-20) is bundled with Percona Server for PostgreSQL (it needs that server's patches; packaged in Percona Distribution for PostgreSQL 17 and later), not vanilla Postgres, supporting KMIP, Vault and OpenBao key stores [V] https://docs.percona.com/pg-tde/index.html. Postgres 18 enables data checksums by default [V].
- **Field-level (application) encryption [I].** Envelope scheme: KMS-held KEK per tenant, random DEK per record/field, AES-256-GCM (or XChaCha20-Poly1305), with AAD = `tenant_id | table | column | row_id` to defeat ciphertext swapping. AWS KMS documents this pattern: FIPS 140-3 Level 3 HSMs, AES-GCM-256, GenerateDataKey, ML-DSA keys now available [V] https://docs.aws.amazon.com/kms/latest/developerguide/kms-cryptography.html. Self-hosters need a non-cloud KEK source: OpenBao (fork of Vault, Linux Foundation/OpenSSF Sandbox project, MPL 2.0 code) [V] https://openbao.org/, or a local sealed key file via an `age`-style wrapper. Define a small catalog of encrypted fields (API tokens, OAuth refresh tokens, bank account numbers, national IDs, payroll) since encrypted columns cannot be searched; use HMAC blind indexes where lookup is needed.
- **Crates to avoid or watch.** `rsa` has an unpatched Marvin timing side-channel (RUSTSEC-2023-0071, CVSS 5.9, still open as of 2026-09-12 with `patched = []` intentional; `rsa` 0.9.10 (latest stable) and 0.10.0-rc.18 are both listed as affected, advisory page last modified 2026-09-14): do not use RSA decryption/PKCS#1 v1.5 signing on network-observable paths; prefer Ed25519/ES256 [V] https://rustsec.org/advisories/RUSTSEC-2023-0071.html. `sqlx` RUSTSEC-2024-0363 (fixed in >=0.8.1; current 0.9.0) shows wire-protocol length overflow can become query smuggling with >4 GiB values: cap request/field sizes [V] https://rustsec.org/advisories/RUSTSEC-2024-0363.html.
- **Secrets handling [I].** No secrets in the DB in plaintext, none in env dumps or logs; config supports file/`_FILE`, Kubernetes secrets, Vault/OpenBao, cloud secret managers. Rotatable with overlap (two active KEKs). Password pepper only if stored outside the DB (OWASP).
- **Release hardening flags [V]** https://doc.rust-lang.org/cargo/reference/profiles.html: `overflow-checks` defaults off in release: **turn it on** for money arithmetic (or use integer minor units/decimal types). `panic = "abort"` would kill a whole multi-tenant worker on one request's panic; prefer unwind and catch at the HTTP layer [I].

---

## 6. Audit logging and tamper evidence

Odoo Community ships chatter tracking but no general security audit log; the OCA `auditlog` add-on is Beta, AGPL-3, logs create/read/write/delete per subscribed model, cannot restrict by user, and has an optional 180-day vacuum [V] https://github.com/OCA/server-tools/tree/18.0/auditlog. pgAudit (PostgreSQL License) provides session/object audit into the PG log [V] https://www.pgaudit.org/. Both are insufficient for ERP fraud evidence.

**Design [I].**
1. **Business change log** (who changed what field, old/new) written in the same transaction as the change.
2. **Security event log** (login, MFA, permission change, export, impersonation, elevation, key use, admin CLI) in an append-only table: app role has INSERT only, triggers reject UPDATE/DELETE, partitioned by month.
3. **Tamper evidence:** per-tenant hash chain (`h_n = H(h_{n-1} || canonical(record))`), periodic signed checkpoints (Ed25519, key in KMS) exported to WORM storage (S3 Object Lock, which pgBackRest also leverages) or a transparency log. Reference designs: Sigstore's Rekor v2 reached GA 2025-10-10 on a Tessera tile-based log [V] https://blog.sigstore.dev/rekor-v2-ga/; Tessera is a Go library, "production ready" since v0.2.0, with GCP/AWS/POSIX drivers [V] https://github.com/transparency-dev/tessera. For obop, a Rust hash chain plus optional checkpoint publication is enough; do not embed a Go log.
4. **Financial immutability:** posted journal entries are append-only; corrections via reversal entries; lock dates; sequence gaps detectable. (Odoo's French-localization hash inalterability is [M].)
5. **Privacy interplay:** store actor as pseudonymous id resolvable via a deletable mapping so erasure does not break the chain.
6. **Logging hygiene:** ASVS V16; never log secrets/PANs; A09 and A10 (error handling) in OWASP 2025.

---

## 7. Supply chain

### 7.1 Rust (versions as of 2026-10-01)

| Control | Tool / fact |
|---|---|
| Advisory scanning | `cargo-audit` 0.22.2 (2026-06-05) on the RustSec DB (maintained by the Rust Secure Code WG, exported to OSV and GitHub Advisory DB) [V] https://rustsec.org/, https://crates.io/api/v1/crates/cargo-audit |
| Policy gate | `cargo-deny` 0.20.2 (2026-07-09): advisories, bans, licenses, sources checks; GitHub Action available [V] https://embarkstudios.github.io/cargo-deny/ |
| Human review | `cargo-vet` 0.10.2 (2026-01-13): relative audits, shared audits, exemption list that shrinks [V] https://mozilla.github.io/cargo-vet/ |
| Binary provenance | `cargo-auditable` 0.7.6 (2026-09-13) embeds dependency list in the binary so scanners can inspect deployed artifacts (`cargo audit bin`) [V] https://github.com/rust-secure-code/cargo-auditable, https://crates.io/api/v1/crates/cargo-auditable |
| SBOM | `cargo-cyclonedx` 0.5.9 (2026-03-19) [V]; CycloneDX is at 1.7 (2025-10-21); ECMA-424 2nd edition (December 2025, published 2025-12-10) defines v1.7 and the 1st edition dates from June 2024 [V] https://cyclonedx.org/specification/overview/, https://ecma-international.org/publications-and-standards/standards/ecma-424/. **`cargo-cyclonedx` 0.5.9 cannot emit 1.7:** its README lists `--spec-version` values 1.3, 1.4, 1.5 and defaults to 1.3 [V] https://github.com/CycloneDX/cyclonedx-rust-cargo/blob/main/cargo-cyclonedx/README.md; pass `--spec-version 1.5` explicitly or use another generator if 1.6/1.7 is required |
| Signing/attestation | GitHub artifact attestations give SLSA v1.0 Build L2; L3 via shared reusable workflow; Sigstore public-good instance for public repos; must actually run `gh attestation verify` [V] https://docs.github.com/en/actions/concepts/security/artifact-attestations. SLSA 1.2 is current, 1.1 retired [V] https://slsa.dev/spec/v1.1/levels. Cosign >=2.6.0 / 3.0.1 for Rekor v2 [V] |
| Registry publishing | crates.io has trusted publishing (OIDC, July 2025) per GitHub's list [V]; publish only from CI |
| Reproducible builds | Cargo's `trim-paths` profile option is still documented as **unstable** [V] https://doc.rust-lang.org/cargo/reference/unstable.html; use `--remap-path-prefix`, pinned `rust-toolchain.toml`, `--locked`, containerised builds, and rebuild twice in CI comparing hashes [I] |
| Toolchain | Rust 1.98.1 (2026-09-03) is the newest on the blog index [V] https://blog.rust-lang.org/releases/; releases.rs shows **1.99.0 as current stable** and 1.100.0 due 2026-11-12 [V] https://releases.rs/. The `rust-lang/rust` RELEASES.md on master is headed "Version 1.99.0 (2026-10-01)" [V] https://raw.githubusercontent.com/rust-lang/rust/master/RELEASES.md, so 1.99.0 is dated today (6-week cadence after 1.98.0 on 2026-08-20). Caveat: at about 10:09 UTC on 2026-10-01 the official stable channel manifest still reported rustc 1.98.1 and beta reported 1.100.0-beta.1 [V] https://static.rust-lang.org/dist/channel-rust-stable.toml, so 1.99.0 may not yet have been published on the stable channel when this was checked; pin `rust-toolchain.toml` to a version that is actually downloadable |

Also pin exact versions for security-sensitive, fast-moving crates (Cedar's README itself describes pinning to a specific version to avoid automatic updates) [V] https://github.com/cedar-policy/cedar, and treat pre-1.0 crates in the request path (`opentelemetry` 0.33.0, `samael` 0.0.22, `tower_governor` 0.8.0) as upgrade liabilities.

### 7.2 pnpm / Vite+

- **Vite+ 1.0** is released, MIT, one `vp` CLI bundling Vite, Rolldown, Vitest, Oxlint, Oxfmt, tsdown; it supports pnpm/npm/yarn/bun, selects the package manager from the `packageManager` field and manages Node [V] https://viteplus.dev/. **obop uses pnpm only (owner constraint, 2026-10-01).** Vite+ resolves the manager at the workspace root in this order: `packageManager`, `devEngines.packageManager`, `pnpm-workspace.yaml`, `pnpm-lock.yaml`, then the other managers' lockfiles, and falls back to pnpm when nothing is found; `vp env pin pnpm@<version>` pins the version, and `vp install`, `vp add`, `vp remove` and `vp pm <command>` (for example `vp pm audit`) go through the resolved pnpm [V] https://viteplus.dev/guide/install, https://viteplus.dev/guide/env.
- Choose **pnpm**: since v10 dependency `postinstall` scripts are blocked by default (allow-list via `allowBuilds`, added v10.26.0; `strictDepBuilds` defaults to true, so an unreviewed build script fails the install; v11 removed the older `onlyBuiltDependencies`); `minimumReleaseAge` defaults to 1440 minutes (one day) since v11 (0 before; the built-in default is non-strict unless set explicitly, `minimumReleaseAgeStrict`); `trustPolicy: no-downgrade` rejects releases with weaker provenance than earlier ones and is **off by default** (added v10.21.0), so it must be set explicitly; `blockExoticSubdeps` (default true, added v10.26.0) blocks git/tarball transitive deps. Caveat: `pnpm-lock.yaml` is a two-document format; scanners that read only the first document report zero dependencies without error [V] https://pnpm.io/supply-chain-security, https://pnpm.io/settings/dependency-resolution, https://pnpm.io/settings/build. Verify your SBOM/scanner handles it. Audit with `vp pm audit` (= `pnpm audit`, which queries the registry's bulk advisory endpoint from v11; `pnpm audit signatures` verifies registry signatures since v11.1.0) [V] https://pnpm.io/cli/audit.
- The npm registry threat is live (it reaches obop through pnpm installs): the Shai-Hulud self-replicating worm (2025-09-14) led GitHub to pull 500+ packages and plan tokenless publishing [V] https://github.blog/security/supply-chain-security/our-plan-for-a-more-secure-npm-supply-chain/.
- **npm trusted publishing is npm-CLI-specific and not applicable to obop (pnpm-only, owner constraint 2026-10-01).** For the record, npm documents it only for the npm CLI (>=11.5.1, Node >=22.14.0; GitHub Actions, GitLab CI, CircleCI; provenance generated automatically only for public repos/packages) and never mentions pnpm [V] https://docs.npmjs.com/trusted-publishers. The pnpm-side controls that replace it are only partly documented: since v11 `pnpm publish` is implemented natively and no longer delegates to the npm CLI (so no npm CLI version floor applies), it has a `--provenance` flag ("When publishing from a supported cloud CI/CD system, the package will be publicly linked to where it was built and published from"), and `pnpm stage publish` / `pnpm stage approve` (v11.3.0) implement npm's staged publishing with the 2FA one-time-password deferred to the approve step [V] https://pnpm.io/cli/publish, https://pnpm.io/cli/stage. The pnpm docs do **not** document OIDC trusted publishing for `pnpm publish`; pnpm's own release workflow uses `id-token: write` with `pnpm stage publish` for its CLI package, and the closed issues pnpm/pnpm#11513 and #11566 report OIDC publishing working on pnpm 11 after fixes [V, workflow and issues read via `gh`; the end-to-end behaviour for a third-party repo is unverified]. Rule [I]: obop ships a Rust binary with embedded assets, so it should publish no JS packages at first; if it ever publishes one (for example an SDK), publish only from CI with `pnpm publish --provenance` or `pnpm stage publish`, test the OIDC path before relying on it, never fall back to a long-lived token, and never use the documented `pnpm pack && npm publish` fallback (npm is not used by obop).
- Frontend rules [I]: commit `pnpm-lock.yaml` (never `package-lock.json`, `yarn.lock` or `bun.lock`), CI installs frozen with `vp install --frozen-lockfile` (pnpm also freezes by default when it detects CI and a lockfile exists [V] https://pnpm.io/cli/install; CI setup via `voidzero-dev/setup-vp` pinned to an exact release, not the stale `v1` tag [V] https://viteplus.dev/guide/ci), no runtime CDN scripts, strict CSP with no inline script, Subresource Integrity if any external asset, embed built assets in the Rust binary.

### 7.3 Our own obligations: EU Cyber Resilience Act

In force 2024-12-10; **vulnerability/incident reporting applies from 2026-09-11** (early warning 24 h, notification 72 h, final report 14 days after a corrective measure is available for exploited vulnerabilities, or within one month of the 72 h notification for severe incidents; submitted once via ENISA's Single Reporting Platform, which became operational on 2026-09-11); full obligations from 2027-12-11; open-source stewards fall under Article 24 (the Commission's reporting page gives Article 24(3) reporting for stewards from 2027-12-11) and are exempt from administrative fines under Art. 64(10); non-monetised FOSS is outside "made available on the market" [V] https://digital-strategy.ec.europa.eu/en/policies/cyber-resilience-act, .../cra-reporting, .../cra-open-source. **Implication [I]:** if any legal entity sells obop support, hosting or paid editions, it is a manufacturer with reporting duties as of now; publish SECURITY.md, a coordinated-disclosure address, SBOMs per release, and a support-period statement.

---

## 8. Privacy and compliance

### 8.1 GDPR / CCPA engineering features

| Requirement | Fact | obop feature [I] |
|---|---|---|
| Access (Art. 15) and deadline (Art. 12(3)) | Copy of data plus purposes, recipients, retention, source, transfers; respond within one month, extendable by two further months with notice in month one [V] https://gdpr-info.eu/art-15-gdpr/, https://gdpr-info.eu/art-12-gdpr/ | DSAR exporter driven by a model registry: each model declares personal-data fields and the path to a "data subject" partner. Odoo already has a `privacy_lookup` module (LGPL-3) in Community [V] https://raw.githubusercontent.com/odoo/odoo/master/addons/privacy_lookup/__manifest__.py; match that |
| Erasure (Art. 17) | Not required where processing is necessary for a legal obligation or for legal claims (17(3)(b),(e)) [V] https://gdpr-info.eu/art-17-gdpr/. CCPA 1798.105(d) has parallel exceptions (complete transaction, security/integrity, legal obligation) [V] https://leginfo.legislature.ca.gov/faces/codes_displaySection.xhtml?lawCode=CIV&sectionNum=1798.105 | Erasure = **anonymise in place** when posted documents reference the person: blank PII fields, keep ledger rows, record legal basis and retention expiry, hard-delete when retention lapses |
| Retention conflicts | Germany HGB §257: books 10 y, Buchungsbelege 8 y (10 y for the KWG/VAG/WpIG-regulated financial-sector entities listed in §257(4)), Handelsbriefe 6 y [V] https://www.gesetze-im-internet.de/hgb/__257.html; UK companies 6 y [V] https://www.gov.uk/running-a-limited-company/company-and-accounting-records; US IRS 3 y default, 6 y, 7 y, employment tax 4 y [V] https://www.irs.gov/businesses/small-businesses-self-employed/how-long-should-i-keep-records | Retention class per model and per country (localisation pack), scheduled purge job (Odoo has `data_recycle`) |
| Security of processing (Art. 32) | Pseudonymisation and encryption, CIA/resilience, timely restoration, regular testing [V] https://gdpr-info.eu/art-32-gdpr/ | Maps to Sections 5, 9 |
| Breach notice (Art. 33) | Controller: 72 h to authority; processor: "without undue delay" to controller [V] https://gdpr-info.eu/art-33-gdpr/ | Hosted obop is a processor: runbook, tenant-level breach export |
| Processor contract (Art. 28(3)) | Documented instructions, sub-processors, assist with data-subject rights, delete or return at end, audits [V] https://gdpr-info.eu/art-28-gdpr/ | Tenant export + verified deletion certificate, sub-processor list, audit-log access |
| Consent | n/a verified | Purpose-scoped, versioned, timestamped consent records; marketing module honours them |
| CCPA/CPRA | Regulations incl. risk assessments, cybersecurity audits, ADMT approved by OAL 2025-09-22, effective 2026-01-01 [V] https://cppa.ca.gov/regulations/ccpa_updates.html | Expose tenants' opt-out/limit flags; phase-in dates of individual audit duties not verified |

**Backups vs erasure [I].** PITR restores the whole cluster [V] https://www.postgresql.org/docs/current/continuous-archiving.html, so keep an append-only **erasure ledger** and replay it after any restore; combine with per-tenant (or per-subject for high-sensitivity fields) crypto-shredding by destroying the DEK.

### 8.2 Attestations and regimes

| Regime | Status | What it implies for obop |
|---|---|---|
| SOC 2 | Five criteria (Security, Availability, Confidentiality, Processing Integrity, Privacy); Type II typically 9-12 months observation [V, secondary] https://en.wikipedia.org/wiki/System_and_Organization_Controls | Only relevant to a hosted offering; start evidence collection (access reviews, change management, backups) early |
| ISO/IEC 27001:2022 | 93 Annex A controls; Amd 1:2024 (climate); transition from 2013 ended 2025-10-31 [V, secondary] https://en.wikipedia.org/wiki/ISO/IEC_27001 | Cert belongs to the operating org, not the software |
| PCI DSS | v4.0.1 (June 2024) is the only active version; v4.0 retired 2024-12-31; v3.2.1 retired 2024-03-31 [V, secondary] https://en.wikipedia.org/wiki/Payment_Card_Industry_Data_Security_Standard | **Keep PAN out of scope**: POS/e-commerce integrate with certified terminals/P2PE or hosted payment fields, store only tokens, scrub logs. SAQ mapping [M] |
| HIPAA | Security Rule NPRM published 2025-01-06 (mandatory encryption, MFA, asset inventory, annual audits); no final rule as of 2026-07-08, OMB target slipped to July 2027 [V, secondary] https://www.hipaajournal.com/hipaa-security-rule-update/ | Don't market HIPAA readiness in v1; design to the NPRM controls anyway (encryption, MFA, audit) |
| EU Data Act | Applicable since 2025-09-12; includes cloud-switching rules [V] https://digital-strategy.ec.europa.eu/en/policies/data-act | Full tenant export in open formats is a legal as well as product feature |
| EU-US transfers | Adequacy (DPF) adopted 2023-07-10 (Implementing Decision 2023/1795); the Commission page describes it in the present tense and mentions no suspension or annulment, but shows no dated status update [V] https://commission.europa.eu/law/law-topic/data-protection/international-dimension-data-protection/eu-us-data-transfers_en; secondary source reports a June 2026 US Supreme Court ruling (Trump v. Slaughter) and a noyb announcement of an annulment action [V, secondary] https://en.wikipedia.org/wiki/EU%E2%80%93US_Data_Privacy_Framework. General Court *Latombe* outcome/appeal **not verified** | Do not architect around DPF staying valid |

**Data residency [I].** Make region a tenant attribute pinned at provisioning; everything tenant-scoped (DB, object storage, backups, logs, traces, email relay, webhook egress) must be region-local. One deployment per region, no cross-region replication of personal data by default, BYOK/HYOK option.

---

## 9. Deployment, backup, upgrades

### 9.1 Topologies

| Topology | Shape | Security/ops notes |
|---|---|---|
| Single binary + Postgres | systemd unit, embedded static assets | Primary target. Harden with systemd sandboxing and non-root user [M]; TLS at a reverse proxy or built in; `admin` operations by CLI only |
| Docker Compose | app + Postgres + proxy + pgBackRest sidecar | Minimal image (distroless/static), read-only rootfs, secrets via files |
| Kubernetes / Helm | Kubernetes **1.37.1** latest (2026-09-15); 1.34 EOL 2026-10-27 [V] https://kubernetes.io/releases/; Helm 4 (docs list 4.3.0) [V] https://helm.sh/docs/overview/ | Database via CloudNativePG (Apache-2.0; TLS, PGAudit, PITR to object store; CNCF **Sandbox**, accepted 2025-01-21 [V] https://www.cncf.io/projects/cloudnativepg/) or managed Postgres. Separate Deployments for web, workers, cron; NetworkPolicy default-deny egress |
| Bare metal | Patroni-style HA or single node | Full-disk encryption; key via TPM/network-bound unlock [M] |
| Edge | Stateless API near users, Postgres central | Only read-mostly or offline-queue POS; residency rules apply |
| Tauri local mode | Desktop shell + embedded DB | Tauri 2.12.1 stable (published 2026-09-30), 3.0.0-alpha.3 published 2026-09-26 [V] https://crates.io/api/v1/crates/tauri; OS WebView (patched by OS vendor), capabilities/permissions/scopes, CSP and isolation pattern [V]. Embedded Postgres via `postgresql_embedded` 0.21.0 [V] or SQLite. Bind to loopback/unix socket only, random per-install port and token, encrypt DB with key in the OS keychain [I] |

Odoo's own deployment guidance is a good checklist to clone: random master password, disable database manager (`list_db=False`), `dbfilter`, non-superuser DB role, HSTS and `Secure`/`SameSite=Lax` cookie, proxy request-size limits, outbound default-deny, backups copied to a host the app cannot reach, fail2ban on login logs [V] deploy.rst above. Odoo's worker rule of thumb is (CPU x 2) + 1 workers at about 6 concurrent users each [V], useful as a sizing baseline.

### 9.2 Backup, restore, PITR

- Postgres PITR = base backup + continuous WAL archive; `recovery_target_time`; each recovery creates a new timeline; keep `.history` files; **restores the whole cluster only**; config files are not covered; a failing `archive_command` grows `pg_wal` until the server PANICs; PG17+ has `pg_basebackup --incremental` with `pg_combinebackup`, and you must keep every dependent backup [V] https://www.postgresql.org/docs/current/continuous-archiving.html.
- **pgBackRest** v2.59.2 (announced 2026-09-27), MIT, parallel/incremental/block-level, repo encryption, S3/GCS/Azure, S3 object lock/soft delete for ransomware resilience, supports PG versions back five EOL releases; sponsor-funded [V] https://pgbackrest.org/. Default recommendation [I].
- Pool-mode consequence [I]: cluster-wide PITR cannot restore one tenant. Offer per-tenant logical export/restore (and recommend silo mode for tenants needing their own RPO/RTO).
- Postgres support horizon: 18.6 current; 17.11, 16.15, 15.19; **14 EOL 2026-11-12**; 19 in beta 4 (2026-09-24) [V] https://www.postgresql.org/support/versioning/. Target 16/17/18 at launch.
- Restore drills: automated monthly restore-and-verify (checksums are on by default from PG18 initdb [V]), including replay of the erasure ledger.

### 9.3 Zero-downtime upgrades and online migration (expand/contract)

Lock facts from the PG18 ALTER TABLE page [V] https://www.postgresql.org/docs/current/sql-altertable.html:
- `ADD COLUMN` with no default or a non-volatile default: no rewrite (volatile default, stored generated, identity, constrained domain: full rewrite). PG18 virtual generated columns never rewrite.
- `SET NOT NULL` scans under ACCESS EXCLUSIVE unless a validated `CHECK (col IS NOT NULL)` exists; PG18 lets you add `NOT NULL ... NOT VALID` then `VALIDATE CONSTRAINT` (SHARE UPDATE EXCLUSIVE).
- FK: add `NOT VALID`, then validate. `ALTER COLUMN TYPE` normally rewrites (double disk). `DROP COLUMN` is metadata-only.
- `CREATE INDEX CONCURRENTLY` cannot run in a transaction, can leave an INVALID index on failure, one concurrent build per table [V] https://www.postgresql.org/docs/current/sql-createindex.html.
- Even brief ACCESS EXCLUSIVE locks queue behind long transactions: always set `lock_timeout` and retry [I: common practice; the ALTER TABLE page does not mention `lock_timeout`, it only says ACCESS EXCLUSIVE is the default lock and that `NOT VALID` + `VALIDATE CONSTRAINT` shortens heavy-lock time].
- Major upgrades: `pg_upgrade` (PG18 keeps planner stats, adds `--swap`, parallel `--jobs`); logical replication for near-zero-downtime (the PG18 docs state that the schema/DDL commands are not replicated and that sequence data is not replicated) [V] PG18 release page https://www.postgresql.org/about/news/postgresql-18-released-3142/ and https://www.postgresql.org/docs/18/logical-replication-restrictions.html; PG19 is still in beta, so do not assume this restriction holds there (unverified).

**obop migration framework [I].** Every migration is tagged `expand`, `backfill` or `contract`; CI rejects a contract in the same release as its expand; app version N and N+1 must run concurrently against the schema (rolling deploy, readiness probes, graceful drain). Backfills run as throttled jobs. Job queue and cron must tolerate mixed versions (Postgres `SKIP LOCKED` queue, advisory-lock singletons). `pgroll` (Go, Apache-2.0, Postgres 14+, view-based versioned schemas selected via `search_path`, instant rollback) is a reference design rather than a dependency [V] https://github.com/xataio/pgroll; note PgBouncer tracks `search_path` only on PG18+ [V].

---

## 10. ERP-typical ASVS risks, concretely

| Risk | How it shows up in an Odoo-like suite | Control [I unless cited] |
|---|---|---|
| IDOR/BOLA | `/record/{id}` without record-rule predicate; portal users enumerating invoice ids | Single query path (Section 3); 404 not 403; property test: every endpoint x every role x foreign ids |
| Mass assignment | JSON `create/write` accepting `company_id`, `state`, `user_id`, price | Per-operation DTOs, `deny_unknown_fields`, field write ACLs, server-computed fields read-only |
| SSRF | Webhook URLs, "fetch logo from URL", import by URL, report HTML with remote resources | OWASP: allowlist where possible; otherwise resolve A+AAAA and require every IP public, disable redirects, limit schemes to HTTP/HTTPS, network-layer egress policy; its last-resort deny-list minimum names the metadata endpoints (169.254.169.254, metadata.google.internal), loopback, RFC1918 and multicast ranges, so also block the whole link-local range 169.254.0.0/16 [I]; cheat sheet [V] https://cheatsheetseries.owasp.org/cheatsheets/Server_Side_Request_Forgery_Prevention_Cheat_Sheet.html. Route all outbound HTTP through one egress client that pins the resolved IP |
| Webhook integrity | Outbound/inbound forged events | HMAC-SHA256 signature with timestamp, replay window, per-endpoint secret |
| Template/expression injection | Email/report templates, computed-field formulas, automation rules (Odoo: `t-raw`, `Markup`, `safe_eval`) | Logic-less or sandboxed templates with auto-escaping; expression language with no I/O, step/time budget; never compile user text into code (Odoo's docs stress escape vs sanitise and avoiding `eval`) |
| Report rendering | HTML-to-PDF via headless browser or wkhtmltopdf can read local files/network. Odoo master ships both `base_report_wkhtmltox` and an in-house LGPL-3 engine `base_report_paper_muncher` [V] addon listing + manifest | Render in an isolated worker with no network/file access and no JS; prefer a Rust-native layout engine (candidates not evaluated) |
| File upload | Attachments, logos, CSV/XLSX imports | Extension allowlist, server-generated names, storage outside web root served by id-mapped handler, size limits, decompressed-size checks, avoid ZIP, CDR/AV for PDF and Office, image re-encode, authz on every upload [V] https://cheatsheetseries.owasp.org/cheatsheets/File_Upload_Cheat_Sheet.html |
| CSV/formula injection | Exports opened in Excel | Prefix `'` on cells starting `= + - @` [M] |
| XSS in rich text | HTML fields (notes, descriptions, website builder) | Server-side sanitiser allowlist; strict CSP; never render stored HTML unescaped |
| Business-logic abuse | Double posting, negative quantities/prices, discount stacking at POS, approval bypass, stock race conditions | Invariants in DB (CHECK, exclusion constraints, FK), idempotency keys, SERIALIZABLE or explicit locking on stock/ledger, segregation-of-duties rules |
| Excessive export | Admin exports whole tables | Export permission separate from read; audit + rate-limit; field ACLs apply |
| Unsafe public surface | Odoo: every public method is RPC-callable | Explicit allowlist of exposed commands; default private |
| Admin surface | Odoo DB manager on internet | No web DB manager; master password only in config (Odoo disclosure page also lists missing rate limiting, user enumeration and weak password policy as "non-qualifying" for bounties, i.e. upstream treats them as low priority: don't inherit that stance) [V] https://www.odoo.com/security-report |

---

## 11. Observability, scaling, abuse control

- **OpenTelemetry Rust**: traces, metrics and logs are all **Beta** on the official status page [V] https://opentelemetry.io/status/; `opentelemetry` crate 0.33.0 (2026-09-18) is pre-1.0 [V]. Wrap it behind an obop telemetry facade (`tracing` in code, OTLP exporter at the edge) so breaking releases touch one crate. Resource attributes: deployment, region, `tenant.id` (an opaque id, never names/emails). Scrub query parameters and bodies; sample traces; keep audit log separate from telemetry (different retention and integrity needs).
- **Horizontal scaling [I]:** stateless web workers; separate job workers (Odoo also separates cron workers and sets CPU/memory/time limits per worker [V]); PgBouncer transaction pooling with `SET LOCAL`; read replicas for reports only via a role that still enforces tenant context; websockets through a pub/sub (`LISTEN/NOTIFY` initially).
- **Rate limiting:** `tower_governor` 0.8.0 (keyed and global limits over `governor`) [V] https://crates.io/api/v1/crates/tower_governor. Layers: per-IP pre-auth, per-account login (NIST allows throttling but caps at 100 consecutive failures [V]), per-API-key, per-tenant quotas, concurrency caps on reports/imports/exports, request body limits, pagination caps, cost-weighted limits for search. In-process limits for single node; Postgres- or Redis-backed counters when scaled out.

---

## 12. Prioritised security-by-design checklist for obop

**P0: design-time invariants (cannot be retrofitted)**

1. One tenant-scoped DB handle type (`TenantTx`), transaction-local tenant context, restricted role, `FORCE` RLS in pool mode, composite tenant FKs.
2. Single authorization/query path for list and by-id; default-deny ACLs and record rules; field ACLs in one serializer.
3. Typed elevation token for privileged operations; no raw SQL outside a small audited module; no implicit RPC surface.
4. Append-only, hash-chained audit log and immutable posted accounting entries.
5. Argon2id + NIST-aligned password policy + TOTP + passkeys; server-side revocable sessions; hashed API keys.
6. Central egress client (SSRF defences) and isolated, I/O-less report renderer.
7. Money as integer minor units/decimals; `overflow-checks = true` in release.
8. Model registry with personal-data flags and retention classes (drives DSAR/erasure/retention).
9. Expand/contract migration discipline and N/N+1 compatibility from the first schema.
10. `cargo-deny` + `cargo-audit` + `--locked` + pinned toolchain + pnpm frozen lockfile in CI (`vp install --frozen-lockfile`) from the first commit.

**P1: before first public release**

11. Field-level envelope encryption with pluggable KEK (KMS/OpenBao/local), AAD binding.
12. Signed releases (cosign keyless + GitHub attestations), SBOM (CycloneDX) and `cargo-auditable` per release; SECURITY.md and disclosure process (CRA reporting duty since 2026-09-11 for commercial distributors).
13. OIDC SSO, SCIM subset, LDAP bind; SAML isolated or via bridge.
14. Rate limiting and quotas; body/decompression limits; upload pipeline.
15. pgBackRest-based PITR recipe + automated restore drill; encrypted backups with object lock.
16. OTel facade, PII scrubbing, security dashboards (auth failures, export volume, permission changes).
17. Threat-model-driven test suite: cross-tenant matrix, IDOR matrix, mass-assignment fuzz, SSRF corpus, ASVS 5.0 L1 self-assessment then L2.

**P2: before enterprise / compliance audits**

18. `cargo-vet` with shared audit imports; reproducible-build verification job; SLSA L3 via reusable workflow.
19. Customer-managed keys, region pinning, per-tenant export/restore, crypto-shredding.
20. Checkpoint publication of audit chain (WORM or transparency log).
21. SOC 2 / ISO 27001 evidence automation (operator-level); PCI scoping guide (never store PAN); HIPAA-style control pack tracking the NPRM.
22. Third-party penetration test and bug-bounty.

---

## Confidence & gaps

- **High confidence [V, primary]:** Postgres behaviours (RLS, ALTER TABLE locks, PITR, encryption options), PG/K8s/Rust/crate version readings, Odoo security model and deployment guidance, Cedar/OpenFGA release and CNCF status, CRA dates, GDPR article texts, NIST/OWASP parameters, W3C WebAuthn L3, pnpm supply-chain features (the npm-CLI trusted-publishing facts are kept only as a record and do not apply to obop).
- **Medium [V, secondary]:** SOC 2, ISO 27001 transition, PCI version history (Wikipedia); HIPAA NPRM status (HIPAA Journal); DPF developments (Wikipedia).
- **Not verified / open:** *Latombe v Commission* outcome and appeal; PCI SAQ-type mapping for POS/P2PE; SAML-specific implementation hazards (fetch refused); AWS QLDB end-of-support (fetch 404; [M]: it was retired in 2025, so avoid proprietary ledger DBs); Odoo's `sudo` semantics and French-localization hash chain (memory); TOTP and SCIM Rust crate quality; CloudNativePG production maturity beyond Sandbox status; Postgres 19 features. Whether 1.99.0 has actually been published on the stable channel (RELEASES.md dates it 2026-10-01, the channel manifest still showed 1.98.1 at 10:09 UTC that day). Resolved on re-check: rustls does default to aws-lc-rs; `trim-paths` is still unstable; `cargo-cyclonedx` 0.5.9 does not emit CycloneDX 1.7 (max 1.5).
- Crate "downloads" and "updated" figures are snapshots; GitHub API 403 means release dates for Cedar/OpenFGA came from release pages lacking years (crates.io timestamps confirm 2026 for Cedar; the `datetime` attributes on the GitHub release pages confirm 2026-09-15 for Cedar 4.13.0 and 2026-09-20 for OpenFGA v1.21.0).

---

## Implications for obop

**MUST**
1. **MUST** make tenant context a type: all DB access via a `TenantTx` that sets transaction-local tenant and drops to a non-owner, non-BYPASSRLS role; RLS with `FORCE` in pool mode; composite `(tenant_id, id)` FKs. Never use session-level `SET` behind PgBouncer.
2. **MUST** support DB-per-tenant ("silo") as a first-class mode (Odoo parity, per-tenant PITR/erasure/residency) alongside shared-schema "pool".
3. **MUST** implement Odoo-style ACL + record rules + field groups natively in Rust, default-deny, compiled to SQL, with one code path for list and by-id; no implicit RPC exposure; typed, logged elevation.
4. **MUST** ship Argon2id (OWASP minimums or higher, PHC strings), NIST 800-63B-4-aligned policy, TOTP and WebAuthn passkeys (`webauthn-rs`), and server-side revocable sessions; require SCRAM for Postgres.
5. **MUST** build an append-only, hash-chained audit log and immutable posting with reversal-based correction; pseudonymous actor ids.
6. **MUST** route all outbound HTTP through one SSRF-hardened client and render reports in an isolated, network-less, file-less worker.
7. **MUST** treat erasure as anonymise-in-place driven by per-model personal-data flags and country retention classes (HGB 10/8/6 y, UK 6 y, US 3-7 y), with an erasure ledger replayed after restores.
8. **MUST** gate CI on `cargo-deny`, `cargo-audit`, `--locked`, pinned toolchain, pnpm frozen lockfile (`vp install --frozen-lockfile`) with `minimumReleaseAge`, blocked install scripts (`allowBuilds` allow-list) and `trustPolicy: no-downgrade` (off by default, so set it explicitly), plus `vp pm audit`; verify scanners handle `pnpm-lock.yaml`'s two-document format.
9. **MUST** adopt expand/contract migrations with N/N+1 compatibility, `lock_timeout`, and the PG18-aware lock rules above.
10. **MUST** keep PAN out of scope (tokens/hosted fields/P2PE only) and publish SECURITY.md plus SBOMs because CRA reporting duties started 2026-09-11.
11. **MUST** enable `overflow-checks` in release and avoid `rsa` on network-exposed paths (RUSTSEC-2023-0071 open).

**SHOULD**
12. **SHOULD** support Postgres 16/17/18 (14 EOL 2026-11-12) and recommend pgBackRest (MIT, v2.59.2) with S3 object lock and monthly automated restore drills.
13. **SHOULD** provide envelope-encrypted field storage with pluggable KEK backends (cloud KMS, OpenBao, local sealed file), AES-256-GCM with contextual AAD, and a short catalog of encrypted fields plus HMAC blind indexes.
14. **SHOULD** offer OIDC first-class, SCIM subset (`/Users`, `/Groups`, PATCH, `eq` filter, ETag), LDAP bind (`ldap3`), and isolate SAML (separate process or external bridge) given `samael`'s pre-1.0, C-linked status.
15. **SHOULD** sign releases with cosign keyless and GitHub artifact attestations (SLSA L2 now, L3 via reusable workflow), embed `cargo-auditable` data, publish CycloneDX SBOMs.
16. **SHOULD** wrap OpenTelemetry (Beta, pre-1.0) behind a thin internal facade and keep audit logs separate from telemetry.
17. **SHOULD** use PgBouncer transaction pooling with `max_prepared_statements` tuned, read replicas with tenant context enforced, and per-tenant quotas and concurrency caps.
18. **SHOULD** ship Helm chart targeting Kubernetes 1.35+ and CloudNativePG as the documented in-cluster DB option, with managed-Postgres as the alternative.
19. **SHOULD** design Tauri local mode with loopback-only binding, per-install token, OS-keychain-held DB key, strict capabilities and CSP.

**COULD**
20. **COULD** embed Cedar (Apache-2.0, 4.13.0) later for action-level approval/ABAC policies; revisit for list filtering once `tpe` is stable.
21. **COULD** integrate OpenFGA (CNCF Incubating) as an optional external ReBAC backend for portal sharing graphs.
22. **COULD** adopt `cargo-vet` with imported audits, and publish audit-chain checkpoints to a transparency log (Rekor v2/Tessera) or WORM storage.
23. **COULD** pursue Percona `pg_tde` as an optional at-rest encryption profile for self-hosters willing to run Percona Server.
24. **COULD** use `pgroll`-style versioned-schema views for selected high-risk migrations.
25. **COULD** offer customer-managed keys and region-pinned deployments as the data-residency answer rather than relying on EU-US transfer mechanisms.

---

## Verification log

Adversarial re-check on 2026-10-01 using WebFetch plus direct `curl` of the crates.io API, GitHub release pages (`datetime` attributes), raw repo files, docs.rs, static.rust-lang.org and postgresql.org docs. A different primary source from the author's was used wherever one existed. The body labels ([V], [I], [M]) keep their original meaning; items I could not confirm are listed under "Unresolved" and are **(unverified)**.

**Confirmed as stated: about 95 individually checked claims**, including all 12 author-flagged load-bearing claims:
- PostgreSQL 18.6 / 17.11 / 16.15 / 15.19 current (all released 2026-08-13), 14 EOL 2026-11-12, PG19 Beta 4 on 2026-09-24 (postgresql.org home and versioning pages).
- No TDE in core PG; RLS bypass rules, RI-check bypass, PITR whole-cluster only (PG18 docs).
- Odoo 19.0 security.rst: default-allow record rules, global intersect / group union, public methods RPC-callable.
- Cedar 4.13.0 on 2026-09-15 (GitHub release `datetime`, crates.io), Apache-2.0 (crates.io, GitHub), CNCF Sandbox accepted 2025-10-08, `tpe` still an "experimental" feature in the CHANGELOG.
- OpenFGA v1.21.0 on 2026-09-20, CNCF Incubating 2025-10-28, GHSAs confirmed (GHSA-5278-rrxc-mgf7 in v1.19.0 StreamedListObjects, GHSA-h7w8-xr72-cv4r in v1.20.0 ListUsers).
- EU CRA: in force 2024-12-10; reporting from 2026-09-11 (24 h / 72 h / 14 d); main obligations 2027-12-11; Art. 64(10) steward fine exemption.
- RUSTSEC-2023-0071: no patched version, `rsa` 0.9.10 and 0.10.0-rc.18 affected as of 2026-09-12.
- NIST SP 800-63B-4 (15 / 8 chars, no composition rules, no rotation, blocklist, cap 100 failures); OWASP Argon2id m=19456, t=2, p=1.
- Vite+ 1.0 (MIT, `packageManager` field); pnpm `minimumReleaseAge` 1440, `trustPolicy: no-downgrade`, `blockExoticSubdeps`, script blocking since v10, two-document lockfile caveat.
- npm trusted publishing (npm >= 11.5.1, Node >= 22.14, GitHub Actions / GitLab / CircleCI, provenance only for public repo + public package); this is an npm-CLI fact recorded for reference and is not applicable to obop (pnpm-only).
- Rust blog index newest 1.98.1 (2026-09-03); releases.rs 1.99.0 stable, 1.100.0 on 2026-11-12; Cargo `trim-paths` still unstable.
- pgBackRest v2.59.2 (announced 2026-09-27), MIT, encryption, S3/GCS/Azure, object lock; WebAuthn L3 W3C Recommendation 2026-08-25.
- Also confirmed: ASVS 5.0 figures (70 of 345, 17 chapters), OWASP Top 10:2025 list, all crate versions and dates in the file (argon2, webauthn-rs, openidconnect, samael, ldap3, casbin, rustls, aws-lc-rs, sqlx, cargo-audit/deny/vet/auditable/cyclonedx, tauri 2.12.1, postgresql_embedded, opentelemetry, tower_governor), Kubernetes 1.37.1 and 1.34 EOL 2026-10-27, Helm 4.3.0, CloudNativePG Sandbox 2025-01-21, pg_tde 2.2.2, Odoo addon listing and licences (privacy_lookup and paper_muncher LGPL-3; no `auth_saml`), OCA auditlog, pgAudit licence, Rekor v2 GA 2025-10-10, Tessera, CycloneDX 1.7 / ECMA-424, SLSA 1.2, GitHub attestations, cosign versions, GDPR Arts. 12/15/17/28/32/33, CCPA 1798.105(d) and CPPA regulations, HGB/UK/IRS retention, SOC 2 / ISO 27001 / PCI DSS / HIPAA NPRM / Data Act / DPF statements (secondary ones remain secondary), ALTER TABLE / CREATE INDEX lock facts, pgroll, PgBouncer settings, OTel Rust Beta, SCIM RFC 7644, Odoo deploy.rst and security-report non-qualifying list, OWASP SSRF and file-upload guidance, Tauri trust boundary, AWS KMS and OpenBao (MPL-2.0, OpenSSF Sandbox).

**Corrections and upgrades**

| # | Was | Now | Source |
|---|---|---|---|
| 1 | Tauri `3.0.0-alpha.3` published 2026-09-30 | published 2026-09-26 (2.12.1 is the 2026-09-30 release) | https://crates.io/api/v1/crates/tauri |
| 2 | "`lock_timeout` ... [V, advisory in page]" | Not on the ALTER TABLE page; re-labelled [I] (common practice) | https://www.postgresql.org/docs/current/sql-altertable.html |
| 3 | OWASP A09 "Logging and Alerting Failures" | "Security Logging and Alerting Failures" | https://top10.owasp.org/2025 |
| 4 | ASVS 5.0.0 year "2025 by inference" | Confirmed 2025-05-30 from the release timestamp | https://github.com/OWASP/ASVS/releases/tag/v5.0.0_release |
| 5 | rustls default provider = aws-lc-rs was [M] | Verified: `aws_lc_rs` is in rustls default features | https://docs.rs/crate/rustls/latest/features |
| 6 | cargo-cyclonedx: "check it emits your target spec version [gap]" and "emits CycloneDX 1.7" unverified | It does not: README lists 1.3, 1.4, 1.5, default 1.3 | https://github.com/CycloneDX/cyclonedx-rust-cargo/blob/main/cargo-cyclonedx/README.md |
| 7 | cargo-auditable purpose [M] | Verified (`cargo audit bin`) | https://github.com/rust-secure-code/cargo-auditable |
| 8 | Rust 1.99.0 "most likely shipped 2026-10-01 [I]" | RELEASES.md dates it 2026-10-01, but the stable channel manifest still said 1.98.1 at about 10:09 UTC that day; it may not be downloadable yet | https://raw.githubusercontent.com/rust-lang/rust/master/RELEASES.md, https://static.rust-lang.org/dist/channel-rust-stable.toml |
| 9 | "ECMA-424 (2025-12-10)" with no edition | 2nd edition, December 2025, defines CycloneDX 1.7; 1st edition June 2024 | https://ecma-international.org/publications-and-standards/standards/ecma-424/ |
| 10 | Logical replication "DDL and sequences not replicated" partly [M] | Verified in PG18 docs | https://www.postgresql.org/docs/18/logical-replication-restrictions.html |
| 11 | webauthn-rs "not FIDO-certified" | README says audited by SUSE and makes no standards-compliance claim; "not certified" is an inference | https://github.com/kanidm/webauthn-rs |
| 12 | Cedar README "recommends exact pins" | README only describes pinning as an option | https://github.com/cedar-policy/cedar |
| 13 | OWASP SSRF sheet credited with "link-local" blocking | Sheet's deny list names metadata hosts, loopback, RFC1918, multicast; blocking the whole link-local range is our [I] addition | https://cheatsheetseries.owasp.org/cheatsheets/Server_Side_Request_Forgery_Prevention_Cheat_Sheet.html |
| 14 | DPF "still in force on the Commission page" | Page is in the present tense with no dated status; no annulment or suspension shown | https://commission.europa.eu/law/law-topic/data-protection/international-dimension-data-protection/eu-us-data-transfers_en |
| 15 | pg_tde "ships inside Percona Server for PostgreSQL 17+" | Bundled with Percona Server for PostgreSQL (needs its patches); packaged in Percona Distribution for PostgreSQL 17+ | https://docs.percona.com/pg-tde/index.html |

**Facts added**
- PgBouncer 1.26.0 (2026-09-23) fixes three CVEs (CVE-2026-19888, CVE-2026-6668, CVE-2026-6669) and tracks `search_path` by default on PG18+; run >= 1.26.0 (https://www.pgbouncer.org/changelog.html).
- CRA: severe-incident final report is due within one month of the 72 h notification; ENISA Single Reporting Platform live since 2026-09-11; stewards' Art. 24(3) reporting from 2027-12-11 (https://digital-strategy.ec.europa.eu/en/policies/cra-reporting).
- HGB §257(4): Buchungsbelege are kept 10 y for the listed financial-sector entities (https://www.gesetze-im-internet.de/hgb/__257.html).
- OpenFGA GHSA ids, RUSTSEC-2023-0071 CVSS 5.9 and affected `rsa` versions, Cedar `tpe` = type-aware partial evaluation.

**Unresolved (unverified)**
- Outcome and appeal status of *Latombe v Commission* (DPF): no fetched source covers it.
- Whether Rust 1.99.0 is live on the stable channel (time-sensitive; recheck).
- SOC 2, ISO 27001, PCI DSS version history and DPF developments rest on Wikipedia only; HIPAA NPRM status on HIPAA Journal only. No primary source was re-checked.
- CCPA/CPRA phase-in dates for cybersecurity audits, risk assessments and ADMT: not on the CPPA page fetched.
- Still [M] and not re-checked: Odoo `sudo` semantics, Odoo French-localisation hash inalterability, SAML implementation hazards, AWS QLDB retirement, PCI SAQ mapping, systemd sandboxing and TPM-bound unlock advice, CSV-injection prefix rule, Keycloak/Authentik as SAML bridges.
- Whether PostgreSQL 19 changes any PG18 behaviour cited (logical replication restrictions, RLS, ALTER TABLE locks): PG19 is in beta.
- Crate "downloads" and "updated" figures are point-in-time snapshots.

**Owner-constraint pass**
- Owner-constraint pass (2026-10-01): JS tooling is `vp` + pnpm only (no npm, npx, yarn, bun). Section 7.2 renamed "pnpm / Vite+", "supports pnpm/npm/yarn/bun" kept as a tool description with "obop uses pnpm only" appended, and the Vite+ package-manager detection order, `vp env pin` and `vp pm` added [V viteplus.dev/guide/install, /guide/env]. The npm trusted-publishing bullet (npm CLI >= 11.5.1, Node >= 22.14) is marked not applicable to obop and replaced by the verified pnpm controls (native `pnpm publish` since v11 with `--provenance`, `pnpm stage publish`/`approve` since v11.3.0; OIDC publishing via pnpm is not in the pnpm docs, so it is flagged for testing) [V pnpm.io/cli/publish, /cli/stage; workflow and issues via `gh`]. pnpm defaults corrected and sourced: `minimumReleaseAge` 1440 since v11 (0 before), `trustPolicy` off by default (v10.21.0), `blockExoticSubdeps` default true (v10.26.0), `allowBuilds` (v10.26.0) with `strictDepBuilds` default true, `pnpm audit` / `audit signatures` (v11.1.0). CI install is `vp install --frozen-lockfile`. Threat table T6 now reads "npm registry packages (installed via pnpm)"; the TOTP row's "npm 2FA" is reworded as a registry-account policy.
