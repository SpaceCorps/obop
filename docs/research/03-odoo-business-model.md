# Odoo S.A. business model, editions, pricing and ecosystem

Research note 03 for obop ("one business, one platform"). Research date: 2026-10-01.

## Scope & method

Odoo S.A.'s commercial model was researched from live sources only, with a bias to primary ones. Those are the Odoo Enterprise Subscription Agreement (v13, dated 2026-09-24) and the Partnership Agreement (v11), both read as raw .rst in the `odoo/documentation` GitHub repo. Also used: the odoo.com editions, pricing, partner, about-us and Odoo 20 release-notes pages (HTML parsed directly where the fetch summariser lost table marks), the Odoo Apps vendor guidelines, and the `odoo/odoo` repo (branches, `release.py`, a blobless clone of branch 20.0 for module counts). Secondary sources are press (TechCrunch, Belga/Brussels Times, General Atlantic releases, The Next Web), Hacker News threads (sentiment only), OCA and Flectra sites and repos, and Wikipedia.

There was no general web-search tool, so discovery used news RSS feeds plus guessed primary URLs. Numbers that hinge on a decision were re-checked against raw sources. Tags used below:

- **[P]** primary source.
- **[S]** secondary source.
- **[C]** company or founder claim, not independently audited.
- **[I]** my inference.

## 1. Snapshot (as of 2026-10-01)

| Item | Finding | Source |
|---|---|---|
| Legal entity | Odoo S.A., Belgium; Belgian law and the Nivelles business court govern the Enterprise contract | [P] https://raw.githubusercontent.com/odoo/documentation/20.0/content/legal/terms/enterprise.rst |
| Model | "Open core": Community under LGPL-3; Enterprise under a proprietary licence (OEEL-1), layered on top of Community | [P] https://www.odoo.com/documentation/19.0/legal/licenses.html |
| Current major | Odoo 20.0, released September 2026; standard support planned to Sept 2029 | [P] https://raw.githubusercontent.com/odoo/documentation/20.0/content/administration/standard_extended_support.rst |
| Price (Online, per user per month, yearly billing) | Standard €19.90 first-year promo (struck-through €24.90); Custom €35.90 first-year promo (struck-through €44.90); Light User €7.90; One App Free €0 | [P] https://www.odoo.com/pricing-plan |
| Revenue | €620M in 2025 (reported); billings guided above €900M in 2026 and €1B in 2027 | [S][C] Belga via https://www.brusselstimes.com/belgium/2332306/odoo-announces-e10-billion-valuation-and-a-partial-price-increase |
| Valuation | CEO says about €10B (Sept 2026; he describes it as what potential investors would be willing to pay to buy the company, and repeated that he is not a seller, so it is not a transaction); €7B at the Jan 2026 General Atlantic secondary; €5B at the Nov 2024 secondary | [S] same Brussels Times piece; https://www.generalatlantic.com/media-article/general-atlantic-announces-close-of-further-investment-into-odoo/ ; https://techcrunch.com/2024/11/20/riding-high-on-open-source-erp-odoo-raises-527m-via-secondaries-lifting-its-valuation-to-5-26b/ |
| People | About 7,500 staff (Belga, Sept 2026); "8,000+" on the company's about page; plan to hire about 3,500 in 12 months | [S][C] Brussels Times; https://www.odoo.com/page/about-us |
| Users and customers | "28 million users" on the about page; about 80% on free plans and 20% paying (Belga); 170,000+ paying customers in about 180 countries (General Atlantic, Jan 2026) | [C] |
| Partners | "21,000+ partners" on the about page; the partner directory page's "All Countries" count was 4,454 on re-check (4,458 in the first pass; the directory count drifts) | [C] about-us; [P] https://www.odoo.com/partners |
| GitHub | `odoo/odoo`: 54,768 stars (54,767 in the first pass), 33,902 forks, created 2014-05-13, default branch 20.0, last push 2026-10-01 | [P] https://api.github.com/repos/odoo/odoo |

## 2. History and licence timeline

