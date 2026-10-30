import { describe, expect, test } from "vite-plus/test";
import {
  iosBundleId,
  recordThinningOff,
  withoutDevClientEntries,
  withoutFetchMode,
} from "./ios.ts";

describe("the bundle id of the iOS build", () => {
  test("is the application id unless the build names another", () => {
    expect(iosBundleId(undefined, "org.obop.mileage")).toBe("org.obop.mileage");
    expect(iosBundleId("", "org.obop.mileage")).toBe("org.obop.mileage");
    expect(iosBundleId("org.obop.mileage.test", "org.obop.mileage")).toBe("org.obop.mileage.test");
    expect(iosBundleId("com.example.my-app2", "x.y")).toBe("com.example.my-app2");
  });

  test("refuses a value that is not a bundle id (it goes into the Xcode project)", () => {
    for (const bad of [
      "mileage",
      "org..obop",
      "org.obop.",
      ".org.obop",
      "org obop.x",
      "a.b;rm",
      "é.x",
    ]) {
      expect(() => iosBundleId(bad, "org.obop.mileage"), bad).toThrow(/OBOP_IOS_BUNDLE_ID/);
    }
  });
});

describe("the background modes", () => {
  test("fetch is removed and location stays", () => {
    expect(withoutFetchMode(["location", "fetch"])).toEqual(["location"]);
    expect(withoutFetchMode(["fetch", "location"])).toEqual(["location"]);
    expect(withoutFetchMode(["location"])).toEqual(["location"]);
  });

  test("nothing else is touched and a missing list is an empty one", () => {
    expect(withoutFetchMode(["location", "audio", "fetch"])).toEqual(["location", "audio"]);
    expect(withoutFetchMode(undefined)).toEqual([]);
    expect(withoutFetchMode("fetch")).toEqual([]);
    expect(withoutFetchMode(["location", 5, null])).toEqual(["location"]);
  });
});

describe("the entries of the development client", () => {
  const dev = {
    CFBundleURLTypes: [
      { CFBundleURLSchemes: ["org.obop.mileage", "org.obop.mileage"] },
      { CFBundleURLSchemes: ["exp+obop-mileage"] },
    ],
    NSBonjourServices: ["_expo._tcp"],
    UIBackgroundModes: ["location"],
  };

  test("the Bonjour service and the exp+ scheme are removed, the app's own scheme stays", () => {
    const result = withoutDevClientEntries(dev);
    expect(result.CFBundleURLTypes).toEqual([
      { CFBundleURLSchemes: ["org.obop.mileage", "org.obop.mileage"] },
    ]);
    expect(result).not.toHaveProperty("NSBonjourServices");
    expect(result.UIBackgroundModes).toEqual(["location"]);
  });

  test("another Bonjour service stays, and a type with a scheme of the app and an exp+ one stays whole", () => {
    const result = withoutDevClientEntries({
      NSBonjourServices: ["_expo._tcp", "_other._tcp"],
      CFBundleURLTypes: [{ CFBundleURLSchemes: ["exp+obop-mileage", "org.obop.mileage"] }],
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

  test("nothing is left of an empty list", () => {
    const result = withoutDevClientEntries({
      CFBundleURLTypes: [{ CFBundleURLSchemes: ["exp+x"] }],
    });
    expect(result).not.toHaveProperty("CFBundleURLTypes");
  });
});

describe("OBOP_RECORD_THINNING", () => {
  test("only off turns the thinning off, and unset keeps it", () => {
    expect(recordThinningOff(undefined)).toBe(false);
    expect(recordThinningOff("")).toBe(false);
    expect(recordThinningOff("off")).toBe(true);
  });

  test("anything else is a typo and stops the build", () => {
    for (const bad of ["0", "false", "OFF", "on", "1"]) {
      expect(() => recordThinningOff(bad), bad).toThrow(/OBOP_RECORD_THINNING/);
    }
  });
});

describe("only the schemes of the development client go", () => {
  test("a scheme that merely begins with exp stays", () => {
    const result = withoutDevClientEntries({
      CFBundleURLTypes: [{ CFBundleURLSchemes: ["expenses"] }, { CFBundleURLSchemes: ["exp+x"] }],
    });
    expect(result.CFBundleURLTypes).toEqual([{ CFBundleURLSchemes: ["expenses"] }]);
  });
});
