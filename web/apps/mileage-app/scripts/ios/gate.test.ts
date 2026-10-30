import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { afterAll, describe, expect, test } from "vite-plus/test";
import { addressOf, checkBuild } from "./gate-lib.mjs";

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

  test("a build with the thinning off is not a release build", () => {
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
    expect(text).toMatch(
      /env \| grep OBOP_.*`vp run ios:run:device` for the phone, `vp run ios:build:simulator Release` for a Simulator build.*this gate again/,
    );
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
      // The one fact plus the remedy line, and no other fact. (Plain http off with the thinning
      // flag on also gives the failure "the build ignores OBOP_RECORD_THINNING", with the prefix.)
      const lines = driveLines(failures(result)).filter((text) => !/build ignores/.test(text));
      expect(lines, name).toHaveLength(2);
      expect(lines[0], name).toMatch(pattern);
      expect(lines[1], name).toMatch(/env \| grep OBOP_/);
      // Without the flag the same build is not refused for it.
      expect(driveLines(failures(checkBuild(built, { mode: "test" }))), name).toEqual([]);
    }
  });

  test("the failure for thinning without plain http has the drive prefix with --drive and none without", () => {
    const built = driveBuild();
    built.config.extra = { ...(built.config.extra as object), cleartextAllowed: false };
    const ignores = (list: string[]) => list.filter((text) => /build ignores/.test(text));
    const withDrive = ignores(driveFailures(built));
    expect(withDrive).toHaveLength(1);
    expect(withDrive[0]).toMatch(/^drive build: the build ignores OBOP_RECORD_THINNING/);
    const without = ignores(failures(checkBuild(built, { mode: "test" })));
    expect(without).toHaveLength(1);
    expect(without[0]).toMatch(/^the build ignores OBOP_RECORD_THINNING/);
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

  describe("--expect-server and --expect-bundle-id", () => {
    const SERVER = "http://192.168.1.57:18080";
    const BUNDLE = "org.obop.mileage.test";
    const expecting = (built: ReturnType<typeof plain>, expectation: object) =>
      checkBuild(built, { mode: "test", drive: true, ...expectation });
    const forPc = () => {
      const built = driveBuild();
      built.config.extra = { ...(built.config.extra as object), defaultServer: SERVER };
      return built;
    };
    const withServer = (defaultServer: string) => {
      const built = forPc();
      built.config.extra = { ...(built.config.extra as object), defaultServer };
      return built;
    };

    test("a build that points at the expected instance with the expected bundle id passes and says so", () => {
      const result = expecting(forPc(), { expectServer: SERVER, expectBundleId: BUNDLE });
      expect(failures(result)).toEqual([]);
      const ok = result.findings.filter((f) => f.level === "ok").map((f) => f.text);
      expect(ok).toContain(`drive build: the default server is ${SERVER}, as expected`);
      expect(ok).toContain(`drive build: the bundle id is ${BUNDLE}, as expected`);
    });

    test("the address is compared as an address: a trailing slash, the case of the host and a default port do not matter", () => {
      for (const defaultServer of [
        `${SERVER}/`,
        "HTTP://192.168.1.57:18080",
        "http://192.168.1.57:18080//",
      ]) {
        expect(
          failures(expecting(withServer(defaultServer), { expectServer: SERVER })),
          defaultServer,
        ).toEqual([]);
      }
      expect(
        failures(expecting(withServer("http://PC.local:80"), { expectServer: "http://pc.local/" })),
      ).toEqual([]);
      expect(addressOf("http://PC.local:80/a/")).toBe("http://pc.local/a");
      expect(addressOf("not an address")).toBeUndefined();
    });

    test("another address fails with its own line, the variable that fixes it and the remedy", () => {
      for (const defaultServer of [
        "http://192.168.1.1:18080",
        "http://192.168.1.57:18081",
        "http://192.168.1.57:18080/api",
        "http://192.168.1.57",
      ]) {
        const built = withServer(defaultServer);
        const result = expecting(built, { expectServer: SERVER });
        expect(result.ok, defaultServer).toBe(false);
        const lines = driveLines(failures(result));
        expect(lines, defaultServer).toHaveLength(2);
        expect(lines[0], defaultServer).toContain(
          `defaultServer is ${JSON.stringify(defaultServer)}, it must be ${SERVER} (set OBOP_DEFAULT_SERVER=${SERVER})`,
        );
        expect(lines[1], defaultServer).toMatch(/env \| grep OBOP_/);
        // Without the option the same build passes.
        expect(failures(expecting(built, {})), defaultServer).toEqual([]);
      }
    });

    test("another bundle id, or none, fails with the variable that fixes it", () => {
      const other = forPc();
      other.infoPlist.CFBundleIdentifier = "org.obop.mileage";
      const result = expecting(other, { expectBundleId: BUNDLE });
      expect(result.ok).toBe(false);
      const lines = driveLines(failures(result));
      expect(lines).toHaveLength(2);
      expect(lines[0]).toContain(
        `the bundle id is "org.obop.mileage", it must be ${BUNDLE} (set OBOP_IOS_BUNDLE_ID=${BUNDLE})`,
      );
      expect(failures(expecting(other, {}))).toEqual([]);
      const none = forPc();
      delete none.infoPlist.CFBundleIdentifier;
      expect(driveLines(failures(expecting(none, { expectBundleId: BUNDLE })))[0]).toContain(
        "the bundle id is missing from Info.plist",
      );
    });

    test("a bundle id that only starts with the expected one, or ends with it, is another id", () => {
      for (const other of [
        "org.obop.mileage.test2",
        "org.obop.mileage.test.extra",
        "com.org.obop.mileage.test",
      ]) {
        const built = forPc();
        built.infoPlist.CFBundleIdentifier = other;
        const result = expecting(built, { expectBundleId: BUNDLE });
        expect(result.ok, other).toBe(false);
        const lines = driveLines(failures(result));
        expect(lines, other).toHaveLength(2);
        expect(lines[0], other).toContain(
          `the bundle id is ${JSON.stringify(other)}, it must be ${BUNDLE} (set OBOP_IOS_BUNDLE_ID=${BUNDLE})`,
        );
      }
    });

    test("both wrong gives both lines and one remedy", () => {
      const built = withServer("http://10.0.0.1:1");
      built.infoPlist.CFBundleIdentifier = "org.obop.mileage";
      const lines = driveLines(
        failures(expecting(built, { expectServer: SERVER, expectBundleId: BUNDLE })),
      );
      expect(lines).toHaveLength(3);
    });

    test("an empty default server is reported once, as not an http address", () => {
      const lines = driveLines(failures(expecting(withServer(""), { expectServer: SERVER })));
      expect(lines).toHaveLength(2);
      expect(lines[0]).toMatch(/defaultServer is empty/);
    });

    test("the options need the drive check, and the server must be an http address", () => {
      expect(() => checkBuild(forPc(), { mode: "test", expectServer: SERVER })).toThrow(
        /belong to the drive check/,
      );
      expect(() => checkBuild(forPc(), { mode: "test", expectBundleId: BUNDLE })).toThrow(
        /belong to the drive check/,
      );
      for (const bad of ["https://192.168.1.57:18080", "192.168.1.57:18080", ""]) {
        expect(() => expecting(forPc(), { expectServer: bad }), bad).toThrow(/http:\/\/ address/);
      }
    });
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

    describe("--expect-server and --expect-bundle-id", () => {
      const forPc = () => {
        const built = driveBuild();
        built.config.extra = {
          cleartextAllowed: true,
          defaultServer: "http://192.168.1.57:18080",
          thinningOff: true,
        };
        return built;
      };
      const expectBoth = [
        "--test-build",
        "--drive",
        "--expect-server",
        "http://192.168.1.57:18080/",
        "--expect-bundle-id",
        "org.obop.mileage.test",
      ];

      test("exit 0 and the lines 'as expected' when the build is the expected one", () => {
        const result = run(forPc(), ...expectBoth);
        expect(result.status, result.stdout + result.stderr).toBe(0);
        expect(result.stdout).toContain(
          "the default server is http://192.168.1.57:18080/, as expected",
        );
        expect(result.stdout).toContain("the bundle id is org.obop.mileage.test, as expected");
        expect(result.stdout).toContain("gate passed (test build)");
      });

      test("exit 1 for another server or another bundle id, and 0 without the options", () => {
        const server = forPc();
        server.config.extra = {
          ...(server.config.extra as object),
          defaultServer: "http://192.168.1.1:18080",
        };
        const wrongServer = run(server, ...expectBoth);
        expect(wrongServer.status).toBe(1);
        expect(wrongServer.stdout).toContain("OBOP_DEFAULT_SERVER=http://192.168.1.57:18080/");
        expect(wrongServer.stdout).toContain("gate FAILED (test build)");
        expect(run(server, "--test-build", "--drive").status).toBe(0);
        const bundle = forPc();
        bundle.infoPlist.CFBundleIdentifier = "org.obop.mileage";
        const wrongBundle = run(bundle, ...expectBoth);
        expect(wrongBundle.status).toBe(1);
        expect(wrongBundle.stdout).toContain("OBOP_IOS_BUNDLE_ID=org.obop.mileage.test");
        expect(run(bundle, "--test-build", "--drive").status).toBe(0);
      });

      test("exit 1 for a bundle id that only starts with the expected one", () => {
        const prefixed = forPc();
        prefixed.infoPlist.CFBundleIdentifier = "org.obop.mileage.test2";
        const result = run(prefixed, ...expectBoth);
        expect(result.status, result.stdout + result.stderr).toBe(1);
        expect(result.stdout).toContain(
          'FAIL  drive build: the bundle id is "org.obop.mileage.test2", it must be org.obop.mileage.test',
        );
        expect(result.stdout).not.toContain("the bundle id is org.obop.mileage.test, as expected");
        expect(result.stdout).toContain("gate FAILED (test build)");
      });

      test("exit 2 without --drive, without a value, or with an address that is not http", () => {
        const noDrive = run(
          forPc(),
          "--test-build",
          "--expect-server",
          "http://192.168.1.57:18080",
        );
        expect(noDrive.status).toBe(2);
        expect(noDrive.stderr).toContain("--expect-server belongs to --drive");
        const noValue = run(forPc(), "--test-build", "--drive", "--expect-bundle-id");
        expect(noValue.status).toBe(2);
        expect(noValue.stderr).toContain("--expect-bundle-id takes a value");
        const flagAsValue = run(
          forPc(),
          "--test-build",
          "--drive",
          "--expect-server",
          "--expect-bundle-id",
          "x",
        );
        expect(flagAsValue.status).toBe(2);
        expect(flagAsValue.stderr).toContain("--expect-server takes a value");
        const https = run(
          forPc(),
          "--test-build",
          "--drive",
          "--expect-server",
          "https://192.168.1.57:18080",
        );
        expect(https.status).toBe(2);
        expect(https.stderr).toContain("--expect-server must be an http:// address");
      });

      test("the value of an option is not taken for the path of the app", () => {
        const result = spawnSync(process.execPath, [script, "APP.app", ...expectBoth], {
          encoding: "utf8",
        });
        expect(result.status).toBe(2);
        expect(result.stderr).not.toContain("give the path of one built .app");
      });
    });
  });

  test("exits 2 when it is not told what to read", () => {
    const result = spawnSync(process.execPath, [script], { encoding: "utf8" });
    expect(result.status).toBe(2);
    expect(result.stderr).toContain("usage:");
  });
});
