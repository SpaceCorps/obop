import { describe, expect, test } from "vite-plus/test";
import { iosBundleId, withoutDevClientEntries } from "./ios.ts";

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

describe("the entries of the development client", () => {
  const dev = {
    CFBundleURLTypes: [
      { CFBundleURLSchemes: ["org.obop.rutsa"] },
      { CFBundleURLSchemes: ["exp+rutsa"] },
    ],
    NSBonjourServices: ["_expo._tcp"],
    UIBackgroundModes: ["location"],
  };

  test("the Bonjour service and the exp+ scheme are removed, everything else stays", () => {
    const result = withoutDevClientEntries(dev);
    expect(result.CFBundleURLTypes).toEqual([{ CFBundleURLSchemes: ["org.obop.rutsa"] }]);
    expect(result).not.toHaveProperty("NSBonjourServices");
    expect(result.UIBackgroundModes).toEqual(["location"]);
  });

  test("another Bonjour service stays, and a type with an app scheme and an exp+ one stays whole", () => {
    const result = withoutDevClientEntries({
      NSBonjourServices: ["_expo._tcp", "_other._tcp"],
      CFBundleURLTypes: [{ CFBundleURLSchemes: ["exp+rutsa", "org.obop.rutsa"] }],
    });
    expect(result.NSBonjourServices).toEqual(["_other._tcp"]);
    expect(result.CFBundleURLTypes).toHaveLength(1);
  });

  test("a plist without them is returned as it is, and the input is not changed", () => {
    expect(withoutDevClientEntries({ UIBackgroundModes: ["location"] })).toEqual({
      UIBackgroundModes: ["location"],
    });
    withoutDevClientEntries(dev);
    expect(dev.NSBonjourServices).toEqual(["_expo._tcp"]);
    expect(dev.CFBundleURLTypes).toHaveLength(2);
  });

  test("nothing is left of an empty list, and a scheme that merely begins with exp stays", () => {
    expect(
      withoutDevClientEntries({ CFBundleURLTypes: [{ CFBundleURLSchemes: ["exp+x"] }] }),
    ).not.toHaveProperty("CFBundleURLTypes");
    expect(
      withoutDevClientEntries({
        CFBundleURLTypes: [{ CFBundleURLSchemes: ["expenses"] }, { CFBundleURLSchemes: ["exp+x"] }],
      }).CFBundleURLTypes,
    ).toEqual([{ CFBundleURLSchemes: ["expenses"] }]);
  });
});
