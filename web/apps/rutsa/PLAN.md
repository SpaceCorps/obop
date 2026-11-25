# Rutsa, stage 1: the local prototype (working plan)

Working file of the branch `feat/rutsa-prototype`. It is updated with every commit and **deleted
before the pull request is marked ready for review**. The product is described in
[docs/spec/rutsa.md](../../../docs/spec/rutsa.md); nothing here is a decision.

## Goal

Rutsa runs on a phone without a server or a sign-in. The driver presses Start, drives, presses
Stop, and sees the trip in the list with the right kilometres. A trip can also be added by hand.
Everything is stored on the phone in an encrypted database.

Covers: BR-2 (manual Start and Stop), BR-14 (recording visible, pause), BR-28 (manual trip,
without the purpose, which comes with the classification of stage 2),
BR-23 (minimal look for the prototype), US-2. BR-16 in part: one code base for both systems,
checked on the Android emulator first; iOS needs a Mac.

Not in this stage: classification, rates, the report and mail, Swedish (stage 2); places,
learning, odometer, calendar (stage 3); automatic start and stop (OR-34 c), Live Activity,
widget, the obop design (stage 4); deleting old points (the 7 days of rutsa.md 5.4 are a rule
of the server version; Rutsa needs the points for the map in the report, decided in stage 2);
moving trips to a new Android phone (stage 2, with the export). Known limit until then: on
Android the database key never leaves the phone, so a new phone starts with no trips; on iOS the
key goes with the database into an encrypted iCloud backup or a transfer, so a restore keeps them.

## How it is tested

| Where                       | GPS                                                                                      | Who                        |
| --------------------------- | ---------------------------------------------------------------------------------------- | -------------------------- |
| Android emulator on Windows | A script feeds a route to the emulator (`adb emu geo fix`); no phone and no drive needed | Developer                  |
| iOS Simulator on a Mac      | `xcrun simctl location` feeds the same route (as `mileage-app` `simulator:route`)        | Owner (Mac)                |
| A real iPhone               | A real drive; built with Xcode on a Mac                                                  | Owner, later the developer |

`vp check` and `vp test` run on any system; blocks A and B need no emulator.

## Commits

Status: `[ ]` to do, `[x]` done (with the commit), `[~]` in progress. The order follows the
imports: a commit uses only what earlier commits brought. A package and its Expo config plugin
come in the same commit (a plugin without its package breaks the build). Paths are under `src/`,
except those that start with `config/` or `scripts/`.

### A. Skeleton

- [x] A1 `feat(rutsa): app package on Expo SDK 58`: `package.json` (`@obop/rutsa`, Apache-2.0,
      only the packages the skeleton uses), `tsconfig.json`, `vite.config.ts`, `.gitignore`,
      `index.ts`, `App.tsx` showing "Rutsa", `pnpm-lock.yaml`
- [x] A2 `feat(rutsa): app config and placeholder identity`: `config/identity.ts` (placeholder id
      `org.obop.rutsa`, marked like `org.obop.mileage`; version), `config/native-texts.ts` (the
      texts the system shows: the name under the icon; the text of the iOS location dialog comes
      in C2), `config/ios.ts` (`iosBundleId`: `OBOP_IOS_BUNDLE_ID` for a test build signed with a
      free Apple ID), their tests, `app.config.ts` (name, portrait, light and dark, Android
      package, minSdk 26, no cloud backup, the Android permissions that libraries add and the app
      never uses blocked as in `mileage-app`, iOS bundle id)
- [x] A3 `feat(rutsa): build scripts`: `expo-dev-client` with the removal of its iOS entries from
      builds that are not development builds (`config/ios.ts`); `scripts/check-sdk.mjs`,
      `scripts/gradle.mjs`, `scripts/ios/xcode.mjs`, `xcode-lib.mjs` and its test after
      `mileage-app`;
      the `vp run` scripts `start`, `check:sdk`, `android:prebuild`, `android:build`,
      `android:build:release` (an APK that runs without Metro), `ios:prebuild`, `ios:pods`,
      `ios:build:simulator`, `ios:run:device`. The iOS release gate waits for stage 4
- [x] A4 `ci: check and test apps/rutsa`: both loops of `.github/workflows/ci.yml`

Checkpoint: `vp check` and `vp test` pass.

### B. Logic without a phone (`vp test`)

- [x] B1 `feat(rutsa): trip and point types`: `core/types.ts` (trip: id, start, end, state
      `recording`, `paused` or `ended`, distance, source `recorded` or `manual`, segment, from, to,
      note; point: time, lat, lon, accuracy, speed, segment)
- [x] B2 `feat(rutsa): SQL layer`: `core/sql.ts` (from `mileage-app`) and its tests (queue,
      commit, rollback), a test database on `node:sqlite` (`test-support/memory-db.ts`)
- [x] B3 `feat(rutsa): database schema`: `core/schema.ts` (`trip`, `point`; the version in
      `PRAGMA user_version`, ordered migrations in one transaction, a newer file refused; STRICT
      tables with CHECK rules, one active trip, points deleted with their trip) and tests; foreign
      keys switched on in `test-support/memory-db.ts`
- [x] B4 `feat(rutsa): fix conversion and thinning`: `core/fix.ts` (`toFix`, `plausible`),
      `core/thin.ts` (`keepFix`; `acceptFixes`, the choice of the fixes of one delivery that are
      stored) from `mileage-app`; `thin.ts` uses `@obop/trip-core`; tests
- [x] B5 `feat(rutsa): trip repository`: `core/repository.ts`, one interface (start, the active
      trip, add points, pause, resume, stop, add by hand, get, list, points, delete) and its
      errors (already active, not found, wrong state), so a server can be added later
- [x] B6 `feat(rutsa): local trip store`: `core/sql-store.ts` over a migrated database; one active
      trip at most; ids from a generator the caller passes in; fixes chosen by `acceptFixes` in the
      transaction of the write; at Stop the distance is `analyseTrack` of `@obop/trip-core` per
      segment (a pause starts a new segment, so the gap is not counted); tests
