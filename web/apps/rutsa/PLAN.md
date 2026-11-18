# Rutsa, stage 1: the local prototype (working plan)

Working file of the branch `feat/rutsa-prototype`. It is updated with every commit and **deleted
before the pull request is marked ready for review**. The product is described in
[docs/spec/rutsa.md](../../../docs/spec/rutsa.md); nothing here is a decision.

## Goal

Rutsa runs on a phone without a server or a sign-in. The driver presses Start, drives, presses
Stop, and sees the trip in the list with the right kilometres. A trip can also be added by hand.
Everything is stored on the phone in an encrypted database.

Covers: BR-2 (manual Start and Stop), BR-14 (recording visible, pause), BR-28 (manual trip),
BR-23 (minimal look for the prototype), US-2. BR-16 in part: one code base for both systems,
checked on the Android emulator first; iOS needs a Mac.

Not in this stage: classification, rates, the report and mail, Swedish (stage 2); places,
learning, odometer, calendar (stage 3); automatic start and stop (OR-34 c), Live Activity,
widget, the obop design (stage 4); deleting old points (the 7 days of rutsa.md 5.4 are a rule
of the server version; Rutsa needs the points for the map in the report, decided in stage 2);
moving trips to a new phone (stage 2, with the export). Known limit until then: the database key
stays on the device, so a new phone or a restore starts with no trips.

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
- [ ] B7 `feat(rutsa): totals and formatting`: `core/present.ts` (km, duration, trips by day,
      today and this month; times are stored in UTC, days follow the phone's time zone) and tests
- [ ] B8 `feat(rutsa): English texts`: `i18n/index.ts`, `i18n/en.ts` (the notification of C4 and
      the screens of D use them)

Checkpoint: `vp test` passes.

### C. The phone

Before C: JDK 17, the Android SDK and the emulator installed (list in `docs/spec/mobile.md`,
"Android build on Windows").

- [ ] C1 `feat(rutsa): encrypted database on the phone`: `native/keychain.ts`,
      `native/database.ts` (32 random bytes in the Keystore or Keychain, SQLCipher),
      `native/store.ts` (the one store of the process; `PRAGMA foreign_keys = ON` and `migrate` at
      open); the database file name and the key name in
      `config/identity.ts`, pinned by a test (renaming either loses the trips of an installed
      app); plugins of `expo-sqlite` (SQLCipher) and
      `expo-secure-store` (no Face ID text). After `mileage-app` without the session and the
      server
- [ ] C2 `feat(rutsa): location permissions`: `core/liveness.ts` (readiness: permission, precise
      location, location services) and its tests, `native/permissions.ts`; the text of the iOS
      location dialog in `config/native-texts.ts`; plugin of `expo-location` (foreground service,
      background mode `location`, "while using" only)
- [ ] C3 `feat(rutsa): background location task`: `native/config.ts` (task name), `native/bus.ts`
      (change signal), `native/task.ts` imported in `index.ts` before the app is registered;
      `RECEIVE_BOOT_COMPLETED` on Android; the unused `fetch` mode removed on iOS
      (`config/ios.ts`)
- [ ] C4 `feat(rutsa): start, pause, resume and stop`: `native/recorder.ts`, `native/actions.ts`; the
      Android notification "recording" while a trip runs
- [ ] C5 `feat(rutsa): recover a recording after a restart`: recording health in
      `core/liveness.ts` (no position for a while) and tests; when the app opens with a trip that
      is recording, it starts the location updates again (the screen of D4 shows the health)

### D. Screens

Components from `@obop/ui-native`.

- [ ] D1 `feat(rutsa): navigation and data hooks`: `ui/router.ts`, `ui/hooks.ts`
- [ ] D2 `feat(rutsa): permissions screen`: why location is needed, what is stored and where (on
      the phone only)
- [ ] D3 `feat(rutsa): home screen`: Start; km today and this month
- [ ] D4 `feat(rutsa): recording screen`: elapsed time, km so far, Pause, Resume, Stop; a note
      when no position has come for a while (C5)
- [ ] D5 `feat(rutsa): trips list`: trips by day
- [ ] D6 `feat(rutsa): trip detail`: times, km, duration, delete
- [ ] D7 `feat(rutsa): add a trip by hand`: date, time, from, to, km; marked as manual. Date and
      time are typed and checked (no date picker package in this stage)

Checkpoint: the app runs on the emulator; Start, Pause, Resume, Stop, the list and a trip added by
hand work when tried by hand.

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
- [ ] F3 delete this file, then "Ready for review"

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
