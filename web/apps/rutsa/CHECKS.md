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
- [ ] Trips over two days: a trip of yesterday in Recent trips says "Yesterday" (the samples of D5 give one)
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
- [x] 2026-11-25 (removed in D6: the note "Waiting for the first position" is gone; "No position arrived in the first minute" still warns)
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

Removed during D6 (2026-11-25): Stop asks in a dialog again and the details are typed on the trip's
screen; the window lives on there (`DetailsSheet`), its checks are in D6. The checks run before:

- [x] 2026-11-24 `smoke` Stop opens the window "Stop the trip?" with the time and km of the trip, Name, From, To and Note, Cancel and Stop trip
- [x] 2026-11-24 `smoke` Type a name, Stop trip: the trip ends, the last trip line starts with the name ("Last trip: Meeting, 0.5 km, 3 min, Today 17:17")
- [x] 2026-11-24 `smoke` Cancel: the window closes and the trip goes on recording
- [x] 2026-11-24 `smoke` The switch "Details at Stop" off: Stop ends the trip at once, with no window; force stop and open the app: the switch is still off
- [x] 2026-11-24 `smoke` The app installed over the last build (migration 5) keeps every trip and its km

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

Since D6 the open trip is not in History at all (no Current there); the home screen shows it.

- [ ] Back of Android on the list and the Back button go to Home
- [ ] `pm clear`, open the app: the widget says "No trips yet.", Recent trips has no rows; History says "No trips yet."
- [ ] A trip with From and To but no name: "From → To" as the title (the samples "Home → Office", "Home → Airport")
- [ ] A day of another year has the year (tested in `trips-list.test.ts`; on the emulator only with its date set back)
- [ ] The sample over midnight is under the day of its start (five days ago): "23:40–00:20"
- [ ] Location off on the phone (`set-location-enabled false`): the list opens and shows the trips, with the banner at the top
- [ ] The two samples added by hand show "Added by hand": "Uppsala → Stockholm" with its times and the mark under it; the other with its times as the title and only the mark under it
- [ ] TalkBack: the header of a day is read as a heading, a trip as its title, times and km

## D6. The screen of a trip

- [ ] `smoke` The app installed over the last build (migration 7) keeps every trip; only the trips without a name have a number, counted again in the order of their start without gaps for the named ones ("Untitled trip 30" is followed by 31, whatever named trips lie between)
- [ ] `smoke` History, tap "Site visit": the title is its name; under it From and To ("Not given" where empty); then Date "Sat, Nov 21", Time, Duration, Distance (the km of the list), Positions ("331 positions"), no Kind; then Note ("Empty" when there is none); Edit details at the left and Delete trip at the right of the footer
- [ ] `smoke` An untitled trip, Edit details: Name holds "Untitled trip N"; Save without changing it: the trip stays "Untitled trip N" with the same number
- [ ] `smoke` Untitled trips N-1 and N; name trip N ("Customer"), Save: its title is the name; Start and Stop a new trip with nothing typed: it is "Untitled trip N" (the number was given up)
- [ ] `smoke` A trip named at Stop has no number; the next untitled trip counts on from the last untitled one
- [ ] `smoke` A trip added by hand ("Uppsala → Stockholm"): Duration from its start to its end, no Positions; its note "Phone was off"
- [ ] `smoke` Delete trip: "Delete this trip?" with Cancel and Delete; Cancel keeps it; Delete goes back to History without the trip, and the km of its day and of Today or This month on Home go down; the other trips keep their numbers
- [ ] `smoke` A trip recorded with a pause (Start, a drive, Pause, wait 2 min, Resume, a drive, Stop): Duration leaves out the pause
- [ ] A named trip, Edit details, clear the name, Save: "Untitled trip M", M one more than the highest number held
- [ ] The sample over midnight: Time "23:40–00:20 (…)" with the day of its end
- [ ] A long note (several lines, up to 1,000 characters): the Note block grows with it and the screen scrolls; the footer stays
- [ ] Edit details: clear every field, Save: "Not given" for From and To, "Empty" for Note, the title "Untitled trip M"
- [ ] Edit details: Cancel after typing, then Edit details again: the stored values (the default in Name for an untitled trip), not what was typed
- [ ] Edit details: the back button of Android is Cancel; a name of 100 characters is taken; the keyboard does not hide the Note
- [ ] The window of Stop (D4b) still opens empty, with "Stop the trip?", the time and km, and "Stop trip" in red
- [ ] Back of Android and the Back button go to History
- [ ] Location off on the phone: the screen opens, Edit details and Delete trip work
- [ ] TalkBack: each row is read with its title and value; Delete trip and its dialog are read

