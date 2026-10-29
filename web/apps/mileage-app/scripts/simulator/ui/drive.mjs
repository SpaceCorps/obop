// Drives the app in the iOS Simulator through a whole trip: sign-in through the system browser
// sheet, Start, the system location prompt, the Home button, the route, Stop, Submit. One
// `xcodebuild test` session runs the steps of MileageDrive.swift; this script starts the route
// (`xcrun simctl location ... start`, scripts/simulator/route-lib.mjs) when the test says that the
// trip is recording, and stops `xcodebuild` if it hangs after the result.
//
//   OBOP_DRIVER_USER=ivy OBOP_DRIVER_PASSWORD=... vp run simulator:drive -- --device UDID
//   vp run simulator:drive -- --device UDID --steps test20_StopAndDiscard --no-route
//
// Options: --device UDID (required: `xcrun simctl list devices`), --steps a,b,c (default: the
// whole trip), --no-route (do not start the route), --speed M/S, --interval S or --distance M (as route.mjs).
// Environment: OBOP_DRIVER_USER, OBOP_DRIVER_PASSWORD (read from the environment only, never from a
// file or an argument, and removed from everything that is printed or logged), OBOP_APP_BUNDLE_ID
// (default org.obop.mileage.test), OBOP_SERVER_ADDRESS (optional: typed into the address field of
// the sign-in instead of the build's default), OBOP_BUILD_ROOT (where the test project, its build
// and its logs go; default the temporary directory).
//
// (From web/apps/mileage-app, on the Mac. A test script run by Node; it is not part of the app, its
// build, any image or the shipped toolchain. It runs no shell: every command gets an argument list.
// docs/spec/mobile.md 8.8.)
import { spawn, spawnSync } from "node:child_process";
import { createWriteStream, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";
import { startArgs } from "../route-lib.mjs";
import {
  TRIP_STEPS,
  isRouteMarker,
  sessionVerdict,
  redact,
  runnerEnv,
  testArgs,
} from "./drive-lib.mjs";

const args = process.argv.slice(2);
const option = (name, fallback) => {
  const at = args.indexOf(name);
  return at >= 0 ? args[at + 1] : fallback;
};

const device = option("--device");
if (!device) {
  console.error("drive.mjs: --device UDID is needed (xcrun simctl list devices)");
  process.exit(2);
}
const steps = option("--steps", TRIP_STEPS.join(",")).split(",");
const withRoute = !args.includes("--no-route");
const root = join(process.env.OBOP_BUILD_ROOT ?? tmpdir(), "uitest");
const stamp = new Date().toISOString().replace(/[:.]/g, "-");
const swift = fileURLToPath(new URL("./MileageDrive.swift", import.meta.url));
const generator = fileURLToPath(new URL("./make-project.rb", import.meta.url));
const shots = join(root, "shots");
mkdirSync(shots, { recursive: true });

let env;
try {
  env = runnerEnv({
    ...process.env,
    OBOP_APP_BUNDLE_ID: process.env.OBOP_APP_BUNDLE_ID ?? "org.obop.mileage.test",
    OBOP_SHOT_DIR: shots,
  });
} catch (error) {
  console.error(`drive.mjs: ${error instanceof Error ? error.message : String(error)}`);
  process.exit(2);
}
const secrets = [process.env.OBOP_DRIVER_PASSWORD ?? ""];
const say = (text) => console.log(redact(text, secrets));

const generated = spawnSync("ruby", [generator, join(root, "gen"), swift], { encoding: "utf8" });
if (generated.status !== 0) {
  console.error(`drive.mjs: make-project.rb failed: ${redact(generated.stderr, secrets)}`);
  process.exit(1);
}

let command;
try {
  command = testArgs({
    project: join(root, "gen", "MileageDrive.xcodeproj"),
    device,
    derivedData: join(root, "dd"),
    resultBundle: join(root, `result-${stamp}.xcresult`),
    only: steps,
  });
} catch (error) {
  console.error(`drive.mjs: ${error instanceof Error ? error.message : String(error)}`);
  process.exit(2);
}

const logPath = join(root, `drive-${stamp}.log`);
const log = createWriteStream(logPath);
say(`log: ${logPath}`);
say(`screenshots: ${shots}`);

const child = spawn("xcodebuild", command, { env, stdio: ["ignore", "pipe", "pipe"] });
let routeStarted = false;
let killTimer;
let verdict = null;

function startRoute() {
  if (!withRoute || routeStarted) return;
  routeStarted = true;
  const route = startArgs({
    device,
    speed: Number(option("--speed", "16.667")),
    ...(args.includes("--distance")
      ? { distance: Number(option("--distance", "100")) }
      : { interval: Number(option("--interval", "1")) }),
  });
  const result = spawnSync("xcrun", route, { encoding: "utf8" });
  say(
    result.status === 0
      ? `${new Date().toISOString().slice(11, 19)}Z route started on ${device}`
      : `route did not start: ${result.stderr}`,
  );
}

function onLine(line) {
  const text = redact(line, secrets);
  log.write(`${text}\n`);
  // Our own output (the test prints these) and the verdicts, not every line of xcodebuild.
  if (
    /^(SERVER-ADDRESS|SYSTEM ALERT|  ALERT BUTTON|LOCATION-PROMPT|HOME-|RECORDING-SHOWN|READOUT|ROUTE-DONE|APP-STATE|TRIP-|NO-KEYBOARD|DUMP|ALREADY|SIGNIN-|RESTORED-|HTTP-)/.test(
      text,
    ) ||
    /error:|\*\* TEST (SUCCEEDED|FAILED)|Test Case .* (passed|failed)/.test(text)
  ) {
    say(text);
  }
  if (isRouteMarker(line)) startRoute();
  const result = sessionVerdict(line);
  if (result) {
    verdict = result;
    // After the result `xcodebuild` has been seen to hang for ten minutes: it is stopped.
    killTimer ??= setTimeout(() => child.kill("SIGTERM"), 90_000);
  }
}
for (const stream of [child.stdout, child.stderr]) {
  createInterface({ input: stream }).on("line", onLine);
}
child.on("exit", (code, signal) => {
  clearTimeout(killTimer);
  log.end();
  say(`xcodebuild ended: ${signal ?? `exit ${code}`}; the session ${verdict ?? "gave no result"}`);
  // The verdict of the session decides, also when `xcodebuild` had to be stopped after it.
  process.exit(verdict === "passed" ? 0 : verdict === "failed" ? 1 : (code ?? 1));
});
