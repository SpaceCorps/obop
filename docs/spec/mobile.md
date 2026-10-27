# Mobile: React Native apps and the mileage package

- **Status:** Draft 0.1. Parts (a) to (e) of OR-34 are RATIFIED (D87); the technical design in this document is PROPOSED until it is built.
- **Date:** 2026-10-27
- **Scope:** What a native mobile app is in obop terms (one more client of the surface that exists), how a trip is recorded while the phone is locked, the `mileage` package (trips, points, a review state machine, server-side validation), the first app (Mileage), the repository layout and tooling it needs, the first slice with its acceptance test, and what is not verified. Every capability is a typed command in the CLI, the REST API and MCP (OR-4, D80); section 7 lists the surface of this area.

**Reading notes.** Decisions are cited by ID, owner rulings as OR-n and owner questions as Qn ([DECISIONS.md](../DECISIONS.md)); names shared with other documents follow [CONVENTIONS.md](../CONVENTIONS.md). Nothing here is built and nothing is legal advice. Three markings state how a fact is known, and they are kept from the design proposal that the lead session wrote on 2026-10-27: **verified** means the lead session read the cited page that day (this document does not re-read it); **general knowledge** means the source was not found or could not be loaded, so the claim must be checked before anything depends on it; **unverified** means it was looked for and not found. "Proposed" marks anything the owner has not ratified; names marked Proposed (models, fields, permission groups, commands) are working names. Reasoning from prior art refers to "a prior mileage app" and names no organisation. The owner's words of OR-34 are quoted in D87.

## 1. Purpose and scope

### 1.1 What this area is

obop is a web product: the back office and the PWA are clients of the REST and MCP surface. A PWA cannot keep receiving location updates while the phone is locked (general knowledge), so one use case needs a native client: recording a trip for a mileage report. D87 (RATIFIED in part by OR-34) allows first-party React Native apps built with Expo, only where a PWA cannot do the job, and names Mileage as the first. This document specifies that app and the `mileage` package behind it, plus the general rules every later native app follows.

An app is a client and nothing more. It has no private backdoor: it calls REST and the OpenAPI document, signs in through OAuth, and meets the checks of the one Dispatcher like every other client (D80). Business logic (distance, validation, state changes, permissions) lives on the server; the app captures, buffers and uploads.

### 1.2 What is ruled and what is Proposed

| Item | State |
|---|---|
| React Native (Expo) apps allowed as obop clients | RATIFIED (OR-34 a, D87) |
| The scope limits: first-party apps only, only where a PWA cannot do the job; the PWA stays the default; Tauri mobile is not pursued (Tauri 2 desktop shells for POS stay, D37) | PROPOSED (D87); they came with the proposal and are not in the owner's words |
| The mobile-toolchain exception to "`vp` only" (section 8.2) | RATIFIED (OR-34 b) |
| Manual start and stop is the baseline; auto-stop only after legal sign-off | RATIFIED (OR-34 c). The proposal adds product and works-council sign-off, and this document requires all three (Proposed) |
| The Transistorsoft library is excluded as the default path; an optional adapter later is not forbidden | RATIFIED (OR-34 d) |
| Expo SDK 57 (React Native 0.86) for now; SDK 58 (React Native 0.88, released 2026-10-21) when it is three weeks old, around 2026-11-11 | RATIFIED (OR-34 e); the date follows from D23 as read by the ruling |
| Mobile sign-in class, `device` model, device outbox, the `mileage` package, privacy points, repository layout, the first slice | PROPOSED (D87, D13 and D64 amendments); built nothing |
| Apple and Google accounts, store listings, publishing, push, map tiles, a data class `location`, legal sign-off for auto-stop, Storybook for the app | Open: Q37 to Q42 |

### 1.3 Boundaries with neighbouring areas

