import * as Crypto from "expo-crypto";
import type { TripRepository } from "../core/repository.ts";
import { createSqlStore } from "../core/sql-store.ts";
import { openDatabase } from "./database.ts";

let store: Promise<TripRepository> | null = null;

/**
 * The trips over the encrypted database. The screens and the location task both call this: they
 * share one store and one connection in one process. A trip's id is a random UUID.
 *
 * After `mileage-app` (`src/native/store.ts`).
 */
export function getStore(): Promise<TripRepository> {
  store ??= openDatabase()
    .then(({ db }) => createSqlStore(db, { newId: () => Crypto.randomUUID() }))
    .catch((error: unknown) => {
      store = null;
      throw error;
    });
  return store;
}

/** What the database says about itself: the SQLCipher version, and whether a stale file was replaced. */
export async function databaseInfo(): Promise<{
  cipherVersion: string;
  recoveredStaleFile: boolean;
}> {
  const { cipherVersion, recoveredStaleFile } = await openDatabase();
  return { cipherVersion, recoveredStaleFile };
}
