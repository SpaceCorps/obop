import { analyseTrack, MAX_ACCURACY_M, type Fix } from "@obop/trip-core";
import { describe, expect, test } from "vite-plus/test";
import { MIN_SPAN_MS, signalNote, type SignalNote } from "./signal.ts";
import { MAX_POINTS_PER_TRIP } from "./types.ts";

// Made-up places. About 0.00001 degrees of latitude are 1.11 m.
const T0 = Date.parse("2026-11-04T09:00:00Z");
const north = (metres: number) => 52 + metres / 111_195;

/** A place so far away that no later position within hours can be reached at the speed limit. */
const FAR_M = 500_000;

/** Seconds between two positions of the lists below: three of them span 40 s, more than `MIN_SPAN_MS`. */
const STEP_S = 20;

/** One fix; the time is in seconds after the start. */
const fix = (seq: number, secs: number, northM: number, accuracyM: number | null): Fix => ({
  seq,
  t: T0 + secs * 1000,
  lat: north(northM),
  lon: 13,
  accuracyM,
});

/** Numbers the fixes from 1 and puts them `STEP_S` seconds apart, whatever `secs` they had. */
const sequence = (fixes: readonly Fix[]): Fix[] =>
  fixes.map((f, i) => ({ ...f, seq: i + 1, t: T0 + i * STEP_S * 1000 }));

/** Indoors: every position says its radius is 150 m, and they wander a few metres. */
const indoor = (n: number): Fix[] =>
  sequence(Array.from({ length: n }, (_, i) => fix(0, 0, (i % 3) * 4, 150)));

/** A good first position, then `jumps` positions far away and `bad` positions with a radius of 150 m. */
const anchored = (bad: number, jumps: number): Fix[] => {
  const dropped: Fix[] = [
    ...Array.from({ length: jumps }, () => fix(0, 0, FAR_M, 5)),
    ...Array.from({ length: bad }, () => fix(0, 0, 0, 150)),
  ];
  return sequence([fix(0, 0, 0, 5), ...dropped]);
};

/** A trip whose first position is far from all the others (L37), the others 5 s apart. */
const farFirst = (later: number): Fix[] => {
  const first: Fix = { seq: 1, t: T0, lat: 40, lon: -3, accuracyM: 20 };
  const rest = Array.from({ length: later }, (_, i) => fix(i + 2, 5 + i * 5, i * 10, 6));
  return [first, ...rest];
};

/** A real drive: 12 m/s northwards, one fix every 2 s, a good radius. */
const drive = (n: number): Fix[] =>
  Array.from({ length: n }, (_, i) => fix(i + 1, i * 2, i * 24, 5 + (i % 4)));

/** The same place, the same radius, `STEP_S` seconds apart. */
const standing = (n: number, accuracyM: number | null = 8): Fix[] =>
  Array.from({ length: n }, (_, i) => fix(i + 1, i * STEP_S, 0, accuracyM));

/** The newest time stamp minus the oldest one. */
function spanOf(fixes: readonly Fix[]): number {
  if (fixes.length === 0) return 0;
  const times = fixes.map((f) => f.t);
  return times.reduce((a, b) => Math.max(a, b)) - times.reduce((a, b) => Math.min(a, b));
}

/** The note for a list of fixes, through the real rule of `analyseTrack`. */
function noteFor(fixes: readonly Fix[]): SignalNote | null {
  const newest = fixes.reduce<Fix | null>((a, f) => (a === null || f.seq > a.seq ? f : a), null);
  return signalNote({
    analysis: analyseTrack(fixes),
    stored: fixes.length,
    spanMs: spanOf(fixes),
    newestAccuracyM: newest === null ? null : newest.accuracyM,
  });
}

/**
 * The note for a list that has enough positions and spans enough time, so that a `null` is the
 * verdict of the counters and not of the two minimums in front of them.
 */
function noteForLongTrack(fixes: readonly Fix[]): SignalNote | null {
  expect(fixes.length).toBeGreaterThanOrEqual(3);
  expect(spanOf(fixes)).toBeGreaterThanOrEqual(MIN_SPAN_MS);
  return noteFor(fixes);
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

  test("the spans of the lists", () => {
    expect(spanOf([])).toBe(0);
    expect(spanOf(indoor(3))).toBe(2 * STEP_S * 1000);
    expect(spanOf(indoor(3))).toBeGreaterThanOrEqual(MIN_SPAN_MS);
    expect(spanOf(drive(20))).toBeGreaterThanOrEqual(MIN_SPAN_MS);
    expect(spanOf(farFirst(8))).toBe(40_000);
  });
});

