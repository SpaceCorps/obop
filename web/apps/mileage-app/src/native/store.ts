import { createSqlStore } from "../core/sql-store.ts";
import type { OutboxStore } from "../core/store.ts";
import { openDatabase, type DatabaseInfo } from "./database.ts";

let store: Promise<OutboxStore> | null = null;

/**
 * The outbox over the encrypted database. The screens and the location task both call this: they
 * share one connection in one process, and SQLite's own locking covers a second process.
 */
export function getStore(): Promise<OutboxStore> {
  store ??= openDatabase()
    .then(({ db }) => createSqlStore(db))
    .catch((error: unknown) => {
      store = null;
      throw error;
    });
  return store;
}

export async function databaseInfo(): Promise<DatabaseInfo> {
  return (await openDatabase()).info;
}
