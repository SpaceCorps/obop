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

  test("the storage names stay as an installed app has them", () => {
    // Changing one of these loses the trips of every installed app: the file is another one, or
    // the key is not found. They are written out here on purpose.
    expect(identity.storage).toEqual({
      databaseFile: "rutsa.db",
      databaseKey: "org.obop.rutsa.database-key",
    });
    // expo-secure-store takes only letters, digits, ".", "-" and "_" in a key name.
    expect(identity.storage.databaseKey).toMatch(/^[A-Za-z0-9._-]+$/);
  });
});
