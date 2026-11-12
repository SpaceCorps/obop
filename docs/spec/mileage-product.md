# Mileage: product requirements (PROPOSED)

- **Status:** Draft 0.2. PROPOSED. This is the product view of Mileage: the problem, who it is for, what it must do, how it competes and what is still to decide. The technical design of the `mileage` package and the app stays in [mobile.md](mobile.md), which this document does not replace. Nothing here is ratified unless it cites an owner ruling (OR-n) in [DECISIONS.md](../DECISIONS.md). Section 11.1 lists product positions discussed with the owner on 2026-11-10 to 2026-11-12; they are relayed, not quoted, unconfirmed, and PROPOSED until each is recorded as an owner ruling (OR-n).
- **Date:** 2026-11-12 (first draft 2026-11-11)
- **Scope:** Mileage as a product built on obop: automatic recording of business trips, sorting them into business and private, approval, turning approved kilometres into money for the employee, and the logbook a tax authority accepts. Country rules are data, not code.

**Reading notes.** Decisions are cited by ID and owner rulings as OR-n. Facts are marked as in [mobile.md](mobile.md): **verified** means a page was read on the date above; **search** means it comes from a search result summary and the page itself was not opened; **general knowledge** means no source was checked and it must be verified before anything depends on it. Nothing here is legal or tax advice. Prices of other products come from third-party comparison sites, disagree with each other and must be checked on each vendor's site.

## 1. Summary

Employers in Sweden, Germany and many other countries may pay employees a tax-free amount for every kilometre they drive for work in their own (or privately leased) car, and an employee with a company car pays less benefit tax when a logbook shows the private use. Both need a trip log that the tax authority accepts, and today most people keep it by hand, late and incomplete, and lose money.

Mileage records trips on the phone without Start and Stop buttons, sorts them into business and private with the obop calendar, contacts and fleet, lets a manager approve them, and turns approved kilometres into an expense claim in the same obop instance. Private trips never leave the phone; the employer sees only business trips, and the raw route of those is deleted after a set time.

### 1.1 Product shape

| Part | Who uses it | What it does | State |
|---|---|---|---|
| **obop phone app** with Mileage preinstalled | Employees (drivers) | One obop app for the phone: sign in to the company's obop instance, then the Mileage module records, classifies and sends trips. Further modules can be added to the same app later | Proposed (11.1); the host design in [mobile-host.md](mobile-host.md) is PROPOSED too. Sign-in, shared native components and Swedish texts exist on the branches `feat/mileage-app`, `feat/mileage-ios`, `feat/mileage-sv` |
| **obop back office** (browser) | Approvers, finance, administrators | Approve trips, set rates and vehicles, pay claims, export the logbook. Its screens are generated from the module (D46); no separate desktop app | The Mileage app tile and the trip and point lists exist |
| **obop server**, module `mileage` | Nobody directly | Stores trips, checks the distance, governs the states, computes totals. The only place where business logic lives | Built ([mobile.md](mobile.md) 8.6) |

## 2. The problem

### 2.1 Who loses money today

| Situation | What the rules allow | What goes wrong without a good log |
|---|---|---|
| Employee drives **their own or a privately leased car** for work | A tax-free mileage allowance per kilometre (rates in 8.1). In Sweden a privately leased car counts as an own car (search, skatteverket.se) | No log, no tax-free payment: either nothing is paid, or the employer pays it as salary and about a third goes to tax |
| Employee has a **company car** that is also used privately | The benefit is taxed; a logbook can show that private use is small. In Sweden a logbook is not required by law, but without one the authority may assume private use (search, skatteverket.se). Germany: a Fahrtenbuch instead of the flat 1 % rule (general knowledge) | The employee pays more benefit tax than needed |
| **Finance / payroll** | Pays allowances and books them | Collects paper or spreadsheet logs from everyone, checks them by hand, types the amounts in |

### 2.2 Why the existing habit fails

