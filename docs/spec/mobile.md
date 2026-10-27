# Mobile: React Native apps and the mileage package

- **Status:** Draft 0.1. OR-34 is RATIFIED: React Native (Expo) apps are allowed as obop clients, with the mobile-toolchain exception; manual start and stop is the baseline and auto-stop comes only after legal sign-off; Transistorsoft is excluded as the default; Expo SDK 57 for now. Everything else in this document, including the scope limits and the technical design, is PROPOSED (D87) until it is built. The Expo spike of 2026-10-27 (8.3) is the first code: an app and a shared package that build and run on Android. The typed client of 2026-10-28 (8.4) is the second: the OpenAPI export, the generated types and the call and error layer; nothing else of the design itself is built.
- **Date:** 2026-10-27
- **Scope:** What a native mobile app is in obop terms (one more client of the surface that exists), how a trip is recorded while the phone is locked, the `mileage` package (trips, points, a review state machine, server-side validation), the first app (Mileage), the repository layout and tooling it needs, the first slice with its acceptance test, and what is not verified. Every capability is a typed command in the CLI, the REST API and MCP (OR-4, D80); section 7 lists the surface of this area.

**Reading notes.** Decisions are cited by ID, owner rulings as OR-n and owner questions as Qn ([DECISIONS.md](../DECISIONS.md)); names shared with other documents follow [CONVENTIONS.md](../CONVENTIONS.md). Apart from the Expo spike of 8.3 and the typed client of 8.4 nothing here is built, and nothing is legal advice. Three markings state how a fact is known, and they are kept from the design proposal that the lead session wrote on 2026-10-27: **verified** means the lead session read the cited page that day (this document does not re-read it); **general knowledge** means the source was not found or could not be loaded, so the claim must be checked before anything depends on it; **unverified** means it was looked for and not found, or only seen in a search result without reading the page. "Proposed" marks anything the owner has not ratified; names marked Proposed (models, fields, permission groups, commands) are working names. Where this document describes what the platform does today (for example `bulk_create`), it was checked against the code of this repository on the date above. In 8.3 and 8.4 the word **verified** has the extra meaning that the session ran the command or read the file on the date given and the command is listed, so anyone can repeat it; nothing in 8.3 or 8.4 was tried on a real phone. Reasoning from prior art refers to "a prior mileage app" and names no organisation. The owner's words of OR-34 are quoted in D87.

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
| Apple and Google accounts, store listings, publishing, push, map tiles, a new data class `location`, legal sign-off for auto-stop, Storybook for the app | Open: Q37 to Q42; Google Play services in the Android app: Q43 |

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
| Typed client | A shared package `@obop/client` (working name; ARCHITECTURE 10.2 lists `@obop/rpc`, which may be the same thing, see 15): types generated from `/api/openapi.json` with `openapi-typescript` and `openapi-fetch` (already the plan in D15 and ARCHITECTURE 10.4), plus sign-in, refresh and the outbox interface. Used by the web back office and the app. Because models depend on installed modules, the generic layer reads `/api/meta` as the back office does, and the `mileage` package ships its own spec fragment. | The OpenAPI route exists (`crates/kernel/obop-http/src/lib.rs`). Since 2026-10-28 `obop openapi` and `web/packages/client` (`@obop/client`) exist (8.4): generated types, `bulkCreate`, `getMeta`, one error class and the D74 retry rule. Not built: sign-in, refresh and the outbox interface |
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
| An own native module (Kotlin FusedLocationProvider foreground service, Swift CLLocationManager) as an Expo module | Maximum control, highest cost, two languages to maintain. | The proposed fallback if `expo-location` proves unreliable in the field spike. FusedLocationProvider is part of Google Play services, the same dependency that `expo-location` has on Android (8.3, Q43); a module that avoids it uses the platform `LocationManager` |

### 4.2 Platform rules that shape the design

