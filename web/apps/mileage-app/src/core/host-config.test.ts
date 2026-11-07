import { describe, expect, test } from "vite-plus/test";
import {
  applicationIdProblem,
  defineIdentity,
  groupsProblem,
  redirectProblem,
  type IdentityInput,
} from "./host-config.ts";

const INPUT: IdentityInput = {
  applicationId: "org.example.app",
  clientName: "Example",
  version: "1.2.3",
  locales: ["en", "sv"],
  storage: {
    databaseFile: "example.db",
    databaseKey: "example.database-key",
    server: "example.server",
    tokens: "example.tokens",
  },
};

describe("the application id", () => {
  test.each(["org.example.app", "se.example.app.test", "a.b", "org.example2.app3", "com.x1.y2z3"])(
    "%s is accepted",
    (id) => {
      expect(applicationIdProblem(id)).toBeNull();
    },
  );

  // Each of these builds as an iOS bundle id, or as the scheme the instance accepts, or both, and
  // still fails at the first sign-in or at the Android build. The id has to pass all three.
  test.each([
    ["Org.Example.App", "upper case"],
    ["org.example.My_App", "underscore and upper case"],
    ["org.example.my_app", "underscore (the instance refuses it in the scheme)"],
    ["org.example.my-app", "hyphen (the Android package refuses it)"],
    ["example", "one part"],
    ["org..app", "empty part"],
    ["org.example.", "dot at the end"],
    [".org.example", "dot at the start"],
    ["org.1example.app", "a part that starts with a digit (the Android package)"],
    ["1org.example.app", "the scheme starts with a digit"],
    ["org.example.é", "a letter outside a to z"],
    ["", "empty"],
    ["org.example.app ", "a space"],
    [`org.${"a".repeat(64)}`, "a part of 64 characters"],
    [`org.${"a".repeat(62)}.${"b".repeat(62)}.c`, "129 characters"],
  ])("%j is refused (%s)", (id) => {
    expect(applicationIdProblem(id)).not.toBeNull();
  });

  test("a part of 63 characters and an id of 128 are the longest", () => {
    expect(applicationIdProblem(`org.${"a".repeat(63)}`)).toBeNull();
    expect(applicationIdProblem(`${"a".repeat(63)}.${"b".repeat(62)}.c`)).toBeNull();
    expect(applicationIdProblem(`${"a".repeat(63)}.${"b".repeat(62)}.cd`)).not.toBeNull();
    expect(applicationIdProblem(`${"a".repeat(63)}.${"b".repeat(64)}`)).not.toBeNull();
  });
});

describe("the rule of the instance for a redirect, as written here", () => {
  // The same cases as the server's table of redirects for mobile clients (the test module
  // `mobile.rs` of its OAuth crate) that concern the private-use form.
  test.each([
    "org.obop.mileage:/oauth",
    "org.obop.mileage:/",
    "com.example.app-2:/cb/done",
    "se.example.app:/oauth",
  ])("%s is accepted", (uri) => {
    expect(redirectProblem(uri)).toBeNull();
  });

  test.each([
    "mileage:/oauth",
    "claude:/callback",
    "org.obop.mileage://host/oauth",
    "org.obop.mileage:oauth",
    "org.obop.mileage:",
    "org.obop.mileage:/oauth#frag",
    "org.obop.mileage:/oauth?x=1",
    "org.obop.mileage:/o*",
    "org.obop.mileage:/o auth",
    "org.obop.mileage:/o\\auth",
    "Org.Obop.Mileage:/oauth",
    "org..obop:/oauth",
    "-org.obop:/oauth",
    "org.obop-:/oauth",
    "1org.obop:/oauth",
    "no scheme at all",
  ])("%s is refused", (uri) => {
    expect(redirectProblem(uri)).not.toBeNull();
  });

  test("the length limits are the instance's: a scheme of 128 and a whole address of 256", () => {
    const scheme128 = `${"a".repeat(63)}.${"b".repeat(62)}.c`;
    expect(scheme128).toHaveLength(128);
    expect(redirectProblem(`${scheme128}:/oauth`)).toBeNull();
    expect(redirectProblem(`${scheme128}x:/oauth`)).not.toBeNull();
    expect(redirectProblem(`org.example:/${"p".repeat(256 - "org.example:/".length)}`)).toBeNull();
    expect(
      redirectProblem(`org.example:/${"p".repeat(257 - "org.example:/".length)}`),
    ).not.toBeNull();
  });
});

