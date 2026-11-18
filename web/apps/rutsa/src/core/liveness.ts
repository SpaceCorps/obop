/**
 * Whether the phone can record a trip. Pure, so it runs in `vp test`; `native/permissions.ts`
 * feeds it what the location service says. The health of a recording that runs (no position for
 * a while) comes here too, with the recovery of a recording after a restart of the app.
 *
 * After `mileage-app` (`src/core/liveness.ts`).
 */

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
