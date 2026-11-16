// Drives the host app in the iOS Simulator from a script of actions (HostDrive.swift; the actions
// are listed in drive-lib.mjs). One `xcodebuild test` session runs the whole script.
//
//   OBOP_DRIVER_USER=dave OBOP_DRIVER_PASSWORD=... vp run simulator:drive -- --device UDID --script FILE
//
// The app must be installed (`xcrun simctl install`). Environment: OBOP_DRIVER_USER and
// OBOP_DRIVER_PASSWORD (only when the script types $USER or $PASSWORD; read from the environment
// only and removed from what this script prints and logs), OBOP_APP_BUNDLE_ID (default
// org.obop.app.test), OBOP_HOST_IOS_ROOT (scripts/ios/env.sh; the test project, its build, the logs
// and the screenshots go to its `uitest` folder, else to the temporary directory).
//
// What stays on the machine: `xcodebuild test` hands the password to the test runner as an
// environment variable and may keep it in its DerivedData and in the .xcresult. So the `uitest`
// folder is private (mode 700) and holds a password: use a throwaway one, and delete the folder when
// the drive is done.
//
// (On the Mac. A test script run by Node; not part of the app, its build or any image. It runs no
// shell: every command gets an argument list. docs/spec/mobile-host.md 12.8.)
import { spawn, spawnSync } from "node:child_process";
import { chmodSync, createWriteStream, existsSync, mkdirSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";
import { redact, runnerEnv, scriptProblems, sessionVerdict, testArgs } from "./drive-lib.mjs";

const args = process.argv.slice(2);
const option = (name) => {
  const at = args.indexOf(name);
  return at >= 0 ? args[at + 1] : undefined;
};
function usage(message) {
  console.error(`drive.mjs: ${message}`);
  console.error("usage: node scripts/simulator/drive.mjs --device UDID --script FILE");
  process.exit(2);
}

const device = option("--device");
const scriptFile = option("--script");
if (!device) usage("--device UDID is needed (xcrun simctl list devices)");
if (!scriptFile || !existsSync(scriptFile)) usage("--script FILE is needed");
const script = readFileSync(scriptFile, "utf8");
const problems = scriptProblems(script);
if (problems.length > 0) usage(problems.join("; "));

const root = join(process.env.OBOP_HOST_IOS_ROOT ?? tmpdir(), "uitest");
const stamp = new Date().toISOString().replace(/[:.]/g, "-");
const shots = join(root, "shots");
mkdirSync(shots, { recursive: true, mode: 0o700 });
try {
  chmodSync(root, 0o700);
} catch {
  // Not POSIX: nothing to do.
}

let env;
try {
  env = runnerEnv({
    ...process.env,
    OBOP_APP_BUNDLE_ID: process.env.OBOP_APP_BUNDLE_ID ?? "org.obop.app.test",
    OBOP_SHOT_DIR: shots,
    OBOP_SCRIPT: script,
  });
} catch (error) {
  usage(error instanceof Error ? error.message : String(error));
}
const secrets = [process.env.OBOP_DRIVER_PASSWORD ?? ""];
const say = (text) => console.log(redact(text, secrets));

const swift = fileURLToPath(new URL("./HostDrive.swift", import.meta.url));
const generator = fileURLToPath(new URL("./make-project.rb", import.meta.url));
const generated = spawnSync("ruby", [generator, join(root, "gen"), swift], { encoding: "utf8" });
if (generated.status !== 0) {
  console.error(`drive.mjs: make-project.rb failed: ${redact(generated.stderr ?? "", secrets)}`);
  process.exit(1);
}

let command;
try {
  command = testArgs({
    project: join(root, "gen", "HostDrive.xcodeproj"),
    device,
    derivedData: join(root, "dd"),
    resultBundle: join(root, `result-${stamp}.xcresult`),
  });
} catch (error) {
  usage(error instanceof Error ? error.message : String(error));
}

const logPath = join(root, `drive-${stamp}.log`);
const log = createWriteStream(logPath);
say(`log: ${logPath}`);
say(`screenshots: ${shots}`);

const child = spawn("xcodebuild", command, { env, stdio: ["ignore", "pipe", "pipe"] });
let verdict = null;
let killTimer;
function onLine(line) {
  const text = redact(line, secrets);
  log.write(`${text}\n`);
  if (
    /^(STEP|SHOT|DUMP|  \d+ id=|  SYSTEM ALERT|EXISTS|WAITGONE|TAPPED|NOT-THERE|NO-|KEYBOARD|FRAME|APP-STATE|SAVE-PASSWORD|SCRIPT-DONE)/.test(
      text,
    ) ||
    /error:|\*\* TEST (SUCCEEDED|FAILED)|Test Case .* (passed|failed)/.test(text)
  ) {
    say(text);
  }
  const result = sessionVerdict(line);
  if (result) {
    verdict = result;
    // After the result `xcodebuild` has been seen to hang for minutes: it is stopped.
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
  process.exit(verdict === "passed" ? 0 : verdict === "failed" ? 1 : (code ?? 1));
});
