// Runs the Gradle wrapper of the generated android/ project (created by `expo prebuild`). The
// wrapper is not on PATH and its file name differs by platform, so `vp run` starts it through this
// script (the mobile-toolchain exception, OR-34: Gradle is not run by Vite+).
import { spawnSync } from "node:child_process";
import { existsSync, readdirSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const androidDir = fileURLToPath(new URL("../android/", import.meta.url));
const windows = process.platform === "win32";
const sdk = process.env.ANDROID_HOME ?? process.env.ANDROID_SDK_ROOT;

// Windows only. The CMake that the Android Gradle Plugin picks by default (3.22.1) bundles Ninja
// 1.10.2, which cannot read paths longer than 260 characters even when the system allows long
// paths. With pnpm's symlinked node_modules the sources of expo-modules-core live far deeper than
// that, and the build stops with "build.ninja still dirty after 100 tries". CMake 4.x bundles Ninja
// 1.12, which can. Install it with `sdkmanager "cmake;4.1.2"`; this script then points the build at
// it through the machine-local, git-ignored android/local.properties.
if (windows && sdk && existsSync(androidDir)) {
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
      'No CMake 4.x in the Android SDK: the build will probably fail on long paths. Run: sdkmanager "cmake;4.1.2"',
    );
  }
}

// A .bat file needs a shell on Windows; the path is quoted because the checkout may contain spaces.
const wrapper = windows ? `"${androidDir}gradlew.bat"` : `${androidDir}gradlew`;
const result = spawnSync(wrapper, process.argv.slice(2), {
  cwd: androidDir,
  stdio: "inherit",
  shell: windows,
});
process.exit(result.status ?? 1);
