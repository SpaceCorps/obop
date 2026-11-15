import * as Location from "expo-location";
import type { Readiness } from "../core/liveness.ts";
import { t } from "../i18n/index.ts";
import { LOCATION_TASK } from "./config.ts";
import { locationReadiness } from "./permissions.ts";

/**
 * Starts the recording that delivers fixes to the task (`task.ts`). On Android that is a foreground
 * service with the notification the system requires, which names what is happening; on iOS it is
 * location updates under the background mode `location`, and the system shows its own location
 * indicator while the app is not in front. The start is a tap while the app is visible, as a
 * continuation of that action, so no background ("Always") permission is asked for on either
 * system (docs/spec/mobile.md 4.2).
 *
 * Calling it again while the app is visible starts the recording again if the system has not
 * restarted it: expo-location starts the Android service only while the app is in the foreground,
 * and on both systems the task stays registered when the process was restarted without one (on
 * iOS, after the person closed the app from the app switcher, no position arrives until this is
 * called again).
 *
 * Options of one system only: `timeInterval` and `foregroundService` are Android's and iOS ignores
 * them; iOS delivers by distance (`distanceInterval`), about once a second in a moving car, and
 * `core/thin.ts` stores at most one fix in two seconds when the fixes are written (it leaves the
 * 5 second fixes of Android alone and keeps a fix that is far from the last one). `accuracy: High` is the
 * 10 m class on iOS (`kCLLocationAccuracyNearestTenMeters`), which is enough for road distance and
 * costs less battery than the best class. `showsBackgroundLocationIndicator` matters for apps with
 * "Always" access; under "When In Use" the system shows the indicator by itself.
 */
export async function startRecording(): Promise<void> {
  await Location.startLocationUpdatesAsync(LOCATION_TASK, {
    accuracy: Location.Accuracy.High,
    timeInterval: 5000,
    distanceInterval: 5,
    pausesUpdatesAutomatically: false,
    showsBackgroundLocationIndicator: true,
    foregroundService: {
      notificationTitle: t("recording.notification.title"),
      notificationBody: t("recording.notification.body"),
      notificationColor: "#0b6bcb",
    },
  });
}

/**
 * Checks what a recording needs and starts it. `startLocationUpdatesAsync` resolves also when the
 * location switch of the phone is off or the permission is only approximate, and then no position
 * ever arrives; so the answer says which of these it was, and the service is started only when
 * the phone can record. Errors of the system are not swallowed.
 */
export async function ensureRecording(): Promise<Readiness> {
  const ready = await locationReadiness();
  if (ready === "ready") await startRecording();
  return ready;
}

export async function stopRecording(): Promise<void> {
  if (await Location.hasStartedLocationUpdatesAsync(LOCATION_TASK)) {
    await Location.stopLocationUpdatesAsync(LOCATION_TASK);
  }
}

/**
 * Whether the task is registered with the system. It does not mean that positions arrive (the
 * service may not run after a restart of the process); `recordingHealth` is the proof.
 */
export function isRecording(): Promise<boolean> {
  return Location.hasStartedLocationUpdatesAsync(LOCATION_TASK);
}
