/**
 * Whether a trip that is open is really being recorded. The task being registered with the system
 * (what `hasStartedLocationUpdatesAsync` answers) is not proof: the foreground service may not run
 * after a restart of the app, location may be switched off, a phone may have no Google Play services.
 * The one proof is that positions arrive, so the screen watches the age of the last one.
 */

/** No position at all this long after Start: something is wrong (location off, no Play services, no permission). */
export const NO_FIX_MS = 60_000;

/**
 * No new position this long after the last one. The phone asks for a position every 5 m, so a
 * standing car sends none for a while: the notice is soft and says so.
 */
export const QUIET_MS = 3 * 60_000;

export type Health =
  /** Positions arrive. */
  | { kind: "ok" }
  /** Just started: the first position has not come yet, which is normal. */
  | { kind: "waiting" }
  /** A minute and still no position. */
  | { kind: "no_fix" }
  /** The last position is old; `minutes` whole minutes. */
  | { kind: "quiet"; minutes: number };

export function recordingHealth(input: {
  startedAtMs: number;
  /**
   * When the newest stored position was taken, `null` when there is none, and `undefined` while
   * the stored positions have not been read yet (opening a trip that is older than a minute must
   * not say "no position arrived" before the phone has looked).
   */
  lastFixMs: number | null | undefined;
  nowMs: number;
}): Health {
  const { startedAtMs, lastFixMs, nowMs } = input;
  if (lastFixMs === undefined) return { kind: "waiting" };
  if (lastFixMs === null) {
    return nowMs - startedAtMs >= NO_FIX_MS ? { kind: "no_fix" } : { kind: "waiting" };
  }
  const age = nowMs - lastFixMs;
  return age >= QUIET_MS ? { kind: "quiet", minutes: Math.floor(age / 60_000) } : { kind: "ok" };
}

/** What stands between the driver and a recording, in the order the driver can fix it. */
export type Readiness = "ready" | "no_permission" | "approximate" | "services_off";

/**
 * Whether the phone can record a trip now. `accuracy` is what Android reports for the granted
 * permission (`fine` or `coarse`, the choice "Precise" or "Approximate" in the system dialog); it
 * is `undefined` where the platform has no such choice.
 */
export function readiness(input: {
  granted: boolean;
  accuracy: string | undefined;
  servicesEnabled: boolean;
}): Readiness {
  if (!input.granted) return "no_permission";
  if (input.accuracy !== undefined && input.accuracy !== "fine") return "approximate";
  return input.servicesEnabled ? "ready" : "services_off";
}
