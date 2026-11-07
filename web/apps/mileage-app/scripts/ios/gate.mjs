// The release gate for a built iOS app: reads the Info.plist and the embedded app config of a built
// `.app` and fails when the build is not what the mode says (rules: gate-lib.mjs).
//
//   node scripts/ios/gate.mjs path/to/App.app                 # a release build: fails on plain http
//   node scripts/ios/gate.mjs path/to/App.app --test-build    # a test build: reports it
//   node scripts/ios/gate.mjs path/to/App.app --test-build --drive   # the build for the first drive on a phone
//   node scripts/ios/gate.mjs path/to/App.app --test-build --drive --expect-server http://192.168.1.57:18080 --expect-bundle-id org.obop.mileage.test
//   node scripts/ios/gate.mjs --info-plist-json FILE --app-config FILE [--test-build [--drive ...]]   # from files, any OS
//
// --drive ("this build is for the first drive"): the gate also fails unless the embedded config has
// cleartextAllowed=true, thinningOff=true and an http:// default server, i.e. unless the build was
// made with OBOP_ALLOW_CLEARTEXT=1, OBOP_RECORD_THINNING=off and OBOP_DEFAULT_SERVER=http://... It
// needs --test-build. With --drive, --expect-server URL (an http:// address; the same scheme, host,
// port and path, a trailing slash ignored) and --expect-bundle-id ID also fail a build whose default
// server or bundle id is another one: the first checks that the build points at the instance of this
// test, the second that it is the build of the test bundle id (OBOP_IOS_BUNDLE_ID). Without them
// nothing is compared.
//
// A `.app` is read with `plutil` (macOS only). Exit status: 0 passed, 1 failed, 2 wrong use.
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { addressOf, checkBuild } from "./gate-lib.mjs";

function usage(message) {
  console.error(`gate.mjs: ${message}`);
  console.error(
    "usage: node scripts/ios/gate.mjs APP.app [--test-build [--drive [--expect-server URL] [--expect-bundle-id ID]]]",
  );
  console.error(
    "       node scripts/ios/gate.mjs --info-plist-json FILE --app-config FILE [--test-build [--drive ...]]",
  );
  process.exit(2);
}

const args = process.argv.slice(2);
const mode = args.includes("--test-build") ? "test" : "release";
const drive = args.includes("--drive");
if (drive && mode !== "test") usage("--drive is for a test build: add --test-build");
const valueOf = (name) => {
  const at = args.indexOf(name);
  return at >= 0 ? args[at + 1] : undefined;
};
const takesValue = new Set([
  "--info-plist-json",
  "--app-config",
  "--expect-server",
  "--expect-bundle-id",
]);
/** The value of --expect-server or --expect-bundle-id: undefined when the option is not given. */
function expected(name) {
  if (!args.includes(name)) return undefined;
  if (!drive) usage(`${name} belongs to --drive: add --test-build --drive`);
  const value = valueOf(name);
  if (value === undefined || value === "" || value.startsWith("--")) {
    usage(`${name} takes a value`);
  }
  return value;
}
const expectServer = expected("--expect-server");
if (expectServer !== undefined && addressOf(expectServer)?.startsWith("http://") !== true) {
  usage(
    `--expect-server must be an http:// address such as http://192.168.1.57:18080, got ${expectServer}`,
  );
}
const expectBundleId = expected("--expect-bundle-id");
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

const result = checkBuild({ config, infoPlist }, { mode, drive, expectServer, expectBundleId });
for (const [name, value] of Object.entries(result.facts)) {
  console.log(`${name.padEnd(17)} ${value}`);
}
for (const finding of result.findings) {
  const tag = finding.level === "fail" ? "FAIL" : finding.level === "warn" ? "warn" : "ok  ";
  console.log(`${tag}  ${finding.text}`);
}
console.log(result.ok ? `gate passed (${mode} build)` : `gate FAILED (${mode} build)`);
process.exit(result.ok ? 0 : 1);
