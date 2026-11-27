import { describe, expect, test } from "vite-plus/test";
import {
  DETAIL_MAX,
  DetailsTooLong,
  cleanDetails,
  cleanText,
  codePoints,
  detailsProblems,
} from "./details.ts";

describe("the details a driver types", () => {
  test("are trimmed, and nothing or only spaces is no detail", () => {
    expect(cleanText("  Meeting  ")).toBe("Meeting");
    expect(cleanText("   ")).toBeNull();
    expect(cleanText("")).toBeNull();
    expect(cleanText(null)).toBeNull();
    expect(cleanText(undefined)).toBeNull();
  });

  test("all are optional: nothing typed is four nulls", () => {
    expect(cleanDetails({})).toEqual({ name: null, from: null, to: null, note: null });
  });

  test("what was typed is kept, trimmed", () => {
    expect(
      cleanDetails({ name: " Meeting ", from: "Stockholm", to: " Uppsala", note: "Samples\n" }),
    ).toEqual({ name: "Meeting", from: "Stockholm", to: "Uppsala", note: "Samples" });
  });

  test("a field over its limit is named, and the store refuses it", () => {
    const long = "x".repeat(DETAIL_MAX.name + 1);
    expect(detailsProblems({ name: long, to: "Uppsala" })).toEqual(["name"]);
    expect(() => cleanDetails({ name: long })).toThrow(DetailsTooLong);
  });

  test("the limit counts characters, not bytes: a name of Swedish letters at the limit is fine", () => {
    const name = "å".repeat(DETAIL_MAX.name);
    expect(detailsProblems({ name })).toEqual([]);
  });

  test("a character outside the basic plane (an emoji) counts once", () => {
    expect(codePoints("🚗")).toBe(1);
    expect(codePoints("Möte 🚗")).toBe(6);
    expect(detailsProblems({ name: "🚗".repeat(DETAIL_MAX.name) })).toEqual([]);
  });

  test("spaces around a text do not count for its length", () => {
    const name = ` ${"x".repeat(DETAIL_MAX.name)} `;
    expect(detailsProblems({ name })).toEqual([]);
  });
});
