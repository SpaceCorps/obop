import { analyseTrack } from "@obop/trip-core";
import { beforeEach, describe, expect, test, vi } from "vite-plus/test";
import { progressFrom } from "./progress.ts";
import { MIN_SPAN_MS } from "./signal.ts";
import type { StoredPoint } from "./types.ts";

// The real `analyseTrack`, with a count of its calls.
vi.mock("@obop/trip-core", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@obop/trip-core")>();
  return { ...actual, analyseTrack: vi.fn(actual.analyseTrack) };
});

const T0 = Date.parse("2026-11-04T09:00:00Z");

/** A stored point as the store gives it: the texts of the database (7 decimals, 2 for the radius). */
const point = (
  seq: number,
  secs: number,
  northM: number,
  accuracy: string | null,
): StoredPoint => ({
  seq,
  t: T0 + secs * 1000,
  lat: (52 + northM / 111_195).toFixed(7),
  lon: (13).toFixed(7),
  accuracyM: accuracy,
  speedMps: null,
});

/** `n` points, all with a radius of 150 m, `stepS` seconds apart. */
const imprecise = (n: number, stepS: number): StoredPoint[] =>
  Array.from({ length: n }, (_, i) => point(i + 1, i * stepS, (i % 3) * 4, "150.00"));

beforeEach(() => {
  vi.mocked(analyseTrack).mockClear();
});

describe("the count of a trip from its stored points", () => {
  test("a trip without points: nothing counted, nothing said", () => {
    expect(progressFrom([])).toEqual({ points: 0, metres: 0, lastFixMs: null, signal: null });
  });

  test("a drive: the distance of the points that count, no note", () => {
    // 40 s of positions: long enough for a note, so the silence is the verdict of the counters.
    const drive = Array.from({ length: 40 }, (_, i) => point(i + 1, i, i * 12, "5.00"));
    const progress = progressFrom(drive);
    expect(progress.points).toBe(40);
    expect(progress.metres).toBeGreaterThan(400);
    expect(progress.lastFixMs).toBe(T0 + 39_000);
    expect(progress.signal).toBeNull();
  });

  test("indoors, where every radius is 150 m: 0 m, all points stored, and the note says why", () => {
    expect(progressFrom(imprecise(7, 10))).toEqual({
      points: 7,
      metres: 0,
      lastFixMs: T0 + 60_000,
      signal: { kind: "weak", dropped: 7, stored: 7, accuracyM: 150 },
    });
  });

  test("a first point far away: 0 m and the note", () => {
    const first: StoredPoint = { ...point(1, 0, 0, "20.00"), lat: "40.0000000", lon: "-3.0000000" };
    const later = Array.from({ length: 4 }, (_, i) => point(i + 2, 5 + i * 10, i * 10, "6.00"));
    expect(progressFrom([first, ...later])).toMatchObject({
      points: 5,
      metres: 0,
      signal: { kind: "jumpy", dropped: 4, stored: 5 },
    });
  });

  test("two good points at one place among imprecise ones: 0 m and no note", () => {
    const stored = [
      point(1, 0, 0, "8.00"),
      point(2, 20, 0, "8.00"),
      point(3, 40, 0, "150.00"),
      point(4, 60, 0, "150.00"),
    ];
    const progress = progressFrom(stored);
    expect(vi.mocked(analyseTrack).mock.results[0]!.value).toMatchObject({ accepted: 2 });
    expect(progress).toMatchObject({ points: 4, metres: 0, signal: null });
  });

  test("the radius shown is the one of the newest point by seq, whatever the order of the list", () => {
    const first = point(1, 0, 0, "150.00");
    const second = point(2, 20, 0, "180.00");
    const third = point(3, 40, 0, "260.00");
    // The list is not in the order of storage; the newest is seq 3, not the last of the list.
    expect(progressFrom([third, first, second]).signal).toEqual({
      kind: "weak",
      dropped: 3,
      stored: 3,
      accuracyM: 260,
    });
    expect(progressFrom([first, second, third]).signal).toMatchObject({ accuracyM: 260 });
    expect(progressFrom([second, third, first]).signal).toMatchObject({ accuracyM: 260 });
  });

  test("a newest point without a radius leaves the radius out of the note", () => {
    // Three too imprecise, then a point without a radius: it is the first that counts.
    const stored = [
      point(1, 0, 0, "150.00"),
      point(2, 15, 0, "150.00"),
      point(3, 30, 0, "150.00"),
      point(4, 45, 0, null),
    ];
    expect(progressFrom(stored).signal).toEqual({
      kind: "weak",
      dropped: 3,
      stored: 4,
      accuracyM: null,
    });
  });

  test("a stored radius of 0.00 is not a radius to name", () => {
    // A good point, two too imprecise, and a newest one that is 20 km away within 40 s.
    const stored = [
      point(1, 0, 0, "5.00"),
      point(2, 15, 0, "150.00"),
      point(3, 30, 0, "150.00"),
      point(4, 40, 20_000, "0.00"),
    ];
    expect(progressFrom(stored).signal).toEqual({
      kind: "weak",
      dropped: 3,
      stored: 4,
      accuracyM: null,
    });
  });

  test("the track is analysed once, and the distance and the note come from that analysis", () => {
    const progress = progressFrom(imprecise(5, 10));
    expect(analyseTrack).toHaveBeenCalledTimes(1);
    const analysis = vi.mocked(analyseTrack).mock.results[0]!.value as ReturnType<
      typeof analyseTrack
    >;
    expect(progress.metres).toBe(analysis.distanceM);
    expect(progress.signal).toMatchObject({ dropped: analysis.skippedAccuracy });
  });
});

