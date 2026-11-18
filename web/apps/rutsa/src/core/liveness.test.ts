import { describe, expect, test } from "vite-plus/test";
import { readiness, reportedAccuracy } from "./liveness.ts";

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
