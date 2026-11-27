import * as Crypto from "expo-crypto";
import type { TripRepository } from "../core/repository.ts";
import { createSettings, type Settings } from "../core/settings.ts";
import { applySeed, seedPlan } from "../core/seed.ts";
import { createSqlStore } from "../core/sql-store.ts";
import { openDatabase } from "./database.ts";

let store: Promise<TripRepository> | null = null;

// TEMPORARY, removed before the stage is done (F3): a test build made with
// EXPO_PUBLIC_RUTSA_SEED=1 fills an empty database with weeks of test trips (`core/seed.ts`), once,
// before the screens read it. Metro writes the value into the bundle when the build is made (build
// with `emulator:ship -- --fresh`, see `database.ts`); a normal build has none.
const seed = process.env.EXPO_PUBLIC_RUTSA_SEED === "1";

/** The phone's midnight before `ms`. */
function midnightOf(ms: number): number {
  const day = new Date(ms);
  day.setHours(0, 0, 0, 0);
  return day.getTime();
}

async function seedWhenAsked(trips: TripRepository): Promise<TripRepository> {
  if (seed && (await trips.listTrips()).length === 0) {
    await applySeed(trips, seedPlan(Date.now(), midnightOf));
  }
  return trips;
}

/**
 * The trips over the encrypted database. The screens and the location task both call this: they
 * share one store and one connection in one process. A trip's id is a random UUID.
 *
 * After `mileage-app` (`src/native/store.ts`).
 */
export function getStore(): Promise<TripRepository> {
  store ??= openDatabase()
    .then(({ db }) => createSqlStore(db, { newId: () => Crypto.randomUUID() }))
    .then(seedWhenAsked)
    .catch((error: unknown) => {
      store = null;
      throw error;
    });
  return store;
}

/** The settings of the app, in the same database (`core/settings.ts`). */
export async function getSettings(): Promise<Settings> {
  return createSettings((await openDatabase()).db);
}

/** What the database says about itself: the SQLCipher version, and whether a stale file was replaced. */
export async function databaseInfo(): Promise<{
  cipherVersion: string;
  recoveredStaleFile: boolean;
}> {
  const { cipherVersion, recoveredStaleFile } = await openDatabase();
  return { cipherVersion, recoveredStaleFile };
}
