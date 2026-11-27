import { describe, expect, test } from "vite-plus/test";
import { memoryDb } from "../test-support/memory-db.ts";
import { migrate } from "./schema.ts";
import { applySeed, SEED_DAYS, seedPlan } from "./seed.ts";
import { createSqlStore } from "./sql-store.ts";

const HOUR = 3_600_000;
const DAY = 24 * HOUR;
// A Thursday afternoon, in UTC; the days of the test are UTC days.
const NOW = Date.parse("2026-11-26T15:00:00Z");
const midnightOf = (ms: number) => Math.floor(ms / DAY) * DAY;

describe("the plan of the test trips", () => {
  const plan = seedPlan(NOW, midnightOf);

  test("covers the last days, oldest first, every trip ended before now and none overlapping", () => {
    expect(plan.length).toBeGreaterThan(SEED_DAYS);
    expect(plan[0]!.startedAt).toBeGreaterThanOrEqual(midnightOf(NOW) - (SEED_DAYS - 1) * DAY);
    for (const [i, trip] of plan.entries()) {
      expect(trip.endedAt).toBeGreaterThan(trip.startedAt);
      expect(trip.endedAt).toBeLessThanOrEqual(NOW - 10 * 60_000);
      if (i > 0) expect(trip.startedAt).toBeGreaterThan(plan[i - 1]!.endedAt);
    }
  });

  test("has trips today and yesterday, recorded ones, some paused, and some added by hand", () => {
    const today = midnightOf(NOW);
    expect(plan.some((trip) => trip.startedAt >= today)).toBe(true);
    expect(plan.some((trip) => trip.startedAt >= today - DAY && trip.startedAt < today)).toBe(true);
    expect(plan.some((trip) => trip.source === "manual")).toBe(true);
    expect(plan.some((trip) => trip.pause !== null)).toBe(true);
    expect(plan.some((trip) => trip.name !== null)).toBe(true);
    expect(plan.some((trip) => trip.from !== null && trip.from !== trip.to)).toBe(true);
    for (const trip of plan.filter((trip) => trip.pause)) {
      expect(trip.pause!.at).toBeGreaterThan(trip.startedAt);
      expect(trip.pause!.until).toBeLessThan(trip.endedAt);
    }
  });

  test("is the same for the same hour", () => {
    expect(seedPlan(NOW + 60_000, midnightOf)).toEqual(plan);
  });

  test("early in the day, today has no trip that would end after now", () => {
    const early = Date.parse("2026-11-26T00:30:00Z");
    const planned = seedPlan(early, midnightOf);
    expect(planned.every((trip) => trip.endedAt <= early - 10 * 60_000)).toBe(true);
  });
});

describe("the test trips written into the store", () => {
  test("are ended trips with their km, the recorded ones with positions, none left open", async () => {
    const db = memoryDb();
    await migrate(db);
    let next = 0;
    const store = createSqlStore(db, { newId: () => `t${(next += 1)}` });
    const plan = seedPlan(NOW, midnightOf);

    expect(await applySeed(store, plan)).toBe(plan.length);

    const trips = await store.listTrips();
    expect(trips).toHaveLength(plan.length);
    expect(await store.activeTrip()).toBeNull();
    expect(trips.every((trip) => trip.state === "ended")).toBe(true);
    const recorded = trips.filter((trip) => trip.source === "recorded");
    expect(recorded.length).toBeGreaterThan(0);
    for (const trip of recorded) {
      const planned = plan.find((p) => p.startedAt === trip.startedAt)!;
      // The km of a straight drive at the planned speed, as Stop counts them (the pause not driven).
      const drivenMs =
        trip.endedAt! -
        trip.startedAt -
        (planned.pause ? planned.pause.until - planned.pause.at : 0);
      const expected = (planned.speedMps * drivenMs) / 1000;
      expect(trip.distanceM).toBeGreaterThan(expected * 0.9);
      expect(trip.distanceM).toBeLessThan(expected * 1.05);
      expect((await store.points(trip.id)).length).toBeGreaterThan(0);
      expect(trip.segment).toBe(planned.pause ? 2 : 1);
    }
    const named = plan.filter((p) => p.name !== null).length;
    expect(trips.filter((trip) => trip.name !== null)).toHaveLength(named);
  });
});
