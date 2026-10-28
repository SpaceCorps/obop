import { describe, expect, test } from "vite-plus/test";
import { createEngine } from "./engine.ts";
import { canDiscard } from "./present.ts";
import { createSqlStore, migrate } from "./sql-store.ts";
import { TripAlreadyActive, hasWork, type OutboxStore } from "./store.ts";
import type { NewFix } from "./types.ts";
import { apiError, FakeServer, networkError } from "../test-support/fake-server.ts";
import { memoryDb } from "../test-support/memory-db.ts";

const T0 = Date.parse("2026-10-15T09:00:00Z");

/** A walk north at about 1.4 m/s: `n` fixes, 5 seconds apart, from the 1st of the walk. */
function walk(n: number, from = 0): NewFix[] {
  return Array.from({ length: n }, (_, i) => ({
    t: T0 + (from + i) * 5000,
    lat: 52 + (from + i) * 0.00006,
    lon: 13,
    accuracyM: 5,
    speedMps: 1.4,
  }));
}

async function rig(backoff = { baseMs: 1000, maxMs: 8000 }) {
  const db = memoryDb();
  await migrate(db);
  const store = createSqlStore(db);
  const server = new FakeServer();
  const engine = createEngine({ store, api: server, backoff });
  return { db, store, server, engine };
}

async function start(store: OutboxStore, id = "c1") {
  return store.startTrip({
    clientTripId: id,
    purpose: "business",
    startedAt: new Date(T0).toISOString(),
    nowMs: T0,
  });
}

async function recordAndStop(store: OutboxStore, n: number, id = "c1") {
  await start(store, id);
  await store.appendFixes(id, walk(n));
  return store.stopTrip(id, new Date(T0 + n * 5000).toISOString());
}

describe("the local store", () => {
  test("a trip starts once at a time", async () => {
    const { store } = await rig();
    await start(store, "a");
    await expect(start(store, "b")).rejects.toBeInstanceOf(TripAlreadyActive);
    await store.stopTrip("a", new Date(T0 + 1000).toISOString());
    await expect(start(store, "b")).resolves.toMatchObject({ clientTripId: "b" });
  });

  test("points get consecutive numbers and a fix not newer than the last is dropped", async () => {
    const { store } = await rig();
    await start(store);
    const fixes = walk(3);
    expect(await store.appendFixes("c1", fixes)).toBe(3);
    expect(await store.appendFixes("c1", [fixes[2]!, { ...fixes[2]!, t: fixes[2]!.t - 1 }])).toBe(
      0,
    );
    expect(await store.appendFixes("c1", walk(2, 3))).toBe(2);
    const points = await store.points("c1");
    expect(points.map((p) => p.seq)).toEqual([1, 2, 3, 4, 5]);
    expect(points[0]).toMatchObject({
      lat: "52.0000000",
      lon: "13.0000000",
      accuracyM: "5.00",
      speedMps: "1.40",
    });
  });

  test("a cached fix from before the start is not part of the trip", async () => {
    const { store } = await rig();
    await start(store);
    const stale = { ...walk(1)[0]!, t: T0 - 6 * 60_000, lat: 52.01 };
    const justBefore = { ...walk(1)[0]!, t: T0 - 4000, lat: 52.0001 };
    const fresh = walk(2, 1);
    expect(await store.appendFixes("c1", [stale, justBefore, ...fresh])).toBe(3);
    const points = await store.points("c1");
    expect(points.map((p) => p.seq)).toEqual([1, 2, 3]);
    expect(points[0]).toMatchObject({ lat: "52.0001000" });
    expect(points.every((p) => p.lat !== "52.0100000")).toBe(true);
  });

  test("the cached position is dropped from 10 seconds before the start, kept at 9", async () => {
    const { store } = await rig();
    await start(store);
    const at = (ms: number, lat: number) => ({
      t: T0 + ms,
      lat,
      lon: 13,
      accuracyM: 5,
      speedMps: 1,
    });
    expect(
      await store.appendFixes("c1", [at(-11_000, 52.011), at(-9_000, 52.009), at(1000, 52.001)]),
    ).toBe(2);
    expect((await store.points("c1")).map((p) => p.lat)).toEqual(["52.0090000", "52.0010000"]);
  });

  test("a trip holds at most 20,000 points; the rest is dropped", async () => {
    const { store } = await rig();
    await start(store);
    const many = (from: number, n: number): NewFix[] =>
      Array.from({ length: n }, (_, i) => ({
        t: T0 + (from + i) * 1000,
        lat: 52 + ((from + i) % 1000) * 0.00001,
        lon: 13,
        accuracyM: 5,
        speedMps: 1,
      }));
    expect(await store.appendFixes("c1", many(0, 19_990))).toBe(19_990);
    expect(await store.appendFixes("c1", many(19_990, 25))).toBe(10);
    expect(await store.pointCount("c1")).toBe(20_000);
    expect(await store.appendFixes("c1", many(20_015, 5))).toBe(0);
  });

  test("a trip that was never uploaded is deleted with its points, a standing refusal is cleared", async () => {
    const { store } = await rig();
    await start(store);
    await store.appendFixes("c1", walk(3));
    await store.deleteTrip("c1");
    expect(await store.getTrip("c1")).toBeNull();
    expect(await store.pointCount("c1")).toBe(0);
    expect(await store.activeTrip()).toBeNull();

    await recordAndStop(store, 3, "c2");
    await store.patchTrip("c2", { problem: { step: "end", code: "bad_request", message: "x" } });
    expect((await store.getTrip("c2"))!.problem).not.toBeNull();
    await store.clearProblem("c2");
    expect((await store.getTrip("c2"))!.problem).toBeNull();
  });

  test("notes kept with the outbox are read back, replaced and removed", async () => {
    const { store } = await rig();
    expect(await store.getMeta("k")).toBeNull();
    await store.setMeta("k", "a");
    await store.setMeta("k", "b");
    expect(await store.getMeta("k")).toBe("b");
    await store.setMeta("k", null);
    expect(await store.getMeta("k")).toBeNull();
  });

  test("a fix of a phone without accuracy or speed is stored without them", async () => {
    const { store } = await rig();
    await start(store);
    await store.appendFixes("c1", [
      { t: T0, lat: 52.1, lon: 13.1, accuracyM: null, speedMps: -1 },
      { t: T0 + 1000, lat: 52.1, lon: 13.1, accuracyM: Number.NaN, speedMps: null },
    ]);
    const points = await store.points("c1");
    expect(points.map((p) => [p.accuracyM, p.speedMps])).toEqual([
      [null, null],
      [null, null],
    ]);
  });

  test("an impossible position is not stored", async () => {
    const { store } = await rig();
    await start(store);
    expect(
      await store.appendFixes("c1", [
        { t: T0, lat: 91, lon: 0, accuracyM: 1, speedMps: 0 },
        { t: T0 + 1, lat: Number.NaN, lon: 0, accuracyM: 1, speedMps: 0 },
        { t: T0 + 2, lat: 10, lon: 181, accuracyM: 1, speedMps: 0 },
      ]),
    ).toBe(0);
  });

  test("Stop fixes the counts, ends the recording, and a late fix is refused", async () => {
    const { store } = await rig();
    await start(store);
    await store.appendFixes("c1", walk(4));
    const stopped = await store.stopTrip("c1", new Date(T0 + 20_000).toISOString());
    expect(stopped).toMatchObject({ stopRequested: true, expectedPoints: 4, lastSeq: 4 });
    expect(await store.activeTrip()).toBeNull();
    expect(await store.appendFixes("c1", walk(1, 10))).toBe(0);
    // Stop twice changes nothing.
    expect(await store.stopTrip("c1", "2030-01-01T00:00:00.000Z")).toMatchObject({
      endedAt: stopped.endedAt,
    });
  });

  test("a batch keeps its range once it is made", async () => {
    const { store } = await rig();
    await start(store);
    await store.appendFixes("c1", walk(3));
    const first = await store.nextBatch("c1", 1000);
    expect(first).toMatchObject({ firstSeq: 1, lastSeq: 3 });
    await store.appendFixes("c1", walk(2, 3));
    // Not sent: the same range again, not a bigger one.
    expect(await store.nextBatch("c1", 1000)).toMatchObject({ firstSeq: 1, lastSeq: 3 });
    await store.markBatchSent(first!);
    expect(await store.nextBatch("c1", 1000)).toMatchObject({ firstSeq: 4, lastSeq: 5 });
  });

  test("clearAll leaves nothing of any trip", async () => {
    const { store } = await rig();
    await recordAndStop(store, 3, "a");
    await start(store, "b");
    await store.appendFixes("b", walk(2));
    await store.clearAll();
    expect(await store.listTrips()).toEqual([]);
    expect(await store.activeTrip()).toBeNull();
    expect(await store.pointCount("b")).toBe(0);
    await expect(start(store, "c")).resolves.toMatchObject({ clientTripId: "c" });
  });

  test("a database of a newer app is refused", async () => {
    const db = memoryDb();
    await db.exec("PRAGMA user_version = 99");
    await expect(migrate(db)).rejects.toThrow(/newer version/);
  });
});

