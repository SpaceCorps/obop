import { describe, expect, test } from "vite-plus/test";
import {
  NO_FIX_MS,
  QUIET_MS,
  lastFixOfSegment,
  readiness,
  recordingHealth,
  reportedAccuracy,
} from "./liveness.ts";

const T0 = Date.parse("2026-11-21T09:00:00Z");
const SEC = 1000;
const MIN = 60_000;

describe("the health of a trip that is recording", () => {
  const health = (lastFixAt: number | null | undefined, now: number) =>
    recordingHealth({ segmentStartedAt: T0, lastFixAt, now });

  test("before the positions have been read nothing is said, however old the segment is", () => {
    expect(health(undefined, T0 + 10 * MIN)).toEqual({ kind: "waiting" });
  });

  test("no position yet is normal for a minute, and then it is no position", () => {
    expect(health(null, T0)).toEqual({ kind: "waiting" });
    expect(health(null, T0 + NO_FIX_MS - 1)).toEqual({ kind: "waiting" });
    expect(health(null, T0 + NO_FIX_MS)).toEqual({ kind: "no_fix" });
    expect(health(null, T0 + 30 * MIN)).toEqual({ kind: "no_fix" });
  });

  test("a position that came recently is ok, also when the segment is old", () => {
    expect(health(T0 + 2 * SEC, T0 + 5 * SEC)).toEqual({ kind: "ok" });
    expect(health(T0 + 40 * MIN, T0 + 41 * MIN)).toEqual({ kind: "ok" });
  });

  test("a last position three minutes old is quiet, with its whole minutes", () => {
    expect(health(T0, T0 + QUIET_MS - 1)).toEqual({ kind: "ok" });
    expect(health(T0, T0 + QUIET_MS)).toEqual({ kind: "quiet", minutes: 3 });
    expect(health(T0, T0 + 7 * MIN + 59 * SEC)).toEqual({ kind: "quiet", minutes: 7 });
  });

  test("a position with a time after the clock of the phone is not old", () => {
    expect(health(T0 + MIN, T0)).toEqual({ kind: "ok" });
  });

  test("after a Resume the clock starts again at the start of the new segment", () => {
    // Recording since T0, paused at +10 min, resumed at +30 min.
    const resumedAt = T0 + 30 * MIN;
    const points = [
      { segment: 1, t: T0 + 5 * MIN },
      { segment: 1, t: T0 + 10 * MIN },
    ];
    const at = (now: number) =>
      recordingHealth({
        segmentStartedAt: resumedAt,
        lastFixAt: lastFixOfSegment(points, 2),
        now,
      });
    // The positions of the first segment are 20 minutes old, and say nothing about the second.
    expect(at(resumedAt + 10 * SEC)).toEqual({ kind: "waiting" });
    expect(at(resumedAt + NO_FIX_MS)).toEqual({ kind: "no_fix" });
  });
});

describe("the last position of a segment", () => {
  test("the newest time of that segment, whatever the order of the rows", () => {
    const points = [
      { segment: 1, t: 30 },
      { segment: 2, t: 50 },
      { segment: 2, t: 70 },
      { segment: 2, t: 60 },
      { segment: 3, t: 10 },
    ];
    expect(lastFixOfSegment(points, 1)).toBe(30);
    expect(lastFixOfSegment(points, 2)).toBe(70);
    expect(lastFixOfSegment(points, 3)).toBe(10);
  });

  test("a segment with no positions, or no positions at all, has none", () => {
    expect(lastFixOfSegment([{ segment: 1, t: 30 }], 2)).toBeNull();
    expect(lastFixOfSegment([], 1)).toBeNull();
  });
});

describe("the accuracy a platform reports", () => {
  test("Android says fine, coarse or none, and that is passed on", () => {
    expect(reportedAccuracy({ android: { accuracy: "fine" } })).toBe("fine");
    expect(reportedAccuracy({ android: { accuracy: "coarse" } })).toBe("coarse");
    expect(reportedAccuracy({ android: { accuracy: "none" } })).toBe("none");
  });

  test("iOS says full or reduced: full is fine, anything else is coarse", () => {
    expect(reportedAccuracy({ ios: { accuracy: "full" } })).toBe("fine");
    expect(reportedAccuracy({ ios: { accuracy: "reduced" } })).toBe("coarse");
    expect(reportedAccuracy({ ios: { accuracy: "something new" } })).toBe("coarse");
  });

  test("a platform that says nothing has no accuracy to judge", () => {
    expect(reportedAccuracy({})).toBeUndefined();
    expect(reportedAccuracy({ ios: {} })).toBeUndefined();
  });
});

describe("whether the phone can record a trip", () => {
  test("a precise permission with location on is ready, also where no accuracy is reported", () => {
    expect(readiness({ granted: true, accuracy: "fine", servicesEnabled: true })).toBe("ready");
    expect(readiness({ granted: true, accuracy: undefined, servicesEnabled: true })).toBe("ready");
  });

  test("an approximate permission cannot measure a trip", () => {
    expect(readiness({ granted: true, accuracy: "coarse", servicesEnabled: true })).toBe(
      "approximate",
    );
    expect(readiness({ granted: true, accuracy: "none", servicesEnabled: true })).toBe(
      "approximate",
    );
  });

  test("no permission is no permission", () => {
    expect(readiness({ granted: false, accuracy: undefined, servicesEnabled: true })).toBe(
      "no_permission",
    );
  });

  test("on Android the permission is named before the location switch", () => {
    expect(readiness({ granted: false, accuracy: undefined, servicesEnabled: false })).toBe(
      "no_permission",
    );
    expect(readiness({ granted: true, accuracy: "fine", servicesEnabled: false })).toBe(
      "services_off",
    );
  });

  test("on iOS the location switch is named first: with it off the app reads as refused", () => {
    expect(
      readiness({
        granted: false,
        accuracy: undefined,
        servicesEnabled: false,
        servicesFirst: true,
      }),
    ).toBe("services_off");
    expect(
      readiness({
        granted: false,
        accuracy: undefined,
        servicesEnabled: true,
        servicesFirst: true,
      }),
    ).toBe("no_permission");
  });
});
