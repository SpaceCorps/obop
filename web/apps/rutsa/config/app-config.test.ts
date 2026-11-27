import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { describe, expect, test } from "vite-plus/test";
import { identity } from "./identity.ts";
import { NATIVE_TEXTS } from "./native-texts.ts";

// These tests run the real Expo CLI on the real `app.config.ts` (`expo config --type introspect`,
// a second or more each), so what they check is what `expo prebuild` is given: the plugins and the
// mods of the app have run. Only the files Expo can introspect are seen: Info.plist and the app's
// own AndroidManifest.xml. The manifests of the libraries are merged later by Gradle, and the
// minimum SDK is a change of build.gradle, which introspection does not run; both are seen only in
// a built app.
//
// Where a generated `android/` or `ios/` folder exists (it is not checked in), Expo starts from its
// files instead of the template; CI has none and checks from the template.
//
// After `mileage-app` (`config/app-config.test.ts`).
const appDir = fileURLToPath(new URL("..", import.meta.url));
const expoCli = createRequire(import.meta.url).resolve("expo/bin/cli");

type Plist = Record<string, unknown>;
interface UsesPermission {
  $: { "android:name": string; "tools:node"?: string };
}
interface Manifest {
  manifest: {
    "uses-permission"?: UsesPermission[];
    application?: { $: Record<string, string> }[];
  };
}
interface Introspected {
  name: string;
  slug: string;
  version: string;
  android: { package: string };
  ios: { bundleIdentifier: string; infoPlist: Plist };
  _internal: { modResults: { android: { manifest: Manifest } } };
}

function run(env: Record<string, string> = {}) {
  const base = { ...process.env };
  for (const name of ["OBOP_IOS_BUNDLE_ID", "OBOP_DEV_CLIENT"]) delete base[name];
  return spawnSync(
    process.execPath,
    [expoCli, "config", "--type", "introspect", "--json", appDir],
    { cwd: appDir, env: { ...base, ...env }, encoding: "utf8" },
  );
}

function introspect(env: Record<string, string> = {}): Introspected {
  const result = run(env);
  expect(result.status, result.stderr).toBe(0);
  return JSON.parse(result.stdout) as Introspected;
}

/** The permissions the app's manifest asks for, and the ones it removes from the merged manifest. */
function permissions(c: Introspected): { asked: string[]; removed: string[] } {
  const entries = c._internal.modResults.android.manifest.manifest["uses-permission"] ?? [];
  const removed = entries
    .filter((e) => e.$["tools:node"] === "remove")
    .map((e) => e.$["android:name"]);
  const asked = entries
    .filter((e) => e.$["tools:node"] !== "remove")
    .map((e) => e.$["android:name"]);
  return { asked, removed };
}

const P = (name: string) => `android.permission.${name}`;

// Each test starts the Expo CLI (a second alone, several when the machine is busy): the default of
// 5 s is too short.
describe("the configuration that expo prebuild gets", { timeout: 30_000 }, () => {
  test("the name, the slug, the ids and the version are the ones of the identity of the app", () => {
    const c = introspect();
    // The name under the icon.
    expect(c.name).toBe("Rutsa");
    expect(c.name).toBe(NATIVE_TEXTS.en.appName);
    // The development client builds its scheme (`exp+<slug>`) from the slug.
    expect(c.slug).toBe("rutsa");
    expect(c.android.package).toBe(identity.applicationId);
    expect(c.ios.bundleIdentifier).toBe(identity.applicationId);
    expect(c.version).toBe(identity.version);
  });

  test("a test build for iOS has its own bundle id and keeps the Android package", () => {
    const c = introspect({ OBOP_IOS_BUNDLE_ID: "org.obop.rutsa.test" });
    expect(c.ios.bundleIdentifier).toBe("org.obop.rutsa.test");
    expect(c.android.package).toBe(identity.applicationId);
  });

  test("a bundle id of the wrong form stops the build before anything is generated", () => {
    const result = run({ OBOP_IOS_BUNDLE_ID: "not a bundle id" });
    expect(result.status).not.toBe(0);
    expect(`${result.stdout}${result.stderr}`).toContain("OBOP_IOS_BUNDLE_ID");
  });

  test("the Info.plist asks for When In Use access and the location mode, and nothing else", () => {
    const plist = introspect().ios.infoPlist;
    // The one background mode is location: no fetch, which expo-task-manager adds.
    expect(plist.UIBackgroundModes).toEqual(["location"]);
    expect(plist.NSLocationWhenInUseUsageDescription).toBe(NATIVE_TEXTS.en.locationWhenInUse);
    // Keys of things the app does not do are not in the file.
    for (const key of [
      "NSLocationAlwaysAndWhenInUseUsageDescription",
      "NSLocationAlwaysUsageDescription",
      "NSMotionUsageDescription",
      "NSLocationTemporaryUsageDescriptionDictionary",
      "NSLocationDefaultAccuracyReduced",
      "NSFaceIDUsageDescription",
    ]) {
      expect(plist, key).not.toHaveProperty(key);
    }
  });

  test("the entries of the development client are not in the Info.plist unless OBOP_DEV_CLIENT=1", () => {
    const plain = introspect().ios.infoPlist;
    expect(plain).not.toHaveProperty("NSBonjourServices");
    expect(JSON.stringify(plain.CFBundleURLTypes ?? [])).not.toContain("exp+");

    const dev = introspect({ OBOP_DEV_CLIENT: "1" }).ios.infoPlist;
    expect(dev.NSBonjourServices).toEqual(["_expo._tcp"]);
    // As a whole scheme, not as the start of a longer one.
    const schemes = ((dev.CFBundleURLTypes ?? []) as { CFBundleURLSchemes: string[] }[]).flatMap(
      (type) => type.CFBundleURLSchemes,
    );
    expect(schemes).toContain("exp+rutsa");
  });

  test("the Android manifest asks for the location of a trip started with a tap, and for no background location", () => {
    const c = introspect();
    const { asked } = permissions(c);
    for (const name of [
      "ACCESS_FINE_LOCATION",
      "ACCESS_COARSE_LOCATION",
      // The foreground service of type location that keeps a trip recording with the screen off.
      "FOREGROUND_SERVICE",
      "FOREGROUND_SERVICE_LOCATION",
      // Its notification on Android 13 and later.
      "POST_NOTIFICATIONS",
      // expo-task-manager cannot schedule its job without it (app.config.ts).
      "RECEIVE_BOOT_COMPLETED",
    ]) {
      expect(asked, name).toContain(P(name));
    }
    // Background location would be "Allow all the time"; a trip is started with a tap.
    expect(asked).not.toContain(P("ACCESS_BACKGROUND_LOCATION"));
    expect(asked).not.toContain(P("ACTIVITY_RECOGNITION"));
  });

  test("the permissions that libraries add and the app never uses are removed from the merged manifest", () => {
    const { asked, removed } = permissions(introspect());
    for (const name of [
      "SYSTEM_ALERT_WINDOW",
      "READ_EXTERNAL_STORAGE",
      "WRITE_EXTERNAL_STORAGE",
      "VIBRATE",
      "USE_BIOMETRIC",
      "USE_FINGERPRINT",
    ]) {
      expect(removed, name).toContain(P(name));
      expect(asked, name).not.toContain(P(name));
    }
  });

  test("Android's backup of the app is off: the database would leave without its key", () => {
    const application = introspect()._internal.modResults.android.manifest.manifest.application;
    expect(application?.[0]?.$["android:allowBackup"]).toBe("false");
  });
});