describe("the four trips of the field", () => {
  test("indoors, every position is too imprecise: weak, with the radius of the newest", () => {
    expect(noteFor(indoor(7))).toEqual({ kind: "weak", dropped: 7, stored: 7, accuracyM: 150 });
  });

  test("a far-off first position makes every later one unfit (L37): jumpy", () => {
    const analysis = analyseTrack(farFirst(8));
    expect(analysis.distanceM).toBe(0);
    expect(noteFor(farFirst(8))).toEqual({ kind: "jumpy", dropped: 8, stored: 9 });
  });

  test("a good drive has no note", () => {
    expect(analyseTrack(drive(20)).distanceM).toBeGreaterThan(200);
    expect(noteForLongTrack(drive(20))).toBeNull();
  });

  test("two good positions at one place are 0 m and have no note", () => {
    const analysis = analyseTrack(standing(2));
    expect(analysis.accepted).toBe(2);
    expect(analysis.distanceM).toBe(0);
    // The same two positions among imprecise ones: stored enough, spanning enough, and still none.
    const among = sequence([
      fix(0, 0, 0, 8),
      fix(0, 0, 0, 8),
      fix(0, 0, 0, 150),
      fix(0, 0, 0, 150),
    ]);
    expect(analyseTrack(among).accepted).toBe(2);
    expect(noteForLongTrack(among)).toBeNull();
    // Nor does a longer wait at one place.
    expect(noteForLongTrack(standing(40))).toBeNull();
  });
});

describe("how many positions are stored", () => {
  // Everything is dropped in each list, so only the number stored (and the span) decides.
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

  test("two stored positions that do not fit are still too few to say anything", () => {
    // The span is long enough here, so the number stored is what decides.
    const analysis = analyseTrack(anchored(0, 1));
    expect(analysis.skippedSpeed).toBe(1);
    expect(signalNote({ analysis, stored: 2, spanMs: MIN_SPAN_MS, newestAccuracyM: 5 })).toBeNull();
    expect(noteFor(anchored(0, 2))).toEqual({ kind: "jumpy", dropped: 2, stored: 3 });
  });

  test("the number stored is the one that is passed in, not the sum of the counters", () => {
    const analysis = analyseTrack(indoor(2));
    const input = { analysis, spanMs: MIN_SPAN_MS, newestAccuracyM: 150 };
    expect(signalNote({ ...input, stored: 2 })).toBeNull();
    expect(signalNote({ ...input, stored: 3 })).toEqual({
      kind: "weak",
      dropped: 2,
      stored: 3,
      accuracyM: 150,
    });
  });
});

