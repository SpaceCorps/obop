import { beforeEach, describe, expect, test } from "vite-plus/test";
import { memoryDb } from "../test-support/memory-db.ts";
import { DetailsTooLong } from "./details.ts";
import {
  TripAlreadyActive,
  TripNotFound,
  TripStateError,
  type TripRepository,
} from "./repository.ts";
import { migrate } from "./schema.ts";
import type { SqlDb } from "./sql.ts";
import { createSqlStore } from "./sql-store.ts";
import type { NewFix } from "./types.ts";

const T0 = Date.parse("2026-11-18T09:00:00Z");
const MIN = 60_000;

/** About 0.00001 degrees of latitude are 1.11 m. */
const north = (metres: number) => 59 + metres / 111_195;

/** `n` fixes `everyMs` apart from `T0 + fromMs`, moving north at `mps` from `startM` metres. */
function drive(n: number, everyMs: number, mps: number, fromMs = 0, startM = 0): NewFix[] {
  return Array.from({ length: n }, (_, i) => ({
    t: T0 + fromMs + i * everyMs,
    lat: north(startM + (i * everyMs * mps) / 1000),
    lon: 18,
    accuracyM: 5,
    speedMps: mps,
  }));
}

let db: SqlDb;
let store: TripRepository;

beforeEach(async () => {
  db = memoryDb();
  await migrate(db);
  let next = 0;
  store = createSqlStore(db, { newId: () => `t${(next += 1)}` });
});

describe("starting and recording", () => {
  test("a started trip is the active one, recording in segment 1", async () => {
    const trip = await store.startTrip(T0);
    expect(trip).toEqual({
      id: "t1",
      state: "recording",
      source: "recorded",
      startedAt: T0,
      endedAt: null,
      distanceM: null,
      segment: 1,
      segmentStartedAt: T0,
      name: null,
      from: null,
      to: null,
      note: null,
    });
    expect(await store.activeTrip()).toEqual(trip);
  });

  test("a second trip cannot start while one is recording or paused", async () => {
    const first = await store.startTrip(T0);
    await expect(store.startTrip(T0 + MIN)).rejects.toThrow(TripAlreadyActive);
    await store.pauseTrip(first.id, T0 + MIN);
    await expect(store.startTrip(T0 + 2 * MIN)).rejects.toThrow(TripAlreadyActive);
  });

  test("fixes are stored numbered 1, 2, 3 ..., and the next delivery goes on from there", async () => {
    const trip = await store.startTrip(T0);
    expect(await store.appendFixes(trip.id, drive(3, 5000, 10))).toBe(3);
    expect(await store.appendFixes(trip.id, drive(3, 5000, 10, 15_000, 150))).toBe(3);
    const points = await store.points(trip.id);
    expect(points.map((p) => p.seq)).toEqual([1, 2, 3, 4, 5, 6]);
    expect(points.every((p) => p.segment === 1 && p.tripId === trip.id)).toBe(true);
  });

  test("the thinning is applied: a fix every second keeps about every second one", async () => {
    const trip = await store.startTrip(T0);
    expect(await store.appendFixes(trip.id, drive(60, 1000, 14))).toBe(30);
  });

  test("a trip that is paused, ended or gone takes no fixes, and that is no error", async () => {
    const trip = await store.startTrip(T0);
    await store.pauseTrip(trip.id, T0 + MIN);
    expect(await store.appendFixes(trip.id, drive(3, 5000, 10))).toBe(0);
    await store.stopTrip(trip.id, T0 + 2 * MIN);
    expect(await store.appendFixes(trip.id, drive(3, 5000, 10))).toBe(0);
    expect(await store.appendFixes("nobody", drive(3, 5000, 10))).toBe(0);
    expect(await store.points(trip.id)).toEqual([]);
  });
});

