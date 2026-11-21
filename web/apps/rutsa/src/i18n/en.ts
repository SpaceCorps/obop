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
  "common.cancel": "Cancel",
  "common.close": "Close",
  "common.delete": "Delete",
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

  "error.generic": "Something went wrong. Try again.",
  "error.alreadyActive": "A trip is already being recorded. Stop it before you start a new one.",
  "error.notFound": "This trip is no longer on the phone.",
  "error.wrongState": "This cannot be done with the trip as it is now.",
  "error.database": "The trips on this phone could not be opened.",
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
