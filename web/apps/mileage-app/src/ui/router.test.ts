import { describe, expect, test } from "vite-plus/test";
import { reduce, type Route } from "./router.ts";

const home: Route = { name: "home" };

describe("the screen stack", () => {
  test("push, replace and back", () => {
    let stack: Route[] = [home];
    stack = reduce(stack, { type: "push", route: { name: "trips" } });
    stack = reduce(stack, { type: "push", route: { name: "trip", id: "c1" } });
    expect(stack.map((r) => r.name)).toEqual(["home", "trips", "trip"]);
    stack = reduce(stack, { type: "replace", route: { name: "account" } });
    expect(stack.map((r) => r.name)).toEqual(["home", "trips", "account"]);
    stack = reduce(stack, { type: "back" });
    expect(stack.map((r) => r.name)).toEqual(["home", "trips"]);
  });

  test("back on the first screen stays", () => {
    expect(reduce([home], { type: "back" })).toEqual([home]);
  });

  test("home clears the stack", () => {
    const stack = reduce([home, { name: "trips" }, { name: "recording" }], { type: "home" });
    expect(stack).toEqual([home]);
  });

  test("a stack is never changed in place", () => {
    const before: Route[] = [home];
    reduce(before, { type: "push", route: { name: "trips" } });
    expect(before).toEqual([home]);
  });
});
