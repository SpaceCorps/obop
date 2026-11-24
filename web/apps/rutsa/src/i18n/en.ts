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
  "error.alreadyActive": "A trip is already being recorded. Stop it before you start a new one.",
  "error.notFound": "This trip is no longer on the phone.",
  "error.wrongState": "This cannot be done with the trip as it is now.",
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
