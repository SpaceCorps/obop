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

  test("the test bundle id with plain http off warns that OBOP_ALLOW_CLEARTEXT was not set", () => {
    const built = plain();
    built.infoPlist.CFBundleIdentifier = "org.obop.mileage.test";
    built.config.extra = { cleartextAllowed: false, defaultServer: "" };
    const result = checkBuild(built, { mode: "test" });
    expect(result.ok).toBe(true);
    expect(
      result.findings.filter(
        (f) => f.level === "warn" && /OBOP_ALLOW_CLEARTEXT was not set for this build/.test(f.text),
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
      checkBuild(built, { mode: "release" }).findings.filter((f) =>
        /OBOP_ALLOW_CLEARTEXT/.test(f.text),
      ),
    ).toEqual([]);
  });

  test("the test bundle id with no cleartextAllowed in the config fails and does not warn", () => {
    // The warning is for `cleartextAllowed=false`; a config that says nothing is the failure of
    // "not made from this app.config.ts" and gets no second line about the bundle id.
    const built = plain();
    built.infoPlist.CFBundleIdentifier = "org.obop.mileage.test";
    built.config.extra = { defaultServer: "" };
    const result = checkBuild(built, { mode: "test" });
    expect(result.ok).toBe(false);
    expect(failures(result).join("\n")).toMatch(/no extra\.cleartextAllowed/);
    expect(result.findings.filter((f) => f.level === "warn")).toEqual([]);
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

describe("a build for the first drive on a phone (--drive)", () => {
  const driveBuild = () => {
    const built = plain();
    built.infoPlist.CFBundleIdentifier = "org.obop.mileage.test";
    built.config.extra = {
      cleartextAllowed: true,
      defaultServer: "http://192.168.1.132:18081",
      thinningOff: true,
    };
    return built;
  };
  const driveFailures = (built: ReturnType<typeof plain>) =>
    failures(checkBuild(built, { mode: "test", drive: true }));
  const driveLines = (list: string[]) => list.filter((text) => text.startsWith("drive build:"));

  test("passes when plain http, the unthinned fixes and an http default server are all set", () => {
    const result = checkBuild(driveBuild(), { mode: "test", drive: true });
    expect(failures(result)).toEqual([]);
    expect(result.ok).toBe(true);
    expect(result.findings.some((f) => f.level === "ok" && f.text.startsWith("drive build:"))).toBe(
      true,
    );
  });

  test("fails on the build that was rebuilt without the variables, and says what fixes each", () => {
    // The case the trap is about: a rebuild in a Terminal without the variables keeps the test
    // bundle id and writes cleartext false, no default server and thinning on.
    const rebuilt = driveBuild();
    rebuilt.config.extra = { cleartextAllowed: false, defaultServer: "" };
    const text = driveFailures(rebuilt).join("\n");
    expect(text).toMatch(/cleartextAllowed is false.*OBOP_ALLOW_CLEARTEXT=1/);
    expect(text).toMatch(/thinningOff is undefined.*OBOP_RECORD_THINNING=off/);
    expect(text).toMatch(/defaultServer is empty.*OBOP_DEFAULT_SERVER=http:\/\/ADDRESS:PORT/);
    expect(text).toMatch(/env \| grep OBOP_.*vp run ios:run:device.*this gate again/);
    expect(checkBuild(rebuilt, { mode: "test", drive: true }).ok).toBe(false);
  });

  test("fails on each of the settings missing alone, with its own line and the remedy", () => {
    const base = driveBuild().config.extra as Record<string, unknown>;
    const cases: Array<[string, Record<string, unknown>, RegExp]> = [
      ["cleartext false", { ...base, cleartextAllowed: false }, /cleartextAllowed is false/],
      [
        "cleartext undefined",
        { ...base, cleartextAllowed: undefined },
        /cleartextAllowed is undefined/,
      ],
      [
        "thinning false",
        { ...base, thinningOff: false },
        /thinningOff is false.*OBOP_RECORD_THINNING=off/,
      ],
      ["thinning missing", { ...base, thinningOff: undefined }, /thinningOff is undefined/],
      ["server empty", { ...base, defaultServer: "" }, /defaultServer is empty/],
      ["server missing", { ...base, defaultServer: undefined }, /defaultServer is empty/],
      [
        "server https",
        { ...base, defaultServer: "https://obop.example.org" },
        /defaultServer is "https:\/\/obop\.example\.org".*an http:\/\/ address/,
      ],
      [
        "server without a scheme",
        { ...base, defaultServer: "192.168.1.132:18081" },
        /defaultServer is "192\.168\.1\.132:18081"/,
      ],
    ];
    for (const [name, extra, pattern] of cases) {
      const built = driveBuild();
      built.config.extra = extra;
      const result = checkBuild(built, { mode: "test", drive: true });
      expect(result.ok, name).toBe(false);
      // The one fact plus the remedy line, and no other fact.
      const lines = driveLines(failures(result));
      expect(lines, name).toHaveLength(2);
      expect(lines[0], name).toMatch(pattern);
      expect(lines[1], name).toMatch(/env \| grep OBOP_/);
      // Without the flag the same build is not refused for it.
      expect(driveLines(failures(checkBuild(built, { mode: "test" }))), name).toEqual([]);
    }
  });

  test("an uppercase scheme is still an http address", () => {
    const built = driveBuild();
    built.config.extra = {
      ...(built.config.extra as object),
      defaultServer: "HTTP://192.168.1.132:18081",
    };
    expect(driveFailures(built)).toEqual([]);
  });

  test("without the flag nothing changes: the rebuilt build passes with its warning", () => {
    const rebuilt = driveBuild();
    rebuilt.config.extra = { cleartextAllowed: false, defaultServer: "" };
    const result = checkBuild(rebuilt, { mode: "test" });
    expect(result.ok).toBe(true);
    expect(result.findings.filter((f) => f.level === "warn")).toHaveLength(1);
    expect(result.findings.some((f) => f.text.startsWith("drive build:"))).toBe(false);
  });

  test("a release build cannot be checked as a drive build", () => {
    expect(() => checkBuild(driveBuild(), { mode: "release", drive: true })).toThrow(/test build/);
  });

  test("the other findings still apply", () => {
    const built = driveBuild();
    built.infoPlist.UIBackgroundModes = ["location", "fetch"];
    const result = checkBuild(built, { mode: "test", drive: true });
    expect(result.ok).toBe(false);
    expect(failures(result)).toEqual([expect.stringContaining("UIBackgroundModes")]);
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

  describe("--drive", () => {
    const driveBuild = () => {
      const built = plain();
      built.infoPlist.CFBundleIdentifier = "org.obop.mileage.test";
      built.config.extra = {
        cleartextAllowed: true,
        defaultServer: "http://192.168.1.132:18081",
        thinningOff: true,
      };
      return built;
    };

    test("exits 0 and says gate passed (test build) when the three settings are set", () => {
      const result = run(driveBuild(), "--test-build", "--drive");
      expect(result.status).toBe(0);
      expect(result.stdout).toContain("gate passed (test build)");
      expect(result.stdout).toContain("drive build:");
    });

    test("exits 1 for the build rebuilt without the variables, which passes without the flag", () => {
      const rebuilt = driveBuild();
      rebuilt.config.extra = { cleartextAllowed: false, defaultServer: "" };
      const without = run(rebuilt, "--test-build");
      expect(without.status).toBe(0);
      expect(without.stdout).toContain("gate passed (test build)");
      const withFlag = run(rebuilt, "--test-build", "--drive");
      expect(withFlag.status).toBe(1);
      expect(withFlag.stdout).toContain("gate FAILED (test build)");
      expect(withFlag.stdout).toContain("OBOP_ALLOW_CLEARTEXT=1");
      expect(withFlag.stdout).toContain("OBOP_RECORD_THINNING=off");
      expect(withFlag.stdout).toContain("OBOP_DEFAULT_SERVER=http://ADDRESS:PORT");
      expect(withFlag.stdout).toContain("env | grep OBOP_");
    });

    const alone: Array<[string, Record<string, unknown>]> = [
      [
        "cleartext",
        { cleartextAllowed: false, defaultServer: "http://10.0.0.5:18081", thinningOff: false },
      ],
      ["thinning", { cleartextAllowed: true, defaultServer: "http://10.0.0.5:18081" }],
      ["the server", { cleartextAllowed: true, defaultServer: "", thinningOff: true }],
    ];
    for (const [name, extra] of alone) {
      test(`exits 1 when only ${name} is missing`, () => {
        const built = driveBuild();
        built.config.extra = extra;
        expect(run(built, "--test-build", "--drive").status).toBe(1);
      });
    }

    test("exits 2 without --test-build", () => {
      const result = run(driveBuild(), "--drive");
      expect(result.status).toBe(2);
      expect(result.stderr).toContain("--drive is for a test build");
    });
  });

  test("exits 2 when it is not told what to read", () => {
    const result = spawnSync(process.execPath, [script], { encoding: "utf8" });
    expect(result.status).toBe(2);
    expect(result.stderr).toContain("usage:");
  });
});
