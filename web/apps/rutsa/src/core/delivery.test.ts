import { beforeEach, describe, expect, test } from "vite-plus/test";
import { memoryDb } from "../test-support/memory-db.ts";
import { storeDelivery } from "./delivery.ts";
import type { Position } from "./fix.ts";
import type { TripRepository } from "./repository.ts";
import { migrate } from "./schema.ts";
import { createSqlStore } from "./sql-store.ts";

const T0 = Date.parse("2026-11-20T09:00:00Z");

/** About 0.00001 degrees of latitude are 1.11 m. */
const north = (metres: number) => 59 + metres / 111_195;

/** `n` positions 5 s apart from `T0 + fromMs`, 100 m apart, as expo-location hands them over. */
function positions(n: number, fromMs = 0): Position[] {
  return Array.from({ length: n }, (_, i) => ({
    timestamp: T0 + fromMs + i * 5000,
    coords: { latitude: north(i * 100), longitude: 18, accuracy: 5, speed: 20 },
  }));
}

let store: TripRepository;

beforeEach(async () => {
  const db = memoryDb();
  await migrate(db);
  let next = 0;
  store = createSqlStore(db, { newId: () => `t${(next += 1)}` });
});

describe("a delivery of the location service", () => {
  test("goes to the trip that is recording, and the updates go on", async () => {
    const trip = await store.startTrip(T0);
    expect(await storeDelivery(store, positions(3))).toEqual({ stored: 3, stopUpdates: false });
    expect(await store.points(trip.id)).toHaveLength(3);
  });

  test("a paused trip takes none, and the updates are left to Pause", async () => {
    const trip = await store.startTrip(T0);
    await store.pauseTrip(trip.id, T0 + 1000);
    expect(await storeDelivery(store, positions(3))).toEqual({ stored: 0, stopUpdates: false });
    expect(await store.points(trip.id)).toHaveLength(0);
  });

  test("with no active trip nothing is stored and the updates are stopped", async () => {
    expect(await storeDelivery(store, positions(2))).toEqual({ stored: 0, stopUpdates: true });
    const trip = await store.startTrip(T0);
    await store.stopTrip(trip.id, T0 + 1000);
    expect(await storeDelivery(store, positions(2, 2000))).toEqual({
      stored: 0,
      stopUpdates: true,
    });
    expect(await store.points(trip.id)).toHaveLength(0);
  });

  test("an empty delivery changes nothing, with or without a trip", async () => {
    expect(await storeDelivery(store, [])).toEqual({ stored: 0, stopUpdates: false });
    await store.startTrip(T0);
    expect(await storeDelivery(store, [])).toEqual({ stored: 0, stopUpdates: false });
  });

  test("positions that are not valid are dropped and only the others are counted", async () => {
    const trip = await store.startTrip(T0);
    const delivery = positions(3).map((p, i): Position =>
      // CoreLocation says a position is not valid with a negative accuracy; 200° is off the Earth.
      i === 1
        ? { ...p, coords: { ...p.coords, accuracy: -1 } }
        : i === 2
          ? { ...p, coords: { ...p.coords, latitude: 200 } }
          : p,
    );
    expect(await storeDelivery(store, delivery)).toEqual({ stored: 1, stopUpdates: false });
    expect(await store.points(trip.id)).toHaveLength(1);
  });
});
