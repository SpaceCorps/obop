// One command from a change to the app on the emulator, ready to try: `vp check --fix`, `vp test`,
// the emulator started if none runs (and waited for until Android has booted), the release APK
// built, installed over the last one (the trips stay) and opened.
//
//   vp run emulator:ship -- [--fresh] [--no-check] [--no-test] [--no-build] [--serial emulator-5554]
//
//   --fresh      deletes the bundle of the last build and the cache of Metro first: Gradle does not
//                see a change in a shared package only (web/packages/ui-native, trip-core) and
//                would install the old bundle (CHECKS.md, Setup)
//   --no-check, --no-test   skip those steps
//   --no-build   only make sure the emulator runs, install the APK that is there and open the app
//
// Stops at the first step that fails, with its exit status. (From web/apps/rutsa. A test script,
// started through `vp run` and run by Node; it runs `vp`, `emulator`, `adb` and scripts/gradle.mjs,
// imports only Node's own modules, and is not part of the app, its build, any image or the shipped
// toolchain. Gradle runs under the mobile-toolchain exception of OR-34 through gradle.mjs.)
import { spawn, spawnSync } from "node:child_process";
import { existsSync, readdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const APP = fileURLToPath(new URL("../../", import.meta.url));
const PACKAGE = "org.obop.rutsa";
const AVD = "rutsa";
const TIME_ZONE = "Europe/Stockholm";
const APK = join(APP, "android", "app", "build", "outputs", "apk", "release", "app-release.apk");

const args = process.argv.slice(2);
const has = (flag) => args.includes(flag);
const option = (name, fallback) => {
  const at = args.indexOf(name);
  return at >= 0 ? args[at + 1] : fallback;
};
const serial = option("--serial", "emulator-5554");

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const stamp = () => new Date().toISOString().slice(11, 19);
const say = (text) => console.log(`\n${stamp()} ── ${text}`);

/** Runs a program in the app's folder with its output on this console; stops on a failure. */
function step(program, programArgs) {
  const result = spawnSync(program, programArgs, { cwd: APP, stdio: "inherit" });
  if (result.error) {
    console.error(`ship: ${program} could not be started: ${result.error.message}`);
    process.exit(1);
  }
  if (result.status !== 0) {
    console.error(`ship: ${program} ${programArgs.join(" ")} failed (status ${result.status})`);
    process.exit(result.status ?? 1);
  }
}

/** The output of adb for the device, or `null` when adb fails. */
function adb(parts) {
  const result = spawnSync("adb", ["-s", serial, ...parts], { encoding: "utf8" });
  return result.status === 0 ? result.stdout.trim() : null;
}

/** Whether the emulator is listed by adb as a device that is ready. */
function emulatorListed() {
  const result = spawnSync("adb", ["devices"], { encoding: "utf8" });
  return (result.stdout ?? "").split("\n").some((line) => line.trim() === `${serial}\tdevice`);
}

/** The emulator running and booted: started in its own window when none runs. */
async function ensureEmulator() {
  if (!emulatorListed()) {
    say(`starting the emulator ${AVD}`);
    // In its own process that outlives this script, as when it is started by hand.
    spawn("emulator", ["-avd", AVD, "-timezone", TIME_ZONE], {
      detached: true,
      stdio: "ignore",
    }).unref();
  }
  for (let i = 0; i < 180; i += 1) {
    if (emulatorListed() && adb(["shell", "getprop", "sys.boot_completed"]) === "1") {
      say("the emulator is ready");
      return;
    }
    await sleep(1000);
  }
  console.error("ship: the emulator did not boot within 3 minutes");
  process.exit(1);
}

/** The bundle of the last build and the cache of Metro, gone (`--fresh`). */
function freshBundle() {
  say("deleting the last bundle and the cache of Metro");
  for (const dir of ["assets", "res"]) {
    rmSync(join(APP, "android", "app", "build", "generated", dir, "react"), {
      recursive: true,
      force: true,
    });
  }
  for (const entry of readdirSync(tmpdir())) {
    if (entry.startsWith("metro-")) rmSync(join(tmpdir(), entry), { recursive: true, force: true });
  }
}

const started = Date.now();

if (!has("--no-check")) {
  say("vp check --fix");
  step("vp", ["check", "--fix"]);
}
if (!has("--no-test")) {
  say("vp test");
  step("vp", ["test"]);
}

// The emulator boots while the APK builds.
const emulator = ensureEmulator();

if (!has("--no-build")) {
  if (!existsSync(join(APP, "android"))) {
    say("expo prebuild (no android/ yet)");
    step("vp", ["run", "android:prebuild"]);
  }
  if (has("--fresh")) freshBundle();
  say("building the release APK");
  step("node", ["scripts/gradle.mjs", "assembleRelease"]);
}

await emulator;

if (!existsSync(APK)) {
  console.error(`ship: no APK at ${APK}; build without --no-build first`);
  process.exit(1);
}
say("installing over the last build (the trips stay)");
step("adb", ["-s", serial, "install", "-r", APK]);

say("opening Rutsa");
// No force-stop: the install has ended the old process, and a recording comes back by itself (C5).
adb(["shell", "monkey", "-p", PACKAGE, "-c", "android.intent.category.LAUNCHER", "1"]);

const minutes = ((Date.now() - started) / 60_000).toFixed(1);
console.log(`\n${stamp()} ── done in ${minutes} min: Rutsa is open on the emulator`);
