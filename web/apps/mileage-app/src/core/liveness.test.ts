import { describe, expect, test } from "vite-plus/test";
import { NO_FIX_MS, QUIET_MS, readiness, recordingHealth, reportedAccuracy } from "./liveness.ts";

const T0 = Date.parse("2026-10-15T09:00:00Z");

describe("is the trip really being recorded", () => {
  test("no position yet is normal for the first minute, a fault after it", () => {
    const at = (ms: number) =>
      recordingHealth({ startedAtMs: T0, lastFixMs: null, nowMs: T0 + ms });
    expect(at(0)).toEqual({ kind: "waiting" });
    expect(at(NO_FIX_MS - 1)).toEqual({ kind: "waiting" });
    expect(at(NO_FIX_MS)).toEqual({ kind: "no_fix" });
  });

  test("before the stored positions have been read nothing is said, however old the trip is", () => {
    expect(
      recordingHealth({ startedAtMs: T0, lastFixMs: undefined, nowMs: T0 + 3 * NO_FIX_MS }),
    ).toEqual({ kind: "waiting" });
    // Once read, an old trip without a position is a fault as before.
    expect(
      recordingHealth({ startedAtMs: T0, lastFixMs: null, nowMs: T0 + 3 * NO_FIX_MS }),
    ).toEqual({ kind: "no_fix" });
  });

  test("a position that is not old is fine, an old one is a soft notice with its age", () => {
    const at = (ms: number) =>
      recordingHealth({ startedAtMs: T0, lastFixMs: T0 + 10_000, nowMs: T0 + 10_000 + ms });
    expect(at(0)).toEqual({ kind: "ok" });
    expect(at(QUIET_MS - 1)).toEqual({ kind: "ok" });
    expect(at(QUIET_MS)).toEqual({ kind: "quiet", minutes: 3 });
    expect(at(10 * 60_000 + 5_000)).toEqual({ kind: "quiet", minutes: 10 });
  });

  test("a position after a long quiet spell makes it fine again", () => {
    expect(
      recordingHealth({
        startedAtMs: T0,
        lastFixMs: T0 + 20 * 60_000,
        nowMs: T0 + 20 * 60_000 + 5_000,
      }),
    ).toEqual({ kind: "ok" });
  });
});

describe("can the phone record now", () => {
  const base = { granted: true, accuracy: "fine", servicesEnabled: true };
  test("precise, allowed and switched on is ready", () => {
    expect(readiness(base)).toBe("ready");
  });
  test("a platform without the precise/approximate choice is ready", () => {
    expect(readiness({ ...base, accuracy: undefined })).toBe("ready");
  });
  test("approximate location is not enough", () => {
    expect(readiness({ ...base, accuracy: "coarse" })).toBe("approximate");
  });
  test("a refused permission outranks everything", () => {
    expect(readiness({ granted: false, accuracy: "none", servicesEnabled: false })).toBe(
      "no_permission",
    );
  });
  test("location switched off is named last", () => {
    expect(readiness({ ...base, servicesEnabled: false })).toBe("services_off");
  });
});

describe("the accuracy of a granted permission on either platform", () => {
  const ready = (permission: Parameters<typeof reportedAccuracy>[0]) =>
    readiness({
      granted: true,
      accuracy: reportedAccuracy(permission),
      servicesEnabled: true,
    });

  test("Android says fine or coarse and that is passed on", () => {
    expect(reportedAccuracy({ android: { accuracy: "fine" } })).toBe("fine");
    expect(reportedAccuracy({ android: { accuracy: "coarse" } })).toBe("coarse");
    expect(ready({ android: { accuracy: "fine" } })).toBe("ready");
    expect(ready({ android: { accuracy: "coarse" } })).toBe("approximate");
  });

  test("iOS says full or reduced: full is precise, reduced is approximate", () => {
    expect(reportedAccuracy({ ios: { accuracy: "full" } })).toBe("fine");
    expect(reportedAccuracy({ ios: { accuracy: "reduced" } })).toBe("coarse");
    expect(ready({ ios: { accuracy: "full" } })).toBe("ready");
    expect(ready({ ios: { accuracy: "reduced" } })).toBe("approximate");
  });

  test("a word that is not known is not taken for precise", () => {
    expect(ready({ ios: { accuracy: "something new" } })).toBe("approximate");
    expect(ready({ android: { accuracy: "none" } })).toBe("approximate");
  });

  test("a platform that says nothing has no choice to make", () => {
    expect(reportedAccuracy({})).toBeUndefined();
    expect(reportedAccuracy({ ios: {} })).toBeUndefined();
    expect(ready({})).toBe("ready");
  });
});
