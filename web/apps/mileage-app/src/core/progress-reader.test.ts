import { describe, expect, test } from "vite-plus/test";
import { progressFrom, type Progress } from "./progress.ts";
import { READ_EVERY_MS, startProgressReader, type ProgressReaderPorts } from "./progress-reader.ts";
import type { StoredPoint } from "./types.ts";

const T0 = Date.parse("2026-11-04T09:00:00Z");

/** A stored point as the store gives it (the texts of the database). */
const point = (seq: number, secs: number, northM: number, accuracy: string): StoredPoint => ({
  seq,
  t: T0 + secs * 1000,
  lat: (52 + northM / 111_195).toFixed(7),
  lon: (13).toFixed(7),
  accuracyM: accuracy,
  speedMps: null,
});

/** `n` points of a radius of 150 m, 5 s apart: from the seventh on, the note says the signal is weak. */
const imprecise = (n: number): StoredPoint[] =>
  Array.from({ length: n }, (_, i) => point(i + 1, i * 5, (i % 3) * 4, "150.00"));

/** Lets every promise that is ready run. */
const flush = () => new Promise<void>((resolve) => setImmediate(resolve));

/** The ports of the reader, made of things a test can look at. */
function rig(initial: { id?: string | null; now?: number; points?: StoredPoint[] } = {}) {
  const state = {
    now: initial.now ?? 1_000_000,
    points: initial.points ?? imprecise(7),
    /** The ids `read` was asked for, in order. */
    reads: [] as string[],
    /** What `set` was handed, in order. */
    sets: [] as Progress[],
    /** The listeners that are subscribed now. */
    listeners: new Set<() => void>(),
    subscribed: 0,
    unsubscribed: 0,
    /** What a read does: the default answers with `state.points`. */
    answer: (): Promise<readonly StoredPoint[]> => Promise.resolve(state.points),
  };
  const ports: ProgressReaderPorts = {
    id: initial.id === undefined ? "trip-1" : initial.id,
    read: (id) => {
      state.reads.push(id);
      return state.answer();
    },
    subscribe: (listener) => {
      state.subscribed++;
      state.listeners.add(listener);
      return () => {
        state.unsubscribed++;
        state.listeners.delete(listener);
      };
    },
    clock: () => state.now,
    set: (progress) => {
      state.sets.push(progress);
    },
  };
  /** The store reports a change: every listener is called, as the bus does. */
  const change = () => {
    for (const listener of state.listeners) listener();
  };
  return { state, ports, change };
}

describe("the reader of the Recording screen's count", () => {
  test("it reads the stored positions of the open trip at once and hands over the count", async () => {
    const { state, ports } = rig();
    startProgressReader(ports);
    expect(state.reads).toEqual(["trip-1"]);
    await flush();
    expect(state.sets).toEqual([progressFrom(state.points)]);
    // And that is the count with the note about a distance of 0 m, not only the numbers.
    expect(state.sets[0]).toMatchObject({
      points: 7,
      metres: 0,
      lastFixMs: T0 + 30_000,
      signal: { kind: "weak", dropped: 7, stored: 7, accuracyM: 150 },
    });
  });

  test("the read is asked for the id it was given, and for nothing else", () => {
    const { state, ports } = rig({ id: "another-trip" });
    startProgressReader(ports);
    expect(state.reads).toEqual(["another-trip"]);
  });

  test("the first read does not wait for the clock, whatever the clock says", async () => {
    // The throttle must not swallow the first read when the clock starts at 0 or is small.
    for (const now of [0, 1, 3999, 4000, 1_000_000, 1.7e12]) {
      const { state, ports } = rig({ now });
      startProgressReader(ports);
      await flush();
      expect(state.reads).toEqual(["trip-1"]);
      expect(state.sets).toHaveLength(1);
    }
  });

  test("it listens once, and a change is a read", async () => {
    const { state, ports, change } = rig();
    startProgressReader(ports);
    expect(state.subscribed).toBe(1);
    expect(state.listeners.size).toBe(1);
    state.now += READ_EVERY_MS;
    change();
    expect(state.reads).toEqual(["trip-1", "trip-1"]);
  });

  test("the time between two reads is 4 seconds", () => {
    expect(READ_EVERY_MS).toBe(4000);
  });

  test("a change inside the time is not read, one at the time (or later) is", async () => {
    const { state, ports, change } = rig({ now: 50_000 });
    startProgressReader(ports);
    expect(state.reads).toHaveLength(1);
    state.now = 50_000 + 1;
    change();
    state.now = 50_000 + READ_EVERY_MS - 1;
    change();
    expect(state.reads).toHaveLength(1);
    state.now = 50_000 + READ_EVERY_MS;
    change();
    expect(state.reads).toHaveLength(2);
    await flush();
    expect(state.sets).toHaveLength(2);
  });

  test("the time runs from the last read, not from the last change", () => {
    const { state, ports, change } = rig({ now: 50_000 });
    startProgressReader(ports);
    // A change just before the time is up must not push the next read back.
    state.now = 50_000 + READ_EVERY_MS - 1;
    change();
    state.now = 50_000 + READ_EVERY_MS;
    change();
    expect(state.reads).toHaveLength(2);
    // And the next time runs from that read.
    state.now = 50_000 + 2 * READ_EVERY_MS - 1;
    change();
    expect(state.reads).toHaveLength(2);
    state.now = 50_000 + 2 * READ_EVERY_MS;
    change();
    expect(state.reads).toHaveLength(3);
  });

  test("a change inside the time is not read later: the reader sets no timer", async () => {
    const { state, ports, change } = rig({ now: 50_000 });
    startProgressReader(ports);
    state.now += 1000;
    change();
    state.now += 60_000;
    await flush();
    expect(state.reads).toHaveLength(1);
  });

  test("a time that goes by without a change reads nothing", async () => {
    const { state, ports } = rig({ now: 50_000 });
    startProgressReader(ports);
    state.now += 600_000;
    await flush();
    expect(state.reads).toHaveLength(1);
  });

  test("the time between two reads can be given, and is the same kind of limit", () => {
    const { state, ports, change } = rig({ now: 10_000 });
    startProgressReader({ ...ports, throttleMs: 1000 });
    state.now += 999;
    change();
    expect(state.reads).toHaveLength(1);
    state.now += 1;
    change();
    expect(state.reads).toHaveLength(2);
  });

  test("a read after the time hands over the new count", async () => {
    const { state, ports, change } = rig({ now: 10_000, points: imprecise(3) });
    startProgressReader(ports);
    await flush();
    expect(state.sets[0]).toMatchObject({ points: 3, signal: null });
    state.points = imprecise(8);
    state.now += READ_EVERY_MS;
    change();
    await flush();
    expect(state.sets).toHaveLength(2);
    expect(state.sets[1]).toEqual(progressFrom(imprecise(8)));
    expect(state.sets[1]).toMatchObject({ points: 8, signal: { kind: "weak", stored: 8 } });
  });

  test("a trip without an id reads nothing and listens to nothing", async () => {
    const { state, ports, change } = rig({ id: null });
    const stop = startProgressReader(ports);
    state.now += 100 * READ_EVERY_MS;
    change();
    await flush();
    expect(state.reads).toEqual([]);
    expect(state.subscribed).toBe(0);
    expect(state.sets).toEqual([]);
    // There is still something to call when the screen goes away.
    expect(typeof stop).toBe("function");
    expect(() => stop()).not.toThrow();
    expect(state.unsubscribed).toBe(0);
  });
});

