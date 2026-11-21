import * as Crypto from "expo-crypto";
import * as SQLite from "expo-sqlite";
import { identity } from "../../config/identity.ts";
import {
  openDatabase as openWith,
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

let opened: Promise<Opened> | null = null;

/**
 * The one connection of this process, opened on first use and opened again after a failed try. The
 * location task and the screens share it.
 */
export function openDatabase(): Promise<Opened> {
  opened ??= openWith(ports, databaseKey).catch((error: unknown) => {
    opened = null;
    throw error;
  });
  return opened;
}
