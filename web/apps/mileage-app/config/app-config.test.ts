import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { describe, expect, test } from "vite-plus/test";

// These tests run the real Expo CLI on the real `app.config.ts` (`expo config --type introspect`,
// about a second each), so what they check is what `expo prebuild` is given: the plugins and mods
// of the app have run. The ATS part of an introspected Info.plist is a built-in fallback of Expo
// and says nothing about a build (the generated project has its own, read on the Mac), so it is not
// asserted here.
const appDir = fileURLToPath(new URL("..", import.meta.url));
const expoCli = createRequire(import.meta.url).resolve("expo/bin/cli");

type Plist = Record<string, unknown>;
interface Introspected {
  scheme: string;
  android: { package: string };
  ios: { bundleIdentifier: string; infoPlist: Plist };
  extra: { cleartextAllowed: boolean; defaultServer: string; thinningOff: boolean };
}

function run(env: Record<string, string> = {}) {
  const base = { ...process.env };
  for (const name of [
    "OBOP_ALLOW_CLEARTEXT",
    "OBOP_DEFAULT_SERVER",
    "OBOP_IOS_BUNDLE_ID",
    "OBOP_RECORD_THINNING",
    "OBOP_DEV_CLIENT",
  ]) {
    delete base[name];
  }
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

// Each test starts the Expo CLI (about a second alone, several when the machine is busy): the
// default of 5 s was exceeded under load.
describe("the configuration that expo prebuild gets", { timeout: 30_000 }, () => {
  test("a build without the test variables is the plain one", () => {
    const c = introspect();
    expect(c.scheme).toBe("org.obop.mileage");
    expect(c.ios.bundleIdentifier).toBe("org.obop.mileage");
    expect(c.android.package).toBe("org.obop.mileage");
    expect(c.extra).toEqual({ cleartextAllowed: false, defaultServer: "", thinningOff: false });
  });

  test("a test build has its own bundle id and keeps the sign-in scheme and the Android package", () => {
    const c = introspect({
      OBOP_IOS_BUNDLE_ID: "org.obop.mileage.test",
      OBOP_ALLOW_CLEARTEXT: "1",
      OBOP_DEFAULT_SERVER: "http://192.168.1.132:18081",
    });
    expect(c.ios.bundleIdentifier).toBe("org.obop.mileage.test");
    expect(c.scheme).toBe("org.obop.mileage");
    expect(c.android.package).toBe("org.obop.mileage");
    expect(c.extra).toEqual({
      cleartextAllowed: true,
      defaultServer: "http://192.168.1.132:18081",
      thinningOff: false,
    });
  });

  test("OBOP_RECORD_THINNING=off reaches the app, and a typo stops the build", () => {
    expect(introspect({ OBOP_RECORD_THINNING: "off" }).extra.thinningOff).toBe(true);
    const result = run({ OBOP_RECORD_THINNING: "of" });
    expect(result.status).not.toBe(0);
    expect(`${result.stdout}${result.stderr}`).toContain("OBOP_RECORD_THINNING");
  });

  test("the entries of the development client are not in the Info.plist unless OBOP_DEV_CLIENT=1", () => {
    const plain = introspect().ios.infoPlist;
    expect(plain).not.toHaveProperty("NSBonjourServices");
    expect(JSON.stringify(plain.CFBundleURLTypes)).not.toContain("exp+");
    // The sign-in scheme stays.
    expect(JSON.stringify(plain.CFBundleURLTypes)).toContain("org.obop.mileage");

    const dev = introspect({ OBOP_DEV_CLIENT: "1" }).ios.infoPlist;
    expect(dev.NSBonjourServices).toEqual(["_expo._tcp"]);
    expect(JSON.stringify(dev.CFBundleURLTypes)).toContain("exp+obop-mileage");
  });

  test("a bundle id of the wrong form stops the build before anything is generated", () => {
    const result = run({ OBOP_IOS_BUNDLE_ID: "not a bundle id" });
    expect(result.status).not.toBe(0);
    expect(`${result.stdout}${result.stderr}`).toContain("OBOP_IOS_BUNDLE_ID");
  });

  test("the Info.plist asks for When In Use access, the location mode and the local network, and nothing else", () => {
    const plist = introspect().ios.infoPlist;
    // The one background mode is location: no fetch, which expo-task-manager adds.
    expect(plist.UIBackgroundModes).toEqual(["location"]);
    expect(plist.NSLocationWhenInUseUsageDescription).toMatch(
      /keeps recording while the screen is off/,
    );
    // The development client sets its own text where the app has none, so the text is checked in full.
    expect(plist.NSLocalNetworkUsageDescription).toMatch(
      /^obop Mileage connects to the obop server /,
    );
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
});
