import { analyseTrack, MAX_ACCURACY_M, type Fix } from "@obop/trip-core";
import { describe, expect, test } from "vite-plus/test";
import { signalNote, type SignalNote } from "./signal.ts";
import { MAX_POINTS_PER_TRIP } from "./types.ts";

// Made-up places. About 0.00001 degrees of latitude are 1.11 m.
const T0 = Date.parse("2026-11-04T09:00:00Z");
const north = (metres: number) => 52 + metres / 111_195;

/** One fix; the time is in seconds after the start. */
const fix = (seq: number, secs: number, northM: number, accuracyM: number | null): Fix => ({
  seq,
  t: T0 + secs * 1000,
  lat: north(northM),
  lon: 13,
  accuracyM,
});

/** Numbers the fixes from 1 and puts them 2 s apart, whatever `secs` they had. */
const sequence = (fixes: readonly Fix[]): Fix[] =>
  fixes.map((f, i) => ({ ...f, seq: i + 1, t: T0 + i * 2000 }));

/** Indoors: every position says its radius is 150 m, and they wander a few metres. */
const indoor = (n: number): Fix[] =>
  sequence(Array.from({ length: n }, (_, i) => fix(0, 0, (i % 3) * 4, 150)));

/** A good first position, then `jumps` positions 3 km away within seconds and `bad` positions with a radius of 150 m. */
const anchored = (bad: number, jumps: number): Fix[] => {
  const dropped: Fix[] = [
    ...Array.from({ length: jumps }, () => fix(0, 0, 3000, 5)),
    ...Array.from({ length: bad }, () => fix(0, 0, 0, 150)),
  ];
  return sequence([fix(0, 0, 0, 5), ...dropped]);
};

/** A trip whose first position is far from all the others (L37). */
const farFirst = (later: number): Fix[] => {
  const first: Fix = { seq: 1, t: T0, lat: 40, lon: -3, accuracyM: 20 };
  const rest = Array.from({ length: later }, (_, i) => fix(i + 2, 5 + i * 5, i * 10, 6));
  return [first, ...rest];
};

/** A real drive: 12 m/s northwards, one fix a second, a good radius. */
const drive = (n: number): Fix[] =>
  Array.from({ length: n }, (_, i) => fix(i + 1, i, i * 12, 5 + (i % 4)));

/** The same place, the same radius, two seconds apart. */
const standing = (n: number, accuracyM: number | null = 8): Fix[] =>
  Array.from({ length: n }, (_, i) => fix(i + 1, i * 2, 0, accuracyM));

/** The note for a list of fixes, through the real rule of `analyseTrack`. */
function noteFor(fixes: readonly Fix[]): SignalNote | null {
  const newest = fixes.reduce<Fix | null>((a, f) => (a === null || f.seq > a.seq ? f : a), null);
  return signalNote({
    analysis: analyseTrack(fixes),
    stored: fixes.length,
    newestAccuracyM: newest === null ? null : newest.accuracyM,
  });
}

describe("the lists behind the tests are what they claim to be", () => {
  // The notes below rest on the counters of `analyseTrack`; this fails first if a list drifts.
  test("the counters of each list", () => {
    const counters = (fixes: Fix[]) => {
      const { accepted, skippedAccuracy, skippedSpeed } = analyseTrack(fixes);
      return { accepted, skippedAccuracy, skippedSpeed };
    };
    expect(counters(indoor(7))).toEqual({ accepted: 0, skippedAccuracy: 7, skippedSpeed: 0 });
    expect(counters(farFirst(8))).toEqual({ accepted: 1, skippedAccuracy: 0, skippedSpeed: 8 });
    expect(counters(drive(20))).toEqual({ accepted: 20, skippedAccuracy: 0, skippedSpeed: 0 });
    expect(counters(standing(2))).toEqual({ accepted: 2, skippedAccuracy: 0, skippedSpeed: 0 });
    expect(counters(anchored(2, 3))).toEqual({ accepted: 1, skippedAccuracy: 2, skippedSpeed: 3 });
    expect(analyseTrack(standing(2)).distanceM).toBe(0);
  });
});

