import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { afterAll, describe, expect, test } from "vite-plus/test";
import { createSqlStore, migrate } from "../../src/core/sql-store.ts";
import { RECORDING_THINNING } from "../../src/core/thin.ts";
import { memoryDb } from "../../src/test-support/memory-db.ts";
import {
  RULES,
  cadence,
  dropFirst,
  looksThinned,
  percent,
  replay,
  toFixes,
  type TimedFix,
} from "./thinning-lib.mjs";

const T0 = Date.parse("2026-10-30T09:00:00Z");
/** Metres per degree of latitude, as the server's sphere has it (2 * pi * 6371008.8 / 360). */
const M_PER_DEG = (2 * Math.PI * 6_371_008.8) / 360;

/**
 * An L: `legs` fixes north at 14 m/s, one per second, then `legs` fixes east. The corner is the fix
 * at index `legs`. `from` drops that many fixes from the start (the feed starts later).
 */
function ell(legs = 10, from = 0): TimedFix[] {
  const lat0 = 52;
  const lonPerM = 1 / (M_PER_DEG * Math.cos((lat0 * Math.PI) / 180));
  const out: TimedFix[] = [];
  for (let i = 0; i <= 2 * legs; i += 1) {
    const north = Math.min(i, legs) * 14;
    const east = Math.max(i - legs, 0) * 14;
    out.push({ t: T0 + i * 1000, lat: lat0 + north / M_PER_DEG, lon: 13 + east * lonPerM });
  }
  return out.slice(from);
}

describe("the rules that are compared", () => {
  test("the first is the default of the app", () => {
    expect(RULES[0]!.thinning).toEqual(RECORDING_THINNING);
    expect(RULES.map((r) => r.thinning?.minGapMs ?? "off")).toEqual([2000, 1500, "off"]);
  });
});

describe("the replay", () => {
  test("without a rule every fix is kept and the distance is the sum of the legs", () => {
    const r = replay(ell(), null);
    expect(r.kept).toBe(21);
    expect(r.distanceM).toBeCloseTo(280, 0);
  });

  test("the default rule keeps every second fix of a feed of one per second", () => {
    expect(replay(ell(), RULES[0]!.thinning).kept).toBe(11);
    expect(replay(ell(), RULES[1]!.thinning).kept).toBe(11);
  });

  test("a straight feed loses nothing to the rule", () => {
    const straight = ell(10).slice(0, 11);
    const all = replay(straight, null);
    const thinned = replay(straight, RULES[0]!.thinning);
    expect(thinned.kept).toBe(6);
    expect(thinned.distanceM).toBeCloseTo(all.distanceM, 6);
  });

  test("a corner costs distance only when the kept fixes skip it: the cost depends on the phase of the feed", () => {
    const all = replay(ell(10), null).distanceM;
    // The feed starts on the corner's parity: the corner is kept, no loss.
    expect(replay(ell(10), RULES[0]!.thinning).distanceM).toBeCloseTo(all, 6);
    // The feed starts one second later (and ends on a kept fix): the corner is skipped, and the
    // chord across it (14 m north and 14 m east: 19.8 m) is shorter than the path (28 m).
    const later = ell(10, 1).slice(0, 19);
    const laterAll = replay(later, null).distanceM;
    const cut = replay(later, RULES[0]!.thinning).distanceM;
    expect(laterAll - cut).toBeGreaterThan(8);
    expect(laterAll - cut).toBeLessThan(8.5);
  });

  test("a fix far from the last stored one is kept whatever the gap", () => {
    const jump: TimedFix[] = [
      { t: T0, lat: 52, lon: 13 },
      { t: T0 + 500, lat: 52 + 60 / M_PER_DEG, lon: 13 },
    ];
    expect(replay(jump, RULES[0]!.thinning).kept).toBe(2);
  });

  test("fixes out of order are sorted and a fix not later than the last stored one is dropped", () => {
    const fixes = [ell()[4]!, ell()[0]!, ell()[0]!, ell()[2]!];
    expect(replay(fixes, null).kept).toBe(3);
  });

  test("it keeps what the store keeps: the same fixes through appendFixes give the same points", async () => {
    const db = memoryDb();
    await migrate(db);
    const store = createSqlStore(db);
    await store.startTrip({
      clientTripId: "c1",
      purpose: "business",
      startedAt: new Date(T0).toISOString(),
      nowMs: T0,
    });
    const fixes = ell(30, 1);
    await store.appendFixes(
      "c1",
      fixes.map((f) => ({ ...f, accuracyM: 5, speedMps: 14 })),
    );
    const stored = await store.points("c1");
    expect(stored.length).toBe(replay(fixes, RECORDING_THINNING).kept);
    expect(stored.map((p) => p.t)).toEqual(fixes.filter((_, i) => i % 2 === 0).map((f) => f.t));
  });
});