describe("a trip that goes well", () => {
  test("the trip, the points, the end and the submit reach the server once each", async () => {
    const { store, server, engine } = await rig();
    await recordAndStop(store, 5);
    await store.requestSubmit("c1");

    const report = await engine.drain();

    expect(report).toMatchObject({ state: "idle", pending: 0, problems: 0 });
    const trip = server.stored("c1")!;
    expect(trip.state).toBe("in_review");
    expect(server.pointSeqs(trip.id)).toEqual([1, 2, 3, 4, 5]);
    expect(server.calls.filter((c) => c.key).map((c) => c.key)).toEqual([
      "trip:c1",
      "points:c1:1-5",
    ]);
    const local = (await store.getTrip("c1"))!;
    expect(local).toMatchObject({
      serverId: trip.id,
      endDone: true,
      submitDone: true,
      serverState: "in_review",
      distanceM: "1234.56",
    });
    expect(trip.expectedPoints).toBe(5);
    expect(trip.lastSeq).toBe(5);
    // The route is not kept once the server has it.
    expect(await store.pointCount("c1")).toBe(0);
  });

  test("points are sent while the trip is still recording, in batches of at most 1000", async () => {
    const { store, server, engine } = await rig();
    await start(store);
    await store.appendFixes("c1", walk(2500));
    await engine.drain();
    const id = server.stored("c1")!.id;
    expect(server.pointSeqs(id)).toHaveLength(2500);
    expect(server.calls.filter((c) => c.op === "createPoints").map((c) => c.key)).toEqual([
      "points:c1:1-1000",
      "points:c1:1001-2000",
      "points:c1:2001-2500",
    ]);
    expect(server.stored("c1")!.state).toBe("recording");
    // More points later go in their own batch.
    await store.appendFixes("c1", walk(10, 2500));
    await engine.drain();
    expect(server.pointSeqs(id)).toHaveLength(2510);
    expect(server.calls.filter((c) => c.op === "createPoints").at(-1)!.key).toBe(
      "points:c1:2501-2510",
    );
  });

  test("the points reach the server in the order and with the texts of the phone", async () => {
    const { store, server, engine } = await rig();
    await recordAndStop(store, 3);
    await engine.drain();
    const rows = [...server.points.get(server.stored("c1")!.id)!.values()];
    expect(rows[0]).toEqual({
      trip: "trip-1",
      seq: 1,
      recorded_at: "2026-10-15T09:00:00.000Z",
      lat: "52.0000000",
      lon: "13.0000000",
      accuracy_m: "5.00",
      speed_mps: "1.40",
    });
  });
});