| Neighbour | Owns | This area owns |
|---|---|---|
| [people.md](people.md) 4.5 Expenses | `expense_category` of kind `mileage`, rate tables, claims, reimbursement | The confirmed trip and its distance; turning a confirmed trip into an expense line is later (5.7) |
| [services.md](services.md) 4.7 Fleet | Vehicles, odometer readings, assignments; "no telematics or location tracking" | An optional vehicle reference on a trip; the route is recorded only while a driver has started a trip and only by the Mileage app |
| [security-privacy.md](security-privacy.md) | Identity, audit, privacy tooling, the data-class registry owner | The privacy points of 5.5, device revocation in the Customize, Security page |
| [integrations.md](integrations.md), `obop-connectors` (D54) | Egress gate for flows to third parties | Any push, tile or geocoder flow goes through it (Q38, Q39) |
| [ux.md](ux.md) | Back-office views, tokens, the offline contract of the web apps | The app's screens (6); the tokens target for native (8.1) |
| [cli.md](cli.md) | Command grammar and the parity matrix | The commands of 7 |

## 2. Personas and roles

| Persona | Needs | Permission group (names Proposed) |
|---|---|---|
| Driver | Starts and stops a trip, sees own trips, submits them, signs out and revokes own devices | `mileage.driver` |
| Reviewer | Sees submitted trips, confirms or rejects them, reopens a rejected trip | `mileage.reviewer` |
| Mileage administrator | Installs the package, locks a month, sees every device and revokes any, reads the route audit | `mileage.admin` |
| Works council or data-protection contact | Reads the privacy notice of the package and the audit of route reads; takes part in the sign-off for auto-stop (Q41) | none; a human role, not a permission |

Agents are principals narrowed to their owner's groups (D18). They never see raw points by default (5.6).

## 3. What native support means

| Piece | Specification | State today |
|---|---|---|
| Typed client | A shared package `@obop/client` (working name; ARCHITECTURE 10.2 lists `@obop/rpc`, which may be the same thing, see 15): types generated from `/api/openapi.json` with `openapi-typescript` and `openapi-fetch` (already the plan in D15 and ARCHITECTURE 10.4), plus sign-in, refresh and the outbox interface. Used by the web back office and the app. Because models depend on installed modules, the generic layer reads `/api/meta` as the back office does, and the `mileage` package ships its own spec fragment. | The OpenAPI route exists (`crates/kernel/obop-http/src/lib.rs`); no shared client package yet (`web/packages` has `ui` and `e2e`) |
| Device sign-in | The native app flow of RFC 8252: the system browser (ASWebAuthenticationSession on iOS, Custom Tabs on Android), authorization code with PKCE S256, rotating refresh tokens with reuse detection. `obop-oauth` does this for MCP clients (D64) but binds tokens to the `/mcp` audience and refuses them on `/api`. Needed: a client class "mobile" with the `/api` audience, scopes narrowed the way agent tokens narrow groups, and a custom-scheme or universal-link redirect (the D64 amendment). | Server side exists for MCP; the mobile class is missing |
| Passkeys | The sign-in and consent pages already offer passkeys (D65), so they work in the system browser with no native code. Native passkey prompts inside the app need associated-domains and Digital Asset Links files on the instance domain and a matching RP id (verified by search only; a library such as react-native-passkeys exists). Not in the first slice. | D65 built |
| Token storage | The refresh token lives in the iOS Keychain or in Keystore-backed storage on Android, for example `expo-secure-store`. A background task must read it while the phone is locked; on iOS the item needs the "after first unlock" accessibility class (general knowledge, to verify). | n/a |
| Device registration and revocation | A built-in model `device` (user, name, platform, app version, push token, created, last seen, revoked). The refresh-token family is bound to the device id; revoking the device ends the family and its queued commands are refused. Devices are listed in Customize, Security beside the Connected apps table. This is the device revocation that D13 and security-privacy 6.3 already promise. | Not built |
| Offline command outbox | The command outbox of D13 for native apps: a UUIDv7 `command_id` per queued command, replay through normal domain services so permissions are checked at replay. It reuses the idempotency store of `bulk_create` (D70) and `If-Match` (D75). For mileage the unit is a batch of points keyed `trip_id:first_seq-last_seq`. Device storage is SQLite (`expo-sqlite`), not AsyncStorage or browser storage: the prior mileage app's research recorded that browser storage "may be cleared under storage pressure" and that it held raw GPS unencrypted. | D70 built, D75 core built, D13 Proposed |
| Push | Not needed in the first slice: a local notification covers "trip still recording". Later, either a push service (a third party in the data path, a connector egress under D54) or direct calls to the platform push services through `obop-connectors`. Q38. | Connectors exist |
| App updates | Store releases for native code; JavaScript-only updates through `expo-updates`. Expo documents a custom updates server (the Updates protocol) and open-source self-hosted servers exist (verified by search; their maintenance is not checked), so no dependency on Expo's cloud is needed. Native builds run locally with `expo prebuild` plus Gradle and Xcode. | n/a |

