import { distanceMeters } from "@obop/trip-core";
import { describe, expect, test } from "vite-plus/test";
import { createSqlStore, migrate } from "./sql-store.ts";
import { keepFix, RECORDING_THINNING, thinningFor, type Anchor } from "./thin.ts";
import type { NewFix } from "./types.ts";
import { memoryDb } from "../test-support/memory-db.ts";

const T0 = Date.parse("2026-10-15T09:00:00Z");
const { minGapMs, farM } = RECORDING_THINNING;

/** About 0.00001 degrees of latitude are 1.11 m. */
const north = (metres: number) => 52 + metres / 111_195;
const here = (t: number, metres = 0): Anchor => ({ t, lat: north(metres), lon: 13 });

describe("which fixes are kept", () => {
  test("the first fix of a trip is kept", () => {
    expect(keepFix(null, here(T0), RECORDING_THINNING)).toBe(true);
  });

  test("a fix that comes soon after the last one and close to it is dropped", () => {
    expect(keepFix(here(T0), here(T0 + 1000, 14), RECORDING_THINNING)).toBe(false);
    expect(keepFix(here(T0), here(T0 + minGapMs - 1, 0), RECORDING_THINNING)).toBe(false);
  });

  test("a fix at the minimum gap, or later, is kept however close it is", () => {
    expect(keepFix(here(T0), here(T0 + minGapMs, 0), RECORDING_THINNING)).toBe(true);
    expect(keepFix(here(T0), here(T0 + 5000, 0), RECORDING_THINNING)).toBe(true);
  });

  test("a fix that is far from the last one is kept even when it comes soon", () => {
    expect(keepFix(here(T0), here(T0 + 500, farM + 1), RECORDING_THINNING)).toBe(true);
    expect(keepFix(here(T0), here(T0 + 500, farM - 5), RECORDING_THINNING)).toBe(false);
  });

  test("a fix exactly as far as the limit is kept", () => {
    const a = here(T0);
    const b = here(T0 + 500, 30);
    const exactly = { minGapMs: 10_000, farM: distanceMeters(a, b) };
    expect(keepFix(a, b, exactly)).toBe(true);
    expect(keepFix(a, b, { ...exactly, farM: exactly.farM + 0.001 })).toBe(false);
  });
});

async function rig(options?: Parameters<typeof createSqlStore>[1]) {
  const db = memoryDb();
  await migrate(db);
  const store = createSqlStore(db, options);
  await store.startTrip({
    clientTripId: "c1",
    purpose: "business",
    startedAt: new Date(T0).toISOString(),
    nowMs: T0,
  });
  return store;
}

/** `n` fixes `everyMs` apart, moving north at `mps`. */
function drive(n: number, everyMs: number, mps: number, from = 0): NewFix[] {
  return Array.from({ length: n }, (_, i) => ({
    t: T0 + (from + i) * everyMs,
    lat: north(((from + i) * everyMs * mps) / 1000),
    lon: 13,
    accuracyM: 5,
    speedMps: mps,
  }));
}

describe("thinning in the outbox", () => {
  test("iOS delivers a fix every second: about every second one is stored, with no gap in seq", async () => {
    const store = await rig();
    // Ten seconds at 14 m/s (50 km/h): 14 m between fixes, below the 50 m of a far jump.
    expect(await store.appendFixes("c1", drive(10, 1000, 14))).toBe(5);
    const points = await store.points("c1");
    expect(points.map((p) => p.seq)).toEqual([1, 2, 3, 4, 5]);
    expect(points.map((p) => p.t - T0)).toEqual([0, 2000, 4000, 6000, 8000]);
  });

  test("Android delivers a fix about every five seconds: all of them are stored", async () => {
    const store = await rig();
    expect(await store.appendFixes("c1", drive(20, 5000, 14))).toBe(20);
  });

  test("the last stored fix counts across calls, not only inside one call", async () => {
    const store = await rig();
    expect(await store.appendFixes("c1", drive(1, 1000, 0))).toBe(1);
    // One second after the stored fix, in a call of its own: dropped. Three seconds: kept.
    expect(await store.appendFixes("c1", drive(1, 1000, 0, 1))).toBe(0);
    expect(await store.appendFixes("c1", drive(1, 1000, 0, 4))).toBe(1);
    expect((await store.points("c1")).map((p) => p.seq)).toEqual([1, 2]);
  });

  test("a fix that jumped far in a second is kept", async () => {
    const store = await rig();
    // 60 m in one second is 216 km/h, but a jump that large is a position and not the same one again.
    expect(await store.appendFixes("c1", drive(3, 1000, 60))).toBe(3);
  });

  test("without a thinning every fix is stored", async () => {
    const store = await rig({ thinning: null });
    expect(await store.appendFixes("c1", drive(10, 1000, 14))).toBe(10);
  });

  test("a batch that arrives out of order is sorted first and thinned in time order", async () => {
    // Without the sort the first fix is t=5000, and the next two (t=0, t=2500) are older than the
    // anchor or closer than the gap to it: one point instead of three. Sorted, they are 2500 ms
    // apart and all kept.
    const store = await rig();
    const fix = (t: number): NewFix => ({
      t: T0 + t,
      lat: north(0),
      lon: 13,
      accuracyM: 5,
      speedMps: 0,
    });
    expect(await store.appendFixes("c1", [fix(5000), fix(0), fix(2500)])).toBe(3);
    expect((await store.points("c1")).map((p) => p.t - T0)).toEqual([0, 2500, 5000]);
  });
});

describe("the thinning of a build", () => {
  test("is the rule unless a test build was made without it", () => {
    expect(thinningFor(undefined)).toBe(RECORDING_THINNING);
    expect(thinningFor({})).toBe(RECORDING_THINNING);
    expect(thinningFor({ cleartextAllowed: false, thinningOff: false })).toBe(RECORDING_THINNING);
    expect(thinningFor({ cleartextAllowed: true, thinningOff: true })).toBeNull();
  });

  test("a build that does not allow plain http keeps the rule whatever else it says", () => {
    expect(thinningFor({ cleartextAllowed: false, thinningOff: true })).toBe(RECORDING_THINNING);
    expect(thinningFor({ thinningOff: true })).toBe(RECORDING_THINNING);
    expect(thinningFor({ cleartextAllowed: true })).toBe(RECORDING_THINNING);
    expect(thinningFor({ cleartextAllowed: true, thinningOff: "off" })).toBe(RECORDING_THINNING);
  });
});
