// The release gate for a built Android APK: reads its manifest, its permissions and the embedded
// Expo config and fails when the build is not what the mode says (rules: gate-lib.mjs).
//
//   node scripts/gate.mjs path/to/app-release.apk               # a release build: fails on plain http or DevTools
//   node scripts/gate.mjs path/to/app-release.apk --test-build  # a test build: reports the same facts
//   node scripts/gate.mjs --manifest FILE --permissions FILE --app-config FILE [--test-build]   # from files, any machine
//
// An APK is read with `apkanalyzer` of the Android SDK command-line tools (ANDROID_HOME or
// ANDROID_SDK_ROOT; it needs Java, so JAVA_HOME or a `java` on PATH). The embedded config is the
// file `assets/app.config` of the APK, which expo-constants writes at the Gradle build.
//
// Exit status: 0 passed, 1 failed, 2 wrong use or an APK that could not be read.
import { spawnSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { checkBuild } from "./gate-lib.mjs";

/** Where expo-constants puts the evaluated app config in an APK. */
const EMBEDDED_CONFIG = "/assets/app.config";

function usage(message) {
  console.error(`gate.mjs: ${message}`);
  console.error("usage: node scripts/gate.mjs APK [--test-build]");
  console.error(
    "       node scripts/gate.mjs --manifest FILE --permissions FILE --app-config FILE [--test-build]",
  );
  process.exit(2);
}

const args = process.argv.slice(2);
const mode = args.includes("--test-build") ? "test" : "release";
const takesValue = new Set(["--manifest", "--permissions", "--app-config"]);
const known = new Set([...takesValue, "--test-build"]);
for (const arg of args) {
  if (arg.startsWith("--") && !known.has(arg)) usage(`unknown option ${arg}`);
}
const valueOf = (name) => {
  const at = args.indexOf(name);
  if (at < 0) return undefined;
  const value = args[at + 1];
  if (value === undefined || value.startsWith("--")) usage(`${name} takes a file`);
  return value;
};
const positional = args.filter((arg, i) => !arg.startsWith("--") && !takesValue.has(args[i - 1]));

/** The apkanalyzer of the newest command-line tools in the SDK. */
function findApkanalyzer() {
  const sdk = process.env.ANDROID_HOME ?? process.env.ANDROID_SDK_ROOT;
  if (!sdk) usage("ANDROID_HOME is not set: the gate reads an APK with the SDK's apkanalyzer");
  const root = resolve(sdk, "cmdline-tools");
  const name = process.platform === "win32" ? "apkanalyzer.bat" : "apkanalyzer";
  const versions = existsSync(root) ? readdirSync(root) : [];
  // `latest` first, then the highest version number.
  versions.sort((a, b) =>
    a === "latest" ? -1 : b === "latest" ? 1 : b.localeCompare(a, "en", { numeric: true }),
  );
  for (const version of versions) {
    const tool = resolve(root, version, "bin", name);
    if (existsSync(tool)) return tool;
  }
  return usage(`no apkanalyzer under ${root}: install the Android SDK command-line tools`);
}

/** One apkanalyzer command on the APK; its standard output. */
function analyse(tool, apk, command) {
  let result;
  if (process.platform === "win32") {
    // The tool is a .bat file, so cmd.exe has to run it; the command line is passed verbatim (as
    // scripts/gradle.mjs does). The words of `command` are fixed text of this script; the two paths
    // are the only other parts, so refuse the characters cmd.exe would interpret in them.
    for (const path of [tool, apk]) {
      if (/["%^&|<>!]/.test(path)) {
        usage(`the path ${path} contains a character that cmd.exe cannot pass safely`);
      }
    }
    const line = `"${tool}" ${command.join(" ")} "${apk}"`;
    result = spawnSync("cmd.exe", ["/d", "/s", "/c", `"${line}"`], {
      encoding: "utf8",
      windowsVerbatimArguments: true,
      maxBuffer: 64 * 1024 * 1024,
    });
  } else {
    result = spawnSync(tool, [...command, apk], { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
  }
  if (result.error) usage(`could not start apkanalyzer: ${result.error.message}`);
  if (result.status !== 0) {
    usage(
      `apkanalyzer ${command.join(" ")} failed with status ${result.status}: ${(result.stderr || result.stdout).trim()}`,
    );
  }
  return result.stdout;
}

let built;
try {
  const files = [valueOf("--manifest"), valueOf("--permissions"), valueOf("--app-config")];
  if (files.some((file) => file !== undefined)) {
    if (files.some((file) => file === undefined) || positional.length > 0) {
      usage("give --manifest, --permissions and --app-config together, and no APK");
    }
    const [manifest, permissions, config] = files.map((file) => readFileSync(file, "utf8"));
    built = { manifest, permissions, config: JSON.parse(config) };
  } else {
    if (positional.length !== 1) usage("give the path of one APK");
    const apk = resolve(positional[0]);
    if (!existsSync(apk)) usage(`${apk} does not exist`);
    const tool = findApkanalyzer();
    built = {
      manifest: analyse(tool, apk, ["manifest", "print"]),
      permissions: analyse(tool, apk, ["manifest", "permissions"]),
      config: JSON.parse(analyse(tool, apk, ["files", "cat", "--file", EMBEDDED_CONFIG])),
    };
    console.log(`apk               ${apk}`);
    console.log(`embedded config   ${EMBEDDED_CONFIG}`);
  }
} catch (error) {
  usage(error instanceof Error ? error.message : String(error));
}

const result = checkBuild(built, { mode });
for (const [name, value] of Object.entries(result.facts)) {
  console.log(`${name.padEnd(17)} ${value}`);
}
for (const finding of result.findings) {
  const tag = finding.level === "fail" ? "FAIL" : finding.level === "warn" ? "warn" : "ok  ";
  console.log(`${tag}  ${finding.text}`);
}
console.log(result.ok ? `gate passed (${mode} build)` : `gate FAILED (${mode} build)`);
process.exit(result.ok ? 0 : 1);
