import { describe, expect, test } from "vite-plus/test";
import { createEngine, type DrainReport, type Engine } from "./engine.ts";
import { createSqlStore, migrate } from "./sql-store.ts";
import { createSyncController } from "./sync.ts";
import { apiError, FakeServer } from "../test-support/fake-server.ts";
import { memoryDb } from "../test-support/memory-db.ts";

const report = (over: Partial<DrainReport> = {}): DrainReport => ({
  state: "ok",
  pending: 0,
  problems: 0,
  retryInMs: null,
  ...over,
});

/** Differs from every wait below, so a timer cannot be right by accident. */
const IDLE_MS = 90_000;

function rig(reports: DrainReport[]) {
  const timers: { fn: () => void; ms: number; live: boolean }[] = [];
  let drains = 0;
  let signedIn = true;
  let rejected = 0;
  const engine: Engine = {
    drain: async () => reports[Math.min(drains++, reports.length - 1)]!,
    refreshTrip: async () => {},
    refreshRemote: async () => {},
  };
  const sync = createSyncController({
    engine,
    signedIn: () => signedIn,
    onRejected: async () => {
      rejected++;
    },
    idleMs: IDLE_MS,
    timers: {
      set: (fn, ms) => {
        const t = { fn, ms, live: true };
        timers.push(t);
        return t;
      },
      clear: (h) => {
        (h as { live: boolean }).live = false;
      },
    },
  });
  const live = () => timers.filter((t) => t.live);
  const settle = () => new Promise((resolve) => setTimeout(resolve, 0));
  return {
    sync,
    live,
    settle,
    drains: () => drains,
    rejected: () => rejected,
    signOut: () => (signedIn = false),
  };
}

describe("the sync controller", () => {
  test("a pass with nothing left schedules nothing", async () => {
    const r = rig([report({ state: "idle" })]);
    r.sync.kick();
    await r.settle();
    expect(r.drains()).toBe(1);
    expect(r.live()).toHaveLength(0);
    expect(r.sync.status().report?.state).toBe("idle");
  });

  test("a trip that waits is tried again after the wait the engine asked for", async () => {
    const r = rig([
      report({ state: "rate_limited", pending: 1, retryInMs: 3_000 }),
      report({ state: "idle" }),
    ]);
    r.sync.kick();
    await r.settle();
    expect(r.live().map((t) => t.ms)).toEqual([3_000]);
    const due = r.live()[0]!;
    due.live = false;
    due.fn();
    await r.settle();
    expect(r.drains()).toBe(2);
    expect(r.live()).toHaveLength(0);
  });

  test("a recording trip with nothing to wait for is looked at on the idle interval", async () => {
    const r = rig([report({ pending: 1, retryInMs: null })]);
    r.sync.kick();
    await r.settle();
    expect(r.live().map((t) => t.ms)).toEqual([IDLE_MS]);
  });

  test("signed out: the token is dropped once and the pass is tried again, then it stops", async () => {
    const r = rig([report({ state: "signed_out", pending: 1 })]);
    r.sync.kick();
    await r.settle();
    expect(r.rejected()).toBe(1);
    expect(r.drains()).toBe(2);
    expect(r.live()).toHaveLength(0);
    expect(r.sync.status().report?.state).toBe("signed_out");
  });

  test("a kick during a pass gets one more pass", async () => {
    const r = rig([report({ state: "idle" })]);
    r.sync.kick();
    r.sync.kick();
    await r.settle();
    await r.settle();
    expect(r.drains()).toBe(2);
  });

  test("nobody signed in: no pass", async () => {
    const r = rig([report()]);
    r.signOut();
    r.sync.kick();
    await r.settle();
    expect(r.drains()).toBe(0);
  });

  test("stop clears the timer", async () => {
    const r = rig([report({ pending: 1 })]);
    r.sync.kick();
    await r.settle();
    r.sync.stop();
    expect(r.live()).toHaveLength(0);
  });

  test("a broken pass is reported and tried again in 30 seconds", async () => {
    const timers: { fn: () => void; ms: number }[] = [];
    const sync = createSyncController({
      engine: {
        drain: async () => {
          throw new Error("disk full");
        },
        refreshTrip: async () => {},
        refreshRemote: async () => {},
      },
      signedIn: () => true,
      onRejected: async () => {},
      timers: { set: (fn, ms) => timers.push({ fn, ms }), clear: () => {} },
    });
    sync.kick();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(sync.status().error).toBe("disk full");
    expect(timers.map((t) => t.ms)).toEqual([30_000]);
  });
});

describe("the controller over the real engine", () => {
  test("the wait the server asks for with Retry-After reaches the timer", async () => {
    const db = memoryDb();
    await migrate(db);
    const store = createSqlStore(db);
    const server = new FakeServer();
    const engine = createEngine({ store, api: server, backoff: { baseMs: 1000, maxMs: 8000 } });
    const T0 = Date.parse("2026-10-15T09:00:00Z");
    await store.startTrip({
      clientTripId: "c1",
      purpose: "business",
      startedAt: new Date(T0).toISOString(),
      nowMs: T0,
    });
    await store.appendFixes("c1", [
      { t: T0 + 5000, lat: 52, lon: 13, accuracyM: 5, speedMps: 1.4 },
      { t: T0 + 10_000, lat: 52.0001, lon: 13, accuracyM: 5, speedMps: 1.4 },
    ]);
    server.fail("createPoints", apiError(503, { retryAfter: 3 }));

    const timers: { fn: () => void; ms: number; live: boolean }[] = [];
    const sync = createSyncController({
      engine,
      signedIn: () => true,
      onRejected: async () => {},
      idleMs: IDLE_MS,
      timers: {
        set: (fn, ms) => {
          const t = { fn, ms, live: true };
          timers.push(t);
          return t;
        },
        clear: (h) => {
          (h as { live: boolean }).live = false;
        },
      },
    });
    sync.kick();
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(sync.status().report?.state).toBe("unavailable");
    expect(timers.filter((t) => t.live).map((t) => t.ms)).toEqual([3000]);
  });
});
