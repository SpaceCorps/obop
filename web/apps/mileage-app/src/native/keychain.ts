import * as SecureStore from "expo-secure-store";

/**
 * The one door to expo-secure-store. Every value the app keeps there (the key of the database, the
 * server address and the tokens) is written through `set`, with the same class of protection.
 *
 * On iOS the class of an item decides when it can be read. The default of the library is
 * `WHEN_UNLOCKED`, and a read while the phone is locked then fails. The app reads the key of the
 * database once, when its process starts, and the location task then stores fixes with the database
 * that is open: a recording that was started with the phone unlocked does not read the Keychain
 * again, so the test of a locked recording (docs/spec/mobile.md 9.2, T1) does not test this class.
 * A process that is started while the phone is locked must still be able to read the key, so every
 * item is `AFTER_FIRST_UNLOCK_THIS_DEVICE_ONLY`: readable from the first unlock after a start of
 * the phone, and not copied to another phone by a backup (lead decision, docs/spec/mobile.md 8.8).
 * The option is for iOS only; on Android the library ignores it and keeps the Keystore.
 */
export const keychain = {
  get: (name: string): Promise<string | null> => SecureStore.getItemAsync(name),
  set: (name: string, value: string): Promise<void> =>
    SecureStore.setItemAsync(name, value, {
      keychainAccessible: SecureStore.AFTER_FIRST_UNLOCK_THIS_DEVICE_ONLY,
    }),
  delete: (name: string): Promise<void> => SecureStore.deleteItemAsync(name),
};
