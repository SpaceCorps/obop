import { describe, expect, test } from "vite-plus/test";
import type { Trip } from "../core/types.ts";
import { lastEndedTrip, tripDurationMs, whenText } from "./home.ts";

const STOCKHOLM = "Europe/Stockholm";
const at = (iso: string) => Date.parse(iso);
const WORDS = { today: "Today", yesterday: "Yesterday" };

let next = 0;
const trip = (
  startIso: string,
  state: Trip["state"],
  source: Trip["source"] = "recorded",
): Trip => {
  const startedAt = at(startIso);
  return {
    id: `t${(next += 1)}`,
    state,
    source,
    startedAt,
    endedAt: state === "ended" ? startedAt + 600_000 : null,
    distanceM: state === "ended" ? 1_000 : null,
    segment: 1,
    segmentStartedAt: startedAt,
    from: null,
    to: null,
    note: null,
  };
};

describe("the last trip", () => {
  test("is the newest one that has ended, whatever the order of the list", () => {
    const older = trip("2026-11-17T08:00:00Z", "ended");
    const newer = trip("2026-11-18T08:00:00Z", "ended", "manual");
    expect(lastEndedTrip([older, newer])).toBe(newer);
    expect(lastEndedTrip([newer, older])).toBe(newer);
  });

  test("a trip that records or is paused is not the last trip", () => {
    const ended = trip("2026-11-17T08:00:00Z", "ended");
    expect(
      lastEndedTrip([
        trip("2026-11-18T09:00:00Z", "recording"),
        ended,
        trip("2026-11-18T07:00:00Z", "paused"),
      ]),
    ).toBe(ended);
  });

  test("none before the first", () => {
    expect(lastEndedTrip([])).toBeNull();
    expect(lastEndedTrip([trip("2026-11-18T09:00:00Z", "recording")])).toBeNull();
  });
});

describe("how long the last trip took", () => {
  test("a recorded trip: its segments, without the pauses", () => {
    const recorded = trip("2026-11-18T08:00:00Z", "ended");
    expect(tripDurationMs(recorded, 7 * 60_000)).toBe(7 * 60_000);
  });

  test("a recorded trip whose segments are not read yet has no duration yet", () => {
    expect(tripDurationMs(trip("2026-11-18T08:00:00Z", "ended"), null)).toBeNull();
  });

  test("a trip added by hand: from its start to its end", () => {
    const manual = trip("2026-11-18T08:00:00Z", "ended", "manual");
    expect(tripDurationMs(manual, 0)).toBe(600_000);
    expect(tripDurationMs(manual, null)).toBe(600_000);
  });
});

describe("when it was", () => {
  const now = at("2026-11-18T12:00:00Z");

  test("today and yesterday by the phone's day, with the clock", () => {
    expect(whenText(at("2026-11-18T13:05:00Z"), now, "en", STOCKHOLM, WORDS)).toBe("Today 14:05");
    expect(whenText(at("2026-11-17T07:30:00Z"), now, "en", STOCKHOLM, WORDS)).toBe(
      "Yesterday 08:30",
    );
  });

  test("an older day without its year", () => {
    expect(whenText(at("2026-11-10T07:30:00Z"), now, "en", STOCKHOLM, WORDS)).toBe("Nov 10 08:30");
  });

  test("half past midnight in Stockholm is today there, though yesterday in UTC", () => {
    expect(whenText(at("2026-11-17T23:30:00Z"), now, "en", STOCKHOLM, WORDS)).toBe("Today 00:30");
    expect(whenText(at("2026-11-17T23:30:00Z"), now, "en", "UTC", WORDS)).toBe("Yesterday 23:30");
  });

  test("yesterday across the start of a month", () => {
    const first = at("2026-12-01T10:00:00Z");
    expect(whenText(at("2026-11-30T10:00:00Z"), first, "en", STOCKHOLM, WORDS)).toBe(
      "Yesterday 11:00",
    );
  });
});
