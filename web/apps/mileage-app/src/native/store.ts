import Constants from "expo-constants";
import { createHostState, type HostState } from "../core/host-state.ts";
import { createSqlStore } from "../core/sql-store.ts";
import type { OutboxStore } from "../core/store.ts";
import { thinningFor } from "../core/thin.ts";
import { openDatabase, type DatabaseInfo } from "./database.ts";

let store: Promise<OutboxStore> | null = null;

/**
 * The outbox over the encrypted database. The screens and the location task both call this: they
 * share one connection in one process, and SQLite's own locking covers a second process.
 */
export function getStore(): Promise<OutboxStore> {
  store ??= openDatabase()
    .then(({ db }) => createSqlStore(db, { thinning: thinningFor(Constants.expoConfig?.extra) }))
    .catch((error: unknown) => {
      store = null;
      throw error;
    });
  return store;
}

let hostState: Promise<HostState> | null = null;

/** The host's own rows of the file (the owner of the outbox, the disclosure flag, the stale-file note), on the same connection. */
export function getHostState(): Promise<HostState> {
  hostState ??= openDatabase()
    .then(({ db }) => createHostState(db))
    .catch((error: unknown) => {
      hostState = null;
      throw error;
    });
  return hostState;
}

export async function databaseInfo(): Promise<DatabaseInfo> {
  return (await openDatabase()).info;
}
