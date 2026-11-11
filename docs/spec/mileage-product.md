# Mileage: product requirements (PROPOSED)

- **Status:** Draft 0.1. PROPOSED. This is the product view of Mileage: the problem, who it is for, what it must do, how it competes and what is still to decide. The technical design of the `mileage` package and the app stays in [mobile.md](mobile.md), which this document does not replace. Nothing here is ratified unless it cites an owner ruling (OR-n) in [DECISIONS.md](../DECISIONS.md).
- **Date:** 2026-11-11
- **Scope:** Mileage as a product built on obop: automatic recording of business trips, sorting them into business and private, approval, turning approved kilometres into money for the employee, and the logbook a tax authority accepts. Country rules are data, not code.

**Reading notes.** Decisions are cited by ID and owner rulings as OR-n. Facts are marked as in [mobile.md](mobile.md): **verified** means a page was read on the date above; **search** means it comes from a search result summary and the page itself was not opened; **general knowledge** means no source was checked and it must be verified before anything depends on it. Nothing here is legal or tax advice. Prices of other products come from third-party comparison sites, disagree with each other and must be checked on each vendor's site.

## 1. Summary

Employers in Sweden, Germany and many other countries may pay employees a tax-free amount for every kilometre they drive for work in their own (or privately leased) car, and an employee with a company car pays less benefit tax when a logbook shows the private use. Both need a trip log that the tax authority accepts, and today most people keep it by hand, late and incomplete, and lose money.

Mileage records trips on the phone without Start and Stop buttons, sorts them into business and private with the obop calendar, contacts and fleet, lets a manager approve them, and turns approved kilometres into an expense claim or a payroll line in the same obop instance. The raw route stays on the phone; the employer sees only what the tax rules require.

## 2. The problem

### 2.1 Who loses money today

| Situation | What the rules allow | What goes wrong without a good log |
|---|---|---|
| Employee drives **their own or a privately leased car** for work | A tax-free mileage allowance per kilometre. Sweden 2026: 25 SEK per mil (10 km) for an own car; a privately leased car counts as an own car (search, skatteverket.se). Germany: about 0.30 EUR per km (general knowledge) | No log, no tax-free payment: either nothing is paid, or the employer pays it as salary and about a third goes to tax |
| Employee has a **company car** that is also used privately | The benefit is taxed; a logbook can show that private use is small. Sweden 2026: the tax-free fuel amount for a company car is 12 SEK per mil, 9.50 SEK for a fully electric one (search, skatteverket.se). Germany: a Fahrtenbuch instead of the flat 1 % rule (general knowledge) | Without a logbook the authority may assume private use (search, skatteverket.se), and the employee pays more tax than needed |
| **Finance / payroll** | Pays allowances and books them | Collects paper or spreadsheet logs from everyone, checks them by hand, types the amounts into payroll |

### 2.2 Why the existing habit fails

- People forget to write trips down, and reconstruct them weeks later from memory.
- Kilometres are rounded by eye, which is a risk in a tax audit.
- Business and private trips are mixed, and nobody sorts them.
- The log and the payroll live in different tools.

## 3. Goals and non-goals

**Goals**

1. The driver does nothing during a normal working day except confirm a summary (one tap per day or per week).
2. Every approved business kilometre becomes money for the employee in the same obop instance, without retyping.
3. The log meets what the tax authority of the configured country asks for.
4. The employer never sees where an employee drove privately, and never sees raw routes after approval.
5. A new country is a new set of configuration data, not new code.

**Non-goals (for now)**

- Fleet telematics, live vehicle tracking, route optimisation, dispatch maps.
- A dongle or other hardware in the car (a later option, see 9).
- Filing anything with a tax authority directly.
- A desktop app: recording needs the phone (desktop is the obop back office).

## 4. Personas

