import { describe, expect, test } from "vite-plus/test";
import { identity } from "./identity.ts";
import { iosBundleId } from "./ios.ts";

describe("the identity of the build", () => {
  test("the application id has the form of a bundle id and an Android package", () => {
    // `iosBundleId` throws on a value that is not in reverse domain form.
    expect(iosBundleId(identity.applicationId, "x.y")).toBe(identity.applicationId);
    // An Android package needs every part to start with a letter.
    for (const part of identity.applicationId.split(".")) {
      expect(part, part).toMatch(/^[A-Za-z]/);
    }
  });

  test("the version is major.minor.patch", () => {
    expect(identity.version).toMatch(/^\d+\.\d+\.\d+$/);
  });
});
