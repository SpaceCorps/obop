# Voice of Customer: what users love and hate about Odoo and its peers (ERPNext, Dolibarr, NetSuite)

Research date: 2026-10-01. Author: research subagent 04 for obop. Evidence tags used throughout: **[V]** verified against a live primary source fetched today; **[S]** secondary sentiment (reviews, forums, HN, blogs; real but biased or anecdotal); **[I]** my inference; **[G]** guess.

## Scope & method

More than 60 fetches today. Primary sources: Odoo docs and docs source on GitHub, Odoo pricing and security pages, GitHub repos and release pages for Odoo, OCA, OpenUpgrade, ERPNext and Dolibarr, the NVD API, and the Frappe forum. Sentiment sources: Software Advice/GetApp (the same Gartner Digital Markets review pool as Capterra), Trustpilot, SourceForge, Hacker News (via the Algolia API, about 15 threads and comment searches), the Odoo forum, and vendor and consultant blogs, which I label as biased.

Blocked or unusable today: G2, TrustRadius and Gartner Peer Insights returned 403. The Capterra URL I guessed returned one 2018 review of the wrong listing. Reddit is blocked for the fetch tool, so Reddit evidence below comes only from search-engine snippets of thread titles and openings, not from reading threads. DuckDuckGo showed a bot check and I did not attempt to bypass it. See "Confidence & gaps".

## 1. Headline findings

Pain themes ranked by breadth across independent sources, severity (money lost or operations blocked) and how often they appear in "why we left" stories. The last column says whether a greenfield Rust backend plus modern frontend can fix the pain by technology alone, by governance and business model alone, or needs both.

| Rank | Pain theme | Severity | Fix lever |
|---|---|---|---|
| 1 | Upgrade and migration pain | Very high | Technology plus governance (strong opportunity) |
| 2 | Pricing, licensing and open-core surprises | Very high | Governance and business model only |
| 3 | Customization debt | High | Technology plus governance |
| 4 | Support quality and partner dependence | High | Mostly neither (partly product design) |
| 5 | Accounting localization gaps | High | Labor plus data-driven design |
| 6 | App-store and third-party module quality variance | Medium-high | Governance plus technology |
| 7 | Performance at scale | Medium | Technology, but DB-bound limits |
| 8 | UX inconsistency and learning curve | Medium | Technology plus design discipline |
| 9 | Hosting and ops burden | Medium | Technology (strong) |
| 10 | Documentation and API | Medium | Process, helped by tooling |
| 11 | Testing culture | Low-medium (thin evidence) | Technology plus process |
| 12 | Security and disclosure | Medium (thin evidence) | Partly technology, partly governance |

Praise themes ranked: (1) one integrated system on one data model, (2) modular, incremental adoption, (3) price/value, (4) open, hackable framework with a real community, (5) operational depth in inventory, sales-to-invoice and multi-currency, (6) ecosystem and talent pool, (7) acceptable-to-good UX after ramp-up.

Core insight [I]: the two worst Odoo pains (upgrades, pricing/open-core) are mostly *design and business-model choices*, not Python limitations. A Rust rewrite with the same model would reproduce them. The opportunity is in the delivery contract (upgradable, open, predictable), not raw language speed.

## 2. Odoo baseline (verified today)