| Persona | Needs | Group (from [mobile.md](mobile.md) 2) |
|---|---|---|
| **Driver** (employee, salesperson, technician, consultant) | Get paid for business kilometres without effort; keep private trips private | `mileage.driver` |
| **Approver** (manager or finance) | See and approve the month's business trips quickly; trust the numbers | `mileage.reviewer` |
| **Payroll / finance** | Approved amounts arrive as claims or payroll lines, ready to pay and book | `expenses` and payroll groups |
| **Administrator** | Set the country, rates, vehicles; handle stuck trips and lost phones | `mileage.admin` |
| **Sole trader** (later) | The same log for their own tax deduction, no approver | Open (11, item 7) |

## 5. How it works

### 5.1 A day for Anna

1. Anna has a meeting with a customer at 14:00 in the obop calendar, at the customer's address from Contacts.
2. She gets into her car. She presses nothing. The phone notices it is in a vehicle and starts recording.
3. She arrives; the phone notices the drive has ended and stops recording.
4. On the phone, the trip is matched: it ended near the meeting address at 13:55, so it is proposed as **business**, purpose "Meeting with <customer>".
5. In the evening a notification shows the day's trips: two business, one private. Anna checks them with one tap.
6. Only the business trips go to obop. The private trip never leaves the phone.
7. At the end of the month her manager approves the month in the obop back office.
8. obop computes kilometres times the country rate and creates one expense claim (or payroll line) for Anna. Finance pays it as usual.
9. The logbook for the month (and the year, with odometer readings for a company car) can be exported in the format of the country.

### 5.2 How a trip is detected (no Start and Stop)

| Signal | Use |
|---|---|
| Motion sensors (iOS Core Motion, Android activity recognition): "in a vehicle" | Main start and stop signal, low battery use |
| The car's Bluetooth, CarPlay or Android Auto connecting and disconnecting | Stronger signal where the car has it |
| A vehicle dispatched to the driver in `fleet` (`dispatch`, `return_to_pool`) | Opens and closes the window in which trips of a company car are expected |
| A calendar event with a location | A hint that a trip is likely; also used to classify |
| Manual Start and Stop | Always available as a fallback |

Detection is not done by AI. It needs an own native module on both platforms: `expo-location` has no motion detection, and the library that has it is excluded as the default (OR-34 d, [mobile.md](mobile.md) 4.1).

### 5.3 How a trip is classified

Rules first, on the phone:

| Rule | Result |
|---|---|
| The trip ends near the location of a calendar event of the driver, close to its start time | Business, purpose = the event title |
| The trip ends at the address of a contact or lead in obop | Business, purpose = the contact |
| Home to the usual workplace and back | Commute (not a business trip in Sweden or Germany; general knowledge for Germany) |
| Outside working hours, on a weekend, or no match | Proposed as private |

AI is optional and only for unclear trips (for example, proposing a purpose text). If it is used, it runs on the device: the SpaceCorps project `open-apple-models` runs Apple's on-device model with tool calling on iOS; an Android equivalent is not ready. The driver always confirms; nothing is approved by a rule or a model alone (D18).

### 5.4 What stays on the phone and what is sent

| Data | Where |
|---|---|
| Raw GPS points of a private trip | Phone only, deleted after a short time |
| Raw GPS points of a business trip | Sent so the server can check the distance (see below), deleted after approval |
| A business trip's date, start and end address, kilometres, purpose, vehicle | Sent and kept: the tax rules require it |
| Calendar and contact matching | Done on the phone |

**A decision to make (11, item 3).** The built design sends every point to the server and computes the distance there, because a phone's number cannot be trusted ([mobile.md](mobile.md) 5.4: a prior app once recorded a trip of 8,666 km). Sending nothing but totals is more private but trusts the phone. The proposal above is the middle: points of business trips only, checked on the server, deleted after approval; private trips never leave the phone.

## 6. Business requirements

