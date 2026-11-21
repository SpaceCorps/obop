import { beforeEach, describe, expect, test } from "vite-plus/test";
import { memoryDb } from "../test-support/memory-db.ts";
import { storeDelivery } from "./delivery.ts";
import type { Position } from "./fix.ts";
import type { Readiness } from "./liveness.ts";
import {
  CannotRecord,
  JUST_STARTED_MS,
  createRecording,
  type LocationUpdates,
  type Recording,
} from "./recording.ts";
import { TripAlreadyActive, TripStateError, type TripRepository } from "./repository.ts";
import { migrate } from "./schema.ts";
import { createSqlStore } from "./sql-store.ts";

const T0 = Date.parse("2026-11-20T09:00:00Z");
const MIN = 60_000;

/** About 0.00001 degrees of latitude are 1.11 m. */
const north = (metres: number) => 59 + metres / 111_195;

/** `n` positions 5 s apart from `T0 + fromMs`, north at 10 m/s from `startM` metres. */
function drive(n: number, fromMs: number, startM = 0): Position[] {
  return Array.from({ length: n }, (_, i) => ({
    timestamp: T0 + fromMs + i * 5000,
    coords: { latitude: north(startM + i * 50), longitude: 18, accuracy: 5, speed: 10 },
  }));
}

/** The location updates of a test: what the phone says, and whether the system refuses. */
class FakeLocation implements LocationUpdates {
  ready: Readiness = "ready";
  running = false;
  starts = 0;
  stops = 0;
  failStart = false;
  failStop = false;

  async readiness() {
    return this.ready;
  }
  async start() {
    this.starts += 1;
    if (this.failStart) throw new Error("the system refused");
    this.running = true;
  }
  async stop() {
    this.stops += 1;
    if (this.failStop) throw new Error("the service is gone");
    this.running = false;
  }
}

let clock: number;
let store: TripRepository;
let location: FakeLocation;
let recording: Recording;

beforeEach(async () => {
  const db = memoryDb();
  await migrate(db);
  let next = 0;
  store = createSqlStore(db, { newId: () => `t${(next += 1)}` });
  location = new FakeLocation();
  clock = T0;
  recording = createRecording({ store, location, now: () => clock });
});

describe("Start", () => {
  test("stores a recording trip and starts the location updates", async () => {
    const trip = await recording.start();
    expect(trip).toMatchObject({ state: "recording", segment: 1, startedAt: T0 });
    expect(await store.activeTrip()).toEqual(trip);
    expect(location.running).toBe(true);
  });

  test("a phone that cannot record says why, and nothing is stored or started", async () => {
    for (const reason of ["no_permission", "approximate", "services_off"] as const) {
      location.ready = reason;
      const refusal = await recording.start().catch((e: unknown) => e);
      expect(refusal).toBeInstanceOf(CannotRecord);
      expect((refusal as CannotRecord).reason).toBe(reason);
    }
    expect(await store.listTrips()).toEqual([]);
    expect(location.starts).toBe(0);
  });

  test("when the system refuses the updates, its error is thrown and no trip is left", async () => {
    location.failStart = true;
    await expect(recording.start()).rejects.toThrow("the system refused");
    expect(await store.activeTrip()).toBeNull();
    expect(await store.listTrips()).toEqual([]);
    location.failStart = false;
    expect((await recording.start()).state).toBe("recording");
  });

  test("a second Start while a trip is open is refused before the updates are touched", async () => {
    const first = await recording.start();
    await expect(recording.start()).rejects.toThrow(TripAlreadyActive);
    await recording.pause(first.id);
    await expect(recording.start()).rejects.toThrow(TripAlreadyActive);
    expect(location.starts).toBe(1);
    expect((await store.activeTrip())?.id).toBe(first.id);
  });
});

