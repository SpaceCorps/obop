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

Since D4 these read the warnings on the home screen instead of the lines of the temporary panel:
the reason is "Location is switched off: the trip is not recorded." with Fix it, and the quiet
one is "The last position is 3 minutes old ...".

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
- [ ] (since D4) Notifications refused, the note on the guide: Open settings, turn them on, back: the note is gone

## D3. Home and the status banner

- [x] 2026-11-24 `smoke` The app opens on Home: Today, This month, Start, "No trips yet." or the last trip
- [x] 2026-11-24 `smoke` Start, feed a drive, Stop, back home: Today and This month grew by the trip, the last trip line shows its km and "Today HH:MM"
- [x] 2026-11-24 `smoke` Location off on the phone: one low line at the top, under the clock: "Location is off.", Open settings, × at the right in the middle; nothing hidden under it
- [x] 2026-11-24 `smoke` The banner's close button folds it up and the screen moves up
- [ ] Banner, location off on the phone: Open settings opens the location settings; on, back: the banner is gone
- [ ] Banner, no permission (`pm clear`, then Start, refuse twice, Close): "Location access is off."; Open settings opens the app's page
- [ ] Banner, approximate only: "Precise location is off."
- [ ] Banner, notifications off (`pm revoke ... POST_NOTIFICATIONS`): "Notifications are off."; Open settings opens the app's page; allowed, back: gone
- [ ] Banner, location off and notifications off: "Location and notifications are off."; Open settings opens the location settings
- [ ] Closed banner: home and back to the app (`keyevent 3`, then the icon): it stays closed; force stop and open: it is back
- [ ] Closed for the notifications, then location off: the banner is back: "Location and notifications are off."
- [ ] Location switched off in the quick settings without leaving the app: the banner comes within about 15 s
- [ ] No banner on the screen of the permissions, which says the same in full
- [ ] Trips over two days: the row History on Home says "Last trip: X km, Yesterday HH:MM" for a trip of yesterday (the sample trips of D5 give one)
- [ ] After midnight Today starts at 0.0 km, This month keeps the month (or at the start of a month, This month starts at 0.0 km)
- [ ] The note about a replaced backup file cannot be made on Android (the app's backup is off); its open is tested by `open-db.test.ts`

The checks of D3 with a recording route went with D4, which records on the home screen.

## D4. The recording on the home screen

- [x] 2026-11-24 `smoke` Start: the big button turns red and says Stop, Pause at its left; Current shows the time running and 0.0 km
- [x] 2026-11-24 `smoke` Feed a drive: the km of Current grow (within about 4 s of each position); Today and This month do not
- [x] 2026-11-24 `smoke` Pause: the big button says Resume, Stop at its left, Current says "Current · Paused", the clock stands; Resume: Stop and Pause again, the clock goes on from where it stood
- [x] 2026-11-24 `smoke` The big button stands exactly in the middle (in line with the home bar of Android) and stays there when the small one comes, goes or changes; the small one is in the top left corner of the footer; Pause, Resume, Pause, Resume: nothing jumps, only the pressed button shows that it works
- [x] 2026-11-24 `smoke` After Stop the last trip line says its km, how long it took without the pauses, and when: "Last trip: 0.5 km, 3 min, Today 17:17"
- [x] 2026-11-24 `smoke` The app installed over the last build (migration 4) keeps every trip and its km
- [x] 2026-11-24 `smoke` Stop: "Stop the trip?"; Cancel keeps recording; Stop ends it: Current goes, Today and This month grow by its km (the same km Current showed), the last trip line shows it
- [ ] Stop from a pause (the small Stop): asks, then ends the trip
- [ ] 0.5 km, Pause, move 4 km, Resume, 0.5 km: Current shows about 1.0 km all along, and Stop stores the same
- [ ] Right after Start, before the first position: "Waiting for the first position."
- [ ] A minute with no position (location off before Start is refused, so: `pm clear`, start in a place the emulator gives no fix, or wait after switching location off in the quick settings without leaving the app): "No position arrived in the first minute" with Restart recording
- [ ] Location off in the quick settings while recording, 3 min: "The last position is 3 minutes old ..." with Restart recording; Restart recording with location off: "Location is switched off ..." with Fix it
- [ ] Fix it: the screen of the permissions; location on, back: the recording goes on by itself, the warning is gone, home
- [ ] Force stop while recording, open the app: Home shows the trip as Current, still recording (the check of C5), no warning after a few seconds
- [ ] Location off, then the app away and back (the check of C5 finds it): "Location is switched off ..." with Fix it; the banner at the top says nothing about location while the trip is open
- [ ] Notifications off while a trip is open: the banner at the top says "Notifications are off."
- [ ] Resume with location off: the screen of the permissions; location on, back: the trip goes on by itself
- [ ] Positions too imprecise for a minute (`adb emu geo fix` cannot give an accuracy; tested in `signal.test.ts` and `sql-store.test.ts`)
- [ ] TalkBack on: "Recording started" at Start, "Recording stopped ..." at Stop

## D4b. Details at Stop

- [x] 2026-11-24 `smoke` Stop opens the window "Stop the trip?" with the time and km of the trip, Name, From, To and Note, Cancel and Stop trip
- [x] 2026-11-24 `smoke` Type a name, Stop trip: the trip ends, the last trip line starts with the name ("Last trip: Meeting, 0.5 km, 3 min, Today 17:17")
- [x] 2026-11-24 `smoke` Cancel: the window closes and the trip goes on recording
- [x] 2026-11-24 `smoke` The switch "Details at Stop" off: Stop ends the trip at once, with no window; force stop and open the app: the switch is still off
- [x] 2026-11-24 `smoke` The app installed over the last build (migration 5) keeps every trip and its km
- [ ] Stop trip with nothing typed: the trip ends, and the list shows it by its times
- [ ] From and To with no name: the list shows it as "From → To"

Since D5 the name is shown in the list only; the row History on Home says "Last trip: 0.5 km, Today 17:17".

- [ ] The back button of Android in the window is Cancel
- [ ] The keyboard does not hide the field being typed in, Note included (several lines)
- [ ] A name of 100 characters is taken; the field takes no more (the limit is shown if one is pasted)
- [ ] The small Stop of a paused trip opens the same window, and ends the trip with the details
- [ ] The switch sits in the bottom left corner of the footer and moves neither button
- [ ] The window opens empty each time, also after a Cancel with something typed

## D5. The list of trips

The emulator's database holds ten sample trips on the five days before 2026-11-24, added once by a
temporary button of D5 (removed before its commit); `pm clear` erases them.

- [x] 2026-11-24 `smoke` Home has no separate last trip line; the row "History" under Today and This month has "Last trip: X km, Today HH:MM" under its title (km and when, never the name) and the number of trips at the right; it opens the list "History" with Back
- [x] 2026-11-24 `smoke` The trips of today under "Today · N trips · X km", newest first; the km of the day are the sum of its trips and the same as Today on Home
- [x] 2026-11-24 `smoke` A trip with a name shows the name and its times under it ("08:30–09:15"); one with nothing typed shows its times as the title; each has its km at the right
- [x] 2026-11-24 `smoke` Start, then the list: the open trip alone on top under the header "Current" (the same style as the days), with "Recording" and no km; Today does not count it (neither the trips nor the km); tap it: Home with Current. Pause: "Paused" in the list
- [x] 2026-11-24 `smoke` Stop, then the list: no Current; the trip is the newest of Today, which counts it now
- [x] 2026-11-24 `smoke` Tap an ended trip: the screen of the trip (until D6 the temporary "comes in a later step")
- [x] 2026-11-24 `smoke` The days before today follow Today, newest first, each under its date with its trips and km ("Mon, Nov 23 · 3 trips · 50.3 km", no "Yesterday")
- [ ] Back of Android on the list and the Back button go to Home
- [ ] `pm clear`, open the app: the row History says "No trips yet." with no number; the list says "No trips yet."
- [ ] A trip with From and To but no name: "From → To" as the title (the samples "Home → Office", "Home → Airport")
- [ ] A day of another year has the year (tested in `trips-list.test.ts`; on the emulator only with its date set back)
- [ ] The sample over midnight is under the day of its start (five days ago): "23:40–00:20"
- [ ] Location off on the phone (`set-location-enabled false`): the list opens and shows the trips, with the banner at the top
- [ ] The two samples added by hand show "Added by hand": "Uppsala → Stockholm" with its times and the mark under it; the other with its times as the title and only the mark under it
- [ ] TalkBack: the header of a day is read as a heading, a trip as its title, times and km