describe("a connection that fails", () => {
  test("offline: nothing is lost and the next pass sends everything", async () => {
    const { store, server, engine } = await rig();
    await recordAndStop(store, 6);
    server.fail("createTrip", networkError(), 2);

    const first = await engine.drain();
    expect(first).toMatchObject({ state: "offline", pending: 1 });
    expect(first.retryInMs).toBe(1000);
    const second = await engine.drain();
    expect(second.state).toBe("offline");
    expect(second.retryInMs).toBe(2000); // backoff doubles
    expect(server.trips.size).toBe(0);
    expect(await store.pointCount("c1")).toBe(6);

    expect(await engine.drain()).toMatchObject({ state: "idle", pending: 0 });
    expect(server.stored("c1")!.state).toBe("ended");
  });

  test("the wait doubles with every failure and stops at the cap", async () => {
    const { store, server, engine } = await rig();
    await recordAndStop(store, 6);
    server.fail("createTrip", networkError(), 6);
    const waits: (number | null)[] = [];
    for (let i = 0; i < 6; i++) waits.push((await engine.drain()).retryInMs);
    expect(waits).toEqual([1000, 2000, 4000, 8000, 8000, 8000]);
    expect(await engine.drain()).toMatchObject({ state: "idle" }); // a pass that works resets it
  });

  test("a lost answer to the trip is repeated with the same key and makes one trip", async () => {
    const { store, server, engine } = await rig();
    await recordAndStop(store, 4);
    server.loseAnswer("createTrip");
    expect((await engine.drain()).state).toBe("offline");
    expect(server.trips.size).toBe(1);
    expect((await store.getTrip("c1"))!.serverId).toBeNull();
    expect((await store.getTrip("c1"))!.createAttempted).toBe(true);

    await engine.drain();
    expect(server.trips.size).toBe(1);
    expect(server.calls.filter((c) => c.op === "createTrip").map((c) => c.key)).toEqual([
      "trip:c1",
      "trip:c1",
    ]);
    expect(server.stored("c1")!.state).toBe("ended");
  });

  test("a lost answer to a batch is repeated with the same key and stores each point once", async () => {
    const { store, server, engine } = await rig();
    await recordAndStop(store, 8);
    server.loseAnswer("createPoints");
    expect((await engine.drain()).state).toBe("offline");
    // More points arrive before the repeat (a trip that is still recording): the range stays.
    await engine.drain();
    const keys = server.calls.filter((c) => c.op === "createPoints").map((c) => c.key);
    expect(keys).toEqual(["points:c1:1-8", "points:c1:1-8"]);
    expect(server.pointSeqs(server.stored("c1")!.id)).toEqual([1, 2, 3, 4, 5, 6, 7, 8]);
  });

  test("a batch that waits keeps its range when more points are recorded before the repeat", async () => {
    const { store, server, engine } = await rig();
    await start(store);
    await store.appendFixes("c1", walk(3));
    server.fail("createPoints", networkError());
    await engine.drain();
    await store.appendFixes("c1", walk(2, 3));
    await engine.drain();
    expect(server.calls.filter((c) => c.op === "createPoints").map((c) => c.key)).toEqual([
      "points:c1:1-3",
      "points:c1:1-3",
      "points:c1:4-5",
    ]);
    expect(server.pointSeqs(server.stored("c1")!.id)).toEqual([1, 2, 3, 4, 5]);
  });

  test("429: waits what Retry-After says and sends the same batch again", async () => {
    const { store, server, engine } = await rig();
    await recordAndStop(store, 5);
    server.fail("createPoints", apiError(429, { code: "rate_limited", retryAfter: 60 }));
    const report = await engine.drain();
    expect(report).toMatchObject({ state: "rate_limited", retryInMs: 60_000, pending: 1 });
    expect(await engine.drain()).toMatchObject({ state: "idle" });
    expect(server.pointSeqs(server.stored("c1")!.id)).toHaveLength(5);
  });

  test("503 with applied none and with applied unknown: repeated with the same key", async () => {
    const { store, server, engine } = await rig();
    await recordAndStop(store, 5);
    server.fail("createPoints", apiError(503, { applied: "none", retryAfter: 3 }));
    expect(await engine.drain()).toMatchObject({ state: "unavailable", retryInMs: 3000 });
    server.loseAnswer("createPoints", apiError(503, { applied: "unknown" }));
    expect((await engine.drain()).state).toBe("unavailable");
    expect(await engine.drain()).toMatchObject({ state: "idle" });
    expect(server.calls.filter((c) => c.op === "createPoints").map((c) => c.key)).toEqual([
      "points:c1:1-5",
      "points:c1:1-5",
      "points:c1:1-5",
    ]);
    expect(server.pointSeqs(server.stored("c1")!.id)).toHaveLength(5);
  });

  test("401: stops, keeps everything, and goes on after a new sign-in", async () => {
    const { store, server, engine } = await rig();
    await recordAndStop(store, 5);
    server.fail("createTrip", apiError(401, { message: "expired" }));
    const report = await engine.drain();
    expect(report).toMatchObject({ state: "signed_out", pending: 1, retryInMs: null });
    expect(await store.pointCount("c1")).toBe(5);
    expect(await engine.drain()).toMatchObject({ state: "idle" });
  });
});