describe("the start phase", () => {
  test("dropFirst drops the first n fixes in time order", () => {
    const fixes = ell(10);
    expect(dropFirst(fixes, 0)).toEqual(fixes);
    expect(dropFirst([...fixes].reverse(), 3).map((f) => f.t)).toEqual(
      fixes.slice(3).map((f) => f.t),
    );
    expect(dropFirst(fixes, 100)).toEqual([]);
  });

  test("the cost of the default rule alternates with the parity of the first fix kept", () => {
    // ell(10): fixes 0..20, the corner is fix 10. The rule keeps every second fix of what it is given.
    const cost = (skip: number) => {
      const sub = dropFirst(ell(10), skip);
      const all = replay(sub, null).distanceM;
      return (all - replay(sub, RULES[0]!.thinning).distanceM) / all;
    };
    expect(cost(0)).toBeCloseTo(0, 9); // keeps 0, 2, ..., 20: the corner and the end
    expect(cost(1)).toBeGreaterThan(0.05); // keeps 1, 3, ..., 19: skips the corner and the last fix
    expect(cost(2)).toBeCloseTo(0, 9);
    expect(cost(3)).toBeGreaterThan(0.05);
  });

  test("a rounded-to-zero percentage has no minus sign", () => {
    expect(percent(-0.0000001)).toBe("0.00 %");
    expect(percent(0)).toBe("0.00 %");
    expect(percent(-0.0123)).toBe("-1.23 %");
    expect(percent(0.0115)).toBe("1.15 %");
  });
});

describe("the cadence of a track", () => {
  test("is the points per minute and the gaps between neighbours", () => {
    const c = cadence(ell(10));
    expect(c.count).toBe(21);
    expect(c.spanS).toBe(20);
    expect(c.perMinute).toBeCloseTo(60, 6);
    expect(c.gapsS).toEqual({ smallest: 1, median: 1, largest: 1 });
  });

  test("the smallest, the median and the largest gap are told apart", () => {
    // Gaps of 1, 3 and 5 seconds.
    const c = cadence([{ t: 0 }, { t: 1000 }, { t: 4000 }, { t: 9000 }]);
    expect(c.gapsS).toEqual({ smallest: 1, median: 3, largest: 5 });
  });

  test("a track with fewer than two points has none", () => {
    expect(cadence([]).gapsS).toBeNull();
    expect(cadence([{ t: 1 }]).gapsS).toBeNull();
  });

  test("a track recorded with the default thinning is recognised, a full one is not", () => {
    expect(looksThinned(ell(30))).toBe(false);
    const thinned = ell(30).filter((_, i) => i % 2 === 0);
    expect(looksThinned(thinned)).toBe(true);
    // Too short to say.
    expect(looksThinned(thinned.slice(0, 5))).toBe(false);
  });
});

describe("the rows of the server", () => {
  test("recorded_at and the decimal texts become milliseconds and numbers", () => {
    expect(
      toFixes([{ recorded_at: "2026-10-30T09:00:01.500Z", lat: "52.0000000", lon: "13.0000000" }]),
    ).toEqual([{ t: T0 + 1500, lat: 52, lon: 13 }]);
    expect(toFixes([{ t: 7, lat: 1, lon: 2 }])).toEqual([{ t: 7, lat: 1, lon: 2 }]);
  });
});

