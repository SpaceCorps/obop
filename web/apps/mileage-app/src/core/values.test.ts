import { describe, expect, test } from "vite-plus/test";
import {
  clock,
  fixed,
  instant,
  plausible,
  pointRow,
  pointsKey,
  toStored,
  tripKey,
  uuidV4,
} from "./values.ts";

describe("uuidV4", () => {
  test("sets the version and the variant and nothing else", () => {
    expect(uuidV4(new Uint8Array(16))).toBe("00000000-0000-4000-8000-000000000000");
    expect(uuidV4(new Uint8Array(16).fill(255))).toBe("ffffffff-ffff-4fff-bfff-ffffffffffff");
  });

  test("looks like a random UUID of version 4", () => {
    const bytes = new Uint8Array(16).map((_, i) => (i * 37 + 11) % 256);
    expect(uuidV4(bytes)).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/,
    );
  });

  test("needs 16 bytes", () => {
    expect(() => uuidV4(new Uint8Array(8))).toThrow(RangeError);
  });
});

describe("texts", () => {
  test("fixed has the scale asked and no minus on a zero", () => {
    expect(fixed(52.123456789, 7)).toBe("52.1234568");
    expect(fixed(-0.00000001, 7)).toBe("0.0000000");
    expect(fixed(-13.5, 7)).toBe("-13.5000000");
    expect(fixed(5, 2)).toBe("5.00");
  });

  test("an instant has milliseconds and the Z", () => {
    expect(instant(Date.parse("2026-10-15T09:00:00.123Z"))).toBe("2026-10-15T09:00:00.123Z");
  });

  test("a stored point keeps what the server can hold and drops what it cannot", () => {
    const p = toStored(3, { t: 1, lat: 52, lon: 13, accuracyM: 1e7, speedMps: 2e5 });
    expect(p).toEqual({
      seq: 3,
      t: 1,
      lat: "52.0000000",
      lon: "13.0000000",
      accuracyM: null,
      speedMps: null,
    });
    expect(
      toStored(1, { t: 1, lat: 1, lon: 1, accuracyM: 999999.99, speedMps: 99999.99 }),
    ).toMatchObject({
      accuracyM: "999999.99",
      speedMps: "99999.99",
    });
  });

  test("a negative speed or accuracy is no number to store (CoreLocation says -1 for none)", () => {
    const p = toStored(1, { t: 1, lat: 52, lon: 13, accuracyM: -1, speedMps: -1 });
    expect(p.accuracyM).toBeNull();
    expect(p.speedMps).toBeNull();
    // Zero is a value.
    const zero = toStored(1, { t: 1, lat: 52, lon: 13, accuracyM: 0, speedMps: 0 });
    expect(zero.accuracyM).toBe("0.00");
    expect(zero.speedMps).toBe("0.00");
  });

  test("a row leaves out the fields the phone did not have", () => {
    const row = pointRow(
      "t-1",
      toStored(2, { t: 0, lat: 1, lon: 2, accuracyM: null, speedMps: null }),
    );
    expect(row).toEqual({
      trip: "t-1",
      seq: 2,
      recorded_at: "1970-01-01T00:00:00.000Z",
      lat: "1.0000000",
      lon: "2.0000000",
    });
  });

  test("the keys name the trip and the range, and fit the 128 characters of the server", () => {
    const id = "123e4567-e89b-42d3-a456-426614174000";
    expect(tripKey(id)).toBe(`trip:${id}`);
    expect(pointsKey(id, 1001, 2000)).toBe(`points:${id}:1001-2000`);
    expect(pointsKey(id, 19001, 20000).length).toBeLessThanOrEqual(128);
    expect(pointsKey(id, 1, 1)).toMatch(/^[\x21-\x7e]+$/);
  });

  test("a time span reads as a clock", () => {
    expect(clock(0)).toBe("00:00");
    expect(clock(65_000)).toBe("01:05");
    expect(clock(3_725_000)).toBe("1:02:05");
    expect(clock(-5)).toBe("00:00");
  });

  test("plausible positions", () => {
    expect(plausible({ t: 0, lat: 90, lon: -180, accuracyM: null, speedMps: null })).toBe(true);
    expect(plausible({ t: 0, lat: 90.1, lon: 0, accuracyM: null, speedMps: null })).toBe(false);
    expect(plausible({ t: Number.NaN, lat: 0, lon: 0, accuracyM: null, speedMps: null })).toBe(
      false,
    );
  });

  test("a negative accuracy is how CoreLocation says the position is not valid, so it is not stored", () => {
    const at = (accuracyM: number | null) => ({
      t: 0,
      lat: 52,
      lon: 13,
      accuracyM,
      speedMps: null,
    });
    expect(plausible(at(-1))).toBe(false);
    expect(plausible(at(-0.01))).toBe(false);
    // A zero, a small radius, a wide one and no accuracy at all are positions.
    expect(plausible(at(0))).toBe(true);
    expect(plausible(at(4.5))).toBe(true);
    expect(plausible(at(2000))).toBe(true);
    expect(plausible(at(null))).toBe(true);
  });
});
