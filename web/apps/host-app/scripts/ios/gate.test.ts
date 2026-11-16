import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { afterAll, describe, expect, test } from "vite-plus/test";
import { checkBuild, signingFacts, type Finding, type Signing } from "./gate-lib.mjs";
import { IOS_USAGE_TEXTS, withTransportSecurity } from "../../config/ios.ts";

const adhoc: Signing = { profile: false, signature: "adhoc", team: null };

/** The Info.plist of a build as app.config.ts makes it, plain or test. */
function plist(local: boolean, extra: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    ...withTransportSecurity(
      { CFBundleIdentifier: "org.obop.app.test", ...IOS_USAGE_TEXTS },
      local,
    ),
    ...extra,
  };
}
const config = (extra: Record<string, unknown>) => ({ extra });
const plainConfig = config({ cleartextAllowed: false, webviewDebug: false, defaultServer: "" });
const testConfig = config({
  cleartextAllowed: true,
  webviewDebug: true,
  defaultServer: "http://127.0.0.1:18180",
});
const fails = (findings: Finding[]) =>
  findings.filter((f) => f.level === "fail").map((f) => f.text);
const warns = (findings: Finding[]) =>
  findings.filter((f) => f.level === "warn").map((f) => f.text);

describe("a plain build", () => {
  test("passes as a release build", () => {
    const result = checkBuild(
      { config: plainConfig, infoPlist: plist(false), signing: adhoc },
      { mode: "release" },
    );
    expect(fails(result.findings)).toEqual([]);
    expect(result.ok).toBe(true);
    expect(result.facts.transportSecurity).toBe('{"NSAllowsArbitraryLoads":false}');
    expect(result.facts.signing).toBe("adhoc");
  });

  test("fails with plain http, DevTools, an http default server or local networking", () => {
    const result = checkBuild(
      { config: testConfig, infoPlist: plist(true), signing: adhoc },
      { mode: "release" },
    );
    const texts = fails(result.findings).join("\n");
    expect(texts).toMatch(/cleartextAllowed=true/);
    expect(texts).toMatch(/webviewDebug=true/);
    expect(texts).toMatch(/default server of a release build must be an https address/);
    expect(texts).toMatch(/NSAllowsLocalNetworking is true/);
    expect(result.ok).toBe(false);
  });

  test("fails when the config does not say (not built from this app.config.ts)", () => {
    const result = checkBuild(
      { config: {}, infoPlist: plist(false), signing: adhoc },
      { mode: "release" },
    );
    expect(fails(result.findings).join("\n")).toMatch(
      /no extra.cleartextAllowed[\s\S]*no extra.webviewDebug/,
    );
  });

  test("the entries of the development client fail it", () => {
    const result = checkBuild(
      {
        config: plainConfig,
        infoPlist: plist(false, {
          NSBonjourServices: ["_expo._tcp"],
          CFBundleURLTypes: [{ CFBundleURLSchemes: ["exp+obop-host"] }],
        }),
        signing: adhoc,
      },
      { mode: "release" },
    );
    expect(fails(result.findings)).toHaveLength(2);
  });
});

describe("a test build", () => {
  test("passes and says what it allows: plain http, DevTools, local networking", () => {
    const result = checkBuild(
      { config: testConfig, infoPlist: plist(true), signing: adhoc },
      { mode: "test" },
    );
    expect(result.ok).toBe(true);
    const oks = result.findings
      .filter((f) => f.level === "ok")
      .map((f) => f.text)
      .join("\n");
    expect(oks).toMatch(/test build: plain http is allowed/);
    expect(oks).toMatch(/test build: the WebView is open to DevTools/);
    expect(oks).toMatch(/allows local networking \(NSAllowsLocalNetworking\), nothing more/);
    expect(warns(result.findings)).toEqual([]);
  });

  test("warns when the prebuild and the build had different variables", () => {
    const noLocal = checkBuild(
      { config: testConfig, infoPlist: plist(false), signing: adhoc },
      { mode: "test" },
    );
    expect(warns(noLocal.findings).join("\n")).toMatch(/iOS will refuse the http address/);
    const localOnly = checkBuild(
      { config: plainConfig, infoPlist: plist(true), signing: adhoc },
      { mode: "test" },
    );
    expect(warns(localOnly.findings).join("\n")).toMatch(/different variables/);
  });
});