describe("how long the stored positions span (the first seconds of an ordinary trip)", () => {
  const analysis = analyseTrack(indoor(5));
  const at = (spanMs: number) => signalNote({ analysis, stored: 5, spanMs, newestAccuracyM: 150 });
  const weak: SignalNote = { kind: "weak", dropped: 5, stored: 5, accuracyM: 150 };

  test("the minimum is 30 seconds", () => {
    expect(MIN_SPAN_MS).toBe(30_000);
  });

  test("less than that says nothing, exactly that or more says it", () => {
    expect(at(0)).toBeNull();
    expect(at(1)).toBeNull();
    expect(at(10_000)).toBeNull();
    expect(at(MIN_SPAN_MS - 1)).toBeNull();
    expect(at(MIN_SPAN_MS)).toEqual(weak);
    expect(at(MIN_SPAN_MS + 1)).toEqual(weak);
    expect(at(3_600_000)).toEqual(weak);
  });

  test("a span that is not a number, or negative, says nothing", () => {
    expect(at(Number.NaN)).toBeNull();
    expect(at(-MIN_SPAN_MS)).toBeNull();
    expect(at(Number.NEGATIVE_INFINITY)).toBeNull();
  });

  test("the span does not change which kind it is", () => {
    const jumpy = analyseTrack(anchored(0, 3));
    expect(
      signalNote({ analysis: jumpy, stored: 4, spanMs: MIN_SPAN_MS - 1, newestAccuracyM: 5 }),
    ).toBeNull();
    expect(
      signalNote({ analysis: jumpy, stored: 4, spanMs: MIN_SPAN_MS, newestAccuracyM: 5 }),
    ).toEqual({ kind: "jumpy", dropped: 3, stored: 4 });
  });

  test("a phone that is still finding its position: fixes 5 s apart, weak at 30 s and not before", () => {
    // Android, a fix every 5 s, every one with a radius of 150 m: 3 stored at t = 10 s used to be
    // enough for a note (observed in the review of the first version).
    const fives = (n: number): Fix[] =>
      Array.from({ length: n }, (_, i) => fix(i + 1, i * 5, i * 8, 150));
    expect(analyseTrack(fives(3)).accepted).toBe(0);
    expect(noteFor(fives(3))).toBeNull(); // t = 10 s
    expect(noteFor(fives(6))).toBeNull(); // t = 25 s
    expect(noteFor(fives(7))).toEqual({ kind: "weak", dropped: 7, stored: 7, accuracyM: 150 }); // t = 30 s
    expect(noteFor(fives(8))).toMatchObject({ kind: "weak" });
  });

  test("the same for a far-off first position (L37)", () => {
    expect(noteFor(farFirst(5))).toBeNull(); // 25 s
    expect(noteFor(farFirst(6))).toEqual({ kind: "jumpy", dropped: 6, stored: 7 }); // 30 s
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
    const two = sequence([
      fix(0, 0, 0, 5),
      fix(0, 0, 10, 5),
      ...Array.from({ length: 6 }, () => fix(0, 0, 0, 150)),
    ]);
    const analysis = analyseTrack(two);
    expect(analysis.accepted).toBe(2);
    expect(analysis.skippedAccuracy).toBe(6);
    expect(noteForLongTrack(two)).toBeNull();
    // And with jumps as well.
    const jumpy = sequence([...two, fix(0, 0, FAR_M, 5), fix(0, 0, FAR_M, 5)]);
    expect(analyseTrack(jumpy).skippedSpeed).toBe(2);
    expect(noteForLongTrack(jumpy)).toBeNull();
  });

  test("three or more are no case either", () => {
    expect(noteForLongTrack(standing(3))).toBeNull();
    expect(noteForLongTrack(drive(30))).toBeNull();
  });
});

describe("weak or jumpy", () => {
  // [dropped for accuracy, dropped for speed], with one position that counts as the anchor. The
  // number in the line is every position that was left out, whatever the reason.
  const cases: [number, number, SignalNote | null][] = [
    [1, 0, null], // two stored: too few
    [3, 0, { kind: "weak", dropped: 3, stored: 4, accuracyM: 150 }],
    [0, 3, { kind: "jumpy", dropped: 3, stored: 4 }],
    [2, 2, { kind: "weak", dropped: 4, stored: 5, accuracyM: 150 }], // a tie is weak
    [1, 1, { kind: "weak", dropped: 2, stored: 3, accuracyM: 150 }],
    [3, 2, { kind: "weak", dropped: 5, stored: 6, accuracyM: 150 }],
    [2, 3, { kind: "jumpy", dropped: 5, stored: 6 }],
    [1, 5, { kind: "jumpy", dropped: 6, stored: 7 }],
    [5, 1, { kind: "weak", dropped: 6, stored: 7, accuracyM: 150 }],
  ];
  test.each(cases)("%i imprecise and %i not fitting", (bad, jumps, expected) => {
    // `anchored` puts the jumps first and the imprecise ones last: the newest is imprecise, or a
    // jump when there is no imprecise one.
    const fixes = anchored(bad, jumps);
    const analysis = analyseTrack(fixes);
    expect(analysis.accepted).toBe(1);
    expect(analysis.skippedAccuracy).toBe(bad);
    expect(analysis.skippedSpeed).toBe(jumps);
    expect(noteFor(fixes)).toEqual(expected);
  });

  test("the kind is the more common reason, the number is every position left out", () => {
    expect(noteFor(anchored(4, 3))).toMatchObject({ kind: "weak", dropped: 7, stored: 8 });
    expect(noteFor(anchored(3, 4))).toMatchObject({ kind: "jumpy", dropped: 7, stored: 8 });
  });

  test("the order of the dropped positions does not change the kind", () => {
    // The imprecise ones first, then the jumps.
    const reordered = sequence([
      fix(0, 0, 0, 5),
      fix(0, 0, 0, 150),
      fix(0, 0, 0, 150),
      fix(0, 0, FAR_M, 5),
      fix(0, 0, FAR_M, 5),
      fix(0, 0, FAR_M, 5),
    ]);
    const analysis = analyseTrack(reordered);
    expect([analysis.accepted, analysis.skippedAccuracy, analysis.skippedSpeed]).toEqual([1, 2, 3]);
    expect(noteFor(reordered)).toEqual({ kind: "jumpy", dropped: 5, stored: 6 });
  });
});