describe("a trip that the server discarded or no longer has", () => {
  test("points refused because the server discarded the trip finish the trip on the phone", async () => {
    const { store, server, engine } = await rig();
    await start(store);
    await engine.drain(); // the trip exists on the server
    await store.appendFixes("c1", walk(7));
    await store.stopTrip("c1", new Date(T0 + 40_000).toISOString());
    server.stored("c1")!.state = "discarded";
    const report = await engine.drain();
    expect(report).toMatchObject({ pending: 0, problems: 0 });
    expect(await store.pointCount("c1")).toBe(0);
    expect(await store.getTrip("c1")).toMatchObject({
      discardDone: true,
      serverState: "discarded",
      problem: null,
    });
    expect(hasWork((await store.getTrip("c1"))!)).toBe(false);
  });

  test("a trip that is still recording when the server discards it is flagged and stays recording", async () => {
    const { store, server, engine } = await rig();
    await start(store);
    await engine.drain();
    await store.appendFixes("c1", walk(3));
    server.stored("c1")!.state = "discarded";
    await engine.drain();
    const trip = (await store.getTrip("c1"))!;
    expect(trip.problem).toMatchObject({ step: "points", code: "discarded" });
    expect(await store.activeTrip()).not.toBeNull();
    expect(await store.pointCount("c1")).toBe(3); // the route is not thrown away under the driver
  });

  test("a trip the server lost stops with the code gone, and the driver can discard it here", async () => {
    const { store, server, engine } = await rig();
    await recordAndStop(store, 6);
    await engine.drain(); // sends the points; one point more than the server can end without
    const sent = (await store.getTrip("c1"))!;
    await store.patchTrip("c1", { endDone: false, problem: null });
    server.trips.delete(sent.serverId!); // the instance was restored from an older backup
    await store.appendFixes("c1", []); // no change; the points are sent already
    await engine.drain();
    const stuck = (await store.getTrip("c1"))!;
    expect(stuck.problem).toMatchObject({ step: "end", code: "gone" });
    expect(canDiscard(stuck)).toBe(true);
    await store.requestDiscard("c1", "lost on the server", new Date(T0 + 99_000).toISOString());
    await store.clearProblem("c1");
    expect(await engine.drain()).toMatchObject({ pending: 0, problems: 0 });
    expect(await store.getTrip("c1")).toMatchObject({ discardDone: true, problem: null });
    expect(await store.pointCount("c1")).toBe(0);
  });

  test("points of a trip the server lost are kept until the driver discards", async () => {
    const { store, server, engine } = await rig();
    await start(store);
    await engine.drain();
    const id = (await store.getTrip("c1"))!.serverId!;
    await store.appendFixes("c1", walk(4));
    server.trips.delete(id);
    await engine.drain();
    const stuck = (await store.getTrip("c1"))!;
    expect(stuck.problem).toMatchObject({ step: "points", code: "gone" });
    expect(await store.pointCount("c1")).toBe(4); // nothing is deleted without the driver's word
    expect(canDiscard(stuck)).toBe(true);
  });

  test("a route that a kill left behind after the end is removed at the first pass", async () => {
    const { store, engine } = await rig();
    await recordAndStop(store, 5);
    await engine.drain();
    await store.appendFixes("c1", []); // ended and purged by the engine
    // Simulate the older two-step write: the trip says ended, the points are still there.
    const trip = (await store.getTrip("c1"))!;
    expect(trip.endDone).toBe(true);
    const { engine: second, store: store2 } = await rig();
    await recordAndStop(store2, 5);
    await store2.patchTrip("c1", { endDone: true });
    expect(await store2.pointCount("c1")).toBe(5);
    await second.drain();
    expect(await store2.pointCount("c1")).toBe(0);
    expect(engine).toBeDefined();
  });
});

