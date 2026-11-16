import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { describe, expect, test } from "vite-plus/test";
import { applicationIdProblem, identity } from "./identity.ts";

// Everything here is something an installed app already has. The values are written out and not
// computed, so that a change of one shows as a change of this file. What each is bound to:
//   applicationId, scheme:  the Android package, the URL scheme of the app, the iOS bundle id
//   slug:                   the scheme of the development client (`exp+<slug>`)
//   version:                the user agent of the WebView, which the page reads
//   storage.server:         the Keychain or Keystore item with the address of the instance; another
//                           name and the app asks for the address again

describe("the identity of the host app", () => {
  test("is what an installed app has", () => {
    expect(identity).toEqual({
      applicationId: "org.obop.app.test",
      scheme: "org.obop.app.test",
      name: "obop",
      slug: "obop-host",
      version: "0.1.0",
      versionCode: 1,
      storage: { server: "obop.host.server" },
    });
  });

  test("cannot be changed while the app runs", () => {
    expect(Object.isFrozen(identity)).toBe(true);
    expect(Object.isFrozen(identity.storage)).toBe(true);
  });

  test("can be loaded by Node as it is, because Node reads it when the app is built", () => {
    // Node's own stripping of types: it loads erasable syntax only, so this fails on an enum, a
    // namespace, a parameter property, or an import that needs a build step.
    const url = new URL("./identity.ts", import.meta.url).href;
    const script = `const m = await import(${JSON.stringify(url)}); console.log(m.identity.applicationId);`;
    const result = spawnSync(process.execPath, ["--input-type=module", "-e", script], {
      encoding: "utf8",
    });
    expect(result.stderr, result.stderr).not.toMatch(/Error/);
    expect(result.status).toBe(0);
    expect(result.stdout.trim()).toBe("org.obop.app.test");
  });

  test("imports nothing", () => {
    const source = readFileSync(new URL("./identity.ts", import.meta.url), "utf8");
    expect(source).not.toMatch(/^\s*import\s/m);
    expect(source).not.toMatch(/\brequire\(/);
  });
});

describe("an application id", () => {
  test("is at least two parts of lower-case letters and digits, each starting with a letter", () => {
    for (const good of ["org.obop.app", "org.obop.app.test", "a.b", "com.example2.app9"]) {
      expect(applicationIdProblem(good), good).toBeNull();
    }
  });

  test("is refused in words when the package, the scheme or the bundle id would not take it", () => {
    for (const bad of [
      "obop",
      "org..obop",
      ".org.obop",
      "org.obop.",
      "Org.obop.app",
      "org.obop.my-app",
      "org.obop.my_app",
      "org.9obop.app",
      "org.obop.äpp",
      `org.${"a".repeat(64)}.app`,
      `${"a.".repeat(64)}b`,
    ]) {
      expect(applicationIdProblem(bad), bad).toEqual(expect.any(String));
    }
  });
});
