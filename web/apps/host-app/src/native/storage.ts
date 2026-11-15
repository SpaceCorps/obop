import * as SecureStore from "expo-secure-store";
import { identity } from "../../config/identity.ts";

const ITEM = identity.storage.server;

/**
 * The one door to expo-secure-store. The app keeps one item there: which server it shows
 * (`src/core/server-record.ts`). The address is not a secret; the store is used because it is the
 * convention of the obop apps and adds no second storage module.
 *
 * The class of the item is "after first unlock, this device only": readable from the first unlock
 * after a start of the phone, and not copied to another phone by a backup. The option is for iOS;
 * on Android the library ignores it and keeps the Keystore.
 */
export const serverStore = {
  read: (): Promise<string | null> => SecureStore.getItemAsync(ITEM),
  write: (text: string): Promise<void> =>
    SecureStore.setItemAsync(ITEM, text, {
      keychainAccessible: SecureStore.AFTER_FIRST_UNLOCK_THIS_DEVICE_ONLY,
    }),
  clear: (): Promise<void> => SecureStore.deleteItemAsync(ITEM),
};
