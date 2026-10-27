# Mobile: React Native apps and the mileage package

- **Status:** Draft 0.1. OR-34 is RATIFIED: React Native (Expo) apps are allowed as obop clients, with the mobile-toolchain exception; manual start and stop is the baseline and auto-stop comes only after legal sign-off; Transistorsoft is excluded as the default; Expo SDK 57 for now. Everything else in this document, including the scope limits and the technical design, is PROPOSED (D87) until it is built.
- **Date:** 2026-10-27
- **Scope:** What a native mobile app is in obop terms (one more client of the surface that exists), how a trip is recorded while the phone is locked, the `mileage` package (trips, points, a review state machine, server-side validation), the first app (Mileage), the repository layout and tooling it needs, the first slice with its acceptance test, and what is not verified. Every capability is a typed command in the CLI, the REST API and MCP (OR-4, D80); section 7 lists the surface of this area.

**Reading notes.** Decisions are cited by ID, owner rulings as OR-n and owner questions as Qn ([DECISIONS.md](../DECISIONS.md)); names shared with other documents follow [CONVENTIONS.md](../CONVENTIONS.md). Nothing here is built and nothing is legal advice. Three markings state how a fact is known, and they are kept from the design proposal that the lead session wrote on 2026-10-27: **verified** means the lead session read the cited page that day (this document does not re-read it); **general knowledge** means the source was not found or could not be loaded, so the claim must be checked before anything depends on it; **unverified** means it was looked for and not found, or only seen in a search result without reading the page. "Proposed" marks anything the owner has not ratified; names marked Proposed (models, fields, permission groups, commands) are working names. Where this document describes what the platform does today (for example `bulk_create`), it was checked against the code of this repository on the date above. Reasoning from prior art refers to "a prior mileage app" and names no organisation. The owner's words of OR-34 are quoted in D87.

## 1. Purpose and scope

### 1.1 What this area is

obop is a web product: the back office and the PWA are clients of the REST and MCP surface. A PWA cannot keep receiving location updates while the phone is locked (general knowledge), so one use case needs a native client: recording a trip for a mileage report. OR-34 allows React Native (Expo) apps as obop clients. D87 proposes to limit them to first-party apps and to cases a PWA cannot serve (Proposed), and names Mileage as the first. This document specifies that app and the `mileage` package behind it, plus the general rules a later native app would follow.

An app is a client and nothing more. It has no private backdoor: it calls REST and the OpenAPI document, signs in through OAuth, and meets the checks of the one Dispatcher like every other client (D80). Business logic (distance, validation, state changes, permissions) lives on the server; the app captures, buffers and uploads.

### 1.2 What is ruled and what is Proposed