describe("the accuracy of the newest position", () => {
  const analysis = analyseTrack(indoor(4));
  const weakWith = (newestAccuracyM: number | null | undefined) =>
    signalNote({ analysis, stored: 4, spanMs: MIN_SPAN_MS, newestAccuracyM });

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
    // 100 m is still accepted by the rule; "within 20 m" beside "imprecise" would contradict it.
    expect(weakWith(MAX_ACCURACY_M)).toMatchObject({ accuracyM: null });
    expect(weakWith(99.9)).toMatchObject({ accuracyM: null });
    expect(weakWith(20)).toMatchObject({ accuracyM: null });
    expect(weakWith(MAX_ACCURACY_M + 0.01)).toMatchObject({ accuracyM: MAX_ACCURACY_M });
    expect(weakWith(MAX_ACCURACY_M + 1)).toMatchObject({ accuracyM: MAX_ACCURACY_M + 1 });
  });

  test("the limit is the one of the rule: positions at the limit count, positions over it do not", () => {
    // If the rule moved, the note would follow it, because the counters come from the rule.
    expect(noteForLongTrack(standing(3, MAX_ACCURACY_M))).toBeNull();
    expect(noteForLongTrack(standing(3, MAX_ACCURACY_M + 0.01))).toEqual({
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
      spanMs: MIN_SPAN_MS,
      newestAccuracyM: 500,
    });
    expect(jumpy).toEqual({ kind: "jumpy", dropped: 5, stored: 6 });
  });

  test("a newest position without a radius, in a trip that is mostly imprecise", () => {
    const fixes = sequence([
      fix(0, 0, 0, 5),
      fix(0, 0, 0, 150),
      fix(0, 0, 0, 150),
      fix(0, 0, FAR_M, null),
    ]);
    const counted = analyseTrack(fixes);
    expect([counted.accepted, counted.skippedAccuracy, counted.skippedSpeed]).toEqual([1, 2, 1]);
    expect(noteFor(fixes)).toEqual({ kind: "weak", dropped: 3, stored: 4, accuracyM: null });
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

  test("20,000 positions, one that counts and all the others not fitting it", () => {
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
        spanMs: huge,
        newestAccuracyM: 150,
      }),
    ).toEqual({ kind: "weak", dropped: huge, stored: huge, accuracyM: 150 });
  });
});

describe("what the function does not do", () => {
  test("nothing was dropped: the phone simply delivered few positions", () => {
    // Fewer than 2 count and nothing was dropped cannot come out of one analysis of 3 or more
    // positions; it can when the number stored is ahead of the positions analysed.
    const analysis = analyseTrack(standing(1));
    expect(analysis).toMatchObject({ accepted: 1, skippedAccuracy: 0, skippedSpeed: 0 });
    const input = { analysis, spanMs: MIN_SPAN_MS };
    expect(signalNote({ ...input, stored: 3, newestAccuracyM: 8 })).toBeNull();
    expect(signalNote({ ...input, stored: 20_000, newestAccuracyM: 150 })).toBeNull();
  });

  test("counters that are not numbers say nothing", () => {
    const nan = Number.NaN;
    expect(
      signalNote({
        analysis: { accepted: nan, skippedAccuracy: 3, skippedSpeed: 0 },
        stored: 3,
        spanMs: MIN_SPAN_MS,
        newestAccuracyM: 150,
      }),
    ).toBeNull();
    expect(
      signalNote({
        analysis: { accepted: 0, skippedAccuracy: 3, skippedSpeed: 0 },
        stored: nan,
        spanMs: MIN_SPAN_MS,
        newestAccuracyM: 150,
      }),
    ).toBeNull();
  });

  test("it reads the counters it is given and leaves the analysis alone", () => {
    const analysis = analyseTrack(indoor(5));
    const before = JSON.stringify(analysis);
    signalNote({ analysis, stored: 5, spanMs: MIN_SPAN_MS, newestAccuracyM: 150 });
    expect(JSON.stringify(analysis)).toBe(before);
  });
});
