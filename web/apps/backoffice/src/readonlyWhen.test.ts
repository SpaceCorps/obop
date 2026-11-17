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
});
