import * as SecureStore from "expo-secure-store";

/**
 * The one door to expo-secure-store: the Android Keystore or the iOS Keychain. Rutsa keeps one item
 * there, the key of the database (`core/open-db.ts`).
 *
 * The iOS class is `AFTER_FIRST_UNLOCK`: readable from the first unlock after the phone starts, so
 * a process started while the phone is locked (the location task) can still open the database. It
 * is not "this device only", unlike `mileage-app` (`src/native/keychain.ts`): there the trips are on
 * the server too, here the phone holds the only copy, and with this class the key goes with the
 * database into an encrypted iCloud backup or a transfer to a new iPhone, so the trips move with
 * them. On Android the option does nothing: Keystore keys never leave the phone, and the app's
 * backup is off (`app.config.ts`).
 */
export const keychain = {
  get: (name: string): Promise<string | null> => SecureStore.getItemAsync(name),
  set: (name: string, value: string): Promise<void> =>
    SecureStore.setItemAsync(name, value, {
      keychainAccessible: SecureStore.AFTER_FIRST_UNLOCK,
    }),
};