describe("pause, resume and the distance", () => {
  test("after Resume the fixes go into segment 2, numbered on from segment 1", async () => {
    const trip = await store.startTrip(T0);
    await store.appendFixes(trip.id, drive(2, 5000, 10));
    expect((await store.pauseTrip(trip.id, T0 + MIN)).state).toBe("paused");
    const resumed = await store.resumeTrip(trip.id, T0 + 2 * MIN);
    expect(resumed).toMatchObject({ state: "recording", segment: 2 });
    await store.appendFixes(trip.id, drive(2, 5000, 10, 2 * MIN, 500));
    const points = await store.points(trip.id);
    expect(points.map((p) => [p.seq, p.segment])).toEqual([
      [1, 1],
      [2, 1],
      [3, 2],
      [4, 2],
    ]);
  });

  test("Stop computes the distance of the route", async () => {
    const trip = await store.startTrip(T0);
    // 100 s north at 10 m/s, a fix every 5 s: 1,000 m.
    await store.appendFixes(trip.id, drive(21, 5000, 10));
    const ended = await store.stopTrip(trip.id, T0 + 2 * MIN);
    expect(ended).toMatchObject({ state: "ended", endedAt: T0 + 2 * MIN });
    expect(ended.distanceM).toBeCloseTo(1000, 0);
    expect(await store.activeTrip()).toBeNull();
  });

  test("a pause adds no straight line: 1 km, pause, 4 km away, 1 km more is 2 km", async () => {
    const trip = await store.startTrip(T0);
    await store.appendFixes(trip.id, drive(21, 5000, 10));
    await store.pauseTrip(trip.id, T0 + 2 * MIN);
    await store.resumeTrip(trip.id, T0 + 5 * MIN);
    await store.appendFixes(trip.id, drive(21, 5000, 10, 5 * MIN, 5000));
    const ended = await store.stopTrip(trip.id, T0 + 7 * MIN);
    expect(ended.distanceM).toBeCloseTo(2000, 0);
  });

  test("each segment starts at Start or at its Resume", async () => {
    const trip = await store.startTrip(T0);
    expect(trip.segmentStartedAt).toBe(T0);
    await store.pauseTrip(trip.id, T0 + MIN);
    expect((await store.resumeTrip(trip.id, T0 + 5 * MIN)).segmentStartedAt).toBe(T0 + 5 * MIN);
    await store.pauseTrip(trip.id, T0 + 6 * MIN);
    // A clock set back during the pause: the segment does not start before the one before it.
    expect((await store.resumeTrip(trip.id, T0 + 2 * MIN)).segmentStartedAt).toBe(T0 + 5 * MIN);
    expect((await store.getTrip(trip.id))?.segmentStartedAt).toBe(T0 + 5 * MIN);
  });

  test("after Resume a position taken during the pause is not stored, one just before Resume is", async () => {
    const trip = await store.startTrip(T0);
    await store.appendFixes(trip.id, drive(2, 5000, 10));
    await store.pauseTrip(trip.id, T0 + MIN);
    await store.resumeTrip(trip.id, T0 + 5 * MIN);
    const duringPause = { t: T0 + 2 * MIN, lat: north(100), lon: 18, accuracyM: 5, speedMps: null };
    const justBefore = {
      t: T0 + 5 * MIN - 5000,
      lat: north(4000),
      lon: 18,
      accuracyM: 5,
      speedMps: null,
    };
    expect(await store.appendFixes(trip.id, [duringPause, justBefore])).toBe(1);
    const points = await store.points(trip.id);
    expect(points.filter((p) => p.segment === 2).map((p) => p.t)).toEqual([justBefore.t]);
  });

  test("the cached position of the pause, handed over first after Resume, adds nothing: 2 km, not 6", async () => {
    const trip = await store.startTrip(T0);
    // 1 km north, then a pause at the 1 km mark while the car goes on to the 5 km mark.
    await store.appendFixes(trip.id, drive(21, 5000, 10));
    await store.pauseTrip(trip.id, T0 + 2 * MIN);
    await store.resumeTrip(trip.id, T0 + 5 * MIN);
    const cached = { t: T0 + 2 * MIN, lat: north(1000), lon: 18, accuracyM: 5, speedMps: null };
    await store.appendFixes(trip.id, [cached, ...drive(21, 5000, 10, 5 * MIN, 5000)]);
    const ended = await store.stopTrip(trip.id, T0 + 7 * MIN);
    expect(ended.distanceM).toBeCloseTo(2000, 0);
  });

  test("a trip with no fixes ends with 0 m", async () => {
    const trip = await store.startTrip(T0);
    expect((await store.stopTrip(trip.id, T0 + MIN)).distanceM).toBe(0);
  });

  test("a paused trip can be stopped; an ended one cannot be stopped, paused or resumed", async () => {
    const trip = await store.startTrip(T0);
    await store.pauseTrip(trip.id, T0 + MIN);
    expect((await store.stopTrip(trip.id, T0 + 2 * MIN)).state).toBe("ended");
    await expect(store.stopTrip(trip.id, T0 + 3 * MIN)).rejects.toThrow(TripStateError);
    await expect(store.pauseTrip(trip.id, T0 + 3 * MIN)).rejects.toThrow(TripStateError);
    await expect(store.resumeTrip(trip.id, T0 + 3 * MIN)).rejects.toThrow(TripStateError);
  });

  test("a recording trip cannot be resumed, nor a paused one paused", async () => {
    const trip = await store.startTrip(T0);
    await expect(store.resumeTrip(trip.id, T0 + MIN)).rejects.toThrow(TripStateError);
    await store.pauseTrip(trip.id, T0 + MIN);
    await expect(store.pauseTrip(trip.id, T0 + 2 * MIN)).rejects.toThrow(TripStateError);
  });

  test("an action on a trip that does not exist is refused as not found", async () => {
    await expect(store.pauseTrip("nobody", T0)).rejects.toThrow(TripNotFound);
    await expect(store.resumeTrip("nobody", T0)).rejects.toThrow(TripNotFound);
    await expect(store.stopTrip("nobody", T0)).rejects.toThrow(TripNotFound);
  });

  test("a clock set back during the trip does not put the end before the start", async () => {
    const trip = await store.startTrip(T0);
    expect((await store.stopTrip(trip.id, T0 - MIN)).endedAt).toBe(T0);
  });

  test("after Stop a new trip can start", async () => {
    const first = await store.startTrip(T0);
    await store.stopTrip(first.id, T0 + MIN);
    const second = await store.startTrip(T0 + 2 * MIN);
    expect(second.id).toBe("t2");
  });
});

