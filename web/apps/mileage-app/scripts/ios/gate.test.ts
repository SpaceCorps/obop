import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { afterAll, describe, expect, test } from "vite-plus/test";
import { checkBuild } from "./gate-lib.mjs";

const plain = () => ({
  config: { extra: { cleartextAllowed: false, defaultServer: "" } } as Record<string, unknown>,
  infoPlist: {
    CFBundleIdentifier: "org.obop.mileage",
    UIBackgroundModes: ["location"],
    NSLocationWhenInUseUsageDescription: "obop Mileage records the route of a trip that you start.",
    NSLocalNetworkUsageDescription:
      "obop Mileage connects to the obop server whose address you enter.",
    // What the Expo template ships in every build.
    NSAppTransportSecurity: { NSAllowsArbitraryLoads: false, NSAllowsLocalNetworking: true },
  } as Record<string, unknown>,
});

const failures = (result: ReturnType<typeof checkBuild>) =>
  result.findings.filter((f) => f.level === "fail").map((f) => f.text);

describe("a release build", () => {
  test("the plain build passes", () => {
    const result = checkBuild(plain(), { mode: "release" });
    expect(failures(result)).toEqual([]);
    expect(result.ok).toBe(true);
    expect(result.facts).toMatchObject({
      bundleId: "org.obop.mileage",
      cleartextAllowed: "false",
      defaultServer: "(none)",
      backgroundModes: '["location"]',
    });
  });

  test("a build that allows plain http is not a release build", () => {
    const built = plain();
    built.config.extra = { cleartextAllowed: true, defaultServer: "http://192.168.1.132:18081" };
    const result = checkBuild(built, { mode: "release" });
    expect(result.ok).toBe(false);
    expect(failures(result).join("\n")).toMatch(/cleartextAllowed=true/);
    expect(failures(result).join("\n")).toMatch(/https address/);
  });

  test("a build whose config does not say is refused too", () => {
    const built = plain();
    built.config = {};
    expect(failures(checkBuild(built, { mode: "release" })).join("\n")).toMatch(
      /no extra\.cleartextAllowed/,
    );
    expect(checkBuild({ config: null, infoPlist: null }, { mode: "release" }).ok).toBe(false);
  });

  test("an https default server is fine", () => {
    const built = plain();
    built.config.extra = { cleartextAllowed: false, defaultServer: "https://obop.example.org" };
    expect(checkBuild(built, { mode: "release" }).ok).toBe(true);
  });

  test("the Bonjour service of the development client fails a release build", () => {
    const built = plain();
    built.infoPlist.NSBonjourServices = ["_expo._tcp"];
    const result = checkBuild(built, { mode: "release" });
    expect(result.ok).toBe(false);
    expect(failures(result)).toEqual([
      expect.stringContaining("NSBonjourServices holds _expo._tcp"),
    ]);
  });

  test("the URL scheme of the development client fails a release build", () => {
    const built = plain();
    built.infoPlist.CFBundleURLTypes = [
      { CFBundleURLSchemes: ["org.obop.mileage"] },
      { CFBundleURLSchemes: ["exp+obop-mileage"] },
    ];
    const result = checkBuild(built, { mode: "release" });
    expect(result.ok).toBe(false);
    expect(failures(result)).toEqual([expect.stringContaining("exp+obop-mileage")]);
  });

  test("the app's own scheme and another Bonjour service are not the development client", () => {
    const built = plain();
    built.infoPlist.CFBundleURLTypes = [{ CFBundleURLSchemes: ["org.obop.mileage"] }];
    built.infoPlist.NSBonjourServices = ["_other._tcp"];
    const result = checkBuild(built, { mode: "release" });
    expect(result.ok).toBe(true);
    expect(result.findings.some((f) => f.level === "warn" && /_other/.test(f.text))).toBe(true);
  });

  test("a build that stores every fix is not a release build", () => {
    const built = plain();
    built.config.extra = { cleartextAllowed: false, defaultServer: "", thinningOff: true };
    const result = checkBuild(built, { mode: "release" });
    expect(result.ok).toBe(false);
    expect(failures(result)).toEqual([expect.stringContaining("thinningOff=true")]);
    // Also when the build allows plain http: both are reported.
    built.config.extra = { cleartextAllowed: true, defaultServer: "", thinningOff: true };
    expect(failures(checkBuild(built, { mode: "release" }))).toHaveLength(2);
  });

  test("the plain build says that the thinning is on", () => {
    expect(checkBuild(plain(), { mode: "release" }).facts.thinningOff).toBe("false");
  });
});

