import { describe, expect, test } from "vite-plus/test";
import {
  type Fix,
  MAX_ACCURACY_M,
  MAX_SPEED_MPS,
  analyseTrack,
  distanceText,
  integrityFlags,
} from "./track.ts";
import { distanceMeters } from "./distance.ts";
import vectors from "./track.fixtures.json";

// The cases are shared with the server: crates/modules/mileage/src/track.rs runs the same file
// through the Rust function that computes the distance that counts (docs/spec/mobile.md 5.4).
interface Case {
  name: string;
  options?: { maxSpeedMps?: number; maxAccuracyM?: number; startedAt?: number };
  points: Fix[];
  expect: {
    distance_m: string;
    accepted: number;
    skipped_accuracy: number;
    skipped_speed: number;
    skipped_early: number;
    reanchors: number;
    seq_gaps: number;
    integrity: string;
    first_seq: number | null;
    last_seq: number | null;
    /** Checked here only: the server does not draw the route. */
    stretches: number[][];
  };
}

describe("the shared track vectors", () => {
  test("the defaults of the file are the defaults of the code", () => {
    expect(vectors.defaults).toEqual({ maxSpeedMps: MAX_SPEED_MPS, maxAccuracyM: MAX_ACCURACY_M });
  });

  test("the file has the cases the spec asks for", () => {
    expect((vectors.cases as Case[]).length).toBeGreaterThanOrEqual(12);
  });

  test.each(vectors.cases as Case[])("$name", (c) => {
    const a = analyseTrack(c.points, c.options ?? {});
    expect({
      distance_m: distanceText(a.distanceM),
      accepted: a.accepted,
      skipped_accuracy: a.skippedAccuracy,
      skipped_speed: a.skippedSpeed,
      skipped_early: a.skippedEarly,
      reanchors: a.reanchors,
      seq_gaps: a.seqGaps,
      integrity: integrityFlags(a),
      first_seq: a.first?.seq ?? null,
      last_seq: a.last?.seq ?? null,
      stretches: a.stretches,
    }).toEqual(c.expect);
  });
});

describe("analyseTrack", () => {
  test("does not change the array it is given", () => {
    const pts: Fix[] = [
      { seq: 2, t: 5000, lat: 50.0001, lon: 10 },
      { seq: 1, t: 0, lat: 50, lon: 10 },
    ];
    analyseTrack(pts);
    expect(pts.map((p) => p.seq)).toEqual([2, 1]);
  });

  test("a re-anchor after a drive starts a new stretch of the route, and the jump is not part of it", () => {
    // 4 fixes driven north 50 m apart every 5 s, then the phone jumps about 11 km and three
    // fixes agree with each other there.
    const drive: Fix[] = [0, 1, 2, 3].map((i) => ({
      seq: i + 1,
      t: i * 5000,
      lat: 50 + i * 0.00045,
      lon: 10,
    }));
    const far: Fix[] = [0, 1, 2].map((i) => ({
      seq: i + 5,
      t: 20_000 + i * 5000,
      lat: 50.1 + i * 0.00045,
      lon: 10,
    }));
    const a = analyseTrack([...drive, ...far]);
    expect(a.reanchors).toBe(1);
    expect(a.stretches).toEqual([
      [1, 2, 3, 4],
      [5, 6, 7],
    ]);
    // Before anything was driven, the fixes before the jump are not part of the route.
    const still = drive.map((f) => ({ ...f, lat: 50 }));
    expect(analyseTrack([...still, ...far]).stretches).toEqual([[5, 6, 7]]);
  });

  test("a speed exactly at the cap is accepted and one above it is not", () => {
    // The cap is inclusive: one second for the distance in metres is exactly that many m/s.
    const a = { seq: 1, t: 0, lat: 0, lon: 0 };
    const metres = distanceMeters(a, { lat: 0, lon: 1 });
    const b = { seq: 2, t: 1000, lat: 0, lon: 1 };
    expect(analyseTrack([a, b], { maxSpeedMps: metres }).skippedSpeed).toBe(0);
    expect(analyseTrack([a, b], { maxSpeedMps: metres * 0.999 }).skippedSpeed).toBe(1);
  });
});