describe("Pause", () => {
  test("pauses the trip and stops the updates; a late delivery is not stored", async () => {
    const trip = await recording.start();
    clock += MIN;
    expect((await recording.pause(trip.id)).state).toBe("paused");
    expect(location.running).toBe(false);
    expect(await storeDelivery(store, drive(3, MIN))).toEqual({ stored: 0, stopUpdates: false });
  });

  test("the trip stays paused when the updates cannot be stopped", async () => {
    const trip = await recording.start();
    location.failStop = true;
    expect((await recording.pause(trip.id)).state).toBe("paused");
    expect((await store.getTrip(trip.id))?.state).toBe("paused");
  });

  test("a trip that is not recording cannot be paused, and the updates are not touched", async () => {
    const trip = await recording.start();
    await recording.pause(trip.id);
    const stops = location.stops;
    await expect(recording.pause(trip.id)).rejects.toThrow(TripStateError);
    await recording.stop(trip.id);
    await expect(recording.pause(trip.id)).rejects.toThrow(TripStateError);
    expect(location.stops).toBe(stops + 1);
  });
});

describe("Resume", () => {
  test("goes on recording in a new segment and starts the updates again", async () => {
    const trip = await recording.start();
    await recording.pause(trip.id);
    const resumed = await recording.resume(trip.id);
    expect(resumed).toMatchObject({ state: "recording", segment: 2 });
    expect(location.running).toBe(true);
  });

  test("a phone that cannot record says why, and the trip stays paused", async () => {
    const trip = await recording.start();
    await recording.pause(trip.id);
    location.ready = "services_off";
    await expect(recording.resume(trip.id)).rejects.toThrow(CannotRecord);
    expect(await store.getTrip(trip.id)).toMatchObject({ state: "paused", segment: 1 });
    expect(location.starts).toBe(1);
  });

  test("when the system refuses the updates, the trip is paused again and can be resumed later", async () => {
    const trip = await recording.start();
    await recording.pause(trip.id);
    location.failStart = true;
    await expect(recording.resume(trip.id)).rejects.toThrow("the system refused");
    expect((await store.getTrip(trip.id))?.state).toBe("paused");
    location.failStart = false;
    expect((await recording.resume(trip.id)).state).toBe("recording");
  });

  test("a trip that records or has ended cannot be resumed", async () => {
    const trip = await recording.start();
    await expect(recording.resume(trip.id)).rejects.toThrow(TripStateError);
    await recording.stop(trip.id);
    await expect(recording.resume(trip.id)).rejects.toThrow(TripStateError);
  });
});

describe("Stop", () => {
  test("stops the updates and ends the trip, recording or paused", async () => {
    const a = await recording.start();
    clock += MIN;
    expect(await recording.stop(a.id)).toMatchObject({ state: "ended", endedAt: T0 + MIN });
    expect(location.running).toBe(false);
    const b = await recording.start();
    await recording.pause(b.id);
    expect((await recording.stop(b.id)).state).toBe("ended");
    expect(await store.activeTrip()).toBeNull();
  });

  test("the trip is ended also when the updates cannot be stopped", async () => {
    const trip = await recording.start();
    location.failStop = true;
    expect((await recording.stop(trip.id)).state).toBe("ended");
    // The next delivery finds no trip and the task stops the updates (C3).
    expect(await storeDelivery(store, drive(2, MIN))).toEqual({ stored: 0, stopUpdates: true });
  });

  test("an ended trip cannot be stopped again", async () => {
    const trip = await recording.start();
    await recording.stop(trip.id);
    await expect(recording.stop(trip.id)).rejects.toThrow(TripStateError);
  });
});

