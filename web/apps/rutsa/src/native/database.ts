import * as Crypto from "expo-crypto";
import * as SQLite from "expo-sqlite";
import { identity } from "../../config/identity.ts";
import {
  eraseDatabaseAndKey,
  openDatabase as openWith,
  testFailure,
  type FileHandle,
  type Opened,
  type OpenPorts,
} from "../core/open-db.ts";
import type { SqlValue } from "../core/sql.ts";
import { keychain } from "./keychain.ts";

const hex = (bytes: Uint8Array): string =>
  Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");

const { databaseFile, databaseKey } = identity.storage;

/**
 * The parts of the phone that `openDatabase` (`core/open-db.ts`, where the procedure and its tests
 * are) needs: expo-sqlite with SQLCipher (`app.config.ts`), the secure store, and 32 random bytes
 * from expo-crypto. The key is never logged.
 *
 * After `mileage-app` (`src/native/database.ts`).
 */
const ports: OpenPorts = {
  secrets: keychain,
  randomKey: async () => hex(await Crypto.getRandomBytesAsync(32)),
  openFile: async (): Promise<FileHandle> => {
    const sqlite = await SQLite.openDatabaseAsync(databaseFile);
    return {
      exec: (sql) => sqlite.execAsync(sql),
      run: async (sql, params) => {
        await sqlite.runAsync(sql, params as SqlValue[]);
      },
      all: (sql, params) => sqlite.getAllAsync<Record<string, SqlValue>>(sql, params as SqlValue[]),
      close: () => sqlite.closeAsync(),
    };
  },
  deleteFile: () => SQLite.deleteDatabaseAsync(databaseFile),
};

// TEMPORARY, removed before the stage is done: a test build made with
// EXPO_PUBLIC_RUTSA_TEST_DB_FAILURE=wrong_key|newer|no_cipher pretends that the open fails, until
// the trips are erased, so the screen of the failure can be tried on the emulator. Metro writes the
// value into the bundle when the build is made; a normal build has none. Gradle does not see the
// variable as an input: after a change of it, delete android/app/build/generated/assets/react and
// android/app/build/generated/res/react (and the Metro cache), or the old bundle is packed again.
const pretended = testFailure(process.env.EXPO_PUBLIC_RUTSA_TEST_DB_FAILURE);
let erased = false;

let opened: Promise<Opened> | null = null;

/**
 * The one connection of this process, opened on first use and opened again after a failed try. The
 * location task and the screens share it.
 */
export function openDatabase(): Promise<Opened> {
  if (pretended !== null && !erased) return Promise.reject(pretended);
  opened ??= openWith(ports, databaseKey).catch((error: unknown) => {
    opened = null;
    throw error;
  });
  return opened;
}

/**
 * Erases the database file and its key after an open failed (the screen of the failure): the next
 * `openDatabase` makes a new key and an empty file. The trips on this phone are gone for good.
 *
 * After `eraseDatabase` of `mileage-app` (`src/native/database.ts`).
 */
export async function eraseDatabase(): Promise<void> {
  await eraseDatabaseAndKey(
    { deleteFile: () => SQLite.deleteDatabaseAsync(databaseFile), secrets: keychain },
    databaseKey,
  );
  erased = true;
}