| ID | Requirement | Priority |
|---|---|---|
| BR-1 | A trip is recorded without the driver pressing Start or Stop on a normal working day | Must (needs 11, item 1) |
| BR-2 | Manual Start and Stop stay available | Must |
| BR-3 | The server computes the distance; the phone's number is never trusted | Must (built) |
| BR-4 | Each trip is classified as business, private or commute, proposed automatically and confirmed by the driver | Must |
| BR-5 | Private trips and their routes are never sent to the employer | Must |
| BR-6 | Raw points of business trips are deleted a set number of days after approval; distance and addresses stay | Must |
| BR-7 | An approver who is not the driver approves business trips; a driver never approves their own | Must (built) |
| BR-8 | Approved kilometres times the rate of the configured country become an expense claim or a payroll line in the same instance | Must |
| BR-9 | Rates, the unit (km or mil), the currency and the required log fields come from a country configuration, not from code | Must |
| BR-10 | The monthly and yearly logbook can be exported in the format of the configured country (PDF and CSV) | Must |
| BR-11 | A trip can reference a vehicle from `fleet`; for a company car the logbook includes the odometer at the start and end of the year | Should |
| BR-12 | Trips of a dispatched company car are linked to that car automatically | Should |
| BR-13 | Recording is always visible (notification or status indicator) and can be paused | Must |
| BR-14 | Automatic recording can be limited to working hours and is switched on by the driver | Must |
| BR-15 | The app works on Android and iOS; iOS ships first, Android follows | Must (OR-36 requires both platforms and sets no order; iOS first was decided on 2026-11-11, relayed) |
| BR-16 | Every capability exists in the CLI, the REST API and MCP; the app and the back office are clients | Must (OR-4, D80) |
| BR-17 | Trips can be imported from a Google Timeline export | Built (`obop import timeline`) |
| BR-18 | No limit on the number of trips per month | Should |
| BR-19 | A published privacy notice states what is stored, where, for how long and why | Must |

## 7. User stories

Each story has acceptance criteria (AC). "Built" marks what exists today.

**US-1 Automatic recording.** As a driver, I want my drives recorded without pressing anything, so that I never forget a trip.
- AC1: Getting into a car and driving starts a recording within a set time; stopping and leaving the car ends it.
- AC2: A walk, a bus or a train ride is not recorded as a car trip.
- AC3: Recording is visible while it runs, and I can pause automatic recording.

**US-2 Manual fallback.** As a driver, I can start and stop a trip by hand when detection misses one. *(Built in the backend; the app is on branches, not merged.)*

**US-3 Proposed classification.** As a driver, I want each trip proposed as business, private or commute with a purpose, so that I only have to confirm.
- AC1: A trip ending near a calendar event's location around its start time is proposed as business, with the event title as purpose.
- AC2: A trip ending at a contact's address is proposed as business.
- AC3: Home to work is proposed as commute.
- AC4: I can change the proposal before confirming.

**US-4 Daily confirmation.** As a driver, I want one summary per day (or week) that I confirm with one tap.
- AC1: Only trips I confirmed as business are sent to obop.
- AC2: Private trips are kept on the phone only and deleted after a set time.

**US-5 Privacy.** As a driver, I want my employer to see only my business trips, never my private routes.
- AC1: No request from the app contains points of a private trip.
- AC2: After approval, the raw points of a business trip are deleted; distance, addresses and purpose stay.

**US-6 Approval.** As an approver, I want to see a month of business trips per driver with distance, addresses, purpose and any integrity flags, and approve them together.
- AC1: I cannot approve my own trips. *(Built.)*
- AC2: I can reject a trip with a reason; the driver can correct and resubmit. *(Built.)*

**US-7 Getting paid.** As a driver, I want my approved kilometres paid with my salary or as an expense claim, without filling in anything.
- AC1: After approval, one claim (or payroll line) per driver per month is created with kilometres times the rate of the country and the vehicle type.
- AC2: The claim lists the trips it covers.

