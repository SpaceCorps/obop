// Moves the iOS Simulator along the synthetic reference route of the emulator test (route.mjs of
// scripts/emulator: a closed rectangle of 41 waypoints, 100 m apart) with `xcrun simctl location`.
// The trip itself is started and stopped in the app; this script only moves the "phone".
//
//   vp run simulator:route                          # prints the command, runs nothing
//   vp run simulator:route -- --run                 # starts the route on the booted Simulator
//   vp run simulator:route -- --run --device UDID --speed 16.7
//   vp run simulator:route -- --clear               # stops a running route and clears the location
//   vp run simulator:route -- --set-start           # puts the Simulator at the first waypoint
//
// (From web/apps/mileage-app, on the Mac. A test script, started through `vp run` and run by Node;
// it imports only Node's own modules and its siblings, and it is not part of the app, its build, any
// image or the shipped toolchain. It runs no shell: `xcrun` gets an argument list.)
//
// `simctl location start` returns at once and moves the Simulator in the background for about
// length / speed seconds; the script prints that time and, with --run, waits for it.
import { spawnSync } from "node:child_process";
import { START } from "../emulator/route.mjs";
import { DEFAULT_SPEED_MPS, routeTime, startArgs } from "./route-lib.mjs";

const args = process.argv.slice(2);
const option = (name, fallback) => {
  const at = args.indexOf(name);
  return at >= 0 ? args[at + 1] : fallback;
};
const device = option("--device", "booted");
const speed = Number(option("--speed", String(DEFAULT_SPEED_MPS)));
const interval = Number(option("--interval", "1"));

function xcrun(parts) {
  const result = spawnSync("xcrun", parts, { encoding: "utf8" });
  if (result.error) throw new Error(`could not start xcrun: ${result.error.message}`);
  if (result.status !== 0)
    throw new Error(`xcrun ${parts.slice(0, 4).join(" ")} failed: ${result.stderr}`);
  return result.stdout.trim();
}

const stamp = () => new Date().toISOString().slice(11, 19);

try {
  if (args.includes("--clear")) {
    xcrun(["simctl", "location", device, "clear"]);
    console.log(`${stamp()} location cleared on ${device}`);
  } else if (args.includes("--set-start")) {
    xcrun(["simctl", "location", device, "set", `${START.lat},${START.lon}`]);
    console.log(`${stamp()} ${device} is at ${START.lat},${START.lon}`);
  } else {
    const command = startArgs({ device, speed, interval });
    const { metres, seconds } = routeTime(speed);
    console.log(
      `route: 41 waypoints, ${metres.toFixed(2)} m (haversine), ${speed.toFixed(2)} m/s, about ${Math.round(seconds)} s`,
    );
    if (!args.includes("--run")) {
      console.log(
        `xcrun ${command.map((a) => (a.includes(" ") ? JSON.stringify(a) : a)).join(" ")}`,
      );
    } else {
      xcrun(command);
      console.log(`${stamp()} route started on ${device}`);
      await new Promise((resolve) => setTimeout(resolve, (seconds + 5) * 1000));
      console.log(`${stamp()} route done`);
    }
  }
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
}