describe("in either mode", () => {
  for (const mode of ["release", "test"] as const) {
    const cfg = mode === "release" ? plainConfig : testConfig;
    const local = mode === "test";

    test(`${mode}: any other App Transport Security exception fails`, () => {
      for (const ats of [
        { NSAllowsArbitraryLoads: true },
        { NSAllowsArbitraryLoadsInWebContent: true },
        { NSAllowsArbitraryLoadsForMedia: true },
        { NSExceptionDomains: { "example.com": {} } },
      ]) {
        const result = checkBuild(
          { config: cfg, infoPlist: plist(local, { NSAppTransportSecurity: ats }), signing: adhoc },
          { mode },
        );
        expect(result.ok, JSON.stringify(ats)).toBe(false);
      }
    });

    test(`${mode}: a provisioning profile, a team or a certificate fails`, () => {
      for (const signing of [
        { profile: true, signature: "adhoc", team: null },
        { profile: false, signature: "adhoc", team: "ABCDE12345" },
        { profile: false, signature: "identity", team: null },
      ] satisfies Signing[]) {
        const result = checkBuild({ config: cfg, infoPlist: plist(local), signing }, { mode });
        expect(result.ok, JSON.stringify(signing)).toBe(false);
      }
      const unsigned = checkBuild(
        {
          config: cfg,
          infoPlist: plist(local),
          signing: { profile: false, signature: "none", team: null },
        },
        { mode },
      );
      expect(unsigned.ok).toBe(true);
    });

    test(`${mode}: no background mode and no location`, () => {
      const result = checkBuild(
        {
          config: cfg,
          infoPlist: plist(local, {
            UIBackgroundModes: ["location"],
            NSLocationWhenInUseUsageDescription: "x",
          }),
          signing: adhoc,
        },
        { mode },
      );
      expect(fails(result.findings)).toHaveLength(2);
    });

    test(`${mode}: the local network text is the app's own`, () => {
      const missing = plist(local);
      delete missing.NSLocalNetworkUsageDescription;
      expect(checkBuild({ config: cfg, infoPlist: missing, signing: adhoc }, { mode }).ok).toBe(
        false,
      );
      const dev = plist(local, { NSLocalNetworkUsageDescription: "Expo Dev Launcher uses ..." });
      expect(checkBuild({ config: cfg, infoPlist: dev, signing: adhoc }, { mode }).ok).toBe(false);
    });
  }
});

describe("the signing facts from codesign", () => {
  test("ad hoc, unsigned and a signature for a team", () => {
    expect(
      signingFacts(
        "Executable=/x/obop\nIdentifier=org.obop.app.test\nSignature=adhoc\nTeamIdentifier=not set\n",
        false,
      ),
    ).toEqual({ profile: false, signature: "adhoc", team: null });
    expect(signingFacts("/x/obop.app: code object is not signed at all", false)).toEqual({
      profile: false,
      signature: "none",
      team: null,
    });
    expect(
      signingFacts("Authority=Apple Development: X (Y)\nTeamIdentifier=ABCDE12345\n", true),
    ).toEqual({ profile: true, signature: "identity", team: "ABCDE12345" });
  });
});

describe("the command", () => {
  const script = fileURLToPath(new URL("./gate.mjs", import.meta.url));
  const dir = mkdtempSync(join(tmpdir(), "obop-host-gate-"));
  afterAll(() => rmSync(dir, { recursive: true, force: true }));
  const write = (name: string, value: unknown) => {
    const path = join(dir, name);
    writeFileSync(path, typeof value === "string" ? value : JSON.stringify(value));
    return path;
  };
  const run = (...args: string[]) =>
    spawnSync(process.execPath, [script, ...args], { encoding: "utf8" });
  const plainPlist = write("plain.json", plist(false));
  const testPlist = write("test.json", plist(true));
  const plainCfg = write("plain-config.json", plainConfig);
  const testCfg = write("test-config.json", testConfig);
  const adhocText = write("adhoc.txt", "Signature=adhoc\nTeamIdentifier=not set\n");
  const teamText = write("team.txt", "Authority=Apple Development\nTeamIdentifier=ABCDE12345\n");

  test("exits 0 for the plain build and 1 for the test build as a release", () => {
    const plain = run(
      "--info-plist-json",
      plainPlist,
      "--app-config",
      plainCfg,
      "--codesign-text",
      adhocText,
    );
    expect(plain.status, plain.stdout).toBe(0);
    expect(plain.stdout).toMatch(/gate passed \(release build\)/);
    const asRelease = run("--info-plist-json", testPlist, "--app-config", testCfg);
    expect(asRelease.status).toBe(1);
    const asTest = run("--info-plist-json", testPlist, "--app-config", testCfg, "--test-build");
    expect(asTest.status, asTest.stdout).toBe(0);
  });

  test("exits 1 for a profile or a team, in a test build too", () => {
    expect(
      run("--info-plist-json", testPlist, "--app-config", testCfg, "--test-build", "--profile")
        .status,
    ).toBe(1);
    expect(
      run(
        "--info-plist-json",
        testPlist,
        "--app-config",
        testCfg,
        "--test-build",
        "--codesign-text",
        teamText,
      ).status,
    ).toBe(1);
  });

  test("exits 2 on wrong use", () => {
    expect(run().status).toBe(2);
    expect(run("--info-plist-json", plainPlist).status).toBe(2);
    expect(run("--info-plist-json", plainPlist, "--app-config", plainCfg, "--drive").status).toBe(
      2,
    );
    expect(run("--info-plist-json", "--app-config", plainCfg).status).toBe(2);
  });
});
