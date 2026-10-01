# 25 - Localization Program: First-Market Decision and Country-Pack Operating Model

Research date: 2026-10-01. Workstream 25 (localization). Builds on 03, 09, 12, 13, 17, 19, 20.

## Scope & method

Web search was unavailable (search engines return bot challenges; not bypassed), so everything comes from direct fetches: Eurostat and US Census APIs, EC factsheets, BOE and gesetze-im-internet.de legal texts, OpenPeppol, KSeF, bas.se, vendor pricing pages, crates.io and the GitHub API. Several official sites were blocked or 404 (impots.gouv.fr, economie.gouv.fr, BMF, bycyb.fr, SAP). Tags: **[V]** verified today on a live primary or first-hand source ("sec." = secondary, e.g. Wikipedia or VATupdate), **[I]** inference or my arithmetic from verified inputs, **[G]** guess or unverified. Person-month (PM) figures are [G] anchored on measured proxies. Regulatory facts are "as of 2026-10-01"; all legal points need a local accountant or counsel.

---

## Part A. First-market decision

### A1. SME base and ERP gap (Eurostat, per 20 s1.2)

Enterprise counts: Eurostat `sbs_sc_ovw`, 2024 (updated 2026-09-30) [V]; ERP use: `isoc_eb_iip` indicator E_ERP1, 2025 (updated 2026-02-27) [V] (https://ec.europa.eu/eurostat/databrowser/view/isoc_eb_iip). "Without ERP" = firms x (1 - ERP share) per size band [I].

| Country | Firms 10-249 | ERP use 10-49 / 50-249 | 10-249 without ERP [I] | Micro (<10) firms |
|---|---|---|---|---|
| **DE** | 480,442 | 37.5% / 67.7% | **~281k** | 2.74M |
| IT | 236,333 | 46.0% / 71.9% | ~121k | 4.41M |
| FR | 192,646 | 48.9% / 80.1% | ~90k | 5.25M |
| ES | 186,085 | 55.8% / 80.0% | ~77k | 3.33M |
| PL | 108,625 | 31.9% / 68.7% | ~68k | 2.71M |
| Nordics (SE/DK/NO/FI) | 121,201 | DK 61.2, FI 52.3, NO 35.8, SE 21.9* / 85.3, 80.6, 66.8, 49.8 | ~67k | 2.08M |
| NL | 64,485 | 45.9% / 76.4% | ~32k | 2.36M |
| AT | 46,759 | 30.9% / 66.4% | ~30k | 0.58M |
| CH | 54,180 | no Eurostat ERP data | n/a | 0.11M (coverage differs) |
| BE | 34,087 | 56.8% / 84.4% | ~13k | 0.90M |
| UK | 258,520 (DBT, start 2025) [V] https://www.gov.uk/government/statistics/business-population-estimates-2025 | last Eurostat 2019: 19.1% / 43.5% (stale) | ~200k at 2019 rates [I, stale] | 4.27M no-employee firms |
| US | ~1.28M firms with 10-199 employees (Census SUSB 2022) [V] https://www2.census.gov/programs-surveys/susb/tables/2022/us_state_naics_detailedsizes_2022.txt | not measured | not measured | n/a |

EU27: 41.1% (10-49) and 69.9% (50-249) use ERP, matching 20 s1.2. *Sweden's 2025 value (21.9%) is a break from 54.3% in 2023; treat it as a data-quality flag [I]. Germany has the largest absolute gap, more than twice any other EU country, and its small-firm ERP use (37.5%) is below the EU average.

### A2. Mandates re-verified (as of 2026-10-01)

EC country factsheets ("eInvoicing in <Country>", e.g. https://ec.europa.eu/digital-building-blocks/sites/spaces/DIGITAL/pages/467108886/eInvoicing+in+Germany) carry "last updated 14 Aug 2025", edited 2026-03-05/06 (Austria page 2026-03-18); later changes come from doc 09 (VATupdate chronology, sec.). National official pages were mostly unreachable (impots.gouv.fr and ksef.podatki.gov.pl were reachable on the 2026-10-01 re-check).

| Country | Obligation and dates | Tag |
|---|---|---|
| **DE** | Receive EN 16931 e-invoices since 1 Jan 2025; issue from **1 Jan 2027 if turnover > EUR 800k**, all businesses **1 Jan 2028**; no clearance platform, no real-time reporting; XRechnung (KoSIT, also German Peppol Authority), ZUGFeRD, Peppol BIS. Section 14 UStG defines the e-invoice as structured EN 16931-conformant format (https://www.gesetze-im-internet.de/ustg_1980/__14.html) | [V] |
| **FR** | Reception for all **1 Sep 2026**; large and mid-size issue and e-report 1 Sep 2026; **SMEs and micro 1 Sep 2027**; private "plateformes agreees" only (EC: "nearly 100" accredited, PPF reduced to directory, DGFiP = Peppol France authority); national CIUS. Dates confirmed by DGFiP: reform "effective since 1 Sep 2026", reception for all and issue/e-reporting for large and ETI on 1 Sep 2026, PME and micro on 1 Sep 2027 [V https://www.impots.gouv.fr/professionnel/questions/partir-de-quand-suis-je-concerne-par-la-reforme-de-la-facturation] (the EC page itself gives only the 1 Sep 2027 e-reporting date for the smallest firms) | [V] EC + DGFiP; later decree and tolerance details [sec.] |
| **BE** | B2B e-invoicing mandatory **1 Jan 2026**, Peppol BIS 3.0, no national CIUS; near-real-time e-reporting planned 2028; EC page lists no penalties | [V] |
| **PL** | KSeF FA(3): 1 Feb 2026 (> PLN 200M, EC) and **1 Apr 2026 all** except taxpayers invoicing up to PLN 10,000 per month, whose KSeF 2.0 obligation starts **1 Jan 2027** [V http://ksef.podatki.gov.pl/etapy-wdrozenia-ksef/]; official TEST/DEMO/PROD environments | [V] (doc 09, KSeF repo) |
| **ES** | Veri*Factu for corporate-tax payers before **1 Jan 2027**, others before **1 Jul 2027** (RD 1007/2023 Final Provision 4 as amended) [V https://www.boe.es/buscar/act.php?id=BOE-A-2023-24840]; SII > EUR 6M [V]; B2B e-invoicing has no fixed date: RD 238/2026 of 25 Mar 2026 (BOE 2026-03-31, in force 2026-04-20) applies 12 months (prior-year turnover > EUR 8M) or 24 months (others) after a ministerial order on the public solution enters into force (DF 4) [V https://www.boe.es/buscar/act.php?id=BOE-A-2026-7295]; that order was a draft under consultation 17 Apr-8 May 2026 with planned effect 1 Oct 2026, implying 1 Oct 2027 / 1 Oct 2028 [sec., VATupdate 2026-04-27; final order not checked (unverified)] | [V] / [sec.] |
| **IT** | SDI/FatturaPA since 2019 for everyone incl. flat-rate taxpayers from Jan 2024; EU derogation to 31 Dec 2027 | [V] |
| **NL** | No B2B mandate [V EC]; cabinet plans B2B e-invoicing in 2030 and e-reporting in 2031 (VATupdate headlines Sep 2026, sec.; the exact day "1 Jul 2030" is unverified) | [V] / [sec.] |
| Nordics | SE: no B2B mandate, Peppol BIS as-is [V]. FI: no B2B mandate, preparation by 2028 [V]. DK: Bookkeeping Act requires certified digital bookkeeping systems able to send/receive via Peppol/NemHandel and produce SAF-T on request (EC: Jan/Jul 2026, the July date "expected"; the "deferred to 1 Mar 2027" chronology could not be confirmed (unverified)). NO: EC page says no B2B mandate (Ministry assessing); the EHF B2B planning date 1 Jan 2027 and digital bookkeeping by 2030 come only from VATupdate headlines (sec., Mar-Sep 2026); SAF-T Financial since 1 Jan 2020 (sec., not on the EC page) | [V] EC / [sec.] |
| AT / CH | AT: no B2B mandate; since 18 Apr 2020 all suppliers must send structured e-invoices to federal entities (ebInterface 4.3/5/6 or UBL accepted) [V EC]. CH: not checked | [V] / [G] |
| UK | Peppol named core interoperability network 24 Jun 2026, mandatory e-invoicing expected 2029 (OpenPeppol news of 2026-06-24 and 2026-03-24) [V https://peppol.org/news/]; HMRC VAT API v1.0 (beta label) updated 2026-08-05, fraud-prevention header data must be submitted [V] https://developer.service.hmrc.gov.uk/api-documentation/docs/api/service/vat-api/1.0 | mixed |
| US | No federal B2B mandate | [sec.] |

Reading: the only mandates a new entrant can still catch with a 2027 pack are **DE (2028), FR (Sep 2027 SMEs, long tail), ES (2027, but see liability)**. PL and BE are already live and become table stakes rather than triggers [I].

### A3. Filing formats, charts, accountant interoperability

| Country | VAT return / audit file | Chart: owner and licence | Accountant surface |
|---|---|---|---|
| DE | UStVA via ELSTER, EC sales list; DATEV-Format (EXTF) export, GoBD export. In Odoo these are `l10n_de_reports` and `l10n_de_reports_elster` [V docs https://www.odoo.com/documentation/19.0/applications/finance/fiscal_localizations/germany.html] | SKR03/04: DATEV publishes them; licence terms not found [G]. Odoo ships them (557 KB CSV in `l10n_de`) and ERPNext ships `de_kontenplan_SKR03/04.json` [V], so community practice tolerates it, which is not legal clearance | **DATEV**: 40,296 members (2025), EUR 1.65B revenue (2025) (en.wikipedia, sec.; DATEV's own site says "over 40,000 members" and EUR 1.51B for 2024), ~2.8M companies' bookkeeping (de.wikipedia, sec.) https://de.wikipedia.org/wiki/DATEV; Bundeskartellamt 2019 forced access to the power-of-attorney database (Vollmachtsdatenbank); partner/interface certification is criticised |
| FR | CA3 return; **FEC** mandatory since 1 Jan 2014 (not OECD SAF-T) [V sec. https://en.wikipedia.org/wiki/SAF-T] | PCG (ANC regulation): official text; ERPNext ships `fr_plan2025_comptable_general_*` [V]; reuse licence [G] | Pennylane public Company API v1.0 and v2.0 (https://pennylane.readme.io/) [V]; a separate "Firm API" is not confirmed by that page (unverified) |
| BE | Intervat XML VAT return, client listing, EC sales list, CODA [V Odoo docs] | PCMN/MAR, regulation; in Odoo and ERPNext [V] | Exact Online, WinBooks [G] |
| NL | BTW via Digipoort/SBR [G]; Auditfile Financieel **XAF 3.2** [V, cited by RGS site] | **RGS**: under SBR governance since 2015, adopted by Twinfield, SnelStart, Visionplanner, Nextens; RGS 3.9 alpha; licence text not found [V/G] https://www.referentiegrootboekschema.nl/ | Exact Online REST API v1 publicly documented (division model, page size 60, sync endpoints) [V] https://start.exactonline.nl/docs/HlpRestAPIResources.aspx |
| ES | Modelos 303/390/347/349/111/115... ; SII; OCA has 18 `l10n_es_aeat_*` modules (33 modules in `l10n-spain` on 19.0) [V] | PGC (royal decree) [G] | Holded, A3 [G] |
| PL | JPK family since 2016 [V sec.], JPK_V7 VAT files [G]; KSeF | No statutory chart | Comarch, enova [G] |
| IT | FatturaPA, LIPE [G] | No statutory chart | TeamSystem, Zucchetti [G] |
| SE | **SIE** (open format; only SIE-Gruppen members' software can be "approved") [V https://sie.se/] | **BAS: proprietary.** Free list of numbers/names to view; machine-readable JSON **SEK 4,000 ex VAT**; T&C 2026-04-14: licensee may integrate into its accounting system and sublicense to customers (cl. 4.3) but may not "copy, spread" (Swedish "obehörigen kopiera, sprida", i.e. without authorisation) or sublicense beyond 4.3 (cl. 4.5); the right is limited to the licensee and not extended to group companies (4.4); when a new annual version is released, the prior version may not be used for new products or sales where the new version applies (cl. 3.2); audit right (4.6); as-is, no maintenance or update promise (8.1) [V https://www.bas.se/kontoplaner/kontoplanen-i-maskinlasbart-format/ and https://www.bas.se/wp-content/uploads/2026/04/Allmanna-Villkor-2026.pdf]. Dolibarr nonetheless ships `llx_accounting_account_se.sql` [V] | Fortnox/Visma APIs: not reachable [G] |
| NO / DK | SAF-T (NO since 2020; DK on request) [sec./V EC] | NO/DK standard charts [G] | Tripletex, e-conomic [G] |
| UK / US | HMRC VAT API [V]; US sales tax via provider, 1099/NACHA (09) | No statutory chart | QuickBooks, Xero |

Engineering consequence: an open-source pack cannot publish BAS data in a public AGPL repository (cl. 4.5), but obop-the-company can buy one licence and ship it inside a commercial distribution (cl. 4.3) [I]. SKR03/04 needs written confirmation from DATEV [G].

### A4. Incumbents and price bands (vendor pages, 2026-10-01, excl. VAT except Exact, whose page does not state the VAT basis) [V]

| Market | Incumbent and price |
|---|---|
| DE | Lexware Office (ex lexoffice) S/M/L/XL EUR 7.90 / 12.90 / 21.90 / 32.90 per month (50% off 3 months); payroll add-on from EUR 12.90. sevdesk: free (3 e-invoices/month), Rechnung EUR 8.90-11.90, Buchhaltung 19.90-25.90, Pro 27.90-34.90, 1 user included, extra user 5.90; DATEV export in all paid plans |
| ES | Holded Plus 15 / Basico 29 / Estandar 59 / Avanzado 99 / Premium 199 per month, "all plans include Verifactu", 900,000+ users |
| FR | Pennylane Free 0 / Starter 7 / Basique 14 / Essentiel 24 per month (1 user), e-invoicing included in all plans |
| NL | Exact Online Essentials 49 / Plus 99 / Professional 159 / Premium 299 per month |
| UK | Xero Ignite 18 / Grow 39 / Comprehensive 55 / Ultimate 70 GBP (90% off 6 months); claims 250,000 partner accountants |
| US | QuickBooks Online 38 / 85 / 140 / 340 USD list (50% off 3 months); Wave Starter 0, Pro 19 |
| Reference | Odoo Enterprise EUR 299-539 per user per year (20 s4) = EUR 24.90 / 44.90 per user-month on yearly billing (Standard / Custom) x 12; monthly billing 31.10 / 56.90; the page also shows discounted 19.90 / 35.90 (promo terms not checked) [V https://www.odoo.com/pricing] |

Reading: micro-accounting is a EUR 8-35/month commodity in DE/FR/ES; obop competes in the 10-249 band where the incumbents are Odoo, SAP B1, Business Central, weclapp-style ERPs, not Lexware [I].

### A5. Channel, language, hiring

Odoo partner directory, live 2026-10-01 [V] (https://www.odoo.com/partners/country/germany-56 etc.), density per 10k firms of 10-249 [I]:

| BE | CH | AT | NL | FR | ES | IT | US | DE | Nordics | PL | UK |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 137 (40.2) | 59 (10.9) | 40 (8.6) | 49 (7.6) | 137 (7.1) | 106 (5.7) | 73 (3.1) | 280 (2.2) | 102 (2.1) | 25 (2.1) | 20 (1.8) | 40 (1.5) |

Germany (12 Gold / 23 Silver / 67 Ready), the UK and Poland are the most under-served big markets; Belgium is saturated. Labour cost, Eurostat `lc_lci_lev` 2025, EUR/hour all economy [V]: NO 56.2, DK 51.7, BE 48.2 (2024), NL 47.9, AT 46.3, DE 45.0, FR 44.3, SE 43.1, FI 39.4, IT 32.0, ES 26.4, PL 19.1, EU27 34.9. At 160 h, one DE/FR/NL person-month of average labour is about EUR 7.1-7.7k, PL about 3.1k [I]; software roles cost more, so I plan EUR 9-10k per PM for DE/FR/NL and EUR 5-6k for PL/ES [G]. Language need: German (DE/AT/CH), French (FR, BE), Polish, plus English-only UIs for UK/US.

### A6. Liability exposure (producer side)

- **Spain**: Art. 201 bis Ley General Tributaria: fixed fine **EUR 150,000 per fiscal year with sales and per distinct type of system** for producing/selling non-compliant billing software; EUR 1,000 per system sold without a required certificate; EUR 50,000 per year for users holding uncertified systems [V BOE consolidated text, API id BOE-A-2003-23186]. Compliance is by the producer's own "responsible declaration" (RD 1007/2023 Art. 13), visible in the system and retained for all versions [V]. Whether gratis distribution counts as "producing" is a counsel question [G].
- **Germany**: Section 146a AO needs a BSI-certified TSE for electronic cash systems [V]; section 379(6) AO fines up to **EUR 25,000** for commercially advertising or distributing non-compliant systems or software [V https://www.gesetze-im-internet.de/ao_1977/__379.html]. Accounting software itself has no state certification [G].
- **France**: anti-fraud regime for cash/management software (certificate from an accredited body or publisher attestation); penalty amounts not verified [G].

### A7. Scoring

Weights: SME base 15, ERP gap 15, dated mandate reachable by a 2027-H2 GA 20, pack build cost 15 (from A8 PM), and 5 each for chart licence, accountant interop, incumbent weakness, Odoo under-service, liability, language/hiring, Peppol/PA access. Scores 1-5 are my judgement anchored on A1-A6 [I].

| | Size | Gap | Mandate | Cost | Chart | Interop | Incumb. | Odoo | Liab. | Lang. | PA | **Total /100** |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| **DE** | 5 | 5 | 5 | 3 | 3 | 3 | 3 | 5 | 4 | 2 | 5 | **84** |
| UK | 4 | 4 | 3 | 5 | 5 | 4 | 1 | 5 | 4 | 4 | 4 | 78 |
| US | 5 | 5 | 1 | 5 | 5 | 4 | 1 | 5 | 4 | 4 | 5 | 77 |
| PL | 3 | 3 | 3 | 3 | 5 | 4 | 3 | 5 | 4 | 4 | 4 | 68 |
| **FR** | 4 | 3 | 4 | 2 | 5 | 4 | 2 | 3 | 2 | 2 | 2 | 63 |
| ES | 4 | 3 | 3 | 1 | 5 | 4 | 3 | 3 | 2 | 4 | 3 | 60 |
| Nordics | 3 | 3 | 2 | 4 | 2 | 3 | 1 | 5 | 3 | 2 | 4 | 58 |
| IT | 4 | 4 | 1 | 2 | 5 | 3 | 2 | 3 | 3 | 3 | 3 | 56 |
| NL | 2 | 2 | 2 | 4 | 4 | 3 | 2 | 3 | 4 | 3 | 4 | 55 |
| BE | 1 | 1 | 3 | 5 | 5 | 4 | 2 | 1 | 4 | 2 | 4 | 55 |
| AT/CH | 3 | 2 | 1 | 5 | 3 | 3 | 3 | 1 | 4 | 2 | 4 | 54 |

Sensitivity (arithmetic re-checked): with mandate weight 30 (the 10 extra points taken from Size and Gap, 5 each) Germany still leads (84 vs UK 76, PL 68, FR 64; a proportional rescale of all other weights gives DE 86, UK 76, PL 67, FR 65); with cost weight 25 and mandate 10 the order becomes US 85, UK 82, DE 80, PL 68. **Germany is first under every weighting except the cost-heavy ones; UK and US win on build cost and language but have no compliance wedge, the weakest incumbent position (Xero, QuickBooks) and would need a product-led, not mandate-led, motion** (20 s5) [I]. Second/third ordering (FR vs PL vs ES) is within model error (about +/-8 points).

### A8. Recommendation and dated roadmap

**1st Germany (with AT/CH as follow-on), 2nd France, 3rd Poland.** FR is placed above the higher-scoring PL because (a) DE ZUGFeRD 2.5.2 equals Factur-X 1.09.2 [V 09], so the format component is reused, (b) the French SME base is 1.8x Poland's and its 1 Sep 2027 deadline drives the whole 2027-28 migration wave, (c) DE+FR+PL together exercise the three transport archetypes of the `EDocTransport` trait (email/Peppol without clearance, decentralised accredited platforms, central clearance), with Spain's report-only hash chain as the fourth [I]. Flip FR and PL if no French-speaking compliance lead or PA API partner is secured by 2027-Q1. Spain is a gated 4th (liability, cost, deadlines earlier than any possible GA).

PM per component, stand-alone, after the shared pack SDK exists; +/-40%; excludes payroll and POS fiscal [G]. Proxy for relative size: Community-visible Odoo 20.0 Python per country (KB): ES 682, FR 590, IT 502, PL 194, DK 150, CH 80, DE 52, US 24, NL 12, SE 11, AT 6, UK 5 (GitHub trees API; Enterprise report modules invisible, so DE/BE/NL are understated) [V]. Clearance- and report-heavy packs are 10x chart-plus-tax packs.

| Pack | Chart | Taxes | VAT return | E-invoice format | Transport | Audit export | **Total PM** | Notes |
|---|---|---|---|---|---|---|---|---|
| DE | 1.0 | 1.5 | 3.0 (ELSTER/ERiC, ZM) | 2.5 | 1.5 (email + Peppol AP partner) | 5.0 (DATEV EXTF 2.0, GoBD 1.0, DATEV Connect online 2.0, partner-gated) | **14.5** | |
| FR | 0.75 | 2.0 | 3.0 (CA3, EDI-TVA via partner) | 2.0 (Factur-X, CIUS) | 5.0 (PA connector, lifecycle, e-reporting flows) | 1.5 (FEC) | **14.25** (components sum to 14.25; was shown as 14.5) | -1.0 if DE done first |
| PL | 0.5 | 1.5 | 3.0 (JPK_V7) | 2.5 (FA(3)) | 4.0 (KSeF sessions, offline modes, UPO) | 2.0 (JPK_FA/KR_PD) | **13.5** | |
| ES | 0.75 | 2.0 | 4.0 | 2.0 (Facturae+UBL) | 7.0 (Veri*Factu 4, SII 3; TicketBAI +3 per territory) | 1.0 | **~17** | |
| IT | 0.5 | 2.0 | 5.0 | 3.0 | 3.0 (SDI) | 0.5 | ~14 | |
| NL | 1.0 | 1.0 | 4.0 (SBR) | 0.5 | 1.0 | 2.0 (XAF) | ~9.5 | |
| BE | 0.5 | 1.0 | 3.0 | 0.5 | 1.5 | 1.0 | ~7.5 | cheapest Peppol proof |
| AT / CH | 0.75 | 1-2 | 2.5 | 0.5-1 | 1.0 | 0.5 | ~7 / ~8 | reuse DE |
| SE/NO/DK/FI | 0.5 | 1.0 | 2.0 | 0.5-1 | 1.0-1.5 | SE 1.5, NO 2.5, DK 3.0, FI 0.5 | 6 / 7.5 / 9 / 6 | BAS licence (SE) |
| UK | 0.3 | 2.0 | 3.0 (MTD) | 1.0 | 0.5 | 0.3 | ~7 | |
| US | 0.3 | 2.5 (TaxProvider) | 0.5 | 0 | 0 | 3.0 (1099/NACHA) | ~6 | |

Flagged separately [G]: **payroll (13)** is 15-25 PM per country or bought via providers; **POS fiscal (12)**: DE TSE via cloud signer 3-5 PM, FR NF525 6-8 PM plus certification. Shared SDK, harness, signing and data pipelines: 10-14 PM. Three packs: about 42 PM plus 12 shared, roughly EUR 0.45M engineering plus EUR 0.1M legal/membership over 21 months, i.e. 2.6 FTE [G]. Maintenance planning figure: 25% of build per year [G], about 11 PM per year for three packs.

| Milestone | Date |
|---|---|
| Foundations: pack SDK, conformance harness, signing, counsel retained, DATEV/PA/AP partner outreach | 2026-Q4 |
| DE alpha (chart, taxes, XRechnung/ZUGFeRD create and validate) | 2027-03-31 |
| DE beta (UStVA, DATEV EXTF, Peppol via partner AP) | 2027-06-30 |
| **DE GA** (3 months before 1 Jan 2028 all-issuer deadline) | **2027-09-30** |
| FR start (second squad), alpha (PCG, FEC, Factur-X) | 2027-04 / 2027-09 |
| FR PA connector beta, GA | 2027-12 / **2028-03** |
| PL start, KSeF beta, GA | 2027-10 / 2028-05 / **2028-08** |
| Re-score gate (ES liability decision, Peppol-only BE/NL/AT/CH via partners) | 2027-Q2 and 2028-Q2 |

---

## Part B. Pack operating model

### B1. How comparable programmes are staffed (GitHub, branch defaults, 12 months since 2025-10-01) [V]

| Programme | Country content | Maintainer evidence | Lesson [I] |
|---|---|---|---|
| **Odoo 20.0 Community** | 229 `l10n_*` dirs of 642; **zero** `l10n_*_reports` modules (VAT returns, DATEV, ELSTER, Intervat, FEC exports are Enterprise) https://github.com/odoo/odoo/tree/20.0/addons | Non-i18n commits per country: FR 222 (103 authors), ES 168 (98), IT 152 (68), SA 146 (81), PL 72, DK 73, BE 49, CH 44, US 26, DE 23, NL 18, FI 17, UK 15, SE 14, AT 11, NO 10 | Employees cover statutory packs; thin "chart + taxes" packs get 10-25 commits a year. Paywalled returns are the grievance OCA exists for (03) |
| **OCA l10n-\*** (default branch 19.0; empty `20.0` branches ("Initial commit", 0 modules) were created in all 51 repos on 2026-09-28, nothing ported yet) | 51 repos; Germany 7 modules (3 DATEV, 4 DIN5008), France 14, Italy 12, Spain 33 (incl. SII, mod303/390/347/349), Belgium 3, NL 1, CH 1, AT 1; PL and NO repos hold no modules on 19.0; UK and US repos had no commits in 12 months | Human commits / authors / top-author share: ES 1,712/216/35%, FR 622/34/**82%**, IT 258/45/21%, BE 127/21/43%, DE 44/12/48%, NL 34/6/**82%**, CH 28/6/50%, AT 5/2 | Community packs lag one Odoo major and depend on one or two people |
| **ERPNext** | Only 6 countries in `erpnext/regional` (AU, IT, ZA, TR, AE, US); 72 verified chart JSON files (76 files in `verified/` incl. 4 .py; +24 in `unverified/`) incl. SKR03/04, PCG, PCMN, NL | India moved to `resilient-tech/india-compliance`: 1,342 commits, top dev 50%; Germany in partner app `alyf-de/erpnext_germany`: 40 commits, top dev 70%; `erpnext_datev`: 20, 75% | Compliance lives in separately released apps run by firms with a commercial reason |
| **Tryton** | In-tree: `account_be`, `account_fr`, `account_fr_chorus`, `account_eu`, `edocument_peppol`(+Peppyrus), CODA/MT940/OFX/AEB43 importers, Winbooks export, Factur-X; `account_es`, `account_de_skr03` published separately (7.8.1, 2026-03-02) | ~219 commits in 12 months for the entire 221-module monorepo | Small, curated, Peppol-first scope |
| **Dolibarr** | ~40 chart SQL files (SE, DE, FR, NL, ES, AT, CH, BE, DK, GB, US) | not measured | Charts only |
| Xero / QuickBooks | QBO has own sites for AU, CA, UK, IE, SG, ZA, AE etc.; DE/IT/ES only translated pages | n/a | Localise a few markets deeply |
| SAP / Sage | Not verified (SAP site blocked) | | [G] country versions plus partner add-ons |

### B2. Operating model

| Tier | Packs | Maintainer | Terms |
|---|---|---|---|
| T1 | DE, FR, PL + shared EN 16931/Peppol | obop core team, two named maintainers per pack (avoid the 82% single-author pattern) | Legal-change SLA below; golden-case gate |
| T2 | BE, NL, AT, CH, ES, IT, Nordics | Paid local partners (accounting-aware integrators) under a partner agreement with revenue share | Same gates and signing; partner holds the local declaration and insurance |
| T3 | everything else | Community, best effort | Marked `community`; no SLA, no "stable" badge without gates |

**Legal-change SLA** (commercial "Assured" tier only) [I]: law or final spec published >= 30 days before effect: pack released >= 14 days before effect; shorter notice: 5 business days; schema-rejection hotfix 72 h; draft rules shipped as `preview` with no SLA. Feasible against observed lead times: Peppol BIS 3.0.20 published 2025-11-24, mandatory 2026-02-23; 3.0.21 published 2026-05-20, mandatory 2026-08-17 (about 13 weeks each); EC code lists out 11 Sep 2026, artefacts 9 Oct, effective 16 Nov 2026 (about 9 weeks) [V 09]. Tracking cost is real: about 25% of build per year [G].

### B3. Test oracles and conformance gates (13 Impl 11)

| Area | Oracle (version, date) | Status |
|---|---|---|
| EN 16931 | CEN validation artefacts 1.3.16 (GitHub release 2026-04-13; EC registry: published 2026-04-16, effective 2026-05-15; EUPL-1.2), https://github.com/ConnectingEurope/eInvoicing-EN16931 | [V] |
| XRechnung | KoSIT validator 1.6.3 (2026-08-20, Apache-2.0), configuration and testsuite v2026-08-31 https://github.com/itplr-kosit/validator | [V] |
| Peppol | peppol-bis-invoice-3 (GitHub release v3.0.20; 3.0.21 on docs.peppol.eu per 09), phase4 4.8.0 (2026-10-01, Apache-2.0) | [V] |
| ZUGFeRD / Factur-X | Mustang 2.26.0 (2026-08-25, Apache-2.0) | [V] |
| KSeF | Official TEST / DEMO / PROD at api-test / api-demo / api.ksef.mf.gov.pl, FA(3) and FA_PEF(3) (TEST also accepts FA(2)); docs MIT, release 2.8.1 (GitHub 2026-09-22; deployed TEST 2026-09-22, DEMO and PROD 2026-09-23 per `api-changelog.md`) https://github.com/CIRFMF/ksef-api | [V] |
| VIES | Live REST API, no key: `.../vies/rest-api/check-vat-number`; per-state availability at `.../check-status` (called today, returned valid result and all states Available) | [V] |
| DATEV, ELSTER, AEAT, SDI, PA sandboxes | not reachable | [G] |

Gate ladder: `draft` (compiles, schema-valid), `preview` (passes oracles on regulator examples), `stable` (golden cases incl. edge cases, differential tests, local accountant or counsel sign-off recorded in the release, signed release), `certified` (external certificate where one exists). A pack with no golden cases cannot reach `stable` (13 Impl 11). Run Odoo only as a black-box differential oracle, never copy code (09 MUST 7).

### B4. Certification routes, cost, lead time (as of 2026-10-01)

| Route | Mechanism | Cost and lead time |
|---|---|---|
| **France NF525 / anti-fraud** | Cash and management software needs an accredited-body certificate or publisher attestation (CGI 286 I 3 bis; penalty not verified). LNE stopped issuing: handed to subsidiary BYCYB, LNE certificates up to 2025-08-06 [V https://www.lne.fr/fr/service/certification/certification-systemes-caisse-bycyb]. Validity 3 years with annual review [sec. https://en.wikipedia.org/wiki/Fiscalization] | Quote only, no public price [V]; lead time unknown [G]. Decide attestation vs certificate with counsel |
| **Spain Veri\*Factu / SIF** | **Self-declaration** by producer, visible in system, kept per version (RD 1007/2023 Art. 13); hash chain, event log, QR, Order HAC/1177/2024 specs [V]; producers were due July 2025 per EC factsheet | No certification fee; cost is engineering plus EUR 150k/year exposure (A6) |
| **Germany KassenSichV / TSE** | BSI-certified TSE (section 146a(3) AO); user registers within one month (146a(4)) [V]; POS software itself uncertified [sec.] | Use certified cloud TSE (fiskaly SIGN DE/DSFinV-K); vendor pricing page 404 [G]. POS out of first GA |
| **Poland KSeF** | No vendor certification; integration tested in TEST/DEMO [V] | API fee not stated in docs read [G] |
| **ZATCA (SA)** | Sandbox portal https://sandbox.zatca.gov.sa/ (returned HTTP 403/520 on the 2026-10-01 re-check, so unverified) and technical specification (the zatca.gov.sa e-invoicing page links a Developer Portal with E-Invoice Specifications and Security Requirements, but no sandbox link); waves announced >= 6 months ahead (not confirmed on the ZATCA page, unverified); onboarding steps and fees not verified | [G] |
| **Peppol Access Point** | OpenPeppol membership. AP only, 1-50 staff: EUR 1,050 sign-up + EUR 1,850/yr + EUR 1,500 certification; AP+SMP 1-10 staff: 1,800 + 2,750 + 2,500; 11-50: 2,100 + 3,700 + 2,500 (new members from 2025-07-01; existing members pay these from 2026-01-01; certification fee charged at first certification) [V https://peppol.org/join/fees/] | About EUR 4.4k year one, AP-only; lead time 2-4 months incl. test [G]. **Cheaper start**: resell a certified AP; Peppyrus plans (effective 2026-11-01): Free 100 messages/month, EUR 15 (250), EUR 49 (1,000), EUR 149 (unlimited sending), REST API, Tryton integrates it [V https://www.peppyrus.be] |
| HMRC MTD | Fraud-prevention headers legally required; recognition process not verified [V/G] | |

France PA: integrate, do not accredit (09 #9); accreditation cost and requirements not found [G].

### B5. Release channel and signing, independent of core

A pack is a versioned bundle: `manifest` (pack id, country, semver, `valid_from/valid_to`, `requires_core` range, data version, capabilities, oracle versions), declarative data, optional Rust/WASM code, SBOM and provenance. Three channels (`stable`, `preview`, `edge`) on an update repository secured with TUF (Rust `tough` 0.24.0, 2026-07-10, 650k recent downloads) and keyless Sigstore signatures (`sigstore` 0.14.0) or minisign (`minisign-verify` 0.3.0) [V crates.io]; pack-signing keys are separate from core-release keys, threshold-signed with the compliance lead; revocation and freeze lists; mirrors for air-gapped sites. Cadences decouple: data (rates, code lists) weekly or on event, pack code monthly, core per its release train. This also satisfies CRA signed-release and SBOM duties (17).

### B6. Warranty and liability ownership

Packs are AGPL "as is" with no warranty in the open edition (BAS itself disclaims everything, cl. 8-9 [V]). The commercial entity, as legal "producer", signs declarations (ES Art. 13), holds E&O insurance [G] and offers a capped warranty only on the Assured tier. The non-profit steward (17 #8) distributes but does not claim conformity. T2 partners sign local declarations and indemnify.

### B7. Free vs commercial: reconciling 17 and 03

17 says monetise "commercially maintained compliance/e-invoicing packs with update-SLAs" yet keep all core ERP open; 03 says never gate features and sell compliance-as-a-service (feeds, connectors). Reconciliation rule [I]: **anything the law defines (formats, tables, algorithms, report definitions, chart templates, validators, VAT-return and audit-file exports) is open and in the AGPL edition; anything operated, warranted or certified (hosted AP/PA/KSeF/Veri*Factu gateway, data feeds with uptime, legal-change SLA, warranty, signed certified builds, local-declaration service) is paid.** Evidence that the alternative backfires: Odoo Community 20.0 contains no VAT-return or DATEV export module while 229 of 642 directories are country packs [V]. Caveat found on re-check: Community 20.0 does ship e-invoicing and e-reporting connectors (`l10n_es_edi_verifactu`, `l10n_es_edi_sii`, `l10n_es_edi_tbai`, `l10n_fr_pdp`, `l10n_it_edi`, `l10n_pl_edi`, `l10n_fr_facturx_chorus_pro`), so Odoo's paywall sits on periodic returns and audit exports, not on e-invoice transport [V GitHub tree].

### B8. Data pipelines

| Feed | Cadence and access | Pipeline |
|---|---|---|
| EC EN 16931 code lists/artefacts | 15 May / 15 Nov; next 16 Nov 2026; then 17 May 2027 [V 09] | bot PR, oracle rerun |
| Peppol BIS 3.0.x, KoSIT XRechnung | about twice a year; mandatory ~13 weeks after publication [V] | same |
| VIES | REST, unauthenticated, per-state availability, `EL` for Greece [V]; tolerate outages, cache with validity | background validator |
| OSS | EUR 10,000 threshold; quarterly Union/non-Union, monthly import; returns via member-state portal [V https://vat-one-stop-shop.ec.europa.eu/one-stop-shop_en] | report definition in EU pack |
| VAT rates | EC points to TEDB, data supplied by member states; API not confirmed [V page/G] | own effective-dated tables, cross-checked to national texts |
| Charts | BAS and SKR annual; RGS release calendar (3.9 alpha) [V] | annual chart release in data channel |
| Bank formats | camt.053/MT940/pain.001 (09); EPC rulebook cadence not verified [G] | schema-tolerant parsers |
| KSeF, AEAT, HMRC | regulator changelogs (KSeF `api-changelog.md`) [V] | watch feeds |

### B9. Buy vs build

- **Tax engines**: EU VAT is a bounded table-plus-rules problem (rates, reverse charge, OSS) so **build**; US sales tax (nexus, sourcing, thousands of jurisdictions) so **buy** behind a `TaxProvider` trait (09 #14). Stripe Tax: Tax Basic 0.5% per transaction (no-code) or SEK 5 per transaction via API on the Swedish price page, Tax Complete from SEK 960/month, 100+ countries for collection, business location supported in all EU states plus NO, CH, GB, US, CA, AU [V https://stripe.com/en-se/tax/pricing, https://docs.stripe.com/tax/supported-countries]. Avalara and Vertex publish no prices (quote-based) [G].
- **Payroll**: buy or connect per 13 (Check, Gusto Embedded, Finch at $65 per connection per month on the Starter tier (capped at 15 connections) [V https://www.tryfinch.com/pricing], Employment Hero); local payroll packs only after a reference pack.

---

## Part C. Language and i18n program

### C1. Tooling

| Tool | Licence and cost | Git integration | Fit |
|---|---|---|---|
| **Weblate** 2026.9.1 (2026-09-08), self-hostable | GPL-3.0; hosted from EUR 47/month (10k strings) to 616 (10M); yearly ~20% cheaper; **Libre plan free** for public projects (160k-string limits); self-host support EUR 53-106/month [V https://weblate.org/en/hosting/] | native Git push/pull, PO | **Odoo itself uses Weblate**: `Odoo Translation Bot` commits "[I18N] *: fetch latest Weblate translations" (2026-09-12) with per-module `.po` in-tree [V https://github.com/odoo/odoo/commits/20.0/addons/l10n_de/i18n] |
| Crowdin | Proprietary SaaS; free licence for open-source on request (unlimited strings/members); paid prices did not load | integrations advertised (700+) | Good UX, lock-in |
| Transifex | Pricing page now routes to XTM; one usage plan by AI-translated words, no published price, no OSS programme on the page [V] | GitHub, Bitbucket, Azure DevOps | Not recommended |

**Decision**: self-hosted Weblate, PO files in the module repos, bot-synced like Odoo; use the free hosted Libre plan until volume justifies self-hosting [I]. Odoo ships 63 languages in `base` (incl. ar, he, fa, ja, ko, zh_CN, zh_TW, and variants fr_BE, fr_CA, es_419) [V]; target 12 locales at GA (de, fr, pl, en, nl, es, it, pt, sv, da, nb, fi) [G].

### C2. Machine-translation policy [I]
MT is a suggestion inside Weblate (DeepL/other), never auto-committed; forbidden for statutory labels (tax names, report lines, legal text, e-invoice business terms), which are copied from the authority's own vocabulary; glossary enforcement on; no unreleased customer data sent; MT-sourced strings carry a "needs review" flag until a human approves. DeepL's data-security page says texts are "never stored or used for model training without your consent" but does not tie this to a named plan [V https://www.deepl.com/en/pro-data-security]; plan-specific and API terms not verified [G].

### C3. Glossary and review gates
Per-language accounting glossary as a Weblate glossary component, seeded from the authority vocabulary of each pack (ELSTER/UStG terms, PCG, KSeF field names in Polish) and EU terminology (IATE, reuse terms not verified [G]), owned by a named reviewer per language. Gates: (1) translator, (2) second-person review, (3) for finance, tax and HR strings an accountant sign-off recorded in the release notes, (4) string freeze and screenshot context before each pack release.

### C4. RTL and CJK readiness
Logical CSS properties only, `dir` on `html`, CLDR 48 data (19; latest release is 48.2 of 2026-03-17, CLDR 49 has been in beta since 2026-09-22 [V https://cldr.unicode.org/]), ICU4X `icu` 2.3.1 (2026-08-21) for Rust formatting [V]; CI fixtures in ar, he, fa and ja, ko, zh in every UI snapshot test; IME-safe key handling (19 #10); embed CJK/Arabic fonts in PDF invoices [G].

### C5. Postgres full-text search for CJK
PostgreSQL's built-in parser only tokenises alphabet and digits, so Japanese and Chinese need help (PGroonga statement, [V] https://pgroonga.github.io/). Options, all PostgreSQL-licensed unless noted [V GitHub license files]: **PGroonga 4.0.9** (2026-09-23) for all languages (the extension is PostgreSQL-licensed, but the Groonga engine it builds on is LGPL-2.1 and the Windows package bundles MeCab under GPL/LGPL/BSD [V https://pgroonga.github.io/]); **pg_bigm** v1.2 (tag v1.2-20250903; bigram GIN for LIKE, Japanese-origin); **zhparser** 2.4 on master (CHANGELOG 2026-06-06, untagged; last tag v2.3 of 2025-01-24; Chinese segmentation via SCWS; licence file is PostgreSQL-style COPYRIGHT, GitHub shows NOASSERTION); **ParadeDB pg_search** (AGPL-3.0, BM25, v0.25.11 of 2026-09-29; v0.26.0 release candidates appeared 2026-09-30/10-01). Plan [I]: core search is an abstraction with capability detection; baseline `pg_trgm` + `unaccent` + ICU collations for alphabetic locales; for CJK tenants enable PGroonga (preferred) or pg_bigm; fall back to ILIKE on managed Postgres where extensions are unavailable (availability on hosted services not verified [G]); normalise to NFKC at write time.

### C6. Message-format decision (02, 06, 07, 19)
Facts: Lingui 6.8.0 (2026-09-23, MIT), `.po` catalogs by default, Vite plugin [V https://lingui.dev/]. MessageFormat 2 is "a stable part of CLDR" but some default functions and items in the `u:` namespace are still Draft [V https://github.com/unicode-org/message-format-wg]. Rust: `fluent-bundle` 0.16.0 (2025-05-22, 6.6M recent downloads), `icu` 2.3.1, `polib` 0.3.0, `ferrocat` 3.4.2 (PO + ICU MF parsing, created 2026-03, 0 stars); MF2 crates are tiny or brand new (`mf2_parser` 0.1.1 of 2024-10, `mf2_i18n_*` 0.1.0 of July 2026, `ox_mf2_parser` 0.14.0-alpha.12 of 2026-08-10 with 341 total downloads, and a `mf2` 2.0.0 family published 2026-10-01 with under 40 downloads per crate) [V crates.io]. **Decision**: one catalog format, PO; one message syntax, ICU MessageFormat 1 subset (plural, select, selectordinal, number, date) as Lingui uses; Rust evaluates the same subset using `icu` plural rules and a PO parser, verified by shared golden fixtures across 10+ locales in both runtimes. Fluent is not adopted because it would double catalogs, glossaries and review flows; MF2 is reserved for a 2027-H2 review once Lingui and a Rust implementation ship it [I].

---

## Confidence & gaps

- **High [V]**: Eurostat/Census/DBT counts and ERP shares; EC factsheet mandate dates (as published Aug 2025); BOE Art. 201 bis fines and RD 1007/2023 Art. 13; sections 146a and 379 AO; BAS terms and price; OpenPeppol fees; Peppyrus plans; KSeF environments; Odoo tree facts, partner counts, commit counts; Weblate/Lingui/crates versions; VIES live call.
- **Medium (sec.)**: DATEV figures (Wikipedia), SAF-T history, French fiscalisation summary, Fiscalization-page claims on NF525 validity and self-attestation, all post-Aug-2025 mandate shifts (09).
- **Unverified [G]**: NF525/BYCYB price and lead time; French penalty amounts; SKR03/04 and RGS licence terms; DATEV developer-programme terms and fees; Fortnox/Visma/Twinfield API terms; Avalara/Vertex pricing; SAP/Sage programmes; ZATCA onboarding details; Peppol AP and PA certification lead times; EPC rulebook cadence; TEDB API; IATE reuse terms; CH mandates; managed-Postgres extension availability; PM estimates; score weights. The OCA "human commit" filter is heuristic. Odoo counts cover the 20.0 branch, which includes forward-ports; Enterprise code is invisible.
- Eurostat compares persons-employed classes; SUSB counts employees; US/UK ERP shares are not comparable [I].

## Implications for obop

**MUST**
1. **MUST make Germany the first country** (GA 2027-09-30, anchored on the 1 Jan 2028 all-issuer deadline), with AT/CH as derivative packs; keep POS/TSE and payroll out of the first GA.
2. **MUST ship packs as signed, separately versioned, effective-dated bundles on their own TUF-secured channel** with SBOM and provenance; core never imports a country (09 #8).
3. **MUST keep the statutory baseline open** (chart templates, taxes, VAT-return and audit-file exports, e-invoice syntaxes, validators) and charge only for operated, warranted or certified services; publish this rule (03 #1).
4. **MUST NOT publish BAS data in the public repo**; buy one BAS licence for the commercial distribution and obtain written DATEV confirmation for SKR03/04 before public GA, else ship those charts as licensee-downloaded data packs.
5. **MUST gate `stable` on oracle validators (CEN 1.3.16, KoSIT 1.6.3, KSeF DEMO) plus golden cases and recorded local-accountant sign-off**; failing CI blocks release.
6. **MUST retain counsel for Spain (EUR 150k/year/type), Germany (EUR 25k) and France (NF525) before any pack claims conformance**; the commercial entity signs declarations.
7. **MUST adopt PO + ICU MessageFormat 1 subset** (Lingui on TS, PO parser + `icu` in Rust) as the single translation format and run Weblate as in Odoo; MT is suggestion-only and banned for statutory labels.
8. **MUST test RTL and CJK (ar/he/fa, ja/ko/zh) in CI and expose search as a capability-detected interface.**

**SHOULD**
9. **SHOULD choose France second and Poland third**, re-scored at 2027-Q2; swap if no French compliance lead or PA API partner by 2027-Q1. Integrate with a PA, do not accredit.
10. **SHOULD start Peppol through a certified AP partner** (Peppyrus-style, EUR 0-149/month) and join OpenPeppol only when volume justifies about EUR 4.4k year one.
11. **SHOULD run tiers T1/T2/T3 with two named maintainers per T1 pack** and revenue-share partners for BE, NL, AT, CH, ES, IT, Nordics.
12. **SHOULD offer the legal-change SLA (14 days pre-effect, 72h hotfix) only on the paid Assured tier.**
13. **SHOULD buy US sales tax (Stripe Tax at 0.5% per transaction or quoted engines) and payroll** behind traits; build EU VAT.
14. **SHOULD automate feeds** (EC code lists, Peppol, KoSIT, VIES, rates) as bot PRs with a pack-freshness dashboard.
15. **SHOULD fund reusable Rust libraries (EN 16931, KSeF client, SAF-T) via NLnet Restack (next deadline 2026-11-03 12:00 CET [V https://nlnet.nl/propose/]) and the Sovereign Tech Fund** (17; STF requires work above EUR 50,000 and excludes activities that already have other public funding, so do not double-fund the same work [V https://www.sovereign.tech/programs/fund]).

**COULD**
16. **COULD ship thin UK/US English packs** (about 6-7 PM each) for OSS adoption, not as go-to-market countries.
17. **COULD evaluate PGroonga for CJK search; pg_search if BM25 is wanted and AGPL is acceptable.**
18. **COULD apply to the DATEV developer programme early**, as Connect-online access is the German accountant-channel gate.
19. **COULD revisit MessageFormat 2 and ZATCA/Saudi (partner-led) in 2027-H2.**

---

## Verification log

Fact-check pass of 2026-10-01 (adversarial, independent re-fetch of primary sources: Eurostat API, BOE, gesetze-im-internet.de, EC factsheets, impots.gouv.fr, ksef.podatki.gov.pl, peppol.org, bas.se PDF text, GitHub API via `gh`, crates.io, PyPI, vendor pages). All 12 flagged claims were re-checked; 11 hold as written and the 12th (Lingui/MF2/PGroonga) holds with the wording fixes below.

**Confirmed as stated: about 330 individual facts.** This includes every flagged claim: the German e-invoice dates and the EC "14 Aug 2025 / edited 2026-03-06" stamp; all Eurostat counts and ERP shares (13 countries, recomputed "without ERP" figures, 13 labour-cost values); Art. 201 bis LGT amounts (EUR 150,000 / 1,000 / 50,000); RD 1007/2023 Art. 13 and Final Provision 4 (1 Jan 2027 / 1 Jul 2027, Art. 3.1.a = corporate-tax payers); BAS price (SEK 4,000 ex VAT), terms version 2026-04-14 and clauses 3.2, 4.3, 4.5, 4.6, 8.1, 9; Odoo 20.0 tree (229 of 642, zero `l10n_*_reports`), 63 base languages, Translation Bot commit 2026-09-12, per-country Python KB and 10 of the per-country commit counts re-derived exactly; sections 379(6), 146a(3), 146a(4) AO and section 14 UStG; OpenPeppol fees; KSeF environments, licence and release 2.8.1; LNE/BYCYB page; KoSIT 1.6.3 (2026-08-20, Apache-2.0) with testsuite/config v2026-08-31, CEN 1.3.16, Mustang 2.26.0, phase4 4.8.0 (2026-10-01); Lingui 6.8.0 (2026-09-23, MIT); PGroonga 4.0.9 (2026-09-23); Peppol BIS 3.0.20/3.0.21 publication and mandatory dates; EC code-list calendar; Factur-X 1.09.2 = ZUGFeRD 2.5.2 (4 Aug 2026); UK 258,520 and US 1,278,265 firm counts; HMRC VAT API; all vendor price tables (Lexware, sevdesk, Holded, Pennylane, Exact, Xero, QuickBooks, Wave, Odoo, Weblate, Peppyrus, Stripe Tax, Finch); crates.io versions (tough, sigstore, minisign-verify, fluent-bundle, icu, polib, ferrocat); OCA repo/module counts; ERPNext, India-compliance, alyf-de commit stats; Tryton (221 modules, 219 commits, 7.8.1 of 2026-03-02); Dolibarr charts; VIES REST; OSS rules; NLnet Restack deadline; scoring matrix arithmetic (all 11 totals recomputed).

### Corrections (was / now / source)

| # | Was | Now | Source |
|---|---|---|---|
| 1 | OCA l10n-*: "default branch 19.0, no 20.0 yet" | Default is still 19.0, but empty `20.0` branches ("Initial commit", 0 modules) exist in all 51 repos since 2026-09-28 | GitHub API, `OCA/l10n-*` branches |
| 2 | OCA has ">20 `l10n_es_aeat_*` modules"; Spain "~30" | 18 `l10n_es_aeat_*` modules; 33 modules in `l10n-spain` on 19.0 | GitHub tree `OCA/l10n-spain@19.0` |
| 3 | ERPNext "76 chart files" | 72 verified chart JSON files (76 files in `verified/` include 4 .py; plus 24 in `unverified/`) | GitHub tree `frappe/erpnext@develop` |
| 4 | zhparser "v2.3" | 2.4 on master (CHANGELOG 2026-06-06, untagged); last tag v2.3 (2025-01-24) | github.com/amutu/zhparser |
| 5 | France pack total 14.5 PM | 14.25 (0.75+2.0+3.0+2.0+5.0+1.5) | arithmetic |
| 6 | Poland "1 Apr 2026 all" | 1 Apr 2026 for all except taxpayers invoicing up to PLN 10,000/month, who start 1 Jan 2027 | http://ksef.podatki.gov.pl/etapy-wdrozenia-ksef/ |
| 7 | Austria "B2G ebInterface since 18 Apr 2020" | Structured e-invoices to federal entities since 18 Apr 2020; ebInterface 4.3/5/6 or UBL accepted | EC "eInvoicing in Austria" |
| 8 | Spain B2B "no fixed date ... projected Oct 2027/2028" | RD 238/2026 (BOE 2026-03-31, in force 2026-04-20): 12 months (> EUR 8M) / 24 months (others) after a ministerial order; order was a draft planned for 2026-10-01 (sec.), final order unchecked | https://www.boe.es/buscar/act.php?id=BOE-A-2026-7295; VATupdate 2026-04-27 (sec.) |
| 9 | NL "cabinet target 1 Jul 2030" | 2030 for B2B e-invoicing and 2031 for e-reporting (sec.); the exact day is unverified | VATupdate headlines Sep 2026 (sec.) |
| 10 | NO row tagged [V] EC for "EHF B2B planning date 1 Jan 2027" | EC page says no B2B mandate; the 2027 date is secondary only | EC "eInvoicing in Norway"; VATupdate (sec.) |
| 11 | DK "deferred to 1 Mar 2027" | Not confirmable; EC says Jan 2026 / Jul 2026 (expected). Marked unverified | EC "eInvoicing in Denmark" |
| 12 | France "about 100" platforms; dates tagged EC + sec. | EC: "nearly 100"; the 1 Sep 2026 / 1 Sep 2027 calendar is now confirmed by DGFiP | https://www.impots.gouv.fr/professionnel/questions/partir-de-quand-suis-je-concerne-par-la-reforme-de-la-facturation |
| 13 | UK Peppol core network 24 Jun 2026 and 2029 mandate tagged sec. | Confirmed on OpenPeppol news (2026-06-24, 2026-03-24) | https://peppol.org/news/ |
| 14 | Pennylane "public Company and Firm API v2" | Company API v1.0 and v2.0 shown; Firm API not confirmed | https://pennylane.readme.io/ |
| 15 | MF2: "`u:` function namespace is still draft"; MF2 crates "tiny (`mf2_parser`, `mf2_i18n_*`)" | "Some default functions and items in the `u:` namespace are still Draft"; also `ox_mf2_parser` 0.14.0-alpha.12 (341 downloads) and a `mf2` 2.0.0 family published 2026-10-01 (<40 downloads each); still no mature implementation | unicode-org/message-format-wg README; crates.io |
| 16 | PGroonga and others "all PostgreSQL-licensed" | PGroonga itself yes, but Groonga engine is LGPL-2.1 and the Windows package bundles MeCab (GPL/LGPL/BSD); zhparser licence is a PostgreSQL-style COPYRIGHT file | https://pgroonga.github.io/ |
| 17 | BAS cl. 4.5 "may not copy, spread" | Clause reads "obehörigen" (without authorisation) and also bars sublicensing beyond 4.3; added cl. 4.4 (no extension to group companies) | https://www.bas.se/wp-content/uploads/2026/04/Allmanna-Villkor-2026.pdf |
| 18 | EC factsheet parent URL `.../pages/467108874/` | That URL returns 404; replaced with the working Germany page URL; Austria page edit date 2026-03-18 added | EC |
| 19 | Sensitivity "mandate weight 30: DE 84, UK 76, PL 68, FR 64" (method unstated) | Reproducible only if the extra 10 points come from Size and Gap (5 each); proportional rescale gives DE 86, UK 76, PL 67, FR 65. Second case (US 85, UK 82, DE 80, PL 68) reproduces exactly | recomputed |
| 20 | DeepL "Translator Pro states content is not used for training [V, consumer plans]" | Page states "never stored or used for model training without your consent" with no plan distinction | https://www.deepl.com/en/pro-data-security |
| 21 | ZATCA sandbox and "waves >= 6 months ahead" tagged [V] | Sandbox returned HTTP 403/520; the ahead-notice rule is not on the ZATCA page. Marked unverified | zatca.gov.sa |
| 22 | A4 "excl. VAT [V]" for all | Exact Online page does not state the VAT basis | https://www.exact.com/nl/software/exact-online/prijzen |
| 23 | CLDR 48 given without status | Latest release 48.2 (2026-03-17); CLDR 49 in beta since 2026-09-22 | https://cldr.unicode.org/ |
| 24 | OpenPeppol fees "new members from 2025-07-01" | Existing members pay them from 2026-01-01 (added) | https://peppol.org/join/fees/ |

Additions (missing facts revealed by checking): Odoo Community 20.0 does ship e-invoicing/e-reporting connectors (Veri*Factu, SII, TicketBAI, l10n_fr_pdp, l10n_it_edi, l10n_pl_edi), so the paywall is on periodic returns and audit exports; KSeF 2.8.1 deployment dates (TEST 2026-09-22, DEMO/PROD 2026-09-23); CEN artefacts licence EUPL-1.2 and EC registry dates; Odoo list price arithmetic (EUR 24.90 / 44.90 per user-month yearly); Sovereign Tech Fund eligibility limits (minimum EUR 50,000; no other public funding for the same activity); DATEV's own figures (EUR 1.51B for 2024).

### Unresolved

- Odoo France "103 authors": my count (GitHub login or name, same window and filter) gives 98 for the 222 non-i18n commits; FR 222 and all other re-derived commit counts (DE, PL, SE, UK, AT, NO, FI, NL, US, CH, DK, BE) match. ES, IT, SA commit counts not re-derived.
- OCA per-repo "human commit / author / top-author share" figures re-checked only for FR, NL and DE (FR and NL ratios 82% and DE 48% match); ES, IT, BE, CH, AT not re-checked.
- Denmark deferral to 1 Mar 2027, Norway EHF 2027 date, Netherlands 1 Jul 2030, Spain final ministerial order and 1 Oct 2027/2028 dates: secondary or unconfirmed.
- Partner counts other than DE (102) and BE (137); A5 densities for the others derived from the author's counts.
- French anti-fraud penalty amount and NF525 "3 years, annual review" (Wikipedia only); BYCYB price and lead time; SKR03/04 and RGS licence terms; DATEV developer programme; IATE reuse; TEDB API; EPC cadence; managed-Postgres extension availability; Fortnox/Visma/Twinfield/Avalara/Vertex terms; SAP/Sage programmes; ZATCA sandbox and onboarding.
- Tryton details beyond module presence (Winbooks export, MT940/OFX/AEB43 importers), Lingui Vite plugin, Crowdin paid prices (page did not load), Comarch/enova/TeamSystem/Zucchetti positions, PM estimates and score weights (judgement, [G]/[I] by design).
