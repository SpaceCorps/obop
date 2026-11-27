import { MAX_ACCURACY_M } from "@obop/trip-core";
import { describe, expect, test } from "vite-plus/test";
import { MIN_SPAN_MS, signalNote } from "./signal.ts";

/** A trip of 5 positions over a minute, none of which counts, all dropped for their accuracy. */
const BASE = {
  accepted: 0,
  skippedAccuracy: 5,
  skippedSpeed: 0,
  stored: 5,
  spanMs: 60_000,
  newestAccuracyM: 250,
};

describe("why the distance stays at 0 km", () => {
  test("a weak signal: the newest position is over the accuracy limit", () => {
    expect(signalNote(BASE)).toEqual({ kind: "weak", dropped: 5, stored: 5, accuracyM: 250 });
  });

  test("positions that do not fit: dropped for their speed, the newest one is precise", () => {
    expect(
      signalNote({ ...BASE, skippedAccuracy: 0, skippedSpeed: 4, newestAccuracyM: 5 }),
    ).toEqual({ kind: "jumpy", dropped: 4, stored: 5 });
  });

  test("the kind follows the newest position: a precise one after weak ones says nothing", () => {
    expect(signalNote({ ...BASE, newestAccuracyM: 8 })).toBeNull();
    expect(signalNote({ ...BASE, newestAccuracyM: null })).toBeNull();
  });

  test("nothing while the phone is still finding its position", () => {
    expect(signalNote({ ...BASE, stored: 2 })).toBeNull();
    expect(signalNote({ ...BASE, spanMs: MIN_SPAN_MS - 1 })).toBeNull();
  });

  test("nothing once two positions count: the distance is a distance", () => {
    expect(signalNote({ ...BASE, accepted: 2 })).toBeNull();
  });

  test("nothing when no position was dropped: the phone simply delivered few", () => {
    expect(signalNote({ ...BASE, skippedAccuracy: 0 })).toBeNull();
  });

  test("a radius exactly at the limit is within it", () => {
    expect(signalNote({ ...BASE, newestAccuracyM: MAX_ACCURACY_M })).toBeNull();
  });

  test("a count that is not a number says nothing", () => {
    expect(signalNote({ ...BASE, stored: Number.NaN })).toBeNull();
    expect(signalNote({ ...BASE, accepted: Number.NaN })).toBeNull();
  });

  test("a radius that is not finite is shown as unknown", () => {
    expect(signalNote({ ...BASE, newestAccuracyM: Number.POSITIVE_INFINITY })).toMatchObject({
      kind: "weak",
      accuracyM: null,
    });
  });
});