| Item | State |
|---|---|
| React Native (Expo) apps allowed as obop clients | RATIFIED (OR-34 a) |
| The scope limits: first-party apps only, only where a PWA cannot do the job; the PWA stays the default; Tauri mobile is not pursued (Tauri 2 desktop shells for POS stay, D37) | PROPOSED (D87); they came with the proposal and are not in the owner's words |
| The mobile-toolchain exception to "`vp` only" (section 8.2) | RATIFIED (OR-34 b): the owner said "with the mobile-toolchain exception". Its conditions in 8.2 (the tools named, `vp env exec` or `vp run`, pnpm only, `vp check`, `vp test` and `vp build` on the shared packages) are the exception as proposed and recorded |
| Manual start and stop is the baseline; auto-stop only after legal sign-off | RATIFIED (OR-34 c). The proposal adds product and works-council sign-off, and this document requires all three (Proposed) |
| Automatic start of a trip | Not ruled and out of scope (manual start is the baseline); neither OR-34 nor Q41 covers it |
| The Transistorsoft library is excluded as the default | RATIFIED (OR-34 d): the owner's words are "Transistorsoft excluded as the default" |
| The reason (obop is completely free to build and self-host, OR-1 and OR-2; the library's release builds need a paid licence key) and that an optional adapter later is not forbidden | PROPOSED (the proposal's, not in the owner's words) |
| `expo-location` with `expo-task-manager` and a foreground service as the default path; an own native module as the fallback | PROPOSED (D87) |
| Expo SDK 57 (React Native 0.86) for now | RATIFIED (OR-34 e) |
| When to move to SDK 58 (React Native 0.88; an upgrade guide dated 2026-10-21, seen in a search result, page not read, so the dates below are arithmetic on an unverified date). Not ruled: D23 would make it eligible after its 7 to 14 day cooldown (2026-10-28 to 2026-11-04) and OR-8 caps the lag at about three weeks (2026-11-11); "for now" leaves the move to the owner, who is asked again by then | PROPOSED (the lead session's reading of D23) |
| Start of the first slice | The main session relayed on 2026-10-27 that the owner asked for the decisions to be recorded and the first slice to start; relayed, not a quotation |
| Mobile sign-in class, `device` model, device outbox, the `mileage` package, privacy points, repository layout, the first slice, the kernel capabilities of 5.8 | PROPOSED (D87, D13 and D64 amendments); nothing built |
| Apple and Google accounts, store listings, publishing, push, map tiles, a new data class `location`, legal sign-off for auto-stop, Storybook for the app | Open: Q37 to Q42 |

### 1.3 Boundaries with neighbouring areas

| Neighbour | Owns | This area owns |
|---|---|---|
| [people.md](people.md) 4.5 Expenses | `expense_category` of kind `mileage`, rate tables, claims, reimbursement | The confirmed trip and its distance; turning a confirmed trip into an expense line is later (5.9) |
| [services.md](services.md) 4.7 Fleet | Vehicles, odometer readings, assignments; "no telematics or location tracking" | An optional vehicle reference on a trip; the route is recorded only while a driver has started a trip and only by the Mileage app |
| [security-privacy.md](security-privacy.md) | Identity, audit, privacy tooling, the data-class registry owner | The privacy points of 5.6, device revocation in the Customize, Security page |
| [integrations.md](integrations.md), `obop-connectors` (D54) | Egress gate for flows to third parties | Any push, tile or geocoder flow goes through it (Q38, Q39) |
| [ux.md](ux.md) | Back-office views, tokens, the offline contract of the web apps | The app's screens (6); the tokens target for native (8.1) |
| [cli.md](cli.md) | Command grammar and the parity matrix | The commands of 7 |

## 2. Personas and roles

| Persona | Needs | Permission group (names Proposed) |
|---|---|---|
| Driver | Starts and stops a trip, sees own trips, submits or discards them, signs out and revokes own devices | `mileage.driver` |
| Reviewer | Sees submitted trips, confirms or rejects them, reopens a rejected trip; a different group from the driver, and a reviewer does not confirm their own trip (5.7) | `mileage.reviewer` |
| Mileage administrator | Discards stuck trips, sees every device and revokes any. Installing the package is a command of the platform administrator (D45), not of this group | `mileage.admin` |
| Works council or data-protection contact | Reads the privacy notice of the package and the audit of route reads; takes part in the sign-off for auto-stop (Q41) | `audit.reader` for the audit (CONVENTIONS section 4); the sign-off is a human role, not a permission |

Agents are principals narrowed to their owner's groups (D18). They never see raw points by default (5.7).

## 3. What native support means

| Piece | Specification | State today |
|---|---|---|
| Typed client | A shared package `@obop/client` (working name; ARCHITECTURE 10.2 lists `@obop/rpc`, which may be the same thing, see 15): types generated from `/api/openapi.json` with `openapi-typescript` and `openapi-fetch` (already the plan in D15 and ARCHITECTURE 10.4), plus sign-in, refresh and the outbox interface. Used by the web back office and the app. Because models depend on installed modules, the generic layer reads `/api/meta` as the back office does, and the `mileage` package ships its own spec fragment. | The OpenAPI route exists (`crates/kernel/obop-http/src/lib.rs`); no shared client package yet (`web/packages` has `ui` and `e2e`) |
| Device sign-in | The native app flow of RFC 8252: the system browser (ASWebAuthenticationSession on iOS, Custom Tabs on Android), authorization code with PKCE S256, rotating refresh tokens with reuse detection. `obop-oauth` does this for MCP clients (D64) but binds tokens to the `/mcp` audience and refuses them on `/api`. Needed: a client class "mobile" with the `/api` audience, scopes narrowed the way agent tokens narrow groups, and a custom-scheme or universal-link redirect (the D64 amendment). | Server side exists for MCP; the mobile class is missing |
| Passkeys | The sign-in and consent pages already offer passkeys (D65), so they work in the system browser with no native code. Native passkey prompts inside the app need associated-domains and Digital Asset Links files on the instance domain and a matching RP id (unverified: seen in a search result, the page was not read; a library such as react-native-passkeys exists). Not in the first slice. | D65 built |
| Token storage | The refresh token lives in the iOS Keychain or in Keystore-backed storage on Android, for example `expo-secure-store`. A background task must read it while the phone is locked; on iOS the item needs the "after first unlock" accessibility class (general knowledge, to verify). | n/a |
| Device registration and revocation | A built-in model `device` (user, name, platform, app version, push token, created, last seen, revoked). The refresh-token family is bound to the device id; revoking the device ends the family and its queued commands are refused. Devices are listed in Customize, Security beside the Connected apps table. This is the same kind of revocation that D13 and security-privacy 6.3 promise for POS devices; whether the two device records are one model is open (15, item 2). | Not built |
| Offline command outbox | The command outbox of D13 for native apps. For mileage the device queues, in order per trip: the creation of the trip, batches of points, an update of the trip's expected counts, `end` and `submit` (5.3), and, when the driver discards a trip, an update of `discard_reason` and `discard`. Each queued command carries a UUIDv7 `command_id`; replay goes through normal domain services, so permissions are checked at replay. What makes a replay safe is the idempotency of `bulk_create` (D70) as 5.3 states it exactly, and unique constraints (D57); a replayed `update` sets the same values again. `if_version`, the optimistic-lock input that D75 reserves, is not yet accepted by `update` (it is refused as an unknown field); needing it would be a required capability (5.8). Device storage is SQLite (`expo-sqlite`), not AsyncStorage or browser storage: the prior mileage app's research recorded that browser storage "may be cleared under storage pressure" and that it held raw GPS unencrypted. **The device database must itself be encrypted at rest, with its key in the platform keystore** (a requirement, as for the POS edge store in security-privacy 6.3; how this is done with `expo-sqlite` is not checked, so unverified), and it is emptied of uploaded points and wiped when the device is revoked or the person signs out. | D70 built, D75 core built, D13 Proposed |
| Push | Not needed in the first slice: a local notification covers "trip still recording". Later, either a push service (a third party in the data path, a connector egress under D54) or direct calls to the platform push services through `obop-connectors`. Q38. | Connectors exist |
| App updates | Store releases for native code; JavaScript-only updates through `expo-updates`. Expo documents a custom updates server (the Updates protocol) and open-source self-hosted servers exist (unverified: seen in search results, the pages were not read; their maintenance is not checked), so no dependency on Expo's cloud is needed. Native builds run locally with `expo prebuild` plus Gradle and Xcode. | n/a |

## 4. Background location

### 4.1 The options

Sources read by the lead session (verified): the Expo SDK 58 Location documentation, the Android documentation on foreground service types, the Google Play background-location policy and the Transistorsoft README. The Expo pages were those of SDK 58, not SDK 57; SDK 57 is assumed to behave the same (unverified). Apple's documentation page did not render, so the iOS rules in 4.2 are general knowledge.

| Option | What it is | Verdict |
|---|---|---|
| `expo-location` with `expo-task-manager` | `startLocationUpdatesAsync(task, options)` with a top-level `TaskManager.defineTask`; on Android the `foregroundService` option (notification title and body required); config plugin flags `isIosBackgroundLocationEnabled`, `isAndroidBackgroundLocationEnabled` and `isAndroidForegroundServiceEnabled`. It needs a development build, not Expo Go. Limits from the documentation (verified): updates stop if the user terminates the app; Android does not restart a terminated app on a location event; vendor behaviour varies (the documentation links dontkillmyapp.com); `requestBackgroundPermissionsAsync` is "Always" on iOS and "Allow Once" makes it fail silently; on Android 11 and later it opens the system settings page; foreground permission must be granted first. Free, no licence key. | **The proposed default path of the first slice (D87).** Gap: no motion or stationary detection, so continuous GPS drains the battery; mitigate with accuracy, a distance filter and deferred updates (the documentation says deferred updates save battery) and measure on real phones. |
| `react-native-background-geolocation` (Transistorsoft) | Mature, battery-aware, with a built-in SQLite buffer and HTTP uploader. The repository is MIT, but release builds on iOS and Android need a paid licence key (a 30-day trial for release builds; the price is not on the page). | Excluded as the default (OR-34 d). The reason (obop is completely free to build and self-host, OR-1 and OR-2, and a paid key in the build path of the flagship app breaks that) and the remark that an optional adapter later is not forbidden are the proposal's (Proposed). |
| A Capacitor plugin | Not React Native. A prior mileage app used one: it worked on a simulator, then its first real drive exposed seven bugs. | Evidence only, not an option |
| An own native module (Kotlin FusedLocationProvider foreground service, Swift CLLocationManager) as an Expo module | Maximum control, highest cost, two languages to maintain. | The proposed fallback if `expo-location` proves unreliable in the field spike |

### 4.2 Platform rules that shape the design

- **Android 14 and later (verified).** A foreground service of type `location` needs `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_LOCATION` and `android:foregroundServiceType="location"`; `ACCESS_FINE_LOCATION` or `ACCESS_COARSE_LOCATION` must be granted; and a location foreground service cannot be started from the background unless the app holds `ACCESS_BACKGROUND_LOCATION`. Consequence: a trip started by a tap while the app is visible works without any background permission; a trip that starts by itself (geofence, car Bluetooth) needs the background permission and the Play declaration. That is a further reason, by scope, to leave automatic start out (5.9); automatic start is not ruled by OR-34, which covers only auto-stop.
- **Google Play (verified).** Background location is allowed only for core functionality; the Permissions Declaration Form in Play Console; one background-location feature; a short demonstration video of the Android app (30 seconds or less recommended); an in-app disclosure before the runtime prompt that contains the word "location" and a background phrase such as "when the app is closed or not in use", also in the store listing and on the website. Deadlines were not found. **Unverified and important:** whether a foreground service started from a visible app counts as background access for this policy (it probably does not need the background permission, but the sentence was not found). Check it before the store listing is made (Q37).
- **iOS (general knowledge, verify).** `UIBackgroundModes: location`; "When In Use" first, then an upgrade to "Always" (a two-step prompt); the blue status indicator while recording; and the iOS 17 `CLBackgroundActivitySession`, which lets a "When In Use" app that starts recording in the foreground keep receiving locations in the background. If Expo's location module can use it (not verified), a tap-to-start trip needs no "Always" permission and App Review is much lighter. Force-quit stops updates on both platforms (Expo documentation, verified).
- **Battery.** Continuous high-accuracy GPS for a drive is expensive. Vendor battery killers on Android are the main field risk; the app needs a "keep this app running" guide, and the first real-phone test measures drain over a 30-minute drive (9, acceptance item 5).
- **Versions (unverified: seen in a search result, the page was not read).** An upgrade guide dated 2026-10-21 names SDK 58 as current with React Native 0.88, six days before 2026-10-27; SDK 57 is React Native 0.86. The owner chose SDK 57 for now (OR-34 e); when to move is not ruled (1.2).

## 5. The mileage package

The package `mileage` (`packages/mileage.toml`, an `obop.toml` X1 package, Apache-2.0) declares the models, the state machine and the permissions. A small built-in Rust helper crate (`crates/modules/mileage`) is needed only for geodesic distance, which the expression language cannot compute yet. How the helper computes and stores `distance_m` after points arrive is not designed here (a required capability, 5.8).

### 5.1 Models (names Proposed)

| Model | Fields and notes |
|---|---|
| `mileage_trip` | `id`: assigned by the server, like every record: `create` ignores a supplied `id` (it mints a UUIDv7) and `bulk_create` refuses one (D70). `client_trip_id`: text, a UUIDv7 made on the device at Start, **unique per driver** (`unique_with` `driver`, D57); the device's handle on the trip before it knows the server `id`, and the idempotency handle of the trip's creation (5.3). `driver`: the user (the `own` field, D58; it defaults to the creator). `vehicle`: optional, a `fleet_vehicle` of the bundled `fleet` package (which has `odometer_km`). `purpose`: business, private or commute; a private trip keeps only its distance, or is dropped (open, 15). `started_at` and `ended_at`: D72 `Datetime`. `distance_m`: a D72 decimal **computed on the server from the points, never trusted from the client**. `expected_points` and `last_seq`: numbers the device sets before `end` (5.3). `point_count` and `max_seq`: computed from the child points (D57). `discard_reason` and `reject_reason`: text set by `update` before `discard` or `reject`; who may write each, and when, is in 5.7. `confirmed_at`: a D72 `Datetime` set when the trip is confirmed. `integrity`: flags set by the validator (5.4). `state`: a governed select (D48) with the options in 5.2. |
| `mileage_point` | `trip` (a reference to the server `id` of the trip), `seq`, `recorded_at`, `lat`, `lon`, `accuracy_m`, `speed_mps`; unique on (`trip`, `seq`) (`unique_with`, D57). Written through `bulk_create` (5.3). It must be append-only: no update or delete after creation except by the retention job (5.6), and no new point once its trip has left `recording`; that is a requirement on the Proposed immutability of D77 and on a state-conditioned write rule (5.8), not something the generated `update` and `delete` already guarantee. |

### 5.2 The state machine

`state` is a governed field (D48): `create` starts at the first option, `recording`, and only the actions below move it, each with its own group and approval. The proposal names the states and some actions; the arrows, the groups, `discard` and `discard_stuck` are this document's reading (Proposed). An action takes no inputs (MODULES.md 3.4), so a value it needs (a count, a reason) is a field set by `update` first, and the action asserts on it (D56).

| Action | From | To | Who | Assertion |
|---|---|---|---|---|
| `end` | `recording` | `ended` | the driver | the points are complete (`point_count` equals `expected_points` and `max_seq` equals `last_seq`) and `distance_m` is above 0 |
| `submit` | `ended` | `in_review` | the driver | none beyond the state |
| `confirm` | `in_review` | `confirmed` | `mileage.reviewer`, not the driver of the trip (5.7) | none; sets `confirmed_at` |
| `reject` | `in_review` | `rejected` | `mileage.reviewer` | `reject_reason` is not empty |
| `reopen` | `rejected` | `ended` | `mileage.reviewer`, so the driver can correct and submit again | none |
| `discard` | `recording`, `ended` or `rejected` | `discarded` | the driver | `discard_reason` is not empty |
| `discard_stuck` | `recording` or `ended` | `discarded` | `mileage.admin` | none; the audit log records who discarded the trip |

`discarded` is terminal: the record and its audit trail stay, and it never counts. An action has one group, and groups imply nothing, so the administrator's discard is its own action and not `discard` with a second group; the administrator needs no right to write `discard_reason`. A package action takes one `from` value (MODULES.md 3.4), so an action with several source states is declared once per source state, or without `from` and with an assertion on `state`; that is a package detail. Agents can only propose `confirm`, `reject`, `discard` and `discard_stuck` (approval for agents, D18, D48). Whether a mistakenly confirmed trip can go back to `in_review` is open (15). A month lock is not an action of a trip and is out of scope for the first slice (5.9).

### 5.3 Writing a trip from the device

**What `bulk_create` idempotency delivers today (D70, checked in `crates/kernel/obop-commands/src/engine/bulk.rs`).** The key is scoped to the person, the model and the key text (1 to 128 visible ASCII characters). A repeat of the identical request (same rows, same mode) within 24 hours returns the first answer, which lists the created ids, marked `replayed: true`, and creates nothing; the answer is stored in the same transaction as the rows. The same key with different rows or another mode is refused. After 24 hours the key is forgotten, and the same call creates its rows again unless something else refuses them. A dry run neither reads nor stores a key. No other command of the dispatcher has an idempotency key (single `create`, `update` and actions have none); the public-shop checkout and the payment calls have their own, outside the command surface. Limits: at most 1,000 rows a call, and by default 12 calls a minute per person (`OBOP_BULK_PER_MINUTE`).

**The design (Proposed).**

1. **Start.** The device makes a UUIDv7 `client_trip_id`, stores the trip locally, and queues `mileage_trip.bulk_create` with one row and `idempotency_key = "trip:" + client_trip_id`. Creating the trip through `bulk_create` and not `create` is what gives it an idempotency key. A retry inside 24 hours returns the first answer with the server `id`. A retry after that is refused by the unique constraint on (`driver`, `client_trip_id`), and the app then finds the trip with `list` filtered by `client_trip_id` and takes its `id`.
2. **Points.** Point batches wait until the `id` of the trip is known, because `mileage_point.trip` is a reference to it. Each batch is a `mileage_point.bulk_create` with `idempotency_key = client_trip_id + ":" + first_seq + "-" + last_seq`. A batch's rows and range never change once it is queued, so a replay is identical. A replay after 24 hours is refused by the unique constraint on (`trip`, `seq`) and, because the mode is `all_or_nothing`, writes nothing; the outbox treats that refusal as "already stored" after checking the trip's `point_count` and `max_seq`. The app sends batches of many points about once a minute, inside the rate limit, and not point by point.
3. **Stop.** The device queues, behind the outstanding batches, an `update` of the trip (`ended_at`, `expected_points`, `last_seq`) and then `end`. A repeat of `update` is harmless, and a repeat of `end` after success is refused by the state check; the outbox reads the trip and treats `ended` or a later state as done.
4. **Incomplete.** While points are still missing, `end` is refused because `point_count` or `max_seq` does not match. The outbox must tell this retryable refusal from a refusal for good (zero distance, a wrong state); today a failed assertion is a refusal with a message, so a stable, machine-readable code for it is a required capability (5.8). The outbox retries `end` with backoff until it succeeds or the driver discards the trip.
5. **Submit.** `submit` is a command like the others and queues behind `end`.

If the phone has no connection at Stop, nothing is lost: everything waits in the outbox, and the trip stays `recording` on the server until `end` succeeds. A trip that stays `recording` for hours is flagged (5.4) and can be discarded.

### 5.4 Server-side validation

A prior mileage app fixed its logic in the client and enforced nothing on the server; one implausible segment there produced a trip of 8,666 km. The server therefore decides:

| Rule | Value | Reason |
|---|---|---|
| Speed cap | Reject a step above a cap; the prior app used 70 m/s (about 252 km/h). The value is a package constant, Proposed. | An implausible jump must not count |
| Accuracy filter | Drop fixes whose `accuracy_m` is above a threshold. The threshold is open and set after the field test. | Poor fixes add distance |
| No zero-kilometre trips | `end` on a trip whose computed distance is 0 is refused for good with a reason; the driver `discard`s it. | A recorded trip is never saved as 0 km |
| End position | Taken from the last accepted fix. | The app may stop before the final upload |
| Duplicate batch | Within 24 hours the same request returns the first answer; later, the unique constraint refuses it (5.3). | Replays after a lost response |
| Sequence continuity | A gap in `seq` is flagged in `integrity`, not hidden; `end` also checks the counts. | Lost points are visible to the reviewer |
| Open too long | A trip that stays `recording` for hours is flagged. The number of hours is open. | A forgotten trip is the main risk of the manual baseline |
| Distance | Computed on the server from the accepted points with a geodesic helper. | The client's number is never trusted |

The validator and `trip-core` (8.1) share test vectors, so the device and the server agree on filtering and distance.

### 5.5 What counts: the review gate

Totals, the export and the PDF count trips in state `confirmed`, grouped by the month of `confirmed_at` (in the platform zone `OBOP_ZONE`, D72). That is one predicate that every total, list total and export uses; a prior mileage app called it the `reviewed_at` gate. `in_review`, `rejected` and `discarded` trips never count. If a month lock is added later, it must not change which trips a total counts: a lock never removes a confirmed trip from the totals. The reviewer sees the integrity flags before confirming.

### 5.6 Privacy and retention

Not legal advice; obop supports compliance and is not a certification. The points that the research raised:

- Location on the device and in transit is a new processing location for the data-protection impact assessment.
- Raw points are minimised: the retention job (the one exception to "no update or delete of a point") thins or deletes them a number of days after a trip is confirmed, and the distance and, if needed, a simplified route stay. The number of days is open (15). Two other copies must follow the same rule: the encrypted outbox on the device (3) and the change history of the platform (D75), which can keep earlier versions of a record. For `mileage_point` the history entries are erased or thinned together with the point (`history_retention_days` and the erase operation of D75), and a test shows that no raw point survives in history after the retention date (Proposed). D75 history is not active yet, and while the interim class of 5.7 is `confidential_hr` the install gate refuses `history = "none"` for the model (MODULES.md 3.2), so history cannot simply be switched off.
- No third-party geocoder or map tiles by default (Q39); a prior mileage app sent exact coordinates to two public geocoding services. Start and end addresses are typed, or they come through a connector that passes the egress gate (D54).
- Every read of a route is audited (the audit log, [security-privacy.md](security-privacy.md) 4.4).
- Recording is active only while the driver started it and is visible: an Android notification, the iOS indicator.
- **An automatic trip end (auto-stop) ends a trip the driver did not end. The owner's condition is legal sign-off (OR-34 c); the proposal adds product and works-council sign-off. All three come first, and it stays out until then (Q41).** The prior research named guidance of the Swedish data-protection authority (IMY); it was not read here. Manual start and stop is the baseline, with the review gate and the open-trip warning as safeguards. Automatic start is not ruled and is out of scope (5.9).
- Tests use synthetic data only.

### 5.7 Permissions and data class

The proposal names no permission groups. The groups of section 2 and this matrix are this document's reading of it (Proposed).

| Model | Driver (`mileage.driver`) | Reviewer (`mileage.reviewer`) | Administrator (`mileage.admin`) |
|---|---|---|---|
| `mileage_trip` | create (through `bulk_create`) and read own; `update` own **only while the trip is `recording`, `ended` or `rejected`** and only the fields of the table below; no `delete` (the record is kept for audit, and `delete` is refused while points refer to the trip, D49); `in_review`, `confirmed` and `discarded` trips are read-only for the driver; actions `end`, `submit`, `discard` | read trips from `in_review` on (a state-conditioned read restriction, 5.8 item 2); `confirm`, `reject`, `reopen`; writes `reject_reason` while `in_review`. **Not the trips of which the reviewer is the driver** (separation of duties), unless an explicit policy of the business says otherwise | read all; `discard_stuck`; no other override of a `confirmed` trip |
| `mileage_point` | create through `bulk_create` and read own; no `update` or `delete` except by the retention job (5.6) | read, audited | read, audited |
| `device` | read, rename and revoke own | none | read and revoke any |

Who writes which field of a trip, and when (Proposed; all of it needs the field-level and state-level write rights of 5.8 item 1):

| Field | Written by | Only while the trip is |
|---|---|---|
| `purpose`, `vehicle`, `started_at`, `ended_at`, `expected_points`, `last_seq` | the driver, by `update` | `recording`, `ended` or `rejected` |
| `discard_reason` | the driver, by `update`, before `discard` | `recording`, `ended` or `rejected` |
| `reject_reason` | the reviewer, by `update`, before `reject` | `in_review` |
| `integrity`, `distance_m`, `point_count`, `max_seq`, `confirmed_at`, `state` | nobody by `update`: the validator, a computation or an action sets them | never by `update` |

The reviewer group is distinct from the driver group. A person may hold both; the rule that they cannot confirm their own trip is a required capability (5.8), because an assertion sees the record and its children, not the caller (MODULES.md 3.8).

Data class: none of the existing classes fits location cleanly (`internal` is too weak, `special_category` too strong), so a new class `location` is Proposed, with masking for readers who need only the distance (Q40). CONVENTIONS section 5 already puts attendance geo in `confidential_hr`. Until Q40 is answered, the package declares `confidential_hr` for point fields (an interim reading of this document, not in the proposal); that class is not agent-readable by default and is excluded from the browser cache.

### 5.8 Required kernel capabilities (Proposed)

The design above assumes things the platform does not do today. Each is a requirement, not an assumption, and none is built:

1. **State-conditioned and field-level write rights.** Refuse the driver's `update` outside `recording`, `ended` and `rejected`; let the driver write only the fields of the table in 5.7; let the reviewer write `reject_reason`, and only while `in_review` (the reviewer holds no `write_group` of the model, and the model has one); make `integrity`, `distance_m`, `point_count`, `max_seq`, `confirmed_at` and `state` unwritable by `update`; refuse any `update` and `delete` of a point except by the retention job; refuse new points for a trip that has left `recording`. Today a package field's `group` gates only who may read it, and `[model.access]` (D58) decides which rows a person sees and defaults `own`, not what a state allows. D79 (Proposed) describes a field `write_group`, `write_when`, `action_only` and separate write and delete positions, and D77 immutability is the database-level guard. Whether the installed code accepts any of these in a package is not checked, and which of them carries this is not decided.
2. **A state-conditioned read restriction.** The reviewer reads trips and their points only from `in_review` on. D58 `see_all_group` shows every state; D79 `restrict` (Proposed) is the candidate; not checked.
3. **Separation of duties.** Refuse `confirm` and `reject` when the caller is the driver of the trip. Assertions (D56) see the record and its children, not the caller (MODULES.md 3.8).
4. **Distance computation.** A way for the geodesic helper to compute and store `distance_m` and the integrity flags from the accepted points, in the same transaction as the point batch (or with `end` waiting for the computation), so that a transient zero cannot refuse `end` or lead to a discarded trip; and to apply the speed cap and the accuracy filter.
5. **A retryable refusal.** A stable, machine-readable code for a failed assertion that the outbox can tell from a refusal for good (5.3, step 4). It is not checked whether a refusal already carries such a code.
6. **A retention job** that thins or erases points and their D75 history (5.6).
7. **A way to set `confirmed_at`** when `confirm` runs, for example an action effect (D56); not checked.
8. **The optimistic lock `if_version`** that D75 reserves, accepted by `update` (today it is refused as an unknown field), if an update of the outbox must refuse a change made on an older state of the trip. The first slice does not need it while updates only set the same values again.

### 5.9 Not in the first slice

Reimbursement (confirmed trips can later become expense lines of kind `mileage` through the rate tables of people.md 4.5, with the module `expenses`; the first slice offers an export); a month lock (when designed, it uses the D77 immutability or a `mileage_month` model, never an action of a trip); maps; automatic start and automatic stop; push; native passkey prompts; iOS; a review screen in the app.

## 6. The app

Languages: English first, then the six other shipped languages through the platform's label tables (D52: de, fr, pl, fi, sv, es; seven in all); the order after English is open. Synthetic data only.

| Screen | Content |
|---|---|
| Home | One big Start button; the state of the last trip; the open-trip warning |
| Recording | Elapsed time, distance so far as the device sees it (the server value replaces it), the Stop button, the "keep this app running" guide; the permission and disclosure steps of 4.2 before the first start |
| Trip list | Own trips with their states |
| Trip detail | Distance, time, start and end; no map in the first slice; Submit for an ended trip; Discard with a reason for a trip that is recording, ended or rejected |
| Account | Devices, sign out |

The proposal also lists a review-and-confirm screen in the app. In the first slice review and confirmation happen in the back office, which gets the generated views of the package (D46): acceptance item 4 uses the back office, and a reviewer can work without the app. A review screen in the app is later and open (15, item 7).

## 7. CLI, API and MCP surface

A feature is not done until it exists in the CLI, the API and MCP (OR-4, D80). The registry generates `get`, `list`, `search`, `create`, `update`, `delete` and `bulk_create` for every model, so `mileage_trip`, `mileage_point` and `device` have all of them on all three surfaces; every field carries a data class, a permission group and a description (D41). Actions: `mileage_trip.end`, `.submit`, `.confirm`, `.reject`, `.reopen`, `.discard` and `.discard_stuck`. Device revocation is `device.revoke`. The CLI spelling follows [cli.md](cli.md) (Proposed under Q22). The app calls only these commands.

## 8. Repository layout and tooling

### 8.1 Layout

```
web/packages/client/      @obop/client (working name): generated types, sign-in and refresh, outbox interface (web and app)
web/packages/tokens/      DTCG JSON -> CSS variables for the web, plus a native target (tokens.native.ts)
web/packages/trip-core/   pure TypeScript: fix filtering, distance, the outbox state machine; tested with vp test;
                          test vectors shared with the Rust validator so both sides agree
web/apps/mileage-app/     the Expo app (development build, config plugins, no checked-in ios/ or android/)
packages/mileage.toml     the package; crates/modules/mileage (the geodesic helper) when needed
```

The root `pnpm-workspace.yaml` already globs `web/apps/*` and `web/packages/*`, so the app joins the same workspace. Today the web token source is `web/packages/ui/src/tokens.css`; the proposal is one DTCG source (D35 names DTCG) with two generated targets, not a second hand-kept copy. The app has a small themed component set (Screen, Button, Row, Field, Banner) written for React Native; where it is shown is Q42. obop publishes nothing to the npm registry until the owner rules at W2 (OR-19, Q32), so these packages are workspace-private.

### 8.2 The tooling exception

The rule is that all JavaScript and TypeScript tooling goes through `vp`. Metro (the React Native bundler), the Expo CLI, Gradle and Xcode cannot be run by Vite+. OR-34 b therefore adds the mobile toolchain as a third named exception beside the two of OR-9: these tools are started through `vp env exec` or `vp run` tasks; pnpm stays the only package manager; `vp check`, `vp test` and `vp build` still cover every shared TypeScript package (`client`, `tokens`, `trip-core`). CLAUDE.md states the exception. Open technical point (not verified): Metro with pnpm's symlinked `node_modules`. The first spike must prove it; the fallback of a hoisted linker for the app only must stay inside the exception and pnpm.

## 9. The first slice

"One background-recorded trip from a real phone", Android first (it can be built on the Windows development machine; iOS follows when a Mac is available). Six work packages, in order (size is the unit of one agent package of a few hours: S, M, L), each followed by tests and an independent review as usual; the proposal counted roughly eight packages in all:

1. Spike: an Expo app in the workspace builds a development build; `vp check` and `vp test` run on the shared packages (S).
2. `@obop/client` generated from a running instance (S to M).
3. Server: the mobile client class with the `/api` audience, the `device` model and revocation, with tests and the D74 fault rules (M).
4. The `mileage` package, ingestion through `bulk_create` as in 5.3, and the capabilities of 5.8 that the slice needs; validation vectors shared with `trip-core` (M to L).
5. The app: system-browser sign-in, Start, the foreground service, the encrypted SQLite outbox, upload with airplane mode toggled during the trip, Stop, Submit. Review and confirmation are done in the back office (M to L).
6. Label tables in the shipped languages and the documentation (S).

The real-phone test is the part no agent can do alone.

**Acceptance test** (a real Android phone, a synthetic walk or drive, a test instance):

1. Given a signed-in driver with the Mileage app and a permitted location, when the driver taps Start and then locks the phone for 10 minutes while moving along a reference route, then recording continues.
2. Given airplane mode is switched on and off during the trip, when the driver taps Stop and the outbox drains, then no point is lost: the server holds every point of the trip once and in order, and a repeated batch creates nothing.
3. The computed distance is within a tolerance of the reference route. The tolerance is stated before the test runs; this document does not set it.
4. The driver submits; the trip appears in the back office in `in_review`. A reviewer who is not the driver confirms it, it moves to `confirmed` and counts once in the total of its month; a trip that is not `confirmed` does not count.
5. Reported, not pass or fail (D21: no number is claimed in advance): the battery drain over a 30-minute drive on the test phone, measured and recorded.

## 10. Non-functional requirements

- **Security.** No secret in the app bundle; tokens only in the platform keystore; the device database with unsent points encrypted at rest with a key in the keystore (3); PKCE S256; the system browser only; access tokens are short-lived (one hour for the MCP clients of D64; the value for the mobile class is Proposed) and bound to the `/api` audience (the D64 amendment). A revoked device is refused at once.
- **Privacy.** 5.6. Every field of the package has a data class and a description (D41).
- **Reliability.** The outbox survives a killed app and a restart. A replay inside 24 hours is idempotent by the key of `bulk_create`; a later replay fails closed on the unique constraints (5.3). The server computes the distance.
- **Accessibility.** The app follows the intent of WCAG 2.2 AA (D35) with the platform screen readers; no gate is defined for it yet (Proposed).
- **Performance.** No number is claimed: the battery drain over a 30-minute drive is measured on real phones (9, acceptance item 5).
- **Test plan.** `trip-core` and the shared vectors run under `vp test`; the Rust validator, the mobile class and the capabilities of 5.8 run under `cargo test` including the D74 fault rules; tests prove that a repeated batch inside 24 hours creates nothing, that a repeat after expiry is refused by the unique constraint, that `end` is refused while points are missing and succeeds when the count matches, that the driver cannot edit a `confirmed` trip or touch a point, that a reviewer cannot confirm their own trip, and that retention removes raw points from the table and from history; a Playwright test in `web/packages/e2e` checks that the back office shows a trip in `in_review` and confirms it; a Rust test and the same Playwright suite check that revoking a device ends its refresh-token family and that the app is refused at the next call; the acceptance test of 9 runs on a real phone; all data is synthetic.

## 11. Where obop deliberately differs from Odoo

Odoo's mobile apps are Enterprise. obop has one edition under Apache-2.0 and no `ee` tree (OR-1, OR-2), so a mobile client is never a paid add-on. obop does not plan a suite of native apps: the PWA stays the default client, and D87 proposes that a native app exists only where a PWA cannot do the job (Proposed).

## 12. Out of scope, non-goals and later

- Auto-stop until the sign-off of Q41 exists. Automatic start is not ruled by OR-34 or Q41; it is out of scope for the first slice (manual start is the baseline), and it would also need the background-location permission and the Play declaration (4.2). Geofences and car Bluetooth triggers belong to automatic start.
- Continuous GPS monitoring of staff. A trip is recorded only while a driver has started it, and the route is audited when read.
- Tauri mobile, a Capacitor app, an own native location module (unless the spike fails), and the Transistorsoft library in the default build.
- Maps, reverse geocoding and tile servers (Q39); push (Q38); native passkey prompts; iOS in the first slice; reimbursement and expense lines (5.9); a month lock; a review screen in the app; private-trip route storage.
- Apple and Google accounts, store listings and publication by an agent (Q37).

## 13. Risks

| Risk | Mitigation |
|---|---|
| Android vendor battery killers and the 30-minute drain | Foreground service, a "keep this app running" guide, a measured drain test on real phones |
| Google Play and App Review of background location | Tap-to-start only in the first slice, the in-app disclosure, a check of the open Play question (4.2) before any listing |
| GPS cannot be tested only in emulators (route playback in the Android emulator helps; general knowledge) | The real-phone acceptance test; synthetic routes |
| The Windows development machine cannot build for iOS | Android first; iOS when a Mac is available |
| SDK and React Native churn (SDK 58 is days old) | SDK 57 now (OR-34 e); the owner's "for now" decides the move, which D23 would allow from 2026-10-28 |
| Location-data compliance | 5.6; legal review of auto-stop before it exists (Q41); not legal advice |
| Raw GPS in three places (the server, the device outbox, the D75 history) | Encrypted outbox, one retention rule for all three (5.6), tests |
| The design needs kernel capabilities that do not exist (5.8) | They are listed as requirements; the slice does not claim them until they are built |
| Scope creep toward maps, auto-stop and reimbursement | 5.9 and 12 |
| The server enforcing what the prior app only did in its client | 5.4 and shared vectors |

## 14. What is not verified

Apple's documentation (the iOS rules are general knowledge); Expo's iOS behaviour with `CLBackgroundActivitySession`; whether a foreground service started from a visible app avoids the Google Play background-location declaration; Metro with pnpm; the accessibility classes of `expo-secure-store` items read while the phone is locked; how to encrypt an `expo-sqlite` database; the Transistorsoft price; the maintenance of the self-hosted Expo update servers; the Expo release dates and versions (seen in a search result, the upgrade guide was not read); whether SDK 57 behaves like the SDK 58 documentation that was read; the Google Play deadlines; whether a failed assertion already carries a stable code, whether an action effect can set a timestamp, and whether the installed code accepts the D79 field and state rules of 5.8 in a package; anything on a real phone.

## 15. Open questions

Owner questions are Q37 to Q42 in [DECISIONS.md](../DECISIONS.md) section 8. Questions that this document leaves open and that no register row covers yet:

1. **Client package name.** `@obop/client` (the proposal) or the Proposed `@obop/rpc` of ARCHITECTURE 10.2; one package should serve the web and the app. Not decided here.
2. **Device model.** CONVENTIONS section 9.2 proposes kiosk and POS devices as API-client principals plus a `pos_device` record (D28). Whether `device` and `pos_device` are one model or two is open; nothing in this document or the glossary decides it.
3. **Interim data class.** `confidential_hr` for point fields until Q40 is answered (5.7).
4. **Private trips.** Keep only the distance, or drop the trip.
5. **Numbers.** The accuracy threshold, the open-trip hours, the retention days, the distance tolerance and the speed cap are open or Proposed.
6. **Reviewer scope.** All trips (the first slice), or only a manager's reports (the organisation grants of D79).
7. **Review in the app.** Whether a review screen is added to the app after the first slice (the first slice reviews in the back office).
8. **Permission groups and wire ids.** `mileage.driver`, `mileage.reviewer`, `mileage.admin`, `client_trip_id` and the action names are working names until CONVENTIONS sections 3 and 4 register them.
9. **The Play question.** Whether a foreground service started from a visible app avoids the background-location declaration (4.2).
10. **Correcting a confirmed trip.** Whether `confirmed` may return to `in_review` (an action of the administrator), or only a new trip corrects it.
11. **Month lock.** When designed: D77 immutability or a `mileage_month` model, the date field and zone that assign a trip to a month, and the rule for a trip confirmed late into a locked month.
12. **Separation-of-duties policy.** The explicit policy under which a business lets a reviewer confirm their own trip, if it does.
13. **The kernel capabilities of 5.8**, and which record or decision carries each.

## 16. Sources

Decision IDs, owner rulings and owner questions refer to [DECISIONS.md](../DECISIONS.md): D13, D15, D18, D21, D23, D28, D35, D37, D41, D45, D46, D48, D49, D52, D54, D56, D57, D58, D64, D65, D70, D72, D74, D75, D77, D79, D80, D87; OR-1, OR-2, OR-4, OR-8, OR-9, OR-19, OR-34; Q22, Q32, Q37 to Q42. D75 and D77 have no record in the register yet; they are cited as the repository and its other documents cite them. The design proposal of 2026-10-27 (the lead session) is the source of every marking above; it is not stored in the repository. External sources named in it: the Expo SDK 58 Location documentation, the Android foreground service types, the Google Play background-location policy, the Transistorsoft README, RFC 8252 and the Expo Updates protocol. Platform facts about `bulk_create` come from `crates/kernel/obop-commands/src/engine/bulk.rs` and [../MODULES.md](../MODULES.md). Related: [people.md](people.md), [services.md](services.md), [security-privacy.md](security-privacy.md), [ux.md](ux.md), [cli.md](cli.md), [../CONVENTIONS.md](../CONVENTIONS.md), [../ARCHITECTURE.md](../ARCHITECTURE.md).