describe("the check when the app opens or comes back to the front", () => {
  /** The process of the app was ended: the service with it, the trip stays in the database. */
  const processEnded = () => {
    location.running = false;
  };

  test("no trip: updates that were left running are stopped, nothing is started", async () => {
    location.running = true;
    expect(await recording.ensure()).toEqual({ kind: "none" });
    expect(location.running).toBe(false);
    expect(location.starts).toBe(0);
  });

  test("a paused trip stays paused and updates that were left running are stopped", async () => {
    const trip = await recording.start();
    await recording.pause(trip.id);
    // A crash between storing the pause and stopping the updates leaves them running.
    location.running = true;
    clock += 10 * MIN;
    const ensured = await recording.ensure();
    expect(ensured).toMatchObject({ kind: "paused", trip: { id: trip.id, state: "paused" } });
    expect(location.running).toBe(false);
    expect(location.starts).toBe(1);
  });

  test("a stop that fails is no error: the next check and the task stop them again", async () => {
    location.running = true;
    location.failStop = true;
    expect(await recording.ensure()).toEqual({ kind: "none" });
  });

  test("a recording trip whose process was ended gets its updates started again, and is not changed", async () => {
    const trip = await recording.start();
    processEnded();
    clock += 5 * MIN;
    const ensured = await recording.ensure();
    expect(ensured).toEqual({ kind: "restarted", trip });
    expect(location.running).toBe(true);
    expect(location.starts).toBe(2);
    expect(await store.activeTrip()).toEqual(trip);
  });

  test("updates that still run are started again too: being registered proves nothing", async () => {
    const trip = await recording.start();
    clock += MIN;
    expect(await recording.ensure()).toEqual({ kind: "restarted", trip });
    expect(location.starts).toBe(2);
  });

  test("right after Start or Resume nothing is started again", async () => {
    const trip = await recording.start();
    clock += JUST_STARTED_MS - 1;
    expect(await recording.ensure()).toEqual({ kind: "just_started", trip });
    clock += 1;
    expect((await recording.ensure()).kind).toBe("restarted");

    // Long after Start, but just after a Resume: the start of the segment counts.
    clock += 20 * MIN;
    await recording.pause(trip.id);
    clock += 5 * MIN;
    await recording.resume(trip.id);
    const starts = location.starts;
    clock += 5000;
    expect((await recording.ensure()).kind).toBe("just_started");
    expect(location.starts).toBe(starts);
  });

  test("a phone that cannot record says why, and the trip goes on recording, with its points", async () => {
    const trip = await recording.start();
    await storeDelivery(store, drive(3, 0));
    processEnded();
    clock += 5 * MIN;
    for (const reason of ["no_permission", "approximate", "services_off"] as const) {
      location.ready = reason;
      expect(await recording.ensure()).toEqual({ kind: "blocked", trip, reason });
    }
    expect(location.starts).toBe(1);
    expect(location.running).toBe(false);
    expect(await store.activeTrip()).toEqual(trip);
    expect(await store.points(trip.id)).toHaveLength(3);

    // Location is switched on again and the driver comes back to the app.
    location.ready = "ready";
    expect((await recording.ensure()).kind).toBe("restarted");
    expect(location.running).toBe(true);
  });

  test("when the system refuses, its error is thrown and the trip is neither ended nor paused", async () => {
    const trip = await recording.start();
    processEnded();
    clock += 5 * MIN;
    location.failStart = true;
    await expect(recording.ensure()).rejects.toThrow("the system refused");
    expect(await store.activeTrip()).toEqual(trip);
    location.failStart = false;
    expect((await recording.ensure()).kind).toBe("restarted");
  });

  test("1 km, the app is ended while the car drives 1.2 km, the app is opened, 1 km more: 3.2 km", async () => {
    const trip = await recording.start();
    // 100 s at 10 m/s: 0 to 1,000 m.
    await storeDelivery(store, drive(21, 0));
    processEnded();
    // No position for two minutes while the car goes on at 10 m/s; then the driver opens the app.
    clock = T0 + 220_000;
    expect((await recording.ensure()).kind).toBe("restarted");
    // 2,200 to 3,200 m. The road between the last position before and the first after is counted
    // as a straight line, at a speed a car can drive.
    await storeDelivery(store, drive(21, 220_000, 2200));
    clock = T0 + 6 * MIN;
    const ended = await recording.stop(trip.id);
    expect(ended.distanceM).toBeCloseTo(3200, 0);
  });
});

describe("a trip from Start to Stop", () => {
  test("1 km, a pause while the car moves 4 km, 1 km more: 2 km", async () => {
    const trip = await recording.start();
    // 100 s at 10 m/s, a fix every 5 s: 1,000 m.
    await storeDelivery(store, drive(21, 0));
    clock = T0 + 2 * MIN;
    await recording.pause(trip.id);
    // Fixes that arrive during the pause are dropped.
    await storeDelivery(store, drive(5, 3 * MIN, 2000));
    clock = T0 + 5 * MIN;
    await recording.resume(trip.id);
    await storeDelivery(store, drive(21, 5 * MIN, 5000));
    clock = T0 + 7 * MIN;
    const ended = await recording.stop(trip.id);
    expect(ended).toMatchObject({ state: "ended", startedAt: T0, endedAt: T0 + 7 * MIN });
    expect(ended.distanceM).toBeCloseTo(2000, 0);
    expect(location.running).toBe(false);
  });
});
