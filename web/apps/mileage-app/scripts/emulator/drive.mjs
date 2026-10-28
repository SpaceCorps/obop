// Feeds the synthetic route of route.mjs to a running Android emulator, one waypoint at a time as a
// mock location, and switches the screen and airplane mode on a fixed schedule on the way. The
// trip itself is started and stopped in the app (by hand or with `adb shell input`); this script
// only moves the "phone".
//
//   vp run emulator:drive -- [--adb PATH] [--serial emulator-5554] [--every-ms 6000]
//
// (From web/apps/mileage-app. The task runs this file with Node, as `android:build` runs Gradle:
// the mobile-toolchain exception of OR-34, started through `vp run`.)
//
// The schedule (waypoint index, 0 to 40): the screen goes off at 8 and on at 34; airplane mode goes
// on at 16 and off at 26, and on again at 36, where it stays: stop the trip while offline, then
// switch airplane mode off by hand (`adb shell cmd connectivity airplane-mode disable`). Every step
// is printed with the time, so the log shows what was done when.
import { spawnSync } from "node:child_process";
import { waypoints } from "./route.mjs";

const args = process.argv.slice(2);
const option = (name, fallback) => {
  const at = args.indexOf(name);
  return at >= 0 ? args[at + 1] : fallback;
};
const adb = option("--adb", "adb");
const serial = option("--serial", "emulator-5554");
const everyMs = Number(option("--every-ms", "6000"));

/** @type {Map<number, { label: string, adb: string[] }>} */
const SCHEDULE = new Map([
  [8, { label: "screen off", adb: ["shell", "input", "keyevent", "KEYCODE_SLEEP"] }],
  [
    16,
    { label: "airplane mode on", adb: ["shell", "cmd", "connectivity", "airplane-mode", "enable"] },
  ],
  [
    26,
    {
      label: "airplane mode off",
      adb: ["shell", "cmd", "connectivity", "airplane-mode", "disable"],
    },
  ],
  [34, { label: "screen on", adb: ["shell", "input", "keyevent", "KEYCODE_WAKEUP"] }],
  // Offline again for the end of the route: the trip is stopped with no network, and the outbox
  // has to drain when airplane mode is switched off by hand afterwards (second run, see 8.7).
  [
    36,
    { label: "airplane mode on", adb: ["shell", "cmd", "connectivity", "airplane-mode", "enable"] },
  ],
]);

function run(parts) {
  const result = spawnSync(adb, ["-s", serial, ...parts], { encoding: "utf8" });
  if (result.status !== 0) throw new Error(`adb ${parts.join(" ")} failed: ${result.stderr}`);
  return result.stdout.trim();
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const stamp = () => new Date().toISOString().slice(11, 19);

const points = waypoints();
for (const [index, point] of points.entries()) {
  const action = SCHEDULE.get(index);
  if (action) {
    run(action.adb);
    console.log(`${stamp()} waypoint ${index}: ${action.label}`);
  }
  run(["emu", "geo", "fix", point.lon.toFixed(7), point.lat.toFixed(7)]);
  console.log(
    `${stamp()} waypoint ${index} of ${points.length - 1}: ${point.lat.toFixed(7)} ${point.lon.toFixed(7)}`,
  );
  await sleep(everyMs);
}
console.log(`${stamp()} route done`);
