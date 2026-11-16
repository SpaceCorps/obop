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
of the server version; Rutsa needs the points for the map in the report, decided in stage 2).

## How it is tested

| Where | GPS | Who |
|---|---|---|
| Android emulator on Windows | A script feeds a route to the emulator (`adb emu geo fix`); no phone and no drive needed | Developer |
| iOS Simulator on a Mac | `xcrun simctl location` feeds the same route (as `mileage-app` `simulator:route`) | Owner (Mac) |
| A real iPhone | A real drive; built with Xcode on a Mac | Owner, later the developer |

`vp check` and `vp test` run on any system; blocks A and B need no emulator.

## Commits

Status: `[ ]` to do, `[x]` done (with the commit), `[~]` in progress.

### A. Skeleton

- [ ] A1 `feat(rutsa): app package on Expo SDK 58`: `package.json` (`@obop/rutsa`, Apache-2.0,
      the exact versions of `mileage-app`, without `@obop/client` and `expo-web-browser`),
      `tsconfig.json`, `vite.config.ts`, `.gitignore`, `index.ts`, `App.tsx` showing "Rutsa",
      `pnpm-lock.yaml`
- [ ] A2 `feat(rutsa): app config and placeholder identity`: `config/identity.ts` (placeholder id
      `org.obop.rutsa`, marked like `org.obop.mileage`), `app.config.ts` (minSdk 26, no cloud
      backup, location permission, iOS background mode `location`, SQLCipher)
- [ ] A3 `feat(rutsa): native texts in English`: `config/native-texts.ts` (name under the icon,
      the system permission texts)
- [ ] A4 `feat(rutsa): build scripts`: `scripts/check-sdk.mjs`, `gradle.mjs`, `scripts/ios/*`
      after `mileage-app`; the `vp run` scripts
- [ ] A5 `ci: check and test apps/rutsa`: both loops of `.github/workflows/ci.yml`

Checkpoint: `vp install` and `vp check` pass; the draft pull request is opened.

### B. Logic without a phone (`vp test`)

- [ ] B1 `feat(rutsa): trip and point types`: `core/types.ts` (trip: id, start, end, state
      `recording` or `ended`, distance, source `recorded` or `manual`, note; point: time, lat,
      lon, accuracy, speed, segment)
- [ ] B2 `feat(rutsa): SQL layer`: `core/sql.ts` (from `mileage-app`), a test database on
      `node:sqlite`
- [ ] B3 `feat(rutsa): database schema`: `core/schema.ts` (`trip`, `point`, `meta` with the schema
      version) and tests
- [ ] B4 `feat(rutsa): fix conversion and thinning`: `core/fix.ts`, `core/thin.ts` (from
      `mileage-app`) and tests
- [ ] B5 `feat(rutsa): trip repository`: `core/repository.ts`, one interface (start, add points,
      pause, resume, stop, add by hand, list, get, delete), so a server can be added later
- [ ] B6 `feat(rutsa): local trip store`: `core/sql-store.ts`; one active trip at most; at Stop
      the distance is `analyseTrack` of `@obop/trip-core` per segment (a pause starts a new
      segment, so the gap is not counted); tests
- [ ] B7 `feat(rutsa): totals and formatting`: `core/present.ts` (km, duration, trips by day,
      today and this month) and tests

Checkpoint: `vp test` passes.

### C. The phone

- [ ] C1 `feat(rutsa): encrypted database on the phone`: `native/keychain.ts`,
      `native/database.ts` (32 random bytes in the Keystore or Keychain, SQLCipher), after
      `mileage-app` without the session and the server
- [ ] C2 `feat(rutsa): location permissions`: `native/permissions.ts` (location on, precise,
      "while using" only; the recording starts from a tap, as in `mileage-app`)
- [ ] C3 `feat(rutsa): background location task`: `native/task.ts`, registered in `index.ts`
      before the app starts
- [ ] C4 `feat(rutsa): start, pause and stop`: `native/recorder.ts`, `native/actions.ts`; the
      Android notification "recording" while a trip runs
- [ ] C5 `feat(rutsa): resume a recording after a restart`: the app continues or offers to end a
      trip that was recording when the system stopped it

Before C: JDK 17, the Android SDK and the emulator installed (list in `docs/spec/mobile.md`,
"Android build on Windows").

### D. Screens

- [ ] D1 `feat(rutsa): English texts`: `i18n/index.ts`, `i18n/en.ts`
- [ ] D2 `feat(rutsa): navigation and data hooks`: `ui/router.ts`, `ui/hooks.ts`
- [ ] D3 `feat(rutsa): permissions screen`: why location is needed, what is stored and where (on
      the phone only)
- [ ] D4 `feat(rutsa): home screen`: Start; km today and this month
- [ ] D5 `feat(rutsa): recording screen`: elapsed time, km so far, Pause, Stop
- [ ] D6 `feat(rutsa): trips list`: trips by day
- [ ] D7 `feat(rutsa): trip detail`: times, km, duration, delete
- [ ] D8 `feat(rutsa): add a trip by hand`: date, time, from, to, km; marked as manual

Components from `@obop/ui-native`.

### E. Check on the emulator

- [ ] E1 `feat(rutsa): emulator route scripts`: `scripts/emulator/route.mjs` after `mileage-app`
- [ ] E2 `test(rutsa): check a recorded trip on the emulator`: the km in the app against the
      route's reference distance

Checkpoint: the APK builds and installs, a route gives a trip with the expected km.

### F. Documentation

- [ ] F1 `docs(rutsa): the local prototype`: `docs/spec/rutsa.md` phase 1 becomes the local
      prototype (the calendar reminder moves to stage 3); 5.5 without "Calls to obop" for Rutsa;
      what was run and what was not
- [ ] F2 `docs: add the Rutsa app to the README`
- [ ] F3 delete this file, then "Ready for review"

## Taken from `mileage-app`, and left out

| Taken (copied and simplified) | Left out |
|---|---|
| `sql.ts`, `fix.ts`, `thin.ts` | `@obop/client`, `auth.ts`, `SignIn.tsx`: no sign-in |
| `recorder.ts`, `task.ts`, `permissions.ts` | `sync.ts`, `engine.ts`, `api.ts`: no server |
| `keychain.ts`, `database.ts` | `host-config.ts`, `host-state.ts`: the host app's |
| `check-sdk.mjs`, `gradle.mjs`, iOS and emulator scripts | `Foreign.tsx`: no other person's trips |
| The shape of `app.config.ts` | `schema.ts` as a whole (1,242 lines for the server) |

## Open

- The folder `web/apps/rutsa`: asked to the owner, no answer yet.
- The app id: placeholder `org.obop.rutsa` until the owner rules (as `org.obop.mileage`, Q37).

## Log

- 2026-11-16: plan written.
