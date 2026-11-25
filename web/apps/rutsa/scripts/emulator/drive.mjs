// Feeds the synthetic route of route.mjs to a running Android emulator, one waypoint at a time as a
// mock location, and switches the screen off and on again on the way, so the trip is recorded in
// the background too (C3). The trip itself is started and stopped in the app (by hand, or by
// check-trip.mjs of E2); this script only moves the "phone".
//
//   vp run emulator:drive -- [--adb PATH] [--serial emulator-5554] [--every-ms 6000]
//
// (From web/apps/rutsa. A test script, started through `vp run` like scripts/check-sdk.mjs and run
// by Node; it imports only Node's own modules and its siblings, and it is not part of the app, its
// build, any image or the shipped toolchain. The mobile-toolchain exception of OR-34 (b) names only
// Metro, the Expo CLI, Gradle and Xcode and does not cover it, and none is claimed: the script is
// a `vp run` task.)
//
// After `drive.mjs` of `mileage-app`, without its schedule of airplane mode, which tests the upload
// to its server. The schedule (waypoint index, 0 to 40): the screen goes off at 8 and on at 34.
// Every step is printed with the time, so the log shows what was done when. 41 waypoints 100 m
// apart every 6 s is 60 km/h, a car in town, and takes 4 minutes.
import { spawnSync } from "node:child_process";
import { isMain } from "../lib/is-main.mjs";
import { waypoints } from "./route.mjs";

/** What is done on the way, by waypoint index. */
const SCHEDULE = new Map([
  [8, { label: "screen off", adb: ["shell", "input", "keyevent", "KEYCODE_SLEEP"] }],
  [34, { label: "screen on", adb: ["shell", "input", "keyevent", "KEYCODE_WAKEUP"] }],
]);

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const stamp = () => new Date().toISOString().slice(11, 19);

/**
 * A runner of `adb` for one device: the arguments after `-s SERIAL`; throws when adb fails.
 * @param {string} adb
 * @param {string} serial
 */
export function adbRunner(adb, serial) {
  /** @param {string[]} parts */
  return (parts) => {
    const result = spawnSync(adb, ["-s", serial, ...parts], { encoding: "utf8" });
    if (result.status !== 0) {
      throw new Error(`adb ${parts.join(" ")} failed: ${result.stderr || result.error?.message}`);
    }
    return result.stdout.trim();
  };
}

/**
 * Puts the emulator at a point: `adb emu geo fix` takes the longitude first.
 * @param {(parts: string[]) => string} run
 * @param {{ lat: number, lon: number }} point
 */
export function moveTo(run, point) {
  run(["emu", "geo", "fix", point.lon.toFixed(7), point.lat.toFixed(7)]);
}

/**
 * Feeds the route, `everyMs` apart, with the schedule of the screen.
 * @param {(parts: string[]) => string} run
 * @param {number} everyMs
 */
export async function drive(run, everyMs) {
  const points = waypoints();
  for (const [index, point] of points.entries()) {
    const action = SCHEDULE.get(index);
    if (action) {
      run(action.adb);
      console.log(`${stamp()} waypoint ${index}: ${action.label}`);
    }
    moveTo(run, point);
    console.log(
      `${stamp()} waypoint ${index} of ${points.length - 1}: ${point.lat.toFixed(7)} ${point.lon.toFixed(7)}`,
    );
    await sleep(everyMs);
  }
  console.log(`${stamp()} route done`);
}

if (isMain(import.meta.url, process.argv[1])) {
  const args = process.argv.slice(2);
  const option = (name, fallback) => {
    const at = args.indexOf(name);
    return at >= 0 ? args[at + 1] : fallback;
  };
  const run = adbRunner(option("--adb", "adb"), option("--serial", "emulator-5554"));
  await drive(run, Number(option("--every-ms", "6000")));
}