describe("a refusal that is not a missing trip", () => {
  /** A 404 as the server sends it for a command that is hidden from this person. */
  const hidden = () => apiError(404, { message: "not found" });

  test("a 404 on a trip that exists keeps every point and does not say the trip is gone", async () => {
    const { store, server, engine } = await rig();
    await start(store);
    await engine.drain();
    await store.appendFixes("c1", walk(4));
    server.fail("createPoints", hidden(), 1);
    server.fail("getTrip", hidden(), 1);
    await engine.drain();
    const stuck = (await store.getTrip("c1"))!;
    expect(stuck.problem).toMatchObject({ step: "points", code: "unknown" });
    expect(stuck.problem?.code).not.toBe("gone");
    expect(await store.pointCount("c1")).toBe(4);
    // The driver cannot throw the points away on the strength of a state the server did not give.
    expect(canDiscard(stuck)).toBe(false);
    // Nothing is deleted, and once the server answers again the trip goes on.
    await store.clearProblem("c1");
    await engine.drain();
    expect((await store.getTrip("c1"))!.problem).toBeNull();
    expect(server.stored("c1")!.state).toBe("recording");
    expect(server.pointSeqs(server.stored("c1")!.id)).toHaveLength(4);
  });

  test("only the plain 400 of the module means that the server does not have the trip", async () => {
    const { store, server, engine } = await rig();
    await recordAndStop(store, 6);
    await engine.drain();
    await store.patchTrip("c1", { endDone: false, problem: null });
    server.fail("getTrip", apiError(400, { message: "no mileage_trip 01ab" }), 1);
    await engine.drain();
    expect((await store.getTrip("c1"))!.problem).toMatchObject({ code: "gone" });
    // Other 400 words are an ordinary refusal.
    await store.clearProblem("c1");
    server.fail("getTrip", apiError(400, { message: "the trip is odd" }), 1);
    await engine.drain();
    expect((await store.getTrip("c1"))!.problem).toMatchObject({ code: "bad_request" });
  });

  test("a trip whose create was refused can be discarded: the phone has nothing to discard on the server", async () => {
    const { store, server, engine } = await rig();
    await recordAndStop(store, 4);
    // The person lost the group: every call of mileage is refused, the question for the trip too.
    server.fail("createTrip", apiError(403, { message: "forbidden" }), 5);
    server.fail("findTrip", apiError(403, { message: "forbidden" }), 5);
    await engine.drain();
    const stuck = (await store.getTrip("c1"))!;
    expect(stuck.problem).toMatchObject({ step: "create" });
    expect(canDiscard(stuck)).toBe(true);
    await store.requestDiscard("c1", "wrong group", new Date(T0 + 99_000).toISOString());
    expect(await engine.drain()).toMatchObject({ pending: 0, problems: 0 });
    expect(await store.getTrip("c1")).toMatchObject({
      discardDone: true,
      serverState: "discarded",
      problem: null,
    });
    expect(await store.pointCount("c1")).toBe(0);
  });

  test("a trip whose create was refused but that the server has is discarded there", async () => {
    const { store, server, engine } = await rig();
    await recordAndStop(store, 4);
    // The create ran and the answer was lost, then calls were refused for a while.
    server.loseAnswer("createTrip");
    await engine.drain();
    expect((await store.getTrip("c1"))!.serverId).toBeNull();
    await store.requestDiscard("c1", "mine", new Date(T0 + 99_000).toISOString());
    expect(await engine.drain()).toMatchObject({ pending: 0 });
    expect(server.stored("c1")!.state).toBe("discarded");
    expect(await store.pointCount("c1")).toBe(0);
  });

  test("a discard waits while the server cannot be reached", async () => {
    const { store, server, engine } = await rig();
    await recordAndStop(store, 4);
    server.fail("createTrip", apiError(403, { message: "forbidden" }), 1);
    server.fail("findTrip", apiError(403, { message: "forbidden" }), 1);
    await engine.drain();
    await store.requestDiscard("c1", "x", new Date(T0 + 99_000).toISOString());
    server.fail("createTrip", networkError(), 1);
    expect((await engine.drain()).state).toBe("offline");
    expect((await store.getTrip("c1"))!.discardDone).toBe(false);
    expect(await store.pointCount("c1")).toBe(4);
    expect(await engine.drain()).toMatchObject({ pending: 0 });
    expect((await store.getTrip("c1"))!.discardDone).toBe(true);
  });
});

/** An api that runs `after` once the first answer of `op` is in, then hands the answer on. */
function afterAnswer(server: FakeServer, op: string, after: () => void): FakeServer {
  let done = false;
  return new Proxy(server, {
    get(target, key, receiver) {
      const value = Reflect.get(target, key, receiver) as unknown;
      if (key !== op || typeof value !== "function") return value;
      return async (...args: unknown[]) => {
        const answer = await (value as (...a: unknown[]) => Promise<unknown>).apply(target, args);
        if (!done) after();
        done = true;
        return answer;
      };
    },
  });
}

/**
 * An api whose `op` has read its answer from the server but does not hand it on until `release()`:
 * a late answer. `hold()` arms it for the next call, `reached()` resolves when that call is waiting.
 */
function holdAnswer(server: FakeServer, op: string) {
  let armed = false;
  let release: () => void = () => undefined;
  let reached: () => void = () => undefined;
  let waiting = Promise.resolve();
  const api = new Proxy(server, {
    get(target, key, receiver) {
      const value = Reflect.get(target, key, receiver) as unknown;
      if (key !== op || typeof value !== "function") return value;
      return async (...args: unknown[]) => {
        const answer = await (value as (...a: unknown[]) => Promise<unknown>).apply(target, args);
        if (!armed) return answer;
        armed = false;
        reached();
        await new Promise<void>((resolve) => {
          release = resolve;
        });
        return answer;
      };
    },
  });
  return {
    api,
    hold() {
      armed = true;
      waiting = new Promise<void>((resolve) => {
        reached = resolve;
      });
    },
    reached: () => waiting,
    release: () => release(),
  };
}

/** A trip of another phone, as the server lists it. */
function otherTrip() {
  return {
    id: "other-1",
    clientTripId: "elsewhere",
    purpose: "private" as const,
    startedAt: "2026-10-01T08:00:00.000000Z",
    endedAt: "2026-10-01T08:30:00.000000Z",
    state: "confirmed" as const,
    expectedPoints: 9,
    lastSeq: 9,
    discardReason: null,
    distanceM: "5000.00",
  };
}