describe("the count while a trip records", () => {
  test("the km so far are the km Stop then stores, also across a pause", async () => {
    const trip = await store.startTrip(T0);
    await store.appendFixes(trip.id, drive(21, 5000, 10));
    expect((await store.progress(trip.id))?.distanceM).toBeCloseTo(1000, 0);
    await store.pauseTrip(trip.id, T0 + 2 * MIN);
    await store.resumeTrip(trip.id, T0 + 5 * MIN);
    await store.appendFixes(trip.id, drive(21, 5000, 10, 5 * MIN, 5000));
    const soFar = await store.progress(trip.id);
    const ended = await store.stopTrip(trip.id, T0 + 7 * MIN);
    expect(soFar?.distanceM).toBeCloseTo(2000, 0);
    expect(soFar?.distanceM).toBe(ended.distanceM);
  });

  test("the newest position is that of the current segment: none right after Resume", async () => {
    const trip = await store.startTrip(T0);
    expect(await store.progress(trip.id)).toEqual({
      state: "recording",
      closedMs: 0,
      distanceM: 0,
      points: 0,
      lastFixAt: null,
      signal: null,
    });
    await store.appendFixes(trip.id, drive(3, 5000, 10));
    expect((await store.progress(trip.id))?.lastFixAt).toBe(T0 + 10_000);
    await store.pauseTrip(trip.id, T0 + MIN);
    await store.resumeTrip(trip.id, T0 + 2 * MIN);
    expect(await store.progress(trip.id)).toMatchObject({ points: 3, lastFixAt: null });
  });

  test("positions that are all too imprecise give 0 km and say why", async () => {
    const trip = await store.startTrip(T0);
    const weak = drive(8, 5000, 10).map((f) => ({ ...f, accuracyM: 300 }));
    await store.appendFixes(trip.id, weak);
    expect(await store.progress(trip.id)).toMatchObject({
      distanceM: 0,
      signal: { kind: "weak", accuracyM: 300 },
    });
  });

  test("the time of a trip leaves its pauses out: each Pause ends a segment, Stop the last", async () => {
    const trip = await store.startTrip(T0);
    expect((await store.progress(trip.id))?.closedMs).toBe(0);
    // 10 min recording, 20 min paused, 5 min recording.
    await store.pauseTrip(trip.id, T0 + 10 * MIN);
    expect((await store.progress(trip.id))?.closedMs).toBe(10 * MIN);
    await store.resumeTrip(trip.id, T0 + 30 * MIN);
    expect((await store.progress(trip.id))?.closedMs).toBe(10 * MIN);
    await store.stopTrip(trip.id, T0 + 35 * MIN);
    expect((await store.progress(trip.id))?.closedMs).toBe(15 * MIN);
  });

  test("Stop from a pause keeps the end the Pause gave the segment", async () => {
    const trip = await store.startTrip(T0);
    await store.pauseTrip(trip.id, T0 + 10 * MIN);
    await store.stopTrip(trip.id, T0 + 50 * MIN);
    expect((await store.progress(trip.id))?.closedMs).toBe(10 * MIN);
  });

  test("a clock set back at Pause does not end a segment before its start", async () => {
    const trip = await store.startTrip(T0);
    await store.pauseTrip(trip.id, T0 - MIN);
    expect((await store.progress(trip.id))?.closedMs).toBe(0);
  });

  test("no trip with this id has no count", async () => {
    expect(await store.progress("nobody")).toBeNull();
  });
});