- People forget to write trips down, and reconstruct them weeks later from memory.
- Kilometres are rounded by eye, which is a risk in a tax audit.
- Business and private trips are mixed, and nobody sorts them.
- The log and the payment live in different tools.

## 3. Goals and non-goals

**Goals**

1. The driver does nothing during a normal working day except confirm a summary (one tap per day or per week).
2. Every approved business kilometre becomes money for the employee in the same obop instance, without retyping.
3. The log meets what the tax authority of the configured country asks for; Sweden first.
4. The employer never sees private trips, and raw routes of business trips are deleted after a set time.
5. A new country is configuration data, not new code.

**Non-goals**

- Monitoring working time, performance or where an employee is outside working hours. The Swedish data-protection authority (IMY) does not accept GPS monitoring of working time or of position outside working time (search, imy.se and legal commentary); an electronic logbook for the tax authority is an accepted purpose. Mileage data is used for the logbook and the allowance only.
- Fleet telematics, live vehicle tracking, route optimisation, dispatch maps.
- A device in the car (OBD dongle) for now.
- Filing anything with a tax authority directly.
- A separate desktop app: managers and finance use the obop back office in the browser.

## 4. Personas

| Persona | Needs | Group (from [mobile.md](mobile.md) 2) |
|---|---|---|
| **Driver** (employee, salesperson, technician, consultant) | Get paid for business kilometres without effort; keep private trips private | `mileage.driver` |
| **Approver** (manager or finance) | See and approve the month's business trips quickly; trust the numbers | `mileage.reviewer` |
| **Finance** | Approved amounts arrive as expense claims, ready to pay and book | groups of the `expenses` package |
| **Administrator** | Set the country, rates, vehicles and retention; handle stuck trips and lost phones | `mileage.admin` |
| **Sole trader** | The same log for their own tax deduction, with no approver | Later (11.1: companies first) |

## 5. How it works

### 5.1 A day for Anna

1. Anna has a meeting with a customer at 14:00 in the obop calendar, at the customer's address from Contacts.
2. She gets into her car. She presses nothing. The phone notices it is in a vehicle and starts recording.
3. She arrives; the phone notices the drive has ended and stops recording.
4. On the phone, the trip is matched: it ended near the meeting address at 13:55, so it is proposed as **business**, purpose "Meeting with Example AB".
5. In the evening a notification shows the day's trips: two business, one private. Anna checks them with one tap.
6. Only the business trips go to obop. The private trip never leaves the phone.
7. At the end of the month her manager approves the month in the obop back office.
8. obop computes kilometres times the rate and creates one expense claim for Anna in `expenses`. Finance pays it as usual.
9. The logbook for the month and the year can be exported in the Swedish format.

### 5.2 How a trip is detected (no Start and Stop)

| Signal | Use |
|---|---|
| Motion sensors (iOS Core Motion, Android activity recognition): "in a vehicle" | Main start and stop signal, low battery use |
| The car's Bluetooth, CarPlay or Android Auto connecting and disconnecting | Stronger signal where the car has it |
| A vehicle dispatched to the driver in `fleet` (`dispatch`, `return_to_pool`) | Opens and closes the window in which trips of a company car are expected |
| A calendar event with a location | A hint that a trip is likely; also used to classify |
| Manual Start and Stop | Always available as a fallback, and the alternative to automatic recording that a driver can choose |

Detection is not done by AI. It needs a native module of our own on both platforms: `expo-location` has no motion detection, and the library that has it is excluded as the default (OR-34 d, [mobile.md](mobile.md) 4.1).

### 5.3 How a trip is classified

Rules first, on the phone:

| Rule | Result |
|---|---|
| The trip ends near the location of a calendar event of the driver, close to its start time | Business, purpose = the event title |
| The trip ends at the address of a contact or lead in obop | Business, purpose = the contact |
| Home to the usual workplace and back | Commute (not a business trip in Sweden or Germany) |
| Outside working hours, on a weekend, or no match | Proposed as private |

