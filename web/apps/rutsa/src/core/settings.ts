import type { SqlDb } from "./sql.ts";

/**
 * What the app remembers about the driver's choices, in the table `meta` of the encrypted file
 * (`schema.ts`, migration 3), so it goes with the trips and is gone when they are erased. A key is
 * one of `SETTING_KEYS`; a value is text.
 *
 * After the meta table of `mileage-app` (`src/core/host-state.ts`), without what serves its host.
 */

/** When the driver read the explanation of the location and went on (`ui/screens/Permissions.tsx`). */
export const DISCLOSURE_ACCEPTED = "disclosure_accepted_at";

/**
 * When the app asked for the notifications of Android 13: it asks once, and the driver's answer
 * stands (a second dialog after a refusal is not shown; the settings change it).
 */
export const NOTIFICATIONS_ASKED = "notifications_asked_at";

/**
 * Whether Stop opens the window with the details of the trip (D4b): `on` or `off`. A compromise
 * while it is open how a trip is best ended (PLAN, "Proposed for stage 2").
 */
export const STOP_DETAILS = "stop_details";

export const SETTING_KEYS = [DISCLOSURE_ACCEPTED, NOTIFICATIONS_ASKED, STOP_DETAILS] as const;

/** The switch "Details at Stop" from its stored value: on unless it was turned off. */
export function stopDetailsOn(value: string | null): boolean {
  return value !== "off";
}
export type SettingKey = (typeof SETTING_KEYS)[number];

export interface Settings {
  /** The value, `null` when it was never set. */
  get(key: SettingKey): Promise<string | null>;
  set(key: SettingKey, value: string): Promise<void>;
}

export function createSettings(db: SqlDb): Settings {
  return {
    async get(key) {
      const row = await db.one<{ value: string }>("SELECT value FROM meta WHERE key = ?", [key]);
      return row?.value ?? null;
    },
    async set(key, value) {
      await db.run(
        "INSERT INTO meta (key, value) VALUES (?, ?) ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        [key, value],
      );
    },
  };
}
