import { describe, expect, test } from "vite-plus/test";
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

  test("is an object that cannot be changed", () => {
    expect(Object.isFrozen(startup)).toBe(true);
    expect(Object.isFrozen(startup.groups)).toBe(true);
    expect(Object.isFrozen(startup.legacyGroups)).toBe(true);
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