AI is optional and only for unclear trips (for example, proposing a purpose text). If it is used, it runs on the device: the SpaceCorps project `open-apple-models` runs Apple's on-device model with tool calling on iOS; an Android equivalent is not ready. The driver always confirms; nothing is approved by a rule or a model alone (D18).

### 5.4 What is kept, where and for how long (proposed, 11.1)

| Data | Phone | Server |
|---|---|---|
| Raw points of a **private** trip | Until the daily confirmation, at most 7 days | Never sent |
| Raw points of a **business** trip | At most 7 days | Sent so the server computes and checks the distance; deleted **90 days after approval** by default, configurable per company |
| The logbook of a business trip: date, start and end address, kilometres, purpose, vehicle, odometer where needed | — | Kept as long as the bookkeeping rules require. In Sweden accounting records are kept seven years after the end of the calendar year in which the financial year ended (Bokföringslagen chapter 7, search); that a logbook behind an allowance counts as such a record is a reading of search results, not verified |
| Calendar and contact matching | Done on the phone | — |

This keeps the built rule that the server computes the distance and never trusts the phone's number ([mobile.md](mobile.md) 5.4: a prior app once recorded a trip of 8,666 km), but only for trips the employer pays for. The retention job that deletes raw points is not built (ISSUES M5).

[mobile.md](mobile.md) 5.6 and its open question 4 (15) still treat what happens to private trips as open; mobile.md is out of step with this section until the owner rules on it.

### 5.5 Technology

The stack is set in [mobile.md](mobile.md) and [CLAUDE.md](../../CLAUDE.md); this section only points to it.

| Part | Technology |
|---|---|
| Phone app | React Native with Expo (SDK 57, or 58 if it passes the cooldown, OR-34 e), TypeScript, `expo prebuild`; iOS built with Xcode on a Mac. Started through `vp run` (the mobile-toolchain exception, OR-34 b); pnpm only |
| Calls to obop | `@obop/client` (types generated from the OpenAPI document) and the mobile OAuth sign-in (D64 amendment) |
| Trip detection | An Expo native module of our own (Swift, Kotlin) for motion and Bluetooth; `expo-location` for GPS while driving |
| Distance rules | `@obop/trip-core`, shared test vectors with the server |
| Local storage | `expo-sqlite`, encrypted at rest with the key in the platform keystore ([mobile.md](mobile.md) 3) |
| On-device AI (optional) | `open-apple-models` (Swift) through an Expo native module; iOS only, and per its README only iOS 27 and later on devices with Apple Intelligence, so the rules of 5.3 must work without it |
| Live Activity | A widget extension in SwiftUI with ActivityKit and WidgetKit, added to the Expo app through a native module; designed with SwiftUI previews and the simulator (5.6) |
| Server | The Rust module `crates/modules/mileage`; rates and claims in the `mileage` module and the `expenses` package |

Xcode runs only on macOS. The TypeScript parts can be written on any system and tried on an Android emulator; iOS builds, the Live Activity and the native modules need a Mac. Development builds use the developer's own free Apple ID and a Mac the developer arranges; signing, TestFlight builds for device tests and publishing are done from the owner's Mac and Apple account (11.1, Q37).

### 5.6 The Live Activity and devices

Device and version facts here are general knowledge and are checked when the work starts.

| Presentation | When | Shows |
|---|---|---|
| Compact | Phone unlocked, the island collapsed | Elapsed time on the left, distance on the right |
| Expanded | A long press on the island | Time, distance, Stop and Pause |
| Lock screen | Phone locked | As expanded, with the buttons |
| Minimal | Another activity runs at the same time | The obop icon |

