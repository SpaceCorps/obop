# 13 - HR and Payroll: Odoo reference, country packs, open-source and commercial landscape, scope for obop

Research date: 2026-10-01. Labels used throughout: **[V]** verified against a live source in this session, **[I]** inference drawn from verified facts, **[U]** recalled from training knowledge and not re-verified (treat as a hypothesis).

## Scope & method

Sources: Odoo Community source read through the GitHub API (`odoo/odoo` branches 17.0 to 20.0, module manifests and model files), the Odoo 19 documentation and its RST source (`odoo/documentation`), Odoo 19/20 release-notes pages, the OCA payroll repo (`OCA/payroll`), GitHub metadata and source for Frappe HRMS, OrangeHRM, Sentrifugo, Horilla and the Payroll Engine, government and regulator pages (gov.uk, IRS, EU AI Act portal, GDPR text), and vendor pages (Deel, BambooHR, Rippling, Check, Finch, Gusto docs). Wikipedia is used for company and tax-system background and is labelled secondary. A general web-search tool was not available in this environment, so discovery relied on GitHub search and direct fetches; several sources were bot-walled or 404 (Gusto and Rippling pricing, EUR-Lex, BMF, DSN portal) and were not bypassed. Odoo Enterprise code is private, so Enterprise payroll internals are described only from public docs and the OCA backport of the older open Odoo payroll.

---

## 1. Headline findings