**US-8 Finance.** As finance, I want approved mileage to arrive as normal claims or payroll lines I can pay and book as usual.

**US-9 Logbook export.** As a driver or finance, I want the month's and the year's logbook in the format the tax authority of my country accepts.
- AC1: The export holds the fields the country configuration requires (for Sweden: date, start, destination, purpose, distance; for a company car also the odometer readings).

**US-10 Company car.** As a driver of a company car, I want trips linked to the car I was given in `fleet`, and the yearly odometer readings in the log.

**US-11 Country.** As an administrator, I choose the country of the instance (or of an employee) and get its rates, unit, currency and log format without any code change.

**US-12 Import.** As a driver, I can import past drives from Google Timeline and classify them. *(Built as `obop import timeline`.)*

**US-13 Lost phone.** As an administrator, I revoke a lost phone; its unsent trips are lost, and it can no longer send. *(Device revocation built, D64 amendment.)*

## 8. Country configuration

The core is the same everywhere. A country is a small set of data, shipped the same way as other country packs ([LOCALIZATION.md](../LOCALIZATION.md)).

| Setting | Sweden (search, skatteverket.se, 2026) | Germany (general knowledge, to verify) |
|---|---|---|
| Rate, own or privately leased car | 25 SEK per mil | about 0.30 EUR per km |
| Rate, company car | 12 SEK per mil; 9.50 SEK fully electric | different rules (1 % rule or logbook) |
| Unit and currency | mil (10 km), SEK | km, EUR |
| Commute | not a business trip; a separate deduction above a yearly threshold | not a business trip; separate commuter allowance |
| Required log fields | date, start, destination, purpose, distance; odometer for a company car | date, odometer start and end, destination, purpose, business partner; the log must not be changeable afterwards |
| Where the money goes | expense claim or Swedish payroll (`payroll_se`) | expense claim or German payroll (not built) |

Amounts above the tax-free rate are taxable salary in both countries (general knowledge). A rate change is effective-dated, like any country data.

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

1. **One system, no integration.** Trip, approval, payment and accounting in one database (the obop principle). The others glue a logbook to payroll through integrations.
2. **Better classification.** Matching with the obop calendar, contacts and fleet, not only working hours and frequent places.
3. **Privacy by design.** Classification on the phone; private trips never leave it; raw routes deleted after approval.
4. **Price.** obop is free and open (OR-1, OR-2); no trip limits. Market leaders' prices are rising.
5. **Open surface.** Every capability in CLI, API and MCP: an agent can prepare the month for a human to approve.

### 9.3 What we lack today

1. Automatic detection (decision needed, 11 item 1; native module to build).
2. Kilometres to money (BR-8): `expenses` has the claim flow but no rate calculation; `payroll_se` has no line for a tax-free allowance.
3. A country configuration and a logbook export (BR-9, BR-10); none is filled in yet.
4. A merged mobile app: the app is on the branches `feat/mileage-app`, `feat/mileage-ios` and `feat/mileage-sv`.
5. For Germany, an unchangeable log: the record immutability of D77 is not built.

### 9.4 Verdict

As a stand-alone app in the stores, Mileage would compete with products that have years of polish, automatic detection and many countries. As a part of obop for small and medium businesses, it fills a gap nobody else fills: from the trip to the payment and the books in one system, private by design, and free. The product should be positioned as the second, and needs three things to be credible: automatic detection (or at least one-tap start from a calendar reminder), kilometres to money, and a logbook export for the first country.

## 10. Phases

| Phase | Content | Depends on |
|---|---|---|
| 0 (built) | Backend: trips, points, server distance, review, monthly totals; device sign-in; Google Timeline import | done ([mobile.md](mobile.md) 8.5, 8.6) |
| 1 | The iOS app (merge the branches); manual Start and Stop; calendar reminder with one-tap start. The Android app follows with the same features | OR-34 as it stands |
| 2 | Classification on the phone (calendar, contacts, fleet); daily confirmation; private trips stay on the phone | 11, item 3 |
| 3 | Kilometres to money (expense claim, then payroll); the first country configuration and logbook export | 11, items 4 and 5 |
| 4 | Automatic start and stop (motion, Bluetooth, fleet dispatch) | 11, item 1 |
| Later | On-device AI for unclear trips; sole traders; more countries; a car device or OBD option | — |

