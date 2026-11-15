import * as Crypto from "expo-crypto";
import * as SecureStore from "expo-secure-store";
import * as SQLite from "expo-sqlite";
import { createSqlDb, type RawDb, type SqlDb, type SqlValue } from "../core/sql.ts";
import { migrate } from "../core/sql-store.ts";
import { DATABASE_KEY_NAME, DATABASE_NAME } from "./config.ts";

const hex = (bytes: Uint8Array): string =>
  Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");

/**
 * The key of the database: 32 random bytes made on the first start and kept in the Android
 * Keystore (expo-secure-store encrypts the value with a Keystore key that never leaves the
 * hardware-backed store). The key is never logged.
 */
async function databaseKey(): Promise<string> {
  const known = await SecureStore.getItemAsync(DATABASE_KEY_NAME);
  if (known && /^[0-9a-f]{64}$/.test(known)) return known;
  const key = hex(await Crypto.getRandomBytesAsync(32));
  await SecureStore.setItemAsync(DATABASE_KEY_NAME, key);
  return key;
}

/** What the encrypted database says about itself (shown in Account). */
export interface DatabaseInfo {
  /** The version of SQLCipher the database runs on. */
  cipherVersion: string;
}

let opened: Promise<{ db: SqlDb; info: DatabaseInfo }> | null = null;

async function open(): Promise<{ db: SqlDb; info: DatabaseInfo }> {
  const key = await databaseKey();
  const sqlite = await SQLite.openDatabaseAsync(DATABASE_NAME);
  // The key goes first; nothing else may read the file before it.
  await sqlite.execAsync(`PRAGMA key = "x'${key}'"`);
  const version = await sqlite.getAllAsync<{ cipher_version: string }>("PRAGMA cipher_version");
  const cipherVersion = version[0]?.cipher_version;
  if (!cipherVersion) {
    // Plain SQLite would store the route in the clear. Fail closed.
    await sqlite.closeAsync();
    throw new Error("The database is not encrypted: this build has no SQLCipher");
  }
  await sqlite.execAsync("PRAGMA busy_timeout = 5000");
  await sqlite.execAsync("PRAGMA journal_mode = WAL");
  const raw: RawDb = {
    exec: (sql) => sqlite.execAsync(sql),
    run: async (sql, params) => {
      await sqlite.runAsync(sql, params as SqlValue[]);
    },
    all: (sql, params) => sqlite.getAllAsync<Record<string, SqlValue>>(sql, params as SqlValue[]),
  };
  const db = createSqlDb(raw);
  // A wrong key shows here as "file is not a database": the file is left as it is.
  await migrate(db);
  return { db, info: { cipherVersion } };
}

/** The one connection of this process, opened on first use. The location task and the screens share it. */
export function openDatabase(): Promise<{ db: SqlDb; info: DatabaseInfo }> {
  opened ??= open().catch((error: unknown) => {
    opened = null;
    throw error;
  });
  return opened;
}
