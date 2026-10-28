import { describe, expect, test } from "vite-plus/test";
import type { DrainReport, Engine } from "./engine.ts";
import { createSyncController } from "./sync.ts";

const report = (over: Partial<DrainReport> = {}): DrainReport => ({
  state: "ok",
  pending: 0,
  problems: 0,
  retryInMs: null,
  ...over,
});

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
    idleMs: 60_000,
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
      report({ state: "rate_limited", pending: 1, retryInMs: 60_000 }),
      report({ state: "idle" }),
    ]);
    r.sync.kick();
    await r.settle();
    expect(r.live().map((t) => t.ms)).toEqual([60_000]);
    const due = r.live()[0]!;
    due.live = false;
    due.fn();
    await r.settle();
    expect(r.drains()).toBe(2);
    expect(r.live()).toHaveLength(0);
  });

  test("a recording trip with nothing to wait for is looked at every minute", async () => {
    const r = rig([report({ pending: 1, retryInMs: null })]);
    r.sync.kick();
    await r.settle();
    expect(r.live().map((t) => t.ms)).toEqual([60_000]);
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
