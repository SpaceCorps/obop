import { Platform } from "react-native";
import { tOn, type MessageKey } from "../i18n/index.ts";

/**
 * A sentence of the catalogue for the system the app runs on (`tOn` chooses the `.ios` version
 * where there is one). Used for the sentences that name something one system has and the other
 * has not: a notification, the quick settings, the app switcher.
 *
 * From `mileage-app` (`src/ui/os-text.ts`).
 */
export function tOs(key: MessageKey, params: Record<string, string | number> = {}): string {
  return tOn(Platform.OS, key, params);
}
