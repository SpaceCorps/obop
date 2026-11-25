// Checks a recorded trip on a running Android emulator against the reference route (E2): it puts
// the "phone" at the start of the route, opens Rutsa, presses Start, feeds the route (drive.mjs,
// with the screen off and on again on the way), presses Stop and Stop in the dialog, and reads the
// km of the trip that ended in the current widget of the home screen, which it compares with the
// length of the route. The exit status is 0 when the km fit and 1 when they do not or a step failed.
//
//   vp run emulator:check -- [--adb PATH] [--serial emulator-5554] [--every-ms 6000]
//        [--tolerance 0.02]
//
// Before: the app is installed, location is allowed and its explanation was read (one Start by
// hand after `pm clear`), and no trip is open. The buttons are found in a dump of the screen
// (`uiautomator dump`) by their testID, else by their words, and pressed with `adb shell input
// tap`. A step that does not come is said, with the dump of the screen saved next to the script.
//
// (From web/apps/rutsa. A test script, started through `vp run` and run by Node; it imports only
// Node's own modules and its siblings, and it is not part of the app, its build, any image or the
// shipped toolchain. After `check-trip.mjs` of `mileage-app`, which reads the trip from its
// server; Rutsa has none, so the km are read on the screen, rounded to 0.1 km as the driver sees
// them.)
import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { byId, byText, center, judgeKm, parseKm, parseNodes } from "./check-lib.mjs";
import { adbRunner, drive, moveTo } from "./drive.mjs";
import { START, haversine, length, waypoints } from "./route.mjs";

const PACKAGE = "org.obop.rutsa";

const args = process.argv.slice(2);
const option = (name, fallback) => {
  const at = args.indexOf(name);
  return at >= 0 ? args[at + 1] : fallback;
};
const run = adbRunner(option("--adb", "adb"), option("--serial", "emulator-5554"));
const everyMs = Number(option("--every-ms", "6000"));
const tolerance = Number(option("--tolerance", "0.02"));

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const stamp = () => new Date().toISOString().slice(11, 19);
const say = (text) => console.log(`${stamp()} ${text}`);

/** The screen now, as its nodes. */
function screen() {
  run(["shell", "uiautomator", "dump", "/sdcard/rutsa-check.xml"]);
  const xml = run(["exec-out", "cat", "/sdcard/rutsa-check.xml"]);
  return { xml, nodes: parseNodes(xml) };
}

/** Stops with a reason; the last dump of the screen is kept to look at. */
function fail(reason, xml) {
  if (xml) {
    const file = fileURLToPath(new URL("./check-trip-screen.xml", import.meta.url));
    writeFileSync(file, xml);
    console.log(`the screen was saved to ${file}`);
  }
  console.log(`FAIL ${reason}`);
  process.exit(1);
}

/**
 * Waits until `find` finds its node on the screen (a dump every second), then gives it.
 * @param {string} what
 * @param {(nodes: ReturnType<typeof parseNodes>) => import("./check-lib.mjs").UiNode | null} find
 * @param {number} seconds
 */
async function waitFor(what, find, seconds) {
  let last = "";
  for (let i = 0; i < seconds; i += 1) {
    const { xml, nodes } = screen();
    last = xml;
    const node = find(nodes);
    if (node) return node;
    await sleep(1000);
  }
  return fail(`${what} did not come within ${seconds} s`, last);
}

/** Taps the middle of a node. */
function tap(node) {
  const { x, y } = center(node);
  run(["shell", "input", "tap", String(x), String(y)]);
}

/** Rutsa in front, unlocked: started, or its task brought back. */
function openApp() {
  try {
    run(["shell", "wm", "dismiss-keyguard"]);
  } catch {
    // An image without the command: the screen was not locked, or is unlocked by hand.
  }
  run(["shell", "monkey", "-p", PACKAGE, "-c", "android.intent.category.LAUNCHER", "1"]);
}

/** The screens over the home screen, known by the testIDs of their parts. */
const OVER_HOME = ["trip-", "trips-", "section-", "manual-"];

/**
 * The home screen, whatever was left open on the phone (a trip's screen, History, the form of a
 * trip added by hand): the back button of Android until the current widget shows, and the app
 * opened again if it left. Gives the state of the current widget (Recording, Paused, Ended), or
 * its "No trips yet." before the first trip.
 * @param {string} when
 */
function homeScreen(when) {
  openApp();
  return waitFor(
    `the home screen ${when}`,
    (nodes) => {
      // The state of the widget, or its "No trips yet." before the first trip.
      const state = byId(nodes, "current-state") ?? byId(nodes, "current-none");
      if (state) return state;
      if (nodes.some((node) => OVER_HOME.some((prefix) => node.id.startsWith(prefix)))) {
        run(["shell", "input", "keyevent", "4"]);
      } else {
        openApp();
      }
      return null;
    },
    20,
  );
}

const referenceM = length(waypoints(), haversine);
say(`reference route: ${waypoints().length} waypoints, ${referenceM.toFixed(2)} m`);

// The "phone" at the start before Start, so the first fix is not a jump from elsewhere.
moveTo(run, START);
const before = await homeScreen("before the start");
if (before.text !== "Ended" && before.id !== "current-none") {
  fail(`a trip is open (${before.text}): stop it first`);
}
const start = await waitFor(
  "the button Start",
  (nodes) => byId(nodes, "start") ?? byText(nodes, "Start"),
  20,
);
tap(start);
say("Start pressed");
await waitFor(
  "the recording (if the screen of the permissions came, allow location by hand first)",
  (nodes) => {
    const state = byId(nodes, "current-state");
    return state && state.text === "Recording" ? state : null;
  },
  20,
);
say("recording");

await drive(run, everyMs);
// The last fix stored, and the screen awake and unlocked again after the drive turned it off.
await sleep(5000);
// Back on the home screen, whatever was opened by hand meanwhile.
const home = await homeScreen("after the drive");

if (home.text === "Ended") {
  // Stopped by hand before the script came to it: the trip is read as it is.
  say("the trip was stopped already");
} else {
  // The big Stop while it records, the small one while it is paused.
  const stop = await waitFor(
    "the button Stop",
    (nodes) => byId(nodes, "stop") ?? byId(nodes, "stop-paused"),
    20,
  );
  tap(stop);
  const confirm = await waitFor(
    "the dialog Stop the trip?",
    (nodes) => byId(nodes, "button1") ?? byText(nodes, "Stop"),
    10,
  );
  tap(confirm);
  say("Stop pressed");
}

const ended = await waitFor(
  "the trip that ended",
  (nodes) => {
    const state = byId(nodes, "current-state");
    return state && state.text === "Ended" ? state : null;
  },
  20,
);
const { xml, nodes } = screen();
const kmNode = byId(nodes, "current-km");
const shownKm = kmNode ? parseKm(kmNode.text) : null;
if (shownKm === null) fail("no km in the current widget", xml);
const name = byId(nodes, "current-name")?.text ?? "";
say(`${name}: ${ended.text}, ${kmNode?.text}`);

const verdict = judgeKm(shownKm, referenceM, tolerance);
const line = `${shownKm.toFixed(1)} km on the screen, ${verdict.expectedKm.toFixed(3)} km expected, off by ${verdict.offKm.toFixed(3)} km (allowed ${verdict.allowedKm.toFixed(3)})`;
if (!verdict.ok) fail(line);
console.log(`ok   ${line}`);
