import { describe, expect, test } from "vite-plus/test";
import { en } from "../i18n/en.ts";
import { t } from "../i18n/index.ts";
import {
  canDiscard,
  canSubmit,
  formatDistance,
  formatMetres,
  signalBannerProps,
  tripStatus,
} from "./present.ts";
import type { Progress } from "./progress.ts";
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
  const weakMessage =
    "Positions that are too imprecise are left out of the distance. If you are indoors, move to a window or go outside.";
  const jumpyTitle = "The positions do not fit together";
  const jumpyMessage =
    "Positions that do not fit the first position that was counted are left out of the distance. If the distance stays at 0 m, stop this trip, discard it and start a new one.";

  /** [name, what the screen measured, title, message, testID, detail] */
  const cases: [string, SignalNote, string, string, string, string][] = [
    [
      "weak, with the radius of the newest position",
      { kind: "weak", dropped: 7, stored: 7, accuracyM: 150 },
      weakTitle,
      weakMessage,
      "weak-signal",
      "7 of 7 positions were left out (the last one was only known to within 150 m)",
    ],
    [
      "weak, the radius not known",
      { kind: "weak", dropped: 3, stored: 9, accuracyM: null },
      weakTitle,
      weakMessage,
      "weak-signal",
      "3 of 9 positions were left out",
    ],
    [
      "weak, one position",
      { kind: "weak", dropped: 1, stored: 3, accuracyM: null },
      weakTitle,
      weakMessage,
      "weak-signal",
      "1 of 3 positions was left out",
    ],
    [
      "weak, one position and a radius",
      { kind: "weak", dropped: 1, stored: 4, accuracyM: 1235 },
      weakTitle,
      weakMessage,
      "weak-signal",
      "1 of 4 positions was left out (the last one was only known to within 1,235 m)",
    ],
    [
      "weak, a full trip",
      { kind: "weak", dropped: 12345, stored: 20000, accuracyM: 150 },
      weakTitle,
      weakMessage,
      "weak-signal",
      "12,345 of 20,000 positions were left out (the last one was only known to within 150 m)",
    ],
    [
      "not fitting",
      { kind: "jumpy", dropped: 8, stored: 9 },
      jumpyTitle,
      jumpyMessage,
      "jumpy-signal",
      "8 of 9 positions were left out",
    ],
    [
      "not fitting, one position",
      { kind: "jumpy", dropped: 1, stored: 3 },
      jumpyTitle,
      jumpyMessage,
      "jumpy-signal",
      "1 of 3 positions was left out",
    ],
    [
      "not fitting, a full trip",
      { kind: "jumpy", dropped: 19999, stored: 20000 },
      jumpyTitle,
      jumpyMessage,
      "jumpy-signal",
      "19,999 of 20,000 positions were left out",
    ],
  ];

  test.each(cases)("%s", (_name, signal, title, message, testID, detail) => {
    expect(signalBannerProps({ signal })).toEqual({
      tone: "warning",
      title,
      message,
      testID,
      detail,
    });
  });

  test("nothing before the stored positions have been read, and nothing when there is nothing to say", () => {
    expect(signalBannerProps(null)).toBeNull();
    expect(signalBannerProps({ signal: null })).toBeNull();
  });

  test("a whole count of the trip is read as it is (the screen passes what progressFrom made)", () => {
    const measured: Progress = {
      points: 7,
      metres: 0,
      lastFixMs: 1,
      signal: { kind: "weak", dropped: 7, stored: 7, accuracyM: null },
    };
    expect(signalBannerProps(measured)).toMatchObject({ testID: "weak-signal" });
    expect(signalBannerProps({ ...measured, signal: null })).toBeNull();
  });

  test("the banner is always a warning", () => {
    for (const [, signal] of cases) expect(signalBannerProps({ signal })!.tone).toBe("warning");
  });

  test("the words of the banner do not change with the counts or the radius", () => {
    // A screen reader reads the banner again whenever its text changes; the counts change with
    // nearly every stored position, so they are in `detail` and nowhere else.
    for (const kind of ["weak", "jumpy"] as const) {
      const banners = cases
        .filter(([, signal]) => signal.kind === kind)
        .map(([, signal]) => signalBannerProps({ signal })!);
      expect(banners.length).toBeGreaterThan(2);
      const { title, message, testID } = banners[0]!;
      for (const banner of banners) {
        expect({ title: banner.title, message: banner.message, testID: banner.testID }).toEqual({
          title,
          message,
          testID,
        });
      }
    }
  });

  test("the banner holds no number, and the detail line holds the counts", () => {
    for (const [, signal] of cases) {
      const props = signalBannerProps({ signal })!;
      // "0 m" is the one number the jumpy advice names, and it is not a count of the trip.
      expect(props.title).not.toMatch(/\d/);
      expect(props.message.replace("0 m", "")).not.toMatch(/\d/);
      expect(props.detail).toContain(` of ${signal.stored.toLocaleString("en-US")} positions`);
    }
  });

  test("a radius is only in the detail line, and only for a weak note that has one", () => {
    for (const [, signal] of cases) {
      const props = signalBannerProps({ signal })!;
      const named = signal.kind === "weak" && signal.accuracyM !== null;
      expect(props.detail.includes("known to within")).toBe(named);
      expect(`${props.title} ${props.message}`).not.toMatch(/within|\bm\b off/);
    }
  });

  test("no placeholder is left, and none of the words names a satellite system or the product", () => {
    for (const [, signal] of cases) {
      const { title, message, detail } = signalBannerProps({ signal })!;
      for (const text of [title, message, detail]) {
        expect(text).not.toMatch(/\{\w+\}/);
        expect(text).not.toMatch(/GPS|GNSS|satellite|obop/i);
      }
    }
  });

  test("the not-fitting advice says nothing about what caused it", () => {
    const { title, message } = signalBannerProps({
      signal: { kind: "jumpy", dropped: 8, stored: 9 },
    })!;
    expect(`${title} ${message}`).not.toMatch(/jump|keep recording|wrong|caus|because/i);
    expect(message).toContain("stop this trip, discard it and start a new one");
  });

  test("the weak advice is for someone who is indoors, not for everyone", () => {
    const { message } = signalBannerProps({
      signal: { kind: "weak", dropped: 3, stored: 4, accuracyM: null },
    })!;
    expect(message).toContain("If you are indoors, move to a window or go outside.");
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