Since the second round of D6: no switch "Details at Stop" and no window at Stop; the open trip on Home is the current widget.

- [ ] `smoke` Home with no trip open: no switch in the footer, only Start in the middle
- [ ] `smoke` Start: the current widget, a white card: its name ("Untitled trip N", the next number) at the top left, a red dot and "Recording" at the top right, the big timer, its km at the bottom left and the start time ("22:42", no word) at the bottom right; no "Waiting for the first position" under it; Today and This month under it, then History
- [ ] `smoke` Pause: a grey dot and "Paused", the timer stands; Resume: red and "Recording" again
- [ ] `smoke` Tap the widget: the trip's screen; Time "22:42–now", Duration runs as the timer, Distance and Positions grow while a drive is fed; no Delete trip, Edit details across the footer
- [ ] `smoke` On the open trip's screen, Edit details: the default name in Name; type a name, Save: the title shows it, and back on Home the widget too; the trip goes on recording; Stop: the trip keeps the name in History
- [ ] `smoke` Stop: the dialog "Stop the trip?" "The trip ends here and is saved on this phone." with Cancel and Stop; Cancel keeps recording; Stop ends the trip; the small Stop of a paused trip asks the same
- [ ] `smoke` History while a trip is open: no Current, only the days of the trips that ended
- [ ] On the open trip's screen while it is paused: the Duration stands; Resume on Home, back to it: it runs again
- [ ] A trip started before midnight and still open: the widget says "Yesterday 23:40" at the bottom right
- [ ] Name the open trip, then clear the name: it gets the next free number
- [ ] TalkBack: the widget is read as a button with its name, state, time, km and start
- [ ] `smoke` No trip open: the widget stays and shows the last trip that ended: its name, a green dot and "Ended", its time without the pauses (not running), its km and when it started ("22:51", or "Yesterday 23:40", "Nov 18 08:30"); a tap opens its screen with Delete trip
- [ ] `smoke` Stop: the widget turns from red "Recording" to green "Ended" with the same trip; Start: it shows the new trip
- [ ] `pm clear`, open the app: the widget says "No trips yet." and does nothing on a tap
- [ ] Delete the last trip from its screen: the widget shows the trip before it

Since the third round of D6: the home screen stays mounted under the other screens; Recent trips replaces the row History.

- [ ] `smoke` No trip open, Start (location already allowed): the widget changes its values in place, from green "Ended" to red "Recording", with no flash of "No trips yet." or of an empty card
- [ ] `smoke` Open History, Back: Home is as it was, with no flash
- [ ] `smoke` Under Today and This month: a grey header "Recent trips" with "Open history" at the right in the colour of Back; under it the newest trips that ended, as in History ("Meeting" over "Today 08:30–09:15", an untitled one "08:30–09:15" over "Yesterday", a date before that), as many as fit above the footer without scrolling; "Open history" opens History; a tap on a row opens the trip
- [ ] `smoke` Start: the open trip is not in Recent trips; Stop: it comes at the top of Recent trips
- [ ] A warning on Home (location off while recording, `set-location-enabled false`) or the status banner at the top: Recent trips shows fewer rows, and the screen does not scroll; when it goes, the rows come back
- [ ] With large text (Settings, Display, Font size at the largest): the rows still fit above the footer (at least one is shown)

Since the fourth round of D6: Start without the screen of the permissions when nothing stands in the way; a smaller round button; the small button at the right.

- [ ] `smoke` Start with location allowed: no "How Rutsa uses your location / Loading" for a moment; the widget turns red at once, Start shows that it works while the trip starts
- [ ] `smoke` The round button is smaller (Start, Stop, Resume); Pause, and Stop while paused, stand at the top right of the footer, mirrored; nothing moves when the small button comes, goes or changes
- [ ] `smoke` Recent trips shows one row more than before (6 on the emulator), with at least the space between the title Rutsa and the widget between the last row and the footer
- [ ] `pm clear`, Start: the explanation, as before (the screen of the permissions still comes when something stands in the way)
- [ ] Location off on the phone, Start: the screen "Location is switched off on this phone", as before
- [ ] Notifications not asked yet (`pm clear`, then through the explanation once; or a fresh install): the dialog of the notifications comes once, at a Start
