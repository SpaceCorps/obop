import * as Location from "expo-location";
import { Linking, PermissionsAndroid, Platform } from "react-native";

/** What the app can do about the location permission now. */
export type LocationAccess =
  | "granted"
  /** Not asked yet, or refused once: the system dialog can be shown. */
  | "ask"
  /** Refused with "don't ask again": only the system settings can change it. */
  | "blocked";

function access(r: Location.LocationPermissionResponse): LocationAccess {
  if (r.granted) return "granted";
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
