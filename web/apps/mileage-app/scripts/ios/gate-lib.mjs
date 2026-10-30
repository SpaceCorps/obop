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

const isObject = (value) => typeof value === "object" && value !== null && !Array.isArray(value);

/**
 * @param {{ config: unknown, infoPlist: unknown }} built the embedded app config (EXConstants.bundle/app.config) and the Info.plist of the built app, as JSON values
 * @param {{ mode: "release" | "test" }} options `release` fails a build that allows plain http; `test` reports it
 * @returns {{ ok: boolean, findings: Finding[], facts: Record<string, string> }}
 */
export function checkBuild(built, { mode }) {
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
  const thinningOff = extra.thinningOff;
  if (thinningOff === true) {
    if (mode === "release") {
      fail(
        "the embedded config has thinningOff=true: this build stores every fix and is not a release build",
      );
    } else {
      ok("test build: fixes are stored without thinning (thinningOff=true)");
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
    bundleId: typeof plist.CFBundleIdentifier === "string" ? plist.CFBundleIdentifier : "(unknown)",
    cleartextAllowed: String(cleartext),
    defaultServer: defaultServer === "" ? "(none)" : defaultServer,
    backgroundModes: JSON.stringify(modes),
    thinningOff: String(thinningOff === true),
    mode,
  };
  return { ok: !findings.some((f) => f.level === "fail"), findings, facts };
}