- **Android 14 and later (verified).** A foreground service of type `location` needs `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_LOCATION` and `android:foregroundServiceType="location"`; `ACCESS_FINE_LOCATION` or `ACCESS_COARSE_LOCATION` must be granted; and a location foreground service cannot be started from the background unless the app holds `ACCESS_BACKGROUND_LOCATION`. Consequence: a trip started by a tap while the app is visible works without any background permission; a trip that starts by itself (geofence, car Bluetooth) needs the background permission and the Play declaration. That is a further reason, by scope, to leave automatic start out (5.9); automatic start is not ruled by OR-34, which covers only auto-stop.
- **Google Play (verified).** Background location is allowed only for core functionality; the Permissions Declaration Form in Play Console; one background-location feature; a short demonstration video of the Android app (30 seconds or less recommended); an in-app disclosure before the runtime prompt that contains the word "location" and a background phrase such as "when the app is closed or not in use", also in the store listing and on the website. Deadlines were not found.
- **Google Play and the foreground service (read again by the spike session on 2026-10-27, from the raw page text of the policy, section "Foreground Service (FGS) api"; `support.google.com/googleplay/android-developer/answer/9799150`).** The page makes location through a foreground service subject to review. It says the foreground service must be started as a continuation of an in-app, user-initiated action, and must be "terminated immediately after the application completes the intended use case" of that action (one quote, kept short on purpose; look up the full sentence at the page). It also says that a use of location through a foreground service that is equivalent to `ACCESS_BACKGROUND_LOCATION`, or otherwise location in the background, falls under the requirements for location in the background, and those include the permission declaration process (paraphrased). **Still unverified, and important:** the page does not say whether a trip that a driver starts by a tap and that records for hours counts as a use "equivalent" to the background permission, and it gives no example of a trip-recording app. **Plan (an assumption, not a finding):** assume that a store release needs the declaration form and the demonstration video; assume that an internal test build or a sideloaded APK does not (this was not read from the policy either). Q37 decides what is done before any listing is made.
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
web/packages/client/      @obop/client (working name): generated types, call helpers, error mapping (built, 8.4); sign-in and refresh, outbox interface (later steps) (web and app)
web/packages/tokens/      DTCG JSON -> CSS variables for the web, plus a native target (tokens.native.ts)
web/packages/trip-core/   pure TypeScript: fix filtering, distance, the outbox state machine; tested with vp test;
                          test vectors shared with the Rust validator so both sides agree
