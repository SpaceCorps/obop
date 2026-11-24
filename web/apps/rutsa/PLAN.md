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
- [ ] D4 `feat(rutsa): recording screen`: elapsed time, km so far, Pause, Resume, Stop; a note
      when no position has come for a while or the updates are not registered (C5), with a button
      that starts the recording again and a message when that fails; the reason when the check of
      C5 finds that the phone cannot record, and a Resume that cannot record, lead to the
      permissions (`next: "resume"`); the notifications that are off are said by the status
      banner of D3
- [ ] D5 `feat(rutsa): trips list`: trips by day
- [ ] D6 `feat(rutsa): trip detail`: times, km, duration, delete
- [ ] D7 `feat(rutsa): add a trip by hand`: date, time, from, to, km; marked as manual. Date and
      time are typed and checked (no date picker package in this stage)

Checkpoint: the app runs on the emulator; every check of `CHECKS.md` is run; Start, Pause, Resume,
Stop, the list and a trip added by hand work when tried by hand; the clock times and km on the screen are right (`Intl` of Hermes:
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
      without "Calls to obop" for Rutsa; what was run and what was not
- [ ] F2 `docs: add the Rutsa app to the README`
- [ ] F3 delete this file and `CHECKS.md`, then "Ready for review"

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
