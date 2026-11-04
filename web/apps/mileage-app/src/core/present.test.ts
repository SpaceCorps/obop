import { describe, expect, test } from "vite-plus/test";
import { en } from "../i18n/en.ts";
import { t } from "../i18n/index.ts";
import {
  canDiscard,
  canSubmit,
  formatDistance,
  formatMetres,
  signalBanner,
  tripStatus,
} from "./present.ts";
import type { SignalNote } from "./signal.ts";
import type { LocalTrip } from "./types.ts";

const trip = (over: Partial<LocalTrip> = {}): LocalTrip => ({
  clientTripId: "c1",
  serverId: null,
  origin: "device",
  purpose: "business",
  startedAt: "2026-10-15T09:00:00.000Z",
  endedAt: null,
  createAttempted: false,
  stopRequested: false,
  expectedPoints: null,
  lastSeq: null,
  endDone: false,
  submitRequested: false,
  submitDone: false,
  discardReason: null,
  discardDone: false,
  serverState: null,
  distanceM: null,
  integrity: null,
  rejectReason: null,
  attempts: 0,
  problem: null,
  createdMs: 0,
  ...over,
});

describe("the status of a trip", () => {
  test("recording until Stop, then waiting or uploading, then the server's word", () => {
    expect(tripStatus(trip())).toBe("recording");
    expect(tripStatus(trip({ stopRequested: true }))).toBe("waiting");
    expect(tripStatus(trip({ stopRequested: true, serverId: "t-1" }))).toBe("uploading");
    expect(
      tripStatus(
        trip({ stopRequested: true, serverId: "t-1", endDone: true, serverState: "ended" }),
      ),
    ).toBe("ended");
    expect(
      tripStatus(
        trip({
          stopRequested: true,
          serverId: "t-1",
          endDone: true,
          submitRequested: true,
          submitDone: true,
          serverState: "in_review",
        }),
      ),
    ).toBe("in_review");
  });

  test("a refusal for good shows first, but a notice that the server is still waiting does not", () => {
    const problem = { step: "end", code: "bad_request", message: "no distance" } as const;
    expect(tripStatus(trip({ stopRequested: true, problem }))).toBe("problem");
    expect(
      tripStatus(
        trip({ stopRequested: true, serverId: "t-1", problem: { ...problem, code: "incomplete" } }),
      ),
    ).toBe("uploading");
  });

  test("a discarded trip is discarded", () => {
    expect(tripStatus(trip({ stopRequested: true, discardReason: "x", discardDone: true }))).toBe(
      "discarded",
    );
  });
});

describe("what the driver may do", () => {
  const ended = trip({
    stopRequested: true,
    serverId: "t-1",
    endDone: true,
    serverState: "ended",
  });

  test("submit an ended trip once", () => {
    expect(canSubmit(ended)).toBe(true);
    expect(canSubmit({ ...ended, submitRequested: true })).toBe(false);
    expect(canSubmit({ ...ended, serverState: "in_review", submitDone: true })).toBe(false);
    expect(canSubmit(trip())).toBe(false);
    expect(canSubmit({ ...ended, serverState: "rejected" })).toBe(false);
  });

  test("discard while the server allows it", () => {
    expect(canDiscard(trip())).toBe(true);
    expect(canDiscard(ended)).toBe(true);
    expect(canDiscard({ ...ended, serverState: "rejected" })).toBe(true);
    expect(canDiscard({ ...ended, serverState: "in_review" })).toBe(false);
    expect(canDiscard({ ...ended, serverState: "confirmed" })).toBe(false);
    expect(canDiscard({ ...ended, discardReason: "x" })).toBe(false);
    expect(canDiscard({ ...ended, submitRequested: true })).toBe(false);
  });

  test("a trip stopped by a refusal can be discarded, but not while the server's state is unknown", () => {
    const problem = (code: string) => ({
      ...ended,
      problem: { step: "end" as const, code, message: "x" },
    });
    expect(canDiscard(problem("gone"))).toBe(true);
    expect(canDiscard(problem("bad_request"))).toBe(true);
    // The server answered 404 and the phone cannot tell what that means: the points stay.
    expect(canDiscard(problem("unknown"))).toBe(false);
  });
});

