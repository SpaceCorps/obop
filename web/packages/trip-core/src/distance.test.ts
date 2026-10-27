import { describe, expect, test } from "vite-plus/test";
import { EARTH_MEAN_RADIUS_M, distanceMeters } from "./distance.ts";

// All positions are synthetic.
describe("distanceMeters", () => {
  test("is zero for the same position", () => {
    expect(distanceMeters({ lat: 50, lon: 10 }, { lat: 50, lon: 10 })).toBe(0);
  });

  test("one degree of longitude on the equator is R * pi / 180", () => {
    const expected = (EARTH_MEAN_RADIUS_M * Math.PI) / 180;
    const d = distanceMeters({ lat: 0, lon: 0 }, { lat: 0, lon: 1 });
    expect(d).toBeCloseTo(expected, 6);
    expect(d).toBeCloseTo(111_195.08, 1);
  });

  test("a quarter meridian is R * pi / 2", () => {
    const d = distanceMeters({ lat: 0, lon: 0 }, { lat: 90, lon: 0 });
    expect(d).toBeCloseTo((EARTH_MEAN_RADIUS_M * Math.PI) / 2, 6);
  });

  test("antipodal points are R * pi apart and do not produce NaN", () => {
    const d = distanceMeters({ lat: 0, lon: 0 }, { lat: 0, lon: 180 });
    expect(d).toBeCloseTo(EARTH_MEAN_RADIUS_M * Math.PI, 6);
    expect(Number.isNaN(distanceMeters({ lat: 10, lon: 20 }, { lat: -10, lon: -160 }))).toBe(false);
  });

  test("is symmetric", () => {
    const a = { lat: 50, lon: 10 };
    const b = { lat: 50.01, lon: 10.01 };
    expect(distanceMeters(a, b)).toBe(distanceMeters(b, a));
  });

  test("crosses the antimeridian the short way", () => {
    const d = distanceMeters({ lat: 0, lon: 179.5 }, { lat: 0, lon: -179.5 });
    expect(d).toBeCloseTo((EARTH_MEAN_RADIUS_M * Math.PI) / 180, 6);
  });

  test("the synthetic reference pair of the Mileage app is about 1.32 km", () => {
    const d = distanceMeters({ lat: 50.0, lon: 10.0 }, { lat: 50.01, lon: 10.01 });
    expect(d).toBeCloseTo(1321.81, 1);
  });
});
