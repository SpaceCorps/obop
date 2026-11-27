import type { Health, Readiness } from "../core/liveness.ts";
import type { SignalNote } from "../core/signal.ts";
import type { Trip } from "../core/types.ts";

/**
 * What the home screen shows of a recording (D4): the buttons for the state of the trip, the one
 * warning that matters most, and the elapsed time. Pure, so it is tested; `screens/Home.tsx`
 * draws it.
 */

export type MainButton = "start" | "stop" | "resume";
export type SideButton = "pause" | "stop";

/**
 * The big button and the small one at its left: Start without a trip; a red Stop with Pause while
 * it records; Resume with Stop while it is paused, so a paused trip can be ended too.
 */
export function buttonsFor(state: Trip["state"] | null): {
  main: MainButton;
  side: SideButton | null;
} {
  if (state === "recording") return { main: "stop", side: "pause" };
  if (state === "paused") return { main: "resume", side: "stop" };
  return { main: "start", side: null };
}

export type Warning =
  /** Stop, Pause or Resume failed for a reason the driver cannot fix (the store, the system). */
  | { kind: "action_failed" }
  /** Restart recording failed. */
  | { kind: "restart_failed" }
  /** The phone cannot record now: the check of C5 found it, the way on is the permissions. */
  | { kind: "blocked"; reason: Exclude<Readiness, "ready"> }
  /** The updates are not registered with the system: the trip records nothing. */
  | { kind: "stuck" }
  /** A minute into the segment and no position at all. */
  | { kind: "no_fix" }
  /** The last position is `minutes` old. */
  | { kind: "quiet"; minutes: number }
  /** Why the distance stays at 0 km (`core/signal.ts`). */
  | { kind: "signal"; note: SignalNote };

/**
 * The one warning shown, the most important first: a failed action, a failed restart, a phone that
 * cannot record, updates that are not registered, no position at all, no new position, and why
 * the distance stays at 0 km. A paused trip records nothing on purpose: only a failed action is
 * said then. Nothing without an open trip.
 */
export function warningOf(input: {
  state: Trip["state"] | null;
  actionFailed: boolean;
  restartFailed: boolean;
  blocked: Exclude<Readiness, "ready"> | null;
  /** Whether the updates are registered; `null` before the system was asked. */
  registered: boolean | null;
  health: Health;
  signal: SignalNote | null;
}): Warning | null {
  const { state } = input;
  if (state === null || state === "ended") return null;
  if (input.actionFailed) return { kind: "action_failed" };
  if (state === "paused") return null;
  if (input.restartFailed) return { kind: "restart_failed" };
  if (input.blocked !== null) return { kind: "blocked", reason: input.blocked };
  if (input.registered === false) return { kind: "stuck" };
  if (input.health.kind === "no_fix") return { kind: "no_fix" };
  if (input.health.kind === "quiet") return { kind: "quiet", minutes: input.health.minutes };
  if (input.signal !== null) return { kind: "signal", note: input.signal };
  return null;
}

/**
 * The time of an open trip, without its pauses: the segments that ended (`closedMs`, from the
 * store) plus, while it records, the time since its open segment started. While it is paused the
 * clock stands. A clock set back adds nothing.
 */
export function elapsedMs(input: {
  state: Trip["state"];
  closedMs: number;
  segmentStartedAt: number;
  now: number;
}): number {
  const open = input.state === "recording" ? Math.max(0, input.now - input.segmentStartedAt) : 0;
  return input.closedMs + open;
}

/** A time as `H:MM:SS`; a negative one shows `0:00:00`. */
export function formatElapsed(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const seconds = total % 60;
  const two = (n: number) => String(n).padStart(2, "0");
  return `${hours}:${two(minutes)}:${two(seconds)}`;
}

/** How long the updates must be found not registered before "Recording has stopped" is said. */
export const STUCK_AFTER_MS = 15_000;

/**
 * One answer of the system to "are the updates registered?" (asked every 5 s while a trip
 * records), settled: `false` is said only when every answer since `falseSince` was `false` for at
 * least `STUCK_AFTER_MS`, so a short `false` is never shown (right after Stop, before the list of
 * trips is read again, the trip still looks recording while its updates were stopped already).
 * Until then the answer is `null` (not known yet). A `true` forgets the `false` before it.
 */
export function settleRegistered(
  answer: boolean,
  falseSince: number | null,
  now: number,
): { shown: boolean | null; falseSince: number | null } {
  if (answer) return { shown: true, falseSince: null };
  const since = falseSince ?? now;
  return { shown: now - since >= STUCK_AFTER_MS ? false : null, falseSince: since };
}
