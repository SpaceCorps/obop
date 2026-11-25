import { describe, expect, test } from "vite-plus/test";
import {
  buttonsFor,
  elapsedMs,
  formatElapsed,
  settleRegistered,
  STUCK_AFTER_MS,
  warningOf,
} from "./recording-view.ts";

describe("the buttons of the home screen", () => {
  test("Start without a trip, and after one ended", () => {
    expect(buttonsFor(null)).toEqual({ main: "start", side: null });
    expect(buttonsFor("ended")).toEqual({ main: "start", side: null });
  });

  test("a red Stop with Pause at its left while recording", () => {
    expect(buttonsFor("recording")).toEqual({ main: "stop", side: "pause" });
  });

  test("Resume with Stop at its left while paused: a paused trip can be ended", () => {
    expect(buttonsFor("paused")).toEqual({ main: "resume", side: "stop" });
  });
});

describe("the one warning of a recording", () => {
  const calm = {
    state: "recording" as const,
    actionFailed: false,
    restartFailed: false,
    blocked: null,
    registered: true,
    health: { kind: "ok" as const },
    signal: null,
  };
  const weak = { kind: "weak" as const, dropped: 5, stored: 5, accuracyM: 250 };

  test("nothing while all is well, and nothing without an open trip", () => {
    expect(warningOf(calm)).toBeNull();
    expect(warningOf({ ...calm, state: null, actionFailed: true })).toBeNull();
    expect(warningOf({ ...calm, state: "ended", registered: false })).toBeNull();
  });

  test("each case on its own", () => {
    expect(warningOf({ ...calm, actionFailed: true })).toEqual({ kind: "action_failed" });
    expect(warningOf({ ...calm, restartFailed: true })).toEqual({ kind: "restart_failed" });
    expect(warningOf({ ...calm, blocked: "services_off" })).toEqual({
      kind: "blocked",
      reason: "services_off",
    });
    expect(warningOf({ ...calm, registered: false })).toEqual({ kind: "stuck" });
    expect(warningOf({ ...calm, health: { kind: "no_fix" } })).toEqual({ kind: "no_fix" });
    expect(warningOf({ ...calm, health: { kind: "quiet", minutes: 4 } })).toEqual({
      kind: "quiet",
      minutes: 4,
    });
    expect(warningOf({ ...calm, signal: weak })).toEqual({ kind: "signal", note: weak });
  });

  test("the most important first", () => {
    const all = {
      ...calm,
      actionFailed: true,
      restartFailed: true,
      blocked: "no_permission" as const,
      registered: false,
      health: { kind: "no_fix" as const },
      signal: weak,
    };
    expect(warningOf(all)?.kind).toBe("action_failed");
    expect(warningOf({ ...all, actionFailed: false })?.kind).toBe("restart_failed");
    expect(warningOf({ ...all, actionFailed: false, restartFailed: false })?.kind).toBe("blocked");
    expect(
      warningOf({ ...all, actionFailed: false, restartFailed: false, blocked: null })?.kind,
    ).toBe("stuck");
    expect(
      warningOf({
        ...all,
        actionFailed: false,
        restartFailed: false,
        blocked: null,
        registered: true,
      })?.kind,
    ).toBe("no_fix");
  });

  test("a paused trip records nothing on purpose: only a failed action is said", () => {
    const paused = { ...calm, state: "paused" as const, registered: false };
    expect(warningOf({ ...paused, health: { kind: "no_fix" } })).toBeNull();
    expect(warningOf({ ...paused, actionFailed: true })).toEqual({ kind: "action_failed" });
  });

  test("updates not asked yet are not taken for updates that stopped", () => {
    expect(warningOf({ ...calm, registered: null })).toBeNull();
  });
});

describe("the time of the open trip", () => {
  const MIN = 60_000;

  test("while it records: the segments that ended plus the open one", () => {
    expect(
      elapsedMs({ state: "recording", closedMs: 10 * MIN, segmentStartedAt: 0, now: 5 * MIN }),
    ).toBe(15 * MIN);
  });

  test("while it is paused the clock stands", () => {
    expect(
      elapsedMs({ state: "paused", closedMs: 10 * MIN, segmentStartedAt: 0, now: 99 * MIN }),
    ).toBe(10 * MIN);
  });

  test("a clock set back adds nothing", () => {
    expect(
      elapsedMs({ state: "recording", closedMs: MIN, segmentStartedAt: 5 * MIN, now: 4 * MIN }),
    ).toBe(MIN);
  });
});

describe("the elapsed time", () => {
  test("hours, minutes and seconds", () => {
    expect(formatElapsed(0)).toBe("0:00:00");
    expect(formatElapsed(65_000)).toBe("0:01:05");
    expect(formatElapsed((2 * 3600 + 3 * 60 + 4) * 1000 + 999)).toBe("2:03:04");
  });

  test("a clock set back is no negative time", () => {
    expect(formatElapsed(-5_000)).toBe("0:00:00");
  });
});

describe("Recording has stopped: only after the updates are gone a while", () => {
  const T = 1_000_000;

  test("registered: said at once, and a false before it is forgotten", () => {
    expect(settleRegistered(true, T - 60_000, T)).toEqual({ shown: true, falseSince: null });
  });

  test("a first false is not said: not known yet", () => {
    expect(settleRegistered(false, null, T)).toEqual({ shown: null, falseSince: T });
  });

  test("false answers for less than 15 s are not said (Stop, before the trips are read again)", () => {
    expect(settleRegistered(false, T, T + 10_000)).toEqual({ shown: null, falseSince: T });
  });

  test("false answers for 15 s and more are said", () => {
    expect(settleRegistered(false, T, T + STUCK_AFTER_MS)).toEqual({ shown: false, falseSince: T });
  });

  test("a true between false answers starts the wait again", () => {
    let settled = settleRegistered(false, null, T);
    settled = settleRegistered(true, settled.falseSince, T + 5_000);
    settled = settleRegistered(false, settled.falseSince, T + 10_000);
    expect(settleRegistered(false, settled.falseSince, T + 20_000).shown).toBeNull();
  });
});
