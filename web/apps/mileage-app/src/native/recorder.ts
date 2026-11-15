import * as Location from "expo-location";
import type { Readiness } from "../core/liveness.ts";
import { t } from "../i18n/index.ts";
import { LOCATION_TASK } from "./config.ts";
import { locationReadiness } from "./permissions.ts";

/**
 * Starts the foreground service that delivers fixes to the task (`task.ts`). The service is
 * started by a tap while the app is visible, as a continuation of that action, so no background
 * location permission is asked for (docs/spec/mobile.md 4.2). The notification is the one Android
 * requires for a foreground service; it names what is happening.
 *
 * Calling it again while the app is visible starts the service again if the system has not
 * restarted it (expo-location starts the service only while the app is in the foreground, and the
 * task stays registered when the process was restarted without one).
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