web/apps/mileage-app/     the Expo app (development build, config plugins, no checked-in ios/ or android/)
packages/mileage.toml     the package; crates/modules/mileage (the geodesic helper) when needed
```

The root `pnpm-workspace.yaml` already globs `web/apps/*` and `web/packages/*`, so the app joins the same workspace. Today the web token source is `web/packages/ui/src/tokens.css`; the proposal is one DTCG source (D35 names DTCG) with two generated targets, not a second hand-kept copy. The app has a small themed component set (Screen, Button, Row, Field, Banner) written for React Native; where it is shown is Q42. obop publishes nothing to the npm registry until the owner rules at W2 (OR-19, Q32), so these packages are workspace-private.

### 8.2 The tooling exception

The rule is that all JavaScript and TypeScript tooling goes through `vp`. Metro (the React Native bundler), the Expo CLI, Gradle and Xcode cannot be run by Vite+. OR-34 b therefore adds the mobile toolchain as a third named exception beside the two of OR-9: these tools are started through `vp env exec` or `vp run` tasks; pnpm stays the only package manager; `vp check`, `vp test` and `vp build` still cover every shared TypeScript package (`client`, `tokens`, `trip-core`). CLAUDE.md states the exception. The open technical point of Metro with pnpm's symlinked `node_modules` is answered by the spike (8.3, verified): no hoisted linker and no Metro configuration were needed. If one is ever needed, a hoisted linker for the app only must stay inside the exception and pnpm.

### 8.3 The Expo spike (built and run on 2026-10-27)

Step 1 of the first slice (9). Marking: **verified** here means the spike session ran the command or read the file and the commands are listed below; nothing here was tried on a real phone, and nothing of iOS was tried (iOS needs Xcode and so a Mac). Machine: Windows 11 Pro, 16 cores, 127 GB of memory, a fast network. All data is synthetic.

**What exists.** `web/packages/trip-core` (`@obop/trip-core`: `distanceMeters`, haversine on the WGS84 mean radius, 9 tests under `vp test`, `vp check` with type checks, `vp build` as a library check) and `web/apps/mileage-app` (`@obop/mileage-app`: Expo SDK 57, one screen with the app name and `distanceMeters` of two synthetic points, 1322 m). The development client, `expo-location`, `expo-task-manager`, `expo-secure-store` and `expo-sqlite` are declared and not used. `expo-location` is configured for a foreground service and for no background permission (4.2). The application id and bundle id `org.obop.mileage` are placeholders: no store account, signing key or publication exists (Q37). `android/` and `ios/` are not checked in and are git-ignored.

**Versions (verified: the package manifests, the lockfile and `expo install --check`).** Every direct dependency is pinned exactly: `expo` 57.0.30, `react-native` 0.86.3, `react` and `react-dom` 19.2.3, `expo-dev-client` 57.0.20, `expo-location` 57.0.21, `expo-task-manager` 57.0.21, `expo-secure-store` 57.0.4, `expo-sqlite` 57.0.4, `expo-status-bar` 57.0.1, `@types/react` 19.2.18 and `typescript` 6.0.3. The Expo versions are those of the SDK 57 bill of materials (`expo/bundledNativeModules.json` of 57.0.30). `react-dom` is declared only so that Expo's optional `react-dom` peer is paired with the app's `react`; without it pnpm paired it with the web apps' 19.3.1 and locked `react-dom` 19.3.1 beside `react` 19.2.3, which React Native 0.86 requires.

**The cooldown (verified).** D23 asks for versions at least 7 days old. Until the spike `pnpm-workspace.yaml` held only `packages:`, so nothing enforced it (pnpm's own default is 1 day and not strict), and the first lock of this branch held 15 younger versions, among them `expo-location` 57.0.22 and `expo-modules-core` 57.0.24. `pnpm-workspace.yaml` now sets `minimumReleaseAge: 10080` (minutes) and `minimumReleaseAgeStrict: true`, the key names of pnpm 12.6.0. `vp add expo@57.0.31` is refused with a message that names the cutoff and `minimumReleaseAgeExclude`. The lock was made again from the lockfile of `main` under the setting, which moved `expo-location` to 57.0.21 and `expo-modules-core` to 57.0.23, and an independent check of the publish time of all 780 locked package versions against the npm registry found none younger than 7 days. No `minimumReleaseAgeExclude` entry was needed. `expo` 57.0.31 and `expo-location` 57.0.22 were both published on 2026-10-21 (npm registry) and wait for the cooldown, which ends on 2026-10-28; `vp run check:sdk` asks for exactly these two, and each bump is a one-line change plus a new lock. The Android build and the emulator check were run again with the new lock (below).

**TypeScript.** `typescript` 6.0.3 differs from the rest of the repository (7.0.2). `expo install --check` asks for `~6.0.3`; that figure comes from Expo's versions API (`https://api.expo.dev/v2/versions/latest`, `sdkVersions["57.0.0"].relatedPackages`, which also names `@types/react` `~19.2.4`), not from `bundledNativeModules.json`, which has no `typescript` entry, and not from the app template. It cannot be reproduced offline. Only the Expo CLI uses this TypeScript; `vp check` type-checks with its own checker. Native side, from React Native 0.86.3: Gradle 9.3.1, Android Gradle Plugin 8.12.0, Kotlin 2.1.20, minSdk 24, compileSdk and targetSdk 36, build-tools 36.0.0, NDK 27.1.12297006.

**Commands.** From `web/apps/mileage-app`, with `JAVA_HOME` set to a JDK 17, `ANDROID_HOME` to the Android SDK, optionally `GRADLE_USER_HOME`, and `EXPO_NO_TELEMETRY=1` (all set per process, nothing system-wide). `vp install` runs at the repository root first. `scripts/gradle.mjs` takes the Gradle task and further arguments made of letters, digits and `: . - _ = / \` only (it refuses anything else with a message; JVM options with spaces belong in `GRADLE_OPTS` or in `gradle.properties` in `GRADLE_USER_HOME`), adds the ABIs from `OBOP_ANDROID_ABIS` (default `arm64-v8a,x86_64`; the generated project builds four) and prints the command and the exit code when Gradle cannot be started or fails.

| Command | What it does |
|---|---|
| `vp run check:sdk` | `expo install --check` against the SDK 57 bill of materials |
| `vp run export:android` | a headless Metro bundle for Android (Hermes bytecode, into `dist/`, git-ignored) |
| `vp run start` | `expo start --dev-client`, Metro on port 8081 |
| `vp run android:prebuild` | `expo prebuild --platform android --no-install` (generates `android/`) |
| `vp run android:build` | Gradle `assembleDebug` for arm64-v8a and x86_64 (the development client, 106,428,949 bytes) |
| `vp run android:build:release` | Gradle `assembleRelease` for the same two ABIs (45,917,851 bytes, JavaScript bundle embedded, signed with the template's debug key, not a release key) |

**Metro with pnpm (verified; answers the open point of 8.2).** With pnpm 12.6.0's default isolated linker (symlinked `node_modules`), no `.npmrc` entry, no linker or layout setting in `pnpm-workspace.yaml` (the file gained only the cooldown setting above) and no `metro.config.js`, the default Metro configuration of Expo SDK 57 resolved `@obop/trip-core`, a `workspace:*` dependency whose `exports` point at TypeScript source. Evidence: `vp run export:android` bundled 582 modules; the unminified bundle (`expo export --platform android --no-bytecode --no-minify`) contains `function distanceMeters` from the package; the Hermes bundle inside the release APK contains the name and the screen text; the app on an emulator shows 1322 m; `vp run start` served 710 modules to the development client. Autolinking of the native modules also worked under pnpm's layout.

**Android build on Windows (verified).** Installed outside the repository under `D:\Android`: Microsoft Build of OpenJDK 17.0.21 (the Expo documentation for SDK 57 names JDK 17; a zip of 178.6 MiB whose SHA-256 matched Microsoft's file; the JDK 26 that was already on the machine was not used and not tried), the Android command-line tools 22.0 (`commandlinetools-win-15859902_latest.zip`, 148.4 MiB, SHA-256 matched Google's page; the SDK licences were accepted with `sdkmanager --licenses`), then `platform-tools`, `platforms;android-36`, `build-tools;36.0.0`, `ndk;27.1.12297006` (2.3 GB installed; the four took 37 s), `cmake;3.22.1` (installed by Gradle) and `cmake;4.1.2` (55 MB, see below). Gradle 9.3.1 is downloaded by the wrapper. Sizes on disk after the builds: the SDK 8.3 GB (4.3 GB of it a system image and 1.1 GB the emulator), the Gradle home 4.6 GB, the generated `android/` 2.2 GB. Times: the first build from an empty Gradle home ran 10 min 16 s and failed (below); a clean rerun of the documented sequence (no `android/`, an empty Gradle home, the SDK installed) took 3 s for `vp run android:prebuild` and 5 min 14 s for `vp run android:build`. That run, and the first warm runs (debug 1 min 52 s, release 3 min 36 s), used the first lock; after the lock was made again under the cooldown (above) the native modules changed, and a warm Gradle home needed 4 min 1 s for `vp run android:build:release` and 2 min 55 s for `vp run android:build`. The APK sizes in the table are those of the final lock. No Google account or owner decision was needed.

The first build failed in `:expo-modules-core:buildCMakeDebug` with `ninja: error: manifest 'build.ninja' still dirty after 100 tries` and CMake warnings that object paths pass 250 characters. `LongPathsEnabled` was already 1 and git `core.longpaths` true, so this is not the operating system setting: the CMake that the Android Gradle Plugin picks by default, 3.22.1, bundles Ninja 1.10.2, which fails on paths over 260 characters (checked with a hand-written `build.ninja` and a 280 character output path: Ninja 1.10.2 stops with "Filename longer than 260 characters", the Ninja 1.12.1 of CMake 4.1.2 builds it), and under pnpm the sources of `expo-modules-core` sit in `node_modules/.pnpm/expo-modules-core@<version>_r_<hash>/node_modules/...`. The fix is `cmake;4.1.2` plus `cmake.dir` in the machine-local, git-ignored `android/local.properties`; `scripts/gradle.mjs` writes it on Windows when a CMake 4.x is in the SDK and warns when none is. No registry, firewall or other system setting was touched. Without this step the Windows build does not work with pnpm's layout.

**Emulator (verified).** The Windows Hypervisor Platform was already usable (`emulator -accel-check`: "WHPX(10.0.26300) is installed and usable"; nothing was enabled). Emulator 37.2.12 with `system-images;android-36;google_apis;x86_64` (Android 16), headless (`-no-window`), booted in about 35 s. The release APK installed and showed "obop Mileage" and 1322 m (a screenshot was taken and is kept outside the repository). The debug APK (development build) loaded its JavaScript from Metro started by `vp run start` through `adb reverse tcp:8081 tcp:8081` and showed the developer menu with runtime version exposdk:57.0.0. That check was done with the first lock. After the lock was made again, the release APK was rebuilt and installed on a fresh virtual device, and the accessibility tree of the screen showed "obop Mileage", "Reference distance (synthetic)" and "1322 m"; the check of the development client through Metro was not repeated. Location, the foreground service and everything that runs in the background were not tried; the emulator was stopped and its virtual device deleted afterwards.

**What the built APK says about location (verified for the manifest only).** The merged manifest of both APKs holds `ACCESS_FINE_LOCATION`, `ACCESS_COARSE_LOCATION`, `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_LOCATION` and the service `expo.modules.location.services.LocationTaskService` with `foregroundServiceType` location, and no `ACCESS_BACKGROUND_LOCATION`, as 4.2 asks for tap-to-start trips. The plugin options used are those of `expo-location` 57.0.21, the locked version (`isAndroidForegroundServiceEnabled`, `isAndroidBackgroundLocationEnabled`, `isIosBackgroundLocationEnabled`, read from the installed plugin source after the lock was made again). **Flag for the owner (Q43):** `expo-location` 57.0.21 on Android depends on `com.google.android.gms:play-services-location:21.0.1` (read from the `build.gradle` of the installed 57.0.21; the release APK contains Google Play services activity classes). Google Play services is not an OSI-licensed library and needs Google Play services on the phone; this is a free-to-use dependency but it matters for the rule that dependencies are under Apache-2.0-compatible licences and for phones without Google services. Whether `expo-location` falls back to the platform location manager without Play services was not checked.

**`vp` and the toolchain exception (verified).**

- Metro and the Expo CLI run as `vp run <script>` where the script is an `expo ...` command: `vp run` puts `node_modules/.bin` on `PATH`, so no `vp env exec` was needed. Node is the one that `vp` manages (26.12.1).
- Gradle runs as `vp run android:build`, which calls `node scripts/gradle.mjs`. A script that says `cd android && gradlew assembleDebug` fails, because `vp run` finds executables through `PATH` only ("Failed to find executable gradlew"). `expo run:android` is no substitute for a build-only step: it asks for a connected device or emulator even with `--no-install`. On Windows the script starts the `.bat` wrapper through `cmd.exe` with a command line it builds itself and refuses extra arguments outside a strict pattern (`shell: true` joins arguments without escaping, so a quoted or `&` argument would be split or interpreted); the refusal, a missing `android/` directory and a wrapper that cannot start each print a message, and a failing Gradle run is reported with its command and exit code. The Linux and macOS branch was exercised only by faking the platform on Windows (the failure message), not run on those systems.
- JDK and Android SDK are not JavaScript tooling; they are found through `JAVA_HOME` and `ANDROID_HOME`.
- Script names: `prebuild:android` is taken by `vp run` as the pre-hook of a script named `build:android` (the npm convention), so it ran `expo prebuild` and wiped `android/` before the build; the scripts are named `android:*` for that reason.
- `vp check` formats and lints `.ts`, `.tsx`, `.mjs` and `package.json` of the app without special cases. It reports type errors only when the vite config of the package sets `lint: { options: { typeAware: true, typeCheck: true } }`, which the two new packages do (checked with deliberate errors, also across the package boundary into `distanceMeters`). The existing packages do not set it: a deliberate type error in `web/packages/ui` passed `vp check`, so the type checks of the CI `web` job are not active for them. That is outside this step and is listed for the owner, not changed.
- Starting Metro made the Expo CLI rewrite `tsconfig.json#include`; the committed file already has the form it writes.
- The Expo doctor (`expo-doctor`) is not part of the repository: it is not a dependency, and `vp dlx` would fetch an unpinned copy outside the lockfile and the cooldown. It was run once ad hoc during the spike and reported the same single mismatch as `expo install --check` (20 of 21 checks passed).

**CI and licences (verified).** The steps of the `web` job were run in the worktree: `vp install --frozen-lockfile` (which also verifies the lock against the cooldown), `vp check` for `apps/backoffice`, `apps/platform`, `apps/site`, `apps/mileage-app`, `packages/trip-core` and `packages/ui`, `vp test` for `packages/ui` (167 tests), `packages/trip-core` (9) and `apps/backoffice` (164), and `vp build` for the three apps and `trip-core`; all passed, and the workflow now runs the new members in the same loops. No forbidden lockfile exists. `pnpm-lock.yaml` grew from 3,421 to 7,915 lines (4,636 lines added, 142 removed; the removals are peer-suffix changes of existing entries, because the Expo tree brings `terser` and `yaml` into the workspace). `vp install --frozen-lockfile` into an empty store took 7 s on `main` and 15 s with the branch (641 packages instead of 195 linked; a plain `vp install` the same, 14 s) on this machine with a fast network; with the lockfile deleted, a fresh resolve under the strict cooldown also worked (13 s, no package younger than 7 days, the same Expo versions; it differs from the committed lock in two existing packages, `postcss` 8.5.31 for 8.5.30 and `@shikijs/vscode-textmate` 10.1.0 for 10.0.2, ordinary drift that was not committed). `node_modules` grew from 263 MB to 638 MB and the store from 266 MB to 648 MB (measured on the first lock). The `web` stage of the `Dockerfile` ran as a real `docker build --target web` on 2026-10-27 (a private builder, removed afterwards): over the whole workspace it installed 641 packages (14.5 s) and the image was 1.99 GB, and the build context was 17 MB with a 2.2 GB generated `android/` present, because `.dockerignore` now skips `android`, `ios`, `.expo`, `.gradle`, `.cxx` and APK files. The install step now reads `pnpm install --frozen-lockfile --filter '!@obop/mileage-app'`: 204 packages (7 of 8 workspace projects, 7.9 s) and an image of 1.46 GB, with the same back office and platform app builds byte for byte and the same 1,355 files of the website (only the `Expires` time in `security.txt` differs, as it does between any two builds). The Rust stages of the image build were not run. Licences were read from the CycloneDX SBOM (`vp env exec -- pnpm sbom --sbom-format cyclonedx`, the exception of OR-9): of 643 components, 448 are new. Of those, 392 are MIT, 22 ISC, 11 Apache-2.0, 8 BSD-2-Clause or BSD-3-Clause, 1 0BSD and 1 MIT OR Apache-2.0. The others, all pulled in by `@expo/cli`, Babel and similar build-time tools and none in the APK: BlueOak-1.0.0 (`glob` 13.0.6, `lru-cache` 11.5.3, `minimatch` 10.2.6, `minipass` 7.1.3, `path-scurry` 2.0.2, `sax` 1.6.1), Unlicense (`big-integer` 1.6.52, `stream-buffers` 2.2.0), MIT OR CC0-1.0 (`type-fest` 0.7.1 and 0.21.3), CC-BY-4.0 (`caniuse-lite`, browser data), BSD-3-Clause OR GPL-2.0 (`node-forge` 1.4.0, taken under BSD-3-Clause) and Python-2.0 (`argparse` 2.0.1). There is no GPL-only, LGPL, AGPL or MPL package among the new ones and none without a licence field. This scan does not see the Gradle dependencies of the APK (AndroidX, React Native's own libraries, Google Play services above, and in the debug build only the Google ML Kit code scanner of the development client); a licence check of those is not done.

**Not verified by the spike.** Anything on a real phone (location, the foreground service, battery, vendor behaviour); iOS; JDK 26 or a JDK other than 17; an Android build on Linux or macOS (`scripts/gradle.mjs` is written for them and its failure branch was exercised by faking the platform, but it was run only on Windows); a four-ABI build (the default of the generated project is four, the scripts build two); `expo-location` without Google Play services (Q43); a licence check of the Gradle dependencies; the full image build (only `--target web` was built; the Rust stages were not run).

### 8.4 The typed client `@obop/client` (built and run on 2026-10-28)

Step 2 of the first slice (9). Marking as in 8.3: **verified** means the session ran the command, and the commands are listed. Nothing was tried on a phone or an emulator. There is no sign-in, refresh, outbox or device registration yet.

**What exists.**

- `obop openapi [--out FILE]` (`crates/obop/src/openapi_cmd.rs`) writes the OpenAPI 3.1 document of an instance without a server or a database. It builds the app in memory with the linked modules (base, product, crm, sales, invoicing, connectors, customize), takes an account that holds every permission group (so every command is listed) and runs the very function that `GET /api/openapi.json` runs, with the OAuth additions of a real instance. `servers` and `x-mcp` name the placeholder `http://localhost:8080`: a client takes the real address at run time. Keys are sorted at every level, the indent is two spaces, line ends are LF and the text ends with one newline. Packages installed on an instance later (the bundled marketplace catalogue, `mileage`) are not in it; a generic screen reads them from `GET /api/meta`, as the back office does, and a package may ship its own spec fragment (3).
- `web/packages/client/openapi.json` is that output, committed (868 KB). `web/packages/client/src/generated/schema.d.ts` is what `openapi-typescript` 7.13.0 makes of it, committed (697 KB; the header says it is generated and not to be edited).
- `@obop/client` (`web/packages/client`, private, Apache-2.0, ESM, TypeScript source in `exports` as `@obop/trip-core`): `createObopClient({ baseUrl, getToken, fetch?, onUnauthorized? })` over `openapi-fetch` 0.17.0 with the generated `paths` (`client.api.POST("/api/party/list")` is typed from the schema); `call(model, command, input)` for any command, also of a model the schema does not know; `bulkCreate(model, rows, { idempotencyKey, mode, dryRun })`, typed from the model's writable fields (5.3 uses it for trips and points); `getMeta()` (typed loosely: the schema says only "The metadata."); one error class `ObopApiError` with `status`, `code`, `hint`, `message`, `applied` (`none`, `unknown` or `partial`, from the body or `X-Obop-Applied`) and `retryAfterSeconds`; `ObopNetworkError` (status 0; `applied` is `none` for a read and `unknown` for a write) and `ObopApprovalRequired` as subclasses; and `isSafeToRetry(err, { idempotent })`, the rule of docs/DEPLOY.md "What a 503 means": a 429, and any 5xx or lost connection with `applied: none`, can be repeated; with `unknown`, `partial` or no word, only when the call carries an idempotency key or only reads; every other status never. A 401 calls `onUnauthorized` once (401s that arrive together share one run; a call that sent no token, such as a failed sign-in, does not call it) and rejects; the client never repeats a call itself. The runtime code uses the global `fetch`, `Request`, `Response` and `Headers` and nothing else (a test reads the sources for DOM and Node-only names).

**Generate and update.** After a change to a model, a field, a command or a package that is linked into the binary:

```sh
cargo run -q -- openapi --out web/packages/client/openapi.json   # or: OBOP_UPDATE_OPENAPI=1 cargo test -p obop the_committed_openapi_document
cd web/packages/client && vp run generate:types                  # writes src/generated/schema.d.ts
```

Commit both files. `openapi-typescript` needs the JavaScript API of TypeScript 5, which TypeScript 7 does not have (it fails with `Cannot read properties of undefined (reading 'createKeywordTypeNode')`), so the package pins `typescript` 5.9.3 as a dev dependency for the generator only; `vp check` type-checks with its own checker.

**Where drift is caught.** The Rust job: the test `the_committed_openapi_document_is_current` fails when `openapi.json` differs from what `obop openapi` writes, and prints the two commands above. The web job: regenerates `schema.d.ts` from `openapi.json` and fails on `git diff --exit-code`. The e2e job: runs `test/integration.test.ts`, which starts `obop serve --dev-auth` on a free port with a temporary SQLite database and calls the client against it (nine tests: meta, a read, the typed read, a dry-run `bulk_create`, a bad row, a keyed create and its replay, a 401, a 404 and a closed port). Without `OBOP_BIN` that file is skipped, so `vp test` alone stays offline.

**Metro (verified).** `@obop/client` is a `workspace:*` dependency of `web/apps/mileage-app`, which imports it and exports `createObopClient` without calling it. `vp run export:android` bundled 586 modules (582 before; the four are the package's three source files and `openapi-fetch`). The unminified bundle (`vp run export:android --no-bytecode --no-minify --output-dir DIR`) contains `function createObopClient`, `function createClient` and `class ObopApiError`, and the Hermes bundle contains the names and `idempotency_key`. No Metro configuration was needed for `openapi-fetch` (its `exports` point at an `.mjs` file).

**What the document says and does not say (read for this step).** The generated types are only as good as the document, which is written by hand in `crates/kernel/obop-http/src/openapi.rs`. These gaps are not fixed here; the client works around them:

- The record schemas have no `id`, though every record answered carries one.
- `GET /api/meta` is described only as "The metadata."; `ObopMeta` is written by hand.
- A generated operation declares 400, 401, 404, 503 and the 202 of a proposal. The error table (`errors.rs`) can also answer 403, 409, 410 and 412, and 429 comes from other routes; the client maps every status the same way. The 202 makes `data` of a typed call a union, so `call` is the easier way to run a command.
- The report of `bulk_create` omits `source` and `errors_truncated`, and a dry run answers `valid` where the schema shows `created`, and has no `ids`.
- The rate limit of `bulk_create` (`OBOP_BULK_PER_MINUTE`) is a plain 400 with a sentence: no code, no `Retry-After`, no 429. The outbox of step 5 cannot tell it from invalid input except by that sentence; a stable code for it belongs with the required capabilities of 5.8.
- The 503 of a generated operation was declared twice in the text (the rule was appended to itself). It is declared once now, which made the export about 10 percent smaller.

**Not verified by this step.** See 14: anything on a device, the React Native `fetch` and `URL` under `openapi-fetch`, and the committed files on Linux outside the CI jobs.

## 9. The first slice

"One background-recorded trip from a real phone", Android first (it can be built on the Windows development machine; iOS follows when a Mac is available). Six work packages, in order (size is the unit of one agent package of a few hours: S, M, L), each followed by tests and an independent review as usual; the proposal counted roughly eight packages in all:

1. Spike: an Expo app in the workspace builds a development build; `vp check` and `vp test` run on the shared packages (S). Done on 2026-10-27 for Android; results in 8.3.
2. `@obop/client` generated from the instance's OpenAPI document (S to M). Done on 2026-10-28 for the generated types, the call helpers and the error layer; sign-in, refresh and the outbox interface belong to steps 3 and 5. Results in 8.4.
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
| The Windows development machine cannot build for iOS | Android first; iOS when a Mac is available (iOS needs Xcode, so a Mac; nothing of iOS was tried) |
| Android builds on Windows meet path limits (8.3) | The CMake 4.x fix of 8.3 |
| `expo-location` on Android pulls in Google Play services, which is not an OSI-licensed library and needs Google services on the phone (8.3) | The owner question Q43 before any store release; the own-module fallback of 4.1 avoids it only if it uses the platform `LocationManager` |
| SDK and React Native churn (SDK 58 is days old) | SDK 57 now (OR-34 e); the owner's "for now" decides the move, which D23 would allow from 2026-10-28 |
| Location-data compliance | 5.6; legal review of auto-stop before it exists (Q41); not legal advice |
| Raw GPS in three places (the server, the device outbox, the D75 history) | Encrypted outbox, one retention rule for all three (5.6), tests |
| The design needs kernel capabilities that do not exist (5.8) | They are listed as requirements; the slice does not claim them until they are built |
| Scope creep toward maps, auto-stop and reimbursement | 5.9 and 12 |
| The server enforcing what the prior app only did in its client | 5.4 and shared vectors |

## 14. What is not verified

Apple's documentation (the iOS rules are general knowledge); Expo's iOS behaviour with `CLBackgroundActivitySession`; whether a foreground service started from a visible app, or a trip recording of hours that a driver starts, avoids the Google Play background-location declaration (the policy text was read on 2026-10-27 and does not say, 4.2); the accessibility classes of `expo-secure-store` items read while the phone is locked; how to encrypt an `expo-sqlite` database; the Transistorsoft price; the maintenance of the self-hosted Expo update servers; the Expo release dates and versions (seen in a search result, the upgrade guide was not read); whether SDK 57 behaves like the SDK 58 documentation that was read (the spike checked only that the SDK 57 packages build, the manifest of the built app and the plugin options, not any runtime behaviour); the Google Play deadlines; whether a failed assertion already carries a stable code, whether an action effect can set a timestamp, and whether the installed code accepts the D79 field and state rules of 5.8 in a package; anything on a real phone. Still open after the typed client of 2026-10-28 (8.4): the client on a phone or an emulator (the bundle was built, nothing was run), the `fetch`, `Request` and `URL` behaviour of the React Native runtime under `openapi-fetch`, the committed export and the generated types on Linux outside CI (the CI jobs of this branch run them there), and any use against a PostgreSQL instance. Still open after the spike of 2026-10-27 (8.3): iOS; `expo-location` without Google Play services (Q43); an Android build on Linux or macOS, with a JDK other than 17, or for all four ABIs; the licences of the Gradle dependencies of the APK; the full image build (the Rust stages). Verified by the spike and no longer open: Metro with pnpm's symlinked `node_modules`, an Android debug and release build on the Windows development machine, the app on an Android emulator, the strict 7 day cooldown of pnpm 12.6.0 and the web stage of the image build with the Expo packages in the workspace (8.3).

## 15. Open questions

Owner questions are Q37 to Q43 in [DECISIONS.md](../DECISIONS.md) section 8 (Q43, Google Play services, came with the Expo spike). Questions that this document leaves open and that no register row covers yet:

1. **Client package name.** `@obop/client` (the proposal) or the Proposed `@obop/rpc` of ARCHITECTURE 10.2; one package should serve the web and the app. Not decided here, and not decided by 8.4: the package that exists is named `@obop/client` because the layout of 8.1 says so, and renaming it later is a one-line change in each `package.json` that depends on it.
2. **Device model.** CONVENTIONS section 9.2 proposes kiosk and POS devices as API-client principals plus a `pos_device` record (D28). Whether `device` and `pos_device` are one model or two is open; nothing in this document or the glossary decides it.
3. **Interim data class.** `confidential_hr` for point fields until Q40 is answered (5.7).
4. **Private trips.** Keep only the distance, or drop the trip.
5. **Numbers.** The accuracy threshold, the open-trip hours, the retention days, the distance tolerance and the speed cap are open or Proposed.
6. **Reviewer scope.** All trips (the first slice), or only a manager's reports (the organisation grants of D79).
7. **Review in the app.** Whether a review screen is added to the app after the first slice (the first slice reviews in the back office).
8. **Permission groups and wire ids.** `mileage.driver`, `mileage.reviewer`, `mileage.admin`, `client_trip_id` and the action names are working names until CONVENTIONS sections 3 and 4 register them.
9. **The Play question.** Whether a foreground service started from a visible app, or a trip recording of hours that a driver starts, avoids the background-location declaration (4.2). The policy text read on 2026-10-27 does not settle it.
10. **Correcting a confirmed trip.** Whether `confirmed` may return to `in_review` (an action of the administrator), or only a new trip corrects it.
11. **Month lock.** When designed: D77 immutability or a `mileage_month` model, the date field and zone that assign a trip to a month, and the rule for a trip confirmed late into a locked month.
12. **Separation-of-duties policy.** The explicit policy under which a business lets a reviewer confirm their own trip, if it does.
13. **The kernel capabilities of 5.8**, and which record or decision carries each.
14. **Google Play services.** `expo-location` on Android depends on `com.google.android.gms:play-services-location` (8.3): a library that is not OSI-licensed and needs Google services on the phone. Whether that is accepted under the Apache-2.0-compatibility rule for dependencies and for phones without Google services, or whether the own-module fallback of 4.1 uses the platform `LocationManager`, is owner question Q43 in [DECISIONS.md](../DECISIONS.md) section 8. It is needed before any store release.

## 16. Sources

Decision IDs, owner rulings and owner questions refer to [DECISIONS.md](../DECISIONS.md): D13, D15, D18, D21, D23, D28, D35, D37, D41, D45, D46, D48, D49, D52, D54, D56, D57, D58, D64, D65, D70, D72, D74, D75, D77, D79, D80, D87; OR-1, OR-2, OR-4, OR-8, OR-9, OR-19, OR-34; Q22, Q32, Q37 to Q43. D75 and D77 have no record in the register yet; they are cited as the repository and its other documents cite them. The design proposal of 2026-10-27 (the lead session) is the source of every marking above; it is not stored in the repository. External sources named in it: the Expo SDK 58 Location documentation, the Android foreground service types, the Google Play background-location policy, the Transistorsoft README, RFC 8252 and the Expo Updates protocol. Platform facts about `bulk_create` come from `crates/kernel/obop-commands/src/engine/bulk.rs` and [../MODULES.md](../MODULES.md). Related: [people.md](people.md), [services.md](services.md), [security-privacy.md](security-privacy.md), [ux.md](ux.md), [cli.md](cli.md), [../CONVENTIONS.md](../CONVENTIONS.md), [../ARCHITECTURE.md](../ARCHITECTURE.md).
