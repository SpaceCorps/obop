import { describe, expect, test } from "vite-plus/test";
import {
  checkBuild,
  expectedPermissions,
  manifestFacts,
  permissionList,
  type Finding,
} from "./gate-lib.mjs";

/** The manifest as `apkanalyzer manifest print` writes it, cut to what the gate reads. */
const manifest = (application: string): string => `<?xml version="1.0" encoding="utf-8"?>
<manifest
    xmlns:android="http://schemas.android.com/apk/res/android"
    android:versionCode="1"
    android:versionName="0.1.0"
    package="org.obop.app.test">

    <uses-permission
        android:name="android.permission.INTERNET" />

    <application
        android:label="@ref/0x7f10001d"
        android:name="org.obop.app.test.MainApplication"
${application}
        android:supportsRtl="true">

        <activity
            android:name="org.obop.app.test.MainActivity"
            android:exported="true" />
    </application>
</manifest>
`;

const PLAIN_MANIFEST = manifest('        android:allowBackup="false"');
const TEST_MANIFEST = manifest(
  '        android:allowBackup="false"\n        android:usesCleartextTraffic="true"',
);
const PERMISSIONS =
  "org.obop.app.test.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION\r\nandroid.permission.INTERNET\r\n";

const config = (extra: unknown) => ({ name: "obop", slug: "obop-host", extra });
const PLAIN_EXTRA = { cleartextAllowed: false, webviewDebug: false, defaultServer: "" };
const TEST_EXTRA = {
  cleartextAllowed: true,
  webviewDebug: true,
  defaultServer: "http://localhost:18180",
};

const release = (changes: { manifest?: string; permissions?: string; extra?: unknown } = {}) =>
  checkBuild(
    {
      manifest: changes.manifest ?? PLAIN_MANIFEST,
      permissions: changes.permissions ?? PERMISSIONS,
      config: config("extra" in changes ? changes.extra : PLAIN_EXTRA),
    },
    { mode: "release" },
  );

const failures = (result: { findings: Finding[] }): string[] =>
  result.findings.filter((finding) => finding.level === "fail").map((finding) => finding.text);
const warnings = (result: { findings: Finding[] }): string[] =>
  result.findings.filter((finding) => finding.level === "warn").map((finding) => finding.text);

describe("reading the manifest", () => {
  test("the application's switches, the package and the version", () => {
    expect(manifestFacts(TEST_MANIFEST)).toEqual({
      packageName: "org.obop.app.test",
      versionName: "0.1.0",
      versionCode: "1",
      usesCleartextTraffic: "true",
      networkSecurityConfig: undefined,
      debuggable: undefined,
      allowBackup: "false",
      hasApplication: true,
    });
  });

  test("an attribute of another element is not the application's", () => {
    const xml = manifest('        android:allowBackup="false"').replace(
      'android:exported="true"',
      'android:exported="true" android:debuggable="true" android:usesCleartextTraffic="true"',
    );
    expect(manifestFacts(xml)).toMatchObject({
      debuggable: undefined,
      usesCleartextTraffic: undefined,
    });
  });

  test("a name that only ends like the attribute is not it", () => {
    const xml = manifest('        tools:android:allowBackup="true"\n        xallowBackup="true"');
    expect(manifestFacts(xml).allowBackup).toBeUndefined();
  });

  test("a text that is no manifest has no application", () => {
    expect(manifestFacts("ERROR: not an APK")).toMatchObject({
      hasApplication: false,
      packageName: "",
    });
  });

  test("the permissions, one name per line, whatever the line ends are", () => {
    expect(permissionList(PERMISSIONS)).toEqual(expectedPermissions("org.obop.app.test"));
    expect(permissionList("\n  android.permission.CAMERA  \nWARNING: something else\n")).toEqual([
      "android.permission.CAMERA",
    ]);
  });
});

