import * as Location from "expo-location";
import { t } from "../i18n/index.ts";
import { LOCATION_TASK } from "./config.ts";

/**
 * Starts the location updates that deliver positions to the task (`task.ts`). On Android that is a
 * foreground service with the notification the system requires, which says that a trip is being
 * recorded; on iOS it is location updates under the background mode `location`, and the system
 * shows its own location indicator while the app is not in front. The start is a tap while the app
 * is visible, so no background ("Always") permission is asked for on either system.
 *
 * Options of one system only: `timeInterval` and `foregroundService` are Android's and iOS ignores
 * them; iOS delivers by distance (`distanceInterval`), about once a second in a moving car, and
 * `core/thin.ts` stores at most one fix in two seconds (it leaves the 5 second fixes of Android
 * alone and keeps a fix that is far from the last one). `accuracy: High` is the 10 m class on iOS,
 * which is enough for road distance and costs less battery than the best class.
 *
 * From `mileage-app` (`src/native/recorder.ts`).
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

/** Stops the updates; nothing happens when they are not running (after Pause, or a restart). */
export async function stopRecording(): Promise<void> {
  if (await isRecording()) {
    await Location.stopLocationUpdatesAsync(LOCATION_TASK);
  }
}

/**
 * Whether the updates are registered with the system. `false` while a trip records means they
 * were stopped. `true` does not mean that positions arrive: after a restart of the app the task
 * stays registered without its service; `recordingHealth` (`core/liveness.ts`) is the proof.
 */
export function isRecording(): Promise<boolean> {
  return Location.hasStartedLocationUpdatesAsync(LOCATION_TASK);
}
