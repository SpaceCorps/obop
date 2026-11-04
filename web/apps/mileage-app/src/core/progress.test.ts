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

  test("a newest point without a radius is not imprecise: nothing is said when none was dropped for its place", () => {
    // Three too imprecise, then a point without a radius: it is the first that counts.
    const stored = [
      point(1, 0, 0, "150.00"),
      point(2, 15, 0, "150.00"),
      point(3, 30, 0, "150.00"),
      point(4, 45, 0, null),
    ];
    expect(progressFrom(stored)).toMatchObject({ points: 4, metres: 0, signal: null });
  });

  test("a newest point without a radius, where a position was dropped for its place: not fitting, no radius", () => {
    const stored = [
      point(1, 0, 0, "5.00"),
      point(2, 15, 0, "150.00"),
      point(3, 30, 0, "150.00"),
      point(4, 45, 20_000, null),
    ];
    const progress = progressFrom(stored);
    expect(analyseTrack).toHaveBeenCalledTimes(1);
    expect(vi.mocked(analyseTrack).mock.results[0]!.value).toMatchObject({
      accepted: 1,
      skippedAccuracy: 2,
      skippedSpeed: 1,
    });
    expect(progress.signal).toEqual({ kind: "jumpy", dropped: 3, stored: 4 });
  });

  test("a stored radius of 0.00 is not over the limit, so it is no radius to name", () => {
    // A good point, two too imprecise, and a newest one that is 20 km away within 40 s.
    const stored = [
      point(1, 0, 0, "5.00"),
      point(2, 15, 0, "150.00"),
      point(3, 30, 0, "150.00"),
      point(4, 40, 20_000, "0.00"),
    ];
    expect(progressFrom(stored).signal).toEqual({ kind: "jumpy", dropped: 3, stored: 4 });
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

describe("the kind of the note follows the newest stored point (the highest seq)", () => {
  /** A point at a wrong place, 1,800 km from the others. */
  const wrong = (seq: number, secs: number, accuracy: string): StoredPoint => ({
    ...point(seq, secs, 0, accuracy),
    lat: "40.0000000",
    lon: "-3.0000000",
  });

  /**
   * A wrong first point, `bad` imprecise ones (200 m) 5 s apart from t = 5 s, then `good` fine ones
   * (6 m) 5 s apart (L37 after a cold start).
   */
  const coldStartWrongFirst = (bad: number, good: number): StoredPoint[] => [
    wrong(1, 0, "20.00"),
    ...Array.from({ length: bad }, (_, i) => point(i + 2, (i + 1) * 5, (i % 3) * 4, "200.00")),
    ...Array.from({ length: good }, (_, i) => point(bad + i + 2, (bad + i + 1) * 5, i * 4, "6.00")),
  ];

  /** The kind of note after each point, as the points arrive. */
  const kindsAlong = (stored: StoredPoint[]) =>
    stored.map((_, n) => progressFrom(stored.slice(0, n + 1)).signal?.kind ?? "none");

  test("a wrong first point, then imprecise points: weak while the newest is imprecise", () => {
    expect(progressFrom(coldStartWrongFirst(12, 0)).signal).toEqual({
      kind: "weak",
      dropped: 12,
      stored: 13,
      accuracyM: 200,
    });
  });

  test("then good points: jumpy from the first good point on, although more were imprecise", () => {
    const progress = progressFrom(coldStartWrongFirst(12, 7));
    const analysis = vi.mocked(analyseTrack).mock.results[0]!.value as ReturnType<
      typeof analyseTrack
    >;
    // The older rule (the larger counter) said weak here, for a phone that was already outside.
    expect(analysis.skippedAccuracy).toBe(12);
    expect(analysis.skippedSpeed).toBe(7);
    expect(progress.signal).toEqual({ kind: "jumpy", dropped: 19, stored: 20 });
    expect(progressFrom(coldStartWrongFirst(12, 1)).signal).toEqual({
      kind: "jumpy",
      dropped: 13,
      stored: 14,
    });
  });

  test("point by point, from the stored texts: weak at 30 s, jumpy at the first good point", () => {
    const kinds = kindsAlong(coldStartWrongFirst(12, 8));
    expect(kinds.slice(0, 6)).toEqual(Array(6).fill("none")); // up to 25 s
    expect(kinds.slice(6, 13)).toEqual(Array(7).fill("weak")); // 30 s to 60 s
    expect(kinds.slice(13)).toEqual(Array(8).fill("jumpy")); // 65 s on
  });

  test("imprecise points, then good ones at the right place: weak, then nothing", () => {
    const stored = [
      ...imprecise(12, 5),
      ...Array.from({ length: 3 }, (_, i) => point(13 + i, 60 + i * 5, i * 4, "6.00")),
    ];
    const kinds = kindsAlong(stored);
    expect(kinds.slice(0, 6)).toEqual(Array(6).fill("none"));
    expect(kinds.slice(6, 12)).toEqual(Array(6).fill("weak"));
    // The first good point is the first that counts and none was dropped for its place: nothing.
    expect(kinds.slice(12)).toEqual(["none", "none", "none"]);
    expect(progressFrom(stored.slice(0, 13))).toMatchObject({ points: 13, metres: 0 });
  });

  test("the newest is the highest seq, not the last of the list: unsorted storage", () => {
    const stored = coldStartWrongFirst(12, 2); // the highest seq is a good point: jumpy
    const mixed = [stored[5]!, stored[14]!, stored[0]!, stored[13]!, ...stored.slice(1, 5)].concat(
      stored.slice(6, 13),
    );
    expect(mixed).toHaveLength(stored.length);
    expect(progressFrom(stored).signal?.kind).toBe("jumpy");
    expect(progressFrom([...stored].reverse()).signal?.kind).toBe("jumpy");
    expect(progressFrom(mixed).signal?.kind).toBe("jumpy");
    // And the other way: the highest seq is an imprecise point, a good one comes last in the list.
    const flipped = coldStartWrongFirst(12, 0);
    expect(progressFrom([...flipped].reverse()).signal?.kind).toBe("weak");
    expect(progressFrom([flipped[12]!, ...flipped.slice(0, 12)]).signal?.kind).toBe("weak");
  });

  test("the newest is by seq, not by time: a later seq with an earlier time stamp", () => {
    // seq 5 was stored last but carries an early time stamp (the clock went back).
    const stored = [
      wrong(1, 0, "20.00"),
      point(2, 20, 0, "6.00"),
      point(3, 40, 0, "6.00"),
      point(4, 60, 0, "6.00"),
      point(5, 10, 0, "200.00"),
    ];
    expect(progressFrom(stored).signal).toMatchObject({ kind: "weak", accuracyM: 200 });
    // With the radii swapped the newest by time is the imprecise one and the newest by seq is good.
    const swapped = [
      wrong(1, 0, "20.00"),
      point(2, 20, 0, "6.00"),
      point(3, 40, 0, "6.00"),
      point(4, 60, 0, "200.00"),
      point(5, 10, 0, "6.00"),
    ];
    expect(progressFrom(swapped).signal).toMatchObject({ kind: "jumpy" });
  });

  test("equal time stamps: the higher seq decides, in whatever order they are listed", () => {
    // seq 6 and seq 7 were taken at the same millisecond.
    const base = [
      wrong(1, 0, "20.00"),
      point(2, 10, 0, "200.00"),
      point(3, 20, 0, "200.00"),
      point(4, 30, 0, "200.00"),
      point(5, 40, 0, "200.00"),
    ];
    const imprecisePoint = (seq: number) => point(seq, 50, 0, "200.00");
    const finePoint = (seq: number) => point(seq, 50, 0, "6.00");
    const goodNewest = [...base, imprecisePoint(6), finePoint(7)];
    const impreciseNewest = [...base, finePoint(6), imprecisePoint(7)];
    expect(progressFrom(goodNewest).signal?.kind).toBe("jumpy");
    expect(progressFrom([...goodNewest].reverse()).signal?.kind).toBe("jumpy");
    expect(progressFrom(impreciseNewest).signal?.kind).toBe("weak");
    expect(progressFrom([...impreciseNewest].reverse()).signal?.kind).toBe("weak");
  });

  test("the radius named is the newest point's, from the two decimals of the store, in whole metres", () => {
    const stored = [
      wrong(1, 0, "20.00"),
      ...imprecise(7, 5).map((p) => ({ ...p, seq: p.seq + 1 })),
    ];
    const newest = stored.at(-1)!;
    stored[stored.length - 1] = { ...newest, accuracyM: "260.49" };
    expect(progressFrom(stored).signal).toMatchObject({ kind: "weak", accuracyM: 260 });
    stored[stored.length - 1] = { ...newest, accuracyM: "260.50" };
    expect(progressFrom(stored).signal).toMatchObject({ kind: "weak", accuracyM: 261 });
  });
});
