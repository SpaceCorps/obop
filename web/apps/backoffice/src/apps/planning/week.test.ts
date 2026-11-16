import { describe, expect, it } from "vite-plus/test";
import { addDays, mondayOf, weekDays } from "./week.ts";

describe("week", () => {
  it("finds the Monday of a week", () => {
    expect(mondayOf("2026-11-16")).toBe("2026-11-16");
    expect(mondayOf("2026-11-22")).toBe("2026-11-16");
    expect(mondayOf("2026-03-01")).toBe("2026-02-23");
  });
  it("lists seven days across a month and a change of summer time", () => {
    expect(weekDays("2026-10-26")).toEqual([
      "2026-10-26",
      "2026-10-27",
      "2026-10-28",
      "2026-10-29",
      "2026-10-30",
      "2026-10-31",
      "2026-11-01",
    ]);
    expect(addDays("2026-03-29", -7)).toBe("2026-03-22");
  });
});