describe("a sign-in that changes while a pass runs", () => {
  test("the rest of the pass is not sent under the new sign-in", async () => {
    const db = memoryDb();
    await migrate(db);
    const store = createSqlStore(db);
    const server = new FakeServer();
    let generation = 1;
    let flipped = false;
    // The driver signs out (and somebody else signs in) while the first trip is being created.
    const api = new Proxy(server, {
      get(target, key, receiver) {
        const value = Reflect.get(target, key, receiver) as unknown;
        if (key !== "createTrip" || typeof value !== "function") return value;
        return async (...args: unknown[]) => {
          const answer = await (value as (...a: unknown[]) => Promise<unknown>).apply(target, args);
          if (!flipped) generation += 1;
          flipped = true;
          return answer;
        };
      },
    });
    const engine = createEngine({ store, api, identity: () => generation });
    await recordAndStop(store, 3, "a");
    await recordAndStop(store, 3, "b");
    const report = await engine.drain();
    expect(report.state).toBe("interrupted");
    // The first trip's create went out (it was in flight); nothing after it did.
    expect(server.calls.map((c) => c.op)).toEqual(["createTrip"]);
    expect(server.stored("b")).toBeUndefined();
    // A pass under the new sign-in starts from the beginning and sends the rest.
    const next = await engine.drain();
    expect(next.state).not.toBe("interrupted");
    expect(server.stored("b")).toBeDefined();
  });

  test("a pass that nothing interrupts reports as before", async () => {
    const db = memoryDb();
    await migrate(db);
    const store = createSqlStore(db);
    const server = new FakeServer();
    const engine = createEngine({ store, api: server, identity: () => 1 });
    await recordAndStop(store, 3);
    expect((await engine.drain()).state).toBe("idle");
  });

  test("a sign-in that changes during the last call of a trip stops the pass before the next trip is touched", async () => {
    const db = memoryDb();
    await migrate(db);
    const store = createSqlStore(db);
    const server = new FakeServer();
    let generation = 1;
    // The driver signs out while the end of the first trip is on its way: it is the last call that
    // trip makes, so the next call to meet the new sign-in would be the second trip's create.
    const api = afterAnswer(server, "endTrip", () => (generation += 1));
    const engine = createEngine({ store, api, identity: () => generation });
    await recordAndStop(store, 3, "a");
    await recordAndStop(store, 3, "b");
    const report = await engine.drain();
    expect(report.state).toBe("interrupted");
    expect((await store.getTrip("a"))!.endDone).toBe(true);
    // Nothing of the second trip was touched, not even the mark that a create was tried.
    expect((await store.getTrip("b"))!.createAttempted).toBe(false);
    expect(server.stored("b")).toBeUndefined();
  });

  test("a list that was asked for under one sign-in is not merged under the next", async () => {
    const db = memoryDb();
    await migrate(db);
    const store = createSqlStore(db);
    const server = new FakeServer();
    let generation = 1;
    const held = holdAnswer(server, "listTrips");
    const engine = createEngine({ store, api: held.api, identity: () => generation });
    server.trips.set("other-1", otherTrip());
    held.hold();
    const listing = engine.refreshRemote();
    await held.reached();
    // The driver signs out and somebody else signs in while the instance is still answering.
    generation += 1;
    held.release();
    await listing;
    expect(await store.listTrips()).toEqual([]);
  });

  test("a list under an unchanged sign-in is merged", async () => {
    const db = memoryDb();
    await migrate(db);
    const store = createSqlStore(db);
    const server = new FakeServer();
    const held = holdAnswer(server, "listTrips");
    const engine = createEngine({ store, api: held.api, identity: () => 1 });
    server.trips.set("other-1", otherTrip());
    held.hold();
    const listing = engine.refreshRemote();
    await held.reached();
    held.release();
    await listing;
    expect((await store.listTrips()).map((t) => t.clientTripId)).toEqual(["elsewhere"]);
  });

  test("the answer for one trip that was asked for under one sign-in is not written under the next", async () => {
    const db = memoryDb();
    await migrate(db);
    const store = createSqlStore(db);
    const server = new FakeServer();
    let generation = 1;
    const held = holdAnswer(server, "getTrip");
    const engine = createEngine({ store, api: held.api, identity: () => generation });
    await recordAndStop(store, 3);
    await engine.drain();
    const before = (await store.getTrip("c1"))!.serverState;
    server.stored("c1")!.state = "rejected";
    held.hold();
    const refreshing = engine.refreshTrip("c1");
    await held.reached();
    generation += 1;
    held.release();
    await refreshing;
    expect((await store.getTrip("c1"))!.serverState).toBe(before);

    // Under the same sign-in the answer is written.
    await engine.refreshTrip("c1");
    expect((await store.getTrip("c1"))!.serverState).toBe("rejected");
  });
});

