# 26 - Business model, pricing, unit economics, funding and consolidated risk register

Research date: 2026-10-01. Author: commercial/risk workstream (26). Currency EUR unless stated; prices ex-VAT.

## Scope & method

Inputs: sibling notes 03, 05, 09, 13, 16, 17, 18, 20, 22, plus live fetches today of vendor pricing pages, the Xero FY26 ASX release and investor deck (PDF text extracted locally), GitLab's Q2 FY27 release, OpenPeppol fees, Twilio, Stripe, Levels.fyi, Eurostat, NLnet, Sovereign Tech, the EIC and the EU Open Source Strategy. No search engine was usable (Bing returned noise; DuckDuckGo showed a bot check I did not try to pass), so discovery was by direct URL. Tags: **[V]** verified on a live primary page today, **[S]** secondary or vendor-sourced, **[I]** my inference or arithmetic, **[E]** my estimate or assumption, **[B]** page blocked or unreadable. The 3-year model is mine; every parameter is in B6 and nothing in it is a forecast of record.

## Bottom line

1. **Open the software completely; sell operations, assurance and liability.** The "one open edition" promise (03/04/20) and the security features that 13/18 treat as core can both be kept if the paid list is short, closed and ratcheting (A2). Only SAML-native/SCIM-full identity, advanced audit assurance, fleet control plane, air-gap bundle, support/SLA, compliance packs and hosting are paid.
2. **Price per organisation, banded by full users**, with free portal, accountant and light/ESS users (A3). Obop Business at EUR 249/org/month covers up to 25 full users, against Odoo Custom at EUR 44.90/user list (15 users = EUR 673.50).
3. **The model is honest and sobering.** On 20's phases (4-6 to 37 people by month 36) the base case needs about EUR 10.8M peak funding, reaches EUR 2.2M recurring ARR at M36 and first breaks even at M64 [E]. Gating phase-3 hiring to ARR of EUR 1M cuts the trough to about EUR 7.1M (B5).
4. **People cost is the P&L.** Hosting is about 6% of Y3 revenue; a 2x infra error moves the trough by EUR 0.4-0.5M (B7: 10.8M to 11.3M after rounding), while +/-15% on loaded salary moves it by EUR 2.4-2.6M (B7). Rust is justified by correctness, CRA posture and operations, not hosting cost [I].
5. **Grants validate but do not fund.** NLnet Restack is roughly EUR 7M in total through 2030 across all awardees, in EUR 5-50k first grants [V]. Expect under 6% of base burn from grants and sponsors.
6. **The licence pledge must be contractual before the first priced round** (C3): CLA pledge plus foundation veto plus investor covenant.

---

## Part A - Packaging

### A1. Live pricing benchmark (fetched 2026-10-01 unless noted)

