import { describe, expect, test } from "vite-plus/test";
import {
  byDay,
  dayBefore,
  dayKey,
  durationParts,
  formatClock,
  formatKm,
  formatShortDate,
  monthKey,
  phoneTimeZone,
  totals,
} from "./present.ts";
import type { Trip } from "./types.ts";

const STOCKHOLM = "Europe/Stockholm";
const at = (iso: string) => Date.parse(iso);
const MIN = 60_000;

let next = 0;
const trip = (startIso: string, distanceM: number | null, minutes = 30): Trip => {
  const startedAt = at(startIso);
  return {
    id: `t${(next += 1)}`,
    state: distanceM === null ? "recording" : "ended",
    source: "recorded",
    startedAt,
    endedAt: distanceM === null ? null : startedAt + minutes * MIN,
    distanceM,
    segment: 1,
    segmentStartedAt: startedAt,
    name: null,
    from: null,
    to: null,
    note: null,
  };
};

describe("days and months in the phone's time zone", () => {
  test("half past midnight in Stockholm is that day, though it is still the day before in UTC", () => {
    // Winter: Stockholm is UTC+1.
    expect(dayKey(at("2026-11-17T23:30:00Z"), STOCKHOLM)).toBe("2026-11-18");
    expect(dayKey(at("2026-11-17T23:30:00Z"), "UTC")).toBe("2026-11-17");
    // Summer: Stockholm is UTC+2.
    expect(dayKey(at("2026-07-01T22:30:00Z"), STOCKHOLM)).toBe("2026-07-02");
  });

  test("the month follows the day", () => {
    expect(monthKey(at("2026-10-31T23:30:00Z"), STOCKHOLM)).toBe("2026-11");
    expect(monthKey(at("2026-10-31T23:30:00Z"), "UTC")).toBe("2026-10");
  });
});

describe("trips by day", () => {
  test("days newest first, trips in a day newest first, with the sum of each day", () => {
    const early = trip("2026-11-17T07:00:00Z", 10_000);
    const late = trip("2026-11-17T15:00:00Z", 5_000);
    const nextDay = trip("2026-11-18T08:00:00Z", 2_500);
    const days = byDay([early, nextDay, late], STOCKHOLM);
    expect(days.map((d) => d.day)).toEqual(["2026-11-18", "2026-11-17"]);
    expect(days[1]?.trips.map((t) => t.id)).toEqual([late.id, early.id]);
    expect(days.map((d) => d.distanceM)).toEqual([2_500, 15_000]);
  });

  test("a trip that goes past midnight belongs to the day it started", () => {
    // 23:40 to 00:20 in Stockholm.
    const night = trip("2026-11-17T22:40:00Z", 30_000, 40);
    expect(byDay([night], STOCKHOLM).map((d) => d.day)).toEqual(["2026-11-17"]);
  });

  test("a trip still recording counts 0 m in its day", () => {
    const days = byDay(
      [trip("2026-11-18T08:00:00Z", null), trip("2026-11-18T07:00:00Z", 4_000)],
      STOCKHOLM,
    );
    expect(days).toHaveLength(1);
    expect(days[0]?.distanceM).toBe(4_000);
  });

  test("no trips, no days", () => {
    expect(byDay([], STOCKHOLM)).toEqual([]);
  });
});

describe("the totals of today and this month", () => {
  const now = at("2026-11-18T10:00:00Z");

  test("today and this month in Stockholm, the first half hour of the month included", () => {
    const trips = [
      trip("2026-11-18T07:00:00Z", 1_000),
      trip("2026-11-18T09:00:00Z", 2_000),
      trip("2026-11-17T09:00:00Z", 5_000),
      // 00:30 on 1 November in Stockholm: this month, though still October in UTC.
      trip("2026-10-31T23:30:00Z", 7_000),
      trip("2026-10-31T21:00:00Z", 9_000),
      trip("2026-11-18T09:30:00Z", null),
    ];
    expect(totals(trips, now, STOCKHOLM)).toEqual({ todayM: 3_000, monthM: 15_000 });
  });

  test("nothing yet is 0 and 0", () => {
    expect(totals([], now, STOCKHOLM)).toEqual({ todayM: 0, monthM: 0 });
  });
});

describe("numbers and times for the screen", () => {
  test("kilometres with one decimal, in English and in Swedish", () => {
    expect(formatKm(12_403.7, "en")).toBe("12.4 km");
    expect(formatKm(12_403.7, "sv")).toBe("12,4 km");
    expect(formatKm(0, "en")).toBe("0.0 km");
    expect(formatKm(1_234_567, "en")).toBe("1,234.6 km");
  });

  test("the clock time in Stockholm, in winter and in summer, 24 hours", () => {
    expect(formatClock(at("2026-11-18T08:05:00Z"), "en", STOCKHOLM)).toBe("09:05");
    expect(formatClock(at("2026-07-01T07:05:00Z"), "en", STOCKHOLM)).toBe("09:05");
    expect(formatClock(at("2026-11-18T16:30:00Z"), "en", STOCKHOLM)).toBe("17:30");
    expect(formatClock(at("2026-11-18T16:30:00Z"), "sv", STOCKHOLM)).toBe("17:30");
  });

  test("a duration in hours and minutes, rounded to the minute", () => {
    const start = at("2026-11-18T08:00:00Z");
    expect(durationParts(start, start)).toEqual({ hours: 0, minutes: 0 });
    expect(durationParts(start, start + 42 * MIN)).toEqual({ hours: 0, minutes: 42 });
    expect(durationParts(start, start + 65 * MIN)).toEqual({ hours: 1, minutes: 5 });
    expect(durationParts(start, start + 29_000)).toEqual({ hours: 0, minutes: 0 });
    expect(durationParts(start, start + 31_000)).toEqual({ hours: 0, minutes: 1 });
  });

  test("a day without its year, in the time zone given", () => {
    expect(formatShortDate(at("2026-11-18T08:00:00Z"), "en", STOCKHOLM)).toBe("Nov 18");
    // 00:30 on 1 December in Stockholm, still November in UTC.
    expect(formatShortDate(at("2026-11-30T23:30:00Z"), "en", STOCKHOLM)).toBe("Dec 1");
  });

  test("the day before, across a month, a year and a leap day", () => {
    expect(dayBefore("2026-11-18")).toBe("2026-11-17");
    expect(dayBefore("2026-12-01")).toBe("2026-11-30");
    expect(dayBefore("2027-01-01")).toBe("2026-12-31");
    expect(dayBefore("2028-03-01")).toBe("2028-02-29");
  });

  test("the time zone of the phone is a name the formatter takes", () => {
    const zone = phoneTimeZone();
    expect(zone.length).toBeGreaterThan(0);
    expect(() => dayKey(Date.now(), zone)).not.toThrow();
  });

  test("an end before the start is no negative duration", () => {
    const start = at("2026-11-18T08:00:00Z");
    expect(durationParts(start, start - 5 * MIN)).toEqual({ hours: 0, minutes: 0 });
  });
});