describe("the details typed at Stop", () => {
  test("are stored with the end of the trip, trimmed", async () => {
    const trip = await store.startTrip(T0);
    await store.appendFixes(trip.id, drive(21, 5000, 10));
    const ended = await store.stopTrip(trip.id, T0 + 2 * MIN, {
      name: " Meeting with the customer ",
      from: "Stockholm",
      to: "Uppsala ",
      note: "",
    });
    expect(ended).toMatchObject({
      state: "ended",
      name: "Meeting with the customer",
      from: "Stockholm",
      to: "Uppsala",
      note: null,
    });
    expect(ended.distanceM).toBeCloseTo(1000, 0);
    expect(await store.getTrip(trip.id)).toEqual(ended);
  });

  test("Stop without details leaves the trip without them, as before", async () => {
    const trip = await store.startTrip(T0);
    const ended = await store.stopTrip(trip.id, T0 + MIN);
    expect(ended).toMatchObject({ name: null, from: null, to: null, note: null });
  });

  test("a detail that is too long stops nothing: the trip goes on recording", async () => {
    const trip = await store.startTrip(T0);
    await expect(store.stopTrip(trip.id, T0 + MIN, { name: "x".repeat(101) })).rejects.toThrow(
      DetailsTooLong,
    );
    expect(await store.activeTrip()).toMatchObject({ id: trip.id, state: "recording" });
  });

  test("a paused trip is stopped with its details too", async () => {
    const trip = await store.startTrip(T0);
    await store.pauseTrip(trip.id, T0 + MIN);
    const ended = await store.stopTrip(trip.id, T0 + 2 * MIN, { name: "Lunch run" });
    expect(ended).toMatchObject({ state: "ended", name: "Lunch run" });
  });
});

describe("trips added by hand", () => {
  test("are ended, manual, with the typed distance and texts", async () => {
    const trip = await store.addManualTrip({
      startedAt: T0,
      endedAt: T0 + 30 * MIN,
      distanceM: 12_400,
      from: "  Office ",
      to: "Customer AB",
      note: "",
    });
    expect(trip).toMatchObject({
      state: "ended",
      source: "manual",
      startedAt: T0,
      endedAt: T0 + 30 * MIN,
      distanceM: 12_400,
      from: "Office",
      to: "Customer AB",
      note: null,
      // No segments: it has no positions.
      segmentStartedAt: T0,
    });
    // A trip added by hand does not block recording.
    expect(await store.activeTrip()).toBeNull();
  });

  test("an end before the start or a distance below 0 is refused", async () => {
    await expect(
      store.addManualTrip({ startedAt: T0, endedAt: T0 - 1, distanceM: 1 }),
    ).rejects.toThrow(RangeError);
    await expect(
      store.addManualTrip({ startedAt: T0, endedAt: T0, distanceM: -1 }),
    ).rejects.toThrow(RangeError);
    await expect(
      store.addManualTrip({ startedAt: T0, endedAt: T0, distanceM: Number.NaN }),
    ).rejects.toThrow(RangeError);
    expect(await store.listTrips()).toEqual([]);
  });
});

describe("reading and deleting", () => {
  const manual = (startedAt: number) =>
    store.addManualTrip({ startedAt, endedAt: startedAt + MIN, distanceM: 1000 });

  test("the list is newest first and can be limited to a range of starts", async () => {
    await manual(T0);
    await manual(T0 + 60 * MIN);
    await manual(T0 + 120 * MIN);
    expect((await store.listTrips()).map((t) => t.id)).toEqual(["t3", "t2", "t1"]);
    expect(
      (await store.listTrips({ from: T0 + 60 * MIN, to: T0 + 120 * MIN })).map((t) => t.id),
    ).toEqual(["t2"]);
    expect((await store.listTrips({ from: T0 + 61 * MIN })).map((t) => t.id)).toEqual(["t3"]);
  });

  test("a trip that does not exist is null", async () => {
    expect(await store.getTrip("nobody")).toBeNull();
  });

  test("deleting an ended trip deletes its points and segments too", async () => {
    const trip = await store.startTrip(T0);
    await store.appendFixes(trip.id, drive(3, 5000, 10));
    await store.pauseTrip(trip.id, T0 + MIN);
    await store.resumeTrip(trip.id, T0 + 2 * MIN);
    await store.stopTrip(trip.id, T0 + 3 * MIN);
    await store.deleteTrip(trip.id);
    expect(await store.getTrip(trip.id)).toBeNull();
    expect(await db.one("SELECT COUNT(*) AS n FROM point")).toEqual({ n: 0 });
    expect(await db.one("SELECT COUNT(*) AS n FROM segment")).toEqual({ n: 0 });
  });

  test("a trip that records or is paused is not deleted; one that does not exist is not found", async () => {
    const trip = await store.startTrip(T0);
    await expect(store.deleteTrip(trip.id)).rejects.toThrow(TripStateError);
    await store.pauseTrip(trip.id, T0 + MIN);
    await expect(store.deleteTrip(trip.id)).rejects.toThrow(TripStateError);
    await expect(store.deleteTrip("nobody")).rejects.toThrow(TripNotFound);
  });
});
