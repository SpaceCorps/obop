import { describe, expect, test } from "vite-plus/test";
import { toFix, type Position } from "./fix.ts";
import { plausible } from "./values.ts";

const at = (coords: Partial<Position["coords"]>): Position => ({
  timestamp: 1_790_000_000_000,
  coords: { latitude: 60.17, longitude: 24.94, accuracy: 5, speed: 1.4, ...coords },
});

describe("a position of the location service as a fix", () => {
  test("keeps what the platform gave", () => {
    expect(toFix(at({}))).toEqual({
      t: 1_790_000_000_000,
      lat: 60.17,
      lon: 24.94,
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

  test("a negative accuracy is passed on as it is and the outbox then refuses the position", () => {
    const invalid = toFix(at({ accuracy: -1 }));
    expect(invalid.accuracyM).toBe(-1);
    expect(plausible(invalid)).toBe(false);
    expect(plausible(toFix(at({ accuracy: null })))).toBe(true);
  });
});