describe("the four trips of the field", () => {
  test("indoors, every position is too imprecise: weak, with the radius of the newest", () => {
    expect(noteFor(indoor(7))).toEqual({ kind: "weak", dropped: 7, stored: 7, accuracyM: 150 });
  });

  test("a far-off first position makes every later one jump (L37): jumpy", () => {
    const analysis = analyseTrack(farFirst(8));
    expect(analysis.distanceM).toBe(0);
    expect(noteFor(farFirst(8))).toEqual({ kind: "jumpy", dropped: 8, stored: 9 });
  });

  test("a good drive has no note", () => {
    expect(analyseTrack(drive(20)).distanceM).toBeGreaterThan(200);
    expect(noteFor(drive(20))).toBeNull();
  });

  test("two good positions at one place are 0 m and have no note", () => {
    const analysis = analyseTrack(standing(2));
    expect(analysis.accepted).toBe(2);
    expect(analysis.distanceM).toBe(0);
    expect(noteFor(standing(2))).toBeNull();
    // Nor does a longer wait at one place.
    expect(noteFor(standing(40))).toBeNull();
  });
});

describe("how many positions are stored", () => {
  // Everything is dropped in each list, so only the number stored decides.
  const cases: [number, SignalNote | null][] = [
    [0, null],
    [1, null],
    [2, null],
    [3, { kind: "weak", dropped: 3, stored: 3, accuracyM: 150 }],
    [4, { kind: "weak", dropped: 4, stored: 4, accuracyM: 150 }],
  ];
  test.each(cases)("%i stored", (n, expected) => {
    expect(noteFor(indoor(n))).toEqual(expected);
  });

  test("two stored positions that jump are still too few to say anything", () => {
    expect(analyseTrack(anchored(0, 1)).skippedSpeed).toBe(1);
    expect(noteFor(anchored(0, 1))).toBeNull();
    expect(noteFor(anchored(0, 2))).toEqual({ kind: "jumpy", dropped: 2, stored: 3 });
  });

  test("the number stored is the one that is passed in, not the sum of the counters", () => {
    const analysis = analyseTrack(indoor(2));
    expect(signalNote({ analysis, stored: 2, newestAccuracyM: 150 })).toBeNull();
    expect(signalNote({ analysis, stored: 3, newestAccuracyM: 150 })).toEqual({
      kind: "weak",
      dropped: 2,
      stored: 3,
      accuracyM: 150,
    });
  });
});

describe("how many positions count", () => {
  test("one: the note says so, weak or jumpy as the counters say", () => {
    expect(noteFor(anchored(2, 0))).toEqual({
      kind: "weak",
      dropped: 2,
      stored: 3,
      accuracyM: 150,
    });
    expect(noteFor(anchored(0, 2))).toEqual({ kind: "jumpy", dropped: 2, stored: 3 });
  });

  test("two: the distance is a distance, however many are dropped around them", () => {
    // Two that count (an anchor and a close second), then dropped ones.
    const two: Fix[] = [
      fix(1, 0, 0, 5),
      fix(2, 2, 10, 5),
      ...Array.from({ length: 6 }, (_, i) => fix(3 + i, 4 + i * 2, 0, 150)),
    ];
    const analysis = analyseTrack(two);
    expect(analysis.accepted).toBe(2);
    expect(analysis.skippedAccuracy).toBe(6);
    expect(noteFor(two)).toBeNull();
    // And with jumps as well.
    const jumpy: Fix[] = [...two, fix(9, 20, 5000, 5), fix(10, 22, 5000, 5)];
    expect(analyseTrack(jumpy).skippedSpeed).toBe(2);
    expect(noteFor(jumpy)).toBeNull();
  });

  test("three or more are no case either", () => {
    expect(noteFor(standing(3))).toBeNull();
    expect(noteFor(drive(3))).toBeNull();
  });
});