describe("Stop", () => {
  test("end is repeated until the points the server is missing are there", async () => {
    const { store, server, engine } = await rig();
    await recordAndStop(store, 5);
    server.fail("endTrip", apiError(409, { code: "incomplete", message: "4 of 5 points" }), 2);

    const first = await engine.drain();
    expect(first).toMatchObject({ pending: 1, problems: 0 });
    expect(first.retryInMs).toBe(1000 * 4); // no Retry-After on this answer: the engine's own wait
    expect((await store.getTrip("c1"))!.attempts).toBe(1);
    const second = await engine.drain();
    expect(second.retryInMs).toBe(8000); // and it grows with every answer, up to the cap
    expect((await store.getTrip("c1"))!.attempts).toBe(2);
    expect(await engine.drain()).toMatchObject({ state: "idle" });
    expect(server.stored("c1")!.state).toBe("ended");
    expect((await store.getTrip("c1"))!).toMatchObject({
      endDone: true,
      attempts: 0,
      problem: null,
    });
  });

  test("with the production base of 2 s the waits for an incomplete end are 8 s, 16 s, 32 s and so on, up to 5 minutes", async () => {
    // The engine as native/runtime.ts builds it: no backoff option.
    const db = memoryDb();
    await migrate(db);
    const store = createSqlStore(db);
    const server = new FakeServer();
    const engine = createEngine({ store, api: server });
    await recordAndStop(store, 5);
    server.fail("endTrip", apiError(409, { code: "incomplete", message: "x" }), 9);
    const waits: Array<number | null> = [];
    for (let i = 0; i < 9; i++) waits.push((await engine.drain()).retryInMs);
    expect(waits).toEqual([8000, 16000, 32000, 64000, 128000, 256000, 300_000, 300_000, 300_000]);
  });

  test("with a base of 1 s the waits for an incomplete end are 4 s, 8 s, 16 s, then the cap", async () => {
    const { store, server, engine } = await rig({ baseMs: 1000, maxMs: 20_000 });
    await recordAndStop(store, 5);
    server.fail("endTrip", apiError(409, { code: "incomplete", message: "4 of 5 points" }), 5);
    const waits: Array<number | null> = [];
    for (let i = 0; i < 5; i++) waits.push((await engine.drain()).retryInMs);
    expect(waits).toEqual([4000, 8000, 16000, 20_000, 20_000]);
  });

  test("a Retry-After of the server beats the engine's own wait on an incomplete end", async () => {
    const { store, server, engine } = await rig();
    await recordAndStop(store, 5);
    server.fail("endTrip", apiError(409, { code: "incomplete", message: "x", retryAfter: 3 }), 1);
    expect((await engine.drain()).retryInMs).toBe(3000);
  });

  test("the driver is told after five incomplete answers and the outbox keeps asking", async () => {
    const { store, server, engine } = await rig();
    await recordAndStop(store, 5);
    server.fail("endTrip", apiError(409, { code: "incomplete", message: "4 of 5 points" }), 6);
    for (let i = 0; i < 5; i++) await engine.drain();
    const noticed = (await store.getTrip("c1"))!;
    expect(noticed.problem).toMatchObject({ step: "end", code: "incomplete" });
    expect(hasWork(noticed)).toBe(true);
    await engine.drain();
    await engine.drain();
    expect(server.stored("c1")!.state).toBe("ended");
    expect((await store.getTrip("c1"))!.problem).toBeNull();
  });

  test("a refusal for good is shown and stops the trip until the driver acts", async () => {
    const { store, server, engine } = await rig();
    await recordAndStop(store, 1); // one point: no distance
    const report = await engine.drain();
    expect(report).toMatchObject({ pending: 0, problems: 1 });
    const trip = (await store.getTrip("c1"))!;
    expect(trip.problem).toMatchObject({ step: "end", code: "bad_request" });
    expect(trip.problem!.message).toContain("no distance");
    const calls = server.calls.length;
    await engine.drain();
    expect(server.calls.length).toBe(calls); // nothing more is sent
    // The driver discards the trip: that goes through.
    await store.requestDiscard("c1", "Test drive, nothing recorded", new Date(T0).toISOString());
    expect(await engine.drain()).toMatchObject({ state: "idle", problems: 0 });
    expect(server.stored("c1")!.state).toBe("discarded");
    expect(server.stored("c1")!.discardReason).toBe("Test drive, nothing recorded");
  });

  test("a repeat of end after its answer was lost finds the trip ended", async () => {
    const { store, server, engine } = await rig();
    await recordAndStop(store, 5);
    server.loseAnswer("endTrip");
    expect((await engine.drain()).state).toBe("offline");
    expect(server.stored("c1")!.state).toBe("ended");
    expect(await engine.drain()).toMatchObject({ state: "idle" });
    expect(server.count("endTrip")).toBe(1); // read, found ended, not sent again
    expect((await store.getTrip("c1"))!).toMatchObject({ endDone: true, distanceM: "1234.56" });
  });
});

describe("Submit", () => {
  test("a repeat of submit after its answer was lost does not send it again", async () => {
    const { store, server, engine } = await rig();
    await recordAndStop(store, 5);
    await engine.drain();
    await store.requestSubmit("c1");
    server.loseAnswer("submitTrip");
    expect((await engine.drain()).state).toBe("offline");
    expect(await engine.drain()).toMatchObject({ state: "idle" });
    expect(server.count("submitTrip")).toBe(1);
    expect((await store.getTrip("c1"))!).toMatchObject({
      submitDone: true,
      serverState: "in_review",
    });
  });

  test("a submit the server refuses is shown with the server's words", async () => {
    const { store, server, engine } = await rig();
    await recordAndStop(store, 5);
    await engine.drain();
    server.stored("c1")!.state = "rejected";
    await store.requestSubmit("c1");
    await engine.drain();
    const trip = (await store.getTrip("c1"))!;
    expect(trip.problem).toMatchObject({ step: "submit", code: "state_conflict" });
    expect(trip.problem!.message).toContain("rejected");
  });
});

