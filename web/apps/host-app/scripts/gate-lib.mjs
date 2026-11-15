// The rules of the Android release gate (`gate.mjs` reads a built APK and applies them). Pure, so
// that `vp test` runs them on any machine (scripts/gate.test.ts). After the gate of the Mileage app
// (web/apps/mileage-app/scripts/ios/gate-lib.mjs).
//
// Why it exists: `app.config.ts` is evaluated with the environment of the prebuild and again with
// the environment of the Gradle build. A forgotten variable or `.env.local` can switch plain http
// or DevTools on in a build that was meant as a release. The address rule of the app is its own
// guard; this is the check of the result.

/** @typedef {{ level: "fail" | "warn" | "ok", text: string }} Finding */

/**
 * @typedef {object} ManifestFacts
 * @property {string} packageName
 * @property {string} versionName
 * @property {string} versionCode
 * @property {string | undefined} usesCleartextTraffic the text of the attribute, `undefined` when the manifest does not have it
 * @property {string | undefined} debuggable
 * @property {string | undefined} allowBackup
 * @property {boolean} hasApplication whether the text holds an `<application>` element at all
 */

const isObject = (value) => typeof value === "object" && value !== null && !Array.isArray(value);

/** The value of one attribute in the text of a start tag. */
function attribute(tag, name) {
  const match = new RegExp(`(?:^|\\s)${name.replace(/[.:]/g, "\\$&")}="([^"]*)"`).exec(tag);
  return match ? match[1] : undefined;
}

/** The attributes of the first start tag of an element, as its text; `undefined` without one. */
function startTag(xml, element) {
  const match = new RegExp(`<${element}(\\s[^>]*)?>`).exec(xml);
  return match ? (match[1] ?? "") : undefined;
}

/**
 * What the gate reads from the manifest as `apkanalyzer manifest print` writes it.
 * @param {string} xml
 * @returns {ManifestFacts}
 */
export function manifestFacts(xml) {
  const manifest = startTag(xml, "manifest") ?? "";
  const application = startTag(xml, "application");
  const of = (name) => (application === undefined ? undefined : attribute(application, name));
  return {
    packageName: attribute(manifest, "package") ?? "",
    versionName: attribute(manifest, "android:versionName") ?? "",
    versionCode: attribute(manifest, "android:versionCode") ?? "",
    usesCleartextTraffic: of("android:usesCleartextTraffic"),
    debuggable: of("android:debuggable"),
    allowBackup: of("android:allowBackup"),
    hasApplication: application !== undefined,
  };
}

/**
 * The permissions of `apkanalyzer manifest permissions`: one name per line.
 * @param {string} text
 * @returns {string[]}
 */
export function permissionList(text) {
  return text
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => /^[A-Za-z0-9_.]+$/.test(line))
    .sort();
}

/**
 * The permissions a build of this app is expected to have: the network, and the permission React
 * Native declares for its own receivers under the application id.
 * @param {string} packageName
 * @returns {string[]}
 */
export function expectedPermissions(packageName) {
  return [
    "android.permission.INTERNET",
    `${packageName}.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION`,
  ].sort();
}

/**
 * @param {{ manifest: string, permissions: string, config: unknown }} built the manifest and the permission list as apkanalyzer prints them, and the embedded Expo config (assets/app.config) as a JSON value
 * @param {{ mode: "release" | "test" }} options `release` fails a build that speaks plain http or is open to DevTools; `test` reports it
 * @returns {{ ok: boolean, findings: Finding[], facts: Record<string, string> }}
 */
export function checkBuild(built, { mode }) {
  /** @type {Finding[]} */
  const findings = [];
  const fail = (text) => findings.push({ level: "fail", text });
  const warn = (text) => findings.push({ level: "warn", text });
  const ok = (text) => findings.push({ level: "ok", text });
  /** A fact that keeps a build from being a release: a failure there, a statement in a test build. */
  const notARelease = (text) => (mode === "release" ? fail(text) : ok(`test build: ${text}`));

  const manifest = manifestFacts(built.manifest);
  const permissions = permissionList(built.permissions);
  const config = isObject(built.config) ? built.config : {};
  const extra = isObject(config.extra) ? config.extra : undefined;

  // The manifest.
  if (!manifest.hasApplication) {
    fail("the manifest has no <application> element: the APK could not be read");
  } else {
    if (manifest.usesCleartextTraffic === "true") {
      notARelease(
        "the manifest has usesCleartextTraffic=true: Android lets this build speak plain http",
      );
    } else {
      ok(
        `the manifest has usesCleartextTraffic=${manifest.usesCleartextTraffic ?? "unset (false from Android 9)"}`,
      );
    }
    if (manifest.debuggable === "true") {
      notARelease("the manifest has debuggable=true: any computer with adb reads this app's data");
    } else {
      ok(`the manifest has debuggable=${manifest.debuggable ?? "unset (false)"}`);
    }
    if (manifest.allowBackup === "false") {
      ok("the manifest has allowBackup=false");
    } else {
      warn(
        `the manifest has allowBackup=${manifest.allowBackup ?? "unset (true)"}: the WebView's data would go into a cloud backup`,
      );
    }
  }

  // The embedded Expo config: what the app itself reads at run time.
  if (extra === undefined) {
    fail("the embedded config has no `extra`: the build was not made from this app.config.ts");
  } else {
    const flag = (name, means) => {
      const value = extra[name];
      if (value === true) {
        notARelease(`the embedded config has ${name}=true: ${means}`);
      } else if (value === false) {
        ok(`the embedded config has ${name}=false`);
      } else {
        fail(
          `the embedded config has no extra.${name}: the build was not made from this app.config.ts`,
        );
      }
    };
    flag("cleartextAllowed", "the app takes http addresses of the local network");
    flag("webviewDebug", "the WebView is open to DevTools, which reads the session");
    const server = extra.defaultServer;
    if (typeof server !== "string") {
      fail(
        "the embedded config has no extra.defaultServer: the build was not made from this app.config.ts",
      );
    } else if (/^\s*http:/i.test(server)) {
      notARelease(
        `the embedded config has the default server ${JSON.stringify(server)}, an http address`,
      );
    } else {
      ok(
        server === ""
          ? "the embedded config has no default server"
          : `the embedded config has the default server ${JSON.stringify(server)}`,
      );
    }
    // The two switches of plain http must agree, or one half of the build believes the other.
    if (
      manifest.hasApplication &&
      (manifest.usesCleartextTraffic === "true") !== (extra.cleartextAllowed === true)
    ) {
      warn(
        "the manifest and the embedded config disagree about plain http: OBOP_ALLOW_CLEARTEXT differed between the prebuild and the Gradle build",
      );
    }
  }

  // Permissions: reported, and anything unexpected is named.
  const expected = expectedPermissions(manifest.packageName);
  const extraPermissions = permissions.filter((name) => !expected.includes(name));
  const missing = expected.filter((name) => !permissions.includes(name));
  if (extraPermissions.length > 0) {
    warn(`permissions the app is not expected to have: ${extraPermissions.join(", ")}`);
  }
  if (missing.length > 0) warn(`expected permissions that are missing: ${missing.join(", ")}`);
  if (extraPermissions.length === 0 && missing.length === 0) {
    ok("the permissions are the network and the app's own receiver permission, nothing else");
  }

  return {
    ok: !findings.some((finding) => finding.level === "fail"),
    findings,
    facts: {
      package: manifest.packageName || "(not in the manifest)",
      version: `${manifest.versionName || "?"} (${manifest.versionCode || "?"})`,
      permissions: permissions.length > 0 ? permissions.join(", ") : "(none)",
    },
  };
}
