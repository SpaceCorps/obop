import { describe, expect, it } from "vite-plus/test";
import { compileCondition } from "./displayCondition.ts";
import fixtures from "./displayCondition.fixtures.json";

// The condition of `shows_option_of` is evaluated here, in the browser, and checked by the server's
// install gate in Rust. crates/kernel/obop-commands/src/shows_option_tests.rs runs these same
// fixtures through the server's parser, gate and evaluator; this file runs them through the browser's.
// A change to either side that makes them disagree fails the test of the other.

describe("a display condition", () => {
  for (const c of fixtures.cases) {
    it(c.name, () => {
      const cond = compileCondition(c.when);
      expect(cond, `${c.when} compiles`).not.toBeNull();
      expect(cond!(c.record)).toBe(c.expected);
    });
  }

  it("is refused where the install gate refuses it", () => {
    for (const src of fixtures.invalid)
      expect(compileCondition(src), JSON.stringify(src)).toBeNull();
  });

  it("never throws, whatever the record holds", () => {
    const cond = compileCondition('kind == "a" || is_set(n) && n > 1 || !done')!;
    for (const v of [null, undefined, 0, -1, 1.5, "", "a", true, false, [], {}, [1], { a: 1 }])
      expect(() => cond({ kind: v, n: v, done: v })).not.toThrow();
  });

  it("reads a field the record does not have as empty", () => {
    expect(compileCondition("n < 1")!({})).toBe(true);
    expect(compileCondition('kind == ""')!({})).toBe(true);
    expect(compileCondition("is_set(kind)")!({})).toBe(false);
  });

  it("can be used again on many records", () => {
    const cond = compileCondition('kind == "status"')!;
    expect([{ kind: "status" }, { kind: "note" }, { kind: "status" }].map(cond)).toEqual([
      true,
      false,
      true,
    ]);
  });
});
