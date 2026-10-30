// A clean synthetic feed along the reference route (route.mjs): one fix per second at a steady
// 100 m in 6 s (16.667 m/s), the 40 legs of the rectangle, 241 fixes. Nothing here is a real
// person's track. It is the input that shows what the thinning rule of the app costs on a perfectly
// regular feed, so the cost that a drive shows can be told from the cost of the rule itself:
//
//   node scripts/emulator/route-feed.mjs feed.json
//   vp run emulator:thinning -- --points feed.json --skip 0      # then --skip 1, 2, 3, 4, 5
//
// (`node scripts/emulator/route-feed.mjs > feed.json` does the same in a shell whose `>` writes
// UTF-8; Windows PowerShell 5.1 writes UTF-16, which thinning-replay.mjs cannot read.) The file is
// the JSON array of {recorded_at, lat, lon} that `--points` reads. Replayed with the default rule
// (2 s / 50 m) the six start phases `--skip 0..5` fall short of the unthinned distance by 0.00,
// 1.15, 0.00, 1.16, 0.00 and 1.17 % and keep 121, 120, 120, 119, 119, 118 points: the even phases
// keep the three corners (fixes 72, 120 and 192), the odd ones skip them, and each corner then
// loses 9.7 m (the chord of two steps, 23.5 m, instead of the two steps, 33.2 m) and the last step
// of 16.6 m is dropped too (scripts/emulator/thinning-replay.test.ts pins these numbers).

import { writeFileSync } from "node:fs";
import { isMain } from "../lib/is-main.mjs";
import { waypoints } from "./route.mjs";

/** Seconds a leg of the route takes: 100 m in 6 s is 16.667 m/s, a car in town. */
const LEG_SECONDS = 6;
const START_MS = Date.parse("2026-10-30T10:00:00Z");

/**
 * One fix per second, from the first waypoint to the last, interpolated along the legs (a straight
 * line between waypoints, which are 100 m apart).
 * @returns {Array<{ recorded_at: string, lat: number, lon: number }>}
 */
export function routeFeed() {
  const way = waypoints();
  const legs = way.length - 1;
  const out = [];
  for (let second = 0; second <= legs * LEG_SECONDS; second += 1) {
    const along = second / LEG_SECONDS;
    const leg = Math.min(Math.floor(along), legs - 1);
    const part = along - leg;
    const a = way[leg];
    const b = way[leg + 1];
    out.push({
      recorded_at: new Date(START_MS + second * 1000).toISOString(),
      lat: a.lat + (b.lat - a.lat) * part,
      lon: a.lon + (b.lon - a.lon) * part,
    });
  }
  return out;
}

if (isMain(import.meta.url, process.argv[1])) {
  const text = `${JSON.stringify(routeFeed())}\n`;
  const file = process.argv[2];
  if (file === undefined) process.stdout.write(text);
  else writeFileSync(file, text, { flag: "wx" });
}
