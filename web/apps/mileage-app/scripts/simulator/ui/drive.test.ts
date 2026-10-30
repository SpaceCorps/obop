import { describe, expect, test } from "vite-plus/test";
import {
  ROUTE_MARKER,
  TRIP_STEPS,
  isRouteMarker,
  sessionVerdict,
  redact,
  runnerEnv,
  testArgs,
} from "./drive-lib.mjs";

const UDID = "B7B0C077-9759-4B59-9F9F-22D8C9800B2E";
const base = {
  project: "/b/uitest/gen/MileageDrive.xcodeproj",
  device: UDID,
  derivedData: "/b/uitest/dd",
  resultBundle: "/b/uitest/r.xcresult",
  only: TRIP_STEPS,
};

describe("the xcodebuild command of the drive", () => {
  test("runs every step in one session on the device", () => {
    const args = testArgs(base);
    expect(args[0]).toBe("test");
    expect(args).toContain(`id=${UDID}`);
    const only = args.filter((a) => a.startsWith("-only-testing:"));
    expect(only).toEqual(TRIP_STEPS.map((m) => `-only-testing:MileageDrive/MileageDrive/${m}`));
    // XCTest runs the methods of a class in the order of their names, so the names must sort as
    // the steps run.
    expect([...TRIP_STEPS].sort()).toEqual(TRIP_STEPS);
    expect(args).toContain("CODE_SIGNING_ALLOWED=NO");
    expect(args.join(" ")).not.toMatch(/team|provision|authenticat|apple ?id/i);
  });

  test("refuses what could change the command", () => {
    expect(() => testArgs({ ...base, device: "booted" })).toThrow(/UDID/);
    expect(() => testArgs({ ...base, device: `${UDID}; rm -rf /` })).toThrow(/UDID/);
    expect(() => testArgs({ ...base, scheme: "a b" })).toThrow(/scheme/);
    expect(() => testArgs({ ...base, only: [] })).toThrow(/at least one/);
    expect(() => testArgs({ ...base, only: ["test01; ls"] })).toThrow(/test method/);
    expect(() => testArgs({ ...base, only: ["notATest"] })).toThrow(/test method/);
  });
});

describe("the environment of the runner", () => {
  const env = {
    PATH: "/bin",
    OBOP_DRIVER_USER: "ivy",
    OBOP_DRIVER_PASSWORD: "p4ssw0rd-p4ssw0rd",
    OBOP_SHOT_DIR: "/b/shots",
    OBOP_SERVER_ADDRESS: "http://192.168.1.132:18084",
    OTHER: "x",
  };

  test("hands the names the Swift file reads on with the prefix xcodebuild strips", () => {
    const out = runnerEnv(env);
    expect(out.TEST_RUNNER_OBOP_SERVER_ADDRESS).toBe("http://192.168.1.132:18084");
    expect(out.TEST_RUNNER_OBOP_DRIVER_USER).toBe("ivy");
    expect(out.TEST_RUNNER_OBOP_DRIVER_PASSWORD).toBe("p4ssw0rd-p4ssw0rd");
    expect(out.TEST_RUNNER_OBOP_SHOT_DIR).toBe("/b/shots");
    expect(out.TEST_RUNNER_OBOP_APP_BUNDLE_ID).toBeUndefined();
    // Nothing else of the environment is renamed for the runner.
    expect(out.TEST_RUNNER_OTHER).toBeUndefined();
    expect(out.TEST_RUNNER_PATH).toBeUndefined();
    expect(out.PATH).toBe("/bin");
  });

  test("needs a user and a password", () => {
    expect(() => runnerEnv({ ...env, OBOP_DRIVER_PASSWORD: "" })).toThrow(/OBOP_DRIVER_PASSWORD/);
    expect(() => runnerEnv({ ...env, OBOP_DRIVER_USER: undefined })).toThrow(/OBOP_DRIVER_USER/);
  });
});

describe("the lines of the output", () => {
  test("only the whole line is the marker", () => {
    expect(isRouteMarker(ROUTE_MARKER)).toBe(true);
    expect(isRouteMarker(`  ${ROUTE_MARKER}  `)).toBe(true);
    expect(isRouteMarker(`print("${ROUTE_MARKER}")`)).toBe(false);
    expect(isRouteMarker("RECORDING-SHOWN-LATER")).toBe(false);
  });

  test("the result of the session is the suite of all selected tests", () => {
    expect(sessionVerdict("Test Suite 'Selected tests' passed at 2026-10-29 20:11:03.123.")).toBe(
      "passed",
    );
    expect(sessionVerdict("Test Suite 'Selected tests' failed at 2026-10-29 20:11:03.123.")).toBe(
      "failed",
    );
    expect(
      sessionVerdict("Test Suite 'MileageDrive' passed at 2026-10-29 20:11:03.123."),
    ).toBeNull();
    expect(sessionVerdict("** TEST SUCCEEDED **")).toBeNull();
  });

  test("a secret never survives", () => {
    expect(redact("typed hunter2hunter2 twice: hunter2hunter2", ["hunter2hunter2"])).toBe(
      "typed *** twice: ***",
    );
    // A short secret would blank out ordinary text, so it is not replaced.
    expect(redact("abc", ["abc"])).toBe("abc");
  });
});