## 4. Background location

### 4.1 The options

Sources read by the lead session (verified): the Expo SDK 58 Location documentation, the Android documentation on foreground service types, the Google Play background-location policy and the Transistorsoft README. The Expo pages were those of SDK 58, not SDK 57; SDK 57 is assumed to behave the same (unverified). Apple's documentation page did not render, so the iOS rules in 4.2 are general knowledge.

| Option | What it is | Verdict |
|---|---|---|
| `expo-location` with `expo-task-manager` | `startLocationUpdatesAsync(task, options)` with a top-level `TaskManager.defineTask`; on Android the `foregroundService` option (notification title and body required); config plugin flags `isIosBackgroundLocationEnabled`, `isAndroidBackgroundLocationEnabled` and `isAndroidForegroundServiceEnabled`. It needs a development build, not Expo Go. Limits from the documentation (verified): updates stop if the user terminates the app; Android does not restart a terminated app on a location event; vendor behaviour varies (the documentation links dontkillmyapp.com); `requestBackgroundPermissionsAsync` is "Always" on iOS and "Allow Once" makes it fail silently; on Android 11 and later it opens the system settings page; foreground permission must be granted first. Free, no licence key. | **The default path of the first slice.** Gap: no motion or stationary detection, so continuous GPS drains the battery; mitigate with accuracy, a distance filter and deferred updates (the documentation says deferred updates save battery) and measure on real phones. |
| `react-native-background-geolocation` (Transistorsoft) | Mature, battery-aware, with a built-in SQLite buffer and HTTP uploader. The repository is MIT, but release builds on iOS and Android need a paid licence key (a 30-day trial for release builds; the price is not on the page). | Excluded as the default (OR-34 d): obop is completely free to build and self-host (OR-1, OR-2), and a paid key in the build path of the flagship app breaks that. An optional adapter later is not forbidden. |
| A Capacitor plugin | Not React Native. A prior mileage app used one: it worked on a simulator, then its first real drive exposed seven bugs. | Evidence only, not an option |
| An own native module (Kotlin FusedLocationProvider foreground service, Swift CLLocationManager) as an Expo module | Maximum control, highest cost, two languages to maintain. | The fallback if `expo-location` proves unreliable in the field spike |

### 4.2 Platform rules that shape the design

