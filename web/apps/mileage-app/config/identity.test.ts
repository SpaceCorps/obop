import { spawnSync } from "node:child_process";
import { describe, expect, test } from "vite-plus/test";
import { directImports } from "../src/test-support/imports.ts";
import { identity } from "./identity.ts";

// Everything here is something an installed app, or the instance it signed in to, already has. The
// values are written out and not computed, so that a change of one shows as a change of this file.
// What each is bound to:
//   applicationId, scheme:  the URL scheme the system hands the sign-in back to; the Android package
//   redirectUri:            the instance binds the client it registered to this exact text, and a
//                           stored client id is used again for up to 30 days
//   clientName:             shown by the instance on its consent page
//   storage.databaseKey:    the Keychain item with the key of the database; lose it and the unsent
//                           trips cannot be read (and, on iOS, the file is replaced)
//   storage.server/tokens:  the items of the stored sign-in
//   storage.databaseFile:   the file of the outbox; another name opens an empty one

describe("the identity of the Mileage app", () => {
  test("is what an installed app has", () => {
    expect(identity).toEqual({
      applicationId: "org.obop.mileage",
      scheme: "org.obop.mileage",
      redirectUri: "org.obop.mileage:/oauth",
      clientName: "obop Mileage",
      defaultServer: "",
      serverLocked: false,
      version: "0.1.0",
      locales: ["en"],
      storage: {
        databaseFile: "mileage.db",
        databaseKey: "obop.mileage.database-key",
        server: "obop.mileage.server",
        tokens: "obop.mileage.tokens",
      },
    });
  });

  test("the redirect is the application id and a path", () => {
    expect(identity.redirectUri).toBe(`${identity.applicationId}:/oauth`);
    expect(identity.scheme).toBe(identity.applicationId);
  });

  test("can be loaded by Node as it is, because Node reads it when the app is built", () => {
    // Node's own stripping of types: it loads erasable syntax only, so this fails on an enum, a
    // namespace, a parameter property, or an import that needs a build step.
    const url = new URL("./identity.ts", import.meta.url).href;
    const script = `const m = await import(${JSON.stringify(url)}); console.log(m.identity.redirectUri);`;
    const result = spawnSync(process.execPath, ["--input-type=module", "-e", script], {
      encoding: "utf8",
    });
    expect(result.stderr, result.stderr).not.toMatch(/Error/);
    expect(result.status).toBe(0);
    expect(result.stdout.trim()).toBe("org.obop.mileage:/oauth");
  });

  test("imports nothing but the file that defines the shape, and that file imports nothing", () => {
    expect(directImports("config/identity.ts", true)).toEqual({
      files: ["src/core/host-config.ts"],
      packages: [],
    });
    expect(directImports("src/core/host-config.ts", true)).toEqual({ files: [], packages: [] });
  });
});
