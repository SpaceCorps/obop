import { describe, expect, test } from "vite-plus/test";
import { backPress, canGoBack, current, reduce, TABS, type Route } from "./router.ts";

const home: Route = { name: "home" };
const names = (stack: readonly Route[]) => stack.map((r) => r.name);

describe("the screen stack", () => {
  test("push, replace and back", () => {
    let stack: Route[] = [home];
    stack = reduce(stack, { type: "push", route: { name: "trip", id: "t1" } });
    stack = reduce(stack, { type: "push", route: { name: "permissions", next: "start" } });
    expect(names(stack)).toEqual(["home", "trip", "permissions"]);
    stack = reduce(stack, { type: "replace", route: { name: "add" } });
    expect(names(stack)).toEqual(["home", "trip", "add"]);
    stack = reduce(stack, { type: "back" });
    expect(names(stack)).toEqual(["home", "trip"]);
  });

  test("back on the first screen stays", () => {
    expect(reduce([home], { type: "back" })).toEqual([home]);
  });

  test("home clears the stack", () => {
    const stack = reduce([home, { name: "add" }, { name: "trip", id: "t1" }], { type: "home" });
    expect(stack).toEqual([home]);
  });

  test("a replace on an empty stack makes it", () => {
    expect(reduce([], { type: "replace", route: { name: "add" } })).toEqual([{ name: "add" }]);
  });

  test("a stack is never changed in place", () => {
    const before: Route[] = [home];
    reduce(before, { type: "push", route: { name: "add" } });
    reduce(before, { type: "replace", route: { name: "add" } });
    expect(before).toEqual([home]);
  });

  test("the screen shown is the top of the stack, home when there is none", () => {
    expect(current([home, { name: "trip", id: "t1" }])).toEqual({ name: "trip", id: "t1" });
    expect(current([])).toEqual(home);
  });
});

describe("the back button of Android", () => {
  test("goes back while there is a screen behind, and leaves the app on the first", () => {
    expect(canGoBack([home, { name: "add" }])).toBe(true);
    expect(canGoBack([home])).toBe(false);
  });

  test("a screen over the tabs goes back, Profile and History go to Record, Record leaves", () => {
    expect(backPress([home, { name: "trip", id: "t1" }], "history")).toBe("back");
    expect(backPress([home], "history")).toBe("record");
    expect(backPress([home], "profile")).toBe("record");
    expect(backPress([home], "record")).toBe("leave");
  });
});

describe("the switcher", () => {
  test("History, Record and Profile, in that order", () => {
    expect(TABS).toEqual(["history", "record", "profile"]);
  });
});

describe("without location", () => {
  test("Close on the permissions goes back to where the driver was", () => {
    let stack: Route[] = [home, { name: "trip", id: "t1" }];
    stack = reduce(stack, { type: "push", route: { name: "permissions", next: "start" } });
    stack = reduce(stack, { type: "back" });
    expect(names(stack)).toEqual(["home", "trip"]);
  });
});
