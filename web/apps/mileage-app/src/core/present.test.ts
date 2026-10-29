import { describe, expect, test } from "vite-plus/test";
import { en } from "../i18n/en.ts";
import { t } from "../i18n/index.ts";
import { canDiscard, canSubmit, formatDistance, formatMetres, tripStatus } from "./present.ts";
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
