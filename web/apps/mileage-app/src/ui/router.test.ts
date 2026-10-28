import { describe, expect, test } from "vite-plus/test";
import { gate, keepSigningIn, reduce, type Route } from "./router.ts";

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

describe("what the driver sees before any route", () => {
  const base = {
    status: "signed_in",
    foreign: false,
    openTrip: false,
    loaded: true,
    signingIn: false,
  } as const;

  test("a signed-in driver gets the app", () => {
    expect(gate(base)).toBe("app");
    expect(gate({ ...base, openTrip: true })).toBe("app");
  });

  test("an ended sign-in during a trip shows the recording, with Stop, not the sign-in", () => {
    expect(gate({ ...base, status: "expired", openTrip: true })).toBe("recording");
    expect(gate({ ...base, status: "signed_out", openTrip: true })).toBe("recording");
  });

  test("the driver can go on to the sign-in from the recording, and back", () => {
    expect(gate({ ...base, status: "expired", openTrip: true, signingIn: true })).toBe("sign_in");
  });

  test("the choice to sign in from the recording ends with the sign-in, so the next end shows the recording again", () => {
    // The driver chose "Sign in" on the recording screen, and the sign-in finished.
    let signingIn = true;
    signingIn = keepSigningIn(signingIn, "signed_in");
    expect(signingIn).toBe(false);
    // Later the sign-in ends again during a trip: the recording, with Stop, comes first.
    expect(gate({ ...base, status: "expired", openTrip: true, signingIn })).toBe("recording");
    // While the sign-in has not finished, the choice stands (it ends only by Back or by success).
    expect(keepSigningIn(true, "signed_out")).toBe(true);
    expect(keepSigningIn(true, "expired")).toBe(true);
    expect(keepSigningIn(false, "expired")).toBe(false);
  });

  test("an ended sign-in with no open trip shows the sign-in", () => {
    expect(gate({ ...base, status: "expired" })).toBe("sign_in");
    expect(gate({ ...base, status: "signed_out" })).toBe("sign_in");
  });

  test("the sign-in is not shown before the outbox says whether a trip is open", () => {
    expect(gate({ ...base, status: "expired", loaded: false })).toBe("loading");
  });

  test("trips of another sign-in block the app and nothing else", () => {
    expect(gate({ ...base, foreign: true })).toBe("foreign");
    expect(gate({ ...base, foreign: true, openTrip: true })).toBe("foreign");
    expect(gate({ ...base, status: "expired", foreign: true })).toBe("sign_in");
  });
});