describe("a test build", () => {
  test("may allow plain http and says so", () => {
    const built = plain();
    built.config.extra = { cleartextAllowed: true, defaultServer: "http://192.168.1.132:18081" };
    const result = checkBuild(built, { mode: "test" });
    expect(result.ok).toBe(true);
    expect(
      result.findings.some((f) => f.level === "ok" && /plain http is allowed/.test(f.text)),
    ).toBe(true);
  });

  test("may store every fix and says so", () => {
    const built = plain();
    built.config.extra = {
      cleartextAllowed: true,
      defaultServer: "http://192.168.1.132:18081",
      thinningOff: true,
    };
    const result = checkBuild(built, { mode: "test" });
    expect(result.ok).toBe(true);
    expect(result.facts.thinningOff).toBe("true");
    expect(result.findings.some((f) => f.level === "ok" && /without thinning/.test(f.text))).toBe(
      true,
    );
  });

  test("a build that would ignore OBOP_RECORD_THINNING because plain http is off fails", () => {
    // thin.ts honours thinningOff only when cleartextAllowed is true as well.
    for (const cleartextAllowed of [false, undefined]) {
      const built = plain();
      built.config.extra = { cleartextAllowed, defaultServer: "", thinningOff: true };
      const result = checkBuild(built, { mode: "test" });
      expect(result.ok, String(cleartextAllowed)).toBe(false);
      expect(failures(result).join("\n")).toMatch(
        /ignores OBOP_RECORD_THINNING because plain http is not allowed \(set OBOP_ALLOW_CLEARTEXT=1 too\)/,
      );
      expect(result.findings.some((f) => f.level === "ok" && /without thinning/.test(f.text))).toBe(
        false,
      );
    }
  });

  test("the release failure for thinningOff says what the app does with it", () => {
    const built = plain();
    built.config.extra = { cleartextAllowed: false, defaultServer: "", thinningOff: true };
    expect(failures(checkBuild(built, { mode: "release" })).join("\n")).toMatch(
      /OBOP_RECORD_THINNING=off was set.*ignores the flag if not/,
    );
  });

  test("the test bundle id with plain http off warns that the variables may not have been exported", () => {
    const built = plain();
    built.infoPlist.CFBundleIdentifier = "org.obop.mileage.test";
    built.config.extra = { cleartextAllowed: false, defaultServer: "" };
    const result = checkBuild(built, { mode: "test" });
    expect(result.ok).toBe(true);
    expect(
      result.findings.filter(
        (f) => f.level === "warn" && /without the OBOP_ variables/.test(f.text),
      ),
    ).toHaveLength(1);
    // Neither the plain bundle id nor a test build that does allow http warns.
    expect(
      checkBuild(plain(), { mode: "test" }).findings.filter((f) => f.level === "warn"),
    ).toEqual([]);
    built.config.extra = { cleartextAllowed: true, defaultServer: "" };
    expect(checkBuild(built, { mode: "test" }).findings.filter((f) => f.level === "warn")).toEqual(
      [],
    );
    // A release build says nothing of the kind: it is the expected state of one.
    built.config.extra = { cleartextAllowed: false, defaultServer: "" };
    expect(
      checkBuild(built, { mode: "release" }).findings.filter((f) => /OBOP_ variables/.test(f.text)),
    ).toEqual([]);
  });

  test("an entry of the development client is a warning, not a failure", () => {
    const built = plain();
    built.config.extra = { cleartextAllowed: true, defaultServer: "" };
    built.infoPlist.NSBonjourServices = ["_expo._tcp"];
    built.infoPlist.CFBundleURLTypes = [{ CFBundleURLSchemes: ["exp+obop-mileage"] }];
    const result = checkBuild(built, { mode: "test" });
    expect(result.ok).toBe(true);
    expect(result.findings.filter((f) => f.level === "warn")).toHaveLength(2);
  });
});

