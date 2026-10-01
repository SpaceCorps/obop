# 28 - Documentation, community, support, name/trademark clearance and legal-liability framing

Research date: 2026-10-01. **Nothing here is legal advice.** Parts A and E are briefing notes for counsel; every legal conclusion needs a qualified lawyer per jurisdiction. Tags: **[V]** verified today against a primary or near-primary source; **[S]** secondary source; **[I]** my inference; **[G]** guess or memory, unverified.

## Scope & method

No web-search tool existed in this session. I used WebFetch on known URLs, public back-ends called directly (USPTO search service, IANA-bootstrapped RDAP and ccTLD WHOIS, crates.io, npm, PyPI, Docker Hub, GitHub REST via `gh`, Wikipedia/Wiktionary APIs), OJ texts from the Publications Office, and `gh api` statistics on comparable repos. Bot-gated services (WIPO Brand Database shows an Altcha challenge; EUIPO eSearch and TMview are JS shells) were **not** bypassed, so EU, UK, German, French and WIPO trademark searches were **not performed**. Related files: 03 (Odoo business), 17 (licensing/CRA), 19 (accessibility), 21 (API/Data Act).

## 1. Bottom line

1. **Name: medium risk, clearance incomplete.** USPTO holds no "OBOP" record, live or dead [V], but a pending near-identical mark (OBOPP, class 35) exists, npm `obop` is taken, GitHub and Docker Hub handles are squatted, and `obop.com`/`.org` sit on a resale marketplace. No EU/UK/WIPO result exists. The tagline is descriptive and weak as a mark.
2. **Docs: VitePress 2 on Vite 8**, the stack Vite+ itself uses for its docs [V]; Docusaurus (webpack) and Nextra (Next.js) break "exclusively Vite+". Reference pages are generated from the model registry.
3. **Community:** Odoo's CLA plus the OCA backstop show the cost of open-core distrust; pre-empt with a foundation and public pledge. Paid-maintainer repos merge newcomers' first PRs in 1-2 days [V].
4. **Support:** human support costs roughly EUR 90-215 per ticket in my model [G]; copy Frappe's narrow "warranty" scope and sell priority separately.
5. **Security advisories must break with Odoo**, which gives Enterprise customers advance notice of at least 2 weeks by contract (its public security page cites "e.g. 2-3 weeks") [V https://github.com/odoo/documentation/blob/master/content/legal/terms/enterprise.rst s4.2, https://www.odoo.com/security-report].
6. **Legal:** Odoo caps liability at 50% of 12-month fees and disclaims legal-compliance warranties [V]; the Data Act bans SaaS switching charges from 2027-01-12 [V]; software becomes a "product" under the new Product Liability Directive from 2026-12-09 [V]; the consumer withdrawal function has applied since 2026-06-19 [V].

---

# Part A - Name and brand

## A.1 Register coverage and findings

| Source | Status | Result |
|---|---|---|
| USPTO (search back-end behind tmsearch.uspto.gov) | Queried: exact, prefix, wildcard, fuzzy | **0 records for OBOP**; prefix "obop" gives 7 records (A.2) [V] |
| EUIPO eSearch/TMview, WIPO Brand DB, UKIPO, DPMA, INPI | JS shell, bot check or session form | **Not searched** |

## A.2 USPTO near-conflicts

| Mark | Owner / status | Classes | Assessment [I] |
|---|---|---|---|
| **OBOPP** (serial 98648290) | Leadcraft, Inc. (California); live intent-to-use (basis 1(b)), filed 2024-07-15, unregistered, published for opposition 2025-02-25, USPTO status "SECOND EXTENSION - GRANTED" (time to file a statement of use extended) [V USPTO back-end, 2026-10-01] | 35 business consulting; 41 public-speaking coaching | **Aurally identical, one letter apart, class 35 overlap**; goods differ, so low-medium, but an examiner could cite it against an OBOP class 35 filing. It is past examination and publication, so it can register as soon as a statement of use is accepted [I] |
| OBOPAY (5 records) | Obopay, Inc.; all dead (filed 2005-2008) [V] | 9, 35, 36, 42 | Mobile-money firm still has a site (copyright 2018) [V https://www.obopay.com/]; common-law rights possible |
| OBOL | The Obol Association (Switzerland); live and **registered** (Reg. 7973449, 2025-10-07; use-based, first use 2023-07-31; filed 2024-10-29) [V] | 36, 42 (42 includes e-commerce platform hosting and software programming) | One letter apart, software class |
| OBOL (second owner) | The Obol Corporation (Colorado; Santa Monica address); live, registered 2020-04-07 (Reg. 6028386) [V] | 9 (downloadable messaging software) | One letter apart, **class 9 software**; a second OBOL registration that counsel must also opine on |
| OBOPINK, OBOX, OBOO, BOOP | various | 7, mixed, 5/35/10, 9/41/42 | Visual or phonetic neighbours in other fields |

**Tagline:** no USPTO record of "one business one platform" [V]. Nearest: ONE BUSINESS SOFTWARE (ONE Business Software Inc., class 42, live, "BUSINESS SOFTWARE" disclaimed, registered 2019-07-16); ONE BUSINESS. ONE SOFTWARE. (CentraView, dead, 2003); 30+ "ONE PLATFORM..." marks, e.g. ONE PLATFORM (Webcetera, class 42, live) [V]. It is laudatory and descriptive [I]: use it as copy, keep it out of the protected list.

## A.3 Meaning, pronunciation, prior use

- **OBOP = Ośrodek Badania Opinii Publicznej**, Poland's public-opinion research centre (1958; privatised 1998 as TNS OBOP; merged into TNS Polska 2012; Kantar 2019) [V https://pl.wikipedia.org/wiki/TNS_Polska]. It fills Polish polling tables and search results [V/I].
- "One Brand One Planet" also abbreviates to OBOP (GitHub user OBOPOS, created 2026-07-18) [V]. WordPress.org plugin "OBOP" (v0.3, 999 lifetime downloads, 0 ratings, last updated 2016-04-13, homepage obop.co, unreachable; active installs not reported) [V https://api.wordpress.org/plugins/info/1.0/obop.json]. npm `obop`: kawanet, MIT, 204,381 downloads 2026-08-31 to 2026-09-29 [V].
- No en/de/fr/es/ru/it/pt/nl/ja/ko Wiktionary entry; Polish Wiktionary has OBOP [V]. English "bop" is music slang; readers may say "OH-bop" or spell it out [I].
- **Not done:** native-speaker review (zh, ar, hi, id, tr, pt-BR, ja, ko); the Russian abbreviation "ОБОП" is unconfirmed [G].

## A.4 Namespace availability (2026-10-01)

| Namespace | Status |
|---|---|
| crates.io `obop` (+ `-core`, `-sdk`, `-server`), PyPI, Homebrew, NuGet, RubyGems, Packagist | **Free** [V] |
| npm `obop` | **Taken**, active (use `@obop/*`; scope availability unverified) [V] |
| GitHub user `obop` | **Taken, dormant** (created 2019-07-18, 0 repos, last update 2022-12-04); `obopos` also taken. Orgs `obop-io`, `obop-org`, `obopdev`, `getobop`, `obop-erp`, `obop-oss`, `oneobop` free [V] |
| Docker Hub `obop` | **Taken** (user "Yang Yang", 0 repos, joined 2025-07-07) [V]; GitLab group, Quay org free |
| `obop.com` (registered 2000, expires 2027-04-12), `obop.org` (**expires 2026-10-10**) | Redirect to Atom resale listings; price unknown [V] |
| `.net` (2007), `.io`, `.eu`, `.co`, `.nl`, `.pl`, `.ca`, `.xyz`, `.ai` ("Launching soon"), `.app` (2026-07-17, parked), `.de` (Sedo resale) | Registered [V] |
| `.dev`, `.cloud`, `.tech`, `.software`, `.systems`, `.info`, `.biz`, `.one`, `.store`, `.page`, `.business`, `.company`, `.email`, `.fr`, `.uk`, `.be`, `.it`, `.se`, `.us`, `.sh` | Unregistered per RDAP/WHOIS (premium pricing unchecked) [V] |
| Bluesky `obop.bsky.social`, Mastodon/Fosstodon `@obop`, Matrix `#obop:matrix.org` | Free [V]; YouTube `@obop` taken; X and LinkedIn unprobeable |

**Rating [I]: medium.** Revisit if counsel finds an EU/UK/WIPO conflict in classes 9/35/42 or OBOPP registers broadly.

## A.5 Fallback names (mechanically screened, not legally cleared)

Checked: crates/npm/PyPI, GitHub user, `.com`, `.org`, `.dev`, USPTO prefix hits [V].

| Candidate | Result |
|---|---|
| **kernelbiz** | Packages and GitHub free; `.com` taken, `.org/.net/.dev/.app` free; USPTO 0. Clean but clunky |
| **monobiz** | Packages free; GitHub and `.com` taken; `.org/.dev` free; USPTO 0 |
| **mercora** | Packages free; GitHub and `.com` taken; `.org/.dev` free; 1 non-live USPTO record |
| omnibiz, oxbiz, keelos | Usable but generic or crowded |
| tessera, holdfast, ergos, kinora, fulcra | Rejected: package collisions or live marks |

Short coined `.com` names are nearly all taken. Treat this as a method: run a naming sprint, screen 50 candidates, send three to counsel.

## A.6 Trademark policy draft (for counsel)

Models [V]: **Frappe/ERPNext** (descriptive use allowed; no use in product, company or domain names; no TLD use; inspired by WordPress) https://github.com/frappe/erpnext/blob/develop/TRADEMARK_POLICY.md; **WordPress Foundation** ("Hosting for WordPress" over "Hosted WordPress"; commercial licensing via Automattic) https://wordpressfoundation.org/trademark-policy/; **Mozilla** (truthful text references; modified builds need distinct branding; no marks in domain/product/business names) https://www.mozilla.org/en-US/foundation/trademarks/policy/; **Rust Foundation** (word marks Rust and Cargo; "written in Rust" and `cargo-foobar` allowed if not presented as official; modified builds only for porting/packaging) https://rustfoundation.org/policy/rust-trademark-policy/; **Tryton** (foundation holds EU and US registrations in classes 9, 37, 42) https://www.tryton.org/foundation.

1. **Protected:** word "obop", logo, "obop Cloud", "obop Certified". Not protected: the tagline, module names.
2. **Free, no permission (nominative):** "works with obop", "obop consulting", "hosting for obop"; unmodified screenshots; packages `obop-<thing>` if the README says unofficial.
3. **Never:** obop as the start or whole of a company, product or domain name; TLD use; logo inside another logo; keyword ads; implied endorsement.
4. **Community by notice:** user groups, meetups, translation teams; non-commercial swag; commercial merchandise needs a licence.
5. **Modified builds:** must drop the marks unless changes are packaging, porting, security backports or configuration (Mozilla/Rust rule).
6. **Partners/certification:** written agreement per tier, revocable for cause with notice and cure. **Marketplace:** "<Name> for obop", never "obop <Name>".
7. **Enforcement:** 30-day cure, published criteria, annual transparency report, **no revenue-share demands on hosts** (WP Engine lesson, file 17).
8. **Governance:** foundation owns the marks; the company holds a non-exclusive, non-discriminatory licence on the published terms; policy changes need board approval. The policy adds no restriction on code use (avoids Neo4j-style "further restriction" fights).

## A.7 Foundation hand-off and costs

- Decide the owner before filing. A US intent-to-use application cannot be assigned before an amendment to allege use or statement of use is filed, except to a successor to the applicant's ongoing and existing business (15 U.S.C. 1060(a)(1)) [V https://www.law.cornell.edu/uscode/text/15/1060; counsel to confirm application], so **do not file an ITU in a founder's name planning to assign it later.**
- At foundation formation: assign marks and domains, license back non-exclusively, record assignments at EUIPO/USPTO/UKIPO; foundation holds the GitHub org, crate/npm/GHCR orgs and domains with 2+ independent admins (MinIO/Cal.com withdrawal risk, file 17); board veto over leaving OSI licences.
- Fees: UK IPO GBP 205 first class, GBP 60 each additional (online) [V https://www.gov.uk/how-to-register-a-trade-mark/start-your-application]; EUIPO EUR 850 (electronic) / 50 / 150 for first / second / each further class [V Reg. (EU) 2017/1001 Annex I, https://publications.europa.eu/resource/celex/32017R1001; the EUIPO fee page itself returned 403]; USPTO USD 350 base fee per class plus surcharges for free-text identifications [V https://www.uspto.gov/trademarks/fees-payment-information/summary-2025-trademark-fee-changes]. Classes 9+35+42 across EU (EUR 1,050), UK (GBP 325) and US (USD 1,050): roughly EUR 2.3-2.5k official filing fees at recent exchange rates (rates not checked) plus counsel, excluding renewals, oppositions and Madrid/WIPO fees [I].
- **Needs counsel:** full searches (EUIPO, UKIPO, WIPO, DPMA, INPI, Poland for OBOP legacy); opinions on OBOPP and OBOL; class strategy (9, 35, 42; consider 41, 45); passing-off search; policy and enforcement playbook; assignment mechanics; domain acquisition; native-language review.

---

# Part B - Documentation

## B.1 Docs-stack comparison [V unless noted]

| Stack | Licence / state | Vite+ fit | Search | Versioning | i18n |
|---|---|---|---|---|---|
| **VitePress** | MIT; npm `latest` 1.6.4 (Vite ^5.4); `next` **2.0.0-alpha.20** (2026-09-04, Vite ^8.2.1) | **Native**: Vite+ docs run VitePress 2.0.0-alpha.17, `@voidzero-dev/vitepress-theme` 4.8.4, llms.txt and mermaid plugins (`voidzero-dev/vite-plus/docs/package.json`). Note: alpha.17 declares Vite ^7.3.1 and only runs on Vite 8 because the Vite+ monorepo overrides `vite` with its own core (Vite+ 1.0.0, released 2026-09-28, bundles Vite 8.3.1); alpha.18 and later declare Vite ^8.x. The shared VoidZero theme is published on npm as UNLICENSED (latest 5.0.7), so do not depend on it; use the default theme | Local minisearch (fuzzy 0.2) or Algolia; community Pagefind | **None built in** | Locale folders, per-locale config, RTL via `dir` |
| **Starlight (Astro)** | MIT; 0.42.5, pre-1.0; needs Astro ^7.2.10 (Astro 7.3.5 uses Vite ^8.0.13) | Runs on Vite 8, but a second framework and CLI | Pagefind built in | Community `starlight-versions` 0.10.1 | 35 UI languages, fallback notice, RTL |
| **Docusaurus** | MIT; 3.10.2 (2026-07-10) | **webpack ^5.95** (Rspack optional) | Algolia | **Built in** (docs advise under 10 versions) | Built in |
| **Nextra** | MIT; 4.6.1 (last release 2025-12-04) | Next.js | Pagefind | None | Via Next.js |
| mdBook | MPL-2.0; 0.5.4 | Rust-native | Built in | None | Limited |

**Pick VitePress;** risk is 2.x still being alpha after 20 alphas, so pin and keep content plain Markdown; Starlight is the fallback. **Search:** Pagefind 1.5.2 (MIT) indexes per `lang` attribute, stems many European languages, segments CJK only in the extended build https://pagefind.app/docs/multilingual/; Algolia DocSearch is free for public technical docs but the eligibility page is silent on company-backed products https://docsearch.algolia.com/docs/who-can-apply. Default to local/Pagefind (works in air-gapped installs).

**Versioning is our job:** build each supported branch to `/docs/<version>/` with a `/docs/current/` alias, as PostgreSQL does (18 current, 14-18 supported, 19 in beta, every version back to 7.1 still online, with a per-page "Submit correction" form) https://www.postgresql.org/docs/current/docguide.html; Odoo builds a version switcher with `make html VERSIONS=...` https://github.com/odoo/documentation. Freeze EOL versions as immutable snapshots (Docusaurus's advice, https://docusaurus.io/docs/versioning).

## B.2 Information architecture (Diataxis)

Diataxis = tutorials, how-to, reference, explanation (adopted by Cloudflare, Gatsby per https://diataxis.fr/).

| Type | obop content | Source of truth |
|---|---|---|
| Tutorials | "First company in 30 minutes" per persona; extension hello-world | Hand-written, examples compiled in CI |
| How-to | Per module ("Issue a credit note") and per country ("File VAT in X") | Module/localisation maintainer |
| Reference | Models, fields, actions, access rules, events, REST/MCP tools, config, CLI, errors | **Generated from the model registry** (file 21) |
| Explanation | Data model, posting engine, security, upgrade policy | RFC/ADR-reviewed |

- **Per-module docs** live in the module's `docs/` directory (OWNERS-controlled); a module cannot be "stable" without all four types.
- **Per-country docs** (`/l10n/<country>/`): dated legal citations, chart of accounts, taxes, e-invoicing network, payroll rules, "last verified" front-matter, named local maintainer, staleness alert at 6 months. Precedent: OCA keeps per-country repos such as `l10n-spain` (AGPL-3.0) [V].
- **API reference:** Supabase feeds OpenAPI plus custom SDKSpec, ConfigSpec and CLISpec into its renderer so it can "switch to any documentation system" [V https://github.com/supabase/supabase/tree/master/apps/docs]. obop: registry to OpenAPI 3.1 (track 3.2.1, published 2026-09-10 https://spec.openapis.org/oas/latest.html) to Scalar (MIT, `@scalar/api-reference` 1.72.3 released 2026-09-30) or `vitepress-openapi` 0.2.5 (MIT), plus generated per-model Markdown; **CI fails if any model, field, action or error code lacks a description.** rustdoc for crates.

## B.3 Compiled examples, in-product help, RFC/ADR, guides

- **Tested examples:** the Rust book's CI pins mdBook 0.5.1 and rustc 1.98 and runs `mdbook test`, compiling every listing [V]. obop: examples are real files in `examples/`, embedded with VitePress `<<< @/examples/...#region`, which **fails the build on a missing file or region** [V https://vitepress.dev/guide/markdown]; `cargo test --doc`, Vitest, a job replaying each `curl` snippet against a seeded instance, Twoslash for TypeScript; Vale, typos, lychee link-check; Playwright regenerates screenshots per release.
- **In-product help [I]:** every field/action carries a registry `help` string; "?" opens the matching docs page for the installed version; docs publish `llms.txt` (Vite+ docs already use `vitepress-plugin-llms` [V]) and an MCP docs server; air-gapped installs bundle the search index.
- **RFCs/ADRs:** Vite+ keeps about 45 RFCs as Markdown in `rfcs/` [V gh api, 45 `.md` files on 2026-10-01]. Adopt `rfcs/NNNN-title.md` with Draft, 10-day Final Comment Period, Accepted, Implemented; MADR-style ADRs inside repos for implementation decisions.
- **Guides:** contributor guide (one-command dev env, review SLAs, Diataxis compass, word list; Supabase ships `WORD_LIST.md`, a style guide and docs-agent skills [V]); extension-author guide (plugin boundary from files 17/24, versioned API, testing harness, signing, naming rules from A.6).

## B.4 Comparators [V]

| | Toolchain / licence | Activity and lessons |
|---|---|---|
| **Odoo** | Sphinx/RST; **CC-BY-SA-4.0**; Weblate (`odoo-20-doc`); 25 locale dirs, 19 switcher languages; legal pages translated only for de, es, fr, nl, pt_BR | 1,167 `.rst` files; 513 contributors; last 12 months 1,105 commits by 145 authors (23 with 10+, 6 with 50+, most top authors staff-style `-odoo` logins): paid team does the volume |
| **Frappe** | docs.frappe.io "Built on Wiki"; old `frappe_docs` repo archived 2021-12-06; English only; licence unstated | Dogfooding is cheap, contribution path unclear |
| **Stripe** | Markdoc (MIT, 8.5k stars) built to power Stripe's docs | Custom syntax pays off only at Stripe scale |
| **Supabase** | Next.js in the monorepo, DocSpec generation | 1,690 commits, 395 authors, 38 with 10+ in 12 months |
| **Rust book** | mdBook, MIT/Apache-2.0, listings compiled in CI | 731 contributors ever; 13 authors in 12 months, one wrote 73 of 88 commits (GitHub commits API, 2025-10-01 to 2026-10-01) |
| **PostgreSQL** | DocBook, Make + Meson | 5-year support per major, all old docs retained |

## B.5 Staffing, cost, quality [I/G]

| Stage | Docs headcount | EUR per year, loaded [G] |
|---|---|---|
| Pre-GA to 50 tenants | 1 lead writer + 0.5 docs engineer; engineers write their module docs | 120-150k |
| About 500 tenants | 3-4 (lead, API writer, localisation writer, editor) | 300-400k |
| About 5,000 tenants | 8-12 plus translation vendor | 0.8-1.2M (Odoo shows 10-25 dedicated staff [I]) |

Translation tooling: Weblate hosted is free for public "Libre" projects, else EUR 1,140/year for 160k and EUR 1,930/year for 640k hosted strings (strings x (languages + 1)) [V https://weblate.org/en/hosting/]; 30k source strings x 10 languages = 330k, tier 640k [I]. **Crowdin's free open-source plan requires that no commercial product be tied to the project** [V https://crowdin.com/page/open-source-project-setup-request], likely excluding an open-core vendor; prefer Weblate (GPL-3.0, self-hostable).

**Quality metrics [G targets]:** 100% of registry entities documented; 100% of examples compile; zero broken links; first successful API call under 15 minutes; search zero-result rate under 8%; pages unreviewed over 12 months under 10%; supported locale at least 80% reviewed; "helpful" rating above 75%; ticket-deflection tracked monthly. **Docs licence [I]:** prose CC-BY-SA-4.0 (Odoo's choice) with Apache-2.0 code samples; counsel to confirm share-alike effects on partner training material.

---

# Part C - Community

## C.1 Channels

| Channel | Facts [V] | Role |
|---|---|---|
| GitHub Discussions/Issues | Free | Dev Q&A, RFC feedback, triage |
| **Zulip** | Apache-2.0, release 12.3 (2026-09-21); Cloud Standard "free for open-source projects", conditions unstated; Rust uses it | Contributor chat |
| **Discourse** | GPL-2.0; hosted Pro USD 100/month, Business USD 500/month, 50% non-profit discount, no OSS discount listed | End-user, accountant and implementer forum (self-host) |
| Matrix / Discord | Matrix open; Discord closed and unindexable | Optional bridge / mirror only |

## C.2 Governance, CoC, CLA

- **Ladder (Kubernetes template [V https://github.com/kubernetes/community/blob/master/community-membership.md]):** Member (2 sponsors from different companies, 1+ merged PR) -> Reviewer (3 months, primary reviewer on 5 PRs, 20 substantial PRs) -> Approver (3 months, primary on 10, 30 PRs) -> Subproject owner; 12 months inactive means removal, reversible. Scale down for obop (3/10/20) and add **module maintainer** (OWNERS file) and **localisation maintainer** (must be a practising accountant/payroll specialist). No company holds a majority of core approver seats [I].
- **Code of conduct:** Contributor Covenant 3.0 (CC BY-SA 4.0), ladder Warning, Temporarily Limited Activities, Temporary Suspension, Permanent Ban, each with a repair step [V https://www.contributor-covenant.org/version/3/0/code_of_conduct/]; GitHub's releases list still shows 2.1, so pin the text. Odoo's only published CoC covers its mailing lists (v1, 2017) (unverified: the page could not be located during the fact-check; no CoC file exists in `odoo/odoo` or `odoo/documentation`). Enforcement [I]: committee of 3 including a non-employee, recusal on conflicts, 72-hour acknowledgement, annual transparency numbers, appeal to the foundation.
- **CLA UX:** Odoo uses an Apache-CLA-v2.0-style ICLA/CCLA, not an assignment; sign by adding `<github-login>.md` to `doc/cla/individual` (or company file to `corporate`); **PRs cannot merge until that signature PR merges**; non-GitHub signers email a scanned PDF [V https://github.com/odoo/odoo/blob/master/doc/cla/sign-cla.md]. Tools: CLA Assistant (Apache-2.0, last push 2024-06-06, 249 open issues) looks stagnant; EasyCLA (MIT, v2.9.6 2026-09-03) is Linux-Foundation run; DCO app (ISC) is light [V]. **obop [I]:** GitHub sign-in, one "I agree" comment, public signature ledger, domain-verified corporate click-through, bot reply in seconds, DCO for docs fixes, the OSI-licence-forever pledge quoted in the bot comment (file 17 s4).

## C.3 Funnel and measured comparables [V]

Contributors (GitHub API incl. anonymous): odoo/odoo 4,301; frappe 1,075; erpnext 942; twenty 762; rust-lang/book 731; odoo/documentation 513. OCA: 271 public repos (268 in the first pass; GitHub org API on 2026-10-01), 5,057 followers, top repos AGPL-3.0. Merged PRs in Q3 2026: Twenty 2,926; ERPNext 2,078; Frappe 2,059; Starlight 145; VitePress 94; OCA/web 34.

**Newcomer outcome** (newcomer = first-ever PR in the repo opened and merged in the window; unmerged first PRs excluded, so this is latency for successful PRs):

| Repo | Authors checked | Newcomers | Merged | Median days to merge |
|---|---|---|---|---|
| Starlight (full quarter) | 40 | 20 (50%) | 20 | 1.3 (p75 2.2) |
| VitePress (full quarter) | 29 | 11 (38%) | 11 | about 0 (p75 1.2) |
| OCA/web (full quarter) | 26 | 4 (15%) | 4 | 4.7 (p75 43.0) |
| Frappe (300 latest merges only) | 29 | 5 (17%) | 4 | 1.9 (p75 14.2) |
| Twenty (300 latest merges only) | 21 | 7 (33%) | 3 | 0.8 |

Re-run during fact-check with a slightly different newcomer rule (no earlier PR by that author in the repo, any state): Starlight 23 of 41 authors, median 1.65 days (p75 2.3); VitePress 14 of 30, median 0.57 days (p75 1.4). Counts differ from the table, the conclusion does not; the Frappe, Twenty and OCA rows were not recomputed (unverified). Paid maintainers merge first PRs in 1-2 days; volunteer OCA has a 5-day median and a tail past 40 [I]. **obop targets:** first response under 2 business days, median first-PR merge under 5 days, p75 under 14. Open "good first issue" labels: Twenty 0, ERPNext 4, Frappe 3, Odoo 0, OCA/web 0, VitePress 0 [V]: comparables barely use the label, so a curated, mentored programme (named mentor, 48-hour response) is cheap differentiation. Track visitor -> issue -> PR -> merge -> second PR; aim for 25% returning within 90 days [G].

## C.4 Translation, module community, certification, events

- **Translation:** Weblate, translation memory + glossary, machine-translated first pass labelled "MT, unreviewed", 80% reviewed = "supported". Launch set [I]: en, de, fr, es, nl, pl, it, pt-BR; then ja, ko, zh, ar (RTL). Contracts stay English-authoritative, as Odoo's legal text does [V].
- **OCA-style community:** OCA is a Swiss non-profit (2013) with the stated aim of keeping Odoo viable as open source on equal footing with Odoo SA, CLA before commit rights [V https://odoo-community.org/page/about for the mission and 2013 founding; https://odoo-community.org/page/cla for the CLA rule]. obop [I]: an `obop-community` org under the foundation with a quality bar, maturity labels (Alpha/Beta/Production), a security-response promise for Production modules, forward-port bot and upgrade tool (OCA's OpenUpgrade has 991 stars [V]). Launch at 20+ third-party modules.
- **Certification** (file 20: start at 50 production tenants): tracks Functional (Associate/Professional), **Accountant** (chart of accounts, closing, VAT, reconciliation, e-invoicing, one add-on per country), Developer/Extension author, Operator (deploy, backup, upgrades, CRA hygiene); exam plus scenario lab, 70% pass, validity tied to a major release, fee EUR 150-300 [G], free content (Odoo's e-learning is included in plans [V https://www.odoo.com/pricing]). Odoo exam terms unverified.
- **Events:** Odoo Experience 2026, 24-26 Sept, Brussels Expo, free entrance, 400+ talks [V https://www.odoo.com/experience]; reported budget EUR 4.5M against EUR 3.5M income (press via file 03; not independently re-verified, unverified) [S], about 45,000 attendees ("45k+" expected on the event page; another section of the same page says "more than 40,000") [V], roughly EUR 100 per attendee [I], a sales channel. Frappeverse Mumbai 2026: 21-22 Aug, Nehru Centre Auditorium, bootcamps 19-20 Aug, partner-sponsored [V https://events.frappe.io/events/frappeverse-mumbai-2026]. OCA Days is a yearly event in Belgium, 2026 edition in Liege on 21-23 Sept [V https://odoo-community.org/ocadays]; "since 2018" is unverified. Twenty: no events found. obop plan [G]: Year 1 virtual plus a FOSDEM presence (EUR 15-25k); Year 2 one-day 150-300 person event (EUR 80-150k, sponsor-funded); cap at 5-8% of revenue.

---

# Part D - Support model

## D.1 Comparable promises [V]

| Vendor | Promise |
|---|---|
| **Odoo** | "Unlimited access to our support by email, from Monday to Friday, 24/5, in English, French, Spanish or Dutch"; implementation excluded (pricing page); bug handling starts within 2 business days (Enterprise agreement s4.1). **Cloud SLA:** 99.9% monthly target, "not legally binding", no credits mentioned; 14 full backups kept 3+ months; RPO 24h, RTO 6h (server) or 24h (data centre) https://www.odoo.com/cloud-sla |
| **Frappe** | Mon-Fri 10:00-18:00 IST; plans USD 50+: Critical 1h response / 4h resolution, High 2h / 16h, Medium 2h / 32h, Low 2h / 40h; Premium halves Critical to 30 min / 2h; phone only with Priority add-on; warranty covers bugs, not functional help; cloud terms give no uptime % https://docs.frappe.io/legal/policies/service-level-agreement |
| **Twenty** | Pro standard; Organization (USD 19/seat) priority; Enterprise (from USD 50k/year) "Dedicated support & SLA" https://twenty.com/pricing |
| **Supabase** | No SLA; business-hours best effort https://supabase.com/terms |

## D.2 Proposed tiers [I]

| Tier | Scope | Targets |
|---|---|---|
| Community (free, all editions) | Forum, Discussions, docs | Triage label within 5 business days |
| Cloud Standard (in subscription) | Platform incidents, bugs in first-party modules on supported versions, restore, migrations; **excluded:** customisation, third-party modules, how-to, data entry (Frappe's list [V]) | First response P1 1h (24/5), P2 4 business hours, P3/P4 1 business day; 99.9% monthly **with service credits** (10/25/50%) |
| Cloud Priority (add-on) | Named contact, phone | P1 30 min, P2 2h |
| Enterprise | 24/7 P1, DPA, security review, single-tenant | Contracted |
| Self-hosted subscription | Bug fixes, advisories **at the same time as everyone**, diagnostics triage | Response only; no platform SLA |

Definitions: *response* = first qualified human reply; *resolution* = workaround or fix delivered, clock paused while awaiting the customer; *restoration* = service usable; severity P1 (down, data loss, security) to P4 (question). **Boundary:** on hosted, obop owns infrastructure, backups, patching and platform incidents; on self-hosted it supports only unmodified first-party modules on a published OS/Postgres/version matrix with a diagnostics bundle (Frappe requires a supported Linux distribution and official release [V]). Partners own customisation and functional help; hand-off via the portal (Odoo routes through the partner channel when one exists; Frappe refers out-of-scope work to partners [V]).

## D.3 Tooling, diagnostics, status page

- **Dogfood** obop Helpdesk (file 14) with portal, SLA timers, docs-suggested answers and ticket-to-issue links; keep an independent email address and static status page as outage fallback.
- **`obop diag` bundle [I]:** manifest and checksums; allow-list only: build hash and versions, module list, config keys with values redacted, migration state, Postgres version/extensions/settings, slow-query fingerprints with parameters stripped, scrubbed log tail (regex plus structured redaction, golden-file tests), queue depth, job failures, resource metrics, feature flags, CycloneDX SBOM. Never: row data, secrets, tokens, mail bodies. Customer previews the JSON; signed expiring upload link; deleted after 30 days.
- **Status page** on separate infrastructure: Gatus (Apache-2.0, v5.37.0), Uptime Kuma (MIT, 2.5.5) or Upptime (MIT) [V]. Odoo announces maintenance by email and @OdooStatus [V]. Comms [I]: acknowledge P1 within 15 minutes, update every 30 minutes, post-mortem within 5 business days.

## D.4 Equal-time advisories and CRA runbook

**Odoo's embargo [V]:** Enterprise self-hosters get the advisory at least 2 weeks before public release (Enterprise agreement s4.2); the security page says it goes first to "stakeholders and Odoo Enterprise customers" with "a reasonable delay" given as "e.g. 2-3 weeks", an example rather than a commitment https://www.odoo.com/security-report; Odoo's cloud is patched as soon as a remedy exists (agreement s4.2, Cloud Platform). **obop rule (file 04 #7) [I]:** one publication moment for all editions via GitHub Security Advisories (GitHub is a CNA; CVE requests usually reviewed within 72 hours [V https://docs.github.com/en/code-security/security-advisories/working-with-repository-security-advisories/about-repository-security-advisories]), RustSec for crates, npm registry advisories (what `pnpm audit` reads [V https://pnpm.io/cli/audit]); pre-notification only to a **public, free-to-join** packager/host list with a short fixed embargo; hosted tenants are patched before publication and that asymmetry is stated openly; security fixes never gated (file 17).

**CRA runbook** (Art. 14 applies since 2026-09-11; ENISA's Single Reporting Platform is reported operational from that date; the notification goes to the CSIRT of the manufacturer's main establishment; stewards' duties start 2027-12-11 [V https://digital-strategy.ec.europa.eu/en/policies/cra-reporting]):

| Clock | Action | Owner |
|---|---|---|
| T0 awareness of an actively exploited vulnerability or severe incident | Open incident, start timer, name commander | Security lead |
| T0 + 24h | Early warning via SRP | Security lead |
| T0 + 72h | Notification: nature, affected versions, mitigations | Security lead + legal |
| Fix available + 14 days | Final report (exploited vulnerability) | Security lead |
| 72h notice + 1 month | Final report (severe incident) | Incident commander |
| Continuous | Patch supported branches, advisory + SBOM delta, credit reporter | Release manager |

Pre-work: SRP account and contacts verified; weekend on-call (the clocks ignore weekends); "actively exploited" decision tree; pre-approved templates; monthly drill; manufacturer-vs-steward split per file 17 s7.

## D.5 Staffing and cost per ticket [G: model, not benchmark]

Assumptions: EUR 70k/year loaded per support engineer (EUR 5.8k/month); 120 productive hours per FTE-month; human tickets per tenant-month 0.6 / 0.5 / 0.35 at 50 / 500 / 5,000 tenants (docs, in-product help and AI deflect more at scale); 1.5 h per ticket (1.3 at scale); floor of 2 people for 5-day cover; 20% uplift for L3 and management; tooling EUR 0.5k / 3k / 15k per month (the first pass printed 0.5k / 15k / 15k, which does not reproduce the table: 4 x 5.8k + 3k = about 26k; corrected during fact-check [I]).

| Tenants | Tickets/month | Hours | FTE needed / staffed | Monthly cost | Per ticket | Per tenant-month |
|---|---|---|---|---|---|---|
| 50 | 30 | 45 | 0.4 / 1 + founders | about 6.5k | **about 215** | about 130 |
| 500 | 250 | 375 | 3.1 / 4 | about 26k | **about 105** | about 52 |
| 5,000 | 1,750 | 2,275 | 19 / 24 | about 154k | **about 88** | about 31 |

Against Odoo's Standard price anchor [V https://www.odoo.com/pricing: EUR 19.90 per user per month is the promotional yearly-billing rate for the first 12 months, list EUR 24.90; monthly billing EUR 24.90 promotional, 31.10 list], five users is about EUR 100 per tenant at the promotional rate (about EUR 125 at list), so bundled human support eats roughly 30-50% of revenue at 500-5,000 tenants at the promotional rate (about 25-42% at list) and more below; hence Frappe's bug-only warranty and partner routing [V]. Levers: narrow scope, partner routing, deflection, paid Priority. HDI/Zendesk benchmark pages returned 404. **Hours and languages:** Phase 1 English plus German or French in CET hours; Phase 2 (about 500 tenants) add Spanish/Dutch and a second hub via partners; Phase 3 (about 5,000) true 24/5 in four languages like Odoo [V], 24/7 P1 for Enterprise.

---

# Part E - Legal-liability pack for counsel (not legal advice)

## E.1 Hosted ToS / DPA / SLA skeleton

| Document | Clauses | Benchmarks [V] |
|---|---|---|
| **ToS / order form** | Service and version policy; acceptable use; fees and renewal; suspension; data ownership and export; termination and retention; third-party and AGPL notices; trademark policy reference; law and forum; export/sanctions; term changes | **Cap:** Odoo Enterprise 50% of fees paid in the prior 12 months, no consequential damages, sole remedy re-performance; Supabase 1x 12-month fees, 3x for confidentiality, indemnity, gross negligence, fraud and wilful misconduct, no SLA in the terms (https://supabase.com/terms s11); Frappe fees actually paid in 12 months, sole remedy re-performance or pro-rata refund, no uptime guarantee |
| **DPA (GDPR Art. 28)** | Roles, instructions, sub-processor notice, transfer tools, breach notice (24-48h), audit, deletion aligned with E.4, TOMs annex | n/a |
| **SLA annex** | Availability target and measurement, maintenance windows, credits, RPO/RTO, response table (D.1) | Odoo 99.9% non-binding, RPO 24h, RTO 6-24h. **Decide:** binding target with capped credits [I] |
| **Warranty** | Community edition "as is" (AGPL already disclaims); limited hosted service warranty; no legal-compliance warranty except under a paid compliance product | Odoo: "does not warrant that the Software complies with any local or international law or regulations" (s7.2) |
| **Uncappable items** | Intent, gross negligence, death and personal injury, data-subject rights, product-liability claims by injured persons (PLD Art. 15 from 2026-12-09) | B2B standard-term caps face national review (for example German AGB rules) [G] |

## E.2 Tax, payroll and e-invoicing calculations [I]

(1) Ship **versioned, dated rule packs** with statute citations and effective-from dates; (2) store pack version and inputs on every computed line (audit trail); (3) state output is "as computed from your data and the published rules", filing stays with the customer; (4) offer a **separate paid compliance-assurance product** with a defined remedy (for example penalty reimbursement up to a multiple of annual fees) carried by its own insurance, rather than widening the general warranty; (5) commit to update windows (file 17 s8); (6) flow down e-invoicing network providers' liability terms; (7) flag countries where payroll or e-invoicing software needs certification. Odoo's floor: no compliance warranty, 50% cap [V].

## E.3 Insurance

Insurer price pages returned 404, so **I have no verified premium figures.** Quote structure [I]: tech E&O/professional indemnity expressly covering tax, payroll and e-invoicing calculation software; cyber (incident response, business interruption, third-party, regulatory defence); D&O for the foundation; confirmation there is no exclusion for distributing AGPL code; sub-limit for statutory penalties; retroactive date. Regulatory fines (CRA, GDPR) are largely uninsurable [G]. Typical early limits EUR 1-5M [G]. Get three broker quotes before setting caps.

## E.4 EU Data Act switching duties (Regulation (EU) 2023/2854; applies from 2025-09-12)

SaaS is a data processing service delivery model [V recital], so obop Cloud is in scope.

| Rule [V OJ text] | Content |
|---|---|
| Art. 25 contract terms | Written terms; **max 2 months' notice; 30-day transition (if technically unfeasible, the provider must say so within 14 working days and may offer an alternative period of up to 7 months, Art. 25(4)); exhaustive list of exportable data; at least 30 days' retrieval afterwards; full erasure; exit-strategy support** |
| Art. 29 charges | Cost-based reduced charges allowed 2024-01-11 to 2027-01-12; **none from 2027-01-12**; fees and penalties disclosed pre-contract |
| Art. 30 interfaces | Open interfaces available to an equal extent to all customers and destination providers, free of charge (Art. 30(2)); without harmonised standards, export all exportable data in a structured, commonly used, machine-readable format (Art. 30(5)) |
| Art. 23 obstacles | No barriers to switching, including after a **free tier** |
| Art. 31 exceptions | Custom-built services not sold at broad scale are exempt only from Art. 23(d), Art. 29 (charges) and Art. 30(1) and (3), so Art. 25 contract terms still apply; non-production trials for a limited period are exempt from the whole Chapter |

Product consequences [I]: full export (JSON/CSV, attachments, audit trail, documented schema) in every edition, never gated; free migration help from 2027-01-12.

## E.5 European Accessibility Act (file 19)

Directive (EU) 2019/882 covers products placed on the market and services provided to consumers after 2025-06-28, including **e-commerce services**, consumer banking and payment terminals; microenterprises providing services are exempt (Art. 4(5)); penalties are national (Art. 30); service contracts agreed before 2025-06-28 may continue unaltered until they expire, at most five years (Art. 32(1)) [V OJ text]. Exposure [I]: a pure B2B back office is largely outside, but **obop's storefront, checkout, customer portal, website builder and POS/payment front end are inside** when merchants sell to consumers, and shipping an inaccessible default makes "the merchant is responsible" a thin defence. Default themes must meet EN 301 549 / WCAG 2.2 AA (file 19); ship an accessibility-statement template. National fines: counsel.

## E.6 Sanctions and export

- **Russia IT/software services:** OFAC published "Prohibition on Certain Information Technology and Software Services" under E.O. 14071 s1(a)(ii) on 2024-06-12 [V https://ofac.treasury.gov/recent-actions/20240612]. The determination (PDF at https://ofac.treasury.gov/media/932951/download?inline) prohibits exporting, selling or supplying from the US, or by a US person wherever located, to any person located in Russia (1) IT consultancy and design services and (2) IT support services and cloud-based services for "enterprise management software and design and manufacturing software"; it took effect 2024-09-12, with carve-outs for US-owned Russian entities, wind-down or divestment services, and software licensed or authorised by Commerce [V]. The EU has a comparable restriction in Regulation 833/2014 (unverified; the article was not located in the OJ texts of Reg. 2022/1904 and 2024/1745 that I searched). Both plausibly reach **hosted ERP and paid support** even where the code is open source: counsel to define screening, geo-blocking and contract terms.
- **US encryption:** publicly available encryption source code is "not subject to the EAR" under 15 CFR 742.15(b), with email notification to BIS and NSA only for *non-standard cryptography* [V https://www.law.cornell.edu/cfr/text/15/742.15]; use standard algorithms via well-known crates to stay out of that path [I]. EU dual-use crypto rules: counsel [G].

## E.7 Consumer-law exposure of e-commerce

The merchant is the trader, but the module hard-codes the flows, so defects become shared exposure [I].

| Rule | Status | Product impact |
|---|---|---|
| Directive 2023/2673 adds a **withdrawal function** to the Consumer Rights Directive (Art. 11a); member states apply from **2026-06-19** [V] | Applicable now | Always-visible withdrawal button with a confirmation step for online distance contracts |
| **PLD (EU) 2024/2853**: "product" includes software; applies to products placed on the market after **2026-12-09**; excludes FOSS developed or supplied outside a commercial activity; liability cannot be limited by contract (Art. 15) [V] | Imminent | Commercial editions and hosted components in scope; covered damage is to natural persons [I]; insurance must respond |
| CRD 2011/83 (14-day withdrawal, "order with obligation to pay"), Omnibus 2019/2161 (prior-price, ranking, reviews), Directives 2019/770 and 771 (digital content, goods conformity), GDPR/ePrivacy, DSA if multi-vendor | In force [G] | Checkout defaults, price history, returns and warranty flows, consent manager |
| Digital Fairness Act | Proposal expected (unverified; status page not retrievable) [G] | Watch dark patterns and cancellation flows |

## E.8 Counsel decisions

Entity structure and CRA role; ToS/DPA/SLA and caps per market; compliance-assurance product and insurance; sanctions screening scope; Data Act contract and export design; EAA scope and national penalties; consumer-law defaults and responsibility allocation in merchant terms; product-liability positioning for commercial editions; docs licence; Part A items.

---

## Confidence & gaps

- **High [V]:** USPTO negative result for OBOP; package/domain state (point-in-time); Odoo contract, SLA, security and support terms; Frappe SLA and cloud terms; Supabase terms; docs-tool versions and dependencies; Vite+ docs stack; Data Act, PLD, EAA and withdrawal-function OJ text; Weblate pricing; repo and PR statistics.
- **Medium:** newcomer latency (Frappe and Twenty rows cover only the latest 300 merges; the Starlight and VitePress rows re-ran with different counts, see C.3); Odoo Experience budget (secondary, file 03, unverified); Discourse/Zulip terms; Odoo mailing-list CoC and "OCA Days since 2018" (unverified).
- **Not done:** EUIPO, TMview, WIPO, UKIPO, DPMA, INPI searches; EUIPO/USPTO fee pages; insurance premiums; cost-per-ticket benchmarks; EU Russia ERP-software article and OFAC scope; native-language name review; Twenty events; Odoo certification terms; whether Crowdin, Weblate or Zulip free plans accept an open-core company; Digital Fairness Act status. Support-cost and staffing tables are models from stated assumptions, not observations.

## Implications for obop

**MUST**
1. **MUST NOT** announce or spend on "obop" before counsel completes EUIPO, UKIPO, WIPO, DPMA, INPI searches in classes 9/35/42 (and 41/45) and opines on OBOPP and OBOL; keep two screened fallbacks (A.5).
2. **MUST** reserve the `obop` crate, PyPI name, an unused GitHub org variant (for example `obop-oss`), GHCR path, Matrix room and social handles now; the npm registry name `obop` is taken (use the `@obop/*` scope, published with pnpm) and Docker Hub `obop` is squatted.
3. **MUST** publish the A.6 trademark policy, hold marks in the foundation, and settle the owner before any US filing.
4. **MUST** build docs on VitePress 2/Vite 8 inside Vite+ (pinned, plain Markdown; driven only through `vp` with pnpm as the package manager, owner constraint 2026-10-01), with CI for compiled examples, Vale, lychee, typos and a registry-coverage gate; generate reference pages from the model registry.
5. **MUST** publish security advisories to everyone at once; **MUST NOT** copy Odoo's Enterprise-only 2-3 week head start; **MUST** have the 24h/72h/14-day CRA runbook and an SRP account ready before the first paid tenant.
6. **MUST** ship full data export in every edition and write Data Act switching terms (2 months' notice, 30-day transition, 30-day retrieval, erasure, no switching charges from 2027-01-12) into the hosted contract.
7. **MUST** disclaim legal-compliance warranties by default, store rule-pack version and inputs on every tax/payroll computation, and keep statutory carve-outs uncapped.
8. **MUST** make the default storefront, checkout, portal and POS front end WCAG 2.2 AA conformant and ship the consumer withdrawal function.

**SHOULD**
9. **SHOULD** price human support explicitly: bug/platform "Product Warranty" in the base tier, partner routing for functional help, paid Priority; publish a Frappe-style severity table.
10. **SHOULD** use Weblate (not Crowdin) with an MT-then-review pipeline and an 80% "supported" threshold.
11. **SHOULD** run GitHub Discussions, Zulip and a self-hosted Discourse; adopt Contributor Covenant 3.0 with a non-employee on the enforcement committee.
12. **SHOULD** use a one-click GitHub CLA bot with a public ledger and the OSI-licence-forever pledge, not Odoo's file-in-PR signing.
13. **SHOULD** adopt a scaled-down Kubernetes ladder with module and localisation maintainers, and an RFC repo modelled on Vite+'s `rfcs/`.
14. **SHOULD** run a free-to-join packager/host pre-notification list instead of customer-tiered embargoes.
15. **SHOULD** launch certification (functional, accountant, developer, operator) at 50 production tenants with free content and paid exams.
16. **SHOULD** collect three broker quotes for tech E&O and cyber, and engage counsel on EU/US sanctions covering ERP services, before offering paid hosting globally.

**COULD**
17. **COULD** buy `obop.com`/`obop.org` if under about EUR 5-10k [G]; otherwise launch on a `.dev`/`.org` secondary domain.
18. **COULD** start an OCA-style `obop-community` org at 20 third-party modules, fund events by partner sponsorship (Frappe model) capped at 5-8% of revenue, and publish `llms.txt` plus a docs MCP server; keep Starlight as the docs fallback.

---

## Verification log

Adversarial fact-check run on 2026-10-01 against sources different from the author's where possible: the USPTO trademark search back-end queried directly (not through the UI); the npm, crates.io, PyPI, Docker Hub, WordPress.org and GitHub APIs; Verisign and PIR RDAP for domains; EUR-Lex/Publications Office XHTML for the Data Act, PLD, Directive 2023/2673, EAA, CRA, EUTMR and the 14th sanctions package; raw files from the Odoo documentation repo (`enterprise.rst`, `conf.py`, `.weblate.json`); the OFAC determination PDF; vendor pricing and policy pages; and recomputed GitHub statistics.

**Confirmed as stated: about 110 claims.** All 12 flagged load-bearing claims hold in substance (the USPTO claim and the Data Act claim gain refinements below). The rest are secondary figures counted by claim group: package, domain and handle states; USPTO near-mark classes and dates; Odoo, Frappe, Supabase, Twenty, Discourse, Zulip and Weblate terms and prices; docs-tool versions, dates and dependencies; contributor, merged-PR and good-first-issue counts; Odoo docs repo statistics; CRA, Data Act, PLD, EAA and withdrawal-function text and dates; OFAC and EAR citations; trademark-policy summaries; Kubernetes ladder, Contributor Covenant and CLA details.

**Corrections and additions (was / now / source)**

| Was | Now | Source |
|---|---|---|
| OBOL (Obol Association) "live, filed 2024-10-29" | Registered (Reg. 7973449, 2025-10-07, use-based); class 42 includes e-commerce platform hosting and software programming. Added a second live OBOL registration, The Obol Corporation, class 9, Reg. 6028386 (2020-04-07) | USPTO back-end (`tmsearch.uspto.gov/prod-stage-v1-0-0/tmsearch`) |
| OBOPP "live intent-to-use, unregistered" | Also: published for opposition 2025-02-25; status "SECOND EXTENSION - GRANTED" (past examination, statement of use pending) | USPTO back-end |
| OBOPAY "all dead (2005-2008)" | All dead; 2005-2008 are filing dates (registrations issued 2007-2012, now cancelled) | USPTO back-end |
| WordPress plugin "0 installs" | 999 lifetime downloads, 0 ratings; active installs not reported | https://api.wordpress.org/plugins/info/1.0/obop.json |
| Odoo gives Enterprise customers "2-3 weeks" advance notice | Contract commits to at least 2 weeks; the public page says "e.g. 2-3 weeks" as an example | `enterprise.rst` s4.2; https://www.odoo.com/security-report |
| US ITU assignability "[G]" | [V]: not assignable before use is alleged except to a successor to the ongoing business | 15 U.S.C. 1060(a)(1), https://www.law.cornell.edu/uscode/text/15/1060 |
| EUIPO and USPTO fees "[G, pages not retrievable]"; "roughly EUR 3-4k" for classes 9+35+42 in EU, UK, US | EUIPO EUR 850 / 50 / 150 and USPTO USD 350 per class verified; UK GBP 205 / 60 verified; total is about EUR 2.3-2.5k official fees (EU 1,050 EUR + UK 325 GBP + US 1,050 USD), FX not checked | https://publications.europa.eu/resource/celex/32017R1001 Annex I; https://www.uspto.gov/trademarks/fees-payment-information/summary-2025-trademark-fee-changes; https://www.gov.uk/how-to-register-a-trade-mark/start-your-application |
| Vite+ docs run "VitePress 2 on Vite 8" (stated without qualification) | Pinned alpha.17 declares Vite ^7.3.1 and runs on Vite 8 only through the Vite+ monorepo's `vite` override (Vite+ 1.0.0, 2026-09-28, bundles Vite 8.3.1); alpha.18+ declare Vite ^8.x. The shared `@voidzero-dev/vitepress-theme` is UNLICENSED on npm | https://registry.npmjs.org/vitepress ; https://registry.npmjs.org/@voidzero-dev/vitepress-theme ; https://raw.githubusercontent.com/voidzero-dev/vite-plus/main/packages/core/package.json |
| Vite+ keeps "about 40 RFCs" | 45 Markdown files in `rfcs/` | `gh api repos/voidzero-dev/vite-plus/contents/rfcs` |
| Rust book: 12 authors, one wrote 67 of 80 commits (12 months) | 13 authors, one wrote 73 of 88 commits (2025-10-01 to 2026-10-01) | GitHub commits API |
| OCA 268 public repos | 271 | `gh api orgs/OCA` |
| Odoo "EUR 19.90 per user per month anchor" | Promotional yearly-billing rate for the first 12 months; list EUR 24.90; monthly billing 24.90 promotional / 31.10 list. Support-cost share of revenue recomputed (about 25-42% at list) | https://www.odoo.com/pricing |
| D.5 assumption "tooling EUR 0.5k / 15k / 15k" | 0.5k / 3k / 15k (the only values that reproduce the table's 6.5k / 26k / 154k) | arithmetic on the table [I] |
| Data Act Art. 31 "custom-built services" exempt (unqualified) | Exempt only from Art. 23(d), 29, 30(1) and (3); trials are exempt from the whole Chapter. Added Art. 25(4) (up to 7-month alternative transition) and Art. 30(2) "free of charge" | https://publications.europa.eu/resource/celex/32023R2854 |
| EAA section without transition rule | Added Art. 32(1): service contracts agreed before 2025-06-28 may run unaltered up to five years | https://publications.europa.eu/resource/celex/32019L0882 |
| OFAC scope "I recall ... could not verify [G]" | [V]: IT consultancy and design services, and IT support and cloud-based services for enterprise management and design/manufacturing software, to persons in Russia; effective 2024-09-12 | https://ofac.treasury.gov/media/932951/download?inline |
| Supabase cap "confidentiality, indemnity, gross negligence" | Also fraud and wilful misconduct | https://supabase.com/terms s11 |
| OCA "CLA before commit rights" cited to /page/about | Mission and 2013 founding are on /page/about; the CLA rule is on /page/cla | https://odoo-community.org/page/cla |
| Odoo Experience "about 45,000 attendees" | "45k+" expected on the event page (another section says more than 40,000); the EUR 4.5M budget vs EUR 3.5M income is secondary and unverified | https://www.odoo.com/experience |
| "OCA Days since 2018 [V]" | Only the yearly format and the 2026 Liege edition (21-23 Sept) are verified; "since 2018" is unverified | https://odoo-community.org/ocadays |
| PostgreSQL "18 current, 14-18 supported" | Still correct; added that 19 is in beta (Beta 4 on 2026-09-24) | https://www.postgresql.org/docs/current/docguide.html |
| OpenAPI "track 3.2.1" | 3.2.1 was published 2026-09-10 | https://spec.openapis.org/oas/latest.html |
| Newcomer-latency table (Starlight, VitePress) | Re-run with a slightly different rule gave Starlight 23 of 41 / median 1.65 d and VitePress 14 of 30 / median 0.57 d; same conclusion, counts not identical; note added under the table | GitHub search API |

**Unresolved / unverified**

- Trademark registers not searched: EUIPO, TMview, WIPO, UKIPO, DPMA, INPI (bot-gated); the clearance verdict stays "incomplete".
- Odoo mailing-list Code of Conduct "v1, 2017": page not found; marked unverified in C.2.
- Odoo Experience budget (EUR 4.5M vs 3.5M income): secondary press via file 03; not re-verified.
- "OCA Days since 2018": unverified.
- EU Regulation 833/2014 provision restricting ERP software and services to Russia: article not located; marked unverified in E.6.
- Digital Fairness Act proposal status: official page not retrievable; marked unverified.
- Newcomer table rows for Frappe, Twenty and OCA/web were not recomputed; the Starlight and VitePress rows differ in count (see above).
- TLD states for `.io`, `.eu`, `.co`, `.de`, `.pl`, `.be`, `.it`, `.se`, `.us`, `.sh` could not be re-checked through RDAP bootstrap (no RDAP or TLS error); the other TLDs matched. X and LinkedIn handles remain unprobeable.
- Whether Crowdin, Weblate Libre or Zulip free plans accept an open-core company: only the published criteria were checked; eligibility judgment stays open.
- Modelled numbers (support staffing and cost per ticket, docs headcount, insurance limits, certification fees, event budgets, metric targets) are assumptions, not observations.
- Exchange rates for the trademark fee total were not checked.

- Owner-constraint pass (2026-10-01): no npm, npx, yarn or bun command existed in this file; every "npm" hit is a registry fact (name availability of `obop`, downloads, dist-tags, the UNLICENSED VoidZero theme) and was kept. Changes: MUST 4 now says the docs toolchain is driven only through `vp` with pnpm as the package manager; MUST 2 says "npm registry name" and that `@obop/*` packages are published with pnpm; the security-advisory channel line now reads "npm registry advisories (what `pnpm audit` reads)", checked against the pnpm.io audit docs source (pnpm 11 queries the registry's bulk advisories endpoint).
