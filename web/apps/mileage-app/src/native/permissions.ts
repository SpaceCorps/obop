import * as Location from "expo-location";
import { Linking, PermissionsAndroid, Platform } from "react-native";
import { readiness, reportedAccuracy, type Readiness } from "../core/liveness.ts";

/** What the app can do about the location permission now. */
export type LocationAccess =
  | "granted"
  /** Allowed, but only "Approximate": a trip cannot be measured with it. Only the settings can change it. */
  | "approximate"
  /** Not asked yet, or refused once: the system dialog can be shown. */
  | "ask"
  /** Refused with "don't ask again": only the system settings can change it. */
  | "blocked"
  /** iOS: Location Services are switched off for the whole phone (the app's own status then reads "denied"). */
  | "services_off";

function access(r: Location.LocationPermissionResponse): LocationAccess {
  if (r.granted) {
    // Android 12 and later (Precise or Approximate) and iOS 14 and later (the switch "Precise
    // Location") let the driver allow only an approximate position. Approximate fixes are about
    // two kilometres wide. The server drops every fix wider than 100 m, so a trip recorded with
    // Approximate location would be empty. The app refuses to start instead.
    return readiness({ granted: true, accuracy: reportedAccuracy(r), servicesEnabled: true }) ===
      "approximate"
      ? "approximate"
      : "granted";
  }
  return r.canAskAgain ? "ask" : "blocked";
}

/**
 * On iOS the switch of the whole phone is asked first: with it off the permission of the app reads
 * "denied" and the driver would be sent to the settings of the app, which cannot help. Android
 * reports the permission as it is and the switch only where the permission is granted.
 */
async function withServices(r: Location.LocationPermissionResponse): Promise<LocationAccess> {
  if (Platform.OS === "ios" && !(await Location.hasServicesEnabledAsync())) return "services_off";
  return access(r);
}

export async function locationAccess(): Promise<LocationAccess> {
  return withServices(await Location.getForegroundPermissionsAsync());
}

/** Shows the system dialog. Call it after the disclosure, never before. */
export async function requestLocation(): Promise<LocationAccess> {
  return withServices(await Location.requestForegroundPermissionsAsync());
}

/**
 * Whether the phone can record a trip right now: the permission is precise, and the location
 * switch of the phone is on. Checked before Start and again whenever a trip is open, because
 * `startLocationUpdatesAsync` resolves also when it can obtain no position.
 */
export async function locationReadiness(): Promise<Readiness> {
  const permission = await Location.getForegroundPermissionsAsync();
  const ios = Platform.OS === "ios";
  const servicesEnabled =
    ios || permission.granted ? await Location.hasServicesEnabledAsync() : true;
  return readiness({
    granted: permission.granted,
    accuracy: reportedAccuracy(permission),
    servicesEnabled,
    servicesFirst: ios,
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