describe("a build meant for release", () => {
  test("the plain build passes", () => {
    const result = release();
    expect(failures(result)).toEqual([]);
    expect(warnings(result)).toEqual([]);
    expect(result.ok).toBe(true);
    expect(result.facts).toEqual({
      package: "org.obop.app.test",
      version: "0.1.0 (1)",
      permissions:
        "android.permission.INTERNET, org.obop.app.test.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION",
    });
  });

  test("the test build fails, on each of its four facts", () => {
    const result = release({ manifest: TEST_MANIFEST, extra: TEST_EXTRA });
    expect(result.ok).toBe(false);
    expect(failures(result)).toHaveLength(4);
  });

  test("usesCleartextTraffic in the manifest fails", () => {
    const result = release({ manifest: TEST_MANIFEST });
    expect(result.ok).toBe(false);
    expect(failures(result)).toEqual([expect.stringContaining("usesCleartextTraffic=true")]);
    // The two halves of the build disagree: that is said too.
    expect(warnings(result)).toEqual([expect.stringContaining("disagree")]);
  });

  test("a network security config fails, whatever usesCleartextTraffic says", () => {
    const result = release({
      manifest: manifest(
        '        android:allowBackup="false"\n        android:usesCleartextTraffic="false"\n        android:networkSecurityConfig="@ref/0x7f150001"',
      ),
    });
    expect(result.ok).toBe(false);
    expect(failures(result)).toEqual([expect.stringContaining("networkSecurityConfig")]);
    // A test build reports it and passes.
    const test = checkBuild(
      {
        manifest: manifest('        android:networkSecurityConfig="@xml/network_security_config"'),
        permissions: PERMISSIONS,
        config: config(TEST_EXTRA),
      },
      { mode: "test" },
    );
    expect(failures(test)).toEqual([]);
  });

  test("debuggable in the manifest fails", () => {
    const result = release({
      manifest: manifest('        android:allowBackup="false"\n        android:debuggable="true"'),
    });
    expect(result.ok).toBe(false);
    expect(failures(result)).toEqual([expect.stringContaining("debuggable=true")]);
  });

  test("cleartextAllowed in the embedded config fails", () => {
    const result = release({ extra: { ...PLAIN_EXTRA, cleartextAllowed: true } });
    expect(result.ok).toBe(false);
    expect(failures(result)).toEqual([expect.stringContaining("cleartextAllowed=true")]);
  });

  test("webviewDebug in the embedded config fails", () => {
    const result = release({ extra: { ...PLAIN_EXTRA, webviewDebug: true } });
    expect(result.ok).toBe(false);
    expect(failures(result)).toEqual([expect.stringContaining("webviewDebug=true")]);
  });

  test("an http default server fails; an https one and none pass", () => {
    for (const defaultServer of ["http://localhost:18180", "HTTP://192.168.1.20", " http://x"]) {
      const result = release({ extra: { ...PLAIN_EXTRA, defaultServer } });
      expect(result.ok, defaultServer).toBe(false);
      expect(failures(result), defaultServer).toEqual([expect.stringContaining("http address")]);
    }
    expect(
      release({ extra: { ...PLAIN_EXTRA, defaultServer: "https://erp.example.com" } }).ok,
    ).toBe(true);
  });

  test("a config that is not this app's fails: a missing switch is not a switch that is off", () => {
    expect(release({ extra: undefined }).ok).toBe(false);
    expect(release({ extra: "x" }).ok).toBe(false);
    expect(failures(release({ extra: {} }))).toHaveLength(3);
    expect(release({ extra: { ...PLAIN_EXTRA, cleartextAllowed: "false" } }).ok).toBe(false);
    expect(release({ extra: { cleartextAllowed: false, webviewDebug: false } }).ok).toBe(false);
    expect(
      checkBuild(
        { manifest: PLAIN_MANIFEST, permissions: PERMISSIONS, config: null },
        { mode: "release" },
      ).ok,
    ).toBe(false);
  });

  test("an APK that could not be read fails", () => {
    const result = release({ manifest: "" });
    expect(result.ok).toBe(false);
    expect(failures(result)).toEqual([expect.stringContaining("no <application>")]);
  });

  test("a backup switch that is not off and an unexpected permission are named, and do not fail", () => {
    const backup = release({ manifest: manifest('        android:allowBackup="true"') });
    expect(backup.ok).toBe(true);
    expect(warnings(backup)).toEqual([expect.stringContaining("allowBackup=true")]);
    expect(warnings(release({ manifest: manifest("") }))).toEqual([
      expect.stringContaining("allowBackup=unset"),
    ]);

    const camera = release({ permissions: `${PERMISSIONS}android.permission.CAMERA\n` });
    expect(camera.ok).toBe(true);
    expect(warnings(camera)).toEqual([expect.stringContaining("android.permission.CAMERA")]);
    expect(warnings(release({ permissions: "" }))).toEqual([
      expect.stringContaining("android.permission.INTERNET"),
    ]);
  });
});

describe("a test build", () => {
  test("is reported, not failed", () => {
    const result = checkBuild(
      { manifest: TEST_MANIFEST, permissions: PERMISSIONS, config: config(TEST_EXTRA) },
      { mode: "test" },
    );
    expect(result.ok).toBe(true);
    expect(failures(result)).toEqual([]);
    expect(
      result.findings.filter((finding) => finding.text.startsWith("test build: ")),
    ).toHaveLength(4);
  });

  test("still fails on a config that is not this app's", () => {
    expect(
      checkBuild(
        { manifest: TEST_MANIFEST, permissions: PERMISSIONS, config: {} },
        { mode: "test" },
      ).ok,
    ).toBe(false);
  });
});