describe("distances", () => {
  test("kilometres from a kilometre up, metres below", () => {
    expect(formatDistance("12345.67")).toBe("12.3 km");
    expect(formatDistance("999.4")).toBe("999 m");
    expect(formatDistance(null)).toBeNull();
    expect(formatDistance("x")).toBeNull();
    expect(formatMetres(0)).toBe("0 m");
    expect(formatMetres(2500)).toBe("2.5 km");
  });
});

describe("the note about a distance of 0 m", () => {
  const weakTitle = "The location signal is too weak to measure a distance";
  const jumpyTitle = "The positions keep jumping";
  const cases: [string, SignalNote, string, string][] = [
    [
      "weak, with the radius of the newest position",
      { kind: "weak", dropped: 7, stored: 7, accuracyM: 150 },
      weakTitle,
      "7 of 7 positions were too imprecise to count (the last one was 150 m off). Move to a window or go outside.",
    ],
    [
      "weak, the radius not known",
      { kind: "weak", dropped: 3, stored: 9, accuracyM: null },
      weakTitle,
      "3 of 9 positions were too imprecise to count. Move to a window or go outside.",
    ],
    [
      "weak, one position",
      { kind: "weak", dropped: 1, stored: 3, accuracyM: null },
      weakTitle,
      "1 of 3 positions was too imprecise to count. Move to a window or go outside.",
    ],
    [
      "weak, one position and a radius",
      { kind: "weak", dropped: 1, stored: 4, accuracyM: 1235 },
      weakTitle,
      "1 of 4 positions was too imprecise to count (the last one was 1,235 m off). Move to a window or go outside.",
    ],
    [
      "weak, a full trip",
      { kind: "weak", dropped: 12345, stored: 20000, accuracyM: 150 },
      weakTitle,
      "12,345 of 20,000 positions were too imprecise to count (the last one was 150 m off). Move to a window or go outside.",
    ],
    [
      "jumpy",
      { kind: "jumpy", dropped: 8, stored: 9 },
      jumpyTitle,
      "8 of 9 positions were dropped because they jumped too far, too fast. Keep recording; if the distance stays at 0 m the trip cannot be counted.",
    ],
    [
      "jumpy, one position",
      { kind: "jumpy", dropped: 1, stored: 3 },
      jumpyTitle,
      "1 of 3 positions was dropped because it jumped too far, too fast. Keep recording; if the distance stays at 0 m the trip cannot be counted.",
    ],
    [
      "jumpy, a full trip",
      { kind: "jumpy", dropped: 19999, stored: 20000 },
      jumpyTitle,
      "19,999 of 20,000 positions were dropped because they jumped too far, too fast. Keep recording; if the distance stays at 0 m the trip cannot be counted.",
    ],
  ];
  test.each(cases)("%s", (_name, note, title, message) => {
    expect(signalBanner(note)).toEqual({ title, message });
  });

  test("no placeholder is left, and none of the words names a satellite system", () => {
    for (const [, note] of cases) {
      const { title, message } = signalBanner(note);
      expect(`${title} ${message}`).not.toMatch(/{w+}/);
      expect(`${title} ${message}`).not.toMatch(/GPS|GNSS|satellite/i);
    }
  });
});

describe("the catalogue", () => {
  test("fills placeholders and leaves an unknown one visible", () => {
    expect(t("home.last", { distance: "1.2 km", state: "Ended" })).toBe("Last trip: 1.2 km, Ended");
    expect(t("home.last")).toBe("Last trip: {distance}, {state}");
  });

  test("every sentence is text, and the placeholders are plain words", () => {
    for (const [key, text] of Object.entries(en)) {
      expect(text.length, key).toBeGreaterThan(0);
      for (const match of text.matchAll(/\{([^}]*)\}/g)) expect(match[1], key).toMatch(/^\w+$/);
    }
  });
});
