import { afterEach, beforeEach, describe, expect, test } from "vite-plus/test";
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

/** A timer of the stand-in for `setTimeout`. */
interface FakeTimer {
  handle: number;
  /** The wait it was asked for. */
  ms: number;
  /** The clock when it was set. */
  at: number;
  run: () => void;
  cleared: boolean;
  fired: boolean;
}

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
    /** Every timer that was set, in order. */
    timers: [] as FakeTimer[],
    /** What `clearTimeout` was handed, in order. */
    cleared: [] as unknown[],
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
    setTimeout: (run, ms) => {
      const timer: FakeTimer = {
        handle: state.timers.length + 1,
        ms,
        at: state.now,
        run,
        cleared: false,
        fired: false,
      };
      state.timers.push(timer);
      return timer.handle;
    },
    clearTimeout: (handle) => {
      state.cleared.push(handle);
      const timer = state.timers.find((candidate) => candidate.handle === handle);
      if (timer) timer.cleared = true;
    },
  };
  /** The store reports a change: every listener is called, as the bus does. */
  const change = () => {
    for (const listener of state.listeners) listener();
  };
  /** Time goes by: the timers that fall due run, in order of their time; the clock follows them. */
  const advance = (ms: number) => {
    const end = state.now + ms;
    for (;;) {
      const due = state.timers
        .filter((timer) => !timer.cleared && !timer.fired && timer.at + timer.ms <= end)
        .sort((a, b) => a.at + a.ms - (b.at + b.ms))[0];
      if (!due) break;
      state.now = Math.max(state.now, due.at + due.ms);
      due.fired = true;
      due.run();
    }
    state.now = end;
  };
  /** A timer runs although it was cancelled: its call was queued already. */
  const fire = (timer: FakeTimer) => {
    timer.fired = true;
    timer.run();
  };
  /** The timers that are set and have neither run nor been cancelled. */
  const waiting = () => state.timers.filter((timer) => !timer.cleared && !timer.fired);
  return { state, ports, change, advance, fire, waiting };
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

  test("a trip with no stored position is read, and the count of zero is handed over like any other", async () => {
    // The start of every trip: the screen says "waiting" or "no position" only from this count, so
    // a count of zero must reach `set` (it must not be taken for "nothing read").
    const { state, ports } = rig({ points: [] });
    startProgressReader(ports);
    expect(state.reads).toEqual(["trip-1"]);
    await flush();
    expect(state.sets).toHaveLength(1);
    expect(state.sets[0]).toEqual({ points: 0, metres: 0, lastFixMs: null, signal: null });
    expect(state.sets[0]).toEqual(progressFrom([]));
  });

  test("a count of zero that is read later, at the end of the time, is handed over too", async () => {
    const { state, ports, change, advance } = rig({ now: 10_000 });
    startProgressReader(ports);
    await flush();
    expect(state.sets[0]).toMatchObject({ points: 7 });
    state.points = [];
    state.now += 1000;
    change();
    advance(READ_EVERY_MS);
    await flush();
    expect(state.sets).toHaveLength(2);
    expect(state.sets[1]).toEqual({ points: 0, metres: 0, lastFixMs: null, signal: null });
  });

  test("the read is asked for the id it was given, and for nothing else", () => {
    const { state, ports } = rig({ id: "another-trip" });
    startProgressReader(ports);
    expect(state.reads).toEqual(["another-trip"]);
  });

  test("the first read does not wait for the clock, whatever the clock says", async () => {
    // The throttle must not swallow the first read when the clock starts at 0 or is small.
    for (const now of [0, 1, 3999, 4000, 1_000_000, 1.7e12]) {
      const { state, ports, waiting } = rig({ now });
      startProgressReader(ports);
      await flush();
      expect(state.reads).toEqual(["trip-1"]);
      expect(state.sets).toHaveLength(1);
      expect(waiting()).toEqual([]);
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

  test("a change inside the time is not read at once, one at the time (or later) is", async () => {
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
    // The two changes inside the time set one timer between them, and the read at the time took
    // its place.
    expect(state.timers).toHaveLength(1);
    expect(state.timers[0]!.cleared).toBe(true);
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

  test("a time that goes by without a change reads nothing and sets no timer", async () => {
    const { state, ports, advance } = rig({ now: 50_000 });
    startProgressReader(ports);
    advance(600_000);
    await flush();
    expect(state.reads).toHaveLength(1);
    expect(state.timers).toEqual([]);
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

  test("a trip without an id reads nothing, listens to nothing and sets no timer", async () => {
    const { state, ports, change, advance } = rig({ id: null });
    const stop = startProgressReader(ports);
    state.now += 100 * READ_EVERY_MS;
    change();
    advance(READ_EVERY_MS);
    await flush();
    expect(state.reads).toEqual([]);
    expect(state.subscribed).toBe(0);
    expect(state.sets).toEqual([]);
    expect(state.timers).toEqual([]);
    // There is still something to call when the screen goes away.
    expect(typeof stop).toBe("function");
    expect(() => stop()).not.toThrow();
    expect(state.unsubscribed).toBe(0);
    expect(state.cleared).toEqual([]);
  });
});

describe("a change inside the time is read when the time is up", () => {
  /** 16 imprecise positions 2 s apart: 30 s, so the note says the signal is weak. */
  const weakTrip = Array.from({ length: 16 }, (_, i) => point(i + 1, i * 2, 0, "150.00"));
  /** And a good position (8 m) stored 2 s after the last of them. */
  const recovered = [...weakTrip, point(17, 32, 0, "8.00")];

  test("the phone that was in a weak place and now delivers good positions: the note goes, also when nothing else is stored", async () => {
    // The case of the review: the sixteenth imprecise position is read, 2 s later a good one is
    // stored and the bus reports it inside the time; nothing is stored after it.
    expect(progressFrom(weakTrip).signal).toMatchObject({ kind: "weak" });
    expect(progressFrom(recovered).signal).toBeNull();

    const { state, ports, change, advance, waiting } = rig({ now: 1_000_000, points: weakTrip });
    startProgressReader(ports);
    await flush();
    expect(state.sets).toHaveLength(1);
    expect(state.sets[0]!.signal).toMatchObject({ kind: "weak" });

    state.now += 2000;
    state.points = recovered;
    change();
    // Not read at once ...
    expect(state.reads).toHaveLength(1);
    expect(waiting()).toHaveLength(1);
    // ... but when the time is up, which is 2 s later.
    advance(1999);
    expect(state.reads).toHaveLength(1);
    advance(1);
    expect(state.reads).toHaveLength(2);
    await flush();
    expect(state.sets).toHaveLength(2);
    expect(state.sets[1]).toEqual(progressFrom(recovered));
    expect(state.sets[1]!.signal).toBeNull();
    expect(state.sets[1]).toMatchObject({ points: 17 });

    // And nothing more is read afterwards, however long it takes.
    advance(5 * 60_000);
    await flush();
    expect(state.reads).toHaveLength(2);
    expect(state.sets).toHaveLength(2);
    expect(waiting()).toEqual([]);
  });

  test("the wait is the rest of the time since the last read", () => {
    const { state, ports, change, advance } = rig({ now: 50_000 });
    startProgressReader(ports);
    state.now += 1000;
    change();
    expect(state.timers).toHaveLength(1);
    expect(state.timers[0]!.ms).toBe(READ_EVERY_MS - 1000);
    advance(READ_EVERY_MS - 1000 - 1);
    expect(state.reads).toHaveLength(1);
    advance(1);
    expect(state.reads).toHaveLength(2);
  });

  test("the wait at the edges: a change in the same millisecond waits the whole time, one a millisecond early waits one", () => {
    const same = rig({ now: 50_000 });
    startProgressReader(same.ports);
    same.change();
    expect(same.state.timers.map((timer) => timer.ms)).toEqual([READ_EVERY_MS]);
    expect(same.state.reads).toHaveLength(1);

    const early = rig({ now: 50_000 });
    startProgressReader(early.ports);
    early.state.now += READ_EVERY_MS - 1;
    early.change();
    expect(early.state.timers.map((timer) => timer.ms)).toEqual([1]);
    expect(early.state.reads).toHaveLength(1);

    // At the time itself there is nothing to wait for: it is read at once.
    const exact = rig({ now: 50_000 });
    startProgressReader(exact.ports);
    exact.state.now += READ_EVERY_MS;
    exact.change();
    expect(exact.state.timers).toEqual([]);
    expect(exact.state.reads).toHaveLength(2);
  });

  test("however many changes come in inside the time, one timer waits and one read follows", async () => {
    const { state, ports, change, advance, waiting } = rig({ now: 50_000 });
    startProgressReader(ports);
    for (const at of [1, 2, 500, 1500, 2000, READ_EVERY_MS - 1]) {
      state.now = 50_000 + at;
      change();
      change();
    }
    expect(state.timers).toHaveLength(1);
    expect(waiting()).toHaveLength(1);
    expect(state.reads).toHaveLength(1);
    // The one timer was set by the first change: 4000 - 1.
    expect(state.timers[0]!.ms).toBe(READ_EVERY_MS - 1);
    advance(READ_EVERY_MS);
    await flush();
    expect(state.reads).toHaveLength(2);
    expect(state.sets).toHaveLength(2);
    expect(waiting()).toEqual([]);
  });

  test("the read at the end of the time gets what was stored in between", async () => {
    const { state, ports, change, advance } = rig({ now: 10_000, points: imprecise(3) });
    startProgressReader(ports);
    await flush();
    state.now += 1000;
    state.points = imprecise(5);
    change();
    state.now += 1000;
    state.points = imprecise(9);
    change();
    advance(READ_EVERY_MS);
    await flush();
    expect(state.sets).toHaveLength(2);
    expect(state.sets[1]).toEqual(progressFrom(imprecise(9)));
  });

  test("the read at the end of the time counts for the time, like any other", async () => {
    const { state, ports, change, advance } = rig({ now: 50_000 });
    startProgressReader(ports);
    state.now += 1000;
    change();
    advance(READ_EVERY_MS - 1000); // the timer reads at 54,000
    expect(state.reads).toHaveLength(2);
    // A new time runs from that read: a change 1 s later waits 3 s, one 4 s later is read at once.
    state.now += 1000;
    change();
    expect(state.reads).toHaveLength(2);
    expect(state.timers).toHaveLength(2);
    expect(state.timers[1]!.ms).toBe(READ_EVERY_MS - 1000);
    advance(READ_EVERY_MS - 1000);
    expect(state.reads).toHaveLength(3);
    await flush();
    expect(state.sets).toHaveLength(3);
    state.now += READ_EVERY_MS;
    change();
    expect(state.reads).toHaveLength(4);
    expect(state.timers).toHaveLength(2);
  });

  test("a timer that runs late reads once, and the time runs from when it read", () => {
    const { state, ports, change, advance } = rig({ now: 50_000 });
    startProgressReader(ports);
    state.now += 1000;
    change();
    // The timer was due at 54,000 but the phone was busy: it runs at 56,000.
    state.timers[0]!.fired = true;
    state.now = 56_000;
    state.timers[0]!.run();
    expect(state.reads).toHaveLength(2);
    // The time runs from 56,000: a change at 59,999 waits one millisecond, one at 60,000 reads.
    state.now = 59_999;
    change();
    expect(state.reads).toHaveLength(2);
    expect(state.timers[1]!.ms).toBe(1);
    advance(1);
    expect(state.reads).toHaveLength(3);
  });

  test("the timer decides, not the clock: it reads when it runs", () => {
    const { state, ports, change, fire } = rig({ now: 50_000 });
    startProgressReader(ports);
    state.now += 1000;
    change();
    // The clock has not moved (it is a different clock from the timers'), the timer runs.
    fire(state.timers[0]!);
    expect(state.reads).toHaveLength(2);
    expect(state.timers).toHaveLength(1);
  });

  test("a second time inside the time waits again after the first read at the end of it", () => {
    const { state, ports, change, advance, waiting } = rig({ now: 50_000 });
    startProgressReader(ports);
    state.now += 1000;
    change();
    advance(READ_EVERY_MS - 1000);
    expect(waiting()).toEqual([]);
    state.now += 100;
    change();
    expect(waiting()).toHaveLength(1);
    advance(READ_EVERY_MS - 100);
    expect(state.reads).toHaveLength(3);
  });

  test("the time can be given, and the wait follows it", () => {
    const { state, ports, change, advance } = rig({ now: 10_000 });
    startProgressReader({ ...ports, throttleMs: 1000 });
    state.now += 300;
    change();
    expect(state.timers.map((timer) => timer.ms)).toEqual([700]);
    advance(699);
    expect(state.reads).toHaveLength(1);
    advance(1);
    expect(state.reads).toHaveLength(2);
  });

  test("a clock that was set back is not waited for longer than the time", () => {
    const { state, ports, change, advance } = rig({ now: 50_000 });
    startProgressReader(ports);
    state.now -= 10_000;
    change();
    expect(state.timers.map((timer) => timer.ms)).toEqual([READ_EVERY_MS]);
    advance(READ_EVERY_MS);
    expect(state.reads).toHaveLength(2);
  });

  test("a read that is due when a change comes in takes the place of the one that waits", async () => {
    const { state, ports, change, advance, fire, waiting } = rig({ now: 50_000 });
    startProgressReader(ports);
    state.now += 1000;
    change();
    const first = state.timers[0]!;
    expect(waiting()).toEqual([first]);
    // The timer is late: the clock is past the time and the timer has not run.
    state.now = 50_000 + READ_EVERY_MS + 500;
    change();
    expect(state.reads).toHaveLength(2);
    expect(state.cleared).toEqual([first.handle]);
    expect(waiting()).toEqual([]);
    // The call of the cancelled timer was queued already, and it runs: it reads nothing.
    fire(first);
    expect(state.reads).toHaveLength(2);
    await flush();
    expect(state.sets).toHaveLength(2);
    // The cancelled timer left nothing behind: the next change inside the time waits again.
    state.now += 100;
    change();
    expect(waiting()).toHaveLength(1);
    expect(state.timers).toHaveLength(2);
    advance(READ_EVERY_MS);
    expect(state.reads).toHaveLength(3);
  });

  test("a cancelled timer that runs anyway reads nothing, also while a newer one waits", () => {
    const { state, ports, change, fire, advance, waiting } = rig({ now: 50_000 });
    startProgressReader(ports);
    state.now += 1000;
    change();
    const old = state.timers[0]!;
    // A read that is due cancels it ...
    state.now = 50_000 + READ_EVERY_MS + 500;
    change();
    expect(old.cleared).toBe(true);
    expect(state.reads).toHaveLength(2);
    // ... a change inside the new time sets a newer one ...
    state.now += 100;
    change();
    const newer = state.timers[1]!;
    expect(waiting()).toEqual([newer]);
    // ... and the old call, which was queued already, must neither read nor take the newer one's place.
    fire(old);
    expect(state.reads).toHaveLength(2);
    expect(waiting()).toEqual([newer]);
    advance(READ_EVERY_MS);
    expect(state.reads).toHaveLength(3);
    expect(state.timers).toHaveLength(2);
  });

  test("a timer that has run cannot run a second read", () => {
    const { state, ports, change, fire } = rig({ now: 50_000 });
    startProgressReader(ports);
    state.now += 1000;
    change();
    const timer = state.timers[0]!;
    fire(timer);
    expect(state.reads).toHaveLength(2);
    fire(timer);
    expect(state.reads).toHaveLength(2);
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

  test("stop with no read waiting cancels no timer", () => {
    const { state, ports } = rig();
    const stop = startProgressReader(ports);
    stop();
    expect(state.cleared).toEqual([]);
  });

  test("stop cancels the read that waits, and the timer does nothing if it runs anyway", async () => {
    const { state, ports, change, advance, fire } = rig({ now: 50_000 });
    const stop = startProgressReader(ports);
    await flush();
    state.now += 1000;
    change();
    const timer = state.timers[0]!;
    stop();
    expect(state.cleared).toEqual([timer.handle]);
    expect(timer.cleared).toBe(true);
    advance(10 * READ_EVERY_MS);
    fire(timer);
    await flush();
    expect(state.reads).toHaveLength(1);
    expect(state.sets).toHaveLength(1);
  });

  test("a timer handle that looks like nothing (0, undefined) is cancelled as well", () => {
    for (const handle of [0, undefined]) {
      const { state, ports, change } = rig({ now: 50_000 });
      const cleared: unknown[] = [];
      const stop = startProgressReader({
        ...ports,
        setTimeout: () => handle,
        clearTimeout: (timer) => {
          cleared.push(timer);
        },
      });
      state.now += 1000;
      change();
      stop();
      expect(cleared).toEqual([handle]);
    }
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

  test("a read that the timer started and that is under way when it stops is not handed over", async () => {
    const { state, ports, change, advance } = rig({ now: 50_000 });
    const stop = startProgressReader(ports);
    await flush();
    let release: (points: StoredPoint[]) => void = () => undefined;
    state.answer = () =>
      new Promise<StoredPoint[]>((resolve) => {
        release = resolve;
      });
    state.now += 1000;
    change();
    advance(READ_EVERY_MS);
    expect(state.reads).toHaveLength(2);
    stop();
    release(state.points);
    await flush();
    expect(state.sets).toHaveLength(1);
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
    // At the time, and inside it: neither reads, and neither sets a timer.
    state.now += READ_EVERY_MS;
    kept();
    state.now += 1;
    kept();
    await flush();
    expect(state.reads).toHaveLength(1);
    expect(state.sets).toHaveLength(1);
    expect(state.timers).toEqual([]);
    expect(state.unsubscribed).toBe(1);
  });
});

describe("a read that fails", () => {
  // A rejection nobody handles would end up here (and fail the run of the tests as well).
  const unhandled: unknown[] = [];
  const onUnhandled = (reason: unknown) => {
    unhandled.push(reason);
  };
  beforeEach(() => {
    unhandled.length = 0;
    process.on("unhandledRejection", onUnhandled);
  });
  afterEach(() => {
    process.off("unhandledRejection", onUnhandled);
  });

  test("a rejected read is dropped without a word and hands over nothing", async () => {
    const { state, ports } = rig();
    state.answer = () => Promise.reject(new Error("the database is closed"));
    startProgressReader(ports);
    await flush();
    expect(state.reads).toHaveLength(1);
    expect(state.sets).toEqual([]);
    expect(unhandled).toEqual([]);
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
    expect(unhandled).toEqual([]);
  });

  test("a failed read still counts for the time, and a change inside it is read at the end of it", async () => {
    const { state, ports, change, advance } = rig({ now: 10_000 });
    state.answer = () => Promise.reject(new Error("busy"));
    startProgressReader(ports);
    await flush();
    state.answer = () => Promise.resolve(state.points);
    state.now += 1000;
    change();
    expect(state.reads).toHaveLength(1);
    advance(READ_EVERY_MS - 1000);
    await flush();
    expect(state.reads).toHaveLength(2);
    expect(state.sets).toEqual([progressFrom(state.points)]);
    expect(unhandled).toEqual([]);
  });

  test("a read at the end of the time that fails is dropped without a word, and the next one works", async () => {
    const { state, ports, change, advance, waiting } = rig({ now: 10_000 });
    startProgressReader(ports);
    await flush();
    expect(state.sets).toHaveLength(1);
    state.answer = () => Promise.reject(new Error("busy"));
    state.now += 1000;
    change();
    advance(READ_EVERY_MS - 1000);
    await flush();
    expect(state.reads).toHaveLength(2);
    expect(state.sets).toHaveLength(1);
    expect(unhandled).toEqual([]);
    expect(waiting()).toEqual([]);
    // It counted for the time, and the reader goes on.
    state.answer = () => Promise.resolve(state.points);
    state.now += 1000;
    change();
    expect(waiting()).toHaveLength(1);
    advance(READ_EVERY_MS - 1000);
    await flush();
    expect(state.reads).toHaveLength(3);
    expect(state.sets).toHaveLength(2);
    expect(unhandled).toEqual([]);
  });

  test("a read at the end of the time that throws at once is a failed read too", async () => {
    const { state, ports, change, advance } = rig({ now: 10_000 });
    startProgressReader({
      ...ports,
      read: (id) => {
        state.reads.push(id);
        if (state.reads.length > 1) throw new Error("no store");
        return Promise.resolve(state.points);
      },
    });
    await flush();
    state.now += 1000;
    change();
    expect(() => advance(READ_EVERY_MS - 1000)).not.toThrow();
    await flush();
    expect(state.reads).toHaveLength(2);
    expect(state.sets).toHaveLength(1);
    expect(unhandled).toEqual([]);
  });
});