| Device or system | Result | Setting needed |
|---|---|---|
| iPhone with a Dynamic Island (14 Pro and later, except budget models without one) | Island and lock screen | No |
| iPhone without a Dynamic Island (11 to 14, 14 Plus, SE) | Lock screen and banner only; the same code | No |
| iOS 16.1 to 16.x | Live Activity without buttons; below the minimum of BR-24 | — |
| iOS 26 and later | Liquid Glass for system elements; own components use a glass view where available | No |
| Earlier iOS | Plain look | No |

The system chooses the presentation; the app has no switch for it.

## 6. Business requirements

| ID | Requirement | Priority |
|---|---|---|
| BR-1 | A trip is recorded without the driver pressing Start or Stop on a normal working day | Proposed, needs the ruling of 11.2 (OR-34 c still requires legal sign-off before auto-stop) |
| BR-2 | Manual Start and Stop stay available, and a driver can choose them instead of automatic recording | Must |
| BR-3 | The server computes the distance; the phone's number is never trusted | Must (built) |
| BR-4 | Each trip is classified as business, private or commute, proposed automatically and confirmed by the driver | Must |
| BR-5 | Private trips and their points are never sent to the server | Must |
| BR-6 | Retention as in 5.4: raw points 7 days on the phone, 90 days after approval on the server (configurable per company); the logbook as long as the bookkeeping rules of the country require | Must |
| BR-7 | An approver who is not the driver approves business trips; a driver never approves their own | Must (built) |
| BR-8 | Approved kilometres become one expense claim per driver per month in the `expenses` package, which then follows its own flow to payment. A payroll line is later | Must |
| BR-9 | Rates come from configuration (8.2): the official tax-free rate of the country by default; a company can override it, per employee and per vehicle type; tiers by yearly distance; dated changes | Must |
| BR-10 | A rate above the tax-free rate is allowed: the app warns, and the claim splits the amount into a tax-free part and a taxable part | Must |
| BR-11 | The monthly and yearly logbook can be exported (PDF and CSV); Sweden first, Germany second | Must |
| BR-12 | A trip can reference a vehicle from `fleet`; for a company car the logbook holds the odometer at the start and end of the year and of each trip, as Skatteverket lists | Should |
| BR-13 | Trips of a dispatched company car are linked to that car automatically | Should |
| BR-14 | Recording is always visible (notification or status indicator) and the driver can pause it | Must |
| BR-15 | Automatic recording only in the working hours the driver sets, and only after the driver switches it on | Must |
| BR-16 | The app works on Android and iOS; iOS ships first, Android follows | Must (OR-36 requires both platforms and sets no order; iOS first is proposed, 11.1) |
| BR-17 | Every capability exists in the CLI, the REST API and MCP; the phone app and the back office are clients | Must (OR-4, D80) |
| BR-18 | Trips can be imported from a Google Timeline export | Built (`obop import timeline`) |
| BR-19 | No limit on the number of trips per month | Should |
| BR-20 | A published privacy notice tells employees what is stored, where, for how long and for which purpose, before recording starts | Must |
| BR-21 | Mileage data is not used to monitor working time, performance or position outside working hours | Must |
| BR-22 | A trip being recorded shows as a Live Activity on iOS with the presentations of 5.6 (compact, expanded on a long press, lock screen, minimal), with the time, the distance and Stop and Pause buttons; on Android the ongoing notification shows the same | Must for the release, after the first prototype |
| BR-23 | The first prototype uses a minimal standard iOS look; the release uses the obop design system (the `ui-native` tokens and components) with Liquid Glass where the iOS version has it and a plain fallback elsewhere | Must for the release |
| BR-24 | The app runs on iOS 17 and later (the first version with buttons in a Live Activity) | Must (proposed minimum) |

## 7. User stories

Each story has acceptance criteria (AC). "Built" marks what exists today.

**US-1 Automatic recording.** As a driver, I want my drives recorded without pressing anything, so that I never forget a trip.
- AC1: Getting into a car and driving starts a recording within a set time; stopping and leaving the car ends it.
- AC2: A walk, a bus or a train ride is not recorded as a car trip.
- AC3: Recording is visible while it runs; I can pause it; it only runs in my working hours.

