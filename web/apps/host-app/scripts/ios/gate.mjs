// The gate for a built iOS app of the host: reads the Info.plist, the embedded app config and the
// signature of a built `.app` and fails when the build is not what the mode says (rules:
// gate-lib.mjs).
//
//   node scripts/ios/gate.mjs path/to/obop.app                 # a plain build: fails on plain http, DevTools or an ATS exception
//   node scripts/ios/gate.mjs path/to/obop.app --test-build    # a test build: reports them
//   node scripts/ios/gate.mjs --info-plist-json FILE --app-config FILE [--codesign-text FILE] [--profile] [--test-build]   # from files, any OS
//
// Every mode fails an app that holds a provisioning profile, is signed for a team or with a
// certificate: a build of the host is for the Simulator. A `.app` is read with `plutil` and
// `codesign` (macOS only). Exit status: 0 passed, 1 failed, 2 wrong use.
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { checkBuild, signingFacts } from "./gate-lib.mjs";

function usage(message) {
  console.error(`gate.mjs: ${message}`);
  console.error("usage: node scripts/ios/gate.mjs APP.app [--test-build]");
  console.error(
    "       node scripts/ios/gate.mjs --info-plist-json FILE --app-config FILE [--codesign-text FILE] [--profile] [--test-build]",
  );
  process.exit(2);
}

const args = process.argv.slice(2);
const mode = args.includes("--test-build") ? "test" : "release";
const takesValue = new Set(["--info-plist-json", "--app-config", "--codesign-text"]);
const valueOf = (name) => {
  const at = args.indexOf(name);
  return at >= 0 ? args[at + 1] : undefined;
};
const known = new Set([...takesValue, "--test-build", "--profile"]);
for (const [i, arg] of args.entries()) {
  if (arg.startsWith("--") && !known.has(arg)) usage(`unknown option ${arg}`);
  if (takesValue.has(arg) && (args[i + 1] === undefined || args[i + 1].startsWith("--"))) {
    usage(`${arg} takes a value`);
  }
}
const positional = args.filter((arg, i) => !arg.startsWith("--") && !takesValue.has(args[i - 1]));

let infoPlist;
let config;
let signing;
const plistFile = valueOf("--info-plist-json");
const configFile = valueOf("--app-config");
try {
  if (plistFile !== undefined || configFile !== undefined) {
    if (plistFile === undefined || configFile === undefined) {
      usage("give both --info-plist-json and --app-config");
    }
    if (positional.length > 0) usage("give either an .app or the files, not both");
    infoPlist = JSON.parse(readFileSync(plistFile, "utf8"));
    config = JSON.parse(readFileSync(configFile, "utf8"));
    const codesignFile = valueOf("--codesign-text");
    const text =
      codesignFile === undefined
        ? "code object is not signed at all"
        : readFileSync(codesignFile, "utf8");
    signing = signingFacts(text, args.includes("--profile"));
  } else {
    if (args.includes("--profile") || args.includes("--codesign-text")) {
      usage("--profile and --codesign-text are for the files mode; an .app is read as it is");
    }
    if (positional.length !== 1) usage("give the path of one built .app");
    const app = positional[0];
    if (process.platform !== "darwin")
      usage("reading a .app needs plutil and codesign, which only macOS has");
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
    const signed = spawnSync("codesign", ["-d", "--verbose=2", "--", app], { encoding: "utf8" });
    if (signed.error) usage(`could not start codesign: ${signed.error.message}`);
    signing = signingFacts(
      `${signed.stdout}\n${signed.stderr}`,
      existsSync(join(app, "embedded.mobileprovision")),
    );
  }
} catch (error) {
  usage(error instanceof Error ? error.message : String(error));
}

const result = checkBuild({ config, infoPlist, signing }, { mode });
for (const [name, value] of Object.entries(result.facts)) {
  console.log(`${name.padEnd(17)} ${value}`);
}
for (const finding of result.findings) {
  const tag = finding.level === "fail" ? "FAIL" : finding.level === "warn" ? "warn" : "ok  ";
  console.log(`${tag}  ${finding.text}`);
}
console.log(result.ok ? `gate passed (${mode} build)` : `gate FAILED (${mode} build)`);
process.exit(result.ok ? 0 : 1);
