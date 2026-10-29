// Runs the Apple tools of the generated ios/ project (created by `expo prebuild`) so that `vp run`
// can start them (the mobile-toolchain exception, OR-34: CocoaPods and Xcode are not run by Vite+).
// macOS only.
//
//   node scripts/ios/xcode.mjs pods                       # pod install in ios/
//   node scripts/ios/xcode.mjs build-simulator [Debug|Release]   # an app for the iOS Simulator, signed ad hoc, with no account
//
// OBOP_IOS_ARCHS (default arm64) picks the architectures of the Simulator build. The result is
// ios/build/Build/Products/<configuration>-iphonesimulator/<scheme>.app; the path is printed.
import { spawnSync } from "node:child_process";
import { existsSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { simulatorAppPath, simulatorBuildArgs } from "./xcode-lib.mjs";

const iosDir = fileURLToPath(new URL("../../ios/", import.meta.url)).replace(/[\\/]$/, "");

function fail(message, code = 1) {
  console.error(`xcode.mjs: ${message}`);
  process.exit(code);
}

if (process.platform !== "darwin") fail("this runs on a Mac (CocoaPods and Xcode need macOS)");
if (!existsSync(iosDir)) fail(`${iosDir} does not exist: run "vp run ios:prebuild" first`);

const [command, ...rest] = process.argv.slice(2);

function run(program, args, cwd) {
  const result = spawnSync(program, args, {
    cwd,
    stdio: "inherit",
    // CocoaPods needs a UTF-8 locale and stops with an error otherwise.
    env: { ...process.env, LANG: process.env.LANG ?? "en_US.UTF-8" },
  });
  if (result.error) fail(`could not start ${program}: ${result.error.message}`);
  if (result.status !== 0) process.exit(result.status ?? 1);
}

if (command === "pods") {
  run("pod", ["install"], iosDir);
} else if (command === "build-simulator") {
  const workspaces = readdirSync(iosDir).filter((name) => name.endsWith(".xcworkspace"));
  if (workspaces.length !== 1) {
    fail(
      `expected exactly one .xcworkspace in ${iosDir} (run "vp run ios:pods" first), found ${workspaces.length}`,
    );
  }
  const scheme = workspaces[0].replace(/\.xcworkspace$/, "");
  const options = {
    workspace: `${iosDir}/${workspaces[0]}`,
    scheme,
    configuration: rest[0] ?? "Release",
    archs: process.env.OBOP_IOS_ARCHS ?? "arm64",
    derivedData: `${iosDir}/build`,
  };
  let args;
  try {
    args = simulatorBuildArgs(options);
  } catch (error) {
    fail(error instanceof Error ? error.message : String(error), 2);
  }
  run("xcodebuild", args, iosDir);
  console.log(`built: ${simulatorAppPath(options)}`);
} else {
  fail('give "pods" or "build-simulator"', 2);
}