## 11. Open questions for the owner

1. **Automatic start and stop.** The lead said in chat on 2026-11-10 (relayed, not a quotation) that automatic start and stop is fine, with data minimised before any AI, a clear statement of what is stored, and a public page with the data terms. This changes OR-34 c (auto-stop only after legal sign-off) and covers automatic start, which no ruling covers. It must be recorded in DECISIONS.md as an owner ruling before it is built. Proposed wording: automatic start and stop are allowed under BR-5, BR-6, BR-13, BR-14 and BR-19, with a legal check before the first release rather than before development.
2. **Free or paid.** obop is completely free (OR-1, OR-2). A paid store app or subscription (an idea of about 5 USD a month was mentioned) would be a separate product and needs the owner's decision; until then Mileage is free.
3. **Privacy model.** All points to the server (built), totals only, or the middle of 5.4.
4. **First country.** Sweden or Germany (obop's first market is Germany, OR-7).
5. **Where the money goes first.** Expense claim (`expenses`, closer) or payroll line (`payroll_se`, Sweden only).
6. **Retention.** How many days raw points of a business trip are kept after approval ([mobile.md](mobile.md) 15).
7. **Sole traders.** Whether a driver without an approver (own deduction) is in scope.

## 12. Risks

| Risk | Mitigation |
|---|---|
| Store review of background location (Google Play declaration and video; Apple "Always" permission) | Disclosures in the app and on the website; tap-to-start path as fallback ([mobile.md](mobile.md) 4.2) |
| Employee tracking is sensitive (GDPR; in Germany works councils, general knowledge) | Private trips stay on the phone; raw routes deleted; recording visible and pausable; legal check before release |
| Detection misses or splits trips; Android vendors kill background apps | Manual fallback; "keep running" guide; field test on real phones |
| Battery drain | Motion sensors first, GPS only while driving |
| Wrong rates or log format | Rates are dated data per country, checked against the authority's pages before release |
| The phone's distance is manipulated | Server computes the distance from the points of business trips (BR-3) |

## 13. Sources

- Skatteverket, amounts 2026: https://www.skatteverket.se/privat/skatter/beloppochprocent/2026 (search)
- Skatteverket, travel and car allowances: https://www.skatteverket.se/foretag/arbetsgivare/lonochersattning/traktamenteochbilersattning (search)
- Skatteverket, logbook: https://www.skatteverket.se/privat/skatter/arbeteochinkomst/formaner/bilforman/korjournal (search)
- Driversnote pricing: https://driversnote.com/pricing (search)
- MileIQ pricing (third party): https://frontdeskreview.com/software/mileage-tracking/mileiq/ (search)
- Everlance and TripLog: https://www.g2.com/compare/everlance-vs-triplog (search)
- Pleo mileage: https://blog.pleo.io/sv/hantera-milersattning-med-pleo (search)
- Trackson and Fortnox: https://www.fortnox.se/integrationer/integration/trackson-ab/trackson-korjournal (search)
- Vimcar: https://www.leasingmarkt.de/magazin/technik/vimcar-fahrtenbuch (search)
- Odoo third-party GPS mileage: https://store.softhealer.com/blog/odoo-store-9/fleet-dashboard-v19-15277 (search)
- obop: [mobile.md](mobile.md), [mobile-host.md](mobile-host.md), [people.md](people.md) 4.5, `packages/fleet.toml`, `packages/expenses.toml`, `crates/modules/mileage`, `crates/obop/src/import_timeline`