describe("weak or jumpy", () => {
  // [dropped for accuracy, dropped for speed], with one position that counts as the anchor.
  const cases: [number, number, SignalNote | null][] = [
    [1, 0, null], // two stored: too few
    [3, 0, { kind: "weak", dropped: 3, stored: 4, accuracyM: 150 }],
    [0, 3, { kind: "jumpy", dropped: 3, stored: 4 }],
    [2, 2, { kind: "weak", dropped: 2, stored: 5, accuracyM: 150 }], // a tie is weak
    [1, 1, { kind: "weak", dropped: 1, stored: 3, accuracyM: 150 }],
    [3, 2, { kind: "weak", dropped: 3, stored: 6, accuracyM: 150 }],
    [2, 3, { kind: "jumpy", dropped: 3, stored: 6 }],
    [1, 5, { kind: "jumpy", dropped: 5, stored: 7 }],
    [5, 1, { kind: "weak", dropped: 5, stored: 7, accuracyM: 150 }],
  ];
  test.each(cases)("%i imprecise and %i too fast", (bad, jumps, expected) => {
    // `anchored` puts the jumps first and the imprecise ones last: the newest is imprecise, or a
    // jump when there is no imprecise one.
    const fixes = anchored(bad, jumps);
    const analysis = analyseTrack(fixes);
    expect(analysis.accepted).toBe(1);
    expect(analysis.skippedAccuracy).toBe(bad);
    expect(analysis.skippedSpeed).toBe(jumps);
    expect(noteFor(fixes)).toEqual(expected);
  });

  test("the number in the sentence is the one that decided the kind", () => {
    const weak = noteFor(anchored(4, 3));
    expect(weak).toMatchObject({ kind: "weak", dropped: 4, stored: 8 });
    const jumpy = noteFor(anchored(3, 4));
    expect(jumpy).toMatchObject({ kind: "jumpy", dropped: 4, stored: 8 });
  });

  test("the order of the dropped positions does not change the kind", () => {
    // The imprecise ones first, then the jumps.
    const reordered = sequence([
      fix(0, 0, 0, 5),
      fix(0, 0, 0, 150),
      fix(0, 0, 0, 150),
      fix(0, 0, 3000, 5),
      fix(0, 0, 3000, 5),
      fix(0, 0, 3000, 5),
    ]);
    const analysis = analyseTrack(reordered);
    expect([analysis.accepted, analysis.skippedAccuracy, analysis.skippedSpeed]).toEqual([1, 2, 3]);
    expect(noteFor(reordered)).toEqual({ kind: "jumpy", dropped: 3, stored: 6 });
  });
});

describe("the accuracy of the newest position", () => {
  const analysis = analyseTrack(indoor(4));
  const weakWith = (newestAccuracyM: number | null | undefined) =>
    signalNote({ analysis, stored: 4, newestAccuracyM });

  test("is given in whole metres", () => {
    expect(weakWith(150)).toMatchObject({ accuracyM: 150 });
    expect(weakWith(150.4)).toMatchObject({ accuracyM: 150 });
    expect(weakWith(150.5)).toMatchObject({ accuracyM: 151 });
    expect(weakWith(1234.6)).toMatchObject({ accuracyM: 1235 });
  });

  test("is left out when it is not known", () => {
    expect(weakWith(null)).toEqual({ kind: "weak", dropped: 4, stored: 4, accuracyM: null });
    expect(weakWith(undefined)).toEqual({ kind: "weak", dropped: 4, stored: 4, accuracyM: null });
    expect(weakWith(Number.NaN)).toMatchObject({ accuracyM: null });
    expect(weakWith(Number.POSITIVE_INFINITY)).toMatchObject({ accuracyM: null });
  });

  test("is left out when it is zero or negative (CoreLocation: not valid)", () => {
    expect(weakWith(0)).toMatchObject({ accuracyM: null });
    expect(weakWith(-1)).toMatchObject({ accuracyM: null });
    expect(weakWith(-150)).toMatchObject({ accuracyM: null });
  });

  test("is left out when the newest position is not itself over the limit the rule drops at", () => {
    // 100 m is still accepted by the rule; "20 m off" beside "too imprecise" would contradict it.
    expect(weakWith(MAX_ACCURACY_M)).toMatchObject({ accuracyM: null });
    expect(weakWith(99.9)).toMatchObject({ accuracyM: null });
    expect(weakWith(20)).toMatchObject({ accuracyM: null });
    expect(weakWith(MAX_ACCURACY_M + 0.01)).toMatchObject({ accuracyM: MAX_ACCURACY_M });
    expect(weakWith(MAX_ACCURACY_M + 1)).toMatchObject({ accuracyM: MAX_ACCURACY_M + 1 });
  });

  test("the limit is the one of the rule: positions at the limit count, positions over it do not", () => {
    // If the rule moved, the note would follow it, because the counters come from the rule.
    expect(noteFor(standing(3, MAX_ACCURACY_M))).toBeNull();
    expect(noteFor(standing(3, MAX_ACCURACY_M + 0.01))).toEqual({
      kind: "weak",
      dropped: 3,
      stored: 3,
      accuracyM: MAX_ACCURACY_M,
    });
  });

  test("a jumpy note carries no radius", () => {
    const jumpy = signalNote({
      analysis: analyseTrack(farFirst(5)),
      stored: 6,
      newestAccuracyM: 500,
    });
    expect(jumpy).toEqual({ kind: "jumpy", dropped: 5, stored: 6 });
  });

  test("a newest position without a radius, in a trip that is mostly imprecise", () => {
    const fixes = sequence([
      fix(0, 0, 0, 5),
      fix(0, 0, 0, 150),
      fix(0, 0, 0, 150),
      fix(0, 0, 3000, null),
    ]);
    const counted = analyseTrack(fixes);
    expect([counted.accepted, counted.skippedAccuracy, counted.skippedSpeed]).toEqual([1, 2, 1]);
    expect(noteFor(fixes)).toEqual({ kind: "weak", dropped: 2, stored: 4, accuracyM: null });
  });
});