**US-2 Manual fallback.** As a driver, I can start and stop a trip by hand, and I can choose manual recording instead of automatic. *(Built in the backend; the app is on branches.)*

**US-3 Proposed classification.** As a driver, I want each trip proposed as business, private or commute with a purpose, so that I only have to confirm.
- AC1: A trip ending near a calendar event's location around its start time is proposed as business, with the event title as purpose.
- AC2: A trip ending at a contact's address is proposed as business.
- AC3: Home to work is proposed as commute.
- AC4: I can change the proposal before confirming.

**US-4 Daily confirmation.** As a driver, I want one summary per day (or week) that I confirm with one tap.
- AC1: Only trips I confirmed as business are sent to obop.
- AC2: Private trips are deleted from the phone after the confirmation.

**US-5 Privacy.** As a driver, I want my employer to see only my business trips.
- AC1: No request from the app contains points of a private trip.
- AC2: 90 days after approval (or the company's setting) the raw points of a business trip are gone from the server; date, addresses, distance and purpose stay.

**US-6 Approval.** As an approver, I want to see a month of business trips per driver with distance, addresses, purpose and integrity flags, and approve them together in the back office.
- AC1: I cannot approve my own trips. *(Built.)*
- AC2: I can reject a trip with a reason; the driver can correct and resubmit. *(Built.)*

**US-7 Getting paid.** As a driver, I want my approved kilometres paid without filling in anything.
- AC1: After approval, one expense claim per driver per month is created in `expenses` with the kilometres times my rate.
- AC2: If my rate is above the tax-free rate, the claim shows a tax-free part and a taxable part.
- AC3: The claim lists the trips it covers.

**US-8 Finance.** As finance, I want approved mileage to arrive as normal expense claims I can pay and book as usual.

**US-9 Swedish logbook.** As a driver or finance, I want the month's and the year's logbook in a form Skatteverket accepts.
- AC1: The export holds, per trip, the date, start and end address, kilometres and purpose (purpose not needed for private trips), and for a company car the odometer at the start and end of each trip and of the year, the registration number and the year.

**US-10 Company car.** As a driver of a company car, I want trips linked to the car I was given in `fleet`.

**US-11 Rates.** As an administrator, I want the official rate of my country preset, and to change it for the company, an employee or a vehicle type, with a date from which it applies.
- AC1: A new instance in Sweden starts with 25 SEK per mil for an own car and 12 SEK (9.50 SEK electric) for a company car.
- AC2: Setting a rate above the tax-free rate shows a warning that the difference is taxable.

**US-12 Import.** As a driver, I can import past drives from Google Timeline and classify them. *(Built as `obop import timeline`.)*

**US-13 Lost phone.** As an administrator, I revoke a lost phone; it can no longer send. *(Device revocation built, D64 amendment.)*

## 8. Countries and rates

### 8.1 Default rates, own or privately leased car, 2026

All from search results; each must be checked against the authority's page before it ships as a default.

| Country | Tax-free rate | Notes |
|---|---|---|
| Sweden | 25 SEK per mil (10 km) | Company car: 12 SEK per mil, 9.50 SEK fully electric. Commute is not a business trip; a separate deduction above a yearly threshold |
| Germany | 0.30 EUR per km | Motorcycle 0.20 EUR. Commute is separate (Entfernungspauschale, 0.38 EUR per km from 2026) |
| Netherlands | 0.25 EUR per km | One source still lists 0.23 (2025) |
| Finland | 0.55 EUR per km | 0.59 in 2025 |
| Norway | 3.50 NOK per km | Same for electric cars |
| Denmark | 3.94 DKK per km up to 20,000 km a year, 2.28 DKK above | Needs tiers (BR-9) |

### 8.2 Rate configuration

The core is the same everywhere; a country is a small set of data shipped like other country packs ([LOCALIZATION.md](../LOCALIZATION.md)).

| Setting | Level |
|---|---|
| Rate per vehicle type (own car, company car, electric company car, motorcycle) | Country default → company override → employee override |
| Tiers by yearly distance | Country default (Denmark), company may change |
| Unit (km or mil) and currency | Country |
| Tax-free limit | Country; used for the warning and the split of BR-10 |
| Required logbook fields and export format | Country |
| Valid from | Every rate and limit is dated |

Amounts above the tax-free rate are taxable salary (general knowledge for all countries listed).

## 9. Competitive analysis

### 9.1 The market

| Group | Examples | Price (third-party sites, unverified) | Strong | Weak |
|---|---|---|---|---|
| Automatic mileage apps | MileIQ, Driversnote, Everlance, TripLog | MileIQ: 40 drives a month free, then about 13.99 USD a month (5.99 in 2024). Driversnote: 15 trips free, about 11 USD per driver a month. Everlance and TripLog about 8 to 17 USD | Automatic detection from motion sensors; polished apps. Driversnote sorts trips by working hours | A separate tool: data in the vendor's cloud, payroll and accounting only through integrations. MileIQ and Everlance focus on the US |
| Swedish logbook apps | Trackson, TripTrackIt, Fleetr | Mostly on request | Built for Skatteverket. Trackson sends an approved logbook to Fortnox payroll | Narrow tools; accounting still through an integration |
| Logbook with a device in the car (Germany) | Vimcar | About 24.90 EUR a month with an OBD plug | Reads the real odometer | Needs hardware; a user review says its TÜV certificate is about data protection, not tax compliance |
| Finance and expense platforms | Pleo, Fortnox, Bokio | Part of their plans | Mileage, payment and accounting in one place; Pleo states it meets Skatteverket's requirements and has a GPS variant | Mainly start and end addresses typed in, not automatic recording |
| Odoo (obop's reference) | Odoo Expenses | Part of Odoo | Mileage claims from a rate | No GPS mileage of its own; only third-party modules, at least one not available on Odoo Online |

### 9.2 What Mileage has that they do not

1. **One system, no integration.** Trip, approval, expense claim and accounting in one database (the obop principle). The others glue a logbook to payroll through integrations.
2. **Better classification.** Matching with the obop calendar, contacts and fleet, not only working hours and frequent places.
3. **Visible and in reach.** A Live Activity in the Dynamic Island shows the trip being recorded and stops it with one tap (BR-22).
4. **Privacy by design.** Private trips never leave the phone; raw routes are deleted after 90 days; the data is not used to monitor people (BR-21).
5. **Price.** obop is free and open (OR-1, OR-2); no trip limits. Market leaders' prices are rising.
6. **Open surface.** Every capability in CLI, API and MCP: an agent can prepare the month for a human to approve.

### 9.3 What we lack today

1. Automatic detection (proposed, needs the ruling of 11.2; the native module is to build).
2. Kilometres to money (BR-8, BR-10): `expenses` has the claim flow, but no rate configuration and no link from approved trips to a claim.
3. The Swedish logbook export (BR-11) and the odometer fields (BR-12).
4. A merged phone app: it is on the branches `feat/mileage-app`, `feat/mileage-ios` and `feat/mileage-sv`.
5. The retention job (ISSUES M5).
6. For Germany later, an unchangeable log: the record immutability of D77 is not built.

### 9.4 Verdict

As a stand-alone app in the stores, Mileage would compete with products that have years of polish, automatic detection and many countries. As a part of obop for small and medium businesses, it fills a gap nobody else fills: from the trip to the payment and the books in one system, private by design, and free. The product is positioned as the second, and needs three things to be credible: automatic detection (or at least one-tap start from a calendar reminder), kilometres to money, and the Swedish logbook.

## 10. Phases

| Phase | Content | Depends on |
|---|---|---|
| 0 (built) | Backend: trips, points, server distance, review, monthly totals; device sign-in; Google Timeline import | done ([mobile.md](mobile.md) 8.5, 8.6) |
| 1 | The obop phone app on iOS with Mileage, in a minimal standard iOS look: sign-in to the instance, module list, manual Start and Stop, calendar reminder with one-tap start. Goal: prove that trips are recorded, sent and computed correctly. Android follows with the same features | OR-34 as it stands |
| 1b | The Live Activity of BR-22 (Dynamic Island and lock screen on iOS, the ongoing notification on Android) | The first working prototype of phase 1 |
| 2 | Classification on the phone (calendar, contacts, fleet); daily confirmation; only business trips sent; retention of 5.4 | — |
| 3 | Rate configuration with Swedish defaults; approved trips become expense claims with the tax-free and taxable split; the Swedish logbook export with odometer fields | — |
| 4 | Automatic start and stop (motion, Bluetooth, fleet dispatch) | The ruling recorded in DECISIONS.md (11.2) |
| 4b | The obop design (BR-23): `ui-native` tokens and components, Liquid Glass where available, the Live Activity in the same style | Phase 1 working |
| Release | Phases 1 to 4b complete, BR-22 done, the checks of 11.3 done. A prototype with phase 1 may go to testers earlier; the public release is the complete app | 11.2 recorded before phase 4 |
| Later | A payroll line instead of an expense claim; the German logbook; sole traders; on-device AI for unclear trips; more countries; an OBD option | — |

## 11. Decisions

### 11.1 Proposed, relayed, unconfirmed (2026-11-10 to 2026-11-12)

None of these rows is in DECISIONS.md, and the register treats a decision as made only when it is an owner ruling (OR-n). Each row stays PROPOSED until the owner confirms it and it is recorded there; this document then cites the OR number.

| Topic | Proposal |
|---|---|
| Automatic start and stop | Allowed, under BR-5, BR-6, BR-14, BR-15, BR-20 and BR-21; data is minimised before any AI, what is stored is stated, and a public page sets out the data terms. A legal check comes before the first release, not before development (2026-11-10) |
| Phone app | One obop phone app with Mileage preinstalled; the back office in the browser for approvers and finance (1.1) |
| Platforms | iOS first, Android follows (BR-16) |
| Price | Free (OR-1, OR-2). The only planned paid service is SpaceCorps hosting (OR-22) |
| Rates | Configurable, with the official rate of each country as default (8) |
| Above the tax-free rate | Allowed, with a warning and a split claim (BR-10) |
| Privacy | Only business trips are sent; private trips stay on the phone (5.4) |
| Retention | Phone 7 days; server 90 days after approval by default, configurable; logbook per bookkeeping rules (5.4) |
| Money | Expense claim in `expenses` first; payroll line later (BR-8) |
| First country | Sweden for the logbook; Germany second (BR-11) |
| Sole traders | Later; companies first |
| Apple account and release | Development with the developer's own free Apple ID; signing, TestFlight and publishing from the owner's account at the end (5.5) |
| Order of work | A manual Start and Stop prototype with a minimal iOS look first; automatic start and stop, the Live Activity (BR-22) and the obop design (BR-23) after it; the public release only when phases 1 to 4b and the checks of 11.3 are done (10) |

### 11.2 To record in DECISIONS.md

The proposal on automatic start and stop (11.1) would change OR-34 c (auto-stop only after legal sign-off) and covers automatic start, which no ruling covers. Until the owner rules on it and it is recorded in DECISIONS.md as an OR-n, OR-34 c stands and phase 4 is not built. The other rows of 11.1 are recorded the same way when the owner confirms them.

### 11.3 To check before release (not owner questions)

- The rates of 8.1 against each authority's page.
- Whether a logbook behind an allowance is accounting information that must be kept seven years in Sweden (Skatteverket guidance VL00-5).
- IMY's current guidance on positioning employees: informing them in advance, offering manual recording as an alternative, the retention period.

## 12. Risks

| Risk | Mitigation |
|---|---|
| Store review of background location (Google Play declaration and video; Apple "Always" permission) | Disclosures in the app and on the website; tap-to-start path as fallback ([mobile.md](mobile.md) 4.2) |
| Employee positioning is sensitive (GDPR; IMY has ruled against detailed positioning without information or an alternative, Wihlborgs 2014, search; in Germany works councils, general knowledge) | BR-2, BR-5, BR-14, BR-15, BR-20, BR-21; legal check before release (11.3) |
| Detection misses or splits trips; Android vendors kill background apps | Manual fallback; "keep running" guide; field test on real phones |
| Battery drain | Motion sensors first, GPS only while driving |
| Wrong rates or log format | Rates are dated data per country, checked before release (11.3) |
| The phone's distance is manipulated | The server computes the distance from the points of business trips (BR-3) |

## 13. Sources

- Skatteverket, amounts 2026: https://www.skatteverket.se/privat/skatter/beloppochprocent/2026 (search)
- Skatteverket, travel and car allowances: https://www.skatteverket.se/foretag/arbetsgivare/lonochersattning/traktamenteochbilersattning (search)
- Skatteverket, logbook: https://www.skatteverket.se/privat/skatter/arbeteochinkomst/formaner/bilforman/korjournal (search)
- Skatteverket, guidance on accounting records (VL00-5): https://www4.skatteverket.se/download/18.22f626eb14fb326a4b6da0a/1698823437809/VL00-5-rakenarkiv.pdf (search)
- Bokföringslag (1999:1078): https://data.riksdagen.se/dokument/sfs-1999-1078.html (search)
- IMY decision Wihlborgs 2014: https://www.imy.se/globalassets/dokument/beslut/2014-02-11-wihlborgs-gps.pdf (search)
- Lawline, GPS in the car as monitoring: https://lawline.se/answers/far-arbetsgivare-anvanda-gpsen-i-bilen-som-overvakning (search)
- Germany, Kilometerpauschale 2026: https://www.lexware.de/wissen/mitarbeiter-gehalt/wissenswertes-zur-kilometerpauschale/ and https://www.taxmaro.com/post/kilometerpauschale-2026 (search)
- Finland: https://www.perk.com/uk/blog/mileage-allowance-finland/ (search)
- Norway: https://www.perk.com/uk/blog/mileage-allowance-norway/ (search)
- Denmark: https://www.bdo.dk/getattachment/6a75b0c0-0844-48f1-90d6-53a2ecd1393e/Tax-free-mileage-allowances_2026.pdf?lang=en-GB (search)
- Netherlands: https://www.rydoo.com/compliance/netherlands/netherlands-mileage/ (search)
- Driversnote pricing: https://driversnote.com/pricing (search)
- MileIQ pricing (third party): https://frontdeskreview.com/software/mileage-tracking/mileiq/ (search)
- Everlance and TripLog: https://www.g2.com/compare/everlance-vs-triplog (search)
- Pleo mileage: https://blog.pleo.io/sv/hantera-milersattning-med-pleo (search)
- Trackson and Fortnox: https://www.fortnox.se/integrationer/integration/trackson-ab/trackson-korjournal (search)
- Vimcar: https://www.leasingmarkt.de/magazin/technik/vimcar-fahrtenbuch (search)
- Odoo third-party GPS mileage: https://store.softhealer.com/blog/odoo-store-9/fleet-dashboard-v19-15277 (search)
- obop: [mobile.md](mobile.md), [mobile-host.md](mobile-host.md), [people.md](people.md) 4.5, `packages/fleet.toml`, `packages/expenses.toml`, `crates/modules/mileage`, `crates/obop/src/import_timeline`
