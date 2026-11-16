// The rules of the iOS gate of the host app (`gate.mjs` reads a built app and applies them). Pure,
// so that `vp test` runs them on any machine (scripts/ios/gate.test.ts). Adapted from the Mileage
// app's scripts/ios/gate-lib.mjs: the host has no location and no background mode, it has a
// DevTools switch, its App Transport Security differs between a test build and a plain one, and
// every build of it is for the Simulator, so a provisioning profile, a team or a signing identity
// fails it.
//
// Why it exists: the build phase of expo-constants evaluates `app.config.ts` again at every Xcode
// build with the environment of that build, so the variables of the prebuild and of the build can
// differ. This is the check of the result.

/** @typedef {{ level: "fail" | "warn" | "ok", text: string }} Finding */
/** @typedef {{ profile: boolean, signature: "none" | "adhoc" | "identity", team: string | null }} Signing */

const isObject = (value) => typeof value === "object" && value !== null && !Array.isArray(value);

/**
 * The signing facts of an app from the text `codesign -d --verbose=2 APP` writes to its standard
 * error (`code object is not signed at all` for an unsigned one).
 * @param {string} text
 * @param {boolean} profile whether the app holds embedded.mobileprovision
 * @returns {Signing}
 */
export function signingFacts(text, profile) {
  const team = /^TeamIdentifier=(.*)$/m.exec(text)?.[1]?.trim();
  /** @type {Signing["signature"]} */
  let signature;
  if (/not signed at all/.test(text)) signature = "none";
  else if (/^Signature=adhoc$/m.test(text)) signature = "adhoc";
  else signature = "identity";
  return {
    profile,
    signature,
    team: team === undefined || team === "" || team === "not set" ? null : team,
  };
}

/**
 * @param {{ config: unknown, infoPlist: unknown, signing: Signing }} built the embedded app config (EXConstants.bundle/app.config), the Info.plist of the built app as a JSON value, and its signing facts
 * @param {{ mode: "release" | "test" }} options `release` fails a build that may speak plain http, is open to DevTools or keeps an App Transport Security exception; `test` reports them
 * @returns {{ ok: boolean, findings: Finding[], facts: Record<string, string> }}
 */