describe("a trip of the largest size", () => {
  test("20,000 positions, all of them imprecise", () => {
    const fixes = indoor(MAX_POINTS_PER_TRIP);
    expect(noteFor(fixes)).toEqual({
      kind: "weak",
      dropped: MAX_POINTS_PER_TRIP,
      stored: MAX_POINTS_PER_TRIP,
      accuracyM: 150,
    });
  });

  test("20,000 positions, one that counts and all the others jumping", () => {
    const first: Fix = { seq: 1, t: T0, lat: 40, lon: -3, accuracyM: 20 };
    const rest = Array.from({ length: MAX_POINTS_PER_TRIP - 1 }, (_, i) => fix(i + 2, 5 + i, 0, 6));
    const fixes = [first, ...rest];
    // 1,800 km in 19,999 s would be 90 m/s: still too fast for every one of them.
    expect(analyseTrack(fixes).skippedSpeed).toBe(MAX_POINTS_PER_TRIP - 1);
    expect(noteFor(fixes)).toEqual({
      kind: "jumpy",
      dropped: MAX_POINTS_PER_TRIP - 1,
      stored: MAX_POINTS_PER_TRIP,
    });
  });

  test("counts far beyond that are passed through", () => {
    const huge = Number.MAX_SAFE_INTEGER;
    expect(
      signalNote({
        analysis: { accepted: 0, skippedAccuracy: huge - 1, skippedSpeed: 1 },
        stored: huge,
        newestAccuracyM: 150,
      }),
    ).toEqual({ kind: "weak", dropped: huge - 1, stored: huge, accuracyM: 150 });
  });
});

describe("what the function does not do", () => {
  test("nothing was dropped: the phone simply delivered few positions", () => {
    // Fewer than 2 count and nothing was dropped cannot come out of one analysis of 3 or more
    // positions; it can when the number stored is ahead of the positions analysed.
    const analysis = analyseTrack(standing(1));
    expect(analysis).toMatchObject({ accepted: 1, skippedAccuracy: 0, skippedSpeed: 0 });
    expect(signalNote({ analysis, stored: 3, newestAccuracyM: 8 })).toBeNull();
    expect(signalNote({ analysis, stored: 20_000, newestAccuracyM: 150 })).toBeNull();
  });

  test("counters that are not numbers say nothing", () => {
    const nan = Number.NaN;
    expect(
      signalNote({
        analysis: { accepted: nan, skippedAccuracy: 3, skippedSpeed: 0 },
        stored: 3,
        newestAccuracyM: 150,
      }),
    ).toBeNull();
    expect(
      signalNote({
        analysis: { accepted: 0, skippedAccuracy: 3, skippedSpeed: 0 },
        stored: nan,
        newestAccuracyM: 150,
      }),
    ).toBeNull();
  });

  test("it reads the counters it is given and leaves the analysis alone", () => {
    const analysis = analyseTrack(indoor(5));
    const before = JSON.stringify(analysis);
    signalNote({ analysis, stored: 5, newestAccuracyM: 150 });
    expect(JSON.stringify(analysis)).toBe(before);
  });
});