- **Android 14 and later (verified).** A foreground service of type `location` needs `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_LOCATION` and `android:foregroundServiceType="location"`; `ACCESS_FINE_LOCATION` or `ACCESS_COARSE_LOCATION` must be granted; and a location foreground service cannot be started from the background unless the app holds `ACCESS_BACKGROUND_LOCATION`. Consequence: a trip started by a tap while the app is visible works without any background permission; a trip that starts by itself (geofence, car Bluetooth) needs the background permission and the Play declaration. That is one more reason auto-stop and auto-start stay out (OR-34 c).
- **Google Play (verified).** Background location is allowed only for core functionality; the Permissions Declaration Form in Play Console; one background-location feature; a short demonstration video of the Android app (30 seconds or less recommended); an in-app disclosure before the runtime prompt that contains the word "location" and a background phrase such as "when the app is closed or not in use", also in the store listing and on the website. Deadlines were not found. **Unverified and important:** whether a foreground service started from a visible app counts as background access for this policy (it probably does not need the background permission, but the sentence was not found). Check it before the store listing is made (Q37).
- **iOS (general knowledge, verify).** `UIBackgroundModes: location`; "When In Use" first, then an upgrade to "Always" (a two-step prompt); the blue status indicator while recording; and the iOS 17 `CLBackgroundActivitySession`, which lets a "When In Use" app that starts recording in the foreground keep receiving locations in the background. If Expo's location module can use it (not verified), a tap-to-start trip needs no "Always" permission and App Review is much lighter. Force-quit stops updates on both platforms (Expo documentation, verified).
- **Battery.** Continuous high-accuracy GPS for a drive is expensive. Vendor battery killers on Android are the main field risk; the app needs a "keep this app running" guide, and the first real-phone test measures drain over a 30-minute drive (9).
- **Versions (verified by search; the upgrade guide is dated 2026-10-21).** Expo SDK 58 is current with React Native 0.88, six days old on 2026-10-27; SDK 57 is React Native 0.86. The owner chose SDK 57 for now (OR-34 e).

## 5. The mileage package

The package `mileage` (`packages/mileage.toml`, an `obop.toml` X1 package, Apache-2.0) declares the models, the state machine and the permissions. A small built-in Rust helper crate (`crates/modules/mileage`) is needed only for geodesic distance, which the expression language cannot compute yet.

### 5.1 Models (names Proposed)

| Model | Fields and notes |
|---|---|
| `mileage_trip` | `id`: a UUID generated on the device. `driver`: the user. `vehicle`: optional, a `fleet_vehicle` of the bundled `fleet` package (which has `odometer_km`). `purpose`: business, private or commute; a private trip keeps only its distance, or is dropped (open, 15). `started_at` and `ended_at`: D72 `Datetime`. `distance_m`: a D72 decimal **computed on the server from the points, never trusted from the client**. `integrity`: flags set by the validator (5.3). `state`: a governed select (D48) with the options in 5.2. |
| `mileage_point` | `trip`, `seq`, `recorded_at`, `lat`, `lon`, `accuracy_m`, `speed_mps`. Written only through `bulk_create` with `idempotency_key = trip:seq_range`, append-only, and rate-limited (`OBOP_BULK_PER_MINUTE`, D70). |

### 5.2 The state machine

`state` is a governed field (D48): `create` starts at the first option, `recording`, and only the actions below move it, each with its own group and approval. The proposal names the states and the actions; the arrows and the groups are this document's reading of them (Proposed).

| Action | From | To | Who |
|---|---|---|---|
| `end` | `recording` | `ended` | the driver |
| `submit` | `ended` | `in_review` | the driver |
| `confirm` | `in_review` | `confirmed` | `mileage.reviewer` |
| `reject` | `in_review` | `rejected` | `mileage.reviewer` |
| `reopen` | `rejected` | `ended` | `mileage.reviewer`, so the driver can correct and submit again |
| `lock_month` | `confirmed` | `locked` | `mileage.admin`, for every confirmed trip of a month |

A `locked` trip cannot be reopened. Agents can only propose `confirm`, `reject` and `lock_month` (approval for agents, D18, D48).

### 5.3 Server-side validation

A prior mileage app fixed its logic in the client and enforced nothing on the server; one implausible segment there produced a trip of 8,666 km. The server therefore decides:

| Rule | Value | Reason |
|---|---|---|
| Speed cap | Reject a step above a cap; the prior app used 70 m/s (about 252 km/h). The value is a package constant, Proposed. | An implausible jump must not count |
| Accuracy filter | Drop fixes whose `accuracy_m` is above a threshold. The threshold is open and set after the field test. | Poor fixes add distance |
| No zero-kilometre trips | `end` on a trip whose computed distance is 0 is refused with a reason. | A recorded trip is never saved as 0 km |
| End position | Taken from the last buffered fix. | The app may stop before the final upload |
| Duplicate batch | A batch with a known `idempotency_key` is a success, not an error. | Replays after a lost response |
| Sequence continuity | A gap in `seq` is flagged in `integrity`, not hidden. | Lost points are visible to the reviewer |
| Open too long | A trip that stays `recording` for hours is flagged. The number of hours is open. | A forgotten trip is the main risk of the manual baseline |
| Distance | Computed on the server from the accepted points with a geodesic helper. | The client's number is never trusted |

The validator and `trip-core` (8.1) share test vectors, so the device and the server agree on filtering and distance.

### 5.4 The review gate

Only `confirmed` trips count in totals, in the export and in the PDF. That is one predicate that every total, list total and export uses; a prior mileage app called it the `reviewed_at` gate. The reviewer sees the integrity flags before confirming.

### 5.5 Privacy and retention

Not legal advice; obop supports compliance and is not a certification. The points that the research raised:

- Location on the device and in transit is a new processing location for the data-protection impact assessment.
- Raw points are minimised: they are thinned or deleted a number of days after a trip is confirmed, and the distance and, if needed, a simplified route stay. The number of days is open (15).
- No third-party geocoder or map tiles by default (Q39); a prior mileage app sent exact coordinates to two public geocoding services. Start and end addresses are typed, or they come through a connector that passes the egress gate (D54).
- Every read of a route is audited (the audit log, [security-privacy.md](security-privacy.md) 4.4).
- Recording is active only while the driver started it and is visible: an Android notification, the iOS indicator.
- **An automatic trip end (auto-stop) ends a trip the driver did not end. The owner's condition is legal sign-off (OR-34 c); the proposal adds product and works-council sign-off. All three come first, and it stays out until then (Q41).** The prior research named guidance of the Swedish data-protection authority (IMY); it was not read here. Manual start and stop is the baseline, with the review gate and the open-trip warning as safeguards.
- Tests use synthetic data only.

### 5.6 Permissions and data class

The proposal names no permission groups. The groups of section 2 and this matrix are this document's reading of it (Proposed).

| Model | Driver | Reviewer | Administrator |
|---|---|---|---|
| `mileage_trip` | create, read and update own (the `own` rule, D58); actions `end`, `submit` | read all; `confirm`, `reject`, `reopen` | everything; `lock_month` |
| `mileage_point` | `bulk_create` and read own | read, audited | read, audited |
| `device` | read, rename and revoke own | none | read and revoke any |

Data class: none of the five classes fits location cleanly (`internal` is too weak, `special_category` too strong), so a sixth class `location` is Proposed, with masking for readers who need only the distance (Q40). CONVENTIONS section 5 already puts attendance geo in `confidential_hr`. Until Q40 is answered, the package declares `confidential_hr` for point fields (an interim reading of this document, not in the proposal); that class is not agent-readable by default and is excluded from the browser cache.

### 5.7 Not in the first slice

Reimbursement (confirmed trips can later become expense lines of kind `mileage` through the rate tables of people.md 4.5, with the module `expenses`; the first slice offers an export); maps; automatic start or stop; push; native passkey prompts; iOS.

## 6. The app

Languages: English first, then the seven shipped languages through the platform's label tables (D52); the order after English is open. Synthetic data only.

| Screen | Content |
|---|---|
| Home | One big Start button; the state of the last trip; the open-trip warning |
| Recording | Elapsed time, distance so far as the device sees it (the server value replaces it), the Stop button, the "keep this app running" guide; the permission and disclosure steps of 4.2 before the first start |
| Trip list | Own trips with their states |
| Trip detail | Distance, time, start and end; no map in the first slice |
| Review and confirm | For the reviewer: submitted trips, integrity flags, confirm or reject. Whether this screen belongs in the app or only in the back office is open (15) |
| Account | Devices, sign out |

The back office gets the generated views of the package (D46), so a reviewer can work without the app.