describe("the command", () => {
  const dir = mkdtempSync(join(tmpdir(), "thinning-replay-"));
  afterAll(() => rmSync(dir, { recursive: true, force: true }));
  const script = fileURLToPath(new URL("./thinning-replay.mjs", import.meta.url));
  const run = (...args: string[]) =>
    spawnSync(
      process.execPath,
      ["--disable-warning=MODULE_TYPELESS_PACKAGE_JSON", script, ...args],
      { encoding: "utf8", cwd: fileURLToPath(new URL("../..", import.meta.url)) },
    );

  test("prints the cadence and one line per rule for a file of points", () => {
    const file = join(dir, "points.json");
    writeFileSync(
      file,
      JSON.stringify(
        ell(10, 1).map((f) => ({
          recorded_at: new Date(f.t).toISOString(),
          lat: f.lat,
          lon: f.lon,
        })),
      ),
    );
    const result = run("--points", file, "--reference-m", "266");
    expect(result.status, result.stderr).toBe(0);
    expect(result.stdout).toContain("20 points over 19 s");
    expect(result.stdout).toMatch(/off\s+20\s+/);
    expect(result.stdout).toMatch(/2 s \/ 50 m \(the default\)\s+10\s+/);
    expect(result.stdout).toMatch(/1\.5 s \/ 50 m\s+10\s+/);
    expect(result.stdout).toContain("against the reference");
    expect(result.stdout).not.toContain("note: no two neighbouring points");
  });

  test("--skip drops the first fixes and the cost of the rule follows the phase", () => {
    const file = join(dir, "phase.json");
    writeFileSync(
      file,
      JSON.stringify(
        ell(10).map((f) => ({
          recorded_at: new Date(f.t).toISOString(),
          lat: f.lat,
          lon: f.lon,
        })),
      ),
    );
    const defaultRow = (out: string) =>
      out.split("\n").find((line) => line.startsWith("2 s / 50 m (the default)"))!;
    const zero = run("--points", file);
    expect(zero.status, zero.stderr).toBe(0);
    expect(zero.stdout).not.toContain("are dropped before the replay");
    expect(zero.stdout).toContain("21 points over 20 s");
    expect(defaultRow(zero.stdout)).toMatch(/\s+11\s+280\.\d\d\s+0\.00 %/);
    const one = run("--points", file, "--skip", "1");
    expect(one.status, one.stderr).toBe(0);
    expect(one.stdout).toContain("(the first 1 fixes of the trip are dropped before the replay)");
    // The reference does not fit a track that lost its start: it is not printed.
    const withReference = run("--points", file, "--skip", "1", "--reference-m", "266");
    expect(withReference.stdout).toContain("--reference-m is not used");
    expect(withReference.stdout).not.toContain("against the reference");
    expect(one.stdout).toContain("20 points over 19 s");
    expect(defaultRow(one.stdout)).toMatch(/\s+10\s+\d+\.\d\d\s+-\d+\.\d\d %/);
    // The rows say what the distance is.
    expect(one.stdout).toContain("without the server's accuracy and speed filters");
    expect(zero.stdout).not.toContain("-0.00");
  });

  test("--skip must be a whole number, and not leave fewer than two fixes", () => {
    const file = join(dir, "skip.json");
    writeFileSync(file, JSON.stringify(ell(3).map((f) => ({ t: f.t, lat: f.lat, lon: f.lon }))));
    for (const bad of ["abc", "-1", "1.5", ""]) {
      const result = run("--points", file, "--skip", bad);
      expect(result.status, bad).toBe(2);
      expect(result.stderr, bad).toContain("--skip takes a whole number");
    }
    const tooMany = run("--points", file, "--skip", "6");
    expect(tooMany.status).toBe(2);
    expect(tooMany.stderr).toContain("nothing to replay");
  });

  test("says so when the track was thinned already", () => {
    const file = join(dir, "thinned.json");
    writeFileSync(
      file,
      JSON.stringify(
        ell(30)
          .filter((_, i) => i % 2 === 0)
          .map((f) => ({ t: f.t, lat: f.lat, lon: f.lon })),
      ),
    );
    const result = run("--points", file);
    expect(result.status, result.stderr).toBe(0);
    expect(result.stdout).toContain("probably thinned when it was stored");
  });

  test("asks for what it needs and reads nothing without a password", () => {
    const result = spawnSync(
      process.execPath,
      ["--disable-warning=MODULE_TYPELESS_PACKAGE_JSON", script, "--user", "rita"],
      { encoding: "utf8", env: { ...process.env, OBOP_CHECK_PASSWORD: "" } },
    );
    expect(result.status).toBe(2);
    expect(result.stderr).toContain("usage:");
  });
});
