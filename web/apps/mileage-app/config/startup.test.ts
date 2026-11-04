import { describe, expect, test } from "vite-plus/test";
import { HOST_SCHEMA } from "../src/core/host-state.ts";
import type { SchemaModule } from "../src/core/schema.ts";
import { MILEAGE_SCHEMA } from "../src/core/sql-store.ts";
import { defineStartup } from "../src/core/startup-config.ts";
import { identity } from "./identity.ts";
import { startup } from "./startup.ts";

describe("the startup configuration of the Mileage app", () => {
  test("holds the identity, the groups it asks for, and the groups of every earlier build", () => {
    expect(startup.identity).toBe(identity);
    // What the instance has seen from every build so far: written out, not computed.
    expect(startup.groups).toEqual(["mileage.driver"]);
    expect(startup.legacyGroups).toEqual(["mileage.driver"]);
  });

  test("opens the file with the host's module and then the app's own", () => {
    expect(startup.modules).toEqual([HOST_SCHEMA, MILEAGE_SCHEMA]);
    expect(startup.modules.map((m) => [m.id, m.version])).toEqual([
      ["host", 1],
      ["mileage", 1],
    ]);
  });

  test("is an object that cannot be changed", () => {
    expect(Object.isFrozen(startup)).toBe(true);
    expect(Object.isFrozen(startup.groups)).toBe(true);
    expect(Object.isFrozen(startup.legacyGroups)).toBe(true);
    expect(Object.isFrozen(startup.modules)).toBe(true);
  });
});

describe("the modules of defineStartup", () => {
  const groups = ["a.driver"];
  const own = (id: string, extra: Partial<SchemaModule> = {}): SchemaModule => ({
    id,
    version: 1,
    migrate: async () => undefined,
    ...extra,
  });

  test("an app that lists none gets the host's module alone, and the host's comes first for one that lists some", () => {
    expect(defineStartup({ identity, groups }).modules).toEqual([HOST_SCHEMA]);
    expect(
      defineStartup({ identity, groups, modules: [own("a", { requires: ["host"] })] }).modules.map(
        (m) => m.id,
      ),
    ).toEqual(["host", "a"]);
  });

  test("an app does not list the host's module itself", () => {
    expect(() => defineStartup({ identity, groups, modules: [HOST_SCHEMA] })).toThrow(
      /host is the host's own module/,
    );
  });

  test.each<[string, SchemaModule[], RegExp]>([
    ["a module twice", [own("a"), own("a")], /listed twice/],
    ["an id with a capital", [own("A")], /lower-case/],
    ["a module that needs one that is not listed", [own("a", { requires: ["b"] })], /needs b/],
    [
      "a table claimed that the host owns",
      [own("a", { legacyTables: ["meta"] })],
      /claimed by host/,
    ],
    [
      "a meta key claimed that the host owns",
      [own("a", { legacyMetaKeys: ["owner"] })],
      /meta key owner is claimed by host/,
    ],
  ])("refuses %s when the app starts", (_, modules, message) => {
    expect(() => defineStartup({ identity, groups, modules })).toThrow(
      new RegExp(`startup: modules: .*${message.source}`),
    );
  });
});

describe("defineStartup", () => {
  test("keeps copies of the lists it is given", () => {
    const groups = ["a.driver"];
    const config = defineStartup({ identity, groups });
    groups.push("b.driver");
    expect(config.groups).toEqual(["a.driver"]);
    expect(config.legacyGroups).toBeUndefined();
  });

  test.each([
    ["a group with a space", ["mileage driver"]],
    ["a group twice", ["mileage.driver", "mileage.driver"]],
    ["33 groups", Array.from({ length: 33 }, (_, i) => `g.group${i}`)],
  ])("refuses %s", (_, groups) => {
    expect(() => defineStartup({ identity, groups })).toThrow(/groups/);
    expect(() => defineStartup({ identity, groups: [], legacyGroups: groups })).toThrow(
      /legacyGroups/,
    );
  });
});
