import { describe, expect, test } from "vite-plus/test";
import {
  IOS_USAGE_TEXTS,
  iosBundleId,
  withTransportSecurity,
  withoutDevClientEntries,
} from "./ios.ts";

describe("the bundle id of the iOS build", () => {
  test("is the application id unless the build names another", () => {
    expect(iosBundleId(undefined, "org.obop.app.test")).toBe("org.obop.app.test");
    expect(iosBundleId("", "org.obop.app.test")).toBe("org.obop.app.test");
    expect(iosBundleId("org.obop.app.personal", "org.obop.app.test")).toBe("org.obop.app.personal");
    expect(iosBundleId("com.example.my-app2", "x.y")).toBe("com.example.my-app2");
  });

  test("refuses a value that is not a bundle id (it goes into the Xcode project)", () => {
    for (const bad of [
      "obop",
      "org..obop",
      "org.obop.",
      ".org.obop",
      "org obop.x",
      "a.b;rm",
      "é.x",
    ]) {
      expect(() => iosBundleId(bad, "org.obop.app.test"), bad).toThrow(/OBOP_IOS_BUNDLE_ID/);
    }
  });
});

describe("the entries of the development client", () => {
  const dev = {
    CFBundleURLTypes: [
      { CFBundleURLSchemes: ["org.obop.app.test"] },
      { CFBundleURLSchemes: ["exp+obop-host"] },
    ],
    NSBonjourServices: ["_expo._tcp"],
    NSCameraUsageDescription: "kept",
  };

  test("the Bonjour service and the exp+ scheme are removed, the app's own scheme stays", () => {
    const result = withoutDevClientEntries(dev);
    expect(result.CFBundleURLTypes).toEqual([{ CFBundleURLSchemes: ["org.obop.app.test"] }]);
    expect(result).not.toHaveProperty("NSBonjourServices");
    expect(result.NSCameraUsageDescription).toBe("kept");
  });

  test("another Bonjour service stays, and a type with a scheme of the app and an exp+ one stays whole", () => {
    const result = withoutDevClientEntries({
      NSBonjourServices: ["_expo._tcp", "_other._tcp"],
      CFBundleURLTypes: [{ CFBundleURLSchemes: ["exp+obop-host", "org.obop.app.test"] }],
    });
    expect(result.NSBonjourServices).toEqual(["_other._tcp"]);
    expect(result.CFBundleURLTypes).toHaveLength(1);
  });

  test("a plist without them is returned as it is, and the input is not changed", () => {
    expect(withoutDevClientEntries({ NSCameraUsageDescription: "kept" })).toEqual({
      NSCameraUsageDescription: "kept",
    });
    withoutDevClientEntries(dev);
    expect(dev.NSBonjourServices).toEqual(["_expo._tcp"]);
    expect(dev.CFBundleURLTypes).toHaveLength(2);
  });

  test("nothing is left of an empty list, and a scheme that merely begins with exp stays", () => {
    expect(
      withoutDevClientEntries({ CFBundleURLTypes: [{ CFBundleURLSchemes: ["exp+x"] }] }),
    ).not.toHaveProperty("CFBundleURLTypes");
    const result = withoutDevClientEntries({
      CFBundleURLTypes: [{ CFBundleURLSchemes: ["expenses"] }, { CFBundleURLSchemes: ["exp+x"] }],
    });
    expect(result.CFBundleURLTypes).toEqual([{ CFBundleURLSchemes: ["expenses"] }]);
  });
});

describe("the texts iOS shows when it asks", () => {
  test("are the four the app can make iOS ask for, and no location or tracking text", () => {
    expect(Object.keys(IOS_USAGE_TEXTS).toSorted()).toEqual([
      "NSCameraUsageDescription",
      "NSLocalNetworkUsageDescription",
      "NSMicrophoneUsageDescription",
      "NSPhotoLibraryAddUsageDescription",
    ]);
    expect(Object.isFrozen(IOS_USAGE_TEXTS)).toBe(true);
  });

  test("each is a sentence that names the app and says when", () => {
    for (const [key, text] of Object.entries(IOS_USAGE_TEXTS)) {
      expect(text, key).toMatch(/^obop /);
      expect(text, key).toMatch(/\.$/);
      expect(text.length, key).toBeGreaterThan(40);
    }
  });
});

describe("App Transport Security", () => {
  const template = {
    CFBundleName: "obop",
    NSAppTransportSecurity: {
      NSAllowsArbitraryLoads: false,
      NSAllowsLocalNetworking: true,
      NSExceptionDomains: { "example.com": { NSExceptionAllowsInsecureHTTPLoads: true } },
    },
  };

  test("a plain build has no exception at all, not even the template's local networking", () => {
    const plist = withTransportSecurity(template, false);
    expect(plist.NSAppTransportSecurity).toEqual({ NSAllowsArbitraryLoads: false });
    expect(plist.CFBundleName).toBe("obop");
    // The input is not changed.
    expect(template.NSAppTransportSecurity.NSAllowsLocalNetworking).toBe(true);
  });

  test("a test build keeps local networking and nothing more", () => {
    expect(withTransportSecurity(template, true).NSAppTransportSecurity).toEqual({
      NSAllowsArbitraryLoads: false,
      NSAllowsLocalNetworking: true,
    });
  });

  test("a plist without the entry gets one", () => {
    expect(withTransportSecurity({}, false)).toEqual({
      NSAppTransportSecurity: { NSAllowsArbitraryLoads: false },
    });
  });
});
