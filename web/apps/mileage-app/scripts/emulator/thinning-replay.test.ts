import { spawn, spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { createServer, type IncomingMessage } from "node:http";
import type { AddressInfo } from "node:net";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { afterAll, describe, expect, test } from "vite-plus/test";
import { createSqlStore, migrate } from "../../src/core/sql-store.ts";
import { RECORDING_THINNING } from "../../src/core/thin.ts";
import { memoryDb } from "../../src/test-support/memory-db.ts";
import { routeFeed } from "./route-feed.mjs";
import { START, haversine } from "./route.mjs";
import {
  RULES,
  cadence,
  dropFirst,
  looksThinned,
  percent,
  replay,
  savedPoints,
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

describe("the clean synthetic feed (route-feed.mjs)", () => {
  const feed = routeFeed();
  const asFixes = (skip: number) => dropFirst(toFixes(feed), skip);
  const shortfall = (skip: number, thinning: (typeof RULES)[number]["thinning"]) => {
    const fixes = asFixes(skip);
    const all = replay(fixes, null);
    const kept = replay(fixes, thinning);
    return {
      kept: kept.kept,
      metres: kept.distanceM,
      all: all.distanceM,
      text: percent((kept.distanceM - all.distanceM) / all.distanceM),
    };
  };

  test("is 241 fixes, one per second, from the first waypoint to the last", () => {
    expect(feed).toHaveLength(241);
    expect(cadence(toFixes(feed)).gapsS).toEqual({ smallest: 1, median: 1, largest: 1 });
    expect(cadence(toFixes(feed)).spanS).toBe(240);
    // A closed rectangle: it starts at the first waypoint and ends within a metre of it.
    expect(feed[0]).toMatchObject(START);
    expect(haversine(feed[0]!, feed[240]!)).toBeLessThan(1);
    expect(replay(toFixes(feed), null).distanceM).toBeCloseTo(3989.44, 2);
  });

  test("replays to the series that the documents state: 0, 1.15, 0, 1.16, 0, 1.17 % short, 121 to 118 points kept", () => {
    const wanted = [
      { kept: 121, text: "0.00 %", all: 3989.44, metres: 3989.44 },
      { kept: 120, text: "-1.15 %", all: 3972.81, metres: 3926.99 },
      { kept: 120, text: "0.00 %", all: 3956.17, metres: 3956.17 },
      { kept: 119, text: "-1.16 %", all: 3939.54, metres: 3893.73 },
      { kept: 119, text: "0.00 %", all: 3922.91, metres: 3922.91 },
      { kept: 118, text: "-1.17 %", all: 3906.27, metres: 3860.46 },
    ];
    // The 1.5 s rule keeps the same fixes of a feed of one per second as the 2 s rule does.
    for (const rule of [RULES[0]!, RULES[1]!]) {
      wanted.forEach((want, skip) => {
        const got = shortfall(skip, rule.thinning);
        const label = `${rule.name} --skip ${skip}`;
        expect(got.kept, label).toBe(want.kept);
        expect(got.text, label).toBe(want.text);
        expect(got.all, label).toBeCloseTo(want.all, 2);
        expect(got.metres, label).toBeCloseTo(want.metres, 2);
      });
    }
  });

  test("the loss is three corners of 9.7 m and the last step, and only on the odd phases", () => {
    const odd = shortfall(1, RULES[0]!.thinning);
    expect(odd.all - odd.metres).toBeGreaterThan(45.7);
    expect(odd.all - odd.metres).toBeLessThan(45.9);
    const even = shortfall(2, RULES[0]!.thinning);
    expect(even.all - even.metres).toBeCloseTo(0, 6);
  });

  test("the command writes the JSON that --points reads, to stdout or to a file that does not exist yet", () => {
    const script = fileURLToPath(new URL("./route-feed.mjs", import.meta.url));
    const out = spawnSync(process.execPath, [script], { encoding: "utf8" });
    expect(out.status, out.stderr).toBe(0);
    expect(JSON.parse(out.stdout)).toEqual(feed);
    const dir2 = mkdtempSync(join(tmpdir(), "route-feed-"));
    try {
      const target = join(dir2, "feed.json");
      expect(spawnSync(process.execPath, [script, target], { encoding: "utf8" }).status).toBe(0);
      expect(JSON.parse(readFileSync(target, "utf8"))).toEqual(feed);
      // A second run does not overwrite it.
      expect(spawnSync(process.execPath, [script, target], { encoding: "utf8" }).status).not.toBe(
        0,
      );
      const replayed = spawnSync(
        process.execPath,
        [
          "--disable-warning=MODULE_TYPELESS_PACKAGE_JSON",
          fileURLToPath(new URL("./thinning-replay.mjs", import.meta.url)),
          "--points",
          target,
          "--skip",
          "1",
        ],
        { encoding: "utf8", cwd: fileURLToPath(new URL("../..", import.meta.url)) },
      );
      expect(replayed.status, replayed.stderr).toBe(0);
      expect(replayed.stdout).toMatch(/2 s \/ 50 m \(the default\)\s+120\s+3926\.99\s+-1\.15 %/);
    } finally {
      rmSync(dir2, { recursive: true, force: true });
    }
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

describe("the points that --save-points writes", () => {
  test("are the time and position of each row and nothing else", () => {
    const rows = [
      {
        id: "p1",
        trip: "t1",
        seq: 1,
        recorded_at: "2026-10-30T09:00:00.000Z",
        lat: "52.0000000",
        lon: "13.0000000",
        accuracy_m: "4.0",
        speed_mps: "1.5",
      },
      {
        id: "p2",
        trip: "t1",
        seq: 2,
        recorded_at: "2026-10-30T09:00:01.000Z",
        lat: "52.0001000",
        lon: "13.0001000",
      },
    ];
    expect(savedPoints(rows)).toEqual([
      { recorded_at: "2026-10-30T09:00:00.000Z", lat: "52.0000000", lon: "13.0000000" },
      { recorded_at: "2026-10-30T09:00:01.000Z", lat: "52.0001000", lon: "13.0001000" },
    ]);
    // What --points reads back is the same fixes.
    expect(toFixes(savedPoints(rows))).toEqual(toFixes(rows));
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

  test("a percentage that rounds to zero below the reference has no minus sign in that column", () => {
    // ell(10) from fix 0: the default rule keeps the corner and the end, so its distance is the
    // distance of the row "off"; a reference a hair longer puts the column at -0.0000x %.
    const fixes = ell(10);
    const file = join(dir, "zero.json");
    writeFileSync(
      file,
      JSON.stringify(
        fixes.map((f) => ({ recorded_at: new Date(f.t).toISOString(), lat: f.lat, lon: f.lon })),
      ),
    );
    const reference = (replay(fixes, null).distanceM + 0.001).toFixed(3);
    const result = run("--points", file, "--reference-m", reference);
    expect(result.status, result.stderr).toBe(0);
    const row = result.stdout
      .split("\n")
      .find((line) => line.startsWith("2 s / 50 m (the default)"))!;
    expect(row).toMatch(/\s0\.00 %\s+0\.00 %$/);
    expect(result.stdout).not.toContain("-0.00");
  });

  test("says in the table that the row off need not equal the distance of the trip", () => {
    const file = join(dir, "table.json");
    writeFileSync(file, JSON.stringify(ell(3).map((f) => ({ t: f.t, lat: f.lat, lon: f.lon }))));
    const result = run("--points", file);
    expect(result.status, result.stderr).toBe(0);
    expect(result.stdout).toContain("the row 'off' need not equal the distance_m of the trip");
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

  describe("--save-points", () => {
    type Seen = { path: string; authorization: string | undefined; body: Record<string, unknown> };
    const rows = routeFeed().map((f, i) => ({
      id: `p${i}`,
      trip: "trip-1",
      seq: i + 1,
      recorded_at: f.recorded_at,
      lat: f.lat.toFixed(7),
      lon: f.lon.toFixed(7),
      accuracy_m: "5.0",
    }));

    /** A server that answers the three calls the script makes, and remembers them. */
    async function withServer<T>(
      options: { trips?: unknown[] },
      body: (url: string, seen: Seen[]) => Promise<T>,
    ): Promise<T> {
      const seen: Seen[] = [];
      const server = createServer((req: IncomingMessage, res) => {
        let text = "";
        req.on("data", (chunk) => (text += chunk));
        req.on("end", () => {
          const payload = text === "" ? {} : (JSON.parse(text) as Record<string, unknown>);
          seen.push({
            path: req.url ?? "",
            authorization: req.headers.authorization,
            body: payload,
          });
          const reply = (value: unknown) => {
            res.setHeader("content-type", "application/json");
            res.end(JSON.stringify(value));
          };
          if (req.url === "/api/auth/login") {
            return reply(
              payload.username === "rita" && payload.password === "pw" ? { token: "tok" } : {},
            );
          }
          if (req.headers.authorization !== "Bearer tok") {
            res.statusCode = 401;
            return reply({});
          }
          if (req.url === "/api/mileage_trip/search") {
            return reply({ value: { rows: options.trips ?? [{ id: "trip-1", driver: "dave" }] } });
          }
          if (req.url === "/api/mileage_point/search") {
            const offset = Number(payload.offset ?? 0);
            const limit = Number(payload.limit ?? 100);
            return reply({ value: { rows: rows.slice(offset, offset + limit) } });
          }
          res.statusCode = 404;
          return reply({});
        });
      });
      await new Promise<void>((done) => server.listen(0, "127.0.0.1", done));
      try {
        return await body(`http://127.0.0.1:${(server.address() as AddressInfo).port}`, seen);
      } finally {
        await new Promise((done) => server.close(done));
      }
    }

    const appFolder = fileURLToPath(new URL("../..", import.meta.url));
    const runIn = (cwd: string, ...args: string[]) =>
      new Promise<{ status: number | null; stdout: string; stderr: string }>((done) => {
        const child = spawn(
          process.execPath,
          ["--disable-warning=MODULE_TYPELESS_PACKAGE_JSON", script, ...args],
          { cwd, env: { ...process.env, OBOP_CHECK_PASSWORD: "pw" } },
        );
        let stdout = "";
        let stderr = "";
        child.stdout.on("data", (chunk) => (stdout += chunk));
        child.stderr.on("data", (chunk) => (stderr += chunk));
        child.on("close", (status) => done({ status, stdout, stderr }));
      });
    const runAsync = (...args: string[]) => runIn(appFolder, ...args);

    const tableOf = (stdout: string) => stdout.slice(stdout.indexOf("\n\n"));

    test("writes every point it fetched, so that --points replays the same table later", async () => {
      const target = join(dir, "saved.json");
      await withServer({}, async (url) => {
        const live = await runAsync(
          "--server",
          url,
          "--user",
          "rita",
          "--driver",
          "dave",
          "--save-points",
          target,
        );
        expect(live.status, live.stderr).toBe(0);
        expect(live.stdout).toContain(`saved 241 points of trip trip-1 of dave to ${target}`);
        const saved = JSON.parse(readFileSync(target, "utf8"));
        expect(saved).toHaveLength(241);
        expect(saved).toEqual(savedPoints(rows));
        expect(Object.keys(saved[0]).sort()).toEqual(["lat", "lon", "recorded_at"]);
        if (process.platform !== "win32") expect(statSync(target).mode & 0o777).toBe(0o600);
        // Later, with the instance gone: the same cadence line and the same table.
        const later = await runAsync("--points", target);
        expect(later.status, later.stderr).toBe(0);
        expect(tableOf(later.stdout)).toBe(tableOf(live.stdout));
        expect(later.stdout).toMatch(/241 points over 240 s/);
      });
    });

    test("with --skip it still saves every fix of the trip, and the replay uses the skipped ones", async () => {
      const target = join(dir, "saved-skip.json");
      await withServer({}, async (url) => {
        const live = await runAsync(
          "--server",
          url,
          "--user",
          "rita",
          "--driver",
          "dave",
          "--skip",
          "3",
          "--save-points",
          target,
        );
        expect(live.status, live.stderr).toBe(0);
        expect(live.stdout).toContain(`saved 241 points of trip trip-1 of dave to ${target}`);
        const saved = JSON.parse(readFileSync(target, "utf8")) as Array<{ recorded_at: string }>;
        expect(saved).toHaveLength(241);
        expect(saved[0]?.recorded_at).toBe(rows[0]?.recorded_at);
        expect(saved.at(-1)?.recorded_at).toBe(rows.at(-1)?.recorded_at);
        // The replay is over the 238 fixes that were left, as without --save-points.
        expect(live.stdout).toContain(
          "the first 3 fixes of the trip are dropped before the replay",
        );
        expect(live.stdout).toMatch(/238 points over 237 s/);
        // And the saved file replays the same table with the same --skip later.
        const later = await runAsync("--points", target, "--skip", "3");
        expect(later.status, later.stderr).toBe(0);
        expect(tableOf(later.stdout)).toBe(tableOf(live.stdout));
      });
    });

    test("a relative file name is announced with its absolute path", async () => {
      const name = "relative-saved.json";
      await withServer({}, async (url) => {
        const result = await runIn(dir, "--server", url, "--user", "rita", "--save-points", name);
        expect(result.status, result.stderr).toBe(0);
        const absolute = resolve(dir, name);
        expect(existsSync(absolute)).toBe(true);
        expect(result.stdout).toContain(`saved 241 points of trip trip-1 of dave to ${absolute}`);
        expect(result.stdout).not.toContain(`to ${name}`);
      });
    });

    test("does not overwrite a file, and asks the server nothing then", async () => {
      const target = join(dir, "exists.json");
      writeFileSync(target, "keep me");
      await withServer({}, async (url, seen) => {
        const result = await runAsync("--server", url, "--user", "rita", "--save-points", target);
        expect(result.status).toBe(2);
        expect(result.stderr).toContain("exists already");
        expect(readFileSync(target, "utf8")).toBe("keep me");
        expect(seen).toEqual([]);
      });
    });

    test("writes nothing when there is no trip, and refuses to be used with --points", async () => {
      const target = join(dir, "none.json");
      await withServer({ trips: [] }, async (url) => {
        const result = await runAsync("--server", url, "--user", "rita", "--save-points", target);
        expect(result.status).toBe(2);
        expect(result.stderr).toContain("no trip found");
        expect(existsSync(target)).toBe(false);
      });
      const points = join(dir, "in.json");
      writeFileSync(points, JSON.stringify(rows));
      const both = await runAsync("--points", points, "--save-points", target);
      expect(both.status).toBe(2);
      expect(both.stderr).toContain("use it with --server");
      expect(existsSync(target)).toBe(false);
      const unnamed = await runAsync(
        "--server",
        "http://127.0.0.1:1",
        "--user",
        "rita",
        "--save-points",
      );
      expect(unnamed.status).toBe(2);
      expect(unnamed.stderr).toContain("takes the name of the file");
    });
  });
});