describe("declared groups", () => {
  test("a list of names is accepted, also the empty list", () => {
    expect(groupsProblem([])).toBeNull();
    expect(groupsProblem(["mileage.driver", "fleet.driver_2", "a-b.c"])).toBeNull();
  });

  test.each([
    [["mileage driver"], "a space"],
    [[""], "empty"],
    [["a".repeat(65)], "65 characters"],
    [["mileage.driver", "mileage.driver"], "twice"],
    [["x/y"], "a slash"],
  ])("%j is refused (%s)", (groups) => {
    expect(groupsProblem(groups)).not.toBeNull();
  });

  test("32 groups are the most", () => {
    const names = Array.from({ length: 33 }, (_, i) => `g.group${i}`);
    expect(groupsProblem(names.slice(0, 32))).toBeNull();
    expect(groupsProblem(names)).not.toBeNull();
  });
});

describe("defineIdentity", () => {
  test("derives the scheme and the redirect from the application id and keeps the rest", () => {
    const identity = defineIdentity(INPUT);
    expect(identity).toEqual({
      applicationId: "org.example.app",
      scheme: "org.example.app",
      redirectUri: "org.example.app:/oauth",
      clientName: "Example",
      defaultServer: "",
      serverLocked: false,
      version: "1.2.3",
      locales: ["en", "sv"],
      storage: INPUT.storage,
    });
  });

  test("hands out an object that cannot be changed, and a copy of what it was given", () => {
    const input = { ...INPUT, locales: ["en"], storage: { ...INPUT.storage } };
    const identity = defineIdentity(input);
    expect(Object.isFrozen(identity)).toBe(true);
    expect(Object.isFrozen(identity.storage)).toBe(true);
    expect(Object.isFrozen(identity.locales)).toBe(true);
    input.locales.push("sv");
    input.storage.databaseFile = "changed.db";
    expect(identity.locales).toEqual(["en"]);
    expect(identity.storage.databaseFile).toBe("example.db");
  });

  test("a locked server needs an address", () => {
    expect(() => defineIdentity({ ...INPUT, serverLocked: true })).toThrow(/locked server/);
    const locked = defineIdentity({
      ...INPUT,
      serverLocked: true,
      defaultServer: "https://example.test",
    });
    expect(locked.serverLocked).toBe(true);
    expect(locked.defaultServer).toBe("https://example.test");
  });

  test.each<[string, Partial<IdentityInput>, RegExp]>([
    ["an id the instance would refuse", { applicationId: "Org.Example.App" }, /application id/],
    ["a name that is empty", { clientName: "" }, /clientName/],
    ["a name of 81 characters", { clientName: "n".repeat(81) }, /clientName/],
    ["a name with a line break", { clientName: "a\nb" }, /clientName/],
    ["a name with a space at the end", { clientName: "Example " }, /clientName/],
    ["a version that is not x.y.z", { version: "1.2" }, /version/],
    ["no language", { locales: [] }, /locales/],
    ["a language that is not a tag", { locales: ["english"] }, /language tag/],
    ["a language twice", { locales: ["en", "en"] }, /twice/],
    [
      "a default server that is not an address",
      { defaultServer: "phone.example" },
      /defaultServer/,
    ],
    [
      "a database file with a path",
      { storage: { ...INPUT.storage, databaseFile: "../x.db" } },
      /databaseFile/,
    ],
    ["an empty item name", { storage: { ...INPUT.storage, tokens: "" } }, /tokens/],
    [
      "two names that are the same",
      { storage: { ...INPUT.storage, tokens: INPUT.storage.server } },
      /differ/,
    ],
  ])("refuses %s", (_, change, message) => {
    expect(() => defineIdentity({ ...INPUT, ...change })).toThrow(message);
  });
});
