import { describe, expect, test } from "vite-plus/test";
import { iosBundleId } from "./ios.ts";

describe("the bundle id of the iOS build", () => {
  test("is the application id unless the build names another", () => {
    expect(iosBundleId(undefined, "org.obop.rutsa")).toBe("org.obop.rutsa");
    expect(iosBundleId("", "org.obop.rutsa")).toBe("org.obop.rutsa");
    expect(iosBundleId("org.obop.rutsa.test", "org.obop.rutsa")).toBe("org.obop.rutsa.test");
    expect(iosBundleId("com.example.my-app2", "x.y")).toBe("com.example.my-app2");
  });

  test("refuses a value that is not a bundle id (it goes into the Xcode project)", () => {
    const bad = ["rutsa", "org..obop", "org.obop.", ".org.obop", "org obop.x", "a.b;rm", "é.x"];
    for (const value of bad) {
      expect(() => iosBundleId(value, "org.obop.rutsa"), value).toThrow(/OBOP_IOS_BUNDLE_ID/);
    }
  });
});