describe("the first seconds of a trip", () => {
  test("positions that span less than the minimum say nothing, however imprecise", () => {
    // Seven imprecise positions 2 s apart: 12 s. The phone is still finding its position.
    const progress = progressFrom(imprecise(7, 2));
    expect(analyseTrack).toHaveBeenCalledTimes(1);
    expect(progress.points).toBe(7);
    expect(progress.signal).toBeNull();
    // Android, a fix every 5 s: three of them at t = 10 s.
    expect(progressFrom(imprecise(3, 5)).signal).toBeNull();
  });

  test("the same positions spanning exactly the minimum say it", () => {
    expect(MIN_SPAN_MS).toBe(30_000);
    // Seven positions 5 s apart are 30 s; one second less is not enough.
    expect(progressFrom(imprecise(7, 5)).signal).toMatchObject({ kind: "weak", stored: 7 });
    const short = imprecise(7, 5).map((p, i) => (i === 6 ? { ...p, t: p.t - 1000 } : p));
    expect(progressFrom(short).signal).toBeNull();
  });

  test("the span is the newest time stamp minus the oldest, not the first by seq minus the last", () => {
    // seq 1 is the latest in time and seq 2 the earliest: by seq the span would be negative.
    const stored = [point(1, 40, 0, "150.00"), point(2, 0, 0, "150.00"), point(3, 10, 0, "150.00")];
    expect(progressFrom(stored).signal).toEqual({
      kind: "weak",
      dropped: 3,
      stored: 3,
      accuracyM: 150,
    });
    expect(progressFrom(stored).lastFixMs).toBe(T0 + 40_000);
    // The order of the list does not matter either.
    expect(progressFrom([...stored].reverse()).signal).toMatchObject({ kind: "weak" });
  });

  test("the span is the one of the stored positions, not the time since the trip started", () => {
    // Positions 5 s apart for 10 s, long after the start of the trip: still the first seconds.
    const late = imprecise(3, 5).map((p) => ({ ...p, t: p.t + 3_600_000 }));
    expect(progressFrom(late).signal).toBeNull();
  });
});