1. **Odoo is at 20.0 as of today [V].** `odoo/release.py` on the default branch `20.0` has `version_info = (20, 0, 0, FINAL, 0, '')`, license LGPL-3 ([release.py](https://github.com/odoo/odoo/blob/20.0/odoo/release.py)); the release-notes index lists Version 20 as "Released September 2026" ([release notes](https://www.odoo.com/page/release-notes)).
2. **Odoo's HR split is: employee master, time off, attendance, recruitment, expenses, skills, fleet link, timesheets are Community (LGPL-3); Payroll, Appraisals, Planning, Referrals, Lunch and Sign/Documents-based contracts are Enterprise [V/I].** Community `addons/` on 20.0 contains `hr, hr_attendance, hr_holidays, hr_recruitment, hr_expense, hr_skills, hr_fleet, hr_timesheet, hr_work_entry, hr_presence, hr_gamification` and friends but no `hr_payroll`, `hr_appraisal` or `planning` ([addons listing](https://github.com/odoo/odoo/tree/20.0/addons)). The docs list Payroll, Appraisals, Referrals, Lunch as apps ([HR docs](https://www.odoo.com/documentation/19.0/applications/hr.html)); the editions page states Enterprise is licensed, Community open source ([editions](https://www.odoo.com/page/editions)).
3. **Payroll is the moat.** Odoo's open Payroll left Community in 13.0 [V]: `addons/hr_payroll` (LGPL-3) is present on the `odoo/odoo` branches 9.0 to 12.0 (together with `l10n_fr_hr_payroll`, `l10n_be_hr_payroll`, `l10n_in_hr_payroll`) and absent from 13.0 onwards ([12.0 tree](https://github.com/odoo/odoo/tree/12.0/addons/hr_payroll); 13.0 is listed as "Released October 2019" on the [release notes](https://www.odoo.com/page/release-notes); OCA/payroll's oldest branch is also 13.0); the only open descendant is an OCA backport marked "Beta", with `19.0` carrying just one module (`payroll`) and the `20.0` branch empty scaffolding today ([OCA/payroll](https://github.com/OCA/payroll)). Country packs are the expensive part, and even Odoo ships only about two dozen (section 5).
4. **Odoo's 19 to 20 architecture moved toward dated "versions" and rule-driven time classification [V].** `hr.version` (effective-dated employee/contract snapshot, exists from 19.0, absent in 18.0) replaced `hr.contract` in Community; 20.0 replaced the persisted `hr.work.entry` model with `hr.time.rule` classification of attendance and leave intervals (section 3).
5. **Nobody in open source has solved multi-country payroll.** Best open attempts: Frappe HR (India-first, GPL-3.0, actively released), Payroll Engine (.NET/SQL Server, MIT, v1.0 in April 2026, no shipped country rulesets found), and "legislation as code" projects (OpenFisca, French `publicodes`/`modele-social`) that model law but do not run payroll.
6. **Recommended obop boundary:** build the HR core fully (it is Odoo-Community-equivalent and tractable), build a *payroll engine* (versioned data-driven rules, payslips, batches, accounting posting, work-entry input) but ship country specifics as pluggable **packs and provider connectors**, with statutory filing out of core (section 10).

---

## 2. Odoo HR: app catalogue and edition boundary

| Area (Odoo app) | Edition | Community module(s) on 20.0 | Notes |
|---|---|---|---|
| Employees, departments, jobs, org chart | Community | `hr` | Models: `hr.employee`, `hr.version`, `hr.department`, `hr.job`, `hr.work.location`, `hr.employee.category`, `hr.employee.type`, `hr.departure.reason`, `hr.employee.departure`, `hr.employee.public` [V] |
| Contracts | 17/18 Community (`hr_contract`); 19+ folded into `hr.version` | none in 19/20 | `hr_contract` present on 17.0 and 18.0, absent on 19.0/20.0 [V]; payroll docs still say every employee needs a "running contract" [V] ([contracts](https://www.odoo.com/documentation/19.0/applications/hr/payroll/contracts.html)) |
| Time off | Community | `hr_holidays`, `hr_holidays_attendance` | `hr.leave`, `hr.leave.allocation`, `hr.leave.accrual.plan`, `hr.leave.accrual.level`, `hr.leave.mandatory.day`, `hr.time.rule.allocation.log` [V] |
| Attendance | Community | `hr_attendance` | `hr.attendance` with GPS lat/long, IP, browser, check-in image, time-rule link [V] |
| Recruitment / ATS | Community | `hr_recruitment` (+ `_skills`, `_sms`, `_survey`) | `hr.applicant`, `hr.recruitment.stage`, `hr.applicant.refuse.reason`, `hr.job.platform`, `hr.talent.pool`, `hr.recruitment.source`, `hr.recruitment.degree`, `hr.applicant.category` [V] |
| Expenses | Community | `hr_expense` | 20.0: approved expenses become draft vendor bills; standalone "Employee Expenses" menu removed ([Odoo 20 notes](https://www.odoo.com/odoo-20-release-notes)) [V] |
| Skills / resume | Community | `hr_skills` | `hr.skill.type`, `hr.skill`, `hr.skill.level`, `hr.employee.skill`, `hr.resume.line(.type)`, `hr.job.skill` [V] |
| Timesheets | Community | `hr_timesheet`, `hr_timesheet_attendance` | Posts to analytic lines, depends on `project` and `analytic` [V] |
| Fleet | Community | `fleet`, `hr_fleet` | Vehicle history per employee [V] |
| Work entries | Community (type + rules only on 20.0) | `hr_work_entry` | `hr.work.entry.type`, `hr.time.rule` (persisted `hr.work.entry` existed on 18.0 and 19.0, gone on 20.0) [V] |
| Presence, gamification/badges | Community | `hr_presence`, `hr_gamification` | |
| Payroll (+ localizations, salary configurator `hr_contract_salary`) | **Enterprise** | none | Docs only; Enterprise source is private [V/I] |
| Appraisals (goals, 360 feedback) | **Enterprise** | none | Docs: scheduling, templates, 360, goals, skills evolution [V] |
| Shift planning (Planning) | **Enterprise** (listed under Services in docs) | none | |
| Referrals, Lunch, Frontdesk | **Enterprise** | none | |
| Privacy tooling | Community | `privacy_lookup`, `data_recycle` | No Community audit log; OCA ships `auditlog` in `server-tools` [V] |

Pricing context [V, parse ambiguous]: the Odoo Online pricing page (Yearly/Monthly toggle) lists per-user-per-month prices of EUR 19.90 (Standard) and EUR 35.90 (Custom) on what appears to be the yearly option (EUR 24.90 and 44.90 on the other; a struck-through EUR 31.10 / 56.90 is also shown and the footnote says "The discount is valid for 12 months, for initial users ordered"), a EUR 7.90 "Light User", and a "One App Free" plan; payroll-related light-user access is limited to getting payslips ([pricing](https://www.odoo.com/pricing)). This shows how Odoo handles employee self-service: ESS users are internal users or light users, not a separate portal.

---

## 3. Odoo HR data model (what obop must reproduce)

Read directly from `odoo/odoo` 20.0 ([hr_version.py](https://github.com/odoo/odoo/blob/20.0/addons/hr/models/hr_version.py), [hr_employee.py](https://github.com/odoo/odoo/blob/20.0/addons/hr/models/hr_employee.py)).

### 3.1 Employee and effective-dated version

- `hr.employee` uses `_inherits = {'hr.version': 'version_id'}`: the employee row is identity (name, resource/calendar, user link, work contact, badge/barcode/PIN, parent/coach, tags, properties) and the current `hr.version` row holds everything that changes over time.
- `hr.version` fields (sampled): `employee_id`, `date_version` ("Effective Date"), `last_modified_uid/date`, `department_id`, `job_id`, `job_title`, `address_id`, `work_location_id`, `resource_calendar_id`, `tz`, `contract_date_start/end`, `fixed_term`, `trial_date_end`, `structure_type_id`, `wage` (monthly gross), `employee_type_id`, `hr_responsible_id`, `departure_id`, plus personal data: `identification_id`, `passport_id`, `passport_expiration_date`, `sex`, private address, `marital`, spouse name and birthdate, `children`, home-work distance.
- Field-level ACL is declared on the fields: sensitive ones carry `groups="hr.group_hr_user"` (Officer) and HR manager is `hr.group_hr_manager` (Administrator) implying Officer ([hr_security.xml](https://github.com/odoo/odoo/blob/20.0/addons/hr/security/hr_security.xml)) [V].
- Low-privilege read goes through `hr.employee.public`, a SQL view (`_auto = False`) exposing only name, department, job, work phone/email, work location, user, calendar, presence [V] ([hr_employee_public.py](https://github.com/odoo/odoo/blob/20.0/addons/hr/models/hr_employee_public.py)). This is the pattern to copy: a separate, narrow read model for the directory and org chart, not column hiding on the master table.
- Bank accounts: `bank_account_ids`, `primary_bank_account_id`, `is_trusted_bank_account`, `salary_distribution` on the employee; a "trusted bank account" flag gates payment [V].
- Departure: `hr.employee.departure` with `hr.departure.reason`, dismissal/departure/action dates and an "apply immediately or on date" switch [V].
- Onboarding and offboarding: implemented as `mail.activity.plan` + `mail.activity.plan.template` (in `hr`), i.e. activity checklists per employee, not a dedicated workflow engine [V, source file names]; the docs list Onboarding and Offboarding pages ([Employees docs](https://www.odoo.com/documentation/19.0/applications/hr/employees.html)).

### 3.2 Time off

- `hr.leave.accrual.plan` has levels (`hr.leave.accrual.level`): start count/type, milestone, `added_value` and type (hours/days), frequency (hourly/daily/weekly/bimonthly/monthly/biyearly/yearly with day selectors), caps (`maximum_leave`, yearly cap), carry-over options and maximum carried duration, accrual validity [V].
- `hr.leave.mandatory.day` (company-wide mandatory days), public holidays live as `resource.calendar.leaves` [V]. Time off types decide approval mode and whether allocation is required (docs landing page confirms types, allocations, accrual plans, public holidays, mandatory days, "My time", management, reporting) ([Time Off docs](https://www.odoo.com/documentation/19.0/applications/hr/time_off.html)).
- Payroll link: leaves map to `hr.work.entry.type`; the payroll setting "Deferred Time Off" handles leave taken after payslips were validated, applied to the next pay period ([payroll docs](https://www.odoo.com/documentation/19.0/applications/hr/payroll.html)).

### 3.3 Attendance, overtime, work entries (changed in 20.0)

- `hr.attendance`: `check_in/out`, `worked_hours`, `break_duration`, in/out GPS coordinates, IP address, browser, mode (kiosk/manual/systray) and an `in_image`, plus `time_rule_id`, `source_attendance_id`, `overtime_attendance_ids`, `state`, `source_stale` [V].
- `hr.time.rule` (in `hr_work_entry`): per-country/company/employee-domain rules with threshold operator, hours mode (calendar source, expected hours), period (`quantity_period`, `week_start`), weekday flags, public-holiday flag, timing window (from/to), employee and employer tolerance, source and target `work_entry_type_id`, and `amount_rate` (premium multiplier). A pipeline function resolves overlapping intervals by rule `sequence` and reclassifies them [V]. Release notes confirm overtime rulesets, public-holiday overtime rates, an overtime analysis report, photo at check-in, and eSSL/Mantra biometric device feeds ([Odoo 20 notes](https://www.odoo.com/odoo-20-release-notes)) [V].
- Inference [I]: Odoo 20 derives work entries from attendance/leave intervals via rules at compute time instead of persisting `hr.work.entry` rows (model file gone from `hr_work_entry/models` between 19.0 and 20.0). obop should model "classified time intervals" as the payroll input, whichever way they are stored.
- Work entry types carry `code`, `display_code`, `external_code` (export to third parties), `country_id`, `count_as`, `amount_rate` [V].

### 3.4 Recruitment, expenses, skills

- Recruitment pipeline: default stages New, Initial Qualification, First Interview, Second Interview, Contract Proposal, Contract Signed (the 20.0 data file `hr_recruitment_data.xml` names the second stage "Qualification"; the docs call it "Initial Qualification"); stage-level email templates; kanban status (grey/green/red); interview survey; SMS and CV-OCR via paid credits; job-board posting; salary package configurator for offers ([Recruitment docs](https://www.odoo.com/documentation/19.0/applications/hr/recruitment.html)) [V].
- Expenses in 20.0 become vendor bills in the expense journal; reimbursement via payslip is listed on the editions page as an Enterprise sub-feature [V, [editions](https://www.odoo.com/page/editions)].
- Skills: type, skill, level (with a numeric progress), per-employee skill, resume lines by type, job required skills, and `hr_recruitment_skills` matching applicants to jobs [V].

---

## 4. Odoo payroll structure (the model obop must be able to express)

Sources: Odoo 19 docs RST ([salaries.rst](https://github.com/odoo/documentation/blob/19.0/content/applications/hr/payroll/salaries.rst), [payslips.rst](https://github.com/odoo/documentation/blob/19.0/content/applications/hr/payroll/payslips.rst)) and the OCA backport ([hr_salary_rule.py](https://github.com/OCA/payroll/blob/19.0/payroll/models/hr_salary_rule.py)).

### 4.1 Six building blocks [V]

1. **Structure type** (`hr.payroll.structure.type`, stub lives in Community `hr`): groups structures; sets pay frequency, default working hours/schedule, default structure, wage type (fixed/salary vs hourly). Defaults: "Employee" and "Worker".
2. **Structure** (`hr.payroll.structure`): ordered set of rules, e.g. "Regular Pay", "End of Year Bonus" (Belgium merges to a single "Regular Pay" in 20.0).
3. **Rule** (`hr.salary.rule`): `code` (case sensitive, used as variable), `category`, `sequence`, `appears_on_payslip`, "contributes to employer cost", condition and amount computation, display styling, accounting tab.
4. **Rule parameter** (`hr.rule.parameter`): named values with "when active" dates, looked up by rules (this is how annual rate changes are delivered without code changes).
5. **Rule category**: hierarchical (parent), per-country, code-addressable (classic BASIC, ALW, GROSS, DED, NET, COMP codes; category totals are available to later rules) [V for mechanism, U for exact code list in Enterprise].
6. **Salary input type** / other inputs: typed one-off lines (deduction, reimbursement, tips, expenses) on the payslip, and "Salary Input" rules which create fields on the employee/version form, the payslip, or both (unit: monetary, quantity, percentage, checkbox).

### 4.2 Rule evaluation [V]

- Rules run in ascending `sequence`; earlier results are visible to later ones.
- Condition modes (19.0): Always True, Salary Input, Other Input, Python Expression, Domain. Older/OCA: none/range/python.
- Amount modes: Percentage (base expression x quantity expression x rate), Fixed Amount, Other Input, Python Code. The line total is `total = amount * quantity * (rate / 100)`.
- Python context: `payslip`, `employee`, `version` (was `contract`), `result_rules[code]` with `total/amount/quantity/rate/ytd`, `categories[code]`, `worked_days[work_entry_type_code]` with `number_of_days/number_of_hours/is_paid`, `inputs[code]`. Outputs: `result`, `result_rate` (default 100), `result_qty` (default 1), `result_name`.
- Implementation detail [V in OCA]: evaluation uses Odoo's `safe_eval` on stored Python text (`condition_python`, `amount_python_compute`). That is both the power and the main security/maintainability weakness: payroll logic is user-editable code in the database.
- YTD (`ytd`) is first-class; the Payroll setting "YTD reset date" defaults to 1 January ([payroll docs](https://www.odoo.com/documentation/19.0/applications/hr/payroll.html)).

### 4.3 Payslip life cycle, batches, posting [V]

- Payslip fields: employee, contract/version, batch, structure, period (date_from/to), worked-days lines, input lines, computed salary lines, close date, accounting date, salary journal, note.
- Flow: **Compute Sheet** (recomputable until confirmed) -> **Create Draft Entry** (payslip becomes Done, draft journal entry created) -> post entry -> register payment (bank transfer/SEPA or cheque) -> paid. A payroll Dashboard shows warnings (e.g. employees without running contract, employees without bank account) before processing.
- **Pay runs** (`hr.payslip.run`, "batches") group payslips; settings allow batching account move lines, which anonymises journal entries and disables individual payments.
- Accounting tab per rule: debit account, credit account, "split on names", "excluded from net", "set employee on account line"; OCA's `payroll_account` adds analytic account and tax links per rule and `move_id` on the payslip ([OCA payroll_account](https://github.com/OCA/payroll/tree/18.0/payroll_account)).
- Salary attachments (garnishments, loans, advances; docs page "Salary adjustments"), commissions, headcount report, work entry analysis and "time off to report" are the surrounding features ([payroll docs](https://www.odoo.com/documentation/19.0/applications/hr/payroll.html)).
- Payslip PDFs are filed into the Documents app in 20.0 (dedicated folder, shortcut in each employee's drive) ([Odoo 20 notes](https://www.odoo.com/odoo-20-release-notes)).

### 4.4 OCA payroll (open backport) [V]

`OCA/payroll` (AGPL-3.0 repo, 123 stars, pushed 2026-09-28) `payroll` module 19.0.1.0.1, maturity "Beta", LGPL-3, authored by "Odoo SA, OCA", described as "a backport from Odoo SA" not covered by the OCA CLA; models `hr.payslip`, `hr.payslip.line`, `hr.payslip.input`, `hr.payslip.worked_days`, `hr.payslip.run`, `hr.salary.rule(.category)`, `hr.contribution.register`, `hr.payroll.structure`, `hr.rule.input` ([OCA/payroll](https://github.com/OCA/payroll)). The 18.0 branch has six modules (`payroll`, `payroll_account`, `payroll_contract_advantages`, `hr_payroll_document`, `payroll_hr_public_holidays`, `payroll_rule_time_parameter`); 19.0 has only `payroll`; 20.0 is empty scaffolding. There are no OCA country rule packs in that repo. This is the realistic ceiling for community-maintained Odoo payroll.

---

## 5. Country packs and statutory filing

### 5.1 What Odoo ships (Enterprise) [V]

Odoo 19 docs list payroll localizations for: Australia, Bangladesh, Belgium, Egypt, Hong Kong, India, Indonesia, Jordan, Kenya, Lithuania, Luxembourg, Mexico, Morocco, Netherlands, Pakistan, Poland, Romania, Saudi Arabia, Slovakia, Switzerland, Turkiye, United Arab Emirates, United States (23); the 18 docs add Malaysia. Only 12 have dedicated pages ([localizations index](https://www.odoo.com/documentation/19.0/applications/hr/payroll/payroll_localizations.html), [18.0](https://www.odoo.com/documentation/18.0/applications/hr/payroll/payroll_localizations.html)). **France, Germany and the UK are not on the list**; Odoo instead offers the **Employment Hero Payroll** connector for Malaysia, New Zealand, Singapore and the UK, which pulls payslip journal entries (draft, weekly sync, API key + business ID) into Odoo while payroll itself runs in Employment Hero ([doc](https://www.odoo.com/documentation/19.0/applications/hr/payroll/payroll_localizations/employment_hero.html)). Older Odoo Community did ship a French pack: `l10n_fr_hr_payroll` (plus `l10n_be_hr_payroll` and `l10n_in_hr_payroll`) exists on the `odoo/odoo` branches 9.0 to 12.0, with no German or UK payroll module in those branches [V, [12.0 addons](https://github.com/odoo/odoo/tree/12.0/addons)]. Whether Enterprise 13.0+ ever shipped FR/DE/UK packs is (unverified; private code).

Odoo 20 additions [V, [notes](https://www.odoo.com/odoo-20-release-notes)]: Belgium (Dimona flow, joint committees, CP302 flexi-jobs, meal voucher reporting), Hong Kong (IR56B/E/F/G/M, MPF reports), Saudi Arabia (GOSI, end-of-service), UAE (Emiratization report), India (gratuity report, EPF summary, TDS challans, HDFC eNET file, sandwich leave), new base packages for Iraq, Kuwait, Oman (WPS files) and the Philippines, updates to Egypt, Turkiye, Luxembourg, Lithuania, Indonesia, Mexico, Pakistan, and **US: NYC/Yonkers local taxes plus support for 11 named states** (Georgia, Iowa, Kansas, Kentucky, Michigan, Mississippi, Missouri, New Jersey, South Carolina, Tennessee, Utah, "including state, county, and city tax rules"). Four of those 11 (GA, IA, MS, NJ) already appear in the 19.0 docs table below, so net-new state coverage versus that table is 7 (KS, KY, MI, MO, SC, TN, UT), i.e. about 27 state-level jurisdictions including DC [I].

### 5.2 Depth varies enormously

| Country | What Odoo documents [V] | Filing / e-submission handling |
|---|---|---|
| US | Native federal (income, FICA, unemployment) and **20 jurisdictions** (AL, AZ, CA, CO, FL, GA, ID, IL, IA, MS, NV, NJ, NY, NC, OR, TX, VT, VA, WA, DC) in 19; W-4 and state forms on employee, pre/post-tax deductions; W-2 report, Form 941, Form 940, ADP export ([US doc](https://www.odoo.com/documentation/19.0/applications/hr/payroll/payroll_localizations/united_states.html)) | Reports/exports generated; direct e-filing to IRS/states not evidenced in the doc text I could read |
| Belgium | Dimona (manual and automated sync), DmfA (web or batch; third-party submission), holiday pay incl. double holiday pay, 13th month, termination/notice, forms 281.10, 281.45, 274.xx, 273S ([Belgium doc](https://www.odoo.com/documentation/19.0/applications/hr/payroll/payroll_localizations/belgium.html)) | Automated Dimona sync (needs external technical configuration), XML declaration generation for 281.xx, work-entry **export to SD Worx** |
| India | `l10n_in_hr_payroll`, `_account`, `l10n_in_hr_holidays`; PF, Professional Tax (by state), ESIC, LWF; reports Salary Register, EPF, ESI, LWF ([India doc](https://www.odoo.com/documentation/19.0/applications/hr/payroll/payroll_localizations/india.html)) | Reports; 20.0 adds TDS challans, bank file |
| UK, MY, NZ, SG | External payroll via Employment Hero | Filing stays in the provider |

### 5.3 Statutory regimes that drive the engineering (what packs have to implement)

| Country | Obligation | Cadence / source |
|---|---|---|
| UK | Real Time Information: Full Payment Submission on or before payday for everyone paid; Employer Payment Summary by the 19th of the next tax month; software required ([gov.uk](https://www.gov.uk/running-payroll/reporting-to-hmrc)) [V] | HMRC publishes RTI technical specs per tax year (2027-28 spec dated 16 Sep 2026; 2026-27 dated 16 Sep 2025) ([HMRC collection](https://www.gov.uk/government/collections/real-time-information-online-internet-submissions-support-for-software-developers)) [V] |
| US | Federal withholding per IRS Pub. 15-T, revised yearly; 2026 edition reflects P.L. 119-21, new W-4/W-4P checkboxes, worksheet 1A percentage method for automated systems ([Pub 15-T](https://www.irs.gov/publications/p15t)) [V]; state/local on top; W-2/941/940 [V via Odoo TOC] | Annual table change; quarterly and annual forms |
| Germany | Employer computes and withholds Lohnsteuer from ELStAM data, liable for mistakes (par. 42d EStG); monthly Lohnsteuer-Anmeldung by the 10th; electronic via ELSTER only since 2005 ([de.wikipedia, secondary](https://de.wikipedia.org/wiki/Lohnsteuer_(Deutschland))) [V-secondary]. The BMF publishes the annual "Programmablaufplan" tax algorithm [U; BMF site bot-walled] | Annual algorithm; monthly filing |
| France | DSN (Declaration sociale nominative) replaces dozens of social filings; monthly by the 5th or 15th of M+1 plus event signals (sick leave, end of contract); filed via net-entreprises.fr or MSA, API from payroll software; includes income-tax at-source data since 2019 ([fr.wikipedia, secondary](https://fr.wikipedia.org/wiki/D%C3%A9claration_sociale_nominative)) [V-secondary] | Monthly + events |
| Belgium | Dimona (immediate employment declaration), DmfA quarterly social declaration, Belcotax-type forms [V via Odoo doc headings] | Event + quarterly + yearly |
| India | PF, ESI, PT, LWF, TDS (challans, Form 16/24Q [U]); reports exist in both Odoo and Frappe HR [V] | Monthly/quarterly |
| Gulf | WPS (wage protection system) files, GOSI [V via Odoo 20 notes] | Monthly bank file |

Payment files: SEPA credit transfer (ISO 20022 pain.001) is referenced by Odoo's accounting setting for payroll ("SEPA payments") [V]; ISO 20022 generation is standard [U].

---

## 6. Why payroll is the hardest domain to open-source

Evidence-backed reasons, each tied to something verified above:

1. **Law changes on a schedule, per country, per year.** HMRC re-issues RTI specs each tax year; the IRS re-issues withholding publications yearly and in 2026 changed W-4 mechanics; US coverage inside Odoo grew from 20 jurisdictions in the 19.0 docs table to about 27 (7 net-new states; the 20 release notes name 11 states but four were already in the 19.0 table) plus NYC/Yonkers local taxes in one release [V/I]. A pack is a subscription to legal maintenance, not a one-time port.
2. **Long tail of jurisdictions.** Odoo, with a paid Enterprise business funding it, covers about two dozen countries and 20 US jurisdictions; FR/DE/UK are handled through a connector [V]. Frappe HR, the most active open HR project, ships only `india` and `united_arab_emirates` regional modules [V] ([hrms/regional](https://github.com/frappe/hrms/tree/develop/hrms/regional)).
3. **Liability sits with the software operator's customer and with correctness.** German law makes the employer liable for withholding errors [V-secondary]; filing errors create penalties. Open-source maintainers cannot carry that liability or an SLA, which pushes vendors to closed Enterprise tiers (Odoo), paid "Advanced" tiers (OrangeHRM payroll connector only, [pricing](https://www.orangehrm.com/en/pricing)) or no payroll (Horilla, Sentrifugo).
4. **Money movement is regulated.** Embedded payroll vendors talk about NMLS licensing and a payments entity ([Check](https://checkhq.com/)) [V]: paying wages, remitting taxes and holding funds needs licences and banking partners that a code project does not have [I].
5. **Statutory channels need accreditation and credentials.** HMRC RTI requires compatible software and employer credentials, France's DSN requires a certified-format feed to net-entreprises/MSA, Belgium's Dimona/DmfA need channel setup outside Odoo ("technical configuration (outside Odoo)" in the Belgian doc) [V]. Test-harness access and recognition processes are gatekeepers [I].
6. **Rules are data-dense and retroactive.** Retro-pay recalculation, YTD/cumulative tax methods (IRS "annualized, cumulative, part-year" alternatives in Pub. 15-T section 6 [V]), termination settlements, holiday pay (Belgium double holiday pay [V]) require temporal data and audit trails that generic rule tables do not give. Payroll Engine's design states exactly this: time-stamped cases, retro recalculation, cancel/undo, multiple runs per period, forecast previews ([README](https://github.com/Payroll-Engine/PayrollEngine)) [V].
7. **Testing needs golden cases from the authority.** Correctness is "matches the regulator's worked examples", so each pack needs a maintained conformance suite [I].

### 6.1 Open "law as code" and engine projects worth stealing from

| Project | What it is | License / state [V] |
|---|---|---|
| Payroll Engine | Regulation-driven payroll framework: layered regulations (base, country, industry, company), cases (time-stamped wage data), wage types, collectors, lookups, payruns, sandboxed C# scripting; REST API, Blazor UI, MCP servers; .NET/SQL Server | MIT, v1.0.0 2026-04-21, v1.0.1 2026-05-07, 125 stars; no bundled country regulations found in README ([repo](https://github.com/Payroll-Engine/PayrollEngine)) |
| publicodes + `modele-social` | Declarative language and the French social-security/contribution model used by URSSAF's mon-entreprise simulators; npm `modele-social` 11.1.0 | MIT, pushed daily; government-maintained ([repo](https://github.com/betagouv/mon-entreprise/tree/master/modele-social)) |
| OpenFisca | Microsimulation "legislation as code" engine and country models (France, others) with public web API | Core AGPL-3.0, 239 stars; `openfisca-france` 312 stars ([core](https://github.com/openfisca/openfisca-core)) |
| OCA payroll | Backport of Odoo's old open payroll | Beta, see 4.4 |

Takeaway [I]: the credible open path is **rules as versioned, testable data with explanation traces**, plus country-maintained packs where a national body already publishes machine-readable law (France). There is no Rust payroll engine: GitHub search for Rust payroll returned only blockchain and hobby projects (top result 8 stars).

---

## 7. Open-source HR comparison

| Project | License | Stack | Status (GitHub API, 2026-10-01) | Scope |
|---|---|---|---|---|
| **Odoo Community HR** | LGPL-3 | Python/Odoo | 54,767 stars, 20.0 released Sep 2026 | Full HR core minus payroll/appraisal/planning ([repo](https://github.com/odoo/odoo)) |
| **Frappe HR (hrms)** | GPL-3.0 | Python/Frappe, Vue UI; requires `frappe/erpnext` | 8,851 stars; v16.20.1 on 2026-10-01, v15 and v14 still patched | Most complete open HR+payroll ([repo](https://github.com/frappe/hrms)) |
| **OrangeHRM** | GPL-3.0 | PHP (plugins: PIM, Leave, Time, Attendance, Recruitment, Performance, Claim, Buzz, Directory, LDAP/OpenID) | 1,146 stars; v5.9 released 2026-06-28 | Starter edition free; payroll only as "Payroll Connector" in paid Advanced; audit trail, MFA, org chart, e-sign, onboarding, pay policies also paid-only; price by quote per employee per month ([pricing](https://www.orangehrm.com/en/pricing)) |
| **Horilla** | LGPL-2.1 | Python/Django | 1,445 stars; 2.1.8 on 2026-09-26 | HRMS with recruitment, leave, attendance, payroll-like modules [feature list not verified] ([repo](https://github.com/horilla-opensource/horilla)) |
| **Sentrifugo** | GPL-3.0 | PHP/Zend Framework 1 | 542 stars; last commit 2017-04-17, last push 2021-07-02 | Effectively abandoned ([repo](https://github.com/sapplica/sentrifugo)) |
| IceHrm, OpenCATS, Kimai | mixed | PHP/JS | all pushed within last days | Point solutions (HR, ATS, time tracking; Kimai is AGPL-3.0, 5,054 stars) |

### 7.1 Frappe HR in detail (closest open analogue to what obop needs) [V]

- Doctypes in `hrms/hr` (abridged): Appointment Letter, Appraisal Cycle/Appraisal/KRA/Goal, Attendance, Attendance Request, Employee Checkin, Employee Onboarding/Separation (+ templates), Employee Promotion/Transfer, Full and Final Statement, Exit Interview, Expense Claim (+ advance, taxes), Job Opening/Applicant/Offer/Requisition, Interview (+ feedback, types), Leave Type/Policy/Allocation/Application/Ledger Entry/Encashment/Block List, Shift Type/Assignment/Request/Schedule, Staffing Plan, Training Program/Event, Travel Request, Vehicle Log, Overtime Slip/Type, Employee Grievance, Employee Skill Map.
- Payroll doctypes (`hrms/payroll`): Salary Component (type, formula, condition, abbreviation, payment-days dependency, tax applicability, accrual and arrear flags, per-company accounts), Salary Structure, Salary Structure Assignment (per employee, with base), Salary Slip, Payroll Entry (batch), Payroll Period, Additional Salary, Income Tax Slab, Employee Tax Exemption Declaration/Proof, Employee Benefit Application/Claim, Gratuity (rule + slabs), Retention Bonus, Salary Withholding, Payroll Correction, Arrear.
- Reports: salary register, bank remittance, PF deductions, professional tax deductions, income tax computation, CTC break-up.
- Lesson [I]: Frappe models India-shaped tax (slabs, exemption declaration and proof cycle) in generic doctypes; the generic/regional split is `hrms/regional/india`. Same approach as Odoo (generic structure plus localization). GPL-3.0 (not LGPL) matters if obop wanted to copy anything; clean-room is the brief anyway.

---

## 8. Commercial comparison

| Vendor | Model | Verified facts |
|---|---|---|
| **Gusto** | US-focused payroll + benefits + HR for SMB; Embedded Payroll API for software platforms | Began as ZenPayroll in 2011 (Wikipedia marks the founding date "citation needed"; service launched Dec 2012); $9.3B valuation is from a June 2025 employee tender offer (not early 2026; $9.6B in 2023), passed $1B annual revenue (TechCrunch, 7 May 2026), bought Guideline (announced Aug 2025, ~$600M) and Mosey (multi-state payroll compliance, Apr 2026) ([Wikipedia, secondary](https://en.wikipedia.org/wiki/Gusto,_Inc.)). Embedded payroll requires commercial, security and implementation review before production ([docs](https://docs.gusto.com/embedded-payroll/docs/introduction)). **Pricing not verified** (page bot-walled). |
| **Rippling** | Unbundled HR/IT/finance modules on one data layer; sold by custom quote | "Tell us what services you need, and we'll send you a custom quote" ([pricing](https://www.rippling.com/pricing)) [V]; $16.8B valuation (May 2025), ~$1B revenue by July 2026, Rippling Data Cloud launched June 2026, active litigation with Deel ([Wikipedia, secondary](https://en.wikipedia.org/wiki/Rippling_(company))) |
| **BambooHR** | HRIS first; payroll is an add-on | Published tiers: Core $10, Pro $17, Elite $25 per employee per month (FAQ starting points $10.25/$17.50); 25 or fewer employees flat $250/$425/$650; ATS limited by openings (5/25/50); **payroll US-only add-on**, time and attendance separate add-on, global employment "powered by Remote" ([pricing](https://www.bamboohr.com/pricing/)) |
| **Deel** | Global contractor/EOR/payroll | $49/contractor/month, $325 contractor-of-record, $599/EOR employee/month, $125 US PEO; "130+ countries" EOR; global payroll unpriced; ~250 entities, $1B ARR mid-2025, $17.3B valuation Oct 2025, 13 acquisitions by end 2025 ([pricing](https://www.deel.com/pricing/), [Wikipedia, secondary](https://en.wikipedia.org/wiki/Deel_(company))) |
| **Check** (embedded payroll API) | API: tax calculation, filing, money movement, deductions, garnishments, tax notices | Claims 1M+ employees paid, 50k+ businesses, $15B+/yr; pricing by sales contact ([checkhq.com](https://checkhq.com/)) |
| **Finch** (unified employment API) | Read/write connector to 250+ HR/payroll providers | Starter $65/month per connection (limited to 24 providers and 15 connections, read-only, organization and payroll data); Pro/Premier by quote; products: Organization, Payroll, Deductions (write), Documents (beta) ([pricing](https://www.tryfinch.com/pricing)) |

Market pattern [I]: even unicorns split "HRIS" from "payroll" commercially and geographically (BambooHR US-only payroll; Gusto US-only; Deel/Remote via local entities). No vendor ships one uniform global payroll engine; they either own in-country entities or buy local providers (Deel acquired PayGroup, PaySpace, Safeguard Global's payroll division per Wikipedia).

---

## 9. Privacy, GDPR and access control for HR data

### 9.1 Legal baseline

- **GDPR Art. 9**: special categories (racial/ethnic origin, political opinions, religion/philosophy, trade-union membership, genetic data, biometric data for unique identification, health, sex life/orientation) are prohibited unless an exception applies; Art. 9(2)(b) permits processing necessary for employment, social security and social protection obligations only when authorised by EU/member-state law or a collective agreement with safeguards; Art. 9(4) lets states add conditions for genetic, biometric, health data ([Art. 9](https://gdpr-info.eu/art-9-gdpr/)) [V].
- **GDPR Art. 88**: member states may set more specific employment rules (recruitment, contract performance, work planning, equality, health and safety, termination) and they must include measures on transparency, intra-group transfers and *monitoring systems at work* ([Art. 88](https://gdpr-info.eu/art-88-gdpr/)) [V]. Consequence [I]: HR privacy rules are national; the product needs configurable retention, consent/legal-basis and works-council-style switches per country.
- **Where HR features touch special categories [I from verified fields]:** sick-leave type and attachments (health), union membership or deductions, religion-driven holiday calendars, diversity/EEO fields, work-permit and passport data, dependants (spouse and children on `hr.version`), photo and biometric attendance (Odoo 20 photo at check-in; eSSL/Mantra biometric device feeds [V]), GPS/IP capture on attendance [V], and bank accounts.
- **EU AI Act**: AI used for recruitment, CV sorting and worker management is Annex III high-risk; the Digital Omnibus (in force 27 July 2026) moved those obligations to **2 December 2027** (systems embedded in regulated products such as lifts or toys: 2 August 2028; the Act as a whole became applicable 2 August 2026) ([Commission AI Act page](https://digital-strategy.ec.europa.eu/en/policies/regulatory-framework-ai)) [V]. Any ATS scoring/ranking feature in obop inherits that classification.
- **EU Pay Transparency Directive (EU) 2023/970**: transposition deadline 7 June 2026, pay-range disclosure to candidates, no salary-history questions, worker right to pay information, gender pay-gap reporting by employer-size tiers, joint pay assessment when the gap exceeds 5% **[U; EUR-Lex and Commission pages could not be fetched, also not re-verified in the fact-check pass (unverified)]**. If the 7 June 2026 deadline is right it has already passed as of 2026-10-01, so national transposition status is a live question (unverified). Verify before building; it affects recruitment (salary range on postings) and reporting.
- Fine precedent: H&M, Hamburg DPA, about EUR 35M (Oct 2020) for recording employees' private details accessible to managers [U, recalled; (unverified) after a second fact-check attempt, the EDPB and Wikipedia pages fetched did not contain it]. The cited enforcement tracker lists 3,295 actions and EUR 7.56B total ([tracker](https://www.enforcementtracker.com/)) [V, count only].

### 9.2 What Odoo and peers do (and the gaps)

- Odoo: three-tier groups (Employee / Officer / Administrator in `hr`, parallel groups in Time Off), field-level `groups=` on sensitive columns, a separate public SQL view, manager-hierarchy record rules, `tracking` on sensitive field changes via chatter, `privacy_lookup` (search/erase a person's data) and `data_recycle` in Community; no native audit trail of reads (OCA `auditlog` fills part of this) [V].
- OrangeHRM gates audit trail and MFA behind paid Advanced [V]; Frappe uses role permissions plus "permission levels" on fields [U].
- Gap worth exploiting [I]: field-level ACL in application code is easy to bypass through reports, exports, related fields or search (Odoo's own comment in `hr_employee.py` notes groups "don't naturally propagate to related fields", which forced redefinition). obop should enforce in the data layer.

### 9.3 Recommended control model for obop [I]

1. Data classification tag on every HR column: `public_directory`, `internal`, `confidential_hr`, `special_category`, `financial`.
2. Separate read models: `employee_public` (directory, org chart, presence) vs full record.
3. PostgreSQL row-level security plus column grants (or views) keyed on role, company, and reporting line; do not rely only on handler checks.
4. Purpose-bound access: payroll role, HR-business-partner role, line-manager role, auditor role, "break-glass" with reason and alert.
5. Append-only access and change audit (who read salary, who exported), effective-dated history for compensation changes (`date_version` idea).
6. Field encryption for national IDs, bank accounts and health notes (envelope keys per tenant/company), with crypto-shredding on erasure.
7. Retention schedules per data class and country (applicant data, payroll records, leave medical certificates), automatic anonymisation jobs.
8. Exports (payslips, DSN, RTI payloads) go through an outbox with approval and logging.

---

## 10. Rust/implementation notes (payroll-relevant ecosystem) [V, crates.io 2026-10-01]

- Money/decimals: `rust_decimal` 1.43.0 (151M downloads); `rusty-money` 0.5.1; `iso_currency` 0.7.1. Payroll needs per-jurisdiction rounding modes and never floats.
- Rule expressions: `cel` 0.14.5 (crate of `cel-rust`, MIT, pushed today; the older `cel-interpreter` 0.10.0 is stale), `rhai` 1.26.1 (Apache-2.0, 5.7k stars), `evalexpr` 13.1.0. CEL is non-Turing-complete and deterministic, which suits auditable payroll formulas; Rhai is more powerful and riskier. Odoo's `safe_eval` Python is the anti-pattern to avoid.
- Dates/time: `chrono` 0.4.45; `icu` 2.3.1 for locale-aware formatting.
- PDF payslips: `typst` 0.15.1 (library) or `printpdf` 0.12.8.
- Also verified on crates.io on 2026-10-01 in the fact-check pass: `wasmtime` 49.0.1, `jiff` 0.2.37, `sqlx` 0.9.0, `axum` 0.8.9. Re-check before pinning.

---

## Confidence & gaps

- **High confidence:** Odoo 20.0 state and module lists (read from source), the Community/Enterprise HR boundary at the module level, `hr.version`/`hr.time.rule` model changes, payroll rule semantics (from official docs and OCA source), OCA payroll status, GitHub stats and license tags, Deel/BambooHR/Finch/Check published facts, UK/US/EU-AI-Act regulatory facts fetched from official pages.
- **Medium:** Odoo pricing figures (page parsed ambiguously); country-pack depth (I only saw tables of contents for several localisation pages, truncated bodies); Wikipedia-derived company figures (secondary); statements about Germany and France rely on Wikipedia (secondary).
- **Not verified / open:** Gusto pricing and plan details (Cloudflare wall, not bypassed); Rippling list prices (custom quote only); BMF Programmablaufplan details; DSN technical norm version; EU Pay Transparency Directive specifics and national transposition status; H&M fine; whether Enterprise 13.0+ ever shipped FR/DE/UK packs (Community 9.0 to 12.0 verified: FR, BE, IN only); Enterprise payroll internals (private code); Horilla's payroll depth; Frappe HR field-permission claims; whether Odoo's US module e-files anywhere.
- No Odoo Enterprise code was read or is recommended for reuse; everything about Enterprise payroll comes from public documentation.

---

## Implications for obop

**Scope decisions**

1. **MUST ship an HR core at Odoo-Community parity**, tractable because it is plain CRUD plus workflow: employee master with effective-dated versions, departments and org chart, jobs, work locations, employee types, departures and reasons, time off (types, allocations, accrual plans with levels and carry-over, mandatory days, public holidays), attendance (kiosk/check-in with optional geo/photo off by default), recruitment/ATS (job positions, stages, applicants, refusal reasons, talent pool, source tracking), expenses posting to the obop accounting module as vendor bills, timesheets posting to analytic lines, skills and resume, fleet link, onboarding/offboarding as checklist plans.
2. **MUST model employee data as effective-dated versions** (`date_version` pattern from Odoo 19/20, implemented with Postgres range/exclusion constraints), not as a mutable row plus chatter log. Payroll correctness and retro-pay depend on it.
3. **MUST treat payroll as three separable layers**: (a) a *generic payroll engine* in core, (b) *country packs* as separately versioned, separately licensed/owned artifacts, (c) *provider connectors*. Core never contains country tax code.
4. **MUST NOT promise statutory filing in core.** Filing (RTI, DSN, Dimona/DmfA, 941/W-2, ELSTER, PF/TDS) belongs to packs or providers. Odoo itself delegates UK/MY/NZ/SG to Employment Hero and offers ADP/SD Worx exports; copy that stance.
5. **MUST define the pack interface before writing any pack**: input = classified time intervals + versioned employee data + typed one-off inputs + dated rule parameters; output = payslip lines (amount, quantity, rate, total, category, YTD), employer-cost lines, journal-line templates, filing payloads with validation. Expose it as a stable Rust trait plus a serialisable (JSON/CBOR) schema so packs can be WASM-sandboxed or out-of-process.

**Payroll engine design**

6. **MUST use declarative, data-driven rules** (structure type, structure, rule with sequence/category/condition/amount, dated rule parameters, categories, input types) mirroring Odoo's six building blocks, with `total = amount * quantity * rate / 100`, YTD, and explicit `result_qty/rate/name` outputs, because that vocabulary is what payroll practitioners and localisers already know [Odoo docs].
7. **MUST NOT embed arbitrary code in the database.** Use a non-Turing-complete expression language (CEL via the `cel` crate is the leading candidate) for conditions and formulas, decimals via `rust_decimal`, and reserve Rust/WASM for complex statutory algorithms (e.g. a tax-table pack).
8. **MUST make every payslip line explainable and reproducible**: store inputs, rule version, parameter values and evaluation trace (publicodes-style explanations), and make runs deterministic and re-runnable with retro adjustments and cancel/undo (Payroll Engine's design goals).
9. **SHOULD support multiple runs per period, forecast/preview runs, batches (pay runs), salary attachments (garnishments, advances), off-cycle pay, and deferred time off**, matching the Odoo 19 feature list.
10. **MUST post payroll to accounting via rule-level debit/credit templates** (with split-by-name, exclude-from-net, employee-on-line, analytic, optional batched lines) and produce payment files (SEPA pain.001) through the accounting/banking module.
11. **SHOULD ship a conformance-test harness for packs**: golden cases from regulator examples (IRS Pub. 15-T worksheets, HMRC test service, DSN conformance) run in CI; a pack without golden cases cannot be marked stable.

**Packs and integrations**

12. **SHOULD ship one reference pack first, chosen for data availability rather than market size**: France via the MIT-licensed `modele-social` rules as a rules *source* (publicodes to obop-rule translation or a sidecar evaluator), or a US federal-only pack following Pub. 15-T. Do not start with Germany or the UK: annual algorithm/spec cadence plus certification make them poor first packs.
13. **MUST ship provider connectors as a first-class path**: a `PayrollProvider` interface with import of payslip journals/pay runs (Employment Hero pattern: draft entries, weekly sync, account mapping), employee/deduction push, and payslip document pull. Candidates: Check and Gusto Embedded for US (both require commercial onboarding), Finch as a unified read/write layer (starter $65/connection/month), and local providers per country.
14. **COULD build an "export to payroll bureau" generator** (work entries by `external_code`, as Odoo does for SD Worx/ADP), which is cheap and covers many SMBs.
15. **COULD support third-party country packs under their own licence** (marketplace model) because packs carry legal liability; obop core stays Apache/LGPL-style per the project's choice, packs may be commercial.

**Privacy and access control**

16. **MUST implement the control model in 9.3** (classification tags, narrow public read model, Postgres RLS, append-only access audit, field encryption with crypto-shredding, retention jobs). This is a differentiator: Odoo lacks a read-audit and OrangeHRM paywalls audit and MFA.
17. **MUST default sensitive capture off** (geo/IP/photo/biometric attendance, sick-leave reasons) and gate it per country and company with a DPIA checklist in the UI; biometrics fall under GDPR Art. 9.
18. **MUST keep AI features out of hiring decisions by default** (ranking, auto-rejection); if added, flag as high-risk Annex III with human oversight; obligations start 2 Dec 2027 after the Digital Omnibus.
19. **SHOULD plan for EU pay transparency** (salary ranges on job postings, no salary-history field in application forms, pay-gap reporting) after verifying the directive text and national transpositions.

**Self-service and commercial model**

20. **SHOULD model ESS as a first-class, non-licensed-seat surface** (payslips, time off, expenses, directory, profile), unlike Odoo's internal-user/Light-User approach, because HR ESS user counts equal headcount.
21. **COULD leave Appraisals/OKRs, Planning/shift scheduling, Referrals and Lunch for a later wave** (all Odoo Enterprise, Frappe has KRA/Appraisal Cycle/Goal and Shift Schedule as prior art), but reserve the data-model hooks (goals, skills evolution, shifts tied to work-entry types).

**Open-source positioning**

22. **MUST NOT copy Frappe HR (GPL-3.0) or Odoo Enterprise payroll code**; use them as behavioural references only. OCA payroll is derived from Odoo code and notes it is outside the OCA CLA; treat as reference, not source.

---

## Verification log

Fact-check pass dated 2026-10-01 (adversarial; independent re-fetch of GitHub API/raw files, Odoo docs RST and live pages, gov.uk, IRS, Commission, vendor pricing pages, crates.io, npm, Wikipedia wikitext for secondary company facts).

**Confirmed as stated: 53** (grouped count: 11 of the 12 flagged load-bearing claims, plus about 42 unflagged version/date/price/licence/star/deadline/status statements). Highlights: Odoo `20.0` default branch with `version_info (20, 0, 0, FINAL, 0, '')` and `LGPL-3`; release notes "Released September 2026"; Community addon lists on 17.0 to 20.0 (`hr_contract` only on 17.0/18.0, `hr.version` from 19.0, no `hr_payroll`/`hr_appraisal`/`planning`); `salaries.rst` six building blocks, `total = amount * quantity * (rate/100)` and the Python context; 23 payroll localizations in the 19.0 docs (18.0 list has 24 incl. Malaysia; 12 dedicated pages); Employment Hero for MY/NZ/SG/UK; US 19.0 table of exactly 20 jurisdictions; OCA/payroll (123 stars, `payroll` 19.0.1.0.1 Beta LGPL-3, 19.0 one module, 20.0 empty, 18.0 six modules, not under the OCA CLA); Frappe HRMS (GPL-3.0, `required_apps = ["frappe/erpnext"]`, 8,851 stars, v16.20.1 on 2026-10-01, only `india` and `united_arab_emirates` regional); OrangeHRM v5.9 (2026-06-28), Advanced-only audit trail/MFA/payroll integrations; Sentrifugo last commit 2017-04-17; Payroll Engine MIT, v1.0.0 2026-04-21, v1.0.1 2026-05-07, 125 stars; gov.uk FPS/EPS deadlines and HMRC RTI spec dates (2027-28 spec 16 Sep 2026); IRS Pub 15-T (2026) P.L. 119-21 and W-4/W-4P checkboxes; EU AI Act Annex III delay to 2 Dec 2027, Omnibus in force 27 Jul 2026; Deel, BambooHR, Finch, Rippling, Check, Gusto docs; GDPR Art 9 and 88; enforcement tracker 3,295 / EUR 7.56B; all crates.io versions in section 10; OpenFisca, `modele-social`, Kimai, Horilla, Rhai, `cel` stats.

**Corrections**

| # | Was | Now | Source |
|---|---|---|---|
| 1 | Odoo's open Payroll left Community "years ago [U: I recall v11]" | `hr_payroll` (LGPL-3) is in Community on 9.0, 10.0, 11.0 and 12.0 and absent from 13.0 (released Oct 2019); OCA/payroll's oldest branch is 13.0 | https://github.com/odoo/odoo/tree/12.0/addons/hr_payroll ; https://github.com/odoo/odoo/tree/13.0/addons ; https://www.odoo.com/page/release-notes |
| 2 | Whether older Odoo shipped FR/DE/UK packs is [U] | Community 9.0 to 12.0 shipped `l10n_fr_hr_payroll`, `l10n_be_hr_payroll`, `l10n_in_hr_payroll`; no DE/UK payroll module. Enterprise 13.0+ remains (unverified) | https://github.com/odoo/odoo/tree/12.0/addons |
| 3 | Odoo 20 adds "NYC/Yonkers plus 11 more states" (implying 11 new) | Release notes name 11 states, but GA, IA, MS, NJ are already in the 19.0 docs table; net-new vs that table is 7 (KS, KY, MI, MO, SC, TN, UT), about 27 state-level jurisdictions incl. DC, plus NYC/Yonkers local taxes [I] | https://www.odoo.com/odoo-20-release-notes ; https://www.odoo.com/documentation/19.0/applications/hr/payroll/payroll_localizations/united_states.html |
| 4 | Section 6: US coverage "grew from 20 jurisdictions to 31+" | "about 27" (7 net-new states) plus NYC/Yonkers | same as #3 |
| 5 | Gusto "$9.3B valuation early 2026" | $9.3B is from a June 2025 employee tender offer ($9.6B in 2023); Wikipedia URL is now `Gusto,_Inc.`; ZenPayroll founding date is "citation needed" (launched Dec 2012) | https://en.wikipedia.org/wiki/Gusto,_Inc. |
| 6 | Finch Starter "24 providers, read-only" | Also capped at 15 connections; organization and payroll data | https://www.tryfinch.com/pricing |
| 7 | Default recruitment stage "Initial Qualification" | Docs say "Initial Qualification", but 20.0 data file `hr_recruitment_data.xml` names it "Qualification" | https://github.com/odoo/odoo/blob/20.0/addons/hr_recruitment/data/hr_recruitment_data.xml |
| 8 | Odoo pricing "19.90 / 35.90 on annual billing" | Prices confirmed with a Yearly/Monthly toggle (24.90 / 44.90 other option) and a strikethrough 31.10 / 56.90 with footnote "discount is valid for 12 months"; which option is which stays an interpretation | https://www.odoo.com/pricing |
| 9 | EU AI Act delay stated only for 2 Dec 2027 | Added: product-embedded systems 2 Aug 2028; Act generally applicable 2 Aug 2026 | https://digital-strategy.ec.europa.eu/en/policies/regulatory-framework-ai |
| 10 | "A crates.io rate limit prevented checking `wasmtime`, `jiff`, `sqlx`, `axum`" | Now checked 2026-10-01: wasmtime 49.0.1, jiff 0.2.37, sqlx 0.9.0, axum 0.8.9 | https://crates.io/api/v1/crates/{name} |
| 11 | EU Pay Transparency Directive and H&M fine labelled [U] | Kept as unverified and labelled "(unverified)"; added that the 7 June 2026 deadline (if right) has already passed | see unresolved |

**Unresolved**

- EU Pay Transparency Directive (EU) 2023/970 details (7 June 2026 transposition, reporting tiers, 5% joint assessment, salary-history ban) and national transposition status: EUR-Lex is behind a bot wall, Commission pages were JS-only or 404 (unverified).
- H&M / Hamburg DPA EUR 35M fine (Oct 2020): not found on the EDPB or Wikipedia pages fetched (unverified).
- Whether Odoo 20's four overlapping states (GA, IA, MS, NJ) only gain county/city rules, or whether the 19.0 docs table simply lagged the 19.0 software; "about 27" is an inference.
- Odoo Enterprise 13.0+ FR/DE/UK payroll packs (private code).
- OrangeHRM Starter/Advanced split is read from the pricing table's check/cross icon images (no text legend); consistent across rows but still an inference.
- Payroll Engine "no bundled country regulations": based on the main README and its Examples/Tests list only; satellite repos were not audited.
- Secondary-source items left as labelled: Gusto, Rippling and Deel company figures (Wikipedia), Germany Lohnsteuer rules (de.wikipedia), France DSN (fr.wikipedia; which employers file on the 5th vs the 15th not confirmed), BMF Programmablaufplan, Gusto list prices, Horilla feature depth, Frappe field-permission behaviour.
