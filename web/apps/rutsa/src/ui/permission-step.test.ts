import { describe, expect, test } from "vite-plus/test";
import type { LocationAccess } from "../native/permissions.ts";
import {
  firstStep,
  notificationsStep,
  stepAfterAccess,
  stepAfterRefusal,
  stepOnReturn,
  waitsForFix,
  type Step,
} from "./permission-step.ts";

const ALL: LocationAccess[] = ["granted", "approximate", "ask", "blocked", "services_off"];

describe("the first Start", () => {
  test("the explanation comes first, whatever the access, until it has been read", () => {
    for (const access of ALL) expect(firstStep(false, access), access).toBe("disclosure");
  });

  test("once it was read, a granted permission goes straight to the trip", () => {
    expect(firstStep(true, "granted")).toBe("begin");
  });

  test("a permission that was never asked shows the explanation again, whose button asks", () => {
    expect(firstStep(true, "ask")).toBe("disclosure");
  });

  test("whatever stands in the way is shown with its way on", () => {
    expect(firstStep(true, "blocked")).toBe("blocked");
    expect(firstStep(true, "approximate")).toBe("approximate");
    expect(firstStep(true, "services_off")).toBe("services");
  });
});

describe("after the system dialog", () => {
  test("allowed: the note to keep the app running, then the trip", () => {
    expect(stepAfterAccess("granted")).toBe("guide");
  });

  test("refused once, refused for good, approximate, switched off", () => {
    expect(stepAfterAccess("ask")).toBe("denied");
    expect(stepAfterAccess("blocked")).toBe("blocked");
    expect(stepAfterAccess("approximate")).toBe("approximate");
    expect(stepAfterAccess("services_off")).toBe("services");
  });
});

describe("when the phone refuses to start", () => {
  test("location switched off: Android reads the permission as granted, the start finds it", () => {
    expect(stepAfterRefusal("services_off", "granted")).toBe("services");
  });

  test("approximate is approximate", () => {
    expect(stepAfterRefusal("approximate", "approximate")).toBe("approximate");
  });

  test("no permission is shown as the access says it now", () => {
    expect(stepAfterRefusal("no_permission", "ask")).toBe("denied");
    expect(stepAfterRefusal("no_permission", "blocked")).toBe("blocked");
  });
});

describe("the notifications of Android 13", () => {
  test("are asked once, after a tap of the driver", () => {
    expect(notificationsStep(false, true)).toBe("ask");
  });

  test("are not asked again once the driver answered, also after a refusal", () => {
    expect(notificationsStep(true, true)).toBe("read");
  });

  test("are never asked on a start that goes on by itself after a return from the settings", () => {
    expect(notificationsStep(false, false)).toBe("read");
    expect(notificationsStep(true, false)).toBe("read");
  });
});

describe("coming back from the settings", () => {
  test("only the steps that send the driver away check again", () => {
    const steps: Step[] = [
      "check",
      "disclosure",
      "denied",
      "blocked",
      "approximate",
      "services",
      "guide",
    ];
    expect(steps.filter(waitsForFix)).toEqual(["denied", "blocked", "approximate", "services"]);
  });

  test("with location allowed the trip starts without another tap", () => {
    expect(stepOnReturn("granted")).toBe("begin");
  });

  test("what still stands in the way stays on the screen", () => {
    expect(stepOnReturn("blocked")).toBe("blocked");
    expect(stepOnReturn("approximate")).toBe("approximate");
    expect(stepOnReturn("services_off")).toBe("services");
    expect(stepOnReturn("ask")).toBe("denied");
  });
});
