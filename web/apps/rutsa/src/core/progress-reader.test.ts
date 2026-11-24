import { describe, expect, test } from "vite-plus/test";
import { READ_EVERY_MS, startProgressReader } from "./progress-reader.ts";

/** A clock, timers and a store the test moves by hand. */
function rig(options: { fail?: boolean } = {}) {
  let now = 0;
  let nextTimer = 0;
  const timers = new Map<number, { at: number; run: () => void }>();
  const listeners = new Set<() => void>();
  const reads: number[] = [];
  const got: string[] = [];
  let value = 0;
  const flush = () => new Promise<void>((resolve) => setTimeout(resolve, 0));
  return {
    reads,
    got,
    listeners,
    timers,
    change: () => {
      value += 1;
      listeners.forEach((l) => l());
    },
    advance: async (ms: number) => {
      now += ms;
      // A Map may lose the entry it is at while it is walked; a timer due later stays.
      for (const [id, timer] of timers) {
        if (timer.at <= now) {
          timers.delete(id);
          timer.run();
        }
      }
      await flush();
    },
    flush,
    start: (id: string | null = "t1") =>
      startProgressReader<string>({
        id,
        read: async (tripId) => {
          reads.push(now);
          if (options.fail) throw new Error("database is locked");
          return `${tripId}:${value}`;
        },
        subscribe: (listener) => {
          listeners.add(listener);
          return () => listeners.delete(listener);
        },
        clock: () => now,
        set: (v) => got.push(v),
        setTimeout: (run, ms) => {
          nextTimer += 1;
          timers.set(nextTimer, { at: now + ms, run });
          return nextTimer;
        },
        clearTimeout: (timer) => timers.delete(timer as number),
      }),
  };
}

describe("the count of the open trip", () => {
  test("is read at once", async () => {
    const r = rig();
    r.start();
    await r.flush();
    expect(r.got).toEqual(["t1:0"]);
  });

  test("a change right after a read waits for the end of the time, and is not lost", async () => {
    const r = rig();
    r.start();
    await r.flush();
    r.change();
    r.change();
    await r.flush();
    expect(r.reads).toEqual([0]);
    await r.advance(READ_EVERY_MS);
    expect(r.reads).toEqual([0, READ_EVERY_MS]);
    expect(r.got.at(-1)).toBe("t1:2");
  });

  test("a change after the time is read at once", async () => {
    const r = rig();
    r.start();
    // The first read is under way (an async step): let it happen at 0.
    await r.flush();
    await r.advance(READ_EVERY_MS + 1);
    r.change();
    await r.flush();
    expect(r.reads).toEqual([0, READ_EVERY_MS + 1]);
  });

  test("no trip: nothing is read and nothing is listened to", async () => {
    const r = rig();
    r.start(null);
    await r.flush();
    expect(r.reads).toEqual([]);
    expect(r.listeners.size).toBe(0);
  });

  test("after stop nothing more is set, the timer is cancelled and the listening ends", async () => {
    const r = rig();
    const stop = r.start();
    await r.flush();
    r.change();
    stop();
    await r.advance(READ_EVERY_MS);
    expect(r.got).toEqual(["t1:0"]);
    expect(r.timers.size).toBe(0);
    expect(r.listeners.size).toBe(0);
  });

  test("a read that fails is dropped without a word", async () => {
    const r = rig({ fail: true });
    r.start();
    await r.flush();
    expect(r.reads).toEqual([0]);
    expect(r.got).toEqual([]);
  });
});