- [x] B7 `feat(rutsa): totals and formatting`: `core/present.ts` (km, clock time, duration, trips
      by day, today and this month; times are stored in UTC, days follow the time zone given, the
      phone's own; numbers and times through `Intl` for the language given, no words) and tests
- [x] B8 `feat(rutsa): English texts`: `i18n/index.ts` (`t`, `tp`), `i18n/en.ts` with the
      sentences needed so far (the notification of C4, common words, the store's errors, days,
      durations, trip counts) and tests; each screen of D adds its own sentences

Checkpoint: `vp test` passes.

### C. The phone

Before C: JDK 17, the Android SDK and the emulator installed (list in `docs/spec/mobile.md`,
"Android build on Windows").

- [x] C1 `feat(rutsa): encrypted database on the phone`: `core/open-db.ts` (the key and the file:
      first start, a key without a file, a key that does not fit, a stale file, a pending key,
      plain SQLite refused) with `test-support/fake-file.ts` and tests; `native/keychain.ts` (iOS
      class `AFTER_FIRST_UNLOCK`, so a restore keeps the trips),
      `native/database.ts` (32 random bytes in the Keystore or Keychain, SQLCipher),
      `native/store.ts` (the one store of the process; `PRAGMA foreign_keys = ON` and `migrate` at
      open); the database file name and the key name in
      `config/identity.ts`, pinned by a test (renaming either loses the trips of an installed
      app); plugins of `expo-sqlite` (SQLCipher) and
      `expo-secure-store` (no Face ID text). After `mileage-app` without the session and the
      server
- [x] C2 `feat(rutsa): location permissions`: `core/liveness.ts` (readiness: permission, precise
      location, location services) and its tests, `native/permissions.ts` (the cases of location
      access, the system dialog, the notification permission of Android 13 for the recording's
      notification, the app's page in the settings); the text of the iOS location dialog in
      `config/native-texts.ts`; plugin of `expo-location` (foreground service, background mode
      `location`, "while using" only)
- [x] C3 `feat(rutsa): background location task`: `native/config.ts` (task name), `native/bus.ts`
      (change signal), `native/task.ts` imported in `index.ts` before the app is registered, what
      it does with a delivery in `core/delivery.ts` and its tests; `RECEIVE_BOOT_COMPLETED` on
      Android; the unused `fetch` mode removed on iOS (`config/ios.ts`)
- [x] C4 `feat(rutsa): start, pause, resume and stop`: `native/recorder.ts`, `native/actions.ts`; the
      order of the steps and the rollbacks in `core/recording.ts` and its tests; the Android
      notification "recording" while a trip runs; a temporary panel in `App.tsx` to try it
- [x] C5 `feat(rutsa): recover a recording after a restart`: recording health in
      `core/liveness.ts` (no position for a while, measured from the start of the segment) and
      tests; `ensure` in `core/recording.ts` and its tests: when the app opens or comes back to the
      front with a trip that is recording, it starts the location updates again, and stops updates
      left running for a paused trip or none; `isRecording` in `native/recorder.ts`; the temporary
      panel shows the health, whether the updates are registered and what the check did (the
      screen of D4 shows the health)

### D. Screens

Components from `@obop/ui-native`.

- [x] D1 `feat(rutsa): navigation and data hooks`: `ui/router.ts` (the routes, the stack, the
      gate loading / failed / app, the open trip first; location is a route, never a gate, BR-36)
      and its tests, `ui/hooks.ts` (the store, the trips, the clock, the check of a recording of
      C5, moved out of the temporary panel), `ui/foreground.ts` (now and on every return to the
      front) and its test; the Android back button; the screen "the trips could not be opened"
      (`ui/screens/Failed.tsx`: the reason, Try again, Stop recording, Erase trips after two
      confirmations where erasing can help, not for a newer version or a build without SQLCipher)
      with `databaseProblem` and `canErase` in `core/open-db.ts`, `eraseDatabase` in
      `native/database.ts` and `delete` in `native/keychain.ts`, and a temporary build flag
      `EXPO_PUBLIC_RUTSA_TEST_DB_FAILURE` (Metro writes it into the bundle, no new package) to try
      it on the emulator (removed before F3); the temporary panel moves to `ui/screens/Panel.tsx`
      and stays the home route until D3
- [x] D2 `feat(rutsa): permissions screen`: why location is needed, what is stored and where (on
      the phone only), then the system dialog of location and of notifications; without location
      (BR-36, US-18) a screen for each case (refused for good, approximate only, location off on
      the phone) with Open Settings and Close, and a new check when the app comes back to the
      front, which goes on by itself; the rest of the app works without location; before the
      first trip a note to keep the app running (Android: battery use Unrestricted; iOS: not
      closed from the app switcher), and a note when the notifications of Android 13 are refused
      (the trip records but is not shown, BR-14). `ui/screens/Permissions.tsx`, the steps in
      `ui/permission-step.ts` and its tests; the reading of the explanation is kept in the table
      `meta` (migration 3) through `core/settings.ts`; `tOn` and `ui/os-text.ts` for the texts of
      one system; `openLocationSettings` opens the location settings of Android
- [x] D3 `feat(rutsa): home screen`: Start; km today and this month; a note when a database file
      from a backup that no key could open was replaced at this start; a status banner at the top
      of every screen while the phone cannot record well: location (off on the phone, no
      permission, approximate only, BR-36) and the notifications of Android 13 (the trip records
      but is not shown, BR-14), each said only when it applies; Open settings (the location
      settings when location is off, else the app's page) and a close button that folds the
      banner up until the next start of the app; it goes away by itself when the cause is gone,
      checked again on every return to the front; made in the app from `@obop/ui-native` parts;
      the rest of the screen works as usual
- [x] D4 `feat(rutsa): recording on the home screen`: the recording is shown on the home screen,
      not on a screen of its own. The big button of D3 is Start; while a trip records it turns
      red and says Stop, with a small Pause button at its left; while the trip is paused the big
      button says Resume and the small button at its left says Stop (words only, no symbols yet).
      Stop asks to confirm. While a trip is open the screen shows it as Current, its elapsed time
      and km so far (the same count as Stop's, over the segments), next to the totals of today and
      this month, which leave the open trip out; at Stop its km join Today and This month and
      Current goes. The warnings of a recording stay, for the
      driver and not only for the tests: a note when no position has come for a while or the
      updates are not registered (C5), with a button that starts the recording again and a
      message when that fails; the reason when the check of C5 finds that the phone cannot
      record, and a Resume that cannot record, lead to the permissions (`next: "resume"`); the
      notifications that are off are said by the status banner of D3. The temporary panel, its
      lines for testing (health, service, check) and the recording route go; the permissions end
      on the home screen
- [x] D4b `feat(rutsa): trip details at Stop`: Stop opens a window over the screen with the
      details of the trip, all optional: Name, From, To (a recorded trip has no addresses, they
      can be typed here), Note; Cancel keeps recording, "Stop trip" ends the trip with what was
      typed, also with nothing typed (Stop is never blocked). It replaces the confirmation of
      Stop of D4. A column for the name (migration 5); From, To and Note exist already. A switch
      "Details at Stop" in the footer of the home screen, under the small button and moving
      nothing: on by default; off, Stop ends the trip at once, with no window and no
      confirmation. The switch is kept in `meta`. It is a compromise, not an answer: how a trip
      is best ended (with a name or without, at once or with a step) is an open question, and the
      switch keeps both ways until it is answered (see "Proposed for stage 2"). The details can
      be typed later too, on the trip's screen (D6) and in its edit (D8), since a trip can end
      without the window (the switch off, an app ended by Android, the automatic stop of stage 4).
      The category (work, private, commute) joins this window in stage 2, with the classification
- [x] D5 `feat(rutsa): trips list`: trips by day; each trip shows its kind (a trip added by hand
      is marked; mixed comes with the edit of a trip, stage 2) and its name, else From → To, else
      its times
- [x] D6 `feat(rutsa): trip detail`: its name ("Untitled trip N" for a trip without one, numbered
      among the untitled trips), times, km, duration (without the pauses), the positions stored
      and those the distance left out, its details (From, To, Note) changed in place (promised by
      D4b), delete; the edit of its parts is in stage 2
- [x] D7 `feat(rutsa): manual add`: the last step of the screens. A button "Add trip" in the header
      of Recent trips on the home screen, at the right of "Open history" (which stays a link and
      moves to its left), a small filled button of the main colour, opens a screen of its own (the
      route `add`), in the style of the trip's screen: Back and Save in its footer. It adds a new
      trip typed by hand: name (optional; untitled gets its number), date, start and end times
      (chosen with the pickers of the system, 24 h; an end before the start is the next day, said
      under the field), km (a dot or a comma), From and To (required text, the addresses in
      effect), Note (optional, for example why it was typed). The trip is of the kind manual
      (BR-28), marked "Added by hand" in History and in Recent trips. The checks, each under its
      field: a real date and times, no start or end in the future, km over 0 and under a sane
      maximum (about 2,000 km), no overlap with another trip, the open one included (its km would
      count twice); a dialog, not a refusal, when the average speed is over about 150 km/h. Times
      are typed in the phone's time zone and stored in UTC, like recorded ones. Reachable without location (US-18 AC1). The edit of the parts of a
      trip (the old D8) moved to stage 2
      Checkpoint: the app runs on the emulator; every check of `CHECKS.md` is run; Start, Pause, Resume,
      Stop, History, a trip's screen and a trip added by hand work when tried by hand; the clock
      times and km on the screen are right (`Intl` of Hermes:
      `mileage-app` notes that an engine without data for a language or time zone falls back quietly).

### E. Check on the emulator and the simulator

- [ ] E1 `feat(rutsa): route scripts`: `scripts/emulator/route.mjs` (the synthetic route and its
      reference length), `scripts/emulator/drive.mjs` (feeds it with `adb emu geo fix`, without
      the airplane mode schedule of `mileage-app`, which tests its upload),
      `scripts/simulator/route.mjs` and `route-lib.mjs` (`xcrun simctl location`),
      `scripts/lib/is-main.mjs`
- [ ] E2 `test(rutsa): check a recorded trip on the emulator`: Start and Stop are pressed in the
      app (by hand or with `adb shell input`); the script feeds the route, reads the km on the
      screen (`adb shell uiautomator dump`) and compares it with the reference length

Checkpoint: the APK builds and installs, a route gives a trip with the expected km.

### F. Documentation

- [ ] F1 `docs(rutsa): the local prototype`: `docs/spec/rutsa.md` 1.1 describes two products
      (Rutsa, local, no sign-in; companies on obop use the `mileage` module with the obop phone
      app); phase 1 becomes the local prototype (the calendar reminder moves to stage 3); 5.5
      without "Calls to obop" for Rutsa; what was run and what was not; the proposals below for
      stage 2 go into the spec as PROPOSED, so they outlive this file
- [ ] F2 `docs: add the Rutsa app to the README`
- [ ] F3 delete this file and `CHECKS.md`, then "Ready for review"

## Proposed for stage 2 (not built in this stage)

**The edit of a trip** (D8 of stage 1, moved here on 2026-11-25, after D6 gave the details an edit
in place and D7 adds a forgotten trip whole; what was decided stays): a trip that ended is made of
parts in the order of their times: recorded parts (the segments of GPS between pauses) and typed
parts (start, end, km, From, To, typed by hand; a table of typed parts, a migration). Its km are
the sum of the km of its parts and its time the sum of their times; the gaps between parts
(pauses, a part with no recording) count for neither. Its start is the start of its earliest part
and its end the end of its latest. Its kind follows its parts: recorded, manual (D7) or mixed; the
list and its screen show it, and the report says the km measured (the recorded parts) and the km
typed. The edit screen, opened from the trip's screen, lists the parts top to bottom, each with its
times, km, From and To and a mark of GPS or typed; a typed part can be changed or deleted, "Add a
part" adds one; a recorded part cannot be changed (its km and times were measured) but can be
deleted (a stretch the GPS recorded by mistake), and the km measured go down with it; under the
list the totals and the kind. The list sorts itself by time: when a changed part loses the focus
it moves to its place (no dragging). A trip can be edited as often as the driver likes. The open
trip cannot be edited: a trip is recorded first, and what was forgotten (a late Start) is added
after Stop. The checks of a part: not in the future, its end after its start, no overlap with
another part or another trip, its km and its speed as in D7. It belongs with the report of stage
2, which separates the km measured and typed.

**The GPS side of a manual add** (asked for on 2026-11-24; the manual add is D7, the edit of a
trip is above): in stage 1 From and To of a typed trip or part are text and its km are the
driver's.

- A route or places picked on a map for a typed trip or part, instead of text.
- Dragging the parts of a trip in its edit screen, if the sorting by time is not enough.
- Typed km kept honest: besides the mark in the report (which says how many km were measured and
  how many typed), the shortest route between the places the driver gives could be computed and a
  large excess shown. That needs a routing service and a geocoder, so the network and addresses
  leaving the phone, against the local-only idea of Rutsa: a privacy decision goes with it.
- A company setting that forbids typed km, in the version for companies.

**The driver at the top of the home screen** (a placeholder since D6, `ui/UserBadge.tsx`): their
name and avatar, and what a tap opens (a profile, the settings with the language and the app lock
of BR-35). Stage 1 has no sign-in; where the name comes from is decided with it.

**Statuses of a trip**: today a trip is open (recording or paused) or ended. A later state, such as
a trip in a sent report, could lock it against an edit.

**How a trip is ended** (answered for stage 1 on 2026-11-25, during D6): Stop asks in a dialog and
ends the trip; the details are typed afterwards on the trip's screen. The window of D4b and its
switch went. Asking for a category at Stop may come back with the classification of stage 2 (BR-4,
US-3, US-4), together with the end of a trip from its notification (BR-27) and the automatic stop
of stage 4.

**Joining two trips** that follow each other, in the edit of a trip (above): for a trip split in two by
a Stop pressed by mistake.

## Dependencies

Versions are exactly those of `mileage-app` (they have passed the cooldown of the pin policy,
D23).

| Commit    | Packages                                                                                                                                                                                                          |
| --------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A1        | `expo` 58.0.16, `react` 19.3.0, `react-native` 0.88.0, `expo-status-bar` 58.0.3, `react-native-safe-area-context` 5.10.1, `@obop/ui-native`; dev: `typescript` 6.0.3, `@types/react` 19.3.0, `@types/node` 26.6.5 |
| A2        | `expo-system-ui` 58.0.6 (light and dark on Android)                                                                                                                                                               |
| A3        | `expo-dev-client` 58.0.14 (the development build of `vp run start`)                                                                                                                                               |
| B4        | `@obop/trip-core`                                                                                                                                                                                                 |
| C1        | `expo-sqlite` 58.0.13, `expo-secure-store` 58.0.2, `expo-crypto` 58.0.7                                                                                                                                           |
| C2        | `expo-location` 58.0.18                                                                                                                                                                                           |
| C3        | `expo-task-manager` 58.0.12                                                                                                                                                                                       |
| D7        | `@react-native-community/datetimepicker` 9.2.1 (the version of Expo SDK 58, MIT; the pickers of date and time of the system)                                                                                      |
| Not taken | `@obop/client`, `expo-web-browser` (no sign-in); `expo-constants` (the server settings of `mileage-app`); `react-dom` (no stated reason in `mileage-app`; added only if a check needs it, with the reason)        |

## Taken from `mileage-app`, and left out

| Taken (copied and simplified)                                      | Left out                                            |
| ------------------------------------------------------------------ | --------------------------------------------------- |
| `sql.ts`, `fix.ts`, `thin.ts`, `liveness.ts`                       | `@obop/client`, `auth.ts`, `SignIn.tsx`: no sign-in |
| `recorder.ts`, `task.ts`, `permissions.ts`, `bus.ts`               | `sync.ts`, `engine.ts`, `api.ts`: no server         |
| `keychain.ts`, `database.ts`, `store.ts`, `i18n/`                  | `host-config.ts`, `host-state.ts`: the host app's   |
| `check-sdk.mjs`, `gradle.mjs`, iOS, emulator and simulator scripts | `Foreign.tsx`: no other person's trips              |
| The shape of `app.config.ts`                                       | `schema.ts` as a whole (1,242 lines for the server) |

## Log

- 2026-11-16: plan written.
- 2026-11-16: dependencies are added per commit, not all in A1 (section "Dependencies").
- 2026-11-16: commits checked against the imports of `mileage-app`: each plugin with its package,
  the native texts in the app config (A2), `trip-core` with the thinning (B4), English texts
  before the notification (B8); a `paused` state, days in the phone's time zone, the storage names
  pinned, a release APK, and the known limit of a new phone.
- 2026-11-16: last pass: Resume named with Pause (C4, D4), a manual checkpoint after D, paths under
  `src/`, the taken files listed in full.
- 2026-11-16: checked against the code of `mileage-app` line by line: the blocked permissions in
  the app config (A2), C5 without a screen (its note is in D4), typed date and time (D7), the
  emulator route fed by `drive.mjs` (E1), Start and Stop pressed in the app (E2).
- 2026-11-16: A3 without `scripts/ios/env.sh` (the tool paths of the owner's Mac and the Mileage
  UI test folder) and with the test of `xcode-lib.mjs`.
- 2026-11-18: B1 has `from` and `to` (D7 types them); latitude and longitude are numbers, not the
  texts `mileage-app` sends to its server.
- 2026-11-18: B2 has a test of its own (`mileage-app` tests `sql.ts` only through its store).
- 2026-11-18: B3 keeps the version in `PRAGMA user_version` instead of a `meta` table; the other
  tables (vehicle, recipient, report, setting, place, rule, odometer) come with their stages as
  new migrations.
- 2026-11-18: B4 takes `plausible` (in `values.ts` of `mileage-app`) into `fix.ts`, and the choice
  of the fixes to store (inside `appendFixes` of `mileage-app`) as the pure `acceptFixes`, so B6
  only writes what it returns; not taken: `thinningFor` (a switch of the owner's test builds).
- 2026-11-18: B5 names the active trip and the points of a trip too; every time is given by the
  caller, so B6 tests exact times.
- 2026-11-18: B6 refuses a typed trip that ends before it starts or has a distance below 0 with a
  `RangeError` (D7 checks the input first); a Stop at a time before the start (a clock set back)
  ends the trip at its start.
- 2026-11-18: B7 has no words (B8 comes after it): the duration is hours and minutes, the screens
  write "Today" and "min"; a trip belongs to the day and month of its start, compared by day keys.
- 2026-11-18: B8 is the mechanism and the sentences known now; the screens of D add theirs, so the
  catalogue has no unused key. Not taken yet: the Swedish catalogue, the language choice and the
  translation checks of `mileage-app` (stage 2).
- 2026-11-18: the skeleton built (`android:build:release`, 14 min the first time) and ran on the
  Android 16 emulator on Windows.
- 2026-11-18: C1 takes the open procedure of `mileage-app` (`core/open-outbox.ts`) into
  `core/open-db.ts`, which the plan had left out; the stale file is recovered on both systems; the
  iOS Keychain class is `AFTER_FIRST_UNLOCK`, not "this device only", because the phone holds the
  only copy; `App.tsx` shows a line with the number of trips and the SQLCipher version until the
  screens of D replace it.
- 2026-11-18: outside the plan, `docs/spec/rutsa.md` gained BR-35 and US-17 (an app lock with a PIN
  or a password, before the release); not built in this stage.
- 2026-11-18: `docs/spec/rutsa.md` gained BR-36 and US-18 (the app without location: a reason for
  each case, Open Settings and Close, a new check on coming back); D2 builds them.
- 2026-11-18: C2 asks for the notification permission of Android 13 too (`requestNotifications`,
  from `mileage-app`), which the plan had left out: without it the notification of BR-14 is not
  shown.
- 2026-11-20: C3 takes the body of the task of `mileage-app` into the pure `core/delivery.ts`
  (`storeDelivery`), tested against a real store; `native/task.ts` only stops the updates and
  signals the screens.
- 2026-11-20: C4 puts the order of the steps (check, store, location updates) and the rollbacks in
  the pure `core/recording.ts`, tested with a stand-in for the location updates; Pause and Resume,
  which `mileage-app` does not have, stop and start the updates; a Start that the system refuses
  ends and deletes its trip (`deleteTrip` takes only an ended one). `App.tsx` has a temporary
  panel (Start, Pause, Resume, Stop, the state and the points) so C3 and C4 can be tried on the
  emulator before block D, which replaces it.
- 2026-11-20: an audit of B and C against the spec, the callers in `mileage-app`, our own additions
  and the edge cases found that after Resume the phone may hand over first the cached position of
  the pause, and the jump from there counted the road driven during the pause. Fixed outside the
  list of commits (`fix(rutsa): drop positions from before a resume`): migration 2 adds the table
  `segment` (the start of each segment; the trips of version 1 get their segments at the start of
  the trip), `acceptFixes` and the distance at Stop take the start of the segment, and a trip has
  `segmentStartedAt`, which C5 uses for the health of a recording. The tests the audit found
  missing (the app config as prebuild gets it, the native permissions, the task and the recorder,
  the Keychain class, a failed open of the database) come in the next commit,
  `test(rutsa): native wiring and app config`.
- 2026-11-21: C5 compared with `mileage-app`: the check every 5 s that the updates are still
  registered (`isRecording`) comes in C5; the button that starts the recording again, with a
  message when that fails, goes to D4. Unlike `mileage-app`, the check runs when the app opens on
  any screen, and also stops updates left running for a paused trip or none.
- 2026-11-21: block D checked against `docs/spec/rutsa.md` and the screens of `mileage-app` before
  D1. D1 gains the screen for a database that cannot be opened (`Failed` of `mileage-app`: our
  `open-db` leaves a file that no key fits alone, so without Erase the app would stay on the
  error), the helper for a return to the front and the check of C5 at the level of the app; D2
  gains the note to keep the app running and the note on refused notifications (both from
  `mileage-app`); D3 gains the note on a replaced backup file.
- 2026-11-22: D2 keeps the reading of the explanation in a new table `meta` (migration 3,
  `core/settings.ts`; `mileage-app` keeps it in the meta table of its host), and goes on by itself
  when the driver comes back from the settings with location allowed (US-18 AC5; `mileage-app`
  has a Retry button there). A start that goes on by itself does not pass the note about the
  notifications, so D4 shows that note while they are off. A return counts only after a move to
  the background that no dialog of the app caused (`onReturnFromAway`: on Android a permission
  dialog sends the app to the background), and the actions on a recording run one after another
  (`core/serial.ts`), so the check of C5 on the same return cannot stop the updates of that start.
  Tried on the emulator: the dialog of the notifications came again before every start (Android 13
  gives a second chance after a refusal) and on the start after a return from the settings; it is
  now shown once in the life of the app (`notifications_asked_at` in `meta`), only after a tap,
  and otherwise the app only reads the answer (`notificationsStep`, `notificationsAllowed`).
  D3 gains a quiet note on the home screen while the phone cannot record (asked for while trying
  D2; `mileage-app` has none): the driver sees it before pressing Start. A placeholder where a map
  needs a position comes with the map (stage 2 or later).
- 2026-11-24: the note of D3 becomes a status banner at the top of every screen, asked for after
  trying D2: it says what keeps the phone from recording well, location and the notifications of
  Android 13, each only when it applies (a driver who refused the notifications would otherwise
  forget why a trip is not shown); Open settings and a close button that folds it up until the
  next start of the app; it leaves by itself when the cause is gone. D4 no longer has its own
  note on the notifications. `main` had nothing new (fetched, already merged).
- 2026-11-24: from D3 on, a step is tried on the emulator with a short smoke test only; all its
  manual checks go into `CHECKS.md` (with those already run for C, D1 and D2), and the whole list
  is run at the checkpoint after block D.
- 2026-11-24: D3 also shows the last trip and a note while a trip is open (both from the Home of
  `mileage-app`); the row to the trips comes with D5. The temporary panel becomes the recording
  route until D4 (Start is on the home screen; the permissions lead to the recording), and the
  stack starts on it when a trip is open (`openTripFirst` of D1). The banner takes the top inset
  and the screen under it gets none; a problem closed in it stays closed until the next start of
  the app, a new one shows it again. `present.ts` gains `formatShortDate`, `dayBefore` and
  `phoneTimeZone`. Tried on the emulator: the banner is one low line without a title ("Location is
  off.", "Notifications are off.", "Location and notifications are off.", one sentence per case in
  the catalogue, `bannerText`), with Open settings and × in the middle at the right. Asked for, to
  be decided at the analysis of D4: the recording shown on the home screen while a trip is open,
  instead of a screen of its own.
- 2026-11-24: decided after D3: D4 shows the recording on the home screen (`mileage-app` has a
  screen of its own). The big button goes Start, then a red Stop with a small Pause at its left,
  then Resume while paused; the elapsed time and km so far stand with the totals. The lines of the
  temporary panel for testing (health, service, check) are not kept: what the driver needs of them
  becomes the notes of D4. Open at D4: where Stop is while the trip is paused. Decided the same
  day: while paused, the big button says Resume and the small one at its left says Stop; the
  warnings of a recording (no position, updates not registered, Restart recording, the reason the
  phone cannot record) stay, since a driver must see that a trip is not recorded; the checks of
  C in `CHECKS.md` then read them instead of the lines for testing.
- 2026-11-24: D4 takes from the Recording screen of `mileage-app`: the elapsed time, the count read
  at most every 4 s (`core/progress-reader.ts`), the note on 0 km (`core/signal.ts`), "waiting for
  the first position", the confirmation of Stop, the warnings with Restart recording, and the
  words for a screen reader (`ui/announce.ts`); not its count of points, its upload and its banner
  "Keep this app running" (the note of D2 says it before the first trip). The count while a trip
  records comes from the store (`progress`), by the same rule as Stop (`core/progress.ts`,
  `analyseRecorded`, over the segments), so Current and the stored km are the same. The buttons
  are pinned at the bottom. Without a trip open, the banner keeps every problem; with one, only
  the notifications, since the warning on the home screen says the location with its way on. The
  route `recording`, `openTripFirst` and the temporary panel are gone; `permissions` also takes
  `next: "resume"` and `next: "restart"`. Unused sentences of the panel left the catalogue.
  Tried on the emulator, then changed: the clock of Current stands while the trip is paused
  (migration 4 gives a segment its end, set at Pause and at Stop; the trips of version 3 get the
  ends that can be known: a segment the trip went on from ends at its newest position, the last
  one of an ended trip with the trip, the open one of a paused trip at its newest position;
  `closedMs` of the count, `elapsedMs`); the footer has three columns, so the big button stays in
  the middle when the small one comes or goes, the small one has one width, and only the pressed
  button shows that it works; the count is read again at once when the trip is paused or
  resumed; the note on the notifications of the guide (D2) is read again on the return from the
  settings and goes once they are on. Tried again: the three columns pushed the big button to the
  right (the left one could not shrink below the small button), so the big button is centred on
  its own and the small one lies over the top left corner of the footer, out of the flow; the
  last trip line also says how long the trip took (a recorded one without its pauses, the sum of
  its segments; one added by hand from start to end, `tripDurationMs`).
- 2026-11-24: asked for after D4: D7 gains its checks (future, order, midnight, overlap, km,
  plausible speed, time zone), and a new step D8 "started late" adds a part typed by hand to the
  open trip or a recorded one, before or after its recorded part (the open trip only before,
  since it has no end yet), with three kinds of trip (recorded, manual, mixed). The same day,
  after the questions of its fields and of typed km that can be inflated: "Started late" leaves
  block D and stage 1; it is written down under "Proposed for stage 2" with what was decided,
  its pitfalls and what is open, and goes into the spec at F1. Changed the same day: with the
  model of D7 (times, km, From and To as required text, a note, the mark) the fields are known,
  and the honesty of typed km is the same question for a trip and a part; so D7 and the old D8
  become one "Manual add": D7 the window and a new trip, D8 the same window adding a part to an
  existing trip (before or after) or to the start of the open one, stored at once; Current counts
  the typed part in its km and its time; only the GPS side of a manual add and the statuses of a
  trip stay proposed for stage 2. Changed again the same day: the window asks for the target
  first (new trip, the current one, an existing one with Before or After) and then for the fields
  of that target, instead of the data first and the targets that fit them after: the driver
  types the times while seeing the trip, and a part touches its trip, so it needs only one time
  (its start before, its end after). Final, the same day: no "Started late" and no adding to a
  trip from the window. D7 "Manual add" adds a new trip only; D8 "Edit a trip" opens from the
  trip's screen and shows the trip as its parts sorted by time (recorded parts that can only be
  deleted, typed parts that can be added, changed and deleted, each with its own start and end;
  gaps between parts are allowed and count for nothing); km and time are the sums of the parts;
  the kind (recorded, manual, mixed) follows the parts, and the km measured and typed come from
  them, so nothing else needs keeping for honesty; the list sorts itself when a changed part
  loses the focus; only an ended trip can be edited, as often as the driver likes.
- 2026-11-24: before D5, a new step D4b: Stop opens a window with the details of the trip (Name,
  From, To, Note, all optional; Cancel or "Stop trip"), which replaces the confirmation of D4,
  and a switch "Details at Stop" (on by default, kept in `meta`) ends a trip at once when off.
  The switch is a compromise while the question of how a trip is best ended stays open; it and
  the joining of two trips split by a Stop pressed by mistake are written under "Proposed for
  stage 2". The list of D5 shows the name, else From → To, else the times.
- 2026-11-24: D4b as built: the rules of the details (trimmed, optional, at most 100 characters
  for the name, 200 for From and To, 1,000 for the note, counted in code points) are in
  `core/details.ts`, used by the window (a message under the field) and by the store, which
  stores them in the transaction of Stop and refuses a field that is too long before anything is
  written (`DetailsTooLong`); `Recording.stop` checks them before the updates are stopped. The
  window is a `Modal` of React Native (`ui/StopSheet.tsx`); the switch is a `Switch` of React
  Native with its label, over the bottom left corner of the footer; its value is `stop_details`
  in `meta`. `ui/trip-label.ts` (`tripTitle`) gives the name, else From → To, else nothing; the
  last trip of the home screen already uses it. Not in `docs/spec/rutsa.md` (BR-28 has only the trip added by
  hand); the spec is brought in line at F1.
- 2026-11-24: D5 as built: `ui/screens/Trips.tsx` shows the days of `byDay` (B7) newest first, each
  headed `Today · 2 trips · 12.5 km` (Yesterday, else the weekday and date of `formatDay`, with the
  year for another year); a trip is a `Row` with its name, else From → To, else its times
  (`08:30–09:15`; the times go under a name), its km and "Added by hand" for a manual trip
  (`ui/trips-list.ts`). The open trip is in its day with Recording or Paused and no km (the km of
  the day leave it out, as Today does) and leads to the home screen; an ended one opens `trip`
  (D6). The home screen has a row Trips with their number under the last trip (`Row` "Trips" of
  the Home of `mileage-app`, which has it in the footer, where Rutsa has its buttons). No duration
  in the list (the screen of the trip has it, D6) and a plain list in the scrolling `Screen` as in
  `mileage-app`, not a `SectionList` (it would not virtualise inside the `ScrollView` of `Screen`).
  Tried on the emulator, then changed: the open trip stands alone on top under a header Current in
  the style of the days, and the days count only the trips that ended (`tripSections`); every day
  but today is named by its date, with no "Yesterday"; the last trip line of the home screen goes
  into the row Trips as its subtitle, "Last trip: 0.5 km, Today 17:17" (km and when, without the
  name and the duration of D4 and D4b: the list has the name, the trip's screen the duration).
  Then: the list and its row on the home screen are called History. A temporary button added ten
  sample trips on the five days before today to the emulator's database (recorded ones through
  Start, positions and Stop of the store, two added by hand), so the days could be looked at; it
  was used once and removed before the commit, and the trips stay in that database for the checks.
- 2026-11-24: D6 as built: `ui/screens/TripDetail.tsx` (route `trip`) has the name, else From →
  To, else "Trip" as its title; rows Date (`Mon, Nov 23`, the year for another year), Time
  (`08:10–08:42`, an end on another day with its day: `23:40–00:20 (Sat, Nov 21)`,
  `ui/trip-view.ts`), Duration (`tripDurationMs`: a recorded trip without its pauses), Distance,
  Kind (Recorded or Added by hand) and, for a recorded trip, Positions (`312 positions, 4 left
out`: `progress` gains `dropped`, the positions the distance leaves out for their accuracy or
  speed, the local stand-in for the server's checks in `mileage-app`); then Name, From, To and
  Note ("Not given" when empty), Edit details and, in the footer, Delete trip after a system
  dialog. Edit details opens the window of Stop (`ui/StopSheet.tsx` becomes `ui/DetailsSheet.tsx`,
  given its title, its first values and its action) with Save; the store gains `updateDetails`
  (an ended trip only, all four details at once, the rules of `details.ts`), and
  `native/actions.ts` `updateTripDetails` and `deleteTrip`, in the queue of the actions. A trip
  that is not there says so; an open one sends the screen home. Not in `mileage-app`'s screen: the
  purpose (stage 2), submit and the server's states; its Discard with a reason is a Delete here.
  Tried on the emulator, then changed: a trip without a name is called "Untitled trip 12" on its
  screen, by a number each trip gets once when it is made (migration 6: the column `number`, one
  more than the highest, unique; the trips of version 5 numbered in the order of their start), so
  it keeps its name when others are added or deleted; the default is not stored, so a name cleared
  and saved shows it again (`tripName`). The screen is three blocks: From and To ("Not given"),
  then Date to Positions, then Note ("Empty"), which grows with its text; the Name row is gone (the
  title says it); Edit details and Delete trip share the footer, side by side. The list of D5 keeps
  its titles (name, else From → To, else the times). Tried again, then changed: the untitled trips
  count among themselves ("Untitled trip 30" is followed by 31, whatever named trips lie between):
  migration 7 takes the number from the trips with a name and numbers the untitled ones again in
  the order of their start (migration 6 had run on the emulator); a trip named at Stop or later
  gives its number up, and a new untitled trip, or one whose name is cleared, gets one more than
  the highest number held (`writeDetails` in `sql-store.ts`), so the last untitled number comes
  back when its trip is named. The field Name of the window opens with the default of an untitled
  trip, and the default left as it is stays no name (`typedName`). The row Kind is gone (the list
  marks a trip added by hand); Positions stays.
- 2026-11-25: asked for while trying D6, and built in it: the window of D4b and its switch "Details
  at Stop" go (with the setting `stop_details` and its hook); Stop asks in a dialog of the system
  again, as in D4, and the details are typed on the
  trip's screen only (the window is `DetailsSheet` there; the store still takes details at Stop).
  History leaves the open trip out (no Current; it is not history yet), and the number on the row
  History of the home screen counts only the trips that ended. The open trip on the home screen
  shows, chosen from three layouts: its name ("Untitled trip 33") at the left and a red dot and
  Recording (grey and Paused) at the right, the big timer, then its km at the left and "Started
  22:23" at the right ("Started Yesterday 23:40" for a trip begun before today). The list keeps
  the times as the title of an untitled trip (chosen over "Untitled trip N"). The sentences of the
  window are `details.*`, and those of the switch, of the window at Stop and of Current in the
  list left the catalogue.
  Then: the open trip is the current widget (`ui/CurrentWidget.tsx`), one white card like the row
  History, with the name at the top left, the state at the top right, the timer, the km at the
  bottom left and the start time alone at the bottom right ("Started" went). A tap opens the
  trip's screen, which now shows the open trip too: Time "22:42–now", Duration as the timer, and
  Distance and Positions from the count, read again as positions are stored (`useProgress`); it
  has no Delete (Stop is on the home screen). The open trip can be named there: `updateDetails`
  takes any trip, so the name, with its number given up, goes on through Stop.
  Tried, then changed: the note "Waiting for the first position" goes (the warning after a minute
  without a position stays); the current widget is always there: with no trip open it shows the
  last trip that ended, with a green dot and Ended, its time without the pauses (standing) and when
  it started, and opens its screen; before the first trip it says "No trips yet.".
  Tried, then changed: the home screen stays mounted under the other screens (hidden while one
  is open, `App.tsx`), so a Start through the route of the permissions, or a Back from History,
  finds it as it was, and the widget changes its values in place instead of flashing empty. The
  row History gives way to Recent trips (`ui/RecentTrips.tsx`): a grey header with "Open history"
  in the colour of Back at its right, which opens History, and the newest trips that ended as in
  History with the day before the times (`recentLine`), as many as fit above the footer without
  scrolling, measured on the screen and again after every render (at least one).
  Tried, then changed: Start no longer passes the screen of the permissions when nothing stands in
  the way (it showed "How Rutsa uses your location" with Loading for a moment): `startWhenReady`
  (`native/start.ts`) reads the explanation and the access, asks the notifications as the screen
  does (`notificationsAtStart`, now shared with it) and starts the trip; the screen comes only
  when the explanation is unread, location is not allowed or the phone refuses to record. The round
  button is the app's own (`ui/RoundButton.tsx`, 144 instead of the shared 176 of `Button
size="big"`), so one more recent trip fits; the small button (Pause, Stop while paused) stands
  at the top right of the footer.
  Tried, then changed: the details are changed in place on the trip's screen (`ui/InlineText.tsx`, a
  text field with no frame that looks like the text around it and keeps what was typed when it
  loses the focus or the screen goes): the name is the title (`titleSlot`, a new optional property
  of `Screen` in `@obop/ui-native`, which draws it in the place of the title text), with the
  default of an untitled trip as its text; From and To at the right of their rows ("Not given");
  the note under its label ("Empty"). Edit details and its window (`DetailsSheet`) are gone; Back
  moves from the top into the footer, at the left of Delete trip.
  The block Note reaches down to the footer (its least height measured on the screen, as the
  recent trips are, and again when the keyboard comes or goes), and its field fills it.
  History has its Back in the footer too, across it, as the screen of the open trip.
  `Screen` of `@obop/ui-native` keeps the space under the title in its header, which does not
  scroll, instead of at the top of the content: at rest every screen looks the same, and content
  scrolled up goes out of sight that far below the title (asked for on History).
  A placeholder for stage 2 at the right of the title of the home screen: "User" and a round
  avatar with its first letter (`ui/UserBadge.tsx`); it does nothing yet (no sign-in or profile in
  stage 1).
- 2026-11-25: after D6, D7 and D8 were weighed against what is left of stage 1. D7 stays and is the
  last step of the screens: the goal of the stage, BR-28 and US-18 AC1 need a trip added by hand
  (`mileage-app` has none to follow). Its entry is rewritten for the screens as they are now: a
  filled button "Add trip" at the right of "Open history" in the header of Recent trips, and a
  screen of its own (the route `add`) with Back and Save, instead of a grey link under Current
  and a window; the trip may have a name. D8 (the edit of the parts of a trip) moves to "Proposed
  for stage 2" with what was decided: no BR or US asks for it, it is the heaviest step of block D
  (a table, a migration, the kind mixed, an edit screen), D6 already edits the details in place
  and D7 adds a forgotten trip whole, and it belongs with the report of stage 2, which separates
  the km measured and typed. The checkpoint of block D no longer has "a trip edited by hand".
- 2026-11-25: D7 as built: `ui/screens/AddTrip.tsx` (route `add`, title "Manual add", Back and
  Save in the footer) with the fields of the plan; Date is typed (`2026-11-24`) or set by Today
  and Yesterday; times are typed as `08:30`, `8.30` or `0830` (the number pad of Android has no
  colon). The checks are pure in `core/manual.ts` (`checkManual`): a real date and times, nothing
  in the future, an end before the start on the next day (said under End), an end equal to the
  start refused, km over 0 and at most 2,000 (a dot or a comma), From and To required, the
  lengths of `details.ts`, no overlap with another trip (the open one until now; trips that only
  touch do not overlap), and the average speed, which over 150 km/h asks "Is this right?" with
  Save anyway. Problems show under their fields once Save was pressed. `present.ts` gains
  `momentOf` (a clock time on a day in a time zone, in UTC, right in summer and winter time) and
  `dayAfter`. The store's `addManualTrip` takes a name and the rules of `details.ts`, and a
  named trip gets no number. "Add trip" is a small filled button at the right of "Open history";
  saved, the screen goes back and the trip is in Recent trips and History, marked "Added by
  hand". The temporary `NotYet` of `App.tsx` is gone: every route has its screen.
  Tried, then changed: the form has the look of a trip's screen (`ui/DetailRow.tsx`, shared with
  From and To there): Name, From and To typed in place (`InlineText`, which now also reports every
  change, so Save sees a field that still has the cursor), then Date, Start and End chosen with
  the pickers of the system and Distance typed in place, then Note down to the footer
  (`ui/fill.ts`, `useFillToFooter`, shared with the trip's screen). The pickers are
  `@react-native-community/datetimepicker` 9.2.1, the version Expo SDK 58 expects (MIT; a native
  module, so the build needs `android:prebuild`), against the "no date picker package" of the plan,
  asked for after trying the typed fields: the dialog of Android on a tap on the row, the compact
  picker of iOS in the row (`ui/pick.tsx`); no Today and Yesterday buttons. "Add trip" reaches
  over the line of the header of Recent trips with a negative margin, so the header keeps its
  height and the rows their room.
