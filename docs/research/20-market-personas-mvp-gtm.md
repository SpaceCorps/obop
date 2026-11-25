# obop: Market, Personas, Positioning, MVP Scope and Go-to-Market

Research date: 2026-10-01. Author: market/GTM researcher (workstream 20).

## Scope & method

Web search was not available in this environment, so discovery used direct page fetches of primary sources (Odoo, Frappe, Twenty, Eurostat, European Commission, AEAT, GitHub API through `gh`), analyst pages (Mordor, TechTarget citing Fortune BI, Enlyft) and news discovered through Google News RSS. About 75 fetches were made; roughly 25 failed or were bot-blocked (Gartner, IDC, Statista, Grand View, Panorama's gated report, vatcalc). Every number below carries a label: **[V]** verified on a live page today, **[S]** secondary or vendor-sourced, **[I]** my inference or arithmetic from verified inputs, **[E]** my estimate or assumption. Currency is not converted (EUR vs USD): about 1.16 USD per EUR is implied by ERP Today, which equates EUR 7B with about USD 8.1B ([ERP Today](https://erp.today/open-source-erp-ai-shortlist-erpnext-odoo/)).

---

## 1. Market size and adoption

### 1.1 Analyst figures disagree by about 30%

| Source | 2025 | 2026 | Forecast | CAGR | Notes |
|---|---|---|---|---|---|
| Mordor Intelligence, ERP market [V] | USD 71.62B | USD 78.15B | USD 120.96B (2031) | 9.12% (2026-31) | Cloud 55.11% of 2025 (page also says 55.73%); SMEs growing 14.29% (page also says 14.91%); top-5 vendors "a little more than 60%" (FAQ); page updated 2026-09-11. Further inconsistencies on the page: Microsoft is listed twice among leaders, and the FAQ gives cloud a 16.12% CAGR that the segment text assigns to hybrid. Internal inconsistencies, so treat as indicative. ([Mordor](https://www.mordorintelligence.com/industry-reports/enterprise-resource-planning-market)) |
| Fortune Business Insights, via TechTarget [S] | USD 92.6B | n/a | USD 281.6B (2034) | about 13% | ([TechTarget](https://www.techtarget.com/searcherp/feature/Explaining-the-hidden-costs-of-ERP-implementations)) |
| Global SaaS, all categories [S] | USD 408.21B | USD 465.03B | n/a | 13.32% (2025-34) | Precedence Research figures, quoted on Zylo's page. An Omdia USD 420B figure is not on that page (unverified). ([Zylo](https://zylo.com/blog/saas-statistics/)) |

Gartner, IDC and Statista pages were paywalled or returned 403, so no primary analyst figure is cited. Working assumption: **the ERP software market is roughly USD 70-90B in 2026 and the SME slice grows faster than the large-enterprise slice** [I].

For scale, Odoo's claimed 2025 revenue of EUR 620M ([Brussels Times](https://www.brusselstimes.com/2332306/odoo-announces-e10-billion-valuation-and-a-partial-price-increase)) is about USD 0.72B, roughly 0.9% of Mordor's 2026 figure [I]. The largest open-source ERP therefore has about 1% of the market, and the long tail is open.

### 1.2 SME adoption: ERP, cloud and the spreadsheet gap (EU, Eurostat 2025, enterprises with 10 or more staff)

| Indicator | All | Small (10-49) | Medium (50-249) | Large (250+) |
|---|---|---|---|---|
| Uses ERP software [V] | 46.45% | 41.08% | 69.93% | 88.71% |
| Uses CRM software [V] | 28.51% | 24.69% | n/s | 65.43% |
| Buys paid cloud services [V] | 52.74% | 49.3% | 66.78% | 84.67% |

Sources: [ERP/CRM](https://ec.europa.eu/eurostat/statistics-explained/index.php?title=Digital_economy_and_society_statistics_-_enterprises), [cloud](https://ec.europa.eu/eurostat/statistics-explained/index.php?title=Cloud_computing_-_statistics_on_the_use_by_enterprises).

Of cloud buyers, 58.16% buy cloud accounting software, 30.12% cloud ERP and 27.91% cloud CRM [V].

Derived points:
- About 15.9% of all 10+ enterprises buy cloud ERP (0.3012 x 0.5274), against 46.45% using any ERP, so roughly one in three ERP users consumes it as cloud [I].
- About 59% of small (10-49) EU firms report no ERP [V, by subtraction]. Eurostat does not measure spreadsheets, so "they run on spreadsheets plus an accounting package" is my inference [I].
- Micro firms (fewer than 10 staff) are 94.4% of the EU's roughly 34M enterprises (2024 data, whole business economy including financial activities), and SMEs are 99.8% ([Eurostat SBS](https://ec.europa.eu/eurostat/statistics-explained/index.php?title=Structural_business_statistics_overview)). The Eurostat ICT survey excludes micro firms, so micro-firm adoption is unknown.
- About 1.9M EU firms have 10 or more staff (34M x 5.6%) [I]. Assuming about 85% of those are small [E], about 0.95M small firms lack an ERP (1.6M x 59%) [E]. At EUR 3-4k revenue per account (section 3.2), that is an upper-bound illustration of **EUR 3B/yr for EU small firms alone**, and this is not a forecast.

Spreadsheet attachment is real but the evidence is biased. A Datarails survey (an FP&A vendor) reports 40% of US and UK finance pros prefer Excel over alternatives, against 26% who rank ERP first for planning, with almost every respondent having found a material Excel error ([IT Pro, Dec 2025](https://www.itpro.com/software/microsoft-excel-is-still-alive-and-kicking-at-40-and-its-surging-in-popularity-as-82-percent-of-finance-professionals-report-emotional-attachment-to-the-spreadsheet-software)) [S].

### 1.3 Implementation risk is the real market friction

- Gartner (via TechTarget) says more than 70% of recent ERP initiatives will fail to fully meet their goals and up to 25% fail catastrophically [S]. Panorama reports 30% of projects over budget, with scope expansion as the top cause [S]. Acumatica estimates USD 150-750k implementation for SMBs [S]. ([TechTarget](https://www.techtarget.com/searcherp/feature/Explaining-the-hidden-costs-of-ERP-implementations))
- Odoo's own Success Pack page claims a "98% implementation success rate" with a pack versus 65% without [V, vendor claim]. It also says 80% of projects in the estimator's scope go live in 200 hours or less, and no method or source is given for either figure ([Odoo](https://www.odoo.com/pricing-packs)).

---

## 2. Competitive reference set

### 2.1 Odoo (the functional reference; live facts)

| Fact | Value |
|---|---|
| 2025 revenue / 2026 outlook | EUR 620M (2025); 2026 turnover +48% YoY; billings above EUR 900M expected in 2026, EUR 1B in 2027 [S, CEO statements] |
| Valuation | EUR 10B, self-declared (Sept 2026); EUR 7B after General Atlantic's January 2026 deal; USD 5.26B in Nov 2024 ([Brussels Times](https://www.brusselstimes.com/2332306/odoo-announces-e10-billion-valuation-and-a-partial-price-increase), [Tech Funding News](https://techfundingnews.com/belgian-unicorn-odoo-reaches-e7b-valuation-after-general-atlantic-investment/), [Wikipedia](https://en.wikipedia.org/wiki/Odoo)) |
| Users | 28M, about 80% on free plans (Brussels Times); Odoo's About page says 28M+ ([Odoo](https://www.odoo.com/page/about-us)). ERP Today still quotes 16M, so counts are not reconciled. |
| Paying customers | More than 170,000 [S] (Tech Funding News). Revenue per paying customer is about EUR 3.6k/yr [I]. |
| Headcount | About 7,500 (Brussels Times); "8,000+" on Odoo's own page. Target 10,000 by 2027 with about 3,500 hires in 12 months. |
| Pricing (Oct 2026) | One App Free (one app, unlimited users). Standard EUR 19.90 per user/month (12-month discounted price for initial users; list EUR 24.90; monthly billing EUR 31.10). Custom EUR 35.90 (list EUR 44.90; monthly EUR 56.90). Light user EUR 7.90 (quote-based). Standard is Odoo Online only; Odoo.sh and on-prem only on Custom (Odoo.sh hosting is not included in the plan price). ([Odoo pricing](https://www.odoo.com/pricing-plan)). The Brussels Times reports Enterprise rising from EUR 29 to EUR 35, the first increase in four years, with the basic plan staying at EUR 19.90; these round figures do not match the pricing page's EUR 35.90 Custom and EUR 24.90 Standard list prices, so treat the press figures as approximate. |
| Versions | 19.0 released 2025-09-18 ([Wikipedia](https://en.wikipedia.org/wiki/Odoo)). GitHub default branch is `20.0`, and `saas-19.1`..`19.4` exist ([GitHub API](https://github.com/odoo/odoo)). Odoo 20: the Brussels Times (24 Sep 2026) quotes the CEO that users will "soon" gain access, but Odoo's release-notes index already lists "Version 20, Released September 2026" with a release-notes page and 20.0 docs, and `odoo/release.py` on `20.0` reads `(20, 0, 0, FINAL, 0)` ([Odoo release notes](https://www.odoo.com/page/release-notes), [Odoo 20 notes](https://www.odoo.com/odoo-20-release-notes)). Treat 20 as released or imminent; the exact GA date is unverified. Its notes say AI features need IAP credits and that Odoo can be connected via MCP, which weakens "agent-ready" as a sole differentiator. |
| License | Community LGPLv3 ([LICENSE](https://github.com/odoo/odoo/blob/master/LICENSE)); Enterprise proprietary, source-available to subscribers. |
| Repo | 54.8k stars, 33.9k forks, 2,515 contributors per the GitHub sidebar on 2026-10-01 (the REST contributors API lists only 304; an earlier "about 4,300" figure could not be reproduced) [V] ([GitHub](https://github.com/odoo/odoo)) |
| Community add-on dirs | 642 addon directories, of which 229 are `l10n_*` localization modules [V, GitHub contents API, 20.0 branch] |
| Ecosystem | OCA: 664 members, about 20,000 modules, 1,600 contributors ([OCA](https://odoo-community.org/)). App Store: "nearly 40,000" modules per Tech Funding News [S] (Odoo's About page says 50,000+ community apps). Featured paid apps cost about EUR 30-549 ([Odoo Apps](https://apps.odoo.com/apps)). |

**The open-core line, verified by absence from the public Community repo.** The Community `account` module is literally named "Invoicing" ([manifest](https://github.com/odoo/odoo/blob/master/addons/account/__manifest__.py)). Absent from the Community repo (so Enterprise-only) are `account_accountant`, `account_reports`, `account_asset`, `account_budget`, `helpdesk`, `sign`, `documents`, `planning`, `industry_fsm`, `hr_payroll`, `knowledge`, `approvals`, `marketing_automation`, `sale_subscription`, `sale_renting`, `mrp_plm`, `quality`, `stock_barcode`, `studio`, `voip`, `iot`, `appointment`, `hr_appraisal` [V, GitHub contents API]. Community does contain CRM, sales, stock, MRP, purchase, POS, project, HR basics, website/eCommerce, mass mailing, fleet, maintenance and survey. This is the single clearest product opening for a fully open suite.

**Sentiment, secondary (HN thread "Odoo: Open-Source ERP", 31 Dec 2025, 76 points, 69 comments)** ([HN](https://news.ycombinator.com/item?id=46439993)) [S]: "open core, not open source"; upgrades are "super complex" and need a service, not a script, which is the Odoo founder's own justification for centralising upgrades ([comment](https://news.ycombinator.com/item?id=46486373)); support is not in the licence price, so NetSuite comparisons are contested; Dolibarr and ERPNext are offered as fully open alternatives; Odoo's heavy out-of-home and podcast advertising is noticed.

### 2.2 Other open-source comparables (GitHub API, 2026-10-01) [V]

| Project | Stars | License | Notes |
|---|---|---|---|
| Odoo | 54.8k | LGPLv3 (CE) + proprietary EE | Python/JS; 61MB Python and 51MB JS in repo |
| ERPNext (Frappe) | 39.7k | GPL-3.0 per the repo's `license.txt`, no paid tier (Frappe's pricing page says AGPL-3.0; the repo file governs) | v16.37.0 released 2026-09-30 (v15.121.6 the same day). Framework MIT; apps `hrms` 8.9k, `books` 5.0k, `crm` 3.7k, `helpdesk` 3.4k, `lms` 3.3k. GitHub sidebar shows 741 contributors (REST API lists about 280). |
| Twenty CRM | 57.8k | AGPLv3; files marked `/* @license Enterprise */` are commercial (about 466 code-search hits, no `ee/` directory); SDK, `twenty-ui` and `twenty-apps` packages are MIT ([LICENSE](https://github.com/twentyhq/twenty/blob/main/LICENSE)) | 767 contributors on the GitHub sidebar (REST API lists 464); USD 9 (Pro) / 19 (Organization) per seat per month on yearly billing, Enterprise from USD 50k/yr ([Twenty pricing](https://twenty.com/pricing)); grew from about 20k stars (Nov 2024) ([TechCrunch](https://techcrunch.com/2024/11/18/twenty-is-building-an-open-source-alternative-to-salesforce/)) |
| Medusa | 36.5k | MIT + enterprise carve-out | Commerce |
| Dolibarr | 7.7k | GPL-3.0 | v24.0.1 (v24 announced 2026-09-11); "millions of users" (self-claimed, no figure given) ([site](https://www.dolibarr.org/)) |
| InvenTree | 7.7k | MIT | Inventory |
| Tryton | 226 (GitHub mirror) | n/s | Foundation-run; series 8.2 due 2026-10-05 ([tryton.org](https://tryton.org/)) |
| **Best Rust "ERP" repos** | **40, 23, 16 stars** | n/a | `forgeerp`, `rerp`, `obvia` (GitHub search `erp language:rust`, 2026-10-01); no credible Rust incumbent |

Installed base, from technology-detection vendor Enlyft (crawl estimates [S]):
- Odoo 88,510 identified companies: 77% under 50 staff, most commonly 1-10 staff, US 15%, France 9%, Belgium 8%, Spain 5% ([Enlyft](https://enlyft.com/tech/products/odoo)).
- ERPNext 5,639, with India 26% ([Enlyft](https://enlyft.com/tech/products/erpnext)).
- QuickBooks 274,637 ([Enlyft](https://enlyft.com/tech/products/quickbooks)) and NetSuite 14,120 ([Enlyft](https://enlyft.com/tech/products/netsuite-erp)).

**Odoo's actual customer is a 1-50 person firm.** The "mid-market ERP" image is marketing.

### 2.3 New threats

- AI-native accounting/ERP startups are raising large rounds aimed at NetSuite-class mid-market: Rillet USD 100M Series C at USD 1B (Aug 2026), Campfire USD 35M Series A (Jun 2025, followed by a USD 65M Series B in Oct 2025), Doss USD 55M Series B (Mar 2026, AI inventory layer for ERP). I saw only TechCrunch/Fortune/Yahoo/Business Wire headlines via Google News RSS, not the articles [S].
- ERP Today argues AI is reopening the shortlist for open, inspectable, API-rich ERP, but notes AI maturity is not automatic ([ERP Today, mid-2026; the page shows no date](https://erp.today/open-source-erp-ai-shortlist-erpnext-odoo/)) [S].
- Open-source rug-pull risk: **Cal.com closed its source in April 2026**, citing AI-driven vulnerability discovery, and the public repo became `cal.diy` (48.8k stars), an MIT community edition with all enterprise features removed and a "strictly recommended for personal, non-production use" warning ([repo README](https://github.com/calcom/cal.diy); the README itself does not mention the April change or the reasons, which come from 15-22 Apr 2026 headlines in The New Stack, It's FOSS, BetaNews and GIGAZINE, plus XDA on 11 May, that I could not open) [V for repo state, S for date and reasons].

---

## 3. Personas, jobs-to-be-done and willingness to pay

### 3.1 Persona table

Fit = fit with the MVP loop (contacts, CRM/sales, inventory, invoicing/accounting, payments). WTP figures are price anchors from live vendor pages; the "obop plausible" column is my estimate [E].

| Persona | Typical stack today | Core jobs-to-be-done | Pain | Fit | Price anchors | Buying motion |
|---|---|---|---|---|---|---|
| **Solo founder / freelancer (1-4)** | Accounting SaaS + invoicing + spreadsheet CRM | Send compliant invoices, see cash, file VAT | Mandates (Peppol/PDP), tool fatigue | Medium: wants invoicing-only | Xero USD 27-97/mo, no per-user fee ([Xero](https://www.xero.com/us/pricing-plans/)); QuickBooks USD 38-340/mo ([QBO](https://quickbooks.intuit.com/pricing/)); Odoo One App Free; Twenty self-host free | Self-serve trial, accountant referral. Low revenue, high community value. |
| **Agency / professional services (5-20)** | QuickBooks/Xero + HubSpot + PM/timesheet tool + Zapier | Lead to quote to project to timesheet to invoice to payment; utilisation and margin | Data in 4-6 tools, re-keying, no margin view | **High** | HubSpot Starter USD 20/seat list (USD 7 promo) ([HubSpot](https://www.hubspot.com/pricing/crm)); obop plausible EUR 300-700/mo [E] | Self-serve plus accountant; founder-led decision in days |
| **Manufacturer / distributor (20-100)** | QuickBooks/Sage + inventory tool (Katana: USD 299/mo + add-ons) + spreadsheets | Stock accuracy, purchase to receive to sell to invoice; BoM/MO; lots | Overselling, wrong COGS, month-end pain | **High for distributors, medium for manufacturers** (MRP is phase 3) | Katana Core from USD 299/mo, WMS +149, Manufacturing +199, onboarding USD 2,000 ([Katana](https://katanamrp.com/pricing/)); Odoo EUR 20-45 per user/mo + about EUR 10-20k implementation [E] | Partner-led or consultant-led; 2-6 month cycle |
| **Retail (1-10 stores)** | POS + accounting + stock spreadsheet | POS to stock to accounting; promotions | Certified fiscal POS rules (e.g. France NF525) | Low for MVP (needs POS, phase 3) | Shopify POS Pro add-on SEK 870/mo per location ([Shopify](https://www.shopify.com/pricing)) | Hardware resellers |
| **E-commerce brand (DTC, 3-30 staff)** | Shopify + Katana/inFlow + QuickBooks + HubSpot | Order sync, inventory across channels, COGS, cash | Integrations break; Shopify fees stack up | Medium-high if connector-first | Shopify Basic/Grow SEK 269-729/mo on annual billing | Self-serve app-store motion |
| **Nonprofit / association** | CiviCRM/Excel + accounting | Members/donors, grants, receipts, reporting | Fragmented tools, tiny budgets | Medium (CRM + invoicing + fund accounting) | Frappe discounts non-profits (apply by contacting Frappe); CiviCRM AGPL, used by "tens of thousands of individuals and organisations" (page wording, no source) ([CiviCRM](https://civicrm.org/about)) | Volunteers decide; a small Ask-HN thread (4 points, 5 comments) shows no clear default ([HN Feb 2026](https://news.ycombinator.com/item?id=46920322)) |
| **Local government** | Legacy vendor ERP, tenders | Budget/commitment accounting, procurement, asset register | Lock-in, sovereignty rules | Low near term | EU Tech Sovereignty Package proposes a "Free Software first" principle (Jones Day) but Tech Policy Press finds no explicit procurement preference [S] | Tenders, 12-24 months; deeply national accounting |

### 3.2 SaaS sprawl and TCO (a modelled estimate, not a quote)

Sprawl evidence [S]:
- BetterCloud's 2025 survey of about 600 IT professionals finds 106 apps per company (112 in 2023), and 70% prefer a unified platform ([PR Newswire](https://www.prnewswire.com/news-releases/state-of-saas-2025-report-reveals-operational-complexity-and-risk-concerns-as-economic-uncertainty-and-ai-apply-spending-pressure-on-technology-investments-302441868.html)).
- Zylo's 2026 SaaS Management Index cites 305 apps for the average company (the page does not say "large organisation") and 54% licence utilisation ([Zylo](https://zylo.com/blog/saas-statistics/)).
- I found no verified SMB-specific sprawl figure. Both datasets skew to mid-market and enterprise. **Do not quote them as SMB facts.**

Illustrative annual licence cost for two archetypes. Prices are list prices from the pages above (QBO Plus includes 5 users and Advanced 25, so extra seats would add cost; vendor promos such as QBO 50% off for 3 months and HubSpot USD 7 Starter are excluded); rows marked [E] are my assumptions.

| Scenario | SaaS stack | Odoo Standard (list) | Odoo Custom (list) | ERPNext (Frappe Cloud) | Self-host OSS |
|---|---|---|---|---|---|
| **15-user agency** | QBO Plus USD 1,680 + HubSpot 10 seats USD 2,400 + PM tool 15 x USD 10 x 12 = 1,800 [E] + glue USD 600 [E] = **about USD 6.5k** | 15 x EUR 24.90 x 12 = **EUR 4.5k** (EUR 3.6k promo year 1) | EUR 8.1k | USD 60-1,500 hosting, no per-user fee | VPS plus ops time [E] |
| **35-user distributor** | QBO Advanced USD 4,080 + HubSpot 10 seats USD 2,400 + Katana Core + WMS USD 5,376 + glue USD 1,200 [E] = **about USD 13k** | **EUR 10.5k** | **EUR 18.9k** | USD 60-1,500 | VPS plus ops time [E] |
| One-off implementation | Low (self-serve) | Odoo's own Success Pack: 100 h = EUR 9,900; 200 h = EUR 19,800 (EUR 99/h; new customers get 15% off their first pack) | same | Partners (200+); Frappe does not implement | Internal |

Takeaways [I]:
1. **At 15 users the licence price alone does not beat a SaaS stack decisively.** The win is consolidation, one source of truth, ownership and compliance, not sticker price.
2. At 35 users, if the firm needs Odoo's Custom tier (API, multi-company, Studio), **Odoo can cost more in licences than the SaaS stack**. ERPNext's resource-priced hosting (USD 5-125/mo, no per-user fee) is an order of magnitude cheaper on licences ([Frappe pricing](https://frappe.io/erpnext/pricing)). The real cost in open source is services: "Services, not software, often become the long-term cost center" ([ERP Today](https://erp.today/open-source-erp-ai-shortlist-erpnext-odoo/)).
3. **Pricing lesson:** per-user pricing is the axis where Odoo is exposed (it now also sells a EUR 7.90 light-user tier). Per-organisation or per-resource pricing (Xero, Frappe Cloud) is a credible differentiator, especially if Rust lowers cost to serve.

### 3.3 Procurement patterns [I unless cited]

Under 20 users: self-serve trial, with the accountant as influencer (QuickBooks bundles two accountant seats on Simple Start, Essentials and Plus, and three on Advanced). 20-100 users: a partner or consultant runs the project, and Odoo takes no cut of partner services ([partner program](https://www.odoo.com/become-a-partner)). Public sector: tenders, with any open-source preference arriving only after EU legislation (section 6.2).

---

## 4. Partner and implementer economics

Odoo partner program ([Odoo](https://www.odoo.com/become-a-partner)) [V]:

| Tier | New Enterprise users/yr | Certified staff | Min retention | Fee |
|---|---|---|---|---|
| Learning | none | none | none | EUR 590/yr |
| Ready | 10 | 1 | none | EUR 2,950/yr |
| Silver | 75 | 3 | 70% | EUR 2,950/yr |
| Gold | 300 | 6 | 80% | EUR 2,950/yr |

- Commission on Enterprise is "up to 20%". Partner services (analysis, configuration, development, training, hosting) are partner-priced and are "the main revenue source."
- Directory snapshot 2026-10-01: **4,455 partners in 169 countries** (4,454 on an earlier fetch the same day, so the count drifts) ([Odoo partners](https://www.odoo.com/partners)). Odoo's About page claims "21,000+" partners, which I could not reconcile (likely a broader definition).
- Illustrative partner maths [E]. List prices are EUR 299 (Standard) to EUR 539 (Custom) per user/yr, so a 20% commission is at most EUR 60-108 per user/yr. A Gold partner at the 300-user floor earns at most about EUR 18-32k/yr in commission, while one typical 200-hour project at about EUR 100/h is about EUR 20k in services. **Services dominate commissions by about an order of magnitude** (a partner closing even 30 such projects a year bills roughly EUR 600k in services against at most EUR 32k in commission) [E]. A new platform that wants partners must make implementation profitable (fast, well-documented, low-code customisation), not just offer commission.
- Frappe/ERPNext: 200+ implementation partners and no direct implementation ([Frappe](https://frappe.io/erpnext/pricing)). Frappe lists about 85 team members, with multiple partner-manager roles ([Frappe about](https://frappe.io/about)).

**Partner density by geography (Odoo directory)** [V]:

| Region | Partners |
|---|---|
| MENA | Egypt 204, Saudi Arabia 197, UAE 123, Iraq 45 |
| LATAM | Argentina 178, Mexico 162, Venezuela 136, Peru 87, Guatemala 86, Brazil 49, Colombia 43 |
| South Asia / SEA | India 172, Pakistan 77, Indonesia 68, Thailand 43, Philippines 41, Vietnam 33, Malaysia 25 |
| Europe | Belgium 137, France 137, Spain 106, **Germany 102**, Italy 73, Switzerland 59, Netherlands 50, UK 40, **Poland 20**, Sweden/Denmark/Norway 6 each, Finland 7 |
| US / Canada | 280 / 93 |

The partner network is densest where the dollar is weak and compliance is local (MENA, LATAM, India). It is thin relative to economic size in Germany, the UK, Poland and the Nordics. The Nordic result reflects weak Odoo coverage, not weak incumbents: local accounting incumbents there are strong, from my background knowledge [unverified].

---

## 5. Open-source go-to-market: what the comparables show

| Playbook element | Evidence | Lesson for obop |
|---|---|---|
| **Developer-led adoption** | Twenty: USD 5M raised, 3 founders, about 50 investors (TechCrunch does not give an angel count), 300+ contributors in year one, about 20k stars (Nov 2024) to 57.8k (Oct 2026) [V/S]. Positioned on "network effects, ecosystems, extensibility." | GitHub-first, docs-first, public roadmap, AGPL-plus-commercial is the current template |
| **Free entry** | Odoo One App Free (unlimited users); Twenty self-host; Frappe Cloud from USD 5/mo | A free, genuinely useful first app is table stakes |
| **Managed cloud as the revenue engine** | Frappe Cloud from USD 5/mo (USD 20 and USD 125 for higher server tiers), 100+ apps, 14-day trial; Twenty USD 9/19 per seat | Managed hosting with upgrades/backups is what SMBs pay for |
| **Marketplace / templates** | Odoo paid apps about EUR 30-549 (Emipro Shopify/WooCommerce connectors EUR 549); Frappe quick-install apps; OCA about 20,000 modules | Need an extension API that outsiders can monetise |
| **Partners as sales channel** | Odoo 4,455 directory partners; Frappe 200+ | Services-led channel is where 20+ user deals close |
| **Brand spend** | Odoo Experience: 45,000 expected attendees, EUR 4.5M budget; OOH and podcast ads (Brussels Times, HN) | obop cannot outspend Odoo; win in channels where it cannot cheaply follow (open, Rust, compliance, docs) |
| **Licence strategy** | Odoo LGPL (lets proprietary add-ons exist); Frappe framework MIT, apps GPL/AGPL; Twenty (`@license Enterprise` files) and Medusa (`ENTERPRISE-LICENSE.md`) open-core; **Cal.com closed-source reversal** | Licence choice is a trust commitment. Flip-flops are punished. |

Proposed GTM funnel [E]:
1. Public repo from day one, 5-10 flagship "recipes" (for example Peppol-compliant invoicing in 15 minutes, Shopify to stock to invoice).
2. Free compliant-invoicing app with unlimited users, to use mandate deadlines as the forcing function (section 6.1).
3. Managed cloud private beta.
4. Accountant programme (free seats, co-branded onboarding).
5. Implementer certification once there are 50 or more production tenants.

---

## 6. Geography and regulatory tailwinds and headwinds

### 6.1 EU e-invoicing: a forced-buying moment, but dates slip

| Country / scope | Milestone | Source |
|---|---|---|
| Belgium | B2B e-invoicing mandatory from 1 Jan 2026 (Peppol BIS 3); penalties from EUR 1,500 (OpenText; KPMG gives no penalty figure); e-reporting law for 2028 (vatcalc headline only [S]) | [KPMG](https://kpmg.com/lu/en/insights/risk-and-regulation/hitting-send-time-preparing-mandatory-e-invoicing-requirements.html), [OpenText](https://blogs.opentext.com/e-invoicing-europe-2026/) |
| France | 1 Sep 2026: all must be able to receive; large/mid firms must also send and e-report. 1 Sep 2027: SMEs and micro-enterprises. Must go through a certified platform (PDP/"plateforme agréée"). Odoo's docs say Odoo is itself certified as a plateforme agréée (vendor claim; I could not reach the DGFiP list to confirm). Trade-press headlines of 1 Sep 2026 report the mandate live and the 2026/2027 dates confirmed [S]. | [Odoo France docs](https://www.odoo.com/documentation/19.0/applications/finance/fiscal_localizations/france.html) |
| Germany | Receiving EN 16931 e-invoices since 1 Jan 2025; paper B2B invoices allowed until 31 Dec 2026, and until end-2027 for firms with prior-year turnover up to EUR 800k, so issuing is mandatory from 2027 for larger firms and from 1 Jan 2028 for all others; EDI must be EN 16931 interoperable by 2028 (OpenText); formats XRechnung, ZUGFeRD/Factur-X | [de.wikipedia (E-Rechnung)](https://de.wikipedia.org/wiki/Elektronische_Rechnung), [OpenText](https://blogs.opentext.com/e-invoicing-europe-2026/), [KPMG](https://kpmg.com/lu/en/insights/risk-and-regulation/hitting-send-time-preparing-mandatory-e-invoicing-requirements.html) |
| Poland | KSeF mandatory from 1 Feb 2026 for firms with 2024 sales above PLN 200M, from 1 Apr 2026 for all others; firms invoicing up to PLN 10k/month may stay outside KSeF until 1 Jan 2027. Penalties paused to 2027 (OpenText). Headlines of 16 Sep 2026 (vatcalc) and 28 Sep 2026 (globalvatcompliance) say penalties are deferred to 2028, while a 22 Sep headline says deferral "proposed until 2027"; I could not read the articles, so the penalty date is unverified. | [Ministry of Finance KSeF portal](http://ksef.podatki.gov.pl/od-kiedy-trzeba-wystawiac-faktury-w-ksef/), [OpenText](https://blogs.opentext.com/e-invoicing-europe-2026/) |
| Greece | Firms over EUR 1M from Feb 2026, smaller from Oct 2026 | [OpenText](https://blogs.opentext.com/e-invoicing-europe-2026/) |
| Spain | Veri*factu software adaptation required before 1 Jan 2027 (corporate-tax payers) and before 1 Jul 2027 (others), per RDL 15/2025 | [AEAT](https://sede.agenciatributaria.gob.es/Sede/iva/sistemas-informaticos-facturacion-verifactu/nota-informativa-ampliacion-plazo-adaptacion-facturacion.html) |
| EU | ViDA adopted 11 Mar 2025; digital reporting and e-invoicing for intra-EU B2B from 1 Jul 2030; national systems aligned by 1 Jan 2035 | [European Commission](https://taxation-customs.ec.europa.eu/taxation/vat/vat-digital-age-vida_en) |
| UAE | Voluntary pilot July 2026 (Gulf News 26 Jun 2026, vatcalc 1 Jul 2026 headlines); mandate 2027 in phases (Peppol five-corner); an Oct 2026 service-provider appointment deadline is reported in headlines | [OpenText](https://blogs.opentext.com/e-invoicing-europe-2026/); 2027 phasing from vatcalc/Khaleej Times/A&M headlines only [S] |
| Saudi Arabia | ZATCA Phase 1 since 4 Dec 2021, Phase 2 since 1 Jan 2023 in waves; 25th wave announced Aug 2026 (KPMG headline, 25 Aug 2026; the ZATCA pages list the phases but not the waves) | [ZATCA](https://zatca.gov.sa/en/E-Invoicing/Pages/default.aspx), KPMG headline |
| Malaysia | MyInvois phased by turnover, first wave for firms above RM100M; Wikipedia says Jun 2024 and all businesses by 2027, but that text looks dated (start date unverified). Feb 2026 headlines say the RM1m-RM5m wave moved to Jan 2027 and an Aug 2026 headline mentions a threshold rise (headline-only, unverified) | [Wikipedia](https://en.wikipedia.org/wiki/Electronic_invoicing) [S] |

Cross-country lesson from OpenText: some regimes need up to 400 data fields that ERPs often lack, and manual Word/Excel invoicing is the main failure point [S]. **Mandates create demand for exactly the small firms running Word/Excel invoices, but dates move (Poland, Spain).** Do not time a launch to a single date.

### 6.2 EU digital sovereignty and the Cyber Resilience Act

- On 3 Jun 2026 the Commission proposed a Tech Sovereignty Package with the Cloud and AI Development Act (CADA), a full Open Source Strategy with about EUR 2B over 7 years, and a 2030 target of 30M active users of open-source collaboration tools; the same package also carries a Chips Act 2.0 ([Jones Day](https://www.jonesday.com/en/insights/2026/06/eu-proposes-tech-sovereignty-package-with-major-implications-for-digital-markets), [Tech Policy Press](https://www.techpolicy.press/how-the-eus-tech-sovereignty-package-finally-puts-open-source-to-the-test/)) [S]. Jones Day reports a "Free Software first" principle for public cloud/AI procurement; Tech Policy Press says the text has no explicit procurement preference. **Treat the procurement effect as unconfirmed.** Jones Day places the "Free Software first" principle in the Open Source Strategy, not in CADA (whose procurement element is "Union added-value" award criteria), and does not say whether the Strategy itself needs Parliament and Council adoption (unverified). Jones Day says 18-24 months to final adoption would be typical for the package.
- The Commission notes EU IT spend of EUR 264B/yr, mostly proprietary (Tech Policy Press) [S].
- Cyber Resilience Act: in force 10 Dec 2024, **vulnerability/incident reporting from 11 Sep 2026**, full application 11 Dec 2027 ([EC](https://digital-strategy.ec.europa.eu/en/policies/cyber-resilience-act)) [V]. Commercial vendors of software products with digital elements become "manufacturers." Fines reach EUR 15M or 2.5% of turnover ([Wikipedia](https://en.wikipedia.org/wiki/Cyber_Resilience_Act)). A commercial open-source company selling a managed service must plan for CRA compliance from day one.

### 6.3 Rest of world

- **LATAM / MENA / India / SEA:** large Odoo partner density (section 4) and ERPNext's India concentration (26%) show demand for low-cost open ERP. The compliance cost is enormous and local (Brazil NF-e, Mexico CFDI since 2004 per Wikipedia, India GST, KSA ZATCA, Indonesia Coretax). These markets are reachable only through local partners who carry the localisation. The Odoo docs list 103 fiscal-localisation countries, of which 45 have dedicated pages ([Odoo](https://www.odoo.com/documentation/19.0/applications/finance/fiscal_localizations.html)) [V].
- **Recommendation:** EU-first for Phase 1-2; rest-of-world via "localisation partners" in Phase 3+ (section 9).

---

## 7. Verticals and segments with weak incumbent coverage

Evidence is thin; this is mostly inference [I]:

1. **Open "Enterprise-only" features.** Helpdesk, field service, subscriptions, rental, documents/sign, payroll, barcode and quality are absent from Odoo Community (section 2.1). ERPNext/Frappe covers helpdesk, LMS, CRM and HRMS as separate apps. A suite that ships these open is a horizontal differentiator.
2. **Sub-50-person EU firms facing e-invoicing mandates.** 59% of small firms report no ERP, and Odoo has 6 partners in Sweden, 20 in Poland and 40 in the UK.
3. **Agencies / professional services.** Fragmented SaaS stack, light inventory needs, so a lighter accounting burden.
4. **Shopify-native brands.** Specialist inventory tools (Katana USD 299+/mo) and separate accounting leave an integration gap.
5. **Nonprofits and associations.** CiviCRM (AGPL), Dolibarr and ERPNext serve them, but the Feb 2026 Ask-HN thread shows no clear default. Low revenue but strong community alignment.
6. **Local government.** Policy tailwind (CADA, "Free Software first" if adopted), but national accounting rules, long tenders and a 18-24 month legislative lag make this a Phase 4+ target.

---

## 8. Positioning: "one business, one platform"

**Positioning statement (long form).** For owner-led businesses with 5-250 people that run on a patchwork of subscriptions and spreadsheets, obop is the fully open-source business platform that runs sales, stock, invoicing, accounting and payments on one shared data model, so there is one record of every customer, product and euro. Unlike Odoo, it does not hold accounting, helpdesk or documents behind an enterprise paywall. Unlike a SaaS stack, you own your data and pay per organisation, not per app or per seat. Built in Rust, it is a single fast binary that is cheap to host, and it is agent-ready (typed API, MCP), with e-invoicing (Peppol/EN 16931) built in from day one.

**Short form.** "One business. One platform. Open, fast, yours."

**Proof points to earn (not yet true; each is a hypothesis to benchmark)**:
1. 100% of first-party apps under the same open licence.
2. Cost to serve per tenant materially below a Python/ORM stack. Odoo's own docs size 1 worker to about 6 concurrent users, 60 users needing 9 workers and about 3 GB of RAM ([Odoo deploy docs](https://www.odoo.com/documentation/19.0/administration/on_premise/deploy.html)) [V]. Our benchmark target should beat this by 5-10x [E].
3. Zero-to-first-compliant-invoice in under 15 minutes.
4. Import from Odoo, ERPNext, QuickBooks and Xero.

**Against each competitor:**

| Competitor | Their strength | obop angle |
|---|---|---|
| Odoo | Breadth, brand, partners, EUR 10B valuation, 3,500 planned hires | Fully open suite, per-org pricing, Rust performance, no upgrade-as-a-service lock |
| ERPNext | GPL with no paid tier, 200+ partners, cheap cloud | Modern Rust/TypeScript stack, e-invoicing-native EU story, stronger UX and AI tooling |
| SaaS stack (QBO, Xero, HubSpot, Katana) | Best-of-breed polish | One data model, no integration tax, ownership |
| AI-native ERPs (Rillet, Campfire) | AI UX, VC money | Open, self-hostable, SMB price point |

---

## 9. MVP scoping strategies

### 9.1 What Odoo's own code says about scope

In the Odoo 20.0 Community tree, the `account` module's `models/` directory is about **1.87 MB of Python**, against 0.32 MB for `sale`, 0.87 MB for `stock`, 0.48 MB for `mrp`, 0.41 MB for `point_of_sale`, 0.22 MB for `crm` and 0.19 MB for `purchase` [V, GitHub contents API]. **Accounting is roughly 5-6 times the size of sales and the single largest cost centre**, before counting 229 `l10n_*` modules. The loop's riskiest component is accounting and compliance, not CRM or inventory.

### 9.2 Options

| Option | Apps | Strength | Weakness |
|---|---|---|---|
| **A. Horizontal thin loop** (recommended core) | 1 Contacts/Products (shared `party` + catalogue), 2 CRM, 3 Sales (quote to order), 4 Inventory + Purchase (one warehouse), 5 Invoicing + Accounting (single-country, Peppol), 6 Payments (card + SEPA + bank feed), 7 Portal/Documents (PDF, email, customer portal). Add 8 Project/Timesheets as the services variant | Proves "one data model" end to end; matches the brief; covers distributors, agencies and brands | Broad; accounting dominates cost and risk |
| **B. Vertical wedge** | Pick one ICP (for example agencies or Shopify brands) and build only its path | Fastest to first revenue; clear message; lighter accounting | Weak proof of platform; risk of a niche ceiling; re-architecture |
| **C. Platform/framework first** (Frappe path) | Metadata engine + 2 apps | Elegant for implementers | No customer value for a long time; Frappe needed about 85 people for its ecosystem |
| **D. Compliance-first entry product** | Free e-invoicing/Peppol + invoicing app, expanding into A | Rides mandate deadlines; mirrors Odoo's "One App Free"; shortest path to users | Low initial ARPU; needs a Peppol access-point partner |

**Recommendation: D as the on-ramp and A as the product, with services firms and light-trade distributors (5-50 staff, one country) as the first ICP.**
- Entry is one free app (compliant invoicing) with unlimited users.
- Land with invoicing and payments; expand to CRM/sales, then inventory/purchase.
- Do **not** attempt manufacturing (BoM/MO), POS, eCommerce, payroll or multi-country tax in the MVP.
- Each later app must reuse the same `party`, `product`, `document` and `ledger` primitives, or it is out of scope.

### 9.3 Explicit MVP cut lines [E]

| In | Out (phase 3+) |
|---|---|
| Multi-currency, single-company, single legal entity | Multi-company consolidation |
| Double-entry GL, AR/AP, VAT, bank reconciliation, period lock | Payroll, fixed assets, budgets |
| Peppol send/receive via a certified access point; EN 16931 UBL/CII (XRechnung, Factur-X) | Becoming a certified PDP ourselves |
| Single warehouse, FIFO/average cost, serial/lot later | WMS, barcode, 3PL |
| REST + webhooks + MCP; declarative custom fields; no-Rust customisation path | Studio-style visual builder |
| Import from CSV/Odoo/QuickBooks | Full Odoo-compatible module API |

---

## 10. Phased roadmap, team and skills

Team benchmarks: Twenty got its first traction with 3 founders and USD 5M [S]; Frappe runs ERPNext, a framework, about 20 apps (unverified) and a cloud with about 85 people (my count of profile cards on its About page; no stated headcount) [S]; Odoo employs about 7,500 [S].

| Phase | Months | Milestones | Team |
|---|---|---|---|
| **0 Foundations** | 0-3 | Core data model (party, product, document, ledger, event log); tenancy/auth/RBAC; module manifest and registry; UI shell on Vite+ (v1.0.0 released 2026-09-28, MIT, [voidzero.dev](https://voidzero.dev/posts/announcing-vite-plus-1-0)); OpenAPI + MCP surface; licence, CLA and trademark decided; docs site; 3-5 design-partner firms signed | 4-6 |
| **1 Loop alpha to beta** | 3-12 | 7 apps for one country; Peppol integration via a partner access point; demo seed data; dogfood internally; public beta; private managed-cloud beta at M9 | 8-12 |
| **2 v1.0 and paid cloud** | 12-20 | GA; paid managed cloud; accountant portal; bank feeds; migration importers (Odoo, QuickBooks, Xero, ERPNext); second country; extension SDK + marketplace alpha; CRA readiness; SOC 2/ISO 27001 start | 14-20 |
| **3 Breadth** | 20-32 | Manufacturing, POS, eCommerce/website, HR/expenses, helpdesk, documents/sign (all open); 3-5 countries; partner programme live; localisation-partner model for LATAM/MENA/India | 25-40 |
| **4 Ecosystem** | 32+ | Verticals, marketplace revenue share, certified implementers, public-sector track | 40+ |

Core skills for Phase 0-2 [E]:
- 3-4 senior Rust backend engineers (Rust is used by 14.8% of developers, against 57.9% for Python and 43.6% for TypeScript, so hiring takes effort; [Stack Overflow 2025](https://survey.stackoverflow.co/2025/technology)).
- 2 TypeScript/Vite+ frontend engineers.
- **1-2 accounting domain experts** (CPA or equivalent, part-time acceptable).
- 1 product designer.
- 1 platform/SRE.
- 1 developer advocate and docs lead.
- 1 security/compliance owner from Phase 2.
- Current toolchain versions: Rust 1.98.1 (2026-09-03) ([GitHub](https://github.com/rust-lang/rust/releases)); Vite+ 1.0.0 (2026-09-28, MIT) bundles Vite 8.3.1, Rolldown 1.2.11, Vitest 5.0.1, tsdown 0.23.0, Oxlint 1.85.0 and Oxfmt 0.70.0 ([release notes](https://github.com/voidzero-dev/vite-plus/releases/tag/v1.0.0)). Vite+ needs Node.js ^22.18.0 || ^24.11.0 || >=26.0.0; newer upstream patches (Rolldown 1.2.12, Vitest 5.0.3) already exist outside the bundle.

---

## 11. Risks

| Risk | Evidence | Mitigation |
|---|---|---|
| **Scope explosion** | Odoo CE has 642 addon dirs; Odoo has about 7,500 staff; Gartner-cited failure rates; Panorama scope-creep finding | Enforce section 9.3 cut lines; each phase ships a complete loop; "no new app without a design partner" rule |
| **Accounting localisation burden** | `account` is about 1.9 MB of Python models; 229 `l10n_*` dirs; Odoo documents 103 localisation countries | One country at first; pluggable localisation packs; chart-of-accounts data from official sources, not copied from Odoo's LGPL data; recruit a local accountant per country |
| **Trust and certification** | Odoo had to become a certified French PDP (per its own docs) and ships the `l10n_fr_pos_cert` VAT anti-fraud certification module for POS (CGI 286 I-3 bis; the NF525 label is background knowledge, unverified); Spain's Veri*factu imposes producer specs (RD 1007/2023, Order HAC/1177/2024); CRA reporting duty from 2026-09-11 | Use certified third parties (Peppol access point, PA) initially; audit logs and ledger immutability from day one; SOC 2/ISO 27001 before enterprise sales; CRA process |
| **Extension ecosystem vs Rust** | Odoo has 2,515 GitHub-listed contributors and implementers write Python/XML; only 15% of developers use Rust | Provide a non-Rust extension path (declarative models, scripts, webhooks, UI builder); never make implementers compile Rust |
| **Licence/trust whiplash** | Cal.com closed source in April 2026; Odoo criticised as open-core | Publish a governance charter; put all first-party apps under one licence; CLA caution; trademark policy |
| **Business model** | A GetLatka headline (Oct 2024, unverified estimate) puts ERPNext at about USD 3.9M ARR despite 39.7k stars | Managed cloud + compliance connectors + support + marketplace from Phase 2; avoid free-only trap |
| **Incumbent response** | Odoo adds AI (Agentic AI on Custom plan), raises prices, has 3,500 planned hires | Compete on openness, per-org pricing, compliance speed |
| **Mandate slippage and AI-era security** | Poland and Spain deadlines moved; Cal.com cited AI-driven vulnerability scanning for closing | Treat mandates as a tailwind, not a launch date; treat security as a feature (fuzzing, dependency policy, bug bounty) |

---

## 12. Success metrics (proposed targets [E], benchmarks [V/S])

**North-star:** weekly active organisations completing the loop (quote to invoice to payment reconciled) at least once.

| Metric | M6 | M12 | M24 |
|---|---|---|---|
| Design partners / production orgs | 5 | 50 | 500 |
| Paying managed-cloud orgs | 0 | 10 | 150 |
| Time to first compliant invoice (new trial) | under 30 min | under 15 min | under 10 min |
| GitHub stars (benchmarks: Twenty 57.8k, ERPNext 39.7k, Odoo 54.8k) | 1k | 5k | 15k |
| External contributors (benchmarks, GitHub sidebar 2026-10-01: ERPNext 741, Twenty 767, Odoo 2,515) | 10 | 50 | 200 |
| Certified partners / implementers | 0 | 5 | 30 |
| Ledger integrity | 0 known trial-balance-breaking bugs; property tests on double-entry invariants | same | same |
| Compliance | Peppol conformance test passed | 1 country audited by local accountant | 2 countries + SOC 2 Type I |
| Gross retention (paying) | n/a | at least 90% | at least 95% |

---

## Confidence & gaps

**High confidence [V]:**
- Odoo pricing and plan structure.
- Odoo partner programme terms and the directory snapshot.
- Repo-based open-core evidence.
- Eurostat adoption numbers.
- Mordor's figures (internally inconsistent).
- EC dates for ViDA and CRA, and the AEAT Spain dates.
- GitHub star, licence and release facts.
- Vite+ v1.0.0 and Rust 1.98.1 dates.

**Medium [S]:**
- Odoo revenue, valuation, users and headcount (CEO statements in press; counts differ across Odoo's own pages and ERP Today).
- Enlyft installed-base counts (technographic crawl).
- OpenText and KPMG country timelines (vendor and advisory blogs).
- Fortune/Gartner/Panorama numbers quoted through TechTarget.
- HN sentiment.

**Gaps and discrepancies:**
- Gartner, IDC, Statista, Grand View and Panorama numbers are gated; no primary analyst figure.
- No verified SMB-specific SaaS-sprawl metric.
- Odoo partner revenue is not public; the maths in section 4 is illustrative. The "21,000+" versus 4,455 partner gap is unreconciled.
- Odoo 20 GA date is unconfirmed: Odoo's release-notes index says "Released September 2026" and 20.0 docs and release notes exist, but the CEO was quoted on 24 Sep saying users will "soon" gain access.
- Poland's KSeF penalty date is contradictory (2027 per OpenText; headlines of 16 and 28 Sep 2026 say 2028, one of 22 Sep says a deferral to 2027 was proposed); the Ministry of Finance portal pages I could read are silent on penalties.
- India, LATAM, SEA and KSA mandates were seen only via headlines or old Wikipedia text (Malaysia's MyInvois start date and wave thresholds in particular are unverified).
- France's "Odoo is a plateforme agréée" claim rests on Odoo's own documentation; the DGFiP list could not be fetched. Belgium's 2028 e-reporting date is a headline only. Germany's issuing timeline was confirmed from de.wikipedia because the BMF site blocked the fetch.
- Contributor counts depend on method (GitHub sidebar vs REST API: Odoo 2,515 vs 304, ERPNext 741 vs 280, Twenty 767 vs 464); the earlier "Odoo about 4,300" could not be reproduced.
- An Omdia USD 420B business-SaaS figure and Frappe's "about 20 apps" are unverified.
- Local-government and nonprofit willingness to pay is inferred, not sourced.
- The TCO table mixes list prices with my assumptions and omits payroll, payments fees, HR and e-commerce tools.
- Whether ERPNext's 39.7k stars convert to meaningful revenue is only a third-party estimate.
- I did not verify DATEV (Germany) or other national accountant-software interoperability requirements, nor the copyright status of national charts of accounts.

---

## Implications for obop

**MUST**
1. **Ship one open licence for all first-party apps and publish a governance/licence charter before the first public commit.** Accounting, helpdesk, documents and payroll-lite being open is the sharpest wedge against Odoo's enterprise paywall, and Cal.com's April 2026 reversal shows the cost of licence flip-flops. (Legal review is needed on AGPL-3.0 core plus a permissive extension/SDK boundary, as Frappe does with MIT framework and GPL apps.)
2. **Scope the MVP as the order-to-cash and procure-to-pay loop in seven apps (Contacts/Products, CRM, Sales, Inventory+Purchase, Invoicing+Accounting, Payments, Portal/Documents), for one country, with the first ICP being 5-50 person services and light-trade firms.** Hold the cut lines in section 9.3, because accounting is the largest module in Odoo's own code (about 1.9 MB of model code, 5-6 times sales).
3. **Build e-invoicing (EN 16931 UBL/CII, Peppol BIS 3) into the core from day one, through a certified third-party access point.** Do not try to become a certified French PDP or a Peppol access point in the MVP. Belgium (Jan 2026), France (Sep 2026/2027), Germany (2027/2028) and Spain (2027) create the forced-buying moment, but dates slip.
4. **Provide a non-Rust customisation path from day one** (declarative models and fields, scripting or WASM, webhooks, a UI builder, importers). The implementer channel that carries Odoo is Python/XML-fluent, and Rust is used by only 14.8% of developers.
5. **Make ledger correctness and auditability non-negotiable:** immutable journal, period locks, property-based tests on double-entry invariants, full audit log. Trust and certification (NF525, Veri*factu, CRA) are gates, not features.
6. **Plan CRA compliance now.** Reporting obligations began 2026-09-11 and full application is 2027-12-11, which applies if obop sells a managed service.

**SHOULD**
7. **Lead with a free compliant-invoicing app (unlimited users) and expand into the loop.** This mirrors Odoo's "One App Free" and uses the mandates as the funnel.
8. **Price managed cloud per organisation or per resource bundle (Xero and Frappe Cloud style), not per seat,** and publish prices. Odoo's EUR 20-45 per user/month list and its 2026 price rise (EUR 29 to 35) leave room, but at 15 users the licence delta against a SaaS stack is small, so lead with consolidation, ownership and compliance rather than cost.
9. **Choose the first country by three criteria:** a founding accountant who knows it, mandate pressure, and thin Odoo/incumbent partner coverage. Germany (102 Odoo partners for the EU's largest economy; issuing mandatory from 2027 for firms above EUR 800k prior-year turnover and from 2028 for all others) is attractive on demand but likely needs DATEV interoperability (unverified, so verify first). A Benelux or Nordic start is easier operationally.
10. **Benchmark cost-to-serve against Odoo's documented sizing (1 worker per about 6 users; 60 users need about 3 GB)** and publish the result. "A fast single binary" is a hypothesis until measured.
11. **Target a lean team of 8-12 by month 12, rising to 14-20 by month 20,** with 1-2 accounting domain experts from day one. Frappe's roughly 85-person organisation is the ceiling for the breadth you can plausibly match in the first three years.
12. **Build the importers early** (Odoo, QuickBooks, Xero, ERPNext, CSV). Odoo's own ecosystem states that upgrades are hard, and migration is the main switching cost.
13. **Start a partner/accountant programme at the 50-tenant mark,** structured so implementation is profitable (fast time to go-live, strong docs). Partner economics are mostly services, not commissions.

**COULD**
14. Offer a nonprofit/association tier (free or heavily discounted) to seed community and references, with no revenue expectation.
15. Build a Shopify/WooCommerce connector-first package for DTC brands as the first vertical pack after the core loop.
16. Explore public-sector readiness (EU hosting, CRA, CADA assurance levels) only after the EU package is law (18-24 months).
17. Use localisation partners in MENA, LATAM, India and SEA in Phase 3+, modelled on Odoo's partner density (Egypt 204, Saudi 197, Argentina 178, India 172, Mexico 162).
18. Publish an "obop vs Odoo vs ERPNext vs SaaS stack" TCO calculator with assumptions visible, as content marketing.

---

## Verification log

Adversarial fact-check run on 2026-10-01. Tools: WebFetch of primary pages, the GitHub REST API via `gh`, the npm registry, the Hacker News API and Google News RSS (WebSearch was not available). Independent primary sources were used where reachable (GitHub trees and release notes instead of secondary summaries, Eurostat, EC, AEAT, Odoo repo and docs, Ministry of Finance KSeF portal); where a primary page was blocked (BMF Germany, DGFiP, EUR-Lex) the fallback is named.

**Confirmed as stated: about 110 distinct data points**, including all of the following.
- Mordor 71.62 / 78.15 / 120.96, 9.12% CAGR and the inconsistencies, Fortune BI USD 92.6B.
- Odoo EUR 620M, EUR 10B, 28M users / 80% free, about 7,500 staff, +48% turnover, 3,500 hires, Odoo Experience 45,000 / EUR 4.5M.
- Every Odoo price on the pricing page, the partner tiers and fees, the 642 addon dirs / 229 `l10n_*`, the `account` manifest name "Invoicing", default branch `20.0`, and the absence of every listed Enterprise module from the Community tree.
- All Eurostat 2025 and 2024 figures; France 1 Sep 2026 / 2027; Spain 1 Jan / 1 Jul 2027 under RDL 15/2025; ViDA 11 Mar 2025 / 1 Jul 2030 / 1 Jan 2035.
- Tech Sovereignty Package 3 Jun 2026, EUR 2B over 7 years, 30M users, EUR 264B; CRA 10 Dec 2024 / 11 Sep 2026 / 11 Dec 2027.
- Vite+ 1.0.0 on 2026-09-28 under MIT, bundling Vite 8.3.1, Rolldown 1.2.11 and Vitest 5.0.1 (GitHub release notes and npm); Rust 1.98.1 on 2026-09-03.
- Star counts and licences for Odoo, ERPNext (v16.37.0), Twenty, Medusa, Dolibarr (24.0.1), InvenTree, Tryton (8.2 due 2026-10-05) and the three Rust ERP repos; cal.diy under MIT with the "non-production" warning; Cal.com's April 2026 closure (seven headlines dated 15-22 Apr 2026).
- Enlyft counts, Rillet / Campfire / Doss rounds, BetterCloud, Xero, QuickBooks, HubSpot, Katana, Shopify, Frappe pricing, Stack Overflow 2025 (14.8% / 57.9% / 43.6%), Odoo worker sizing (1 worker ~ 6 concurrent users; 60 users = 9 workers, ~3 GB), model-directory sizes (account 1.87 MB vs sale 0.32 MB) and the HN thread metadata.

**Corrections (was / now / source)**

| Was | Now | Source |
|---|---|---|
| Odoo 20 is "coming soon", GA unconfirmed | Odoo's release-notes index lists Version 20 as "Released September 2026", with release notes and 20.0 docs; the CEO quote says "soon". Exact GA date unverified. Release notes also mention MCP connectivity and IAP-credit AI | https://www.odoo.com/page/release-notes, https://www.odoo.com/odoo-20-release-notes |
| Odoo repo "about 4,300 contributors" | 2,515 on the GitHub sidebar (REST API lists 304); 4,300 not reproducible | https://github.com/odoo/odoo |
| ERPNext about 280 contributors; Twenty 464 (benchmark table) | Same basis as Odoo (sidebar): ERPNext 741, Twenty 767; the API gives 280 and 464 | GitHub sidebar and REST API |
| Twenty "AGPLv3 + commercial `/ee` files" | AGPLv3 with commercial files marked `/* @license Enterprise */` (no `ee/` directory); SDK, `twenty-ui` and `twenty-apps` are MIT; USD 9/19 is per seat on yearly billing | https://github.com/twentyhq/twenty/blob/main/LICENSE, https://twenty.com/pricing |
| Germany: "most businesses must issue from 2027" | Paper allowed to 31 Dec 2026; firms with prior-year turnover up to EUR 800k have until end-2027, so issuing is mandatory from 2027 for larger firms and from 1 Jan 2028 for all others | https://de.wikipedia.org/wiki/Elektronische_Rechnung (BMF blocked), KPMG article |
| Poland: only first wave Feb 2026 | Also 1 Apr 2026 for all other taxpayers and 1 Jan 2027 for firms invoicing up to PLN 10k/month; penalty date headlines now cite two 2028 items (16 and 28 Sep) | http://ksef.podatki.gov.pl/od-kiedy-trzeba-wystawiac-faktury-w-ksef/ and /etapy-wdrozenia-ksef/ |
| Zylo: 305 apps "per large organisation"; Omdia USD 420B on Zylo's page | 305 is the average company; the USD 408B / 465B figures are Precedence Research global SaaS; Omdia is not on the page (unverified) | https://zylo.com/blog/saas-statistics/ |
| Mordor top-5 "a bit over 60%" | "a little more than 60%" (FAQ wording) | Mordor page |
| Frappe Cloud "150+ installable apps" | "100+ apps" | https://frappe.io/cloud |
| Odoo paid apps EUR 43-549 | Featured paid apps about EUR 30-549 (Emipro connectors EUR 549 confirmed) | https://apps.odoo.com/apps |
| Partner directory 4,454; Egypt 203 | 4,455; Egypt 204 (live count drifts; 169 countries confirmed) | https://www.odoo.com/partners |
| QuickBooks "two accountant seats on every paid plan" | Two on Simple Start / Essentials / Plus, three on Advanced | https://quickbooks.intuit.com/pricing/ |
| HN "super complex upgrades" presented as critic sentiment | The comment is the Odoo founder's defence of the paid upgrade service; thread date 31 Dec 2025 | https://hn.algolia.com/api/v1/items/46439993 |
| Datarails: "nearly all" found a material error; 26% for ERP | "Almost every respondent"; 40% of US/UK finance pros prefer Excel; 26% rank ERP first | IT Pro, 12 Dec 2025 |
| Twenty "about 50 angels" | About 50 investors including institutions (Runa, YC, Automattic); no angel count | TechCrunch, 2024-11-18 |
| ERP Today "Jun 2026" | Page shows no date (image paths suggest mid-2026) | ERP Today page |
| Press "EUR 29 to EUR 35" presented without caveat | Brussels Times also says basic stays EUR 19.90; the pricing page shows EUR 35.90 Custom and EUR 24.90 Standard list, so the press numbers are rounded | Brussels Times, Odoo pricing |
| Jones Day "Free Software first" attributed to the package | It sits in the Open Source Strategy, not CADA; the Strategy's formal legal status is not stated (unverified); package also includes a Chips Act 2.0 | Jones Day |
| CiviCRM "tens of thousands of orgs" | "individuals and organisations" combined, no source | https://civicrm.org/about |
| Campfire "USD 35M Series A (2025)" | Correct, but a USD 65M Series B followed in Oct 2025; Doss round was a Series B | Google News RSS headlines |
| Malaysia MyInvois "from Jun 2024, all by 2027" | Marked unverified: Wikipedia text looks dated; headlines say the RM1m-5M wave moved to Jan 2027 | Wikipedia, Google News RSS |
| Vite+ bundle versions missing from the file | Added Vite 8.3.1, Rolldown 1.2.11, Vitest 5.0.1, tsdown 0.23.0, Oxlint 1.85.0, Oxfmt 0.70.0 and the Node requirement | https://github.com/voidzero-dev/vite-plus/releases/tag/v1.0.0 |

**Unresolved**
- Poland KSeF penalty start (2027 vs 2028): headline-only.
- Malaysia MyInvois start date and wave thresholds; UAE per-group phasing; Belgium 2028 e-reporting date; Saudi wave numbers (the KPMG headline is the only evidence for wave 25).
- Odoo being a "plateforme agréée": only Odoo's own documentation; the DGFiP list was unreachable.
- Odoo 20 exact GA date; whether the Open Source Strategy needs legislative adoption.
- NF525 naming for Odoo's French POS module (the module is labelled CGI 286 I-3 bis); Frappe "about 20 apps" and the roughly 85 headcount (my own card count); the Odoo "21,000+" partner claim.
- Gartner, IDC, Statista and Grand View figures (gated); Omdia USD 420B; no 2026 Stack Overflow survey URL was found (2025 remains the latest verified).
- ERPNext licence: the repo says GPL-3.0 but Frappe's pricing page says AGPL-3.0; the repo file was used.
- Odoo contributor count depends on method (2,515 sidebar vs 304 API).
