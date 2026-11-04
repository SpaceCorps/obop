import * as Crypto from "expo-crypto";
import * as SQLite from "expo-sqlite";
import { Platform } from "react-native";
import { openOutbox, policyFor, type FileHandle, type OpenPorts } from "../core/open-outbox.ts";
import type { SqlDb, SqlValue } from "../core/sql.ts";
import type { StartupConfig } from "../core/startup-config.ts";
import { hostConfig } from "./host.ts";
import { keychain } from "./keychain.ts";

const hex = (bytes: Uint8Array): string =>
  Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");

/**
 * The parts of the phone that `openOutbox` (`core/open-outbox.ts`, where the procedure and its
 * tests are) needs. The key of the database is 32 random bytes made on the first start and kept in
 * the Android Keystore or the iOS Keychain (after first unlock, this device only: `keychain.ts`).
 * The key is never logged. The name of the file is the app's (`identity.storage.databaseFile`).
 */
function portsFor(databaseFile: string): OpenPorts {
  return {
    secrets: keychain,
    randomKey: async () => hex(await Crypto.getRandomBytesAsync(32)),
    openFile: async (): Promise<FileHandle> => {
      const sqlite = await SQLite.openDatabaseAsync(databaseFile);
      return {
        exec: (sql) => sqlite.execAsync(sql),
        run: async (sql, params) => {
          await sqlite.runAsync(sql, params as SqlValue[]);
        },
        all: (sql, params) =>
          sqlite.getAllAsync<Record<string, SqlValue>>(sql, params as SqlValue[]),
        close: () => sqlite.closeAsync(),
      };
    },
    deleteFile: () => SQLite.deleteDatabaseAsync(databaseFile),
    now: Date.now,
  };
}

/** What the encrypted database says about itself (shown in Account). */
export interface DatabaseInfo {
  /** The version of SQLCipher the database runs on. */
  cipherVersion: string;
}

type Opened = { db: SqlDb; info: DatabaseInfo };

/**
 * The opener of the database of an app: the one connection of a process, opened on first use, and
 * opened again after a failed attempt. The names come from the configuration it is given.
 */
export function createDatabaseOpener(config: StartupConfig): () => Promise<Opened> {
  const { storage } = config.identity;
  let opened: Promise<Opened> | null = null;
  const open = async (): Promise<Opened> => {
    const result = await openOutbox(
      portsFor(storage.databaseFile),
      { databaseKey: storage.databaseKey, session: [storage.server, storage.tokens] },
      policyFor(Platform.OS),
    );
    return { db: result.db, info: { cipherVersion: result.cipherVersion } };
  };
  return () => {
    opened ??= open().catch((error: unknown) => {
      opened = null;
      throw error;
    });
    return opened;
  };
}

let opener: (() => Promise<Opened>) | null = null;

/**
 * The one connection of this process, opened on first use, with the configuration the app started
 * the host with. The location task and the screens share it. The configuration is read when this
 * is called and not when the file is imported: the task imports this file before anything else.
 */
export async function openDatabase(): Promise<Opened> {
  opener ??= createDatabaseOpener(hostConfig());
  return opener();
}
