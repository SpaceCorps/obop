// track.ts: the route and the check of a trip, for the review screen.
import { describe, expect, it } from "vite-plus/test";
import { drawRoute, flagsOf, kmOf, minutesBetween, pointsOf } from "./track.ts";

const r = (id: string, lat: string, lon: string, accuracy_m: string | null = "8.00") => ({
  id,
  lat,
  lon,
  accuracy_m,
});

describe("mileage track", () => {
  it("keeps the points with a position and draws the accepted ones inside the box", () => {
    const pts = pointsOf([
      r("1", "53.5872339", "9.9345023"),
      r("2", "53.5880000", "9.9330000", "250.00"),
      r("3", "", "9.9"),
      r("4", "53.5868062", "9.9301322", null),
    ]);
    expect(pts).toHaveLength(3);
    const d = drawRoute(pts, 320, 200);
    expect(d.dropped).toHaveLength(1);
    expect(d.path.startsWith("M")).toBe(true);
    expect(d.path.split("L")).toHaveLength(2);
    for (const [x, y] of [d.start!, d.end!]) {
      expect(x).toBeGreaterThanOrEqual(0);
      expect(x).toBeLessThanOrEqual(320);
      expect(y).toBeGreaterThanOrEqual(0);
      expect(y).toBeLessThanOrEqual(200);
    }
  });

  it("fits the box to the accepted route, not to a far-off dropped point", () => {
    // 30 points along about 3 km, and one dropped fix about 55 km away.
    const rows = Array.from({ length: 30 }, (_, i) =>
      r(String(i), (53.55 + i * 0.001).toFixed(7), (9.93 + i * 0.001).toFixed(7)),
    );
    rows.splice(15, 0, r("far", "54.0500000", "9.9400000", "1900.00"));
    const d = drawRoute(pointsOf(rows), 320, 200);
    const [sx, sy] = d.start!;
    const [ex, ey] = d.end!;
    // The route fills the box: start and end are far apart.
    expect(Math.hypot(ex - sx, ey - sy)).toBeGreaterThan(150);
    // The dropped point is pinned inside the box.
    expect(d.dropped).toHaveLength(1);
    const [dx, dy] = d.dropped[0]!;
    expect(dx).toBeGreaterThanOrEqual(0);
    expect(dx).toBeLessThanOrEqual(320);
    expect(dy).toBeGreaterThanOrEqual(0);
    expect(dy).toBeLessThanOrEqual(200);
  });

  it("draws nothing without points", () => {
    expect(drawRoute([], 320, 200).path).toBe("");
  });

  it("reads the server's flags, the distance and the duration", () => {
    expect(flagsOf("seq_gap:2 low_accuracy:14")).toEqual({ seq_gap: 2, low_accuracy: 14 });
    expect(flagsOf("")).toEqual({});
    expect(kmOf("7037.50")).toBe(7);
    expect(kmOf("165142.69")).toBe(165.1);
    expect(kmOf(null)).toBeNull();
    expect(minutesBetween("2026-11-03T13:00:07Z", "2026-11-03T14:43:44Z")).toBe(104);
  });
});
