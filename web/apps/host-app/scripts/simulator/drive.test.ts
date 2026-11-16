import { describe, expect, test } from "vite-plus/test";
import { redact, runnerEnv, scriptProblems, sessionVerdict, testArgs } from "./drive-lib.mjs";

const UDID = "4A160293-D409-4C29-9CE3-38F0E98EAB4D";

describe("the script of the drive", () => {
  test("a script of known actions has no problems; comments and blank lines are skipped", () => {
    const script = [
      "# connect",
      "launch",
      "tap address-field",
      "type address-field http://127.0.0.1:18180",
      "key Go",
      "wait 30 Forgot your password",
      "",
      "tapxy 0.684 0.603",
      "orient landscape",
      "shot 01-address",
    ].join("\n");
    expect(scriptProblems(script)).toEqual([]);
  });

  test("an unknown action, a wrong number of words or a bad name is a problem of its line", () => {
    const problems = scriptProblems(
      [
        "launch now",
        "fly away",
        "tap",
        "shot a/b",
        "orient sideways",
        "wait soon x",
        "tapxy 1",
      ].join("\n"),
    );
    expect(problems).toHaveLength(7);
    expect(problems[0]).toMatch(/^line 1: launch takes 0/);
    expect(problems[1]).toMatch(/^line 2: unknown action "fly"/);
    expect(problems[2]).toMatch(/needs an argument/);
    expect(problems[3]).toMatch(/screenshot name/);
    expect(problems[4]).toMatch(/landscape or portrait/);
    expect(problems[5]).toMatch(/seconds and a text/);
    expect(problems[6]).toMatch(/tapxy takes 2/);
  });
});

describe("the xcodebuild session", () => {
  test("runs the one method of the bundle on the named Simulator, unsigned", () => {
    const args = testArgs({
      project: "/r/gen/HostDrive.xcodeproj",
      device: UDID,
      derivedData: "/r/dd",
      resultBundle: "/r/x.xcresult",
    });
    expect(args[0]).toBe("test");
    expect(args).toContain(`id=${UDID}`);
    expect(args).toContain("-only-testing:HostDrive/HostDrive/testScript");
    expect(args).toContain("CODE_SIGNING_ALLOWED=NO");
    expect(args.join(" ")).not.toMatch(/team|provision|iphoneos/i);
    expect(() =>
      testArgs({ project: "p", device: "booted", derivedData: "d", resultBundle: "r" }),
    ).toThrow(/UDID/);
  });

  test("the runner gets only its own variables, and a script that types a secret needs it", () => {
    const env = runnerEnv({ OBOP_SCRIPT: "launch", OBOP_SHOT_DIR: "/s", OTHER: "x" });
    expect(env.TEST_RUNNER_OBOP_SCRIPT).toBe("launch");
    expect(env.TEST_RUNNER_OBOP_SHOT_DIR).toBe("/s");
    expect(env.TEST_RUNNER_OTHER).toBeUndefined();
    expect(() => runnerEnv({ OBOP_SCRIPT: "type Password $PASSWORD" })).toThrow(
      /OBOP_DRIVER_PASSWORD/,
    );
    expect(() => runnerEnv({ OBOP_SCRIPT: "type Username $USER" })).toThrow(/OBOP_DRIVER_USER/);
  });

  test("the verdict line and the redaction", () => {
    expect(sessionVerdict("Test Suite 'Selected tests' passed at 2026-11-16 10:00:00.000.")).toBe(
      "passed",
    );
    expect(sessionVerdict("Test Suite 'Selected tests' failed at 2026-11-16")).toBe("failed");
    expect(sessionVerdict("STEP launch")).toBeNull();
    expect(redact("pw=secret-1 and secret-1", ["secret-1"])).toBe("pw=*** and ***");
    expect(redact("abc", ["ab"])).toBe("abc");
  });
});