| Fact | Value | Source |
|---|---|---|
| Current major | Odoo 20; `20.0` is the default GitHub branch, updated 2026-10-01; release notes dated "September 2026" (no exact day) [V] | https://github.com/odoo/odoo/branches , https://www.odoo.com/odoo-20-release-notes |
| Previous major | 19.0 released 2025-09-18 [V, via Wikipedia] | https://en.wikipedia.org/wiki/Odoo |
| Support windows | Standard support is 3 years from release: 20.0 (September 2026) to September 2029 (planned); 19.0 to September 2028 (planned); 18.0 to September 2027 (planned); 17.0 standard support ended September 2026 and is now extended support at extra fee [V, added in verification] | https://raw.githubusercontent.com/odoo/documentation/master/content/administration/standard_extended_support.rst |
| Repo size | 54.8k stars, 33.9k forks, about 211.7k commits (211,739 via GitHub API on 2026-10-01) [V] | https://github.com/odoo/odoo |
| Community license | LGPLv3 [V] | https://raw.githubusercontent.com/odoo/odoo/master/LICENSE |
| Enterprise license | Proprietary "Odoo Enterprise Edition License v1.0" (short id OEEL-1); use requires a valid Odoo Enterprise Subscription for the correct number of users [V] | https://raw.githubusercontent.com/odoo/documentation/19.0/content/legal/licenses/enterprise_license.txt , https://www.odoo.com/documentation/19.0/legal/licenses.html |
| Frontend framework | Owl, Odoo's own TypeScript UI framework, LGPLv3, README says "Owl 3.0.0 Alpha" [V] | https://github.com/odoo/owl |
| Plans (EUR/user/month) | One App Free; Standard list 24.90 yearly-billed, promotional 19.90 (list 24.90 less a 5.00 "First Year Initial Discount"); Custom list 44.90 yearly-billed, promotional 35.90 (less 9.00); Light User 7.90 by quote. Monthly-billed: Standard 24.90 promotional vs 31.10 struck-through list, Custom 44.90 vs 56.90 (monthly list prices read from the page's struck-through figures, not from the monthly configurator) [V/I]. So the headline 19.90 / 35.90 are first-12-month prices for initial users, not steady-state list prices (corrected in verification) | https://www.odoo.com/pricing-plan , https://www.odoo.com/pricing-configurator?plan=standard , https://www.odoo.com/pricing-configurator?plan=custom |
| Plan gating | Studio / agentic AI, multi-company and the external API are Custom-only (the FAQ says One App Free may use multiple companies and may choose Studio as its one app; installing Studio or adding companies switches a database to Custom); Odoo.sh and on-premise hosting are listed only under Custom; the discount lasts 12 months and applies to initial users only; IAP credits (SMS, contact auto-complete, lead generation, AI bill/expense scanning) are not included, and Odoo 20 makes all AI features credit-based; Odoo.sh hosting cost and custom-code maintenance are not included [V] | same |
| Ecosystem | App store lists about 93.6k apps across all versions (93,609 at first fetch, 93,612 on re-check; 5.0 to 20.0) [V]; OCA has 664 members, about 1,600 contributors, 271 GitHub repos, and advertises "+20 000" modules [V] | https://apps.odoo.com/apps/modules/browse , https://odoo-community.org/ , https://github.com/OCA |
| Review scores | Software Advice/GetApp 4.2/5 from 1,332 reviews (53% 5-star, 7% 1-star; ease 4.0, support 3.9, value 4.1, functionality 4.2) [V]; Trustpilot 3.1 from 1,280 reviews, 46% 5-star vs 43% 1-star [V]; SourceForge 4.2 is an aggregate of 1,714 ratings (12 SourceForge reviews, 1,702 third-party ratings; the 12 SourceForge-native reviews average 4.7) [V] | https://www.softwareadvice.com/erp/odoo-profile/reviews/ , https://www.trustpilot.com/review/odoo.com , https://sourceforge.net/software/product/Odoo/ |

The Trustpilot split is bimodal. Some Software Advice/GetApp reviews are marked "invited by us ... offered a nominal incentive" (by the review site, not necessarily the vendor), while others are marked organic with no incentive, so scores may be inflated [S] (stated on the review pages).

## 3. Praise themes (ranked)

**P1. One integrated system on a shared data model.** The top pro on Software Advice is the unified platform (CRM, sales, invoicing, inventory, accounting, HR, projects), with less duplicate entry [S] https://www.softwareadvice.com/erp/odoo-profile/reviews/ . On HN the "unified data model on open tech" is called the fundamental win, and a reviewer cites a cleaner audit trail and replacing "hundreds of spreadsheets" [S] https://news.ycombinator.com/item?id=46439993 . This is the obop thesis, so it is validated demand.

**P2. Modular, incremental adoption.** Start with a few apps and add more; "One App Free" is a strong funnel [V/S]. Repeated in Software Advice and GetApp pros.

**P3. Price/value against SAP, NetSuite and Dynamics.** Value-for-money 4.1/5 on Software Advice [V]. An HN user calls Enterprise good value when processes fit the supported regions [S] https://news.ycombinator.com/item?id=46439993 . The caveat is that value erodes with customization and surprise bills (pain 2).

**P4. Hackable open framework.** Developers like the ORM-level permissions (CRUD plus record rules), runtime model and field creation, shared domain syntax for rules and filters, and kanban views [S] https://news.ycombinator.com/item?id=46485746 . A commenter with six years on it across companies calls it essentially a Python web framework that happens to include business apps, about as easy to run as a Django app on Postgres, and notes OCA is mostly people paid to work with Odoo [S] https://news.ycombinator.com/item?id=21869046 . One developer finds it far less awkward than Oracle or SAP [S].

**P5. Operational depth in sales-to-invoice, inventory and warehouse, multi-currency, bank feeds.** Repeated across Software Advice and GetApp [S]. One HN practitioner rates production, inventory and accounting as "second to none" and says free modules cover about 90% of cases [S] https://news.ycombinator.com/item?id=28985377 . Caveat from GetApp: its inventory-management summary (118 reviews, July 2021 to October 2024) says some users have difficulties with large inventories above 20k items; this is a site summary, not an individual dated review [S].

**P6. Ecosystem and talent.** Large partner network and community; one commenter calls it nearly the default ERP in Belgium [S] https://news.ycombinator.com/item?id=46439993 .

**P7. Acceptable UX after ramp-up.** Reviewers describe the interface as modern and intuitive once learned, and praise automation and templates [S] (Software Advice page 2). This is the weakest praise theme because the learning curve is the top con (pain 8).

**P8 (polarized). Support and Studio.** Trustpilot 5-star reviews from September 2026 praise named support agents and a hands-on training session [V]. The same page has 1-star reviews on slow support. Studio gets praise for no-code models and criticism for limits [S] (Software Advice page 2).

## 4. Pain themes (ranked), with fixability

### 4.1 Upgrade and migration pain (rank 1)

**Evidence.**
- Odoo's own process: custom or third-party modules are outside the upgrade SLA unless under a maintenance contract. The docs state a database with custom modules cannot be upgraded until a target-version build of those modules exists. Every major version gets 3 years of standard support, and Online databases on a major version must upgrade every 2 years; on-premise may stay on one version indefinitely, though not recommended [V] https://raw.githubusercontent.com/odoo/documentation/19.0/content/administration/upgrade.rst . Odoo 17.0 standard support ended September 2026 (now extended support at extra fee) [V] https://raw.githubusercontent.com/odoo/documentation/master/content/administration/standard_extended_support.rst .
- Enterprise agreement v13 (dated 2026-09-24; verified against the agreement text in the docs source, though the signed contract governs): "Covered Versions" are the 3 most recent major versions; a database on an older version pays, once per year and no earlier than 6 months after a new major release, an extra fee of 25% of the annualized price (based on current per-user price and user count); self-hosted upgrade requests must include a copy of the database [V] https://raw.githubusercontent.com/odoo/documentation/19.0/content/legal/terms/enterprise.rst . With 20.0 out (September 2026), databases on 17.0 or older become non-covered versions, so the 25% fee would start to apply from roughly March 2027 [I].
- Self-hosted Enterprise user on v15/v16 had to send a full production dump off-site, with no path for third-party addons; a small partner nearly lost customers because on-prem customers cannot run the scripts themselves [S] https://news.ycombinator.com/item?id=46486079 , https://news.ycombinator.com/item?id=46486350 . Another on-prem user called the OpenUpgrade path awful and dropped Odoo [S] https://news.ycombinator.com/item?id=47916231 .
- Odoo's representative on HN says upgrades need a managed service, that problems persist after 1,000+ upgrades, and that it also monetizes Enterprise [S, vendor voice] https://news.ycombinator.com/item?id=46486373 .
- A 24-user Enterprise v15 firm with little customization was quoted about 272 hours for v17 (r/Odoo, Jul 2024; via search snippet, not read directly; Reddit is blocked to my tools, so this is unverified) https://www.reddit.com/r/Odoo/comments/1e8szsw/upgrading_odoo_shouldnt_be_so_expensive/ .
- OCA OpenUpgrade: 35 open issues and 46 open PRs (GitHub search API on 2026-10-01; the first pass recorded 48 PRs, a number that moves daily). Accounting/l10n is the largest cluster among the open issues (about 6 of 35: missing suspense account, unassigned account_root_id, invalid asset move names, one l10n_fr and one l10n_es constraint/XML-ID problem, account_edi_format FK violation), alongside HR, payment, mail, inventory and other module failures, and about 7 migration-tracker or process issues; these span 12.0 to 18.0 hops. A "Migration to version 20.0" tracker (#6044) was opened on 2026-09-29 [V] https://github.com/OCA/OpenUpgrade/issues , https://github.com/OCA/OpenUpgrade .
- Odoo keeps making breaking changes yearly. Odoo 20 removes record rules in favor of domains on access rights, removes tracking values, swaps Font Awesome for Material Symbols, replaces account groups with parent accounts and asset models with depreciation models [V] https://www.odoo.com/odoo-20-release-notes . The XML-RPC/JSON-RPC `db` service is removed in Odoo 20 (and Online 19.1); `common`/`object` services are scheduled for removal in Odoo 22 (fall 2028) and Online 21.1 (winter 2027) per the master/20.0 docs; the 19.0 docs still say all three services are "scheduled for removal in Odoo 22" [V] https://raw.githubusercontent.com/odoo/documentation/master/content/developer/reference/external_api.rst .
- Reviews: version changes break custom modules; costly upgrades of customized builds; a disputed "unsupported version" fee (Software Advice page 2, 2026) [S].
- Vendor blogs put typical partner-run upgrades at $1.5k to $15k and $30k+ for heavy customization [S, biased] https://iventureteam.com/blog/odoo-upgrade-cost/ .

**Can a Rust + modern frontend OSS alternative fix it?** Yes, in large part; this is obop's best differentiator [I]. How:
1. Customizations as *data* (custom fields, formulas, views, automations, state machines) that migrate with the schema, so most "customization" needs no porting.
2. A semver'd, narrow host API for code extensions, running as sandboxed WASM or out-of-process plugins. Rust has no stable ABI, so avoid in-process dynamic loading.
3. Forward-only, transactional, idempotent migrations with dry-run and a report, runnable by the customer on-prem; never require shipping a database to the vendor.
4. CI gate: upgrade a fixture database across N-1 and N-2 with the extension matrix on every merge.
5. Rust's type system catches API drift at compile time for in-tree modules.
Limits: real custom logic still breaks when business rules change; regulatory data (tax tables, charts of accounts) changes every year regardless of language.

### 4.2 Pricing, licensing and open-core surprises (rank 2)

**Evidence.**
- Feature drift from Community to Enterprise over versions: v12 moved accounting reports, budgets and assets ("iirc", one user's recollection); payroll left in v13 (another user's claim) (HN Dec 2019) [S, unverified against release notes] https://hn.algolia.com/api/v1/items/21865699 . The founder replies that they try to keep about 80% of what they do open source and 20% Enterprise [S, vendor voice].
- A Dec 2025 to Jan 2026 HN thread (posted 2025-12-31, cited comments dated 2026-01-04) again centers on open-core economics (commenters say the important modules and "most powerful" features are Enterprise-only) and forced upgrade handling [S] https://news.ycombinator.com/item?id=46439993 .
- Odoo 20: Türkiye's Nilvera e-invoicing integration is now Enterprise-only; all AI features now require purchased credits [V] https://www.odoo.com/odoo-20-release-notes .
- Pricing mechanics: the headline 19.90 (Standard) and 35.90 (Custom) yearly-billed prices are first-12-month promotional prices for initial users; list is 24.90 and 44.90, i.e. a 20% first-year discount and a 25% step-up after year one for the same seats, assuming the discount lapses as the footnote says [V/I; list and discount read from the configurator pages]; Studio, multi-company and external API gated to Custom; IAP credits; Odoo.sh extra [V] https://www.odoo.com/pricing-plan , https://www.odoo.com/pricing-configurator?plan=standard . External API is Custom-only per the JSON-2 docs [V].
- Contract terms (agreement v13, dated 2026-09-24, read from the docs-source text): renewal charges increase by up to 7% per year of the previous Term, where previous charges (excluding any non-covered-version fee) are below the current list price; an extra fee of 300% of the applicable list price for the actual number of Users and Light Users if the customer breaches the Access-to-the-Software section (including the unmodified execution of the license-verification code); Covered Extra Modules (custom code under maintenance) billed as a monthly fee per 100 lines of code (rounded up), counted with the `cloc` command; 30 days' written notice before changing the main contact to another Odoo Partner or to Odoo SA directly [V] https://raw.githubusercontent.com/odoo/documentation/19.0/content/legal/terms/enterprise.rst . The signed contract governs; the docs copy is the public template.
- Reviews: annual fee jumped from about $567 to over $5,000; monthly bill more than doubled because a small part of operations sat in a pricier country; support billed in 15-minute increments (Software Advice, Aug 2026) [S]. Trustpilot complaints (late September 2026) on cancellation, annual-only plans and growing costs; in at least one reply (to a 2026-09-22 review) Odoo says replicating every existing process can significantly increase complexity, duration and cost [V] https://www.trustpilot.com/review/odoo.com .

**Fixable?** Not by technology. A Rust rewrite with an open-core Enterprise tier would reproduce every one of these. Fixing requires governance: one edition, no feature migration between editions, published price caps, ungated API. [I]

### 4.3 Customization debt (rank 3)

**Evidence.** Odoo is opinionated: you adapt your process or pay heavily [S] https://news.ycombinator.com/item?id=38931021 . Unbounded customization makes NGOs dependent on a partner and costs creep [S] https://news.ycombinator.com/item?id=46920322 . Developer loop is slow: template changes require a module upgrade, and the frontend mixes Owl with older styles [S] https://news.ycombinator.com/item?id=32776835 , https://news.ycombinator.com/item?id=36091418 . (The earlier claim that xpath inheritance is fragile is not in those cited comments: unverified.) Reviewers say Studio is limited and real changes need a programmer (Software Advice page 2, e.g. a January 2026 hospitality review) [S]. Odoo's Trustpilot replies lean the same way: works out of the box for standard processes, and one reply says replicating every existing process raises cost [V].

**Fixable?** Partly [I]. Give users a strong no-code and low-code tier (typed custom fields, expression-language computed fields, workflow states, view layout as versioned data) so the 80% case never becomes code. Provide an "customization report" (what differs from stock, upgrade risk) so debt is visible. Cannot eliminate it: any true custom logic is debt.

### 4.4 Support quality and partner dependence (rank 4)

**Evidence.** Customer support is the lowest Software Advice sub-score (3.9) [V]. Horror stories: 400+ hours contracted on Brazilian tax setup, implementation still unfinished (Software Advice, May 2026); database access shut down or restricted during a dispute over an unsupported-version fee (Software Advice, May 2026; the same review appears on GetApp); a custom project unfinished after a year and a database created under a partner's account and later deleted (both unverified: not found in the pages I re-read) [S]. HN: support not included, so third-party support erases cost savings; Enterprise support slow [S] https://news.ycombinator.com/item?id=46439993 .

**Fixable?** Mostly not by technology [I]. Product design can reduce ticket volume: in-app diagnostics, self-serve data import validation, export of everything at any time, and no vendor-controlled database lock. The OSS model can let customers switch partners freely (Odoo's agreement requires 30 days' notice to change partner).

### 4.5 Accounting localization gaps (rank 5)

**Evidence.** Odoo's docs list 103 country entries with a fiscal-localization package (recounted from the 19.0, master and 20.0 docs source; the first pass said roughly 105), but the docs themselves say packages need fine-tuning of the chart of accounts, taxes, statements and certifications "and sometimes more" [V] https://www.odoo.com/documentation/19.0/applications/finance/fiscal_localizations.html . A German user found no automatic tax pre-registration and tax reports missing data unless the chart was manually mapped to tax grids (2022) [S] https://blog.m5e.de/post/the-problem-with-odoo/ . Software Advice reviews in May and August 2026 report Brazil and Chile setups needing third parties or lacking export-invoicing fields, and GetApp's summary of 2021-2024 reviews says Odoo lacks adaptation to Romania's accounting system (a site summary, not a dated 2026 review) [S]. OpenUpgrade's open issues include one l10n_fr and one l10n_es migration breakage, plus several account/account_edi ones [V]. Accounting itself is where open-core bites: several HN users say accounting is the missing piece of Community [S] https://news.ycombinator.com/item?id=33877741 . Peers struggle too: ERPNext finance does not suit Indian accountants on Tally; Dolibarr lacks Italian tax rules and Chile SII built-in [S].

**Fixable?** Partly [I]. It is mostly country-specific labor plus certification. Design levers: localization as declarative data packs (chart, taxes, fiscal positions, report templates, e-invoice formats) in a separate versioned repo; golden-file tests for tax reports; named country maintainers; launch with a few countries done well instead of 100 nominal ones. Rust helps with strict decimal money types and typed e-invoice schemas, not with the law.

### 4.6 App-store and third-party module quality variance (rank 6)

**Evidence.** About 93.6k listings across all versions (93,609 at first fetch, 93,612 on re-check) [V]. Users report inconsistent module quality, costly apps and unhelpful forum answers [S] https://news.ycombinator.com/item?id=21868241 , https://news.ycombinator.com/item?id=32776835 . SourceForge reviewers mention add-on instability (a 2021 review) and new-version modules arriving late (a 2019 review), so these are dated examples [S]. A pro-ERPNext blog claims many third-party modules break at major jumps (unsourced, biased) [S] https://khaledelsayed.com/odoo-erpnext-comparison/ . Dolibarr reviewers say the same about DoliStore modules: inconsistent support, some costly [S].

**Fixable?** Yes, mostly through platform governance [I]: compatibility CI run automatically against each core release, a "Verified on vX" badge, capability-manifest permissions (sandbox), signed artifacts, mandatory tests for verified tier, auto-delisting of unmaintained packages. Needs ongoing operational cost.

### 4.7 Performance at scale (rank 7)

**Evidence.** Odoo's own sizing rule of thumb: workers = (CPU x 2) + 1, with 1 worker roughly 6 concurrent users; its memory estimates are about 1 GB RAM for a heavy worker versus 150 MB for a light one (assuming 20% heavy requests and well-designed computed fields and SQL), and multi-process production needs a proxy in front of Odoo routing `/websocket/` to the live-chat worker, which Odoo starts automatically on its own gevent port [V] https://raw.githubusercontent.com/odoo/documentation/19.0/content/administration/on_premise/deploy.rst (identical in the 20.0 docs). Software Advice cons: slowdowns with large datasets or many enabled apps (July 2026, April 2026), occasional performance issues with larger databases (September 2026) [S]; the earlier "slow, laggy database" quote from a 2025 review is unverified (not found); GetApp's summary notes inventory trouble above about 20k items [S]. Odoo forum titles from 2026 ask about slow deletes and updates on large relational models and unused indexes (titles only, bodies not read) [S] https://www.odoo.com/forum/help-1?search=performance+slow+large+database . HN had almost no first-hand scale discussion, so this theme has thinner evidence than upgrades.

**Fixable?** Partly [I]. Async Rust with compile-time-checked SQL and a shared-nothing pool should serve far more users per core and per GB than the 6-users-per-worker model; I guess 10x or better per-core throughput [G]. But much ERP slowness is database-bound (stored computed fields cascading writes, missing indexes, lock contention, heavy reports). Design levers: declare indexes in the model definition, N+1 and query-count budgets in tests, incremental or async recompute of derived aggregates, materialized reporting views, published benchmarks.

### 4.8 UX inconsistency and learning curve (rank 8)

**Evidence.** The most common Software Advice con is the learning curve: an interface that overwhelms at first and setup that needs consultants [S]. Developers report Owl only partly adopted alongside legacy JS, outdated website features, and a basic mobile app [S]. Odoo keeps churning the UI: Odoo 20 swaps the icon set, adds offline mode and multi-record drag and drop [V] https://www.odoo.com/odoo-20-release-notes . Peers are not better: ERPNext "functional but less polished", Dolibarr "dated", NetSuite "dated and click-heavy" [S].

**Fixable?** Partly [I]. A single design system, strictly typed view definitions and one frontend architecture avoid the multi-era problem. The hard part is the generic metadata-driven views (form, list, kanban, pivot, gantt) that cover many models while staying usable; this is where Odoo's advantage lives and where clones usually fall short. Vite+ and a modern component stack reduce accidental inconsistency, not design effort.

### 4.9 Hosting and ops burden (rank 9)

**Evidence.** Running Odoo well needs a proxy, a websocket/live-chat worker, worker and memory tuning, PostgreSQL care, and backups (an OCA auto_backup module exists in OCA/server-tools for 18.0 and 19.0 [V]; that it is "recommended for small deployments" is unverified, not in Odoo's deploy docs) [V/S]. An HN user lost data running Odoo in Docker [S] https://news.ycombinator.com/item?id=20175089 . Odoo.sh cost is extra and reviewers note costs climb with it [S]. The pricing page says on-premise users must run the latest stable version, which sits awkwardly with the upgrade docs saying you may stay on an old version indefinitely (not advised) [V] https://www.odoo.com/pricing-plan . Vendor-hosted customers report database access restricted during disputes (Software Advice, GetApp) [S]. ERPNext self-hosting has the same complaint: a "very peculiar home-grown deployment system" and forum advice that there is "no easy way out" [S] https://news.ycombinator.com/item?id=46485439 , https://discuss.frappe.io/t/transitioning-from-odoo-to-erpnext-deployment-and-upgrade-concerns/137481 .

**Fixable?** Yes [I]. Single static binary plus Postgres, built-in job queue and websockets, built-in migrate and backup/restore commands, container-optional, health endpoints, and a one-command install. This is the clearest pure-technology win.

### 4.10 Documentation and API (rank 10)

**Evidence.** HN 2026: the external API was XML-RPC/JSON-RPC only, poorly documented, and being removed with little notice; the replies point to the per-database `/doc` page [S] https://news.ycombinator.com/item?id=46439993 . Developers say complex customization takes days reading source, and reference material lags (mostly v13 to v15 for a v16 developer) [S] https://news.ycombinator.com/item?id=36091418 . Status today: the documented JSON-2 API (added in 19.0, bearer API keys, named args, per-database `/doc`) exists, but external API use is Custom-plan only [V] https://raw.githubusercontent.com/odoo/documentation/master/content/developer/reference/external_api.rst . ERPNext docs are praised by a competitor and criticized as thin for client-side development by a reviewer [S].

**Fixable?** Yes, as process [I]: OpenAPI generated from Rust types, docs examples compiled in CI, an SDK with rustdoc, every public endpoint ungated.

### 4.11 Testing culture (rank 11, thin evidence)

**Evidence.** One year-long implementer (HN, Dec 2019) calls testing "a pain" [S] https://news.ycombinator.com/item?id=21868241 . Odoo's docs show a real framework (Python tests, HOOT JS unit tests, test tours, query-count performance tests) [V, outline only; the body was truncated in my fetch] https://www.odoo.com/documentation/19.0/developer/reference/backend/testing.html . OCA repos show CI and pre-commit badges [V]. Frappe v16 changes to `run-tests` created new friction [S] https://discuss.frappe.io/search.json?q=v16%20migrate%20error%20order%3Alatest . I found no systematic evidence either way; treat this as an opinion-level pain.

**Fixable?** Yes [I]: ledger invariants as property tests, DB-backed fixtures, golden-file tests for tax reports, mandatory tests for marketplace "verified".

### 4.12 Security and disclosure (rank 12, thin evidence)

**Evidence.** NVD keyword search counts (crude; includes add-ons and other products with the word): Odoo 59, ERPNext 69, Frappe 163, Dolibarr 170 [V] https://services.nvd.nist.gov/rest/json/cves/2.0?keywordSearch=odoo&resultsPerPage=1 . Odoo's policy: most reports are rejected; the only stated reward is a Hall of Fame listing for critical issues (no monetary bounty is offered); advisories and fixes go first to stakeholders and Odoo Enterprise customers, with public release after a reasonable delay (e.g. 2 to 3 weeks); 86 researchers in the hall of fame, 14 years running [V] https://www.odoo.com/security-report . The agreement promises self-hosted customers advisories at least 2 weeks before public release [V]. The odoo/odoo repo shows zero published GitHub security advisories [V] https://github.com/odoo/odoo/security . A security write-up on a chain of vulnerabilities in the Frappe framework was posted to HN in Aug 2026 (details not verified) [S] https://news.ycombinator.com/item?id=49268986 .

**Fixable?** Partly [I]. Rust removes memory-safety bug classes (mostly irrelevant to Python apps) and makes SQL injection harder via typed queries; authorization, XSS in generated UI and tenant isolation remain design problems. A central, well-tested policy engine matters. Tiered embargoes that favor paying customers are a governance choice obop should reject.

## 5. Peers (shorter form)

### 5.1 ERPNext (Frappe)

Snapshot [V]: GPL-3.0, 39.7k stars, 13.0k forks, 1.7k open issues; v16.37.0 and v15.121.6 both tagged 2026-09-30, so v15 is still maintained alongside v16 [V] https://github.com/frappe/erpnext , https://github.com/frappe/erpnext/releases . Frappe Cloud from $5/month for sites and $40/month for servers [V] https://frappe.io/cloud/pricing . Software Advice 4.5/5 from 140 reviews (ease 4.5, support 4.1, value 4.6) [V] https://www.softwareadvice.com/erp/erpnext-profile/reviews/ .

Praise [S]: fully open, no edition split (frequently contrasted with Odoo) https://news.ycombinator.com/item?id=46485416 ; the DocType approach (one declaration gives table, model, forms and lookups) https://news.ycombinator.com/item?id=49510482 ; easy API scripting https://news.ycombinator.com/item?id=43270987 ; cheap hosting; low cost. Pain [S]: self-hosting complexity; performance under concurrency (long-running Frappe forum history, including a 2026-09-21 thread on v15 concurrent-user performance and deadlock reports) https://discuss.frappe.io/search.json?q=performance%20slow%20many%20users%20order%3Alatest ; v15 to v16 upgrade friction (2026 forum threads on failed patches, "row size too large", permission errors, no rollback command) https://discuss.frappe.io/search.json?q=v16%20migrate%20error%20order%3Alatest ; few implementation partners and uneven support; UI polish and reporting; HR/payroll and project gaps. A pro-ERPNext blog claims ERPNext partner rates run higher than Odoo freelancers ($100 to $150 vs $60 to $90 per hour), unsourced and unverified https://khaledelsayed.com/odoo-erpnext-comparison/ .

Lessons for obop: fully open plus metadata-driven models are loved; upgrades and ops are still painful even without open-core, so these are *technical* problems obop must solve itself. Rust fixability: deploy (yes), performance (partly), upgrades (yes with the same levers as 4.1), partners (no).

### 5.2 Dolibarr

Snapshot [V]: PHP, GPL-3.0, 7.7k stars, 3.5k forks, 649 open issues, 371 PRs; 24.0.0 (2026-08-20) and 24.0.1 (2026-09-07) are the latest releases, with 23.0.4 (2026-08-20), 22.0.5 (2026-05-26) and even 18.0.10 (2026-05-07) still getting patches (the year is confirmed as 2026 by the GitHub API `published_at` timestamps; the first pass had inferred it) [V] https://github.com/Dolibarr/dolibarr/releases ; 24.0.0's release notes list an "Experimental MCP server and AI Assistant" (#37888). Software Advice 4.5/5 from 107 reviews with no 1 or 2 star ratings (ease 4.3, support 3.7, value 4.7) [V] https://www.softwareadvice.com/erp/dolibarr-profile/reviews/ .

Praise [S]: free, no proprietary edition; easy install and hosting; switch modules on and off; lightweight; long-lived stability with few dependencies https://news.ycombinator.com/item?id=46600437 ; automatic VAT handling https://news.ycombinator.com/item?id=39511584 . Pain [S]: dated or click-heavy UI (the most common complaint); thin HR, costing, production and reporting; add-on modules sometimes costly or poorly integrated; country gaps (Italy, Chile SII, India GST); PDF/document customization needs technical skill; limited docs and uneven support; one older review says upgrades are slow, others praise frequent releases. A Dolibarr developer (biased) claims about 1M users and 1,200 add-ons versus Odoo's 16M and 40,000, and positions it for firms below about 5,000 staff https://news.ycombinator.com/item?id=46486460 .

Lessons for obop: simplicity and no edition split win small-business loyalty; an aging UI and feature gaps cap growth. Rust fixability: UI (yes), feature breadth (no, that is work), localization (no).

### 5.3 NetSuite

Snapshot [V/S]: proprietary Oracle SaaS. Software Advice 4.2/5 from 2,076 reviews (47% 5-star, 3% 1-star; functionality 4.2, value 3.9, support 3.8) [V] https://www.softwareadvice.com/erp/netsuite-profile/reviews/ . Vendr (page last updated February 2026): median buyer about $74.8k per year (observed range $7,160 to $299,115; the page header labels $74,816 "average" and the body $74,817 "median", and it cites 1,512 purchases in one place and 647 deals in another), small (10 to 25 users) license $30k to $75k per year, typical renewal uplift 5 to 10 percent per year, premium support 10 to 20 percent of license, implementation $25k to $75k for small firms, customization $150 to $300+ per hour [S; Vendr's anonymized transaction data plus estimate ranges with no stated sample size, not official Oracle pricing] https://www.vendr.com/marketplace/netsuite . Prices are quote-based; the official pricing page returned 403.

Praise [S]: deep customization (SuiteFlow workflows, scripting via SuiteScript), unified finance-to-inventory platform, multi-entity and multi-subsidiary scale, real-time dashboards. Pain [S]: complexity and learning curve, dated interface, reliance on specialists for saved searches and scripts, high cost for small firms, long implementations, uneven support with long-standing bugs, lag on large datasets (Software Advice, 2026 lowest-rated page). A 2015 HN comment (dated) says NetSuite/Oracle/SAP-class systems mean a minimum 3 to 6 month implementation and $50k to 100k+ of spend, too much for an early startup https://news.ycombinator.com/item?id=9557648 ; the earlier claim that 2013 to 2015 HN comments describe a poor integration experience and pricing moving upmarket is unverified (not found in the cited item).

Lessons for obop: the market pays for deep scriptable customization that survives automatic upgrades (background knowledge on NetSuite's semiannual releases, not verified today [G]). obop can undercut the cost structure and UI but must match the scripting and workflow depth. Rust fixability: cost (yes, via open license), UI (yes), implementation effort (partly), depth (only with sustained engineering).

### 5.4 Peer comparison

| Dimension | Odoo | ERPNext | Dolibarr | NetSuite |
|---|---|---|---|---|
| License | LGPLv3 Community + proprietary Enterprise | GPL-3.0, single edition | GPL-3.0, single edition | Proprietary SaaS |
| Review score (Software Advice) | 4.2 (1,332) | 4.5 (140) | 4.5 (107) | 4.2 (2,076) |
| Top praise | Integrated suite, modular, value | Fully open, flexible DocTypes | Free, simple, lightweight | Deep customization, scale |
| Top pain | Upgrades, pricing surprises, custom debt | Ops/upgrades, performance, partners | Dated UI, feature gaps | Cost, complexity, specialists |
| Upgrade story | Hardest for custom code | Bench migrate; v15 to v16 friction | Frequent releases, mixed reports | Vendor-managed |
| Localization | 103 country entries, uneven | Strong India, uneven elsewhere | Patchy | Via SuiteApps/partners |

## 6. Where a greenfield Rust + modern frontend helps

| Pain | Tech alone | Needs governance/business model | Not fixable by us |
|---|---|---|---|
| Upgrades | Typed migrations, data-driven customization, extension ABI | Deprecation policy, upgrade CI | Regulatory-driven changes |
| Pricing/licensing | - | Single open edition, pricing commitments | - |
| Customization debt | Low-code tier, customization report | Partner norms | Genuinely bespoke logic |
| Support/partners | Diagnostics | Partner program | Human service quality |
| Localization | Decimal types, typed e-invoice schemas | Country maintainers, certification | The law itself |
| Module quality | Sandboxing, compile-time checks | Marketplace policy | Author abandonment |
| Performance | Async Rust, typed SQL, lower memory | Query budgets | DB-bound workloads |
| UX | Design system, one frontend architecture | Design investment | Domain complexity |
| Ops | Single binary, built-in jobs/migrations | - | - |
| Docs/API | OpenAPI from types, doc tests | Doc culture | - |
| Testing | Fixtures, property tests | Test requirements | - |
| Security | Memory safety, typed queries | Equal-time disclosure | AuthZ design |

## Confidence & gaps

- **High confidence** [V]: Odoo versions/branch state, licenses, pricing page (with the promotional-versus-list correction above), docs on sizing, upgrade SLA, API deprecation timeline, OpenUpgrade issue counts (issue mix softened), review-site scores, ERPNext/Dolibarr repo stats and recent releases (Dolibarr year now confirmed as 2026), Vendr NetSuite figures as transcribed (the data itself is Vendr's estimate), Enterprise agreement v13 clauses (re-read in the raw docs source on 2026-10-01; the signed contract still governs).
- **Medium**: Forum thread themes (titles and tags only for Odoo forum results). Software Advice/GetApp anecdotes: most dated examples re-checked; a few could not be found (marked unverified inline).
- **Low**: Testing culture, security comparisons (NVD keyword counts are crude, though the counts themselves reproduce: Odoo 59, ERPNext 69, Frappe 163, Dolibarr 170), performance numbers (anecdotes only).
- **Not done**: G2, TrustRadius and Gartner Peer Insights were not readable (403); Capterra yielded no usable data. Reddit was not read directly (blocked for the fetch tool), so r/odoo, r/ERP, r/smallbusiness and r/selfhosted sentiment is represented only by search snippets; r/sysadmin and r/manufacturing threads were seen only as titles. There were no genuine first-person "why we left Odoo" case studies; search for them returned mostly consultant comparisons with commercial bias. Odoo Enterprise source is private, so I made no code-level claims about it.
- **Bias warnings**: Odoo's founder and a Dolibarr developer post in threads I cite; Frappe's founder has a public anti-Odoo post (Aug 2024), with Odoo's founder disputing the facts in comments, so those numbers are unverified [S] https://frappe.io/blog/erpnext/odoos-marketing-blitz-in-india (post by Rushabh Mehta, 30 August 2024; the disputing comments are from Fabien Pinckaers and a partner). Some Software Advice reviews are incentivized (marked "invited ... nominal incentive" on the page).

## Implications for obop

**MUST**
1. **One open edition, no feature withholding.** Ship accounting, bank reconciliation, payroll engine, e-invoicing and API all in the open core. Monetize hosting, support, certified localizations and marketplace revenue share. Evidence: pain 2, Odoo 20's Enterprise-only Nilvera move, HN complaints across 2019 to 2026.
2. **Make upgrade a product requirement, not an afterthought.** Forward-only transactional migrations with dry-run and report, runnable on-prem without sending data anywhere, tested in CI from N-1 and N-2 fixtures on every merge, with a stated goal of skip-level support. Publish a deprecation policy at least as generous as Odoo's multi-year RPC timeline (Odoo 20 to 22).
3. **Customization as data first, code second.** Custom fields, formulas, states, automations and view layouts stored as versioned, migratable metadata. Code extensions only through a semver'd host API, sandboxed as WASM or out-of-process (Rust has no stable ABI, so no in-process dynamic plugins).
4. **Public API from day one, ungated.** OpenAPI generated from types, API keys, webhooks, examples compiled in CI. Odoo's gating of the external API to the Custom plan and its RPC removals were explicit user complaints.
5. **Money-correct accounting core.** Decimal types only, immutable posted entries, invariants as property tests, chart/tax/fiscal-position/report data per country as separate versioned packs with golden-file tests.
6. **Operational simplicity.** Single binary plus Postgres, built-in job queue, websockets, migrate, backup and restore commands. No required reverse proxy or separate real-time worker.
7. **Equal-time security disclosure.** Public advisories for everyone simultaneously; no customer-tiered embargo (unlike Odoo's current policy).

**SHOULD**
8. **Marketplace governance from the first external extension.** Automated compatibility runs against each core release, "Verified on vX" badges, capability-manifest permissions, signed packages, auto-delisting of abandoned ones.
9. **Performance budgets as part of the model DSL.** Declared indexes, N+1 and query-count tests, asynchronous recomputation of derived aggregates, and a published benchmark (for example a large stock-move table with hundreds of concurrent users). Treat any speed-up claims as [G] until measured.
10. **One frontend architecture and a strict design system.** Avoid Odoo's multi-era Owl/legacy mix. Include offline-capable forms early (Odoo 20 now ships it) and accessibility.
11. **Guided onboarding.** Learning curve is the top con. Import wizards with validation, sample data, a configuration linter, and "start with one app" like Odoo's free single-app funnel.
12. **Predictable hosted pricing in writing.** Price caps on renewal, no per-country surprise pricing, no 12-month promotional cliffs, cancel-anytime data export. Software Advice and Trustpilot show this is where trust is lost.
13. **Customization report.** A built-in view of what differs from stock and its upgrade risk score (Odoo bills covered custom modules per 100 lines via `cloc`; the economics are real).
14. **Limited localization launch.** Prioritize a small set of countries done to the point of filing-ready, rather than claiming 100.

**COULD**
15. **Odoo and ERPNext importers.** Migrating users (pricing shock, upgrade fatigue, cancellation complaints on Trustpilot) are the acquisition channel. Importer from Odoo JSON-2 and ERPNext API.
16. **MCP and agent-ready API surface without credit metering.** Odoo 20 (MCP connection to the database, AI agents) and Dolibarr 24 (experimental MCP server and AI assistant) both shipped agent/MCP features; Odoo meters all AI features with purchased credits. An open, ungated agent API is a differentiator.
17. **Declarative model layer with Rust codegen** (DocType-style ergonomics that ERPNext developers praise, with compile-time types).
18. **Public partner program** with published rate cards and no database lock-in, addressing partner-dependence complaints.

## Verification log

Adversarial re-check on 2026-10-01 using different or more primary sources than the first pass: the raw Odoo docs source (19.0, master and 20.0 branches), the GitHub REST and search APIs (repo stats, releases, issue and PR counts, advisories, commit counts), the Hacker News Algolia item API (every cited HN item read), the Frappe and Discourse JSON APIs, the NVD API, and direct page fetches (Odoo pricing page HTML/CSS and configurator, security page, release notes, review sites, Vendr, Trustpilot).

**Confirmed as stated: about 70 individual claims** (counted as atomic facts; approximate). This includes: LGPLv3 Community and OEEL-1 Enterprise with a per-user subscription; Odoo 20 as current (default branch 20.0, updated 2026-10-01; docs table and release notes "September 2026"); 19.0 released September 2025 (Wikipedia: 18 September 2025); upgrade SLA exclusions, "cannot be upgraded until a custom-module build exists", and 3-year support; agreement v13 clauses (25% fee from 6 months after a new major, cloc per 100 lines, 300% fee, 7% renewal uplift, 30-day partner-change notice, 2-week advisory lead for self-hosters, database copy for self-hosted upgrades); `db` service removed in Odoo 20 and `common`/`object` scheduled for Odoo 22 (fall 2028) in the master and 20.0 docs; JSON-2 added in 19.0 and external API Custom-only; sizing rule (CPU x 2) + 1, 1 worker about 6 users, 1 GB versus 150 MB; security policy wording (stakeholders and Enterprise customers first, "e.g. 2-3 weeks", Hall of Fame as the only reward, 86 researchers over 14 years, zero GitHub security advisories); Software Advice scores for Odoo (4.2, 1,332, 53%/7%, sub-scores), ERPNext (4.5, 140), Dolibarr (4.5, 107, no 1-2 star) and NetSuite (4.2, 2,076, 47%/3%); Trustpilot 3.1 from 1,280 (46% vs 43%); OpenUpgrade 35 open issues and the 20.0 tracker (#6044, opened 2026-09-29); ERPNext GPL-3.0, 39.7k stars, 13.0k forks, about 1.7k open issues, v16.37.0 and v15.121.6 both published 2026-09-30; Dolibarr GPL-3.0 PHP, 7.7k stars, 3.5k forks, 649 issues, 371 PRs; Vendr NetSuite figures as transcribed; Odoo repo stars and forks; Owl 3.0.0 Alpha under LGPL; OCA 664 members, 1,600 contributors, 271 repos, "+20 000" modules; NVD keyword counts (59, 69, 163, 170); Frappe Cloud $5 and $40 entry prices; Odoo 20 release-note items (Nilvera Enterprise-only, AI credits, record rules, Material Symbols, offline mode, drag and drop, parent accounts, depreciation models, MCP); the pricing page's "latest stable version" statement for on-premise; HN items read directly (quotes and dates for 46439993 thread comments, 46485746, 21869046, 28985377, 46486079, 46486350, 46486373, 47916231, 46486460, 46485416, 46485439, 49510482, 49268986 and others).

**Corrections (was / now / source)**

1. Pricing. Was: "Standard EUR 19.90/user/month yearly (24.90 monthly), Custom EUR 35.90 yearly (44.90 monthly)" presented as the plan prices. Now: those are the first-12-month promotional prices for initial users; list is Standard 24.90 and Custom 44.90 yearly-billed (configurators show "First Year Initial Discount" of -5.00 and -9.00, i.e. 20%), with monthly-billed 24.90 and 44.90 promotional against 31.10 and 56.90 struck-through list. Also added: Odoo.sh/on-premise are listed only under Custom; One App Free may use Studio and multiple companies for its one app. Source: https://www.odoo.com/pricing-plan (HTML and CSS), https://www.odoo.com/pricing-configurator?plan=standard , https://www.odoo.com/pricing-configurator?plan=custom .
2. OpenUpgrade open PRs. Was: 48. Now: 46 (35 issues + 46 PRs = 81 matches the repo API's open_issues_count). Source: GitHub search API, https://github.com/OCA/OpenUpgrade .
3. OpenUpgrade issue mix. Was: "dominated by accounting/l10n breakage". Now: accounting/l10n is the largest cluster but only about 6 of 35 open issues; the rest are HR, payment, mail, inventory failures and about 7 migration-tracker or process issues; l10n_fr and l10n_es have one open issue each ("repeated" removed). Source: https://github.com/OCA/OpenUpgrade/issues (API listing).
4. Dolibarr release year. Was: "year inferred as 2026". Now: confirmed 2026 (24.0.0 published 2026-08-20, 24.0.1 2026-09-07; 23.0.4 2026-08-20, 22.0.5 2026-05-26, 18.0.10 2026-05-07). Source: https://api.github.com/repos/Dolibarr/dolibarr/releases .
5. Odoo country localizations. Was: "roughly 105 (my tally)". Now: 103 entries in the docs list (same in 19.0, master, 20.0). Source: https://raw.githubusercontent.com/odoo/documentation/master/content/applications/finance/fiscal_localizations.rst .
6. Vendr NetSuite data. Was: "broker estimates". Now: Vendr's anonymized transaction data plus estimate ranges; page is internally inconsistent (average $74,816 in header vs median $74,817 in body; 647 deals vs 1,512 purchases; range $7,160 to $299,115). Source: https://www.vendr.com/marketplace/netsuite .
7. Enterprise agreement caveat. Was: "came through a fetch-tool summary; verify legal text". Now: verified against the agreement text (version 13, dated 2026-09-24). Refinements: the 7% uplift applies to charges below current list price and excludes the non-covered-version fee; the 300% fee applies to breach of the Access-to-the-Software section (license-verification code must run unmodified); the 25% fee is charged once per year. Source: https://raw.githubusercontent.com/odoo/documentation/19.0/content/legal/terms/enterprise.rst (identical in 20.0).
8. Review incentives. Was: "vendor-invited with a nominal incentive". Now: some reviews are "invited by us ... offered a nominal incentive" (by the review site), others organic; the page does not say Odoo invited them. Source: https://www.softwareadvice.com/erp/odoo-profile/reviews/ .
9. SourceForge "4.2 aggregate". Now: 4.2 across 1,714 ratings (12 SourceForge reviews, 1,702 third-party ratings); the 12 native reviews average 4.7. Source: https://sourceforge.net/software/product/Odoo/ .
10. GetApp "inventory difficult beyond 20k items". Now: a GetApp summary of 118 reviews (July 2021 to October 2024), not an individual dated review; same for the Romania remark (was presented as a 2026 review). Source: https://www.getapp.com/operations-management-software/a/odoo/reviews/ .
11. "A 2025 review calls the database 'slow, laggy'". Not found; replaced by the slowdown comments actually seen (July 2026, April 2026, September 2026). Source: Software Advice review pages.
12. Trustpilot "Odoo's replies say heavy customization raises cost". Now: one reply (to the 2026-09-22 review) says replicating every existing process raises complexity, duration and cost; "Aug-Sep 2026" positives narrowed to September 2026. Source: https://www.trustpilot.com/review/odoo.com .
13. HN attributions. "Six-year users" is one commenter; testing "painful" is actually "a pain"; the "2026 HN thread" 46439993 was posted 2025-12-31 (cited comments 2026-01-04); the v12/v13 feature moves are users' recollections ("iirc"), not release-note facts; the NetSuite 2013-2015 "poor integration, pricing moving upmarket" claim is not in the cited item (9557648 is a 2015 comment about 3 to 6 month implementations and $50k to 100k+), so it is replaced and flagged. Source: https://hn.algolia.com/api/v1/items/<id> for each cited item.
14. Minor drift. Odoo commits 211,729 to about 211.7k (211,739 now); app store 93,609 to about 93.6k (93,612 now). Source: GitHub API, https://apps.odoo.com/apps/modules/browse .
15. Sizing wording. The docs call (CPU x 2) + 1 a "rule of thumb" and the memory figures "estimates" (20% heavy requests); the live-chat/websocket worker is started automatically and the proxy's job is to route /websocket/ to it. Source: https://raw.githubusercontent.com/odoo/documentation/19.0/content/administration/on_premise/deploy.rst .
16. API deprecation wording. The 19.0 docs say all three RPC services are scheduled for removal in Odoo 22; the master/20.0 docs say `db` is already removed in Odoo 20 (and Online 19.1) and `common`/`object` go in Odoo 22 (fall 2028) and Online 21.1 (winter 2027). The file now states both. Source: https://raw.githubusercontent.com/odoo/documentation/master/content/developer/reference/external_api.rst .

**Facts added** (missing from the first pass): support windows (20.0 to September 2029 planned, 19.0 to September 2028, 18.0 to September 2027; 17.0 standard support ended September 2026); Covered Versions = 3 most recent majors, so 17.0 and older become non-covered and the 25% fee could start about March 2027 [I]; Odoo 20 ships MCP connection and AI agents; the Dolibarr MCP server is experimental. Source: https://raw.githubusercontent.com/odoo/documentation/master/content/administration/standard_extended_support.rst , https://www.odoo.com/odoo-20-release-notes .

**Unresolved (unverified)**
- The r/Odoo thread (24-user firm quoted about 272 hours for v17): Reddit blocked to my tools; snippet-only.
- Software Advice/GetApp anecdotes not found when re-reading pages 1-3 and GetApp: "a database created under a partner's account and later deleted", "a custom project unfinished after a year". Marked unverified inline.
- "xpath inheritance is fragile" is not in the cited HN comments (removed from the claim, flagged inline).
- "An OCA auto_backup module is recommended for small deployments": the module exists (OCA/server-tools 18.0 and 19.0), but the recommendation is not in Odoo's deploy docs.
- Monthly-billed list prices (31.10 and 56.90) and the assumption that the 20% discount lapses after 12 months are read from the pricing page structure and footnote; the monthly configurator state was not loaded and the Browser pane could not open odoo.com for a visual check.
- Whether Odoo actually moved accounting reports, budgets, assets (v12) and payroll (v13) out of Community: HN recollections only, not checked against Odoo release notes.
- Odoo testing docs body (HOOT, query-count helpers): page body truncated in my fetch; only section headings confirmed.
- Not re-verified: NetSuite, ERPNext and Dolibarr praise and pain lists from review summaries, ERPNext "docs thin" remarks, Tally and Italy/Chile SII gaps, the pro-ERPNext blog's unsourced claims (its hourly-rate and module-breakage statements were confirmed as what the blog says, not as true), the agreement's signed form versus the public docs template.
- G2, TrustRadius, Gartner Peer Insights and Capterra remain unread.
