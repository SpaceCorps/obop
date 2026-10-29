import { describe, expect, test } from "vite-plus/test";
import { iosBundleId, withoutFetchMode } from "./ios.ts";

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
