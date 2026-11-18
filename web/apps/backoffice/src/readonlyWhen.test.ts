import { describe, expect, it } from "vite-plus/test";
import type { FieldMeta } from "./api.ts";
import { readonlyByState } from "./readonlyWhen.ts";

const field = (readonly_when?: string): FieldMeta =>
  ({
    name: "vehicle",
    kind: "text",
    ...(readonly_when ? { ui: { readonly_when } } : {}),
  }) as unknown as FieldMeta;

describe("readonlyByState", () => {
  const locked = field('state == "in_review" || state == "confirmed"');
  it("holds on a saved record whose state matches", () => {
    expect(readonlyByState(locked, { id: "t", state: "in_review" })).toBe(true);
    expect(readonlyByState(locked, { id: "t", state: "confirmed" })).toBe(true);
    expect(readonlyByState(locked, { id: "t", state: "ended" })).toBe(false);
  });
  it("never holds on a new record or without the hint", () => {
    expect(readonlyByState(locked, null)).toBe(false);
    expect(readonlyByState(field(), { id: "t", state: "in_review" })).toBe(false);
  });
  it("is always on for true and off for a condition the browser cannot read", () => {
    expect(readonlyByState(field("true"), { id: "t" })).toBe(true);
    expect(readonlyByState(field("total * 2 > 3"), { id: "t", total: 9 })).toBe(false);
  });
  it("reads $user, the person viewing the record (M2)", () => {
    const drivers = field('driver != $user || state == "in_review"');
    const ended = { id: "t", state: "ended", driver: "ann" };
    expect(readonlyByState(drivers, ended, "ann")).toBe(false);
    expect(readonlyByState(drivers, ended, "root")).toBe(true);
    expect(readonlyByState(drivers, { ...ended, state: "in_review" }, "ann")).toBe(true);
    // The signed-in login is the default.
    sessionStorage.setItem("obop-user", "ann");
    expect(readonlyByState(drivers, ended)).toBe(false);
    sessionStorage.setItem("obop-user", "rob");
    expect(readonlyByState(drivers, ended)).toBe(true);
    sessionStorage.removeItem("obop-user");
    // `$users` is not `$user`: a condition this browser cannot read.
    expect(readonlyByState(field('$users == "ann"'), ended, "ann")).toBe(false);
  });
});

describe("$user outside readonly_when", () => {
  it("is not read by any other condition", async () => {
    const { compileCondition } = await import("./displayCondition.ts");
    expect(compileCondition("driver == $user")).toBeNull();
    expect(compileCondition("driver == $user", { viewer: true })?.({ driver: "a" }, "a")).toBe(
      true,
    );
  });
});
