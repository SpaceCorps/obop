// The rules of the iOS release gate (`gate.mjs` reads a built app and applies them). Pure, so that
// `vp test` runs them on any machine (scripts/ios/gate.test.ts).
//
// Why it exists: the build phase of expo-constants evaluates `app.config.ts` again at every Xcode
// build with the environment of that build. A build from the Xcode window does not see variables
// exported in another shell, and a forgotten `.env.local` can turn plain http on in a build that
// was meant as a release. The address check of the sign-in is the app's own guard; this is the
// check of the result.

/** @typedef {{ level: "fail" | "warn" | "ok", text: string }} Finding */

const WHEN_IN_USE_ONLY_KEYS = [
  "NSLocationAlwaysAndWhenInUseUsageDescription",
  "NSLocationAlwaysUsageDescription",
];

/** The bundle id of the owner's test builds (OBOP_IOS_BUNDLE_ID, docs/spec/mobile.md 8.8). */
const TEST_BUNDLE_ID = "org.obop.mileage.test";

const isObject = (value) => typeof value === "object" && value !== null && !Array.isArray(value);

/**
 * An address as the gate compares two of them: scheme, host (lower case), port and path, a trailing
 * slash ignored. `undefined` when the text is not an address.
 * @param {string} text
 * @returns {string | undefined}
 */
export function addressOf(text) {
  try {
    const url = new URL(text);
    return `${url.protocol}//${url.host}${url.pathname.replace(/\/+$/, "")}`;
  } catch {
    return undefined;
  }
}

/**
 * @param {{ config: unknown, infoPlist: unknown }} built the embedded app config (EXConstants.bundle/app.config) and the Info.plist of the built app, as JSON values
 * @param {{ mode: "release" | "test", drive?: boolean, expectServer?: string, expectBundleId?: string }} options `release` fails a build that allows plain http; `test` reports it; `drive` (test mode only) is "this build is for the first drive on a phone" and fails a build that lacks plain http, the store-every-fix flag or an http default server; with `drive`, `expectServer` (an http:// address) and `expectBundleId` also fail a build whose default server or bundle id is another one
 * @returns {{ ok: boolean, findings: Finding[], facts: Record<string, string> }}
 */
