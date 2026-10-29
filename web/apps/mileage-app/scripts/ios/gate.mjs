// The release gate for a built iOS app: reads the Info.plist and the embedded app config of a built
// `.app` and fails when the build is not what the mode says (rules: gate-lib.mjs).
//
//   node scripts/ios/gate.mjs path/to/App.app                 # a release build: fails on plain http
//   node scripts/ios/gate.mjs path/to/App.app --test-build    # a test build: reports it
//   node scripts/ios/gate.mjs --info-plist-json FILE --app-config FILE [--test-build]   # from files, any OS
//
// A `.app` is read with `plutil` (macOS only). Exit status: 0 passed, 1 failed, 2 wrong use.
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { checkBuild } from "./gate-lib.mjs";

function usage(message) {
  console.error(`gate.mjs: ${message}`);
  console.error("usage: node scripts/ios/gate.mjs APP.app [--test-build]");
  console.error(
    "       node scripts/ios/gate.mjs --info-plist-json FILE --app-config FILE [--test-build]",
  );
  process.exit(2);
}

const args = process.argv.slice(2);
const mode = args.includes("--test-build") ? "test" : "release";
const valueOf = (name) => {
  const at = args.indexOf(name);
  return at >= 0 ? args[at + 1] : undefined;
};
const takesValue = new Set(["--info-plist-json", "--app-config"]);
const positional = args.filter((arg, i) => !arg.startsWith("--") && !takesValue.has(args[i - 1]));

let infoPlist;
let config;
const plistFile = valueOf("--info-plist-json");
const configFile = valueOf("--app-config");
try {
  if (plistFile !== undefined || configFile !== undefined) {
    if (plistFile === undefined || configFile === undefined) {
      usage("give both --info-plist-json and --app-config");
    }
    infoPlist = JSON.parse(readFileSync(plistFile, "utf8"));
    config = JSON.parse(readFileSync(configFile, "utf8"));
  } else {
    if (positional.length !== 1) usage("give the path of one built .app");
    const app = positional[0];
    if (process.platform !== "darwin") usage("reading a .app needs plutil, which only macOS has");
    const plist = join(app, "Info.plist");
    const embedded = join(app, "EXConstants.bundle", "app.config");
    if (!existsSync(plist)) usage(`${plist} does not exist`);
    if (!existsSync(embedded)) usage(`${embedded} does not exist (is this an Expo build?)`);
    const converted = spawnSync("plutil", ["-convert", "json", "-o", "-", "--", plist], {
      encoding: "utf8",
    });
    if (converted.status !== 0) usage(`plutil could not read ${plist}: ${converted.stderr}`);
    infoPlist = JSON.parse(converted.stdout);
    config = JSON.parse(readFileSync(embedded, "utf8"));
  }
} catch (error) {
  usage(error instanceof Error ? error.message : String(error));
}

const result = checkBuild({ config, infoPlist }, { mode });
for (const [name, value] of Object.entries(result.facts)) {
  console.log(`${name.padEnd(17)} ${value}`);
}
for (const finding of result.findings) {
  const tag = finding.level === "fail" ? "FAIL" : finding.level === "warn" ? "warn" : "ok  ";
  console.log(`${tag}  ${finding.text}`);
}
console.log(result.ok ? `gate passed (${mode} build)` : `gate FAILED (${mode} build)`);
process.exit(result.ok ? 0 : 1);
