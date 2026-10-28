import * as Location from "expo-location";
import { t } from "../i18n/index.ts";
import { LOCATION_TASK } from "./config.ts";

/**
 * Starts the foreground service that delivers fixes to the task (`task.ts`). The service is
 * started by a tap while the app is visible, as a continuation of that action, so no background
 * location permission is asked for (docs/spec/mobile.md 4.2). The notification is the one Android
 * requires for a foreground service; it names what is happening.
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

export async function stopRecording(): Promise<void> {
  if (await Location.hasStartedLocationUpdatesAsync(LOCATION_TASK)) {
    await Location.stopLocationUpdatesAsync(LOCATION_TASK);
  }
}

export function isRecording(): Promise<boolean> {
  return Location.hasStartedLocationUpdatesAsync(LOCATION_TASK);
}
