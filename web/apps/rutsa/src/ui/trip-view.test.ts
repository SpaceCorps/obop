import { describe, expect, test } from "vite-plus/test";
import { tripDate, tripTimes } from "./trip-view.ts";

const STOCKHOLM = "Europe/Stockholm";
const at = (iso: string) => Date.parse(iso);
const NOW = at("2026-11-24T10:00:00Z");

describe("the date of a trip", () => {
  test("its weekday and day, without the year in this year", () => {
    expect(tripDate({ startedAt: at("2026-11-23T07:10:00Z") }, NOW, "en", STOCKHOLM)).toBe(
      "Mon, Nov 23",
    );
  });

  test("with the year for another year", () => {
    expect(tripDate({ startedAt: at("2025-11-23T07:10:00Z") }, NOW, "en", STOCKHOLM)).toBe(
      "Sun, Nov 23, 2025",
    );
  });

  test("the day of its start in the phone's time zone", () => {
    // 00:30 on 24 November in Stockholm, still the 23rd in UTC.
    expect(tripDate({ startedAt: at("2026-11-23T23:30:00Z") }, NOW, "en", STOCKHOLM)).toBe(
      "Tue, Nov 24",
    );
  });
});

describe("the times of a trip", () => {
  test("start and end of one day", () => {
    const trip = { startedAt: at("2026-11-23T07:10:00Z"), endedAt: at("2026-11-23T07:42:00Z") };
    expect(tripTimes(trip, "en", STOCKHOLM)).toBe("08:10–08:42");
  });

  test("an end on the next day says its day", () => {
    const trip = { startedAt: at("2026-11-20T22:40:00Z"), endedAt: at("2026-11-20T23:20:00Z") };
    expect(tripTimes(trip, "en", STOCKHOLM)).toBe("23:40–00:20 (Sat, Nov 21)");
  });

  test("an end in a new year says the year", () => {
    const trip = { startedAt: at("2026-12-31T22:40:00Z"), endedAt: at("2026-12-31T23:20:00Z") };
    expect(tripTimes(trip, "en", STOCKHOLM)).toBe("23:40–00:20 (Fri, Jan 1, 2027)");
  });

  test("an open trip has only its start", () => {
    expect(
      tripTimes({ startedAt: at("2026-11-23T07:10:00Z"), endedAt: null }, "en", STOCKHOLM),
    ).toBe("08:10");
  });
});
