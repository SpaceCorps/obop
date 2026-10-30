import { Platform } from "react-native";
import { tOn, type MessageKey } from "../i18n/index.ts";

/**
 * A sentence of the catalogue for the system the app runs on (`tOn` in `i18n/index.ts` chooses the
 * `.ios` version where there is one). Used for the sentences that name something that one system
 * has and the other has not: a notification, the quick settings, Google Play services, a force-stop.
 */
export function tOs(key: MessageKey, params: Record<string, string | number> = {}): string {
  return tOn(Platform.OS, key, params);
}