export function checkBuild(built, { mode, drive = false, expectServer, expectBundleId }) {
  if (drive && mode !== "test") {
    throw new Error("the drive check is for a test build: it needs mode `test`");
  }
  if (!drive && (expectServer !== undefined || expectBundleId !== undefined)) {
    throw new Error("expectServer and expectBundleId belong to the drive check: they need `drive`");
  }
  const expectedServer = expectServer === undefined ? undefined : addressOf(expectServer);
  if (expectServer !== undefined && !expectedServer?.startsWith("http://")) {
    throw new Error(`expectServer must be an http:// address, got ${JSON.stringify(expectServer)}`);
  }
  /** @type {Finding[]} */
  const findings = [];
  const fail = (text) => findings.push({ level: "fail", text });
  const warn = (text) => findings.push({ level: "warn", text });
  const ok = (text) => findings.push({ level: "ok", text });

  const config = isObject(built.config) ? built.config : {};
  const plist = isObject(built.infoPlist) ? built.infoPlist : {};
  const extra = isObject(config.extra) ? config.extra : {};
  const cleartext = extra.cleartextAllowed;
  const defaultServer = typeof extra.defaultServer === "string" ? extra.defaultServer : "";

  // Plain http.
  if (cleartext === true) {
    if (mode === "release") {
      fail(
        "the embedded config has cleartextAllowed=true: this build speaks plain http and is not a release build",
      );
    } else {
      ok("test build: plain http is allowed (cleartextAllowed=true)");
    }
  } else if (cleartext === false) {
    ok("cleartextAllowed=false: the app connects over https only");
  } else {
    fail(
      "the embedded config has no extra.cleartextAllowed: the build was not made from this app.config.ts",
    );
  }
  // Fixes stored without thinning (OBOP_RECORD_THINNING=off) are for the first drive of a test build.
  // The app (src/core/thin.ts, `thinningFor`) honours the flag only when plain http is also allowed.
  const thinningOff = extra.thinningOff;
  if (thinningOff === true) {
    if (mode === "release") {
      fail(
        "the embedded config has thinningOff=true: OBOP_RECORD_THINNING=off was set for this build (the app then does not thin the fixes if plain http is also allowed, and ignores the flag if not); a release build must not carry it",
      );
    } else if (cleartext === true) {
      ok("test build: fixes are stored without thinning (thinningOff=true)");
    } else {
      // With --drive this is one of the three facts of the drive build, so it carries the same prefix.
      fail(
        `${drive ? "drive build: " : ""}the build ignores OBOP_RECORD_THINNING because plain http is not allowed (set OBOP_ALLOW_CLEARTEXT=1 too): it would thin the fixes`,
      );
    }
  }
  // A test bundle id with plain http off: the Xcode phase of expo-constants evaluates app.config.ts
  // again with the environment of the build, so a rebuild in a shell without the OBOP_ variables
  // keeps the test bundle id and drops the rest.
  const bundleId = typeof plist.CFBundleIdentifier === "string" ? plist.CFBundleIdentifier : "";
  if (mode === "test" && bundleId === TEST_BUNDLE_ID && cleartext === false) {
    warn(
      `the bundle id is ${TEST_BUNDLE_ID} but cleartextAllowed=false: OBOP_ALLOW_CLEARTEXT was not set for this build (it speaks https only and thins the fixes; if this build is for a drive, rebuild with the variables exported, and check it with --drive)`,
    );
  }
  // The build for the first drive on a phone (--drive): the three settings that make it useful must
  // all be in the embedded config. The rules above only warn about a missing one, because a test
  // build can be made for other reasons; for the drive a build without them is useless (it speaks
  // https only to a test instance on http, or thins the fixes the drive is meant to keep).
  if (drive) {
    const missing = [];
    if (cleartext !== true) {
      missing.push(
        `cleartextAllowed is ${String(cleartext)}, it must be true (set OBOP_ALLOW_CLEARTEXT=1)`,
      );
    }
    if (thinningOff !== true) {
      missing.push(
        `thinningOff is ${String(thinningOff)}, it must be true (set OBOP_RECORD_THINNING=off)`,
      );
    }
    if (!/^http:\/\//i.test(defaultServer)) {
      missing.push(
        `defaultServer is ${defaultServer === "" ? "empty" : JSON.stringify(defaultServer)}, it must be an http:// address (set OBOP_DEFAULT_SERVER=http://ADDRESS:PORT)`,
      );
    } else if (expectedServer !== undefined && addressOf(defaultServer) !== expectedServer) {
      missing.push(
        `defaultServer is ${JSON.stringify(defaultServer)}, it must be ${expectServer} (set OBOP_DEFAULT_SERVER=${expectServer})`,
      );
    }
    if (expectBundleId !== undefined && bundleId !== expectBundleId) {
      missing.push(
        `the bundle id is ${bundleId === "" ? "missing from Info.plist" : JSON.stringify(bundleId)}, it must be ${expectBundleId} (set OBOP_IOS_BUNDLE_ID=${expectBundleId})`,
      );
    }
    if (missing.length === 0) {
      ok(
        "drive build: plain http is allowed, the fixes are stored without thinning and the default server is an http address",
      );
      if (expectedServer !== undefined) {
        ok(`drive build: the default server is ${expectServer}, as expected`);
      }
      if (expectBundleId !== undefined) {
        ok(`drive build: the bundle id is ${expectBundleId}, as expected`);
      }
    } else {
      for (const text of missing) fail(`drive build: ${text}`);
      fail(
        "drive build: in this Terminal run `env | grep OBOP_`, export what is missing, build again (`vp run ios:run:device` for the phone, `vp run ios:build:simulator Release` for a Simulator build), then run this gate again",
      );
    }
  }
  if (mode === "release" && defaultServer !== "" && !/^https:\/\//i.test(defaultServer)) {
    fail(
      `the default server of a release build must be an https address, got ${JSON.stringify(defaultServer)}`,
    );
  }

  // App Transport Security: no exception of the app's own (decision 3 of the iOS port). The
  // template's NSAllowsLocalNetworking is not one.
  const ats = isObject(plist.NSAppTransportSecurity) ? plist.NSAppTransportSecurity : {};
  const domains = isObject(ats.NSExceptionDomains) ? Object.keys(ats.NSExceptionDomains) : [];
  if (ats.NSAllowsArbitraryLoads === true) {
    fail(
      "Info.plist: NSAllowsArbitraryLoads is true (App Transport Security is off for the whole app)",
    );
  } else if (
    ats.NSAllowsArbitraryLoadsInWebContent === true ||
    ats.NSAllowsArbitraryLoadsForMedia === true
  ) {
    fail("Info.plist: App Transport Security is relaxed for web content or media");
  } else if (domains.length > 0) {
    fail(`Info.plist: App Transport Security has exception domains (${domains.join(", ")})`);
  } else {
    ok("Info.plist: no App Transport Security exception of the app's own");
  }

  // Background modes: location only.
  const modes = Array.isArray(plist.UIBackgroundModes) ? plist.UIBackgroundModes : [];
  if (modes.length === 1 && modes[0] === "location") {
    ok("Info.plist: the only background mode is location");
  } else {
    fail(`Info.plist: UIBackgroundModes must be exactly [location], got ${JSON.stringify(modes)}`);
  }

  // Location: When In Use only.
  if (
    typeof plist.NSLocationWhenInUseUsageDescription === "string" &&
    plist.NSLocationWhenInUseUsageDescription !== ""
  ) {
    ok("Info.plist: the When In Use text is set");
  } else {
    fail("Info.plist: NSLocationWhenInUseUsageDescription is missing");
  }
  for (const key of WHEN_IN_USE_ONLY_KEYS) {
    if (key in plist) {
      fail(`Info.plist: ${key} is set, but the app asks for When In Use access only`);
    }
  }
  if ("NSMotionUsageDescription" in plist) {
    warn("Info.plist: NSMotionUsageDescription is set, but the app does not read motion activity");
  }

  // The local network text is ours, not the development client's.
  const local = plist.NSLocalNetworkUsageDescription;
  if (typeof local !== "string" || local === "") {
    fail("Info.plist: NSLocalNetworkUsageDescription is missing");
  } else if (/Expo Dev Launcher/.test(local)) {
    fail(
      "Info.plist: NSLocalNetworkUsageDescription is the text of the development client, not the app's own",
    );
  } else {
    ok("Info.plist: the local network text is the app's own");
  }
  // The development client's entries (app.config.ts takes them out unless OBOP_DEV_CLIENT=1).
  const bonjour = Array.isArray(plist.NSBonjourServices) ? plist.NSBonjourServices : [];
  const urlTypes = Array.isArray(plist.CFBundleURLTypes) ? plist.CFBundleURLTypes : [];
  const devSchemes = urlTypes
    .flatMap((type) =>
      isObject(type) && Array.isArray(type.CFBundleURLSchemes) ? type.CFBundleURLSchemes : [],
    )
    .filter((scheme) => typeof scheme === "string" && scheme.startsWith("exp+"));
  const devClientEntry = (text) => (mode === "release" ? fail(text) : warn(text));
  if (bonjour.includes("_expo._tcp")) {
    devClientEntry(
      "Info.plist: NSBonjourServices holds _expo._tcp, an entry of the development client that a build for a phone does not need",
    );
  } else if (bonjour.length > 0 && mode === "release") {
    warn(`Info.plist: NSBonjourServices is set (${bonjour.join(", ")})`);
  }
  if (devSchemes.length > 0) {
    devClientEntry(
      `Info.plist: the URL scheme ${devSchemes.join(", ")} is an entry of the development client that a build for a phone does not need`,
    );
  }

  const facts = {
    bundleId: bundleId === "" ? "(unknown)" : bundleId,
    cleartextAllowed: String(cleartext),
    defaultServer: defaultServer === "" ? "(none)" : defaultServer,
    backgroundModes: JSON.stringify(modes),
    thinningOff: String(thinningOff === true),
    mode,
  };
  return { ok: !findings.some((f) => f.level === "fail"), findings, facts };
}