| Date | Event | Source |
|---|---|---|
| 2002 / 2005 | Fabien Pinckaers starts the company (2002 per investors' press releases) and ships TinyERP (2005 per Odoo's own history). Note the 2002 vs 2005 discrepancy: likely company founding vs first product. [I] | [S] https://www.generalatlantic.com/media-article/odoo-sa-announces-e150m-investment-from-general-atlantic-to-support-continued-global-growth/ ; [P] https://www.odoo.com/blog/odoo-news-5/post/the-odoo-story-56 |
| about 2008 | TinyERP becomes OpenERP ("tiny" hurt credibility, especially in the US). A fork of TinyERP 4.2 becomes Tryton in Nov 2008 (first release 2008-11-17). Odoo's story says the rename came "three years" after 2005 and Wikipedia says 2008; the OCA timeline places "TinyERP becomes Open ERP" in 2009, so the exact year is uncertain. | [P] the-odoo-story-56 above; [S] https://en.wikipedia.org/wiki/Tryton ; https://odoo-community.org/about |
| 2009 | OpenERP moves from GPL to AGPL | [S] https://odoo-community.org/about |
| 2010 | Over 100 staff but mostly services revenue; strategic pivot from services to software publisher plus partner network and maintenance; first round about €3M (Sofinnova, Xavier Niel; dated 17 Feb 2010 by the TechCrunch headline "OpenERP gets €3 million for US expansion" that Wikipedia cites, https://techcrunch.com/2010/02/17/openerp-gets-e3-million-for-us-expansion/, not opened by me) | [P] the-odoo-story-56 |
| 2013 | Odoo Community Association (OCA) founded | [S] OCA about page |
| 2014-05-14 | OpenERP and the company are renamed Odoo (business-app suite, not just ERP); Odoo 8 ships June 2014 under AGPL-3 with CMS, eCommerce, POS, BI; $10M Series B | [P] https://www.odoo.com/blog/odoo-news-5/odoo-the-new-openerp-156 ; the-odoo-story-56 |
| 2015 | **Odoo 9.0: open-core pivot.** Community relicensed to LGPL-3; proprietary Enterprise edition introduced. Fabien says the company was losing about €500k per month and had weeks of cash left. By end of 2016 it was cash-positive by about €500k per month. (The blog post is dated 14 March 2019, so these are retrospective founder figures [C]. The OCA timeline loosely says the Enterprise Edition arrived in 2014; Odoo's own post and the licence doc support 2015 / Odoo 9.) | [P] https://www.odoo.com/blog/odoo-news-5/post/odoo-community-enterprise-532 ; licence facts: https://www.odoo.com/documentation/19.0/legal/licenses.html (Odoo 8 is AGPL-3, 9 onward LGPL-3 plus Enterprise licence) |
| Dec 2019 | $90M secondary led by Summit Partners (no primary capital); "largely profitable", 83% gross margin, 4.5M users | [S] https://techcrunch.com/2019/12/17/odoo-grabs-90m-to-sell-more-smes-on-its-business-app-suite/ |
| Oct 2022 | Pricing collapses from per-app to a single per-user plan (Enterprise Agreement v10 drops the per-app notion). Odoo's own rationale: the old price was per user plus per installed app, so single-user firms in Europe averaged about €96 a month while 100-user firms paid about €20 per user. | [P] ent.rst changelog; [P] https://www.odoo.com/blog/odoo-news-5/odoos-new-pricing-970 (18 Oct 2022); [S] Wikipedia |
| Jun 2023 | General Atlantic strategic investment of about €150M | [S] General Atlantic release above |
| Nov 2024 | €500M secondary (CapitalG, Sequoia lead), €5B valuation, previously €3.2B | [S] TechCrunch above |
| Jan 2026 | General Atlantic buys more from Wallonie Entreprendre (which keeps about 3%); €7B valuation | [S] General Atlantic / The Next Web https://thenextweb.com/news/odoo-tops-7-billion-valuation |
| Sept 2026 | Odoo 20.0 released; Odoo Experience in Brussels (24-26 Sept); first price increase in four years | [P] release notes https://www.odoo.com/odoo-20-release-notes ; [S] Belga |

Why LGPL in 9.0 matters [I]. AGPL-3 (Odoo 8) would have pushed proprietary add-ons into copyleft. LGPL-3 lets a proprietary Enterprise layer, and third-party proprietary apps under OPL-1, sit on an open core. That is the licence mechanism that made the 2015 open-core model possible. It is my reading; I did not find Odoo stating it.

## 3. Community vs Enterprise: exactly what is withheld

### 3.1 Licences

| Layer | Licence | Notes |
|---|---|---|
| Community (Odoo 9 to 20) | LGPL-3 | `release.py` on master declares `LGPL-3`; `LICENSE` is LGPLv3 plus GPLv3 text. The GitHub API shows "NOASSERTION", probably because of the custom header text [I]. Note that the licences doc on the 20.0 docs branch still reads "Odoo 19 Community Edition" and "Odoo 9 - Odoo 18", so LGPL-3 for Odoo 20 rests on `LICENSE` and `release.py`, not on that page. [P] https://raw.githubusercontent.com/odoo/odoo/master/LICENSE ; https://raw.githubusercontent.com/odoo/documentation/20.0/content/legal/licenses.rst |
| Enterprise | Odoo Enterprise Edition License v1.0 (OEEL-1) | Use, modify and run modified code only with a valid subscription for the right number of users. Partners under a Partnership Agreement get the same rights for test and dev only. Derived modules may carry any compatible licence (LGPL, MIT, or proprietary like OEEL). [P] licences doc above |
| Odoo S.A. paid apps and themes | OPL-1 (Odoo Proprietary Licence v1.0) | Needs a purchased licence; no redistribution; library-style dependents may use any compatible licence. [P] same doc |
| Odoo 8 | AGPL-3 | [P] same doc (it covers Odoo 8 only). OpenERP moved from GPL to AGPL in 2009 [S] https://odoo-community.org/about , so earlier versions are AGPL back to 2009 and GPL before. |

The Enterprise source lives in a private repository (`odoo/enterprise`). Subscribers and partners get access; the public cannot read it [S, consistent with the partner benefits table]. The founder confirms this on HN: https://hn.algolia.com/api/v1/search?query=odoo&tags=comment,author_pinky07 (objectID 46563542).

Contribution terms [P]: contributions to `odoo/odoo` require a signed Odoo Contributor License Agreement (https://www.odoo.com/cla , which lands on https://github.com/odoo/odoo/blob/20.0/doc/cla/sign-cla.md). The individual CLA (icla-1.0, based on the Apache ICLA) grants Odoo S.A. a perpetual, irrevocable, sublicensable copyright licence, and the Enterprise Agreement (section 7.1 and footnote) warrants that Odoo S.A. owns the copyright "or an equivalent" on 100% of the Software's code via that CLA. [I] This is the legal mechanism that lets community-contributed code sit under a proprietary layer and would also allow a relicence.

### 3.2 App-level split (parsed from the live comparison table, Odoo 20 era)

Source: https://www.odoo.com/page/editions. I parsed the HTML table and extracted the check and cross marks, because the text summariser dropped them [P].

| Area | In Community | Enterprise-only |
|---|---|---|
| Finance | Invoicing, Payments, Expenses (basic) | Accounting (full ledger, bank statements, reconciliation, budgets, consolidation), AI in invoicing, Payroll (with expense OCR, reimbursement in payslip), Documents, Spreadsheet, Sign, ESG |
| Sales | CRM, Sales, Point of Sale | Subscriptions, Rental, Amazon connector |
| Website | Website Builder, eCommerce, Blog, Forum, Live Chat, eLearning | nothing (all six are in Community) |
| Supply chain | Inventory, Manufacturing (MRP), Purchase, Maintenance | Barcode, Shop Floor/control panel/scheduling, PLM, Quality |
| HR | Employees, Recruitment, Time Off, Fleet | Referrals, Appraisals |
| Marketing | Email Marketing, SMS, Events, Surveys | Social Marketing, Marketing Automation |
| Services | Project, Timesheet (basic) | Timesheet grid/timer/validation, Field Service, Helpdesk, Planning, Appointments |
| Productivity | Discuss, To-Do, Calendar | Approvals, VoIP, IoT, Knowledge |
| Customisation | none | Studio (no-code builder) |
| General | Desktop web UI | Mobile apps (Android and iOS), unlimited functional support, version upgrades, hosting |

Reading of this table [I]:

- **Community has broad horizontal coverage.** Sales, inventory, MRP, purchase, website, eCommerce, POS and project are all there.
- **Enterprise withholds the "professional finance" and "high-value niche" layer.** Accounting proper, payroll, helpdesk, field service, planning, PLM, quality, knowledge, sign, documents and studio are all Enterprise.
- **Community is a real ERP for operations but not for a bookkeeper.** Odoo's README still advertises "Billing & Accounting" [P] https://raw.githubusercontent.com/odoo/odoo/20.0/README.md.

Code-base scale of the open part [P, my count]. A blobless clone of branch 20.0 shows 642 directories under `addons/` and 16 under `odoo/addons/`. Of the 642, 229 are `l10n_*` localisation modules (about 36%). That is the real moat: country packages (charts of accounts, taxes, e-invoicing, payroll in Enterprise) are a large share of the open code. Odoo's own about page says "50 main applications" [C] https://www.odoo.com/page/about-us.

### 3.3 Service-level withholding

- **Hosting, support, upgrades** are included only with a subscription.
- **External API (JSON-2, XML-RPC and JSON-RPC)** is documented as available on the Custom plan only, not on One App Free or Standard. [P] https://raw.githubusercontent.com/odoo/documentation/20.0/content/developer/reference/external_api.rst. The legacy XML-RPC and JSON-RPC `common`/`object` services are deprecated and scheduled for removal in Odoo 22 (fall 2028) and Online 21.1 (winter 2027); the `db` service was dropped in Odoo 20 (Online 19.1) [P] https://raw.githubusercontent.com/odoo/documentation/20.0/content/developer/reference/external_rpc_api.rst.
- **Studio and multi-company** are Custom-plan features on Online (the Custom card now reads "Odoo Studio / Agentic AI, Multi-Company, External API"). The pricing page is internally inconsistent here: one FAQ answer says installing Studio or adding a company switches you to Custom, another says the free plan allows multiple companies and Studio can be the chosen free app. [P] pricing page.
- **On-premise and Odoo.sh hosting** are only on the Custom plan. Standard is Odoo Online only, and Online cannot run custom Python modules (Studio customisations only). [P] pricing page; the Apps FAQ says third-party apps cannot run on Online except data-only modules. https://apps.odoo.com/apps/faq
- **AI**: in Odoo 20 every AI feature needs credits bought via IAP; MCP connectivity was added (docs page `productivity/ai/mcp_server`). [P] https://www.odoo.com/odoo-20-release-notes . The release notes do not say whether any free allowance is included. The docs tree also has a stub "AI API keys" page and the release notes mention selecting an AI provider, so bring-your-own-provider-key may exist and may avoid credits (unverified).

### 3.4 How the boundary drifts

Fabien's 2019 post is the clearest statement of policy [P] https://www.odoo.com/blog/odoo-news-5/post/odoo-community-enterprise-532:

- The target ratio is "80% open source, 20% Enterprise" of what the company builds.
- High-value niche apps go to Enterprise (example: Field Service, about 5% of companies). Mass-market apps that already have free competitors go to Community (example: eLearning).
- The 2015 pivot was rushed, with most new features going to Enterprise.
- For Odoo 13 the post (March 2019) announced, as plans, moving a batch to Community (CRM lead scoring, coupons, POS loyalty, mailing and website themes, digital products) and moving others out: stop maintaining the open Payroll app, stop maintaining the old Accounting report engine, and move the reconciliation widget code to the Enterprise repo. Fabien invited OCA to take over Payroll. My check of the `odoo/odoo` branch trees [P, via GitHub API]: `hr_payroll` and `hr_payroll_account` exist in branch 12.0 (213 files under `addons/hr_payroll`) and are absent from 13.0, so the payroll removal did happen; `sale_coupon` and `website_sale_coupon` first appear in 13.0, so coupons did move to Community. Lead scoring, POS loyalty and the old report engine were not checked (unverified). The announced move of the reconciliation widget is not visible in the 13.0 Community tree, which still contains `account/static/src/js/reconciliation/` and `account/models/reconciliation_widget.py` (unverified what was moved).
- He prefers splitting per app, not per feature, so Community does not end up with half-locked apps.

A current example of drift is in the Odoo 20 release notes, which state that the Nilvera e-invoicing integration (a Turkish localisation) is now available only with Enterprise [P] https://www.odoo.com/odoo-20-release-notes. HN users in Dec 2019 reported accounting reports, budgets and assets leaving Community around v12 [S] https://hn.algolia.com/api/v1/items/21865699.

## 4. Pricing and packaging

### 4.1 Online plans (odoo.com/pricing-plan, EUR, per user per month)

| Plan | Yearly billing | Monthly billing | Includes |
|---|---|---|---|
| One App Free | €0 | €0 | One app (its dependencies free), unlimited users, Odoo Online. Per the upgrade FAQ, Studio or multi-company pushes you to Custom (another FAQ answer on the same page contradicts this, see 3.3); a second app pushes you to Standard. |
| Standard | €19.90 (struck-through €24.90) | €24.90 (struck-through €31.10) | All apps, Odoo Online only |
| Custom | €35.90 (struck-through €44.90) | €44.90 (struck-through €56.90) | All apps; Online, Odoo.sh or on-premise; Studio (the card now says "Odoo Studio / Agentic AI"), multi-company, external API |
| Light User | €7.90 | | Add-on quote via an advisor: own time off/expenses/attendance/planning/field tasks, barcode operator, read-only Knowledge, payslips, POS and shop-floor use |

Source: https://www.odoo.com/pricing-plan [P]. The footnote says the discount applies for 12 months to the initial users ordered. My reading [I]: the headline number is the 20% first-year price and the struck-through number is the base list price (ratio about 0.8). An internal drafting comment in the Enterprise Agreement's `.rst` source (a `..` comment, not rendered in the published terms, so not contractual) says the "current per-user price" is meant to exclude first-year and multi-year discounts and that monthly prices are annualised by multiplying by 12; the published text only says "current per-user price".

Other points on this page [P]:

- There is **no per-app pricing** any more.
- Email support runs 24/5 in English, French, Spanish and Dutch.
- Online storage is unmetered up to 100 GB.
- Not included: Odoo.sh hosting, implementation services, maintenance of custom code, and IAP credits (SMS, lead generation, AI bill scanning).
- Portal users (customers and suppliers) are free.

**Price change, Sept 2026** [S, Belga]. This was the first increase in four years, roughly 20% on Custom (reported as €29 to €35 first-year price), with Standard unchanged at €19.90. Odoo cites cumulative inflation of 26% and the need to offset salary and hosting costs. The Light User tier was presented as new. Belga also says the Light User tier requires at least one full user. The 20% figure matches the live Custom prices. The old price can now be derived: an archived copy of the page (Wayback, 2024-07-21, USD-localised, https://web.archive.org/web/20240721163907/https://www.odoo.com/pricing-plan) shows Standard $24.90 and Custom $37.40 on yearly billing; applying the same USD/EUR ratio as Standard ($24.90 vs €19.90) gives an old Custom price of about €29.90, so €35.90 is +20% and Standard is unchanged [I, derived from a P snapshot that is 2 years old, not from the page just before the change].

**Odoo.sh**. It is a git-based PaaS with unlimited dev branches, CI, staging environments (0 to 20), Shared or Dedicated hosting, 1 to 8 workers on Shared, and 1 to 512 GB. The price grid is rendered by JavaScript and the per-worker, per-GB and per-staging rates could not be retrieved. The hosting price excludes the Enterprise licence. [P] https://www.odoo.sh/pricing.

### 4.2 Contract mechanics (Enterprise Subscription Agreement v13, 2026-09-24)

Source: https://raw.githubusercontent.com/odoo/documentation/20.0/content/legal/terms/enterprise.rst (figures re-verified against the raw file) [P].

| Mechanism | Term |
|---|---|
| Renewal increases | Up to 7% per year when charges are below current list (v12 fixed "per year, not per term") |
| Usage reporting | A validity-check code must run unmodified and reports users, light users, installed apps and lines of custom code. Self-hosted customers must allow audits on request. |
| True-up | Excess usage is charged at list price in force when it occurred, for the rest of the term |
| Breach of the licence section | Extra fee of 300% of list price for the actual users |
| Non-covered versions | Covered versions are the 3 most recent majors (one major per year). From 6 months after a new major, a database on an older version owes an extra 25% of annualised price per year. |
| Custom code | "Covered Extra Modules" (any module on Odoo Online, or any paid-for) are billed monthly per 100 lines of code, counted with `cloc`. Studio customisations count as extra modules. |
| Non-solicitation | Mutual clause: for the term plus 12 months (or 12 months after the person leaves, whichever is later); €30,000 if a breach leads the employee or contractor to leave (a fixed amount per such breach, not "per head" wording) |
| Liability cap | 50% of amounts paid in the prior 12 months |
| Termination | 30 days' written notice before term end; licence revoked immediately on end; stop using Enterprise and the cloud platform |
| Working with a partner | Customer chooses Odoo SA or a partner as main contact; partner subcontracted for extra-module work |

[I] The agreement's changelog shows the commercial evolution: v6 added per-app pricing, v7 removed price changes at renewal after notice, v9 added "working with an Odoo partner" and extra-module maintenance, v10 (fall 2022) removed per-app, v11 added the 25% old-version fee, v13 (2026-09-24, one week before this research) adds the Light User definition and makes maintenance of covered extra modules mandatory on SaaS.

## 5. Release cadence, support policy and upgrade economics

Source: https://raw.githubusercontent.com/odoo/documentation/20.0/content/administration/standard_extended_support.rst and https://raw.githubusercontent.com/odoo/documentation/20.0/content/administration/upgrade.rst [P].

| Version | Release | End of standard support | Online | Odoo.sh / on-prem |
|---|---|---|---|---|
| 20.0 | Sept 2026 | Sept 2029 (planned) | Yes | Yes |
| 19.0 | Sept 2025 | Sept 2028 (planned) | Yes | Yes |
| 18.0 | Oct 2024 | Sept 2027 (planned) | Yes | Yes |
| 17.0 | Nov 2023 | Sept 2026 | No | Extended (extra fee) |
| 16.0 | Oct 2022 | Sept 2025 | No | Extended |
| 15.0 | Oct 2021 | Oct 2024 | No | Extended |
| SaaS 19.1 to 19.4 | Jan, Mar, May, Jul 2026 | not stated | 19.3 and 19.4 only | N/A |

- **Cadence.** One major per year (September or October) plus SaaS minors every two to three months, Online only. The `odoo/odoo` branch list includes `saas-19.1` to `saas-19.4` and `20.0`, and master is `20.1a1` with Python 3.12-3.14 and PostgreSQL 16+ [P] https://raw.githubusercontent.com/odoo/odoo/master/odoo/release.py.
- **Support window.** Standard support is three years from release, which in practice equals "the three most recent majors" (17.0's window ended when 20.0 shipped, slightly under three years). Extended support costs extra and is Odoo.sh for a limited set of versions, on-premise for any. The fee is not published.
- **Upgrade rights.** With Enterprise, upgrades to the latest version are free. They cover standard apps, Studio customisations, and custom code under a maintenance subscription. They do not cover data cleanup, unmaintained third-party or in-house modules, or training.
- **Forced upgrades.** Online databases must move major versions about every two years; SaaS minors a few weeks after the next release, automatically if ignored. Odoo.sh gets a two-year grace after the support window. On-premise is never forced.
- **Mechanics.** Upgrade runs on Odoo's platform (`upgrade.odoo.com`). Self-hosted customers upload a database copy; the founder says an `obfuscate` command exists since v16 but is imperfect [S] https://hn.algolia.com/api/v1/search?query=odoo&tags=comment,author_pinky07. A database with custom modules cannot upgrade until the maintainer ports the modules.
- **Switching editions.** Community to Enterprise, and changing hosting type, are explicitly not covered by an upgrade.
- **Community alternative.** OCA's OpenUpgrade (AGPL-3, 991 stars; default branch 19.0, with a 20.0 branch opened on 2026-09-28 that holds only an initial commit so far) provides community migration scripts [P] https://github.com/OCA/OpenUpgrade.

[I] The economics: upgrades are a lock-in lever disguised as a service. They are free but only for subscribers, bundled with support, and backed by the 25% old-version penalty and the per-LOC custom-code fee. The yearly breaking-change cadence (for example Odoo 20 removes record rules in favour of domains, swaps icon fonts, and renames accounting concepts [P] release notes) sustains both the partner upgrade business and the pull toward Odoo's platform.

## 6. Ecosystem

### 6.1 Partner channel

Source: https://www.odoo.com/become-a-partner (table parsed from HTML) and https://raw.githubusercontent.com/odoo/documentation/20.0/content/legal/terms/partnership.rst [P].

| | Learning | Ready | Silver | Gold |
|---|---|---|---|---|
| New Enterprise users per year | none | 10 | 75 | 300 |
| Certified active staff | none | 1 | 3 | 6 |
| Minimum 3-year retention | none | none | 70% | 80% |
| Annual partnership fee | €590 | €2,950 | €2,950 | €2,950 |
| Commission on Enterprise subscriptions | 10% | 10% | 15% | 20% |
| Commission on Odoo.sh | 10% | 50% | 50% | 50% |
| Commission on maintenance of extra modules | 82% | 82% | 82% | 82% |

- Odoo takes no share of partner service fees (implementation, customisation, training, hosting); those are the partner's main revenue. The partner page says partners "are free to set their services fees" and the Partnership Agreement has no revenue-share clause on services [P].
- The Odoo.sh commission is capped at €150 a month (€1,440 a year) per subscription (agreement footnote; the partner page shows the same cap) [P].
- The partner is non-exclusive. It must not offer services on Enterprise to customers who lack a subscription and may not use the word "Odoo" in company, product or domain names. Commission continues only while the partner keeps the customer contract. The term is one year with 30-day notice, and the mutual non-solicit clause carries a €30,000 amount if a breach leads an employee to leave.
- Partners get Enterprise source access, a sandbox, Odoo.sh, a dashboard, training and an internal licence coupon capped at €200 a year (Learning) or €2,000 (higher tiers).
- Scale is contested. The about page says 21,000+ partners [C]; the partner directory showed 4,454 entries on re-check (4,458 in the first pass) [P]. In 2019 the founder claimed about 2,600 partners employing 31,000 people, with about half earning over half their revenue from Odoo [C] https://hn.algolia.com/api/v1/items/21865699. In Jan 2026 he claimed a partner network of "200k FTE" [C]. These numbers are not reconcilable without definitions.
- Odoo S.A. also sells directly. The partnership agreement has no clause stopping Odoo's own sales, and a customer can drop a partner as main contact [P]. The about page names a head of "Direct and Indirect Channel" with about 600+ new implementations a year [C].

### 6.2 Odoo Apps store

Source: https://apps.odoo.com/apps/faq and https://apps.odoo.com/apps/vendor-guidelines [P].

- Odoo keeps **30%** of sales (vendors get 70%); In-App Purchase carries a 25% commission.
- Monthly payout once a vendor's unredeemed balance reaches €400, by SWIFT; minimum app price €9; EUR or USD only.
- Price parity is enforced: the app's price must be equal to or lower than on any other platform.
- Licence must be declared in the manifest. The vendor guidelines allow any licence that respects the licences of the app's dependencies, and recommend LGPL-3 for open apps and OPL-1 for proprietary ones. The manifest reference lists these values: GPL-2, GPL-2 or later, GPL-3, GPL-3 or later, AGPL-3, LGPL-3, other OSI approved, OEEL-1, OPL-1, other proprietary [P] https://raw.githubusercontent.com/odoo/documentation/20.0/content/developer/reference/backend/module.rst.
- Rules: apps may not clone Enterprise modules, bypass the usage-validation mechanism, use activation keys, or ship obfuscated code. Refunds have a two-week window through the store's support channel.
- Third-party apps cannot run on Odoo Online (the FAQ allows data-only modules with no Python files), and run on Odoo.sh only from private repositories. This restricts the store's reach into the SaaS base.
- Catalogue size: the browse page reported 93,613 apps on re-check (93,609 in the first pass). That count is the store's listing total across series, so it should be read as "listings", not unique products [P] https://apps.odoo.com/apps/modules/browse. Press cite "nearly 40,000 apps" and "50,000+" in different years [S].

### 6.3 OCA (Odoo Community Association)

- Non-profit based in Switzerland, founded 2013. Mission: collaboration between Odoo developers and consultants, and keeping Odoo viable as open-source ERP whatever Odoo S.A. does. [P] https://odoo-community.org/about
- Membership costs €60 (the page does not state the period); Delegates vote in annual elections that approve strategy and budget and elect the board. [P] https://odoo-community.org/page/membership
- The opt-in directory lists 615 people (366 contributors, 317 members, 77 delegates, 8 board members). The top countries are Spain, France, Belgium and Switzerland. [P] https://apps.odoo-community.org/community
- GitHub org `OCA` has 271 repositories and 5.1k followers. Repos such as `server-tools` and `web` carry AGPL-3.0 at repo level, with each module free to declare its own licence; their default branch is 19.0, and 20.0 branches already exist (`server-tools`, `web` and `OpenUpgrade` each got a 20.0 initial commit on 2026-09-28; OCB's 20.0 branch tracks Odoo, last merge 2026-09-24) [P] https://github.com/OCA/server-tools.
- Key assets: OpenUpgrade (migration scripts), OCB (Odoo Community Backports, a fork of `odoo/odoo` that keeps fixing versions Odoo no longer maintains, 376 stars), and hundreds of extension modules (localisations, accounting reports, warehouse, etc.). [P] https://github.com/OCA
- [I] OCA's existence is itself evidence of the pressure points: accounting reports, payroll and upgrade scripts are exactly what OCA backfills.

## 7. Company numbers

| Metric | Value | Source and quality |
|---|---|---|
| Revenue 2023 | €282M, up 33% | [S] Wikipedia citing De Tijd (June 2025 article; year attribution unclear, not fetched) |
| Revenue 2025 | €620M | [S][C] Belga, Sept 2026, CEO remarks |
| 2025 accounting result | loss of €36M, said to be already recouped in 2026; operating income above €50M over Jan-Aug 2026 | [S][C] same. Single source, odd for a "profitable" firm; treat with care |
| 2026 outlook | turnover up 48% year on year; billings above €900M in 2026 and above €1B in fiscal 2027 | [S][C] same |
| Revenue mix | 12% US, 9% Belgium, 9% France, rest across 180+ countries via about 20 subsidiaries plus partners | [S][C] same |
| Growth claims | about 40% a year (Nov 2024); about 50% average over 20 years (Sept 2026) | [C] |
| Users | 28M (about 80% free, 20% paying). Earlier claims by the company do not line up: "over 10 million" (General Atlantic, Jun 2023), "over five million" (TechCrunch, Nov 2024), 16M including free (founder on HN, Jan 2026). 16M to 28M in about 9 months implies a change of definition or a very large jump; comparability is unverified. | [C] |
| Paying customers | 50,000+ (Jun 2023); 170,000+ (Jan 2026); founder says 200k+ clients and 16M users on HN (Jan 2026) | [C][S] |
| Headcount | 7,500 to 8,000+ | [S][C] |
| Funding | about €3M (2010), $10M (2014), $90M secondary (2019), about €150M (2023), €500M secondary (2024), further secondary (Jan 2026). A 2021 "$215M" and a 2022 "€3.2B valuation" appear in headlines only and were not read | [S] |
| Investors | Summit Partners (largest institutional per 2024 report), General Atlantic, CapitalG, Sequoia, BlackRock, HarbourVest, Mubadala, Alkeon, AVP; Wallonie Entreprendre reduced to about 3% | [S] |
| Exit posture | Founder says he is in no rush to list and does not intend to sell | [S] |

Caveat on the Nov 2024 TechCrunch line that about 20% of the business comes from paid Enterprise. It conflicts with the pricing structure (Enterprise is the product that is sold). The HN founder comment says 80/20 is the split of what Odoo *builds*, not of revenue [C]. So the TechCrunch line is probably a garbled version of the feature ratio [I]. A second candidate is the user split: Belga (Sept 2026) reports about 80% of users on free plans and 20% paying, which TechCrunch may have garbled instead [I].

Operating model [I]: SMB-focused, one unified data model, self-serve plus partner-led sales. The cost base is engineering (hundreds of developers), hosting and a very large physical community programme (Odoo Experience 2026, held 24-26 Sept at Brussels Expo per https://www.odoo.com/experience: budget reported at €4.5M against expected income of €3.5M, so a net cost of about €1M, and about 45,000 attendees expected) [S].

## 8. Community complaints about open core

Sources are Hacker News threads (secondary, sentiment only): https://hn.algolia.com/api/v1/items/46439993 (Dec 2025 to Jan 2026) and https://hn.algolia.com/api/v1/items/21865699 (Dec 2019).

- **Boundary drift.** Features leave Community between versions (accounting reports, budgets and assets around v12; payroll around v13; "more in v14" rumoured). Commenters say this makes Community risky as a long-term base. The Odoo 13 blog announced some of this (payroll, accounting engine), and the branch trees confirm the payroll removal (see 3.4).
- **Mislabelled "open source".** Several commenters call it open-core or a misnomer because the strongest features are Enterprise-only, and note total cost can approach conventional ERP once support and implementation are added.
- **Support value.** Some say Enterprise support is slow or weak and that Community plus a freelancer is better value. Official partners are described as uneven, with pressure to buy and to host events at their own cost (2019).
- **Upgrade friction and data exposure.** Self-hosted Enterprise customers had to send a full database dump to upgrade. One partner said he nearly lost customers because he could not run upgrade scripts on-premise. The founder replied with the `obfuscate` command, OpenUpgrade and the argument that upgrades need per-environment testing.
- **API and docs.** Commenters called the XML-RPC and JSON-RPC docs poor and the planned removal abrupt. Verified: the new JSON-2 API exists but is only on Custom plan [P].
- **VC-capture worry.** A 2019 commenter predicts VC-backed open source will eventually relicense. Fabien replies that VC never drove decisions, he kept majority ownership, and the model is 80/20. No commenter cited an actual relicense after 2015.
- **Praise.** ORM-level permissions, runtime-defined models, kanban, a unified data model that traces campaigns to revenue, fast setup, and OCA quality.

The ERPNext founder used the 2019 thread to promote ERPNext as having no Enterprise tier, so features cannot leave [S]. ERPNext is GPL-3.0 with about 39.7k GitHub stars [P] https://github.com/frappe/erpnext.

## 9. Forks and derivatives

| Project | Relationship | Status | Lesson |
|---|---|---|---|
| **Tryton** | Fork of TinyERP 4.2 in Nov 2008; GPL-3.0-or-later | Alive: foundation-governed (Belgian private foundation since 2012), 6-month series, 1-year support, 5-year LTS every fifth series; `trytond` 8.0.10 on PyPI today | Survives as a niche, community-governed, federation-of-companies model, never reached Odoo scale [S] https://en.wikipedia.org/wiki/Tryton ; https://pypi.org/pypi/trytond/json |
| **Flectra** | Odoo Community fork, project created 2018-01-16; GitLab `flectra-hq/flectra` | Still active: branch 3.0 last commit 2026-06-30, 2.0 on 2025-12-19; 252 stars vs Odoo's 54.8k; mirror on GitHub shows 133 stars; GitLab last activity 2026-07-15 | **It copied the open-core playbook**: a free Community plus a paid "Professional" edition, a comparison table that mirrors Odoo's, per-user Cloud, and one-time or lifetime pricing options. Claims "more than 13,000 teams" (home page). [P][C] https://gitlab.com/api/v4/projects/flectra-hq%2Fflectra ; https://flectrahq.com/compare-editions ; https://flectrahq.com/pricing ; https://flectrahq.com/ |
| **OCB** (OCA) | Backports fork of `odoo/odoo` to keep old versions patched | Active (default branch 19.0 with 202,861 commits, a 20.0 branch tracking Odoo, 376 stars) | Patch-only forks are a stable "escape valve", not a competing product [P] https://github.com/OCA/OCB |
| **OpenUpgrade** (OCA) | Migration scripts for Odoo | Active; default branch 19.0, 20.0 branch just opened (2026-09-28) | Fills the paid-upgrade gap for Community [P] |
| **ERPNext / Frappe** | Independent, not an Odoo fork | 39.7k stars, GPL-3.0 | Main open-source rival that advertises "no enterprise edition" [P] |

[I] No fork has dented Odoo. Forks that compete on features inherit the same problem Odoo solved: funding a vendor. The only forks with durable governance (Tryton, OCA) do not try to be a product company. Flectra demonstrates that cloning Odoo's commercial model without Odoo's distribution (partners, events, brand) yields a small vendor. Also the "Odoo" name is trademark-controlled: partners may not use it in company or product names [P] partnership.rst, which pushes forks to rename.

## 10. Synthesis: copy, avoid, occupy

### What works in Odoo's model (copy)

1. **One data model, many modules, one price.** "All apps for one per-user fee" plus a free single app is simple to buy and grows seats, not SKUs. Moving from per-app to per-user in 2022 is the clearest simplification signal.
2. **A free wedge that is a real product.** Community reaches a huge install base (about 28M users, mostly free) and feeds Enterprise and the partner channel.
3. **A cheap, low-barrier partner programme.** €590 to enter, Odoo takes 0% of services, and partners earn commission on licences and extra-module maintenance. A Learning tier with no quota recruits long-tail integrators.
4. **A 70/30 app store with price parity and licence declarations.** Modest, well-specified rules.
5. **A predictable release train.** Yearly majors, three-year standard support, explicit support matrix published in the docs.
6. **Localisation as a moat.** About 36% of Community module directories are country localisations.
7. **Big tent events.** Odoo Experience 2026 is budgeted around €4.5M for about 45,000 attendees, used as a sales channel [S].

### What to avoid

1. **A moving open-core boundary.** Features leave the free edition between versions, and the current release still removes one (Nilvera). This creates distrust. Odoo's own stated preference for per-app splits acknowledges the problem.
2. **Monetising through friction.** Paid upgrades with database upload, a 25% penalty for old versions, 300% breach fee, 7% annual uplift, per-100-LOC fees for custom modules, €30,000 non-solicit clauses and a mandatory usage-reporting beacon are all lock-in by contract.
3. **Gating the integration surface.** External API only on the Custom plan, and no custom code on Online. For a platform pitched as "one platform" this shuts customers out of their own data on the cheap tier.
4. **Breaking changes every year.** Each major rearranges accounting, HR and UI primitives, feeding the partner upgrade business but burdening users.
5. **AI as a metered add-on.** Odoo 20 requires IAP credits for all AI.

### The gap obop could occupy [I]

The uncontested position is **"Odoo's breadth with a complete open edition and a safe extension and upgrade path."** Concretely:

- **A full open accounting and payroll core.** Odoo's open edition stops at invoicing. Even the Flectra fork lists Accounting (financial reports, bank reconciliation, analytic accounting) in its free edition [P] https://flectrahq.com/compare-editions, so this is a differentiator that a new entrant can match or beat without a fork.
- **Upgrades as a product feature, not a paid service.** A typed, versioned schema and extension API with automatic, local migrations would remove the Online forced-upgrade regime, the DB-upload step and the OpenUpgrade dependency.
- **Open API and extension on every tier.** A documented, stable public API and custom-code support in the hosted offering, including third-party modules in sandboxed form, where Odoo Online allows neither.
- **Monetise what cannot be forked**: managed hosting, compliance subscriptions (tax rules, e-invoicing network access, bank feeds), marketplace revenue, and certified-partner tiers. Odoo's founder himself says support and service margins alone did not fund a software vendor (2019 post), so obop needs a credible answer to funding, not just a licence.
- **Rust and Vite+ as a credible differentiator** (single binary, memory safety, predictable performance), but only as a secondary selling point. Buyers purchase outcomes, so the tech stack is not a wedge on its own [I].

## Confidence & gaps

**High confidence** [P, re-verified against raw sources]:

- Licence facts: LGPL-3 for Community, OEEL-1 for Enterprise, OPL-1 for Odoo apps.
- Editions table.
- Support matrix and policy.
- Enterprise Agreement v13 clauses: 7%, 25%, 300%, `cloc` per 100 lines, €30,000 non-solicit, three covered versions.
- Partnership Agreement v11 commissions.
- Partner fees and tier thresholds.
- Apps store 30%, 25% and €400 payout.
- Live Online price points.
- Odoo 20 release notes claims (IAP credits, Nilvera, MCP).
- Module counts for branch 20.0.

**Medium confidence** [S or C]:

- Revenue, valuation and headcount. Figures come from press or CEO statements and conflict in places (headcount 6,700 in Jan 2026, 7,500 in Sept 2026, 8,000+ on the website).
- Partner counts (4,454 vs 21,000+ vs earlier 2,600).
- Paying customers (170k vs 200k+).
- User counts (28M in Sept 2026 vs 16M stated by the same founder in Jan 2026, 10M+ in Jun 2023, 5M+ in Nov 2024; definitions unknown).
- The 2025 "loss of €36M" (one source).
- The interpretation of the price page's two numbers as first-year vs base price.

**Gaps and unverified**:

1. Odoo.sh per-worker, per-GB and per-staging rates (JavaScript-rendered).
2. Extended-support fee amount (unpublished).
3. Exact Odoo 9.0 and 20.0 release days.
4. (Resolved in the verification pass: `odoo/odoo` does require a CLA, see 3.1. `CONTRIBUTING.md` not mentioning it was misleading.)
5. Pre-2022 per-app price points (not verified; Odoo's Oct 2022 blog gives only averages, about €96 a month for a single-user firm and about €20 per user at 100 users).
6. 2024 revenue and the De Tijd article (paywalled, not fetched).
7. 2021 "$215M" and 2022 "€3.2B" headlines (not opened).
8. OCA's general licence policy (only repo-level AGPL-3.0 confirmed).
9. Flectra's exact licence terms for Community and Professional (GitLab shows GPLv3 text; site pages do not state).
10. Third-party "accounting for Community" free apps on the store (not researched).
11. Odoo's competitor set beyond ERPNext, Tryton and Flectra (Dolibarr, Axelor, metasfresh and others were not checked).
12. Search was done without a search engine, so unknown items may exist (for example a lawsuit or licence dispute).

## Implications for obop

**MUST**

1. **MUST fix the open/paid boundary in writing, early, and publish it.** Choose a rule such as "everything in the data model and every first-party app is open; monetisation is hosting, compliance feeds, marketplace, support and certification". Add a public commitment never to move an already-open feature to a paid tier (a signed governance document, ideally with a foundation or at least a trademark-and-licence trust). This is the single biggest grievance against Odoo (section 8) and the thing you can credibly do differently.
2. **MUST ship full accounting (GL, tax, bank reconciliation, multi-currency) in the open edition.** Odoo's open edition stops at invoicing; this is the biggest gap and the obvious entry point. Pair with localisation packs: about 36% of Odoo's Community module directories are country packs, so plan a localisation pipeline and treat it as the moat.
3. **MUST design upgrade and extension stability as the headline feature.** Stable, versioned module API; declarative, typed schema migrations that run locally with no data leaving the customer; a published compatibility and deprecation policy. Odoo makes this a paid, server-side service with yearly breakage (section 5).
4. **MUST make the API first-class and ungated on every tier** (OpenAPI/JSON plus machine-friendly tooling). Odoo gates the external API to its Custom plan, and its old RPC services are being removed on a 2028 schedule.
5. **MUST choose a licence that matches the strategy and keep it stable.** If you want a proprietary paid layer, use LGPL/MPL-style on the core (as Odoo did in 9.0). If you want a true community commons and protection against SaaS strip-mining, use AGPL on the core and fund through hosting and services. Decide once; do not relicense. Decide the contribution terms at the same time: Odoo's CLA (perpetual, irrevocable, sublicensable licence to Odoo S.A.) is what lets it combine community code with a proprietary layer, and a DCO-only policy with no CLA would remove that option.
6. **MUST plan the funding model before the licence.** Odoo's own retrospective says services margins and support could not fund a vendor's product, brand and partners, which drove the 2015 pivot. If obop will not withhold features, say what pays for engineering (section 10).

**SHOULD**

7. **SHOULD copy the pricing shape** for any hosted offering: one per-user price, all apps, a free single-app tier, a cheap "light user" seat for field and self-service staff, and free portal and customer users. Position the hosted price below Odoo's €24.90 to €44.90 list range (yearly billing), since the software itself carries no licence fee.
8. **SHOULD copy a partner programme with a low entry fee, 0% on services, and recurring commission on hosting and support.** Odoo's structure is €590 or €2,950 a year with 10% to 20% licence commission and 82% on extra-module maintenance. Avoid its non-solicit penalty and the "no services for unsubscribed customers" clause.
9. **SHOULD run a marketplace at 70/30 or better for vendors, with price parity rules, mandatory licence declaration, and a clear policy on cloning first-party modules.** Allow third-party code on the hosted offering through a sandbox (WASM or process isolation), which Odoo Online does not.
10. **SHOULD adopt a yearly or six-monthly release train with an explicit support matrix** (Odoo: 3 years standard; Tryton: 1-year series, 5-year LTS every fifth). Publish end-of-life dates at release. Prefer long-lived LTS for the data schema so migrations are rare.
11. **SHOULD give AI a bring-your-own-key and MCP-first design.** Odoo 20 meters all AI through IAP credits (per its release notes; whether its own "AI API keys" option avoids credits is unverified); a local-first, BYO-key approach undercuts it.
12. **SHOULD budget for community infrastructure**: a foundation-like body (Odoo's OCA exists because of the boundary problem), a backport/migration project, and events. OCA has only about 615 listed people despite Odoo's scale, so a healthier contributor funnel is a real opening.

**COULD**

13. **COULD offer an "Odoo import" path** (data, chart of accounts, custom fields) to capture upgrade-weary Odoo Community users, especially those on 17.0 and below, which have left standard support (and for subscribers attract the 25% old-version surcharge from about six months after 20.0's release).
14. **COULD position Rust and Vite+ as reliability and cost benefits** (single binary, low RAM per tenant, predictable latency) in hosted pricing rather than as a developer-facing slogan.
15. **COULD sell compliance-as-a-service** (e-invoicing network connectors, tax-rate feeds, bank feeds) as the paid layer. This is recurring, cannot be meaningfully forked, and is not a withheld feature.
16. **COULD consider a fair-source or delayed-open-source time lock** for any paid add-on, so paid code converts to open after N years. This answers the "VC captured, will relicense" worry seen in the 2019 thread.
17. **COULD avoid** Odoo-style contract levers (300% breach fee, 25% old-version fee, per-LOC maintenance, usage-reporting beacon). They generate revenue but feed the distrust that creates the opening in the first place.

## Verification log

Adversarial fact-check run on 2026-10-01 against live sources, mostly different from the author's: raw `odoo/documentation` and `odoo/odoo` files (20.0 and master), the odoo.com pages fetched as raw HTML (pricing, editions, partner, about, Odoo 20 release notes, Odoo Experience), the Odoo Apps store pages, the GitHub and GitLab APIs, PyPI, Wayback, Hacker News (Algolia API), and the Brussels Times, TechCrunch, General Atlantic and The Next Web pieces.

**Confirmed as stated: about 104 atomic claims** (counted per figure or statement, not per row). All 12 flagged load-bearing claims hold with the qualifications below, except the parts listed as corrected. Confirmed groups:

- Licences: LGPL-3 Community, OEEL-1 Enterprise (valid subscription for the right number of users), OPL-1, Odoo 8 AGPL-3 and 9 onward LGPL-3 plus Enterprise licence; `release.py` and `LICENSE` on master.
- Editions: every check and cross in the editions table (all rows parsed from the live HTML).
- Support and release: Odoo 20.0 Sept 2026 with standard support planned to Sept 2029, the full support matrix, 17.0 ending Sept 2026, 3-year standard support, yearly majors, SaaS minors every 2-3 months, upgrade rights, forced-upgrade rules, master at 20.1a1 with Python 3.12-3.14 and PostgreSQL 16+.
- Enterprise Agreement v13 (2026-09-24): 7% per year, 25% once per year from 6 months after a new major, 300%, monthly fee per 100 lines of code, €30,000 non-solicit, 3 covered versions, 50% liability cap, Nivelles court, 30-day notice, changelog entries v6 to v13.
- Pricing: One App Free €0, Standard €19.90 (€24.90 struck) and Custom €35.90 (€44.90 struck) on yearly billing, monthly-billing numbers, Light User €7.90, 12-month discount on initial users, Custom-only Studio, multi-company, external API and on-prem/Odoo.sh.
- Partner programme: €590 and €2,950, thresholds 10/75/300, certified staff 1/3/6, retention 70/80%, Enterprise commission 10/10/15/20%, Odoo.sh 10/50/50/50%, maintenance 82%, coupon caps €200 and €2,000, trademark and exclusivity rules.
- Apps store: 30% commission, 25% on IAP, €400 payout threshold, €9 minimum, EUR or USD, price parity, two-week refund window, no clones of Enterprise modules, no third-party apps on Online.
- Company figures: €620M revenue 2025, about 7,500 staff, 28M users (about 80% free), €10B stated on 24 Sept 2026, the price rise (about 20%, Standard unchanged), €36M 2025 loss, 48% growth, billings above €900M and €1B, revenue mix, 3,500 planned hires; 8,000+ staff, 21,000+ partners and "50 main applications" on the about page; €7B at the Jan 2026 General Atlantic deal with Wallonie Entreprendre keeping about 3% and 170,000+ customers; Nov 2024 €500M secondary at €5B from €3.2B with Summit staying the largest institutional holder; Dec 2019 $90M; Jun 2023 about €150M; 2023 revenue €282M.
- History: 2005 TinyERP, 2009 AGPL, 2010 pivot, 2013 OCA, 14 May 2014 rename, Odoo 8 in June 2014, the 2015 pivot and the €500k a month figures, the 80/20 target, Oct 2022 pricing change.
- Open-source side: 642 directories in `addons/` with 229 `l10n_*`, 16 under `odoo/addons/`, README "Billing & Accounting"; 33,902 forks; OCA 271 repos and 5.1k followers, 615 directory entries (366/317/77), €60 membership, OCB 376 stars and 202,861 commits, OpenUpgrade 991 stars, ERPNext 39.7k stars and GPL-3.0, Tryton (Nov 2008 fork of TinyERP 4.2, Belgian foundation Dec 2012, 6-month series, 1-year support, 5-year LTS, `trytond` 8.0.10); Flectra created 2018-01-16, 252 stars, branch 3.0 last commit 2026-06-30, mirror 133 stars, accounting in its free edition.
- Odoo 20 release notes: Nilvera integration now Enterprise-only, all AI features need IAP credits, MCP, record rules removed, Font Awesome replaced by Material Symbols; RPC removal schedule; Odoo Experience 24-26 Sept 2026 at Brussels Expo.
- Hacker News quotes and attributions (2019 and Dec 2025/Jan 2026 threads), including the 2,600 partners and 31k staff, 6,700 employees, 200k FTE, 16M users, obfuscate since v16, and the ERPNext founder's comment.

**Corrections (was / now / source):**

1. OCA repos "branched at 19.0, not yet 20.0" / 20.0 branches already exist: `server-tools`, `web` and `OpenUpgrade` each got a 20.0 initial commit on 2026-09-28, and OCB's 20.0 branch tracks Odoo (default branches are still 19.0) / GitHub API, `OCA/*` branches.
2. "Odoo 8 and earlier: AGPL-3" / the licence doc covers Odoo 8 only; OpenERP moved from GPL to AGPL in 2009 / https://www.odoo.com/documentation/19.0/legal/licenses.html ; https://odoo-community.org/about
3. Enterprise Agreement "confirms" first-year discounts are excluded from the per-user price / that text sits in a non-rendered `..` drafting comment in the `.rst`, not in the published contract / https://raw.githubusercontent.com/odoo/documentation/20.0/content/legal/terms/enterprise.rst
4. Non-solicit "€30,000 penalty per employee or contractor" / mutual clause; €30,000 is due when a breach leads the person to leave (same wording in the Partnership Agreement) / enterprise.rst section 6.2 ; partnership.rst section 8.2
5. "Odoo 13 moved ... the old Accounting report engine was dropped, and the reconciliation widget moved to the Enterprise repo" (stated as fact) / the March 2019 post announced these as plans. Payroll removal and coupons moving to Community are confirmed from the 12.0 vs 13.0 trees; the rest is unverified, and 13.0 Community still ships the reconciliation JS / https://www.odoo.com/blog/odoo-news-5/post/odoo-community-enterprise-532 ; GitHub trees `odoo/odoo` 12.0 and 13.0
6. Apps store "accepted values" licence list / vendor guidelines accept any licence compatible with dependencies and recommend LGPL-3 or OPL-1; the list of values comes from the manifest reference / https://apps.odoo.com/apps/vendor-guidelines ; module.rst
7. CEO's €10B described as "the price an acquirer would pay" / "what potential investors would be willing to pay to buy" the company, and he repeated he is not a seller / Brussels Times, 24 Sept 2026
8. Partner directory "4,458" / 4,454 ("All Countries" count on re-check) / https://www.odoo.com/partners
9. Apps catalogue "93,609" / 93,613 on re-check / https://apps.odoo.com/apps/modules/browse
10. `odoo/odoo` stars "54,767" / 54,768 on re-check / GitHub API
11. "Belga's €29 is rounded, likely €29.90, old page not seen" / archived 2024-07-21 page (USD) implies an old Custom price of about €29.90 and an unchanged Standard price, so +20% holds (derived, not seen in EUR) / Wayback snapshot of odoo.com/pricing-plan
12. TinyERP to OpenERP "about 2008" / kept, but the OCA timeline dates it 2009 (Odoo's story and Wikipedia say 2008) / https://odoo-community.org/about
13. Flectra "13,000+ teams" sourced to compare-editions / the claim ("more than 13,000 teams") is on the flectrahq.com home page / https://flectrahq.com/
14. "Third-party apps cannot run on Odoo Online" (section 6.2) / except data-only modules with no Python files (already correct in 3.3) / https://apps.odoo.com/apps/faq
15. RPC removal "Odoo 22 (fall 2028)" / also Online 21.1 (winter 2027); the `db` service was removed in Odoo 20 and Online 19.1 / https://raw.githubusercontent.com/odoo/documentation/20.0/content/developer/reference/external_rpc_api.rst
16. AI "paid IAP credits for every feature" / release notes say every AI feature needs credits bought via IAP, with no statement about a free allowance or bring-your-own-key; a stub "AI API keys" docs page exists (unverified) / https://www.odoo.com/odoo-20-release-notes
17. Gap 4 "whether `odoo/odoo` requires a CLA" / it does: ICLA/CCLA 1.0 (Apache-based) with a perpetual, irrevocable, sublicensable licence to Odoo S.A. / https://github.com/odoo/odoo/blob/20.0/doc/cla/sign-cla.md ; https://raw.githubusercontent.com/odoo/odoo/20.0/doc/cla/icla-1.0.md

**Added facts (missing from the first pass):** CLA terms; the Odoo.sh commission cap of €150 a month or €1,440 a year per subscription; Light User requires at least one full user (Belga); the Custom card now reads "Odoo Studio / Agentic AI" and the pricing page contradicts itself on Studio and multi-company in the free plan; Oct 2022 blog rationale and averages; the user-count series (5M, 10M, 16M, 28M) that does not reconcile; the alternative reading of TechCrunch's "20% Enterprise" line (user split); Odoo Experience budget versus income (€4.5M vs €3.5M).

**Unresolved (left as unverified in the text):**

- Whether Odoo 13 actually dropped the old Accounting report engine, moved the reconciliation widget, and open-sourced lead scoring and POS loyalty (only payroll and coupons checked).
- Whether Odoo 20 allows bring-your-own AI provider keys and whether that avoids IAP credits.
- Odoo.sh per-worker, per-GB and per-staging prices (JavaScript-rendered; only the ranges 1-8 workers, 1-512 GB and 0-20 staging environments are visible).
- Extended-support fee; exact release days of Odoo 9.0 and 20.0.
- Comparability of the user counts; the 2025 €36M loss (single source); 2024 revenue and the De Tijd article; the 2021 "$215M" and 2022 "€3.2B" headlines.
- The TechCrunch 2010 article on the €3M round was not opened (date taken from Wikipedia's citation).
- Flectra's exact Community and Professional licence terms; OCA's general licence policy.
- Whether "Odoo takes no share of partner services" is written anywhere beyond the partner page wording and the absence of a clause.
