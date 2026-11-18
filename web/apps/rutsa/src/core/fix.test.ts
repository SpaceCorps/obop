import { describe, expect, test } from "vite-plus/test";
import { plausible, toFix, type Position } from "./fix.ts";
import type { NewFix } from "./types.ts";

const at = (coords: Partial<Position["coords"]>): Position => ({
  timestamp: 1_790_000_000_000,
  coords: { latitude: 59.33, longitude: 18.06, accuracy: 5, speed: 1.4, ...coords },
});

describe("a position of the location service as a fix", () => {
  test("keeps what the platform gave", () => {
    expect(toFix(at({}))).toEqual({
      t: 1_790_000_000_000,
      lat: 59.33,
      lon: 18.06,
      accuracyM: 5,
      speedMps: 1.4,
    });
  });

  test("a negative speed is CoreLocation saying it has none, so it is no speed", () => {
    expect(toFix(at({ speed: -1 })).speedMps).toBeNull();
    expect(toFix(at({ speed: -0.01 })).speedMps).toBeNull();
    expect(toFix(at({ speed: Number.NaN })).speedMps).toBeNull();
    expect(toFix(at({ speed: Number.POSITIVE_INFINITY })).speedMps).toBeNull();
    expect(toFix(at({ speed: null })).speedMps).toBeNull();
    // A standing phone has a speed of zero, and that is a speed.
    expect(toFix(at({ speed: 0 })).speedMps).toBe(0);
  });

  test("a negative accuracy is passed on, and the fix is then not plausible", () => {
    const invalid = toFix(at({ accuracy: -1 }));
    expect(invalid.accuracyM).toBe(-1);
    expect(plausible(invalid)).toBe(false);
  });
});

describe("a plausible fix", () => {
  const fix = (values: Partial<NewFix>): NewFix => ({
    t: 1_790_000_000_000,
    lat: 59.33,
    lon: 18.06,
    accuracyM: 5,
    speedMps: null,
    ...values,
  });

  test("a fix with or without an accuracy, on the edges of the map too, is plausible", () => {
    expect(plausible(fix({}))).toBe(true);
    expect(plausible(fix({ accuracyM: null }))).toBe(true);
    expect(plausible(fix({ accuracyM: 0 }))).toBe(true);
    expect(plausible(fix({ lat: 90, lon: -180 }))).toBe(true);
    expect(plausible(fix({ lat: -90, lon: 180 }))).toBe(true);
  });

  test("a place off the Earth, a number that is not one, or a bad accuracy is not", () => {
    expect(plausible(fix({ lat: 90.0001 }))).toBe(false);
    expect(plausible(fix({ lon: -180.0001 }))).toBe(false);
    expect(plausible(fix({ lat: Number.NaN }))).toBe(false);
    expect(plausible(fix({ lon: Number.POSITIVE_INFINITY }))).toBe(false);
    expect(plausible(fix({ t: Number.NaN }))).toBe(false);
    expect(plausible(fix({ accuracyM: -0.5 }))).toBe(false);
    expect(plausible(fix({ accuracyM: Number.NaN }))).toBe(false);
  });
});
