import type { ReactElement, ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";
import { progressFrom, type Progress } from "../../core/progress.ts";
import { READ_EVERY_MS } from "../../core/progress-reader.ts";
import { MAX_POINTS_PER_TRIP, type StoredPoint } from "../../core/types.ts";
import { t } from "../../i18n/index.ts";
import type { Runtime } from "../../native/runtime.ts";
import type { Nav } from "../router.ts";

/**
 * What the Recording screen wires up, without a renderer (there is none in this app): the hooks of
 * React are stand-ins that remember what they were given, the screen is called as a function, and
 * the effect that reads the stored positions is run by the test with fake ports. Nothing is drawn.
 */
const hooks = vi.hoisted(() => ({
  /** Every `useState` of the last call, in order: the first one is `measured`. */
  states: [] as Array<{ initial: unknown; set: (value: unknown) => void }>,
  /** Every `useEffect` of the last call, in order: the first one reads the stored positions. */
  effects: [] as Array<{ run: () => unknown; deps: readonly unknown[] | undefined }>,
  /** What `useState` hands out as the value of the first state (`measured`), when not `null`. */
  measured: null as unknown,
  outbox: { active: null, loaded: true } as {
    active: { clientTripId: string; startedAt: string } | null;
    loaded: boolean;
  },
  listeners: [] as Array<() => void>,
  unsubscribed: 0,
}));

vi.mock("react", async (importOriginal) => {
  const actual = await importOriginal<typeof import("react")>();
  return {
    ...actual,
    useState: (initial: unknown) => {
      const set = vi.fn();
      const first = hooks.states.length === 0;
      hooks.states.push({ initial, set });
      return [first && hooks.measured !== null ? hooks.measured : initial, set];
    },
    useEffect: (run: () => unknown, deps?: readonly unknown[]) => {
      hooks.effects.push({ run, deps });
    },
    useRef: (initial: unknown) => ({ current: initial }),
    useCallback: (fn: unknown) => fn,
  };
});

// The components only name themselves; the screen's elements are read, not drawn.
vi.mock("@obop/ui-native", () => ({
  Banner: function Banner() {
    return null;
  },
  Button: function Button() {
    return null;
  },
  Screen: function Screen() {
    return null;
  },
  Text: function Text() {
    return null;
  },
}));
vi.mock("react-native", () => ({
  Alert: { alert: vi.fn() },
  AppState: { addEventListener: vi.fn(() => ({ remove: vi.fn() })) },
  Platform: { OS: "android" },
}));
vi.mock("../hooks.ts", () => ({
  useOutbox: () => hooks.outbox,
  useNow: () => NOW,
}));
vi.mock("../announce.ts", () => ({ announce: vi.fn() }));
vi.mock("../../native/actions.ts", () => ({
  resumeRecording: vi.fn(),
  stopTrip: vi.fn(),
}));
vi.mock("../../native/permissions.ts", () => ({ openSettings: vi.fn() }));
vi.mock("../../native/recorder.ts", () => ({ isRecording: vi.fn() }));
vi.mock("../../native/bus.ts", () => ({
  onChange: vi.fn((listener: () => void) => {
    hooks.listeners.push(listener);
    return () => {
      hooks.unsubscribed++;
    };
  }),
}));

const { Banner, Text } = await import("@obop/ui-native");
const { onChange } = await import("../../native/bus.ts");
const { SignalNote } = await import("../SignalNote.tsx");
const { Recording } = await import("./Recording.tsx");

const T0 = Date.parse("2026-11-04T09:00:00Z");
const NOW = T0 + 90_000;
const TRIP = "trip-9";

const point = (seq: number, secs: number): StoredPoint => ({
  seq,
  t: T0 + secs * 1000,
  lat: (52 + ((seq % 3) * 4) / 111_195).toFixed(7),
  lon: (13).toFixed(7),
  accuracyM: "150.00",
  speedMps: null,
});

/** Seven imprecise points 5 s apart: the note says the signal is weak. */
const weakPoints = Array.from({ length: 7 }, (_, i) => point(i + 1, i * 5));

const flush = () => new Promise<void>((resolve) => setImmediate(resolve));

let clockNow = 0;

beforeEach(() => {
  hooks.states.length = 0;
  hooks.effects.length = 0;
  hooks.listeners.length = 0;
  hooks.unsubscribed = 0;
  hooks.measured = null;
  hooks.outbox = {
    active: { clientTripId: TRIP, startedAt: new Date(T0).toISOString() },
    loaded: true,
  };
  clockNow = 5_000_000;
  vi.spyOn(Date, "now").mockImplementation(() => clockNow);
  vi.mocked(onChange).mockClear();
});

afterEach(() => {
  vi.restoreAllMocks();
});

/** The screen, called once; `points` is what the store holds. */
function render(points: StoredPoint[] = weakPoints) {
  const stored = vi.fn(async (_id: string) => points);
  const rt = { store: { points: stored } } as unknown as Runtime;
  const nav = { home: vi.fn(), replace: vi.fn() } as unknown as Nav;
  const root = Recording({ rt, nav }) as ReactElement<{ children?: ReactNode }>;
  return { rt, stored, root };
}

/** The elements among some children, in order, without the `false` and `null` of the conditions. */
function elements(children: ReactNode): ReactElement<Record<string, unknown>>[] {
  const list = Array.isArray(children) ? children : [children];
  return list.filter(
    (child): child is ReactElement<Record<string, unknown>> =>
      typeof child === "object" && child !== null && "props" in child,
  );
}

describe("the Recording screen reads the stored positions of the open trip", () => {
  test("the first effect reads them at once and sets `measured` to the count, with its note", async () => {
    const { rt, stored } = render();
    const [measured] = hooks.states;
    expect(measured!.initial).toBeNull();
    const stop = hooks.effects[0]!.run();
    expect(stored).toHaveBeenCalledTimes(1);
    expect(stored).toHaveBeenCalledWith(TRIP);
    await flush();
    expect(measured!.set).toHaveBeenCalledTimes(1);
    expect(measured!.set).toHaveBeenCalledWith(progressFrom(weakPoints));
    expect(vi.mocked(measured!.set).mock.calls[0]![0]).toMatchObject({
      points: 7,
      metres: 0,
      signal: { kind: "weak", dropped: 7, stored: 7, accuracyM: 150 },
    });
    // The other states were not touched by it.
    for (const other of hooks.states.slice(1)) expect(other.set).not.toHaveBeenCalled();
    expect(typeof stop).toBe("function");
    expect(rt).toBeDefined();
  });

  test("it runs again when the store or the trip changes, and only then", () => {
    const { rt } = render();
    const deps = hooks.effects[0]!.deps!;
    expect(deps).toHaveLength(2);
    expect(deps[0]).toBe(rt);
    expect(deps[1]).toBe(TRIP);
  });

  test("with no open trip nothing is read and nothing is listened to", async () => {
    hooks.outbox = { active: null, loaded: true };
    const { rt, stored } = render();
    expect(hooks.effects[0]!.deps).toEqual([rt, null]);
    const stop = hooks.effects[0]!.run();
    await flush();
    expect(stored).not.toHaveBeenCalled();
    expect(onChange).not.toHaveBeenCalled();
    expect(hooks.states[0]!.set).not.toHaveBeenCalled();
    expect(() => (stop as () => void)()).not.toThrow();
  });

  test("it listens to the store's changes, and reads again no sooner than every 4 seconds of the clock", async () => {
    const { stored } = render();
    hooks.effects[0]!.run();
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(hooks.listeners).toHaveLength(1);
    const change = hooks.listeners[0]!;
    clockNow += READ_EVERY_MS - 1;
    change();
    expect(stored).toHaveBeenCalledTimes(1);
    clockNow += 1;
    change();
    expect(stored).toHaveBeenCalledTimes(2);
    await flush();
    expect(hooks.states[0]!.set).toHaveBeenCalledTimes(2);
  });

  test("when the screen goes away it stops listening, and nothing more reaches `measured`", async () => {
    const { stored } = render();
    let release: (points: StoredPoint[]) => void = () => undefined;
    stored.mockImplementationOnce(
      () =>
        new Promise<StoredPoint[]>((resolve) => {
          release = resolve;
        }),
    );
    const stop = hooks.effects[0]!.run() as () => void;
    expect(hooks.unsubscribed).toBe(0);
    stop();
    expect(hooks.unsubscribed).toBe(1);
    release(weakPoints);
    await flush();
    expect(hooks.states[0]!.set).not.toHaveBeenCalled();
  });
});

describe("what the Recording screen draws of the note", () => {
  const measuredWith = (points: number): Progress => ({
    points,
    metres: 0,
    lastFixMs: T0,
    signal: { kind: "weak", dropped: points, stored: points, accuracyM: 150 },
  });

  test("before the first read the note is given `null`, not a count of zero", () => {
    const { root } = render();
    const notes = elements(root.props.children).filter((child) => child.type === SignalNote);
    expect(notes).toHaveLength(1);
    expect(notes[0]!.props.measured).toBeNull();
  });

  test("after the read it is given what was measured, once", () => {
    const measured = measuredWith(7);
    hooks.measured = measured;
    const { root } = render();
    const notes = elements(root.props.children).filter((child) => child.type === SignalNote);
    expect(notes).toHaveLength(1);
    expect(notes[0]!.props.measured).toBe(measured);
  });

  test("it follows the last banner and comes before the clock", () => {
    // A trip with the most points a trip may hold shows the "full" banner, the last of the banners.
    hooks.measured = measuredWith(MAX_POINTS_PER_TRIP);
    const { root } = render();
    const drawn = elements(root.props.children);
    const at = drawn.findIndex((child) => child.type === SignalNote);
    expect(at).toBeGreaterThan(0);
    const before = drawn[at - 1]!;
    expect(before.type).toBe(Banner);
    expect(before.props.title).toBe(t("recording.full.title"));
    const after = drawn[at + 1]!;
    expect(after.type).toBe(Text);
    expect(after.props.testID).toBe("elapsed");
  });
});
