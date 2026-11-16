// reminders.ts: how close a licence expiry or a planned service is.
import { describe, expect, it } from "vite-plus/test";
import { addDays, daysUntil, urgencyOf } from "./reminders.ts";

describe("fleet reminders", () => {
  it("counts whole days across a month end", () => {
    expect(addDays("2026-11-16", 30)).toBe("2026-12-16");
    expect(daysUntil("2026-12-01", "2026-11-16")).toBe(15);
    expect(daysUntil("2026-10-31", "2026-11-16")).toBe(-16);
  });

  it("reads a date as past, today, soon or later", () => {
    expect(urgencyOf("2026-10-31", "2026-11-16", 30)).toBe("past");
    expect(urgencyOf("2026-11-16", "2026-11-16", 30)).toBe("today");
    expect(urgencyOf("2026-12-16", "2026-11-16", 30)).toBe("soon");
    expect(urgencyOf("2026-12-17", "2026-11-16", 30)).toBe("later");
  });
});
