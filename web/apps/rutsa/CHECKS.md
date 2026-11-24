# Rutsa: manual checks on the emulator (temporary)

The checks that only a phone can show, kept here while stage 1 is built. Each step adds its own
checks and marks the few that are its smoke test (`smoke`), run right after the step is built.
The whole list is run once at the checkpoint after block D. A check that was run gets `[x]` and
the date; one that failed gets a note and a fix commit. This file is deleted with `PLAN.md` at F3.

## Setup

Emulator, in its own PowerShell window:

```powershell
emulator -avd rutsa -timezone Europe/Stockholm
```

Build and install, in a second window:

```powershell
adb devices
cd C:\Users\nikic\Desktop\obop\web\apps\rutsa
vp run android:build:release
adb install -r android\app\build\outputs\apk\release\app-release.apk
```

Useful commands:

| What                                                  | Command                                                                                                                      |
| ----------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| Feed a drive (10 fixes, about 0.5 km)                 | `for ($i = 0; $i -lt 10; $i++) { $lat = 59.30 + $i * 0.0005; adb emu geo fix 18.0 ([string]$lat); Start-Sleep 5 }`           |
| End the app's process                                 | `adb shell am force-stop org.obop.rutsa`                                                                                     |
| First start again (erases the trips and the settings) | `adb shell pm clear org.obop.rutsa`                                                                                          |
| Location off / on for the whole phone                 | `adb shell cmd location set-location-enabled false` / `true`                                                                 |
| Take the notifications away                           | `adb shell pm revoke org.obop.rutsa android.permission.POST_NOTIFICATIONS`                                                   |
| Back button                                           | `adb shell input keyevent 4`                                                                                                 |
| Home button / app switcher                            | `adb shell input keyevent 3` / `187`                                                                                         |
| Screenshot into a folder                              | `adb shell screencap -p /sdcard/shot.png` then `adb pull /sdcard/shot.png C:\Users\nikic\Desktop\rutsa-screenshots\NAME.png` |

`pm revoke` of the location keeps Expo's own note that it asked already, so the app reads it as
refused for good; `pm clear` resets both.

## C. Recording (regression, run again at the checkpoint)

- [x] 2026-11-21 Start, fixes, Stop: a 1.6 km route gives 1.6 km
- [x] 2026-11-21 0.5 km, Pause, a 4 km move, Resume, 0.5 km: 1.0 km
- [x] 2026-11-21 Recording goes on in the background and with the screen off
- [x] 2026-11-21 Force stop while recording, open the app: the recording starts again
- [x] 2026-11-21 Reboot while recording (`adb reboot`), open the app: the recording starts again
- [x] 2026-11-21 Force stop while paused, open the app: still paused, no notification
- [x] 2026-11-21 Location off while recording, back to the app: the reason; on again: recording
- [x] 2026-11-21 No new position for 3 min: `quiet 3 min`
- [x] 2026-11-21 Swipe the app away: the service survives on this Android

## D1. Navigation and the failure screen

- [x] 2026-11-22 `smoke` The app opens with Loading, then the home route
- [x] 2026-11-22 The check of a recording runs from the level of the app (force stop, open: restarted)
- [x] 2026-11-22 Back on the first screen leaves the app
- [x] 2026-11-22 Build with `EXPO_PUBLIC_RUTSA_TEST_DB_FAILURE=wrong_key`: the failure screen; Details, Try again, Stop recording
- [x] 2026-11-22 Erase trips: Cancel and Back on the first confirmation, Cancel on the second, Erase all trips gives an empty, working database
- [x] 2026-11-22 `newer` and `no_cipher`: their own Details, no Erase trips
- [x] 2026-11-22 A normal build again opens the trips

Changing the flag needs a fresh bundle: delete `android\app\build\generated\assets\react` and
`android\app\build\generated\res\react` (and `$env:TEMP\metro-*`) before the build.

## D2. Permissions

- [x] 2026-11-24 `smoke` First Start: explanation, Continue, While using, the note to keep the app running, Start the trip: recording
- [x] 2026-11-24 `smoke` The next Start records at once, with no screens
- [x] 2026-11-24 Not now on the explanation goes back
- [x] 2026-11-24 Refused once: the screen with Continue; Continue shows the dialog again, then the note (no start by itself)
- [x] 2026-11-24 Refused for good: Open settings, allow, back: the trip starts by itself and keeps recording
- [x] 2026-11-24 Approximate: its screen; Precise in the settings, back: the trip starts by itself
- [x] 2026-11-24 Location off on the phone: Open settings opens the location settings of Android; on, back: the trip starts by itself
- [x] 2026-11-24 Notifications refused: the note on the guide; Start the trip asks no second time
- [x] 2026-11-24 Back from the settings starts the trip with no dialog of the notifications
- [x] 2026-11-24 Close and the back button go back to where the driver was

## D3. Home and the status banner

- [ ] `smoke` The app opens on Home: Today, This month, Start, "No trips yet." or the last trip
- [ ] `smoke` Start, feed a drive, Stop on the recording route, Back: Today and This month grew by the trip, the last trip line shows its km and "Today HH:MM"
- [ ] `smoke` Location off on the phone: one low line at the top, under the clock: "Location is off.", Open settings, × at the right in the middle; nothing hidden under it
- [ ] `smoke` The banner's close button folds it up and the screen moves up
- [ ] With a trip open, Home shows "A trip is being recorded" (or "paused") with its start; Open trip goes to the recording route; Start is greyed out
- [ ] Force stop while recording, open the app: it opens on the recording route; Back goes home
- [ ] Start from the permissions (first start, `pm clear`) ends on the recording route; Back goes home
- [ ] Banner, location off on the phone: Open settings opens the location settings; on, back: the banner is gone
- [ ] Banner, no permission (`pm clear`, then Start, refuse twice, Close): "Location access is off."; Open settings opens the app's page
- [ ] Banner, approximate only: "Precise location is off."
- [ ] Banner, notifications off (`pm revoke ... POST_NOTIFICATIONS`): "Notifications are off."; Open settings opens the app's page; allowed, back: gone
- [ ] Banner, location off and notifications off: "Location and notifications are off."; Open settings opens the location settings
- [ ] Closed banner: home and back to the app (`keyevent 3`, then the icon): it stays closed; force stop and open: it is back
- [ ] Closed for the notifications, then location off: the banner is back: "Location and notifications are off."
- [ ] Location switched off in the quick settings without leaving the app: the banner comes within about 15 s
- [ ] No banner on the screen of the permissions, which says the same in full
- [ ] Trips over two days: "Yesterday HH:MM" for a trip of yesterday (change the date of the emulator, or wait a day)
- [ ] After midnight Today starts at 0.0 km, This month keeps the month (or at the start of a month, This month starts at 0.0 km)
- [ ] The note about a replaced backup file cannot be made on Android (the app's backup is off); its open is tested by `open-db.test.ts`