describe("the Info.plist, in either mode", () => {
  for (const mode of ["release", "test"] as const) {
    test(`${mode}: an exception of App Transport Security fails`, () => {
      const arbitrary = plain();
      arbitrary.infoPlist.NSAppTransportSecurity = { NSAllowsArbitraryLoads: true };
      expect(failures(checkBuild(arbitrary, { mode })).join("\n")).toMatch(
        /NSAllowsArbitraryLoads is true/,
      );

      const media = plain();
      media.infoPlist.NSAppTransportSecurity = { NSAllowsArbitraryLoadsForMedia: true };
      expect(checkBuild(media, { mode }).ok).toBe(false);

      const domain = plain();
      domain.infoPlist.NSAppTransportSecurity = {
        NSExceptionDomains: { "192.168.1.132": { NSExceptionAllowsInsecureHTTPLoads: true } },
      };
      expect(failures(checkBuild(domain, { mode })).join("\n")).toMatch(
        /exception domains \(192\.168\.1\.132\)/,
      );
    });

    test(`${mode}: location is the only background mode`, () => {
      for (const modes of [["location", "fetch"], [], ["audio"], undefined]) {
        const built = plain();
        built.infoPlist.UIBackgroundModes = modes;
        expect(failures(checkBuild(built, { mode })).join("\n"), JSON.stringify(modes)).toMatch(
          /UIBackgroundModes must be exactly \[location\]/,
        );
      }
    });

    test(`${mode}: the app asks for When In Use access only`, () => {
      const missing = plain();
      delete missing.infoPlist.NSLocationWhenInUseUsageDescription;
      expect(failures(checkBuild(missing, { mode })).join("\n")).toMatch(/NSLocationWhenInUse/);

      for (const key of [
        "NSLocationAlwaysAndWhenInUseUsageDescription",
        "NSLocationAlwaysUsageDescription",
      ]) {
        const built = plain();
        built.infoPlist[key] = "x";
        expect(failures(checkBuild(built, { mode })).join("\n"), key).toMatch(
          /When In Use access only/,
        );
      }
    });

    test(`${mode}: the local network text is the app's own`, () => {
      const missing = plain();
      delete missing.infoPlist.NSLocalNetworkUsageDescription;
      expect(checkBuild(missing, { mode }).ok).toBe(false);

      const dev = plain();
      dev.infoPlist.NSLocalNetworkUsageDescription =
        "Expo Dev Launcher uses the local network to discover and connect to development servers running on your computer.";
      expect(failures(checkBuild(dev, { mode })).join("\n")).toMatch(/development client/);
    });
  }
});

describe("the command", () => {
  const dir = mkdtempSync(join(tmpdir(), "obop-gate-"));
  afterAll(() => rmSync(dir, { recursive: true, force: true }));
  const script = fileURLToPath(new URL("./gate.mjs", import.meta.url));

  function run(built: ReturnType<typeof plain>, ...extra: string[]) {
    const plist = join(dir, "Info.json");
    const config = join(dir, "app.config");
    writeFileSync(plist, JSON.stringify(built.infoPlist));
    writeFileSync(config, JSON.stringify(built.config));
    return spawnSync(
      process.execPath,
      [script, "--info-plist-json", plist, "--app-config", config, ...extra],
      { encoding: "utf8" },
    );
  }

  test("exits 0 for a plain release build and prints what it found", () => {
    const result = run(plain());
    expect(result.status).toBe(0);
    expect(result.stdout).toContain("gate passed (release build)");
    expect(result.stdout).toMatch(/cleartextAllowed\s+false/);
  });

  test("exits 1 for a release build that allows plain http, and 0 as a test build", () => {
    const built = plain();
    built.config.extra = { cleartextAllowed: true, defaultServer: "http://10.0.0.5:18081" };
    const release = run(built);
    expect(release.status).toBe(1);
    expect(release.stdout).toContain("FAIL");
    expect(release.stdout).toContain("gate FAILED (release build)");
    const test = run(built, "--test-build");
    expect(test.status).toBe(0);
    expect(test.stdout).toContain("gate passed (test build)");
  });

  test("exits 2 when it is not told what to read", () => {
    const result = spawnSync(process.execPath, [script], { encoding: "utf8" });
    expect(result.status).toBe(2);
    expect(result.stderr).toContain("usage:");
  });
});
