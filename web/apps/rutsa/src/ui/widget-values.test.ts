import { describe, expect, test } from "vite-plus/test";
import type { Trip } from "../core/types.ts";
import { widgetValues } from "./widget-values.ts";

const STOCKHOLM = "Europe/Stockholm";
const at = (iso: string) => Date.parse(iso);
const MIN = 60_000;
// 2026-11-25 12:00 in Stockholm.
const NOW = at("2026-11-25T11:00:00Z");

const trip = (
  over: Partial<Trip>,
): Pick<Trip, "state" | "source" | "startedAt" | "endedAt" | "distanceM" | "segmentStartedAt"> => ({
  state: "ended",
  source: "recorded",
  startedAt: at("2026-11-25T09:00:00Z"),
  endedAt: at("2026-11-25T09:45:00Z"),
  distanceM: 12_400,
  segmentStartedAt: at("2026-11-25T09:00:00Z"),
  ...over,
});

describe("what the current widget shows", () => {
  test("a trip that ended: its time without the pauses, its stored km, its end today", () => {
    const values = widgetValues(
      trip({}),
      { state: "ended", closedMs: 40 * MIN, distanceM: 1 },
      NOW,
      "en",
      STOCKHOLM,
    );
    // Ended 09:45 UTC, 10:45 in Stockholm: a trip that ended says when it ended.
    expect(values).toEqual({ elapsed: "0:40:00", km: "12.4 km", clock: "10:45" });
  });

  test("a trip added by hand: from its start to its end, with no count", () => {
    const values = widgetValues(trip({ source: "manual" }), null, NOW, "en", STOCKHOLM);
    expect(values.elapsed).toBe("0:45:00");
  });

  test("a trip that records: the time runs from its segment, the km are the count's", () => {
    const open = trip({
      state: "recording",
      endedAt: null,
      distanceM: null,
      segmentStartedAt: at("2026-11-25T10:50:00Z"),
    });
    const values = widgetValues(
      open,
      { state: "recording", closedMs: 5 * MIN, distanceM: 2_300 },
      NOW,
      "en",
      STOCKHOLM,
    );
    // 5 min closed and 10 min of the open segment.
    expect(values).toMatchObject({ elapsed: "0:15:00", km: "2.3 km" });
  });

  test("a paused trip stands; an open one not read yet shows 0", () => {
    const open = trip({ state: "paused", endedAt: null, distanceM: null });
    expect(
      widgetValues(
        open,
        { state: "paused", closedMs: 7 * MIN, distanceM: 900 },
        NOW,
        "en",
        STOCKHOLM,
      ).elapsed,
    ).toBe("0:07:00");
    expect(widgetValues(open, null, NOW, "en", STOCKHOLM)).toMatchObject({
      elapsed: "0:00:00",
      km: "0.0 km",
    });
  });

  test("an open trip says when it started, the clock time only, also from yesterday", () => {
    const open = trip({
      state: "recording",
      startedAt: at("2026-11-24T22:40:00Z"),
      endedAt: null,
      distanceM: null,
    });
    expect(widgetValues(open, null, NOW, "en", STOCKHOLM).clock).toBe("23:40");
  });

  test("a trip that ended before today says the clock time of its end, with no day", () => {
    // 23:40 on the 23rd to 00:20 on the 24th in Stockholm: it ended yesterday.
    const values = widgetValues(
      trip({ startedAt: at("2026-11-23T22:40:00Z"), endedAt: at("2026-11-23T23:20:00Z") }),
      null,
      NOW,
      "en",
      STOCKHOLM,
    );
    expect(values.clock).toBe("00:20");
  });
});
