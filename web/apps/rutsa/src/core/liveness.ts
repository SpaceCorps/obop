/**
 * Whether the phone can record a trip, and whether a trip that is recording really is. Pure, so it
 * runs in `vp test`; `native/permissions.ts` feeds it what the location service says, and the
 * screen of a recording feeds it the stored positions and the clock.
 *
 * After `mileage-app` (`src/core/liveness.ts`).
 */

/** No position at all this long after the start of the segment: something is wrong. */
export const NO_FIX_MS = 60_000;

/**
 * No new position this long after the last one. The phone asks for a position every 5 m, so a car
 * that stands sends none for a while: the notice is soft and says so.
 */
export const QUIET_MS = 3 * 60_000;

export type Health =
  /** Positions arrive. */
  | { kind: "ok" }
  /** The segment has just started, or the positions have not been read yet: no position is normal. */
  | { kind: "waiting" }
  /** A minute and still no position (location off, no permission, the service does not run). */
  | { kind: "no_fix" }
  /** The last position is old; `minutes` whole minutes. */
  | { kind: "quiet"; minutes: number };

/**
 * The health of a trip that is recording. Being registered with the system is no proof that it
 * records: after a restart of the app the foreground service does not run, location may be off.
 * The one proof is that positions arrive, so this looks at the age of the last one.
 *
 * Measured from the start of the current segment (Start or the last Resume) and with the positions
 * of that segment only: right after a Resume the positions from before the pause are old, and
 * would say "quiet" before the phone has had a chance to send a new one.
 */
export function recordingHealth(input: {
  /** `Trip.segmentStartedAt`. */
  segmentStartedAt: number;
  /**
   * When the newest position of the segment was taken (`lastFixOfSegment`), `null` when there is
   * none, and `undefined` while the positions have not been read yet (opening a trip that is older
   * than a minute must not say "no position" before the phone has looked).
   */
  lastFixAt: number | null | undefined;
  now: number;
}): Health {
  const { segmentStartedAt, lastFixAt, now } = input;
  if (lastFixAt === undefined) return { kind: "waiting" };
  if (lastFixAt === null) {
    return now - segmentStartedAt >= NO_FIX_MS ? { kind: "no_fix" } : { kind: "waiting" };
  }
  const age = now - lastFixAt;
  return age >= QUIET_MS ? { kind: "quiet", minutes: Math.floor(age / 60_000) } : { kind: "ok" };
}

/** When the newest stored position of `segment` was taken; `null` when the segment has none. */
export function lastFixOfSegment(
  points: readonly { segment: number; t: number }[],
  segment: number,
): number | null {
  let last: number | null = null;
  for (const p of points) {
    if (p.segment === segment && (last === null || p.t > last)) last = p.t;
  }
  return last;
}

/** What stands between the driver and a recording, in the order the driver can fix it. */
export type Readiness = "ready" | "no_permission" | "approximate" | "services_off";

/**
 * The accuracy of a granted location permission as `fine` or `coarse`, whichever platform said it.
 * Android reports `android.accuracy` as `fine`, `coarse` or `none` (the choice "Precise" or
 * "Approximate" in the system dialog). iOS reports `ios.accuracy` as `full` or `reduced` (the
 * switch "Precise Location" in the settings of the app); `full` is `fine` and anything else,
 * `reduced` included, is `coarse`. `undefined` where the platform said nothing (iOS before 14, or
 * a platform without the choice).
 */
export function reportedAccuracy(permission: {
  android?: { accuracy?: string };
  ios?: { accuracy?: string };
}): string | undefined {
  if (permission.android?.accuracy !== undefined) return permission.android.accuracy;
  const ios = permission.ios?.accuracy;
  if (ios === undefined) return undefined;
  return ios === "full" ? "fine" : "coarse";
}

/**
 * Whether the phone can record a trip now. An approximate location is about two kilometres wide;
 * `analyseTrack` of `@obop/trip-core` drops every fix wider than 100 m, so a trip recorded with it
 * would have no distance, and the app does not start one.
 *
 * `servicesFirst` is iOS: with Location Services switched off the status of the app is "denied",
 * the same as a refusal, so the switch has to be named before the permission, or the driver is
 * told that the app has no permission when the whole phone has none to give. On Android the
 * status is "granted" with the switch off, and the permission is named first.
 */
export function readiness(input: {
  granted: boolean;
  accuracy: string | undefined;
  servicesEnabled: boolean;
  servicesFirst?: boolean;
}): Readiness {
  if (input.servicesFirst === true && !input.servicesEnabled) return "services_off";
  if (!input.granted) return "no_permission";
  if (input.accuracy !== undefined && input.accuracy !== "fine") return "approximate";
  return input.servicesEnabled ? "ready" : "services_off";
}
