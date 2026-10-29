import * as Location from "expo-location";
import { Linking, PermissionsAndroid, Platform } from "react-native";
import { readiness, type Readiness } from "../core/liveness.ts";

/** What the app can do about the location permission now. */
export type LocationAccess =
  | "granted"
  /** Allowed, but only "Approximate": a trip cannot be measured with it. Only the settings can change it. */
  | "approximate"
  /** Not asked yet, or refused once: the system dialog can be shown. */
  | "ask"
  /** Refused with "don't ask again": only the system settings can change it. */
  | "blocked";

function access(r: Location.LocationPermissionResponse): LocationAccess {
  if (r.granted) {
    // Android 12 and later let the driver allow Precise or Approximate. Approximate fixes are
    // about two kilometres wide and the phone and the server drop everything wider than 100 m.
    return readiness({ granted: true, accuracy: r.android?.accuracy, servicesEnabled: true }) ===
      "approximate"
      ? "approximate"
      : "granted";
  }
  return r.canAskAgain ? "ask" : "blocked";
}

export async function locationAccess(): Promise<LocationAccess> {
  return access(await Location.getForegroundPermissionsAsync());
}

/** Shows the system dialog. Call it after the disclosure, never before. */
export async function requestLocation(): Promise<LocationAccess> {
  return access(await Location.requestForegroundPermissionsAsync());
}

/**
 * Whether the phone can record a trip right now: the permission is precise, and the location
 * switch of the phone is on. Checked before Start and again whenever a trip is open, because
 * `startLocationUpdatesAsync` resolves also when it can obtain no position.
 */
export async function locationReadiness(): Promise<Readiness> {
  const permission = await Location.getForegroundPermissionsAsync();
  const servicesEnabled = permission.granted ? await Location.hasServicesEnabledAsync() : true;
  return readiness({
    granted: permission.granted,
    accuracy: permission.android?.accuracy,
    servicesEnabled,
  });
}

/**
 * Android 13 and later: the notification of the foreground service is shown only if this is
 * granted. Recording works without it; the notification is how the driver sees that it runs.
 * Answers whether the notification can be shown.
 */
export async function requestNotifications(): Promise<boolean> {
  if (Platform.OS !== "android" || Number(Platform.Version) < 33) return true;
  const result = await PermissionsAndroid.request("android.permission.POST_NOTIFICATIONS");
  return result === PermissionsAndroid.RESULTS.GRANTED;
}

export function openSettings(): Promise<void> {
  return Linking.openSettings();
}