describe("stopping the reader", () => {
  test("stop ends the subscription, once", () => {
    const { state, ports } = rig();
    const stop = startProgressReader(ports);
    expect(state.unsubscribed).toBe(0);
    stop();
    expect(state.unsubscribed).toBe(1);
    expect(state.listeners.size).toBe(0);
  });

  test("a read that is under way when it stops is not handed over", async () => {
    const { state, ports } = rig();
    let release: (points: StoredPoint[]) => void = () => undefined;
    state.answer = () =>
      new Promise<StoredPoint[]>((resolve) => {
        release = resolve;
      });
    const stop = startProgressReader(ports);
    expect(state.reads).toHaveLength(1);
    stop();
    release(state.points);
    await flush();
    expect(state.sets).toEqual([]);
  });

  test("a change that still arrives after the stop reaches nothing: the reader checks the stop, not only the subscription", async () => {
    const { state, ports } = rig({ now: 10_000 });
    let kept: () => void = () => undefined;
    const stop = startProgressReader({
      ...ports,
      subscribe: (listener) => {
        kept = listener;
        return () => {
          state.unsubscribed++;
        };
      },
    });
    await flush();
    stop();
    state.now += READ_EVERY_MS;
    kept();
    await flush();
    expect(state.sets).toHaveLength(1);
    expect(state.unsubscribed).toBe(1);
  });
});

describe("a read that fails", () => {
  test("a rejected read is dropped without a word and hands over nothing", async () => {
    const { state, ports } = rig();
    state.answer = () => Promise.reject(new Error("the database is closed"));
    startProgressReader(ports);
    await flush();
    expect(state.reads).toHaveLength(1);
    expect(state.sets).toEqual([]);
  });

  test("a read that throws at once is a failed read like any other", async () => {
    const { state, ports } = rig();
    expect(() =>
      startProgressReader({
        ...ports,
        read: (id) => {
          state.reads.push(id);
          throw new Error("no store");
        },
      }),
    ).not.toThrow();
    await flush();
    expect(state.reads).toHaveLength(1);
    expect(state.sets).toEqual([]);
    expect(state.subscribed).toBe(1);
  });

  test("a failed read still counts for the time, and the next read after it works", async () => {
    const { state, ports, change } = rig({ now: 10_000 });
    state.answer = () => Promise.reject(new Error("busy"));
    startProgressReader(ports);
    await flush();
    state.answer = () => Promise.resolve(state.points);
    state.now += 1000;
    change();
    expect(state.reads).toHaveLength(1);
    state.now += READ_EVERY_MS;
    change();
    await flush();
    expect(state.reads).toHaveLength(2);
    expect(state.sets).toEqual([progressFrom(state.points)]);
  });
});