| Vendor | Unit and prices | Gating / notes | Source |
|---|---|---|---|
| **Odoo Online** | Per user/month, yearly billing: Standard 19.90 (list 24.90), Custom 35.90 (list 44.90); monthly billing 24.90 / 44.90 (list 31.10 / 56.90). One App Free EUR 0. Light User 7.90 via advisor [V] | Custom adds Odoo.sh/on-prem (on-premise is listed only under yearly billing), Studio and Agentic AI, multi-company, External API. Light User = flagged user or HR employee without a non-portal user (time off, expenses, attendance, barcode, payslips, POS); requires at least one full user [V Belga, below]. Discount lasts 12 months on initial users; IAP credits (SMS, AI) extra. Custom +20% announced Sept 2026 (Enterprise/Custom from 29 to 35 per the article, Standard unchanged at 19.90) [V https://www.brusselstimes.com/belgium/2332306/odoo-announces-e10-billion-valuation-and-a-partial-price-increase] | https://www.odoo.com/pricing-plan |
| **Frappe Cloud** | Resource-based: Sites from USD 5/month; Servers from USD 20 ("incident-only" support; the Hetzner tie is not stated on the page, unverified) or USD 125; the cloud page says Servers from USD 40 [V] | "Not per user"; Product Warranty = bug fixes only, no functional support; nonprofit discounts on request; 200+ certified partners, Frappe does no implementation; "no features hidden behind paywalls" | https://frappe.io/erpnext/pricing, https://frappe.io/cloud/pricing |
| **Twenty** | Pro USD 9 (yearly), Organization USD 19, Enterprise from USD 50k/yr; unlimited seats [V] | Org adds row-level permissions, SSO (SAML/OIDC), audit logs, key rotation; Enterprise adds SCIM, single-tenant, IP allow-list, DPA, SLA. API, MCP, 2FA, workflows on all tiers; API 50/100 req/min | https://twenty.com/pricing |
| **Xero** | UK: Ignite 18, Grow 39, Comprehensive 55, Ultimate 70 GBP/month (90% off six months). US: Early 27, Growing 59, Established 97 USD (80% off three months) [V] | No per-user fee; invoice/bill caps on lowest tier; payroll US USD 36 + 6/employee (Gusto), UK GBP 1.50/person; bank feeds mostly free | https://www.xero.com/uk/pricing-plans/, https://www.xero.com/us/pricing-plans/ |
| **QuickBooks Online** | New **Free** plan (1 user, 2 invoices/month); Simple Start 38 (promo 19), Essentials 85, Plus 140, Advanced 340 USD; 50% off three months [V] | User caps 1/3/5/25; 2-3 accountant seats included | https://quickbooks.intuit.com/pricing/ |
| **Holded** | Plus 15, Basic 29, Standard 59, Advanced 99, Premium 199 EUR/month; 50% off three months; users 2/4/7 (Basic/Standard/Advanced) [V, curl] | Accountant seat free; paid add-ons: HR EUR 1.50/employee, payroll EUR 5/employee, inventory, manufacturing and POS EUR 25 (POS per store), SII from EUR 30, extra user EUR 10, assisted setup from EUR 99; "900,000+ users" claim | https://www.holded.com/pricing |
| **Teamleader** | "From EUR 37.5"; SMART/GROW/FLOW; boosters EUR 20-50 (project mgmt and planning EUR 35); annual -17%; second user free; Peppol send in SMART, receive in GROW [V] | Per-user figures not rendered (calculator) | https://www.teamleader.eu/pricing |
| **Zoho One** | Page shows "US$90" for Flexible-user pricing; All-Employee price not rendered; Essentials 15+ apps, Standard 50+ [V partial] | All-employee requires licensing every employee | https://www.zoho.com/one/pricing.html |
| **Business Central** | Essentials USD 80, Premium USD 110, Team Members USD 8 per user/month, paid yearly [V] | Agents need Copilot Credits plus Azure subscription; partner-sold | https://www.microsoft.com/en-us/dynamics-365/products/business-central/pricing |
| **NetSuite** | **[B]** 403 on every pricing URL | Not retrievable; treat as quote-only | https://www.netsuite.com/portal/pricing.shtml |
| **Metabase / Supabase / n8n / GitLab / Grafana** | Metabase Starter USD 100/month, Pro 575, Enterprise from USD 20k/yr; Supabase Pro USD 25, Team USD 599; n8n Business EUR 667/month self-hosted; GitLab Premium USD 29/user; Grafana Pro from USD 19 plus usage, Enterprise from USD 25k/yr [V] | Metabase Pro gates SSO, row permissions, audit, white-label; Enterprise = same software plus SLA, procurement, air-gap. Supabase Team adds SOC 2/ISO reports and SSO. n8n Business adds SAML/LDAP with a daily licence check | https://www.metabase.com/pricing/, https://supabase.com/pricing, https://n8n.io/pricing/, https://about.gitlab.com/pricing/, https://grafana.com/pricing/ |
| **Akaunting / Invoice Ninja** | Akaunting cloud USD 8 / 24 / 56 / 145 per month (yearly); Invoice Ninja Free (5 clients), Pro USD 14, Enterprise USD 18 (1-2 users) to 300 (51-100 users); self-host white-label USD 40/yr; 250 Peppol credits in Enterprise [V] | Akaunting gates double-entry and bank feeds by tier (BSL licence, 05); Invoice Ninja raised prices 2026-01-01 and lists self-hosted white-label at USD 40/yr there, not on its pricing page [V] | https://akaunting.com/pricing, https://invoiceninja.com/pricing-plans/, https://invoiceninja.com/pricing-update-january-1-2026/ |

Read-across [I]: per-organisation or resource pricing (Xero, Holded, Frappe, Twenty) is the norm at the low end; only Odoo, BC, Zoho One and GitLab are per-user. Identity gating (SSO, SCIM, audit) sits at mid or top tiers everywhere.

### A2. Resolving the enterprise layer against the one-open-edition promise

Conflict: 17 s11.2 recommends gating "SSO/SAML/SCIM, advanced audit and retention, control plane, air-gap, white-label"; 03/04/20 promise one open edition with no moving boundary; 18 lists OIDC, SCIM subset, hash-chained audit, envelope encryption and signed releases as P0/P1 (core); 05 warns against a relicensing CLA.

**Decision rules** [I]: (1) every security *outcome* must be reachable in the open edition; the paid layer sells operator convenience, assurance and fleet scale; (2) never gate security fixes (CRA Annex I Part II(8) requires free updates, 17 s7.2), data export, the API, ledger integrity or the existence of an audit log; (3) the paid list is published, closed and only moves paid to open (a ratchet). Paid `ee` code converts to open after 24 months (COULD in 03 s10, adopted here).

| Capability | Open (AGPL) | Paid | Rationale |
|---|---|---|---|
| All first-party apps (accounting, inventory, MRP, CRM, HR, payroll engine, POS, helpdesk, documents) | Yes | - | Odoo withholds accounting, payroll, helpdesk; this is the wedge (03 s3, 20 s2.1) |
| REST, webhooks, MCP, OpenAPI, importers, exports | Yes | - | Odoo gates external API to Custom (03 s3.3) |
| Multi-company, custom fields/models, WASM extensions | Yes | - | Odoo gates to Custom |
| Password + TOTP + passkeys, OIDC SSO, LDAP bind, RBAC, record rules, field ACLs | Yes | - | 18 P0/P1 |
| SAML | Documented bridge (Keycloak/Authentik) | Native SAML SP, multi-IdP, JIT provisioning (`ee`) | `samael` is pre-1.0 and C-linked (18 s2); support-heavy |
| SCIM 2.0 | Subset (`/Users`, `/Groups`, PATCH, `eq`, ETag) | Full: Bulk, advanced filters, group-to-role policies (`ee`) | 18 SHOULD #14 |
| Audit log | Append-only, hash-chained, 13-month default, UI and export | WORM/object-lock checkpoint publication, SIEM streaming, legal hold, multi-year tiering, auditor evidence packs (`ee`) | 18 s6 and P2 #20-21 |
| Encryption | Envelope encryption, local/OpenBao KEK | Cloud-KMS/HSM connectors, customer-managed keys, region pinning (hosted Scale) | 18 P1 #11 vs P2 #19 |
| Backups | pgBackRest/PITR recipe, per-tenant export | Fleet DR tooling, cross-region failover | 17 s6: ops features counter hosting rivals |
| Control plane (fleet provisioning, upgrades, quotas, billing) | Single-business install | Multi-tenant control plane (`ee`, partner-hosting licence) | Needed only by hosts and MSPs |
| Air-gap | Source + signed artefacts + docs | Supported offline bundle, offline licence/updates | Metabase gates air-gap |
| White-label | Not needed (own use) | Trademark licence in partner programme, not code | WP Engine lesson: objective policy |
| AI | BYOK, MCP server, usage ledger, budget caps | Hosted AI credits at cost+20% | 16 s13; Odoo 20 forces IAP credits (03) |
| Security fixes, SBOM, signed releases, CRA reporting | Always | Never | CRA |
| Support, SLA, compliance packs, hosting | Community only | Yes | Liability and speed cannot be forked |

### A3. Pricing unit, plans and ecosystem

**Unit: per organisation ("one business"), banded by full users.** Full user = creates/edits transactional records. Portal users (customers, suppliers) and up to 3 accountant seats are free (QuickBooks bundles 2-3, Holded one, both [V]). Light/ESS users (time off, expenses, timesheets, payslips, kiosk/POS operator, barcode) are free up to 5x full users, fair-use. This removes the per-seat tax that Odoo answers with a EUR 7.90 Light tier.

| Hosted plan (yearly billing, ex-VAT) | Price | Includes [E] |
|---|---|---|
| **Free** | EUR 0 | Invoicing bundle (contacts, products, quotes/invoices, payments, EN 16931/Peppol), unlimited users, 1 GB, 100 docs/month, API 50 req/min, Peppol sends prepaid at cost+50%, community support, 90-day inactivity archival |
| **Team** | EUR 49 (59 monthly) | 5 full users (+EUR 9 up to 15), all MVP-loop apps, 10 GB, 14-day backups, EU region, email support |
| **Business** | EUR 249 (299 monthly) | 25 full users (+EUR 6), all apps, 5 entities, 100 GB, PITR 35 days, staging, WASM extensions, 1 bank connection per entity, 1-business-day support |
| **Scale** | from EUR 899 | 100+ users, dedicated DB, region pinning, CMK, native SAML/SCIM, advanced audit, 99.9% SLA, 24x5 |

| Self-host and services | Price [E] | Comparator |
|---|---|---|
| Community (AGPL) | EUR 0 | ERPNext free, Twenty self-host |
| Support Standard / Business | EUR 2,400 / 6,000 per year | Frappe Product Warranty (bug fixes) |
| Enterprise (`ee` + 24x5 support) | from EUR 12,000/yr; Plus from EUR 36,000 (24x7, 1 h P1, air-gap) | Metabase USD 20k, Grafana USD 25k, Twenty USD 50k |
| Compliance pack "Assurance" | EUR 19 (Team) / 39 (Business) per country per month: update-within-N-days SLA, filing connectors, capped warranty | Holded SII from EUR 30, Stripe Tax from EUR 80/month |
| Payroll pack | EUR 4/employee/month | Holded EUR 5, Xero UK GBP 1.50, Xero US USD 6 |
| Partner-hosting licence | EUR 6/tenant/month, EUR 300 minimum | Odoo pays 50% on Odoo.sh (capped EUR 150/month) |

**Price check** for a 15-user agency (yearly, ex-VAT): obop Business EUR 249/month; Odoo Standard list 373.50 (year-one promo 298.50); Odoo Custom list 673.50 (comparable features: API, multi-company, Studio); Zoho One flexible 15 x USD 90 = USD 1,350 (basis unstated on the page); BC Essentials USD 1,200; SaaS stack about USD 540 (20 s3.2 [I]). Odoo's own upgrade/renewal levers (7% uplift, 25% old-version fee, 300% breach fee, per-100-LOC fees, usage beacon; 03 s4.2) are **not** copied.

**Partner economics.** Odoo: EUR 590 / 2,950 fees, 10-20% commission on Enterprise, 82% on extra-module maintenance, 0% of services [V via 03]; commission is at most EUR 60-108 per user-year against about EUR 20k for one 200-hour project (20 s4 [I]). Obop: free listing after certification, Certified EUR 1,500/yr, Gold EUR 4,500/yr (25+ live tenants), 20% recurring commission on hosted subscriptions for 24 months (a Business tenant = EUR 598/yr against EUR 4-10k of services), 0% on services, no exclusivity or non-solicit penalty. Profitable implementation, not commission, recruits partners.

**Marketplace take rate (verified, not assumed).** Odoo Apps: 30% (vendor 70%), 25% on In-App Purchase, EUR 400 payout threshold, price parity [V https://apps.odoo.com/apps/faq]. Shopify: 0% on the first USD 1M of gross app revenue (counted cumulatively from 2025-01-01 across associated developer accounts, refunds not netted), then 15%, plus USD 19 one-time registration and 2.9% processing; developers with USD 20M+ prior-year App Store revenue or USD 100M+ company revenue pay 15% from the first dollar [V https://shopify.dev/docs/apps/launch/distribution/revenue-share]. Microsoft Marketplace: 3% [V https://learn.microsoft.com/en-us/partner-center/marketplace-offers/marketplace-commercial-transaction-capabilities-and-considerations]. Frappe and Atlassian rates: **[B]** (404). Decision: 0% on free extensions and on the first EUR 50k lifetime per developer, 15% above, processing at cost; revenue is negligible in three years, the goal is ecosystem.

---

## Part B - Unit economics and the 3-year model

### B1. Cost to serve per paying tenant (monthly, steady state)

| Component | Value | Basis |
|---|---|---|
| Compute/DB | about EUR 6.2 per tenant: one 250-tenant cell = 3 x CCX43 (275.99) + 3 x CPX42 (69.49) = EUR 1,036 x 1.5 for DR | Hetzner list, Germany/Finland prices effective 2026-06-15 [V https://docs.hetzner.com/general/infrastructure-and-availability/price-adjustment/]; cell sizing [I]; DR factor [E] |
| Storage, backups, egress, monitoring, email | EUR 1.6 | [E]; Hetzner object-storage price unreadable **[B]** |
| Free-tier tail | EUR 0.30 per free org | [E] |
| Peppol | 60 docs/tenant/month x EUR 0.20 (third-party) falling to 0.06 (own AP from about 150-200 tenants) | e-invoice.be EUR 0.25 Pro / 0.18 Enterprise [V https://www.e-invoice.be/pricing]; own AP: EUR 1,050 + 1,850/yr + 1,500 certification (AP only, 1-50 staff) or 1,800 + 2,750/yr + 2,500 with SMP (1-10 staff; 2,100 + 3,700 for 11-50), schedule applies to new members from 2025-07-01 [V https://peppol.org/join/fees/]; break-even recomputed: EUR 25k ops / (0.20 - 0.06) = about 180k docs/yr (about 250 tenants at 60 docs/month), so the earlier 110-150k docs/yr (150-200 tenants) needs ops nearer EUR 15-21k [I] |
| AI | EUR 1.0 (BYOK default) | Sonnet 5.5 USD 2/10 per M tokens (batch 1/5) [V https://platform.claude.com/docs/en/about-claude/pricing]; invoice page about USD 0.011 (batch 0.0055), Flash-Lite 0.0015 (16 s13 [V/I]) |
| SMS | metered, cost+50% | Twilio Germany USD 0.112 per segment [V https://www.twilio.com/en-us/sms/pricing/de]; use TOTP/passkeys, not SMS OTP |
| Payments on own billing | 2.0% of revenue | Stripe EEA cards 1.5% + EUR 0.25, premium 2.8%, SEPA DD EUR 0.35 [V https://stripe.com/en-de/pricing] |
| Bank feeds and payroll | Bank EUR 1.5 per connection [E, aggregator price **[B]**]; payroll via provider connectors | Finch Starter USD 65/month per connection, max 15 connections, employers under 500 staff (13 s8 [V]; https://www.tryfinch.com/pricing) |
| Support | 0.75 h per tenant/month = 1 FTE per 180 tenants (EUR 66k loaded) | [E] |
| SOC 2 / ISO / pen test | Type I EUR 50k at M18; Type II + ISO EUR 80k at M30; pen test EUR 40k/yr | Vanta: audit fee USD 10-50k, all-in USD 10-80k+ [S https://www.vanta.com/collection/soc-2/soc-2-audit-cost]; ISO price **[B]** |

### B2. Gross margin and payback by plan (steady state, own AP / third-party AP) [E]

| Plan | Price | Gross margin | Life (capped 60 mo) | LTV/CAC (assumed CAC) | Payback |
|---|---|---|---|---|---|
| Team | 49 | 64% / 58% | 50 mo at 2.0% churn | 3.9x (CAC 400) | 13 mo |
| Business | 249 | 78% / 75% | 60 mo at 1.1% | 7.8x (CAC 1,500) | 8 mo |
| Scale | 900 | 73% / 68% | 60 mo at 0.6% | 6.5x (CAC 6,000) | 9 mo |

Team only works with marginal CAC of about EUR 400 or less (self-serve, accountants); at a fully loaded EUR 1,600 its LTV/CAC is 1.0x. Xero reference (organic, ex-Melio; FY26 = year to 2026-03-31): CAC NZ$735 per gross add, 14.4 CAC-months, LTV/CAC 5.9x [V, FY26 deck]. In the model, year-1 gross margin is negative (fixed infrastructure on 10 tenants), 54% in Y2 and 79% in Y3 including support staff; target 75% steady state versus Xero's organic 89% and GitLab's 84% GAAP [V].

### B3. CAC by channel, churn and expansion benchmarks

Model-implied blended fully loaded CAC (base): about EUR 3.1k in Y2 and EUR 2.2k in Y3 (S&M salaries and programme spend ahead of adds; variable-only CAC is EUR 120 self-serve, 250 accountant, 400 partner [E]). Allocated by channel share of Y3 adds (60% / 30% / 10%) with relative weights 1.0 / 1.4 / 3.0 [E]: self-serve about EUR 1.7k, accountant about EUR 2.3k, partner about EUR 5.0k including 20% first-year commission. Base Y3 CAC payback is about 11 months on revenue (EUR 2.2k / EUR 200 ARPA) but about 14 months on gross-margin-adjusted revenue (EUR 2.2k / (EUR 200 x 79%)), and BVP measures payback against gross-margin-adjusted ARR, so the base case sits slightly outside the SMB target of under 12 months [S BVP https://www.bvp.com/atlas/scaling-to-100-million; I, recomputed].

| Comparable | What is public | Value | Source |
|---|---|---|---|
| **Xero** FY26 | Monthly MRR churn; LTV/CAC; gross margin | 1.14% headline, 0.81% underlying cohorted; LTV per customer NZ$4,304; organic GM 89% (group 83.9% after Melio); 4.92M customers; revenue NZ$2.75B; payments dilute margin | [V] https://brandfolder.xero.com/NE531UQB/as/bx8xwhj65zjm36s5fn4kc4gx/Xero_FY26_Annual_Results_Investor_Presentation |
| **GitLab** Q2 FY27 | Net retention, margin | DBNRR 117%; non-GAAP operating margin 15%; GAAP GM 84%; 1,571 customers over USD 100k; revenue USD 286.3M (+21%) | [V] https://s204.q4cdn.com/984476563/files/doc_financials/2027/q2/Gitlab-Exhibit-99-1-2026-09-1-FY27.pdf |
| **Odoo** | Derived | EUR 620M 2025 revenue (company-stated, Belga) / 170k+ paying customers (General Atlantic release, 2026-01-16) = EUR 3.6k; partner minimum retention of 70% (Silver) / 80% (Gold) over three years implies 89-93%/yr (the partner page states the 70%/80% minimums; the three-year window is from the agreement, 03) | [V/I] https://www.brusselstimes.com/belgium/2332306/odoo-announces-e10-billion-valuation-and-a-partial-price-increase ; https://www.generalatlantic.com/media-article/general-atlantic-announces-close-of-further-investment-into-odoo-acquiring-additional-stake-from-wallonie-entreprendre/ ; https://www.odoo.com/become-a-partner ; 03 s1, s6.1 |
| Private SaaS | NRR, GRR | Median NRR 102% at USD 25-50k ACV (2025 survey, 2023-24 cohorts); BVP cites HubSpot's 70-80% gross retention pre-IPO as its SMB example (typical range 85-90%) | [S] https://www.saas-capital.com/research/saas-retention-benchmarks-for-private-b2b-companies/ ; BVP above |
| Supabase, Grafana, n8n, Twenty, Frappe, Metabase, Akaunting, Invoice Ninja, Holded | **No public churn or NRR found.** Valuation/ARR only: Supabase USD 10B (June 2026 headline); Grafana USD 6B (2024), 10,000+ paying customers; n8n ARR over USD 40M (2025), about USD 5.2B at SAP stake (May 2026); Holded sold to Visma June 2021 for about EUR 120M reported on EUR 7.4M revenue; ERPNext about USD 3.9M ARR (third-party estimate, 20) | [S] https://techcrunch.com/tag/supabase/, https://en.wikipedia.org/wiki/N8n, https://en.wikipedia.org/wiki/Grafana, https://en.wikipedia.org/wiki/Holded |

Planning churn (monthly logo): Team 2.0%, Business 1.1%, Scale 0.6%; blend 1.6% [E]; expansion 8%/yr on survivors [E]. Conservative 2.4% / 3%, upside 1.2% / 12%.

### B4. EU salary benchmarks and planning costs

| Role | Benchmark | Planning gross / loaded [E] |
|---|---|---|
| Senior Rust backend | Talent.com Germany "Rust developer" average EUR 85k (8.5k records) [S] - its TypeScript page shows identical EUR 85k, so low quality. Levels.fyi software-engineer median total comp (pages re-read 2026-10-01; counts drift as users submit): Germany EUR 84.6k (3,180), Netherlands EUR 95.5k (1,483), Poland PLN 272k (1,642), Belgium EUR 57.8k (83; Odoo's median EUR 47.0k, tiny sample) [S] | EUR 95k / 126k (scarcity premium: Rust is used by 14.8% of developers, 20 s10) |
| Senior TypeScript / Vite+ | as above | EUR 80k / 106k |
| Accounting domain expert | Talent.com Germany Bilanzbuchhalter average EUR 77.8k (4.6k records; its Steuerfachangestellter page shows an implausible EUR 104k, discard) [S] | EUR 70k / 93k, part-time early |
| SRE / support engineer | Talent.com DevOps EUR 45.8k looks wrong [S, discard] | EUR 85k / 113k; support EUR 50k / 66k |
| Loader | Eurostat 2025: non-wage share 24.8% EU, 25.6% euro area, hourly cost EUR 34.9 EU, EUR 47.9 NL, EUR 13.6 RO [V https://ec.europa.eu/eurostat/statistics-explained/index.php?title=Hourly_labour_costs] | x1.33 |

Blended loaded cost **EUR 105k per FTE** plus EUR 9k tools and EUR 12k per hire [E]. Sources: https://www.levels.fyi/t/software-engineer/locations/germany (also /netherlands, /poland, /belgium), https://de.talent.com/salary?job=rust+entwickler.

### B5. Scenarios tied to 20's phases and model outputs

20's phases: 0 (M0-3) 4-6 people; 1 (M3-12) 8-12; 2 (M12-20) 14-20; 3 (M20-32) 25-40; 4 (M32+) 40+. Paid cloud starts M9; 20's targets: 10 paying orgs at M12, 150 at M24.

| | Conservative (lean, 25% capacity on services) | **Base (20's phases)** | Upside |
|---|---|---|---|
| Headcount M12 / M24 / M36 | 6 / 8 / 10 | 10 / 22 / 37 | 14 / 32 / 50 |
| ARPA at launch, logo churn, expansion | EUR 135, 2.4%, 3% | EUR 200, 1.6%, 8% | EUR 256, 1.2%, 12% |
| Gross adds vs base | 0.4x, 3 months later | 1.0x | 2.2x |
| Paying tenants M12 / M24 / M36 | 1 / 37 / 188 | 10 / 146 / 637 | 22 / 327 / 1,442 |
| Recurring ARR M36 | EUR 0.38M | EUR 2.16M | EUR 6.95M |
| Revenue Y1 / Y2 / Y3 | 0.10 / 0.23 / 0.47M (services 0.27M in Y3) | 0.01 / 0.20 / 1.28M | 0.02 / 0.59 / 4.06M |
| Gross margin Y2 / Y3 | 68% / 77% | 54% / 79% | 76% / 85% |
| EBITDA Y1 / Y2 / Y3 | -0.70 / -0.88 / -1.04M | -1.13 / -2.20 / -3.29M | -1.53 / -2.96 / -2.52M |
| Cumulative cash (after grants, sponsors) M12 / M24 / M36 | -0.66 / -1.46 / -2.43M | -1.06 / -3.11 / -6.23M (ungated) | -1.43 / -4.20 / -6.49M |
| **Peak funding need** | about EUR 4.7M by M72 and still falling (never breaks even within 72 months) | **EUR 10.8M at M62** | EUR 6.6M at M39 |
| **Break-even (monthly EBITDA >= 0)** | not reached | **M64** | M40 |
| Runway on EUR 3M / 5M / 8M | M44 / beyond 72 / beyond 72 | **M24 / M32 / M43** | M20 / M27 / not reached |

Base milestones: 150 paying tenants at M25, ARR EUR 1M at M29, own Peppol AP from about M31. **Gated base** (phase-3 hiring shifted +9 months, released only at ARR of EUR 1M): peak EUR 7.1M, break-even M62, cumulative -4.27M at M36 [E]; its ceiling is that revenue is modelled as independent of hiring, which flatters it.

Fact-check note [I]: an independent rough rebuild from the B6 parameters (my own guesses for unstated details such as post-M36 enterprise, partner and partner-hosted ramps) reproduced paying tenants at M24 (146) and within 3% at M36 (655 vs 637), cumulative cash within about EUR 0.3M at M12/M24/M36 (-1.06 / -3.15 / -6.47M vs -1.06 / -3.11 / -6.23M) and ARR EUR 1M at about M30 (model: M29), but M36 ARR 1.89M (model 2.16M, 13% lower) and no break-even before about M71 with peak near EUR 12.7M (model: M64, EUR 10.8M). The peak-funding and break-even month are therefore not independently reproducible from B6 alone (unverified); treat them as the author's model outputs.

**Funding path** [E]: base ungated = EUR 3M seed (to M24) + about EUR 9M Series A at M20-24, total near EUR 12M. Gated = EUR 3M seed + about EUR 5M Series A at M27-30 against EUR 1M+ ARR, total about EUR 8M. Conservative: EUR 2.5M lasts to M37; if paying tenants are below 60 at M24, cut to 6 FTE and bridge with services and partners.

### B6. Model parameters (base; all [E] unless marked)

Headcount 4 (M0), 6 (M3), 10 (M12), 17 (M20), 33 (M32), 37 (M36), 48 (M48), 55 (M60); EUR 105k loaded, 9k tools, 12k per hire; legal/foundation/CRA counsel EUR 120k Y1, 70k Y2, 60k Y3; GTM programmes EUR 3k/month, 15k from M10, 40k from M25; gross adds per month (self/accountant/partner): M9 2.5/0/0, M13 6/0/0, M19 12/5/1.5, M25 22/11/3, M31 34/17/6, then +2%/month; plan mix 55/37/8 (Team/Business/Scale) = EUR 191, plus 5% usage margin = ARPA EUR 200; free-to-paid 5%; self-host enterprise customers 2 (M24) to 8 (M36) at EUR 12k; compliance attach 5% (M18) to 25% at EUR 25; partners 2 / 8 / 40 at M18 / M24 / M36 at EUR 1.5k; partner-hosted 600 tenants at M36; grants EUR 60k / 120k / 120k and sponsors 10k / 25k / 50k per year; other costs as B1. Not modelled: annual-prepay cash (favourable), payments take-rate, marketplace, payroll packs, services (base).

### B7. Sensitivity (base; peak funding, first break-even month)

| Change | Peak funding | Break-even |
|---|---|---|
| Base | EUR 10.8M | M64 |
| ARPA -25% / +25% | 14.2M / 8.6M | M72 / M57 |
| Gross adds x0.7 / x1.3 | 14.7M / 8.4M | none in 72 / M56 |
| Logo churn 2.4% / 0.8% | 11.8M / 9.9M | M67 / M61 |
| Expansion 0% / 15% per year | 15.1M / 8.5M | none / M55 |
| Loaded cost +15% / -15% | 13.4M / 8.4M | M67 / M61 |
| Paid cloud launch delayed 6 months (hiring unchanged) | 16.5M | none |
| Hiring -25% | 6.6M | M57 |
| Phase-3 hiring +9 months | 7.1M | M62 |
| Peppol EUR 0.30/doc, no own AP | 11.4M | M65 |
| Infra x2 | 11.3M | M65 |
| No enterprise, compliance, partner revenue | 11.6M | M65 |

Reading: the levers that matter are **adds, ARPA/expansion and hiring pace**; infrastructure and Peppol cost are second order. A six-month slip in the paid-cloud launch (EUR +5.7M) costs more than a 25% ARPA miss (EUR +3.4M), so launch date and conversion are the first metrics to instrument.

---

## Part C - Funding and governance economics

### C1. Funding stack

| Source | Verified terms | Realistic yield for obop |
|---|---|---|
| **NLnet Restack** | First call opened 2026-09-03; first grants EUR 5,000-50,000; about EUR 7M to award through 2030; deadline **2026-11-03 12:00 CET**; open licences, a strong "European dimension" (usually applicants mostly from Horizon Europe-eligible countries); AI-related projects out of scope unless over 1M active human users; "not interested in AI-generated projects or proposals", but generative-AI help is allowed if disclosed with a prompt log, and purely AI-generated output cannot be paid for under a grant (policy v1.1, 2026-01-26); Horizon Europe grant 101299072 [V https://nlnet.nl/restack/ , https://nlnet.nl/propose/ , https://nlnet.nl/foundation/policies/generativeAI/] | 3-5 grants of EUR 30-50k for reusable crates (UBL/EN 16931, ledger engine, hash-chained audit, tenant-safe sqlx layer); EUR 60k / 120k / 120k per year in the model |
| **Sovereign Tech Fund** | Work must cost over EUR 50,000; OSI/FSF licence; prototypes not funded; user-facing applications "currently not" sought; about 6 months to contract (10 + 8 + 8 weeks of checks, scoping and legal); applicants should be recognised in, or endorsed by, the technology's community [V https://www.sovereign.tech/programs/fund]. Separate Resilience programme relaunched 2026-09-28 with CRA-compliance and memory-safety-transition services, applications open [V https://www.sovereign.tech/] | Possible only for foundational Rust crates; treat ERP itself as ineligible; check Resilience services for CRA help |
| **EU Open Source Strategy** (2026-06-03) | Names a Maintenance Instrument, Digital Commons EDIC, open-source-friendly tendering, startup legal/licensing help; **no amounts or dates** [V https://digital-strategy.ec.europa.eu/en/policies/open-source-strategy] | Monitor only |
| **EIC Accelerator** | Grant under EUR 2.5M (TRL 6-8, 24 months, once per Horizon Europe) plus EUR 1-10M equity; 2026 budget EUR 414M (Open) plus EUR 220M (Challenges); 2026 full-proposal batching dates 2 Sep (already passed) and 4 Nov 2026, and a full proposal needs a positive (GO) short application first, with feedback in about 4-6 weeks, so 4 Nov is realistic only for an applicant that already holds a GO; page never mentions open source, success rate not stated [V https://eic.ec.europa.eu/eic-funding-opportunities/eic-accelerator_en] | Company (not foundation) could apply; long shot, use as seed extension |
| Sponsors | Open Source Pledge: over USD 2,000 per employed developer per year; USD 7.4M raised to date, USD 3M+ last year [V https://opensourcepledge.com/] | EUR 10-50k per year; the whole pledge market is small |
| Seed / Series A | B5 funding path | Majority of funding |

Grants plus sponsors cover under 6% of base cumulative burn (about EUR 0.4M against EUR 6.6M through M36) [I].

### C2. Foundation/company split economics (17 s9)

The company (manufacturer, cloud, `ee`, partner programme) earns all revenue. The non-profit (steward) holds trademark, domains and the pledge; its budget (legal, CRA Art. 24 security policy, CI, maintainer stipends) is about EUR 150-300k/yr by Y3 [E], funded by the higher of 2% of company revenue or USD 2,000 per developer (Open Source Pledge yardstick) plus member, sponsor and grant income. The company holds a non-discriminatory trademark licence on a published policy; **no revenue share is demanded from hosts** (WP Engine lesson, 17 s5.2). The board can veto moving the core off OSI licences.

### C3. VC terms that threaten the licence pledge

| Term | Threat | Mitigation |
|---|---|---|
| Liquidation preference, participating preferred, exit pressure | Pushes a sale to an acquirer that closes future versions (precedent: HashiCorp moved to BUSL-1.1 on 2023-08-10 and was then sold to IBM for USD 6.4B, announced 2024-04-24, closed 2025-02-27; Cal.com closed its own source on 2026-04-14 with no acquirer found) [S, 17; https://en.wikipedia.org/wiki/HashiCorp] | Charter plus exit covenant: acquirer must honour the pledge or foundation gets licence-back |
| Protective provisions / board control over IP | Investors can veto or force licence change | Statutory purpose and foundation veto in articles before term sheet |
| Assignment-style CLA | Rug-pull (Elastic 2021, Redis 2024, MinIO archived 2026-04-25; note Elastic re-added AGPL in 2024-08 and Redis 8 added AGPL in 2025-05, 17) | CLA = licence grant with irrevocable OSI-forever pledge; FSFE-style reversion |
| Growth-at-all-costs | Boundary drift toward paywalls (Odoo 9 pivot while losing EUR 500k/month, 03 s2; a retrospective founder claim from a 2019 blog post, not independently verified) | Published closed paid list with ratchet |
| Anti-dilution / ratchets | Governance pressure in down rounds | Prefer EIC/public equity, mission-aligned angels, cap first round |

### C4. Dual-licence revenue

Possible because the CLA holder can sell a commercial exception to buyers who cannot accept AGPL (Google: AGPL "MUST NOT be used", 17 s2.2 [V]) and OEM/embedding licences. For an SMB ERP this is small: plan at most 5% of revenue, zero in base. The pledge forbids relicensing to non-OSI terms, so dual-licence means extra permissions, never closing.

### C5. Why fair-source was rejected

FSL/BUSL are not OSI licences and fail the gates that fund and sell us: NLnet (open licences), STF (OSI/FSF), openCoDE/ZenDiS buyers, and doubtful CRA "FOSS" treatment (17 s2.2, s11.1). FSL's "Competing Use" ban would block partner hosting, our channel. Akaunting's BSL cap (2 users / 1 company / 1,000 invoices) hurts adoption (05 s2, s8 [V]). Counter-evidence accepted: n8n's source-available licence reached ARR over USD 40M and about USD 5.2B [S]; the price is OSI status, which we need. A time-locked `ee` tier gives the anti-free-rider effect for the paid layer only.

### C6. The investor case

**Defensibility.** Ledger correctness with property-tested double-entry; country compliance packs plus certifications (Peppol AP, audited first country); trust (closed paid list, foundation veto); migration importers (data gravity); an extension ecosystem reachable without Rust; cost-to-serve. None is proven today.

**Why now.** Odoo raised Custom about 20% and meters all AI through IAP credits (03 s4.1, s3.3 [V/S]); e-invoicing forcing dates (Belgium 2026-01-01 with a three-month tolerance period [V https://einvoice.belgium.be/en]; France 2026-09-01 live [S vatcalc headline] / 2027-09-01 SME issuing (unverified today, primary pages blocked); Spain Veri*factu postponed to 2027 [S RSM, Grant Thornton Dec 2025]; Germany issuing 2027/2028 [S KPMG headline 2026-06-13]; Poland KSeF mandatory 2026-02-01 / 2026-04-01 and the smallest firms 2027-01-01 [V http://ksef.podatki.gov.pl/etapy-wdrozenia-ksef/]; 20 s6.1); CRA reporting live since 2026-09-11; EU Open Source Strategy 2026-06-03; Cal.com's April 2026 closure opened a trust gap; Vite+ 1.0 shipped 2026-09-28 (07); MCP/agents make typed APIs a buying criterion (16). Market: ERP about USD 78B in 2026 (Mordor, internally inconsistent [S]); 59% of EU small firms (10-49 staff) report no ERP (Eurostat 2025: 41.1% use ERP; firms under 10 staff are not covered; 20 s1.2 [V, by subtraction; dataset isoc_eb_iip, indicator E_ERP1]).

**Why Rust.** Single binary, memory safety and CRA posture, WASM sandboxing. The hosting-cost claim is unvalidated (design target 100+ concurrent users on 2 vCPU/4 GB versus Odoo's vendor formula of about 24-30, 22 s10 [E]) and infra is only about 6% of Y3 revenue, so Rust is a correctness and operations argument. The cost is hiring: Rust is used by 14.8% of developers (20).

**Why not fork ERPNext or Odoo.** The owner's constraint (Rust backend) rules out a code fork; both are Python. Economically: Odoo Community is LGPL and forkable (OCB exists), but its value sits in Enterprise accounting/payroll, which cannot be forked, and `account` alone is about 1.9 MB of model code (20 s9.1). The **Flectra lesson**: an Odoo Community fork since 2018 that copied the open-core commercial model has 252 GitLab stars (133 on its GitHub mirror) against Odoo's 54.8k GitHub stars and claims 13,000 teams on its home page; copying Odoo's model without its distribution yields a small vendor (03 s9 [V]). ERPNext is GPL-3.0 with 39.7k stars and no paid tier, so there is nothing to differentiate on.

**Exit routes** [S unless stated]:

| Route | Comparable |
|---|---|
| Strategic sale | Visma / Holded about EUR 120M reported, June 2021, about 16x revenue [I]; Oracle / NetSuite about USD 9.3B (2016); IBM / HashiCorp USD 6.4B (2025 close) |
| Growth equity / secondary | Odoo at EUR 5B (Nov 2024, on 2024 revenue of EUR 426M per Forbes = about 12x), EUR 7B (Jan 2026, General Atlantic), CEO's EUR 10B (24 Sep 2026, stated by Fabien Pinckaers, not a transaction) on EUR 620M 2025 revenue = 11-16x [I; https://www.brusselstimes.com/belgium/2332306/odoo-announces-e10-billion-valuation-and-a-partial-price-increase ; https://www.forbes.com/profile/fabien-pinckaers/] |
| Minority strategic | SAP about 1.3% of n8n at about USD 5.2B (May 2026) |
| Public comps | GitLab: DBNRR 117%, non-GAAP operating margin 15% [V] |
| Long-term independent | Odoo, Frappe, Tryton model; pledge makes acquirers pay a "trust discount" |

---

## Part D - Consolidated risk register

L = likelihood, I = impact (H/M/L). Thresholds marked [E] are my proposals.

| ID | Risk | Source | L | I | Early-warning trigger | Mitigation | Owner |
|---|---|---|---|---|---|---|---|
| T1 | Vite+ 1.0 is days old, "far from feature-complete" (remote caching, `vp release` unshipped; vite-ssg breakage) | 07 s1.1 | M | M | A 1.x minor changes a CLI flag or config key; plan issue #2405 slips a quarter | One config factory and test re-export; quarterly exit drill on plain Vite 8 + Vitest 5 + Oxlint/Oxfmt | Head of Frontend |
| T2 | VoidZero joined Cloudflare (announced 2026-06-04; Cloudflare's 2026-09-28 post describes the team as "part of Cloudflare" but does not say "closed"); licence flipped MIT to BUSL-1.1 (2025-10-27) and back to MIT (2026-02-27) [V git log of https://github.com/voidzero-dev/vite-plus; https://blog.cloudflare.com/voidzero-joins-cloudflare/ ; https://blog.cloudflare.com/voidzero-update/] | 07 R2 | M | H | LICENSE/NOTICE diff in CI; remote cache or `vp` features behind a paid service | Released MIT versions irrevocable; keep Vite/Rolldown/Vitest usable standalone; fork path | CTO |
| T3 | Rust compile times in a 100+ crate workspace; parallel front-end and Cranelift nightly-only | 06 s1.3, s4 | H | M | Cold CI build over 20 min or incremental check over 30 s [E] | Strict crate DAG (`-api`/`-impl`), sccache, hakari, feature-selected binary | Head of Platform |
| T4 | Dependency churn: sqlx 0.9 (repo moved to transact-rs, MSRV 1.94), axum still 0.x with breaking 0.9 queued, wasmtime monthly majors and WASI 0.3 breaking third-party extensions | 06 s3.2, s3.3, s3.12 | M | M | 90 days without a release; unpatched CVE; WIT-breaking wasmtime release | Seam crates (storage, `obop-http`); pin wasmtime LTS (v48); versioned WIT worlds; 12-month deprecation policy | CTO |
| T6 | DB-per-tenant at scale: 600-900 tables per tenant, about 190k relations per 250-tenant cluster; migrations become fleet jobs | 08 s5, 22 s10 | M | H | Migration fan-out over 1 h per cell; autovacuum lag; catalog bloat | Cells of 250, resumable migrations, `TenantRouter` seam to pool/schema; benchmark before 100 tenants | Head of Platform |
| T7 | LISTEN/NOTIFY commit lock is cluster-wide (Recall.ai outage pattern) | 22 s1.4 | M | H | Commit p99 rises with notifier count | Odoo-style bus table plus post-commit notify on a separate connection | Platform Lead |
| T8 | Performance edge unproven (design targets only) | 22 s10, 20 s8 | M | M | Harness shows under 3x Odoo | Publish harness before any claim | CTO |
| T9 | Runtime customisation without recompiling (metadata plus WASM) is the hardest architecture piece | 05 s7, 08 | M | H | A design partner needs recompilation for ordinary customisation | Declarative models, WASM, webhooks, UI builder from day one | CTO |
| T10 | SAML/XML-DSig: `samael` 0.0.22 pre-1.0, C-linked | 18 s2 | M | M | CVE in xmlsec or samael | Isolated process/IdP bridge; native SAML only in `ee` | Security Lead |
| T11 | Cross-tenant data leak | 18 s1, s12 | L | H | Any failing cross-tenant or IDOR matrix test; pentest finding | `TenantTx`, FORCE RLS, composite FKs, annual pentest | Security Lead |
| T12 | Supply-chain compromise (crates, npm registry via pnpm) | 18 s7 | M | H | Malicious release; scanner gaps on pnpm lockfile | cargo-deny/audit/vet; pnpm `minimumReleaseAge` (default 1 day since pnpm 11), `trustPolicy: no-downgrade`, `blockExoticSubdeps`, install scripts only via an `allowBuilds` allow-list, `vp install --frozen-lockfile` in CI | Security Lead |
| T13 | AI-driven vulnerability discovery raises attack rate (Cal.com's stated reason to close) | 17 s3, 20 s2.3 | H | M | Rising external vulnerability reports | Fuzzing, memory-safe Rust, bounty, 24h/72h runbook | Security Lead |
| L1 | AGPL plugin boundary: in-process Rust crates are derivative; exception untested in court | 17 s2.2, s11 | M | H | Plugin authors ask for legal cover; counsel cannot opine | Out-of-process/WASM plus s.7 exception plus Apache SDK; opinion letter before marketplace | Counsel |
| L2 | CLA versus trust: 05 says avoid a relicensing CLA, 17 says CLA with pledge | 05 Implications #2, 17 s4 | M | M | Contributors refuse; "rug pull" threads | Licence grant, irrevocable OSI pledge, foundation veto, FSFE-style reversion | Foundation Lead |
| L3 | CRA: manufacturer status, reporting live since 2026-09-11 [V https://digital-strategy.ec.europa.eu/en/policies/cyber-resilience-act], fines up to EUR 15M / 2.5% (Art. 64(2): Annex I and Arts. 13-14); steward exemption wording (Art. 64(10)(b)) ambiguous: it derogates from paragraphs 3 to 9 only, so it does not literally reach the EUR 15M tier in paragraph 2 [V https://www.cyberresilienceact.eu/the-cyber-resilience-act/] | 17 s7 | M | H | First exploited vulnerability; Commission guidance change | Foundation as steward, company as manufacturer; runbook; SBOM; counsel | Counsel / Security |
| L4 | CRA free security updates conflicts with support-tier gating | 17 s7.2 | L | H | Sales proposes "patch SLA" as paid feature | Sell response time, never availability of fixes | Head of Product |
| L5 | Trademark unregistered or inconsistently enforced (WP Engine pattern) | 17 s5.2 | M | M | Look-alike host before registration | EUIPO classes 9/35/42 pre-launch; objective policy | Counsel |
| L6 | Payroll, tax and e-invoice liability (employer liability, filing penalties) | 13 s6, 20 s11 | M | H | First incorrect-calculation claim | Capped warranty on paid packs, provider connectors, E&O insurance | Counsel / Compliance |
| L7 | Clean-room contamination (copied Odoo LGPL code/data, Enterprise material) | 03 s3, 20 s11 | M | H | PRs mirroring Odoo structure or comments | Provenance policy, clean-room reviewers, charts from official sources | Head of Engineering |
| L8 | Dependency licence conflicts, grant IP conditions, GDPR for HR data and AI Act duties | 17 s12, 13 s9, 16 s12 | L | M | cargo-deny failure; funder clause; DSAR volume | CI licence gate; grant conditions register; personal-data flags, DPA templates | Counsel |
| M1 | Odoo responds: reprices, opens accounting, bundles AI (+20% Custom, IAP AI, MCP) | 03 s4.1, 20 s2.1 | M | H | Odoo moves accounting into Community or cuts price; trial-to-paid falls | Compete on openness, portability, per-org pricing, not price alone | CEO |
| M2 | E-invoicing mandate dates slip (Poland penalties 2027 vs 2028 (a vatcalc headline of 2026-09-16 says deferred to 2028; primary source not read, unverified); Spain 2027) | 20 s6.1 | H | M | Official date changes | Do not time launch; free invoicing funnel is continuous | Head of Product |
| M3 | SaaS bundlers and AI-native ERPs close the gap (Xero + Melio + Anthropic, Holded/Visma, QuickBooks Free, Zoho One; Rillet USD 1B, Campfire, Doss) | 05 s4, 20 s2.3, A1 | H | M | Xero/Holded ship inventory and CRM depth | Integration depth, ownership, importers, open and self-hostable | Head of Product |
| M5 | Hosting competitor runs stock obop cheaper (AGPL does not stop unmodified hosting) | 17 s6 | H | M | Third-party hosts over 20% of installs [E] | Trademark, partner programme, ops advantage, compliance packs; do not relicense | CEO |
| M6 | Partner channel underdelivers; services dwarf commissions; Odoo also sells direct | 20 s4, 03 s6.1 | M | H | Fewer than 5 certified partners at M24 | Profitable implementation, 0% on services, co-selling | Head of Partnerships |
| M7 | Free tier costs more than it converts | B5 | M | M | Conversion under 2% or free cost over 10% of COGS [E] | Caps, archival, prepaid Peppol | Head of Growth |
| M8 | Accountant/integrator inertia slows adoption | 05 s7 | H | M | Fewer than 20 accountant pilots at M18 [E] | Free accountant seats, 20% rev share, co-branded onboarding | Head of Growth |
| M9 | Per-org price too low: ARPA shortfall | B7 | M | H | ARPA under EUR 150 at M24 | Price tests, Scale tier, usage add-ons | CFO |
| S1 | Scope explosion (Odoo has 642 addon directories on branch 20.0 [V gh api] and about 7,500 staff per the CEO, Sept 2026; its About page says 8,000+) | 20 s11, 05 | H | H | More than 7 apps planned before design partners are live | Cut lines; "no new app without a design partner"; phase gates | Head of Product |
| S2 | Accounting, localisation and payroll burden (`account` 1.87 MB; 229 `l10n_*`; payroll is the moat, 13 s6) | 20 s9.1, 13 s6 | H | H | First-country audit fails; ledger invariant bug; payroll packs requested early | One country, founding accountant, property tests; payroll engine plus connectors, packs after M30 | Head of Accounting |
| P1 | Rust hiring (14.8% of developers; salary premium) | 20 s10, B4 | H | M | Time-to-fill over 90 days [E] | Generalists plus training; remote EU; geographic mix | Head of Engineering |
| P2 | Accounting domain expert shortage or key-person loss | 20 s10 | M | H | No expert review on ledger PRs | Two experts, one part-time per country | Head of Accounting |
| P3 | Maintainer bus factor and burnout; thin contributor funnel (ERP contributors are Python/PHP domain people) | 05 s7 | M | H | Single owner for critical crates | Foundation stipends, pairing, docs-first | Head of Community |
| O1 | SOC 2 / ISO delays (Type II needs 9-12 months observation) | 18 s8.2 | M | M | Enterprise deal blocked | Evidence from M12, Type I M18 | Head of Compliance |
| O2 | Incident response capacity versus CRA 24 h / 72 h / 14 day clock | 17 s7 | M | H | Drill misses a deadline | Runbook, on-call, quarterly drills | Security Lead |
| O3 | Backup/restore or fleet-upgrade failure | 18 s9, 22 | L | H | Restore drill fails | Monthly automated restore drills; expand/contract migrations | SRE Lead |
| O4 | Peppol access-point dependence; no production Rust AS4 stack | 09 s5.4 | M | M | AP price +50% or outage | Two third-party APs; own AP at about 150-200 tenants | Head of Compliance |
| O5 | Bank-feed, AI and cloud provider cost or concentration (bank price unverified; Hetzner stock banners, 2026-06-15 adjustment) | 16 s13, 22 s10 | M | M | Unit cost up 30% [E]; capacity notice | Multi-provider, BYOK, budget caps, portable Kubernetes/Postgres | SRE Lead |
| O6 | Support load above 0.75 h/tenant/month | B1 | M | M | Over 1.0 h [E] | In-product guidance, docs, self-serve import | Head of Support |
| F1 | Funding gap: base trough EUR 10.8M; grant pool small | B5, C1 | H | H | ARR under EUR 1M at M30 | Stage gates, services bridge, EIC, seed extension | CFO |
| F2 | VC-driven relicensing or exit against the pledge | 17 s11.4 | M | H | Term sheet without licence covenant | Charter, foundation veto, investor covenant before priced round | CEO |
| F3 | Steward withdrawal (MinIO, Cal.com) or foundation capture (WordPress) | 17 s3, s9 | L | H | Board drift; unilateral trademark demands | Licence-back, escrow, independent seats | Foundation Lead |
| F4 | Revenue concentrated in a few Scale/Enterprise deals | B5 | M | M | Top 3 accounts over 25% of ARR [E] | Mix targets, partner-led volume | CFO |

**Top 8 to watch:** F1, S2, T9, M6, L1, F2, T6, M9.

---

## Skeptical-investor FAQ

1. **Odoo has 7,500 staff and EUR 620M revenue. Why would it not crush you?** It is tied to per-user pricing, a moving open-core line and yearly breaking majors (03). We attack what it cannot close quickly: open accounting/payroll and an ungated API. A price cut or Community accounting remains a real threat (M1).
2. **Open-source ERP forks stay small (Flectra, 252 GitLab stars). Why are you different?** Flectra copied the model, not the distribution; we do not fork and we add compliance, hosting and a verifiable pledge. Unproven: the base case needs 150 paying tenants by M25.
3. **Your model breaks even at M64 and needs EUR 10.8M. Why fund that?** The base is deliberately ungated on 20's phases; the gated plan needs EUR 7-8M and hires against ARR. Odoo itself lost EUR 500k/month before its 2015 pivot (03).
4. **At EUR 49-249 per org, are you Holded with less brand?** Holded Standard is EUR 59 for four users with HR, inventory and POS as paid add-ons; obop includes all apps, API and multi-company. Our base ARPA is about EUR 2.4k/yr; a 25% miss adds EUR 3.4M to funding.
5. **How do I know you will not rug-pull, and how do you monetise?** The paid list is closed and ratcheting, the CLA carries an irrevocable OSI pledge, and the foundation has a veto. We monetise hosting, support, compliance packs, partner hosting and `ee` identity/assurance.
6. **Who writes extensions in Rust?** Nobody has to: declarative models, WASM, webhooks and a UI builder (20 s11).
7. **What stops a cheaper host?** Nothing in AGPL. Trademark, compliance SLAs, partner economics and fleet tooling do; we will not relicense (17 s11.4).
8. **Payroll and e-invoicing liability?** Warranty is capped and limited to paid packs; payroll runs through provider connectors first (13).
9. **Will AI agents commoditise ERP?** They raise the value of a typed, ungated API and audit trail; Odoo 20 meters AI, we do not (16).
10. **Vite+ is a days-old 1.0 owned by a team now inside Cloudflare.** It is the owner's constraint; isolating it behind one config factory and a quarterly exit drill bounds the exposure (07).
11. **Who buys an AGPL business with a foundation veto?** Acquirers that value trust and recurring revenue (Visma/Holded, SAP/n8n pattern); the pledge lowers the price of a closing-the-code exit.

## One-page thesis

European SMEs run on subscriptions and spreadsheets; 59% of small (10-49 staff) firms report no ERP. Odoo proved the model (EUR 620M revenue) but withholds accounting, payroll and the API behind an enterprise paywall and raised its top plan about 20% in September. E-invoicing mandates (Belgium, France, Germany, Spain, Poland), the Cyber Resilience Act, the EU Open Source Strategy and the 2026 licence reversals (Cal.com, MinIO) open a window for a complete, verifiably open alternative.

Obop ships every first-party app under AGPL-3.0 with a permissive SDK, one shared data model and a Rust core. The company sells hosting, per-organisation plans (Free, EUR 49, EUR 249, from EUR 899), support and SLAs, maintained compliance packs and a narrow identity/assurance layer, through self-serve, accountants and certified partners. A foundation holds the trademark and a veto over any move away from an OSI licence.

The economics are long: on 20's phases the base case needs about EUR 11M, reaches EUR 2.2M ARR at month 36 and breaks even near month 64; gating hiring to EUR 1M ARR cuts the need to about EUR 7M. Grants validate but do not fund. The bet: trust, compliance and ownership, not price or language, win a segment that the incumbent and SaaS bundlers serve poorly, and a governed open platform compounds like Odoo did, without Odoo's lock-in.

---

## Confidence & gaps

- **High [V]:** all vendor prices listed in A1 (except NetSuite and Zoho's full price), Odoo Apps 30% / Shopify 15% / Microsoft 3%, Peppol fees, Twilio and Stripe prices, Xero FY26 churn/CAC/LTV, GitLab DBNRR 117%, NLnet Restack terms and deadline, STF terms, EIC terms.
- **Medium [S]:** Levels.fyi salaries (self-reported, small samples for Belgium), Talent.com (two pages clearly aggregated or wrong), Vanta SOC 2 range (vendor), valuations of Supabase, Grafana, n8n, Holded (press, Wikipedia, headline-only), SaaS Capital/BVP benchmarks (portfolio bias).
- **Blocked or unverified:** NetSuite pricing (403); Zoho All-Employee price; Frappe and Atlassian marketplace commissions (404); ISO 27001 audit cost; bank-feed aggregator pricing; Hetzner object-storage price; SaaS Capital charts; Carta (403); no public churn or NRR for Odoo, Frappe, Twenty, Supabase, Grafana, Metabase, n8n, Akaunting, Invoice Ninja or Holded; Melio price.
- **Model limits:** prices, mixes, adds, churn and costs are [E]; revenue is independent of hiring; no annual-prepay cash, no tax. Treat outputs as sensitivities, not forecasts.
- **Resolved conflict:** 05 (avoid relicensing CLA) and 17 (CLA with pledge) are reconciled as a licence-grant CLA with an irrevocable OSI pledge; counsel must draft (17 s12).

## Implications for obop

**MUST**
1. Publish before the first public commit: the closed paid list (A2), the free-forever list, per-organisation pricing with free portal/accountant/light users, and the ratchet (paid code opens after 24 months).
2. Never gate security fixes, exports, the API, multi-company, ledger integrity or the existence of an audit log; keep the CRA update rule and SBOM in every edition.
3. CLA as licence grant with an irrevocable OSI-forever pledge, foundation veto and an investor covenant **before** any priced round (C3).
4. Stage-gate hiring: do not pass 17 FTE until recurring ARR is EUR 1M (base: about M29); if paying tenants are under 60 at M24, cut to 6 FTE and bridge with services and partners.
5. Raise about EUR 3M seed for roughly 24 months and plan a Series A of EUR 5-9M against EUR 1M+ ARR; total EUR 8M gated, EUR 12M ungated.
6. Instrument from the first paid tenant: free-to-paid conversion, Peppol documents and support hours per tenant, ARPA, logo churn; launch paid cloud at M9 as planned (a six-month slip costs about EUR 5.7M more funding).
7. Keep Peppol on a third-party access point; move to an own AP at about 150-200 paying tenants (B1 recomputes break-even nearer 250 at EUR 25k ops; decide on measured volume); take SOC 2 evidence from M12 and budget EUR 50k + 80k + 40k/yr.
8. File NLnet Restack by 2026-11-03 for discrete crates (disclose any generative-AI use with a prompt log; AI-related scope is excluded); count grants under 6% of burn.
9. Benchmark before claiming Rust performance (T8); publish the harness.

**SHOULD**
10. Launch the accountant programme first (free seats, 20% first-year share), then certified partners with the EUR 6/tenant hosting licence; make implementation profitable.
11. Register the trademark (EUIPO 9/35/42) and form the foundation in year 1; fund it with the higher of 2% of revenue or USD 2,000 per developer.
12. Treat Team as a funnel (marginal CAC up to EUR 400) and price-test Business between EUR 199 and EUR 299.
13. Hire a geographic mix (Benelux, Poland, Portugal, Germany), two accounting experts and plan EUR 105k average loaded cost.
14. Build a marketplace at 0% on free apps and the first EUR 50k, 15% above, after the legal plugin opinion.

**COULD**
15. Add payments take-rate (Stripe Connect from 0.25%) as upside; Xero shows ARPC rises but gross margin falls with payments.
16. Sell OEM/commercial-exception licences (cap 5% of revenue).
17. Apply to EIC Accelerator as a seed extension (company, not foundation).
18. Offer nonprofits a discounted tier to seed references.

## Verification log

Fact-check pass of 2026-10-01 (independent re-fetch of primary sources: raw HTML of the Odoo, Holded, Invoice Ninja and NLnet pages, the Xero FY26 and GitLab Q2 FY27 PDFs text-extracted locally, the Hetzner price-adjustment page, an OJ-mirrored CRA text, the Eurostat API, `gh` and crates.io APIs, Belga/Brussels Times, General Atlantic, Cloudflare and vendor pages). No search engine was available; discovery was by direct URL and Google News RSS headlines (headline-only evidence is marked as such).

**Confirmed as stated: about 160 atomic claims**, including all 11 flagged claims except as corrected below: Odoo Online prices (yearly 19.90/24.90 and 35.90/44.90, monthly 24.90/31.10 and 44.90/56.90, Light User 7.90, Custom-only Odoo.sh/on-prem, Studio, multi-company, External API, 12-month discount on initial users); Odoo Apps 30% / 25% IAP / EUR 400 / price parity; Microsoft Marketplace 3%; Xero FY26 (1.14% headline and 0.81% cohorted churn, CAC NZ$735, 14.4 CAC months, LTV/CAC 5.9x, LTV NZ$4,304, organic GM 89%, group 83.9%, 4.92M customers, NZ$2.75B revenue); GitLab Q2 FY27 (USD 286.3M, +21%, DBNRR 117%, non-GAAP operating margin 15%, GAAP GM 84%, 1,571 customers over USD 100k, quarter ended 2026-07-31); NLnet Restack (EUR 5,000-50,000, about EUR 7M to 2030, 2026-11-03 12:00 CET, grant 101299072); Sovereign Tech Fund terms; EIC Accelerator amounts, budget and cut-offs; e-invoice.be EUR 0.25 / 0.18; OpenPeppol AP-only fees; Twenty pricing and gating; Holded plans, users 2/4/7 and add-ons; Stripe EEA fees and Twilio Germany USD 0.112; the A1 vendor prices (Frappe, Xero UK/US, QuickBooks, BC, Teamleader, Zoho USD 90, Metabase, Supabase, n8n, GitLab, Grafana, Akaunting, Invoice Ninja); Odoo EUR 620M revenue, 170k+ customers, EUR 5B / 7B / 10B valuations, about 7,500 staff, partner fees EUR 590 / 2,950; n8n, Holded/Visma, Grafana, Supabase and Rillet figures; Open Source Pledge; EU Open Source Strategy (2026-06-03); Vite+ 1.0.0 (2026-09-28) and its LICENSE history; VoidZero/Cloudflare (2026-06-04); sqlx 0.9.0 (MSRV 1.94, repo at transact-rs), samael 0.0.22, wasmtime v48 LTS, axum 0.8.x; Rust 14.8% (SO 2025); Eurostat labour-cost figures and 41.1% small-firm ERP use; Hetzner CCX43 275.99 / CPX42 69.49 (new prices from 2026-06-15); Sonnet 5.5 USD 2/10; Finch USD 65; Google AGPL "MUST NOT"; Mordor USD 78.15B; Odoo 642 addon directories and 229 `l10n_*` modules (branch 20.0); ERPNext 39.7k stars GPL-3.0; CRA dates, Art. 24, Art. 64 tiers and the 24 h / 72 h / 14-day clock; HashiCorp/IBM, Oracle/NetSuite and Cal.com dates; MinIO archived; Belgium and Poland e-invoicing dates; internal arithmetic of B2, B4 and the 15-user price check.

**Corrections (was / now / source)**

1. NLnet, C1: "generative-AI proposals not accepted" / NLnet is "not interested in AI-generated projects or proposals", but generative-AI use is allowed with disclosure and a prompt log, purely AI-generated output cannot be paid for, and AI-related projects are out of scope unless over 1M active human users; first call opened 2026-09-03 / https://nlnet.nl/foundation/policies/generativeAI/ , https://nlnet.nl/propose/ , https://nlnet.nl/restack/
2. Shopify, A3: "first USD 1M lifetime gross" / counted cumulatively from 2025-01-01 across associated accounts; 15% from the first dollar for developers with USD 20M+ prior-year App Store revenue or USD 100M+ company revenue / https://shopify.dev/docs/apps/launch/distribution/revenue-share
3. OpenPeppol, B1: "EUR 1,800 + 2,750/yr + 2,500 with SMP" (unqualified) / applies to 1-10 staff (11-50 staff: 2,100 + 3,700); schedule is for new members from 2025-07-01 / https://peppol.org/join/fees/
4. Own-AP break-even, B1: "about 110-150k docs/yr" / about 180k docs/yr (about 250 tenants) at EUR 25k ops and EUR 0.14 saving per document; 150-200 tenants needs ops of EUR 15-21k / recomputed [I]
5. CAC payback, B3: "about 11-12 months, inside the SMB target of under 12" / about 11 months on revenue but about 14 months gross-margin-adjusted, which is BVP's definition, so slightly outside the target / https://www.bvp.com/atlas/scaling-to-100-million
6. Infra x2, Bottom line 4: "EUR 0.4M" / EUR 0.4-0.5M (B7 shows 10.8M to 11.3M) / B7
7. Levels.fyi, B4: Germany "EUR 84.8k (3,225)" and Netherlands "EUR 95.4k" / EUR 84.6k (3,180) and EUR 95.5k on the 2026-10-01 pages / https://www.levels.fyi/t/software-engineer/locations/germany , /netherlands
8. Flectra, C6 and FAQ 2: "252 stars against Odoo's 54.8k" / 252 GitLab stars (133 on the GitHub mirror) against Odoo's 54.8k GitHub stars / GitLab and GitHub APIs
9. Odoo valuation multiples, C6: "8-16x" / 11-16x (EUR 5B in Nov 2024 was on 2024 revenue of EUR 426M, about 12x; EUR 7B and 10B on EUR 620M) / https://www.forbes.com/profile/fabien-pinckaers/ , Brussels Times/Belga
10. Precedent in C3: "acquirer that closes future versions (HashiCorp to IBM, Cal.com 2026)" / HashiCorp relicensed itself in 2023 before the IBM sale (closed 2025-02-27); Cal.com closed its own source on 2026-04-14 and no acquirer was found / https://en.wikipedia.org/wiki/HashiCorp , https://cal.com/blog/cal-com-goes-closed-source-why
11. BVP retention, B3: "SMB gross retention 70-80%" as a benchmark / BVP cites HubSpot's 70-80% gross retention pre-IPO as an example; typical range 85-90% / BVP page above
12. VoidZero, T2: "closing unconfirmed" / Cloudflare's 2026-09-28 post describes VoidZero as "part of Cloudflare" (still no explicit "closed") / https://blog.cloudflare.com/voidzero-update/
13. CRA Art. 64(10), L3: "wording ambiguous" / specifically, 64(10)(b) derogates from paragraphs 3 to 9 only, so the EUR 15M / 2.5% tier in paragraph 2 is not literally covered / https://www.cyberresilienceact.eu/the-cyber-resilience-act/
14. Odoo Custom +20%, A1: "[S via 03]" / verified, Standard unchanged at 19.90, Light User needs at least one full user / https://www.brusselstimes.com/belgium/2332306/odoo-announces-e10-billion-valuation-and-a-partial-price-increase
15. Small-firm ERP, C6 and thesis: "59% of small firms report no ERP" / 59% of firms with 10-49 staff (41.1% use ERP in 2025); micro firms not covered / Eurostat dataset isoc_eb_iip, indicator E_ERP1
16. Invoice Ninja, A1: white-label USD 40/yr cited to the pricing page / it appears on the 2026-01-01 price-update post, not on the pricing page / https://invoiceninja.com/pricing-update-january-1-2026/
17. Elastic / Redis, C3: listed as rug-pulls without the reversal / Elastic re-added AGPL (2024-08) and Redis 8 added AGPL (2025-05) per 17
18. EIC, C1 (addition): the 2 Sep 2026 cut-off has passed; a GO on the short application is required before a full proposal; grant is once per Horizon Europe, TRL 6-8 / https://eic.ec.europa.eu/eic-funding-opportunities/eic-accelerator_en
19. Sovereign Tech, C1 (addition): Resilience programme relaunched 2026-09-28 with CRA-compliance and memory-safety services / https://www.sovereign.tech/

**Unresolved or unverified**

- Model outputs (B5-B7): EUR 10.8M peak at M62, break-even M64, 637 tenants and EUR 2.16M ARR at M36, gated EUR 7.1M. A rough independent rebuild from B6 matched tenants and cumulative cash through M36 (about 3-4%) but gave M36 ARR 13% lower and peak near EUR 12.7M with break-even near M71; B6 omits post-M36 ramps, so these outputs cannot be reproduced from the file alone (unverified).
- France SME e-invoicing issuing on 2027-09-01, Poland penalty start (2027 vs a 2026-09-16 headline saying 2028), Spain Crea y Crece B2B date (a 2026-09-10 headline says about Oct 2027): primary sources blocked or silent.
- Odoo headcount: about 7,500 (CEO, Belga) versus 8,000+ (About page); the Nov 2024 EUR 5B and Jan 2026 EUR 7B valuations are headline-level evidence; the CEO's EUR 10B is self-stated.
- Odoo partner 82% maintenance share and 50% Odoo.sh commission (only the EUR 150/month cap and "up to 20%" commission confirmed on the partner page); Odoo 9 pivot "EUR 500k/month loss" (2019 founder retrospective).
- Frappe server tiers tied to Hetzner; Zoho One All-Employee price; NetSuite pricing (403); Frappe and Atlassian marketplace commissions; ISO 27001 audit cost; bank-feed aggregator price; Hetzner object-storage price; Melio price; Flash-Lite and per-invoice AI cost figures (16 s13); absence of public churn/NRR for the listed vendors; EUIPO class numbers; WP Engine case status.

- Owner-constraint pass (2026-10-01): the owner fixed `vp` (Vite+) plus pnpm as the only JavaScript tooling (no npm, npx, yarn or bun). Risk T12 now reads "npm registry via pnpm" and lists the verified pnpm controls instead of the bare "blocked install scripts": `minimumReleaseAge` (default 1440 minutes since pnpm 11.0.0, 2026-04-28; introduced in 10.16.0 with default 0), `trustPolicy: no-downgrade` (default off, pnpm 10.21.0+), `blockExoticSubdeps` (default true since pnpm 11), dependency build scripts only through an `allowBuilds` allow-list (pnpm 11 replaced `onlyBuiltDependencies`; `strictDepBuilds` default true), and `vp install --frozen-lockfile` in CI (viteplus.dev "Package Management"). Sources: pnpm.io "Mitigating supply chain attacks" and settings pages, pnpm v11.0.0 release notes. The "2FA" hit in the Twenty pricing row is a product feature, and the "Provenance policy" in L7 is clean-room provenance; neither concerns npm and both are unchanged. The T1 and T2 Vite+ rows are unchanged (they name `vp` only).
