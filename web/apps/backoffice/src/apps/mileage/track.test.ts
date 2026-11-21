// track.ts: the route and the check of a trip, for the review screen.
import { describe, expect, it } from "vite-plus/test";
import {
  checkNotes,
  drawRoute,
  flagsOf,
  kmOf,
  minutesBetween,
  pointsOf,
  reviewRefusal,
} from "./track.ts";

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

  it("leaves out a far-off first fix that the check of the track re-anchored away from (UB10)", () => {
    // The phone stood at a far-off place for one fix (good accuracy), then 30 fixes about 140 m
    // apart every 10 s: the server re-anchors on them and the trip starts there.
    const at = (i: number) => new Date(Date.UTC(2026, 10, 3, 13, 0, i * 10)).toISOString();
    const rows = [
      { ...r("far", "37.4219983", "-122.0840000"), seq: 1, recorded_at: at(0) },
      ...Array.from({ length: 30 }, (_, i) => ({
        ...r(String(i), (53.55 + i * 0.001).toFixed(7), (9.93 + i * 0.001).toFixed(7)),
        seq: i + 2,
        recorded_at: at(i + 1),
      })),
    ];
    const d = drawRoute(pointsOf(rows), 320, 200, 16, at(0));
    expect(d.dropped).toHaveLength(1);
    // The route fills the box and is one line from its start to its end.
    const [sx, sy] = d.start!;
    const [ex, ey] = d.end!;
    expect(Math.hypot(ex - sx, ey - sy)).toBeGreaterThan(150);
    expect(d.path.split("M")).toHaveLength(2);
    expect(d.path.split("L")).toHaveLength(30);
  });

  it("draws a jump after a drive as two lines, not as a line across it", () => {
    const at = (i: number) => new Date(Date.UTC(2026, 10, 3, 13, 0, i * 5)).toISOString();
    const rows = [
      ...[0, 1, 2, 3].map((i) => ({
        ...r(`a${i}`, (50 + i * 0.00045).toFixed(7), "10.0000000"),
        seq: i + 1,
        recorded_at: at(i),
      })),
      ...[0, 1, 2].map((i) => ({
        ...r(`b${i}`, (50.1 + i * 0.00045).toFixed(7), "10.0000000"),
        seq: i + 5,
        recorded_at: at(i + 4),
      })),
    ];
    const d = drawRoute(pointsOf(rows), 320, 200);
    expect(d.dropped).toHaveLength(0);
    expect(d.path.split("M")).toHaveLength(3);
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

  it("says plainly that a trip was already reviewed", () => {
    expect(reviewRefusal('state must be in_review to do this, it is "confirmed"')).toBe(
      "This trip was already reviewed.",
    );
    expect(reviewRefusal("not allowed")).toBeNull();
  });
  it("never calls a track clean when the server flagged it", () => {
    const clean = checkNotes("", 10, true);
    expect(clean).toEqual([
      { tone: "ok", text: "The track is clean: all 10 points were used for the distance." },
    ]);
    const early = checkNotes("before_start:1", 10, true);
    expect(early.map((n) => n.tone)).toEqual(["warn"]);
    expect(early[0].text).toBe("1 point was recorded before the trip started and was left out.");
    expect(checkNotes("before_start:3", 10, true)[0].text).toContain("3 points were recorded");
    const jumped = checkNotes("reanchor:2", 10, true);
    expect(jumped.map((n) => n.tone)).toEqual(["warn"]);
    expect(jumped[0].text).toContain("2 times");
    expect(checkNotes("reanchor:1", 10, true)[0].text).not.toContain("times");
    const unknown = checkNotes("something_new:1", 10, true);
    expect(unknown.map((n) => n.tone)).toEqual(["warn"]);
    expect(checkNotes("", 10, false)).toEqual([]);
  });
  it("never calls a track of no points clean, and names a route removed for a private purpose", () => {
    expect(checkNotes("", 0, true)).toEqual([]);
    const removed = checkNotes("route_removed:10", 0, true);
    expect(removed.map((n) => n.tone)).toEqual(["warn"]);
    expect(removed[0].text).toContain("removed when the trip was marked private (10 points)");
  });
});
