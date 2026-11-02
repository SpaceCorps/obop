import { describe, expect, test } from "vite-plus/test";
import { firstStep, stepAfterAccess } from "./permission-step.ts";

describe("the screen after an answer about the permission", () => {
  test("each answer has its own screen", () => {
    expect(stepAfterAccess("granted")).toBe("guide");
    expect(stepAfterAccess("approximate")).toBe("approximate");
    expect(stepAfterAccess("ask")).toBe("denied");
    expect(stepAfterAccess("blocked")).toBe("blocked");
    expect(stepAfterAccess("services_off")).toBe("services");
  });
});

describe("the first screen of a Start", () => {
  test("the disclosure comes first until it was read, whatever the answer", () => {
    for (const access of ["granted", "approximate", "ask", "blocked", "services_off"] as const) {
      expect(firstStep(false, access)).toBe("disclosure");
    }
  });

  test("after it, a granted permission goes straight to the trip", () => {
    expect(firstStep(true, "granted")).toBe("begin");
  });

  test("a permission not asked yet shows the disclosure again, its button asks", () => {
    expect(firstStep(true, "ask")).toBe("disclosure");
  });

  test("whatever stands in the way is shown with its way on", () => {
    expect(firstStep(true, "approximate")).toBe("approximate");
    expect(firstStep(true, "blocked")).toBe("blocked");
    expect(firstStep(true, "services_off")).toBe("services");
  });
});
