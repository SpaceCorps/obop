// Runs the Gradle wrapper of the generated android/ project (created by `expo prebuild`). The
// wrapper is not on PATH and its file name differs by platform, so `vp run` starts it through this
// script (the mobile-toolchain exception, OR-34: Gradle is not run by Vite+).
//
//   node scripts/gradle.mjs <task> [more arguments]
//
// Extra arguments are checked against a strict pattern on every platform, so that Windows (where
// the wrapper is a .bat file run by cmd.exe) and Linux or macOS behave the same. Anything else, for
// example JVM options with spaces, goes into GRADLE_OPTS or gradle.properties in GRADLE_USER_HOME.
import { spawnSync } from "node:child_process";
import { existsSync, readdirSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const androidDir = fileURLToPath(new URL("../android/", import.meta.url));
const windows = process.platform === "win32";
const sdk = process.env.ANDROID_HOME ?? process.env.ANDROID_SDK_ROOT;

// Letters, digits and : . - _ = / \ only.
const SAFE_ARGUMENT = /^[A-Za-z0-9:.\-_=/\\]+$/;
// Two ABIs keep the build small: a phone (arm64-v8a) and the emulator on a PC (x86_64). The
// generated project builds four by default. A comma separated list of ABI names.
const abis = process.env.OBOP_ANDROID_ABIS ?? "arm64-v8a,x86_64";
const SAFE_ABIS = /^[a-z0-9_-]+(,[a-z0-9_-]+)*$/;

function fail(message, code = 1) {
  console.error(`gradle.mjs: ${message}`);
  process.exit(code);
}

const args = process.argv.slice(2);
if (args.length === 0) {
  fail("no Gradle task given, for example: node scripts/gradle.mjs assembleDebug");
}
for (const arg of args) {
  if (!SAFE_ARGUMENT.test(arg)) {
    fail(
      `refusing the argument ${JSON.stringify(arg)}: only letters, digits and : . - _ = / \\ are allowed. ` +
        "Put anything else into GRADLE_OPTS or into gradle.properties in GRADLE_USER_HOME.",
    );
  }
}
if (!SAFE_ABIS.test(abis)) {
  fail(
    `OBOP_ANDROID_ABIS must be a comma separated list of ABI names, got ${JSON.stringify(abis)}`,
  );
}
if (!existsSync(androidDir)) {
  fail(`${androidDir} does not exist: run "vp run android:prebuild" first`);
}

// Windows only. The CMake that the Android Gradle Plugin picks by default (3.22.1) bundles Ninja
// 1.10.2, which cannot read paths longer than 260 characters even when the system allows long
// paths. With pnpm's symlinked node_modules the sources of expo-modules-core live far deeper than
// that, and the build stops with "build.ninja still dirty after 100 tries". CMake 4.x bundles Ninja
// 1.12, which can. Install it with `sdkmanager "cmake;4.1.2"`; this script then points the build at
// it through the machine-local, git-ignored android/local.properties.
if (windows && sdk) {
  const cmakeRoot = `${sdk.replaceAll("\\", "/")}/cmake`;
  const newest = existsSync(cmakeRoot)
    ? readdirSync(cmakeRoot)
        .filter((version) => Number.parseInt(version, 10) >= 4)
        .sort((a, b) => a.localeCompare(b, "en", { numeric: true }))
        .at(-1)
    : undefined;
  if (newest) {
    writeFileSync(
      `${androidDir}local.properties`,
      `sdk.dir=${sdk.replaceAll("\\", "/")}\ncmake.dir=${cmakeRoot}/${newest}\n`,
    );
  } else {
    console.warn(
      'gradle.mjs: no CMake 4.x in the Android SDK, the build will probably fail on long paths. Run: sdkmanager "cmake;4.1.2"',
    );
  }
}

// CMake 4 refuses a project that asks for a minimum below 3.5, and expo-sqlite (with the vendored
// SQLCipher build) still says 3.4.1. CMake 4 reads this variable from the environment, so the files
// of the packages stay untouched; CMake 3 ignores it. A value already set by the caller wins.
const env = {
  ...process.env,
  CMAKE_POLICY_VERSION_MINIMUM: process.env.CMAKE_POLICY_VERSION_MINIMUM ?? "3.5",
};

const gradleArgs = [...args, `-PreactNativeArchitectures=${abis}`];
let result;
let shown;
if (windows) {
  // The wrapper is a .bat file, so cmd.exe has to run it. The command line is built by hand and
  // passed verbatim (no shell: true, which joins arguments without escaping). Every argument was
  // checked above; the path of the wrapper is the only other part, so refuse the characters that
  // cmd.exe would interpret in it.
  const wrapper = `${androidDir}gradlew.bat`;
  if (/["%^&|<>!]/.test(wrapper)) {
    fail(
      `the path ${wrapper} contains a character that cmd.exe cannot pass safely; move the checkout`,
    );
  }
  shown = `"${wrapper}" ${gradleArgs.join(" ")}`;
  result = spawnSync("cmd.exe", ["/d", "/s", "/c", `"${shown}"`], {
    cwd: androidDir,
    stdio: "inherit",
    env,
    windowsVerbatimArguments: true,
  });
} else {
  const wrapper = `${androidDir}gradlew`;
  shown = `${wrapper} ${gradleArgs.join(" ")}`;
  result = spawnSync(wrapper, gradleArgs, { cwd: androidDir, stdio: "inherit", env });
}

if (result.error) {
  fail(`could not start ${shown}: ${result.error.message}`);
}
if (result.status !== 0) {
  const reason = result.signal
    ? `was stopped by ${result.signal}`
    : `exited with code ${result.status}`;
  fail(`${shown} ${reason}`, result.status ?? 1);
}
