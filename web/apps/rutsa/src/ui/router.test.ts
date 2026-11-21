import { describe, expect, test } from "vite-plus/test";
import { canGoBack, current, openTripFirst, reduce, type Route } from "./router.ts";

const home: Route = { name: "home" };
const names = (stack: readonly Route[]) => stack.map((r) => r.name);

describe("the screen stack", () => {
  test("push, replace and back", () => {
    let stack: Route[] = [home];
    stack = reduce(stack, { type: "push", route: { name: "trips" } });
    stack = reduce(stack, { type: "push", route: { name: "trip", id: "t1" } });
    expect(names(stack)).toEqual(["home", "trips", "trip"]);
    stack = reduce(stack, { type: "replace", route: { name: "add" } });
    expect(names(stack)).toEqual(["home", "trips", "add"]);
    stack = reduce(stack, { type: "back" });
    expect(names(stack)).toEqual(["home", "trips"]);
  });

  test("back on the first screen stays", () => {
    expect(reduce([home], { type: "back" })).toEqual([home]);
  });

  test("home clears the stack", () => {
    const stack = reduce([home, { name: "trips" }, { name: "recording" }], { type: "home" });
    expect(stack).toEqual([home]);
  });

  test("a replace on an empty stack makes it", () => {
    expect(reduce([], { type: "replace", route: { name: "trips" } })).toEqual([{ name: "trips" }]);
  });

  test("a stack is never changed in place", () => {
    const before: Route[] = [home];
    reduce(before, { type: "push", route: { name: "trips" } });
    reduce(before, { type: "replace", route: { name: "trips" } });
    expect(before).toEqual([home]);
  });

  test("the screen shown is the top of the stack, home when there is none", () => {
    expect(current([home, { name: "trip", id: "t1" }])).toEqual({ name: "trip", id: "t1" });
    expect(current([])).toEqual(home);
  });
});

describe("the back button of Android", () => {
  test("goes back while there is a screen behind, and leaves the app on the first", () => {
    expect(canGoBack([home, { name: "trips" }])).toBe(true);
    expect(canGoBack([home])).toBe(false);
  });
});

describe("without location", () => {
  test("Close on the permissions goes back to where the driver was, the trips included", () => {
    let stack: Route[] = [home, { name: "trips" }];
    stack = reduce(stack, { type: "push", route: { name: "permissions", next: "start" } });
    stack = reduce(stack, { type: "back" });
    expect(names(stack)).toEqual(["home", "trips"]);
  });
});

describe("a trip that is open when the app starts", () => {
  test("is shown first, on the screen of the recording", () => {
    expect(names(openTripFirst([home], true))).toEqual(["home", "recording"]);
  });

  test("no open trip leaves the stack as it is", () => {
    expect(openTripFirst([home], false)).toEqual([home]);
  });

  test("the screen of the recording is not shown twice", () => {
    const stack: Route[] = [home, { name: "recording" }];
    expect(openTripFirst(stack, true)).toEqual(stack);
  });
});
