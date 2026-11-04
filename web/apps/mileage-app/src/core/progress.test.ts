import { analyseTrack } from "@obop/trip-core";
import { beforeEach, describe, expect, test, vi } from "vite-plus/test";
import { progressFrom } from "./progress.ts";
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

beforeEach(() => {
  vi.mocked(analyseTrack).mockClear();
});

describe("the count of a trip from its stored points", () => {
  test("a trip without points: nothing counted, nothing said", () => {
    expect(progressFrom([])).toEqual({ points: 0, metres: 0, lastFixMs: null, signal: null });
  });

  test("a drive: the distance of the points that count, no note", () => {
    const drive = Array.from({ length: 10 }, (_, i) => point(i + 1, i, i * 12, "5.00"));
    const progress = progressFrom(drive);
    expect(progress.points).toBe(10);
    expect(progress.metres).toBeGreaterThan(100);
    expect(progress.lastFixMs).toBe(T0 + 9000);
    expect(progress.signal).toBeNull();
  });

  test("indoors, where every radius is 150 m: 0 m, all points stored, and the note says why", () => {
    const indoor = Array.from({ length: 7 }, (_, i) => point(i + 1, i * 2, (i % 3) * 4, "150.00"));
    expect(progressFrom(indoor)).toEqual({
      points: 7,
      metres: 0,
      lastFixMs: T0 + 12_000,
      signal: { kind: "weak", dropped: 7, stored: 7, accuracyM: 150 },
    });
  });

  test("a first point far away: 0 m and jumpy", () => {
    const first: StoredPoint = { ...point(1, 0, 0, "20.00"), lat: "40.0000000", lon: "-3.0000000" };
    const later = Array.from({ length: 4 }, (_, i) => point(i + 2, 5 + i * 5, i * 10, "6.00"));
    expect(progressFrom([first, ...later])).toMatchObject({
      points: 5,
      metres: 0,
      signal: { kind: "jumpy", dropped: 4, stored: 5 },
    });
  });

  test("two good points at one place: 0 m and no note", () => {
    const progress = progressFrom([point(1, 0, 0, "8.00"), point(2, 2, 0, "8.00")]);
    expect(progress).toMatchObject({ points: 2, metres: 0, signal: null });
  });

  test("the radius shown is the one of the newest point by seq, whatever the order of the list", () => {
    const first = point(1, 0, 0, "150.00");
    const second = point(2, 2, 0, "180.00");
    const third = point(3, 4, 0, "260.00");
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
      point(2, 2, 0, "150.00"),
      point(3, 4, 0, "150.00"),
      point(4, 6, 0, null),
    ];
    expect(progressFrom(stored).signal).toEqual({
      kind: "weak",
      dropped: 3,
      stored: 4,
      accuracyM: null,
    });
  });

  test("a stored radius of 0.00 is not a radius to name", () => {
    // A good point, two too imprecise, and a newest one that is 3 km away within 6 s.
    const stored = [
      point(1, 0, 0, "5.00"),
      point(2, 2, 0, "150.00"),
      point(3, 4, 0, "150.00"),
      point(4, 6, 3000, "0.00"),
    ];
    expect(progressFrom(stored).signal).toEqual({
      kind: "weak",
      dropped: 2,
      stored: 4,
      accuracyM: null,
    });
  });

  test("the track is analysed once, and the distance and the note come from that analysis", () => {
    const indoor = Array.from({ length: 5 }, (_, i) => point(i + 1, i * 2, 0, "150.00"));
    const progress = progressFrom(indoor);
    expect(analyseTrack).toHaveBeenCalledTimes(1);
    const analysis = vi.mocked(analyseTrack).mock.results[0]!.value as ReturnType<
      typeof analyseTrack
    >;
    expect(progress.metres).toBe(analysis.distanceM);
    expect(progress.signal).toMatchObject({ dropped: analysis.skippedAccuracy });
  });
});
