/**
 * Every sentence the app shows, in one catalogue: this is the English source. Swedish (stage 2) is
 * another object with the same keys. Placeholders are `{name}`. A sentence that depends on a count
 * has one key per plural form (`name.one`, `name.other`) and is read with `tp(name, count)`.
 * Numbers and times are formatted in `core/present.ts`, not written into a sentence.
 *
 * This holds the sentences the app needs so far; each screen adds its own in the commit that adds
 * the screen, so no sentence here is unused.
 *
 * After `mileage-app` (`src/i18n/en.ts`).
 */
export const en = {
  "app.title": "Rutsa",

  "common.cancel": "Cancel",
  "common.close": "Close",
  "common.delete": "Delete",
  "common.details": "Details",
  "common.loading": "Loading",
  "common.retry": "Try again",
  "common.save": "Save",

  "day.today": "Today",
  "day.yesterday": "Yesterday",

  "duration.minutes": "{minutes} min",
  "duration.hoursMinutes": "{hours} h {minutes} min",

  "trips.count.one": "{count} trip",
  "trips.count.other": "{count} trips",
  "trips.title": "History",
  "trips.empty": "No trips yet.",
  "trips.manual": "Added by hand",

  "home.start": "Start",
  "home.start.hint": "Starts recording a trip",
  "home.today": "Today",
  "home.month": "This month",
  "home.none": "No trips yet.",
  "home.recording": "Recording",
  "home.paused": "Paused",
  "home.ended": "Ended",
  "home.current.hint": "Opens the trip",
  "home.stop": "Stop",
  "home.stop.hint": "Ends the trip that is being recorded",
  "home.stop.title": "Stop the trip?",
  "home.stop.message": "The trip ends here and is saved on this phone.",
  "home.pause": "Pause",
  "home.resume": "Resume",
  "home.recent": "Recent trips",
  "home.history": "Open history",
  "home.user": "User",

  "details.name": "Name",
  "details.from": "From",
  "details.to": "To",
  "details.note": "Note",

  "trip.title": "Trip",
  "trip.untitled": "Untitled trip {number}",
  "trip.note.empty": "Empty",
  "trip.date": "Date",
  "trip.time": "Time",
  "trip.time.open": "{start}–now",
  "trip.duration": "Duration",
  "trip.distance": "Distance",
  "trip.positions": "Positions",
  "trip.positions.count.one": "{count} position",
  "trip.positions.count.other": "{count} positions",
  "trip.positions.dropped": "{positions}, {dropped} left out",
  "trip.none": "Not given",
  "trip.details.failed": "The details could not be saved. Try again.",
  "trip.delete": "Delete trip",
  "trip.delete.title": "Delete this trip?",
  "trip.delete.message": "It is deleted from this phone for good.",
  "trip.delete.failed": "The trip could not be deleted. Try again.",
  "trip.notfound": "This trip is not on this phone any more.",

  "recording.restart": "Restart recording",
  "recording.restart.failed": "Recording could not be restarted. Try again.",
  "recording.blocked.services_off": "Location is switched off: the trip is not recorded.",
  "recording.blocked.no_permission": "Location is not allowed any more: the trip is not recorded.",
  "recording.blocked.approximate":
    "Only an approximate location is allowed: the trip cannot be measured.",
  "recording.blocked.action": "Fix it",
  "recording.stuck": "Recording has stopped: the trip is open but the phone is not recording it.",
  "recording.nofix":
    "No position arrived in the first minute. Check that location is switched on, then restart recording.",
  "recording.nofix.ios":
    "No position arrived in the first minute. Check that Location Services are switched on, then restart recording.",
  "recording.quiet.one":
    "The last position is {count} minute old. Standing still, that is normal; driving, restart recording.",
  "recording.quiet.other":
    "The last position is {count} minutes old. Standing still, that is normal; driving, restart recording.",
  "recording.weak":
    "The location signal is too weak to measure the distance. Indoors, move to a window or go outside.",
  "recording.jumpy":
    "The positions do not fit together and are left out of the distance. If it stays at 0 km, stop this trip and start a new one.",
  "recording.signal.detail.one": "{count} of {total} positions was left out",
  "recording.signal.detail.other": "{count} of {total} positions were left out",

  "announce.recording.started": "Recording started",
  "announce.recording.stopped": "Recording stopped. The trip is saved on this phone.",
  "announce.recording.stuck": "Recording has stopped. Restart recording.",
  "home.restored.title": "The trips of a backup could not be opened",
  "home.restored.message":
    "The key of the trips stays on the phone they were made on, so the trips that came back with a backup could not be opened here. They were replaced by an empty list.",
  "home.restored.action": "Got it",

  "status.location_off": "Location is off.",
  "status.no_permission": "Location access is off.",
  "status.approximate": "Precise location is off.",
  "status.notifications_off": "Notifications are off.",
  "status.location_off.notifications": "Location and notifications are off.",
  "status.no_permission.notifications": "Location access and notifications are off.",
  "status.approximate.notifications": "Precise location and notifications are off.",
  "status.close": "Close this note",

  "recording.notification.title": "Recording a trip",
  "recording.notification.body":
    "Rutsa is recording the route of your trip. Open the app to pause or stop it.",
  "recording.error.start": "The trip could not be started. Try again.",

  "disclosure.title": "How Rutsa uses your location",
  "disclosure.body":
    "Rutsa records your location to measure how far you drove. It does this only between Start and Stop, and it keeps recording when the screen is off or the app is in the background, so that the whole trip is measured. A notification shows while it records. The route stays on this phone, encrypted, and is not sent anywhere.",
  "disclosure.body.ios":
    "Rutsa records your location to measure how far you drove. It does this only between Start and Stop, and it keeps recording when the screen is off or the app is in the background, so that the whole trip is measured. The system shows a location indicator while it records. The route stays on this phone, encrypted, and is not sent anywhere.",
  "disclosure.continue": "Continue",
  "disclosure.decline": "Not now",

  "permission.denied.title": "Location is needed to record a trip",
  "permission.denied.message":
    "Without your location Rutsa cannot measure the trip. Press Continue to be asked again.",
  "permission.blocked.title": "Location is turned off for Rutsa",
  "permission.blocked.message":
    "Open the settings of the app and allow location. When you come back, the trip starts.",
  "permission.approximate.title": "Precise location is needed",
  "permission.approximate.message":
    "You allowed only an approximate location, and a trip cannot be measured with it. Open the settings of the app and turn on Precise location. When you come back, the trip starts.",
  "permission.services.title": "Location is switched off on this phone",
  "permission.services.message":
    "Switch location on in the settings or the quick settings of the phone. When you come back, the trip starts.",
  "permission.services.message.ios":
    "Switch on Location Services in the Settings app, under Privacy & Security. When you come back, the trip starts.",
  "permission.settings": "Open settings",
  "permission.notifications.title": "Notifications are off",
  "permission.notifications.message":
    "Android will not show that a trip is recording. Recording still works. You can turn notifications on in the settings of the app.",

  "guide.title": "Keep the app running",
  "guide.body":
    "Some phones stop apps to save battery. Do not force-stop Rutsa while you drive. If a trip stops recording, set the battery use of Rutsa to Unrestricted in the Android settings, and open the app: it starts recording again.",
  "guide.body.ios":
    "Do not close Rutsa from the app switcher while you drive: iPhone stops recording when you do. If a trip stops recording, open the app: it starts recording again.",
  "guide.start": "Start the trip",

  "error.generic": "Something went wrong. Try again.",
  "error.database": "The trips on this phone could not be opened.",
  "error.database.hint":
    "Try again. If it keeps failing, you can stop a recording that is still running, so that the phone stops looking for your position.",
  "error.database.detail.no_cipher":
    "This build of the app cannot encrypt the trips, so it does not open them. Install the app again from where you got it.",
  "error.database.detail.wrong_key":
    "The key of the trips does not fit the file: the key was lost or the file is damaged.",
  "error.database.detail.newer":
    "The trips were saved by a newer version of Rutsa. Install the newer version again to open them.",
  "error.database.stop": "Stop recording",
  "error.database.stopped": "Recording is stopped.",
  "error.database.stop.failed": "Recording could not be stopped. Try again.",
  "error.database.erase": "Erase trips",
  "error.database.erase.title": "Erase all trips?",
  "error.database.erase.first":
    "The trips on this phone cannot be opened. Erasing them makes a new, empty list of trips, and the app works again. The trips are kept only on this phone, so they cannot be brought back.",
  "error.database.erase.continue": "Continue",
  "error.database.erase.second":
    "Last step: every trip on this phone will be gone for good. This cannot be undone.",
  "error.database.erase.yes": "Erase all trips",
  "error.database.erase.failed": "The trips could not be erased. Try again.",
} as const;

type EnglishKey = keyof typeof en;

/** The keys that have plural forms: `name` where the catalogue holds `name.one` and `name.other`. */
export type PluralKey = EnglishKey extends infer K
  ? K extends `${infer Base}.other`
    ? Base
    : never
  : never;

/** The keys read with `t`: every key that is not a plural form. */
export type MessageKey = Exclude<EnglishKey, `${PluralKey}.${"one" | "other"}`>;