export function checkBuild(built, { mode }) {
  /** @type {Finding[]} */
  const findings = [];
  const fail = (text) => findings.push({ level: "fail", text });
  const warn = (text) => findings.push({ level: "warn", text });
  const ok = (text) => findings.push({ level: "ok", text });
  const notRelease = (text) => (mode === "release" ? fail(text) : ok(`test build: ${text}`));

  const config = isObject(built.config) ? built.config : {};
  const plist = isObject(built.infoPlist) ? built.infoPlist : {};
  const extra = isObject(config.extra) ? config.extra : {};
  const cleartext = extra.cleartextAllowed;
  const debug = extra.webviewDebug;
  const defaultServer = typeof extra.defaultServer === "string" ? extra.defaultServer : "";

  // The switches of the embedded config.
  if (cleartext === true) notRelease("plain http is allowed (cleartextAllowed=true)");
  else if (cleartext === false) ok("cleartextAllowed=false: the app connects over https only");
  else {
    fail(
      "the embedded config has no extra.cleartextAllowed: the build was not made from this app.config.ts",
    );
  }
  if (debug === true) notRelease("the WebView is open to DevTools (webviewDebug=true)");
  else if (debug === false) ok("webviewDebug=false: the WebView is closed to DevTools");
  else fail("the embedded config has no extra.webviewDebug");
  if (mode === "release" && defaultServer !== "" && !/^https:\/\//i.test(defaultServer)) {
    fail(
      `the default server of a release build must be an https address, got ${JSON.stringify(defaultServer)}`,
    );
  }

  // App Transport Security (config/ios.ts, `withTransportSecurity`): no exception of any kind in a
  // build that speaks https only, and local networking alone in a test build.
  const ats = isObject(plist.NSAppTransportSecurity) ? plist.NSAppTransportSecurity : {};
  const domains = isObject(ats.NSExceptionDomains) ? Object.keys(ats.NSExceptionDomains) : [];
  const local = ats.NSAllowsLocalNetworking === true;
  if (ats.NSAllowsArbitraryLoads === true) {
    fail(
      "Info.plist: NSAllowsArbitraryLoads is true (App Transport Security is off for the whole app)",
    );
  }
  if (
    ats.NSAllowsArbitraryLoadsInWebContent === true ||
    ats.NSAllowsArbitraryLoadsForMedia === true
  ) {
    fail("Info.plist: App Transport Security is relaxed for web content or media");
  }
  if (domains.length > 0) {
    fail(`Info.plist: App Transport Security has exception domains (${domains.join(", ")})`);
  }
  if (local) {
    if (mode === "release") {
      fail(
        "Info.plist: NSAllowsLocalNetworking is true: plain http to this device and the local network is allowed",
      );
    } else if (cleartext === true) {
      ok("test build: Info.plist allows local networking (NSAllowsLocalNetworking), nothing more");
    } else {
      warn(
        "Info.plist: NSAllowsLocalNetworking is true but cleartextAllowed is not: the prebuild and the build had different variables",
      );
    }
  } else if (cleartext === true) {
    warn(
      "Info.plist: no NSAllowsLocalNetworking although cleartextAllowed=true: iOS will refuse the http address the app accepts (the prebuild ran without OBOP_ALLOW_CLEARTEXT=1)",
    );
  }
  if (!local && ats.NSAllowsArbitraryLoads !== true && domains.length === 0) {
    ok("Info.plist: no App Transport Security exception");
  }

  // Nothing of the Mileage app: no background mode, no location.
  const modes = Array.isArray(plist.UIBackgroundModes) ? plist.UIBackgroundModes : [];
  if (modes.length > 0) fail(`Info.plist: UIBackgroundModes is set (${JSON.stringify(modes)})`);
  else ok("Info.plist: no background mode");
  const location = Object.keys(plist).filter((key) => key.startsWith("NSLocation"));
  if (location.length > 0)
    fail(`Info.plist: the app asks for no location, but ${location.join(", ")} is set`);
  if ("NSFaceIDUsageDescription" in plist) {
    warn("Info.plist: NSFaceIDUsageDescription is set, but the app never asks for Face ID");
  }

  // The local network text is ours, not the development client's.
  const localText = plist.NSLocalNetworkUsageDescription;
  if (typeof localText !== "string" || localText === "") {
    fail("Info.plist: NSLocalNetworkUsageDescription is missing");
  } else if (/Expo Dev Launcher/.test(localText)) {
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
  if (bonjour.length > 0) {
    devClientEntry(
      `Info.plist: NSBonjourServices is set (${bonjour.join(", ")}); the app does no discovery`,
    );
  }
  if (devSchemes.length > 0) {
    devClientEntry(
      `Info.plist: the URL scheme ${devSchemes.join(", ")} is an entry of the development client`,
    );
  }

  // Signing: a build of the host is for the Simulator. Ad hoc or none; never a profile, a team or
  // a certificate.
  const signing = isObject(built.signing)
    ? built.signing
    : { profile: false, signature: "none", team: null };
  if (signing.profile === true) {
    fail("the app holds embedded.mobileprovision: it was signed with a provisioning profile");
  }
  if (typeof signing.team === "string" && signing.team !== "") {
    fail(`the app is signed for the team ${signing.team}`);
  }
  if (signing.signature === "identity") {
    fail("the app is signed with a certificate (not ad hoc)");
  }
  if (signing.profile !== true && !signing.team && signing.signature !== "identity") {
    ok(
      `signing: ${signing.signature === "adhoc" ? "ad hoc" : "none"}; no provisioning profile, no team, no certificate`,
    );
  }

  const bundleId = typeof plist.CFBundleIdentifier === "string" ? plist.CFBundleIdentifier : "";
  const facts = {
    bundleId: bundleId === "" ? "(unknown)" : bundleId,
    cleartextAllowed: String(cleartext),
    webviewDebug: String(debug),
    defaultServer: defaultServer === "" ? "(none)" : defaultServer,
    transportSecurity: JSON.stringify(plist.NSAppTransportSecurity ?? null),
    signing: `${signing.signature}${signing.team ? `, team ${signing.team}` : ""}${signing.profile ? ", profile" : ""}`,
    mode,
  };
  return { ok: !findings.some((f) => f.level === "fail"), findings, facts };
}