## 7. CLI, API and MCP surface

A feature is not done until it exists in the CLI, the API and MCP (OR-4, D80). The registry generates `get`, `list`, `search`, `create`, `update`, `delete` and `bulk_create` for every model, so `mileage_trip`, `mileage_point` and `device` have all of them on all three surfaces; every field carries a data class, a permission group and a description (D41). Actions: `mileage_trip.end`, `.submit`, `.confirm`, `.reject`, `.reopen` and `.lock_month`. Device revocation is `device.revoke`. The CLI spelling follows [cli.md](cli.md) (Proposed under Q22). The app calls only these commands.

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

"One background-recorded trip from a real phone", Android first (it can be built on the Windows development machine; iOS follows when a Mac is available). Work packages, in order (size is the unit of one agent package of a few hours: S, M, L):

1. Spike: an Expo app in the workspace builds a development build; `vp check` and `vp test` run on the shared packages (S).
2. `@obop/client` generated from a running instance (S to M).
3. Server: the mobile client class with the `/api` audience, the `device` model and revocation, with tests and the D74 fault rules (M).
4. The `mileage` package and ingestion through `bulk_create`; validation vectors shared with `trip-core` (M to L).
5. The app: system-browser sign-in, Start, the foreground service, the SQLite outbox, upload with airplane mode toggled during the trip, Stop, review and confirm (M to L).
6. Label tables in the shipped languages and the documentation (S).

Roughly eight packages in sequence, counting the tests and the independent review that follow each. The real-phone test is the part no agent can do alone.

**Acceptance test** (a real Android phone, a synthetic walk or drive, a test instance):

1. Given a signed-in driver with the Mileage app and a permitted location, when the driver taps Start and then locks the phone for 10 minutes while moving along a reference route, then recording continues.
2. Given airplane mode is switched on and off during the trip, when the driver taps Stop, then no point is lost: the server holds every point, in order, and a duplicate batch is accepted once.
3. The computed distance is within a tolerance of the reference route. The tolerance is stated before the test runs; this document does not set it.
4. The trip appears in the back office in `in_review` after submit. Confirming moves it to `confirmed`, and it counts once in the total; a trip that is not confirmed does not count.

## 10. Non-functional requirements

- **Security.** No secret in the app bundle; tokens only in the platform keystore; PKCE S256; the system browser only; access tokens are short-lived (one hour for the MCP clients of D64; the value for the mobile class is Proposed) and bound to the `/api` audience (the D64 amendment). A revoked device is refused at once.
- **Privacy.** 5.5. Every field of the package has a data class and a description (D41).
- **Reliability.** The outbox survives a killed app and a restart; replay is idempotent; the server computes the distance.
- **Accessibility.** The app follows the intent of WCAG 2.2 AA (D35) with the platform screen readers; no gate is defined for it yet (Proposed).
- **Performance.** No number is claimed: the battery drain over a 30-minute drive is measured on real phones (D21).
- **Test plan.** `trip-core` and the shared vectors run under `vp test`; the Rust validator and the mobile class run under `cargo test` including the D74 fault rules; a Playwright test in `web/packages/e2e` checks that the back office shows a trip in `in_review` and confirms it; a Rust test and the same Playwright suite check that revoking a device ends its refresh-token family and that the app is refused at the next call; the acceptance test of 9 runs on a real phone; all data is synthetic.

## 11. Where obop deliberately differs from Odoo

Odoo's mobile apps are Enterprise. obop has one edition under Apache-2.0 and no `ee` tree (OR-1, OR-2), so a mobile client is never a paid add-on. obop does not plan a suite of native apps: the PWA stays the default, and a native app exists only where a PWA cannot do the job (D87).

## 12. Out of scope, non-goals and later

