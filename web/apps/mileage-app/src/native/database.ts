import * as Crypto from "expo-crypto";
import * as SQLite from "expo-sqlite";
import { Platform } from "react-native";
import { openOutbox, policyFor, type FileHandle, type OpenPorts } from "../core/open-outbox.ts";
import type { SqlDb, SqlValue } from "../core/sql.ts";
import { DATABASE_KEY_NAME, DATABASE_NAME, SERVER_KEY_NAME, TOKENS_KEY_NAME } from "./config.ts";
import { keychain } from "./keychain.ts";

const hex = (bytes: Uint8Array): string =>
  Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");

/**
 * The parts of the phone that `openOutbox` (`core/open-outbox.ts`, where the procedure and its
 * tests are) needs. The key of the database is 32 random bytes made on the first start and kept in
 * the Android Keystore or the iOS Keychain (after first unlock, this device only: `keychain.ts`).
 * The key is never logged.
 */
const ports: OpenPorts = {
  secrets: keychain,
  randomKey: async () => hex(await Crypto.getRandomBytesAsync(32)),
  openFile: async (): Promise<FileHandle> => {
    const sqlite = await SQLite.openDatabaseAsync(DATABASE_NAME);
    return {
      exec: (sql) => sqlite.execAsync(sql),
      run: async (sql, params) => {
        await sqlite.runAsync(sql, params as SqlValue[]);
      },
      all: (sql, params) => sqlite.getAllAsync<Record<string, SqlValue>>(sql, params as SqlValue[]),
      close: () => sqlite.closeAsync(),
    };
  },
  deleteFile: () => SQLite.deleteDatabaseAsync(DATABASE_NAME),
  now: Date.now,
};

/** What the encrypted database says about itself (shown in Account). */
export interface DatabaseInfo {
  /** The version of SQLCipher the database runs on. */
  cipherVersion: string;
}

let opened: Promise<{ db: SqlDb; info: DatabaseInfo }> | null = null;

async function open(): Promise<{ db: SqlDb; info: DatabaseInfo }> {
  const result = await openOutbox(
    ports,
    { databaseKey: DATABASE_KEY_NAME, session: [SERVER_KEY_NAME, TOKENS_KEY_NAME] },
    policyFor(Platform.OS),
  );
  return { db: result.db, info: { cipherVersion: result.cipherVersion } };
}

/** The one connection of this process, opened on first use. The location task and the screens share it. */
export function openDatabase(): Promise<{ db: SqlDb; info: DatabaseInfo }> {
  opened ??= open().catch((error: unknown) => {
    opened = null;
    throw error;
  });
  return opened;
}