describe("Discard", () => {
  test("a trip the server never heard of is dropped on the phone alone", async () => {
    const { store, server, engine } = await rig();
    await start(store);
    await store.appendFixes("c1", walk(5));
    await store.requestDiscard("c1", "Started by mistake", new Date(T0 + 30_000).toISOString());
    expect(await engine.drain()).toMatchObject({ state: "idle", pending: 0 });
    expect(server.calls).toHaveLength(0);
    expect(await store.pointCount("c1")).toBe(0);
    expect(await store.activeTrip()).toBeNull();
    expect((await store.getTrip("c1"))!).toMatchObject({
      discardDone: true,
      serverState: "discarded",
    });
  });

  test("a trip whose creation may have reached the server is discarded there too", async () => {
    const { store, server, engine } = await rig();
    await start(store);
    await store.appendFixes("c1", walk(5));
    server.loseAnswer("createTrip");
    await engine.drain(); // the trip is on the server, the phone does not know
    expect(server.trips.size).toBe(1);
    await store.requestDiscard("c1", "Wrong vehicle", new Date(T0 + 30_000).toISOString());
    await engine.drain();
    expect(server.trips.size).toBe(1);
    expect(server.stored("c1")!.state).toBe("discarded");
    expect(server.stored("c1")!.discardReason).toBe("Wrong vehicle");
    // The key was the same, so the lost answer was found again and not a second trip made.
    expect(server.calls.filter((c) => c.op === "createTrip").map((c) => c.key)).toEqual([
      "trip:c1",
      "trip:c1",
    ]);
  });

  test("an ended trip is discarded with its reason", async () => {
    const { store, server, engine } = await rig();
    await recordAndStop(store, 5);
    await engine.drain();
    expect(server.stored("c1")!.state).toBe("ended");
    await store.requestDiscard("c1", "Private errand", new Date(T0 + 99_000).toISOString());
    await engine.drain();
    expect(server.stored("c1")!.state).toBe("discarded");
    expect((await store.getTrip("c1"))!.discardDone).toBe(true);
  });

  test("a trip that is in review cannot be discarded: the driver is told", async () => {
    const { store, server, engine } = await rig();
    await recordAndStop(store, 5);
    await store.requestSubmit("c1");
    await engine.drain();
    await store.requestDiscard("c1", "Changed my mind", new Date(T0 + 99_000).toISOString());
    await engine.drain();
    expect(server.stored("c1")!.state).toBe("in_review");
    expect((await store.getTrip("c1"))!.problem).toMatchObject({
      step: "discard",
      code: "state_conflict",
    });
  });
});

describe("a key the server has forgotten (more than 24 hours)", () => {
  test("the trip is found by its client id and the points the server holds count as stored", async () => {
    const { store, server, engine } = await rig();
    await recordAndStop(store, 5);
    server.loseAnswer("createTrip");
    await engine.drain();
    server.forgetKeys();
    // Now: createTrip is refused (not unique), findTrip gives the trip; the points go in.
    server.loseAnswer("createPoints");
    await engine.drain();
    server.forgetKeys();
    // The batch is refused (not unique) but the server holds all five points.
    await engine.drain();
    expect(server.trips.size).toBe(1);
    expect(server.stored("c1")!.state).toBe("ended");
    expect(server.pointSeqs(server.stored("c1")!.id)).toEqual([1, 2, 3, 4, 5]);
    expect(server.count("findTrip")).toBeGreaterThan(0);
  });

  test("a batch the server refuses and does not hold is a problem, not a loop", async () => {
    const { store, server, engine } = await rig();
    await recordAndStop(store, 5);
    server.fail("createPoints", apiError(400, { message: "the points are not valid" }));
    const report = await engine.drain();
    expect(report).toMatchObject({ problems: 1, pending: 0 });
    expect((await store.getTrip("c1"))!.problem).toMatchObject({ step: "points" });
  });
});

describe("restarts", () => {
  test("a new engine over the same database goes on where the old one stopped", async () => {
    const { db, store, server } = await rig();
    await recordAndStop(store, 5);
    const first = createEngine({ store, api: server });
    server.fail("endTrip", networkError());
    expect((await first.drain()).state).toBe("offline");
    // The app was killed. A new process opens the same database.
    const second = createEngine({ store: createSqlStore(db), api: server });
    expect(await second.drain()).toMatchObject({ state: "idle", pending: 0 });
    expect(server.stored("c1")!.state).toBe("ended");
    expect(server.trips.size).toBe(1);
    expect(server.pointSeqs(server.stored("c1")!.id)).toHaveLength(5);
  });

  test("two passes at once make one pass", async () => {
    const { store, server, engine } = await rig();
    await recordAndStop(store, 5);
    const [a, b] = await Promise.all([engine.drain(), engine.drain()]);
    expect(a.state).toBe("idle");
    expect(b.state).toBe("idle");
    expect(server.trips.size).toBe(1);
    expect(server.count("createPoints")).toBe(1);
  });

  test("two trips are sent in the order they were made", async () => {
    const { store, server, engine } = await rig();
    await recordAndStop(store, 5, "first");
    await store.startTrip({
      clientTripId: "second",
      purpose: "commute",
      startedAt: new Date(T0 + 1e6).toISOString(),
      nowMs: T0 + 1e6,
    });
    await store.appendFixes("second", walk(4, 500));
    await store.stopTrip("second", new Date(T0 + 2e6).toISOString());
    await engine.drain();
    expect(server.calls.filter((c) => c.op === "createTrip").map((c) => c.key)).toEqual([
      "trip:first",
      "trip:second",
    ]);
    expect(server.stored("second")!.purpose).toBe("commute");
  });
});

describe("trips the server lists", () => {
  test("a trip from another phone is shown, and one the phone knows keeps what it still has to send", async () => {
    const { store, server, engine } = await rig();
    await recordAndStop(store, 5);
    await engine.drain();
    server.stored("c1")!.state = "rejected";
    server.trips.set("other-1", {
      id: "other-1",
      clientTripId: "elsewhere",
      purpose: "private",
      startedAt: "2026-10-01T08:00:00.000000Z",
      endedAt: "2026-10-01T08:30:00.000000Z",
      state: "confirmed",
      expectedPoints: 9,
      lastSeq: 9,
      discardReason: null,
      distanceM: "5000.00",
    });
    await engine.refreshRemote();
    const trips = await store.listTrips();
    expect(trips.map((t) => [t.clientTripId, t.origin, t.serverState])).toEqual([
      ["c1", "device", "rejected"],
      ["elsewhere", "server", "confirmed"],
    ]);
    expect(trips[1]).toMatchObject({ distanceM: "5000.00", submitDone: true, endDone: true });
    expect(hasWork(trips[1]!)).toBe(false);
  });
});