- Automatic start or stop of a trip until the sign-off of Q41 exists; geofences and car Bluetooth triggers.
- Continuous GPS monitoring of staff. A trip is recorded only while a driver has started it, and the route is audited when read.
- Tauri mobile, a Capacitor app, an own native location module (unless the spike fails), and the Transistorsoft library in the default build.
- Maps, reverse geocoding and tile servers (Q39); push (Q38); native passkey prompts; iOS in the first slice; reimbursement and expense lines (5.7); private-trip route storage.
- Apple and Google accounts, store listings and publication by an agent (Q37).

## 13. Risks

| Risk | Mitigation |
|---|---|
| Android vendor battery killers and the 30-minute drain | Foreground service, a "keep this app running" guide, a measured drain test on real phones |
| Google Play and App Review of background location | Tap-to-start only in the first slice, the in-app disclosure, a check of the open Play question (4.2) before any listing |
| GPS cannot be tested only in emulators (route playback in the Android emulator helps; general knowledge) | The real-phone acceptance test; synthetic routes |
| The Windows development machine cannot build for iOS | Android first; iOS when a Mac is available |
| SDK and React Native churn (SDK 58 is days old) | SDK 57 now; the D23 pin policy decides the move |
| Location-data compliance | 5.5; legal review of auto-stop before it exists (Q41); not legal advice |
| Scope creep toward maps, auto-stop and reimbursement | 5.7 and 12 |
| The server enforcing what the prior app only did in its client | 5.3 and shared vectors |

## 14. What is not verified

Apple's documentation (the iOS rules are general knowledge); Expo's iOS behaviour with `CLBackgroundActivitySession`; whether a foreground service started from a visible app avoids the Google Play background-location declaration; Metro with pnpm; the accessibility classes of `expo-secure-store` items read while the phone is locked; the Transistorsoft price; the maintenance of the self-hosted Expo update servers; whether SDK 57 behaves like the SDK 58 documentation that was read; the Google Play deadlines; anything on a real phone.

## 15. Open questions

Owner questions are Q37 to Q42 in [DECISIONS.md](../DECISIONS.md) section 8. Questions that this document leaves open and that no register row covers yet:

1. **Client package name.** `@obop/client` (the proposal) or the Proposed `@obop/rpc` of ARCHITECTURE 10.2; one package should serve the web and the app.
2. **Device model.** CONVENTIONS section 9.2 proposes kiosk and POS devices as API-client principals plus a `pos_device` record (D28). Whether `device` and `pos_device` are one model or two is open.
3. **Interim data class.** `confidential_hr` for point fields until Q40 is answered (5.6).
4. **Private trips.** Keep only the distance, or drop the trip.
5. **Numbers.** The accuracy threshold, the open-trip hours, the retention days, the distance tolerance and the speed cap are open or Proposed.
6. **Reviewer scope.** All trips (the first slice), or only a manager's reports (the organisation grants of D79).
7. **Review in the app.** Whether the review screen is in the app or only in the back office.
8. **Permission groups and wire ids.** `mileage.driver`, `mileage.reviewer`, `mileage.admin` and the action names are working names until CONVENTIONS sections 3 and 4 register them.
9. **The Play question.** Whether a foreground service started from a visible app avoids the background-location declaration (4.2).

## 16. Sources

Decision IDs, owner rulings and owner questions refer to [DECISIONS.md](../DECISIONS.md): D13, D15, D18, D23, D31, D35, D37, D41, D46, D48, D52, D54, D58, D64, D65, D70, D72, D74, D75, D79, D80, D87; OR-1, OR-2, OR-4, OR-9, OR-19, OR-34; Q22, Q32, Q37 to Q42. The design proposal of 2026-10-27 (the lead session) is the source of every marking above; it is not stored in the repository. External sources named in it: the Expo SDK 58 Location documentation, the Android foreground service types, the Google Play background-location policy, the Transistorsoft README, RFC 8252 and the Expo Updates protocol. Related: [people.md](people.md), [services.md](services.md), [security-privacy.md](security-privacy.md), [ux.md](ux.md), [cli.md](cli.md), [../CONVENTIONS.md](../CONVENTIONS.md), [../ARCHITECTURE.md](../ARCHITECTURE.md).
