import * as Location from "expo-location";
import { Linking, PermissionsAndroid, Platform } from "react-native";
import { readiness, reportedAccuracy, type Readiness } from "../core/liveness.ts";

/**
 * The location and notification permissions as the screens need them (docs/spec/rutsa.md, BR-36): what
 * the driver can do about each case, the system dialog, and the app's page in the settings.
 *
 * After `mileage-app` (`src/native/permissions.ts`).
 */

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

/** Shows the system dialog. Call it after the app's own explanation, never before. */
export async function requestLocation(): Promise<LocationAccess> {
  return withServices(await Location.requestForegroundPermissionsAsync());
}

/**
 * Whether the phone can record a trip right now: the permission is precise, and the location
 * switch of the phone is on. Checked before Start and again while a trip is open, because
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
 * Android 13 and later: the notification of the recording (BR-14) is shown only if this is
 * granted. Recording works without it; the notification is how the driver sees that it runs.
 * Answers whether the notification can be shown.
 */
export async function requestNotifications(): Promise<boolean> {
  if (Platform.OS !== "android" || Number(Platform.Version) < 33) return true;
  const result = await PermissionsAndroid.request("android.permission.POST_NOTIFICATIONS");
  return result === PermissionsAndroid.RESULTS.GRANTED;
}

/** Whether the notification of a recording can be shown, without a dialog (Android 13 and later). */
export async function notificationsAllowed(): Promise<boolean> {
  if (Platform.OS !== "android" || Number(Platform.Version) < 33) return true;
  return PermissionsAndroid.check("android.permission.POST_NOTIFICATIONS");
}

/** The app's own page in the system settings: Settings, Rutsa on iOS; App info on Android. */
export function openSettings(): Promise<void> {
  return Linking.openSettings();
}

/**
 * Where the driver switches location on for the whole phone. Android opens its location settings
 * directly; where that fails (a phone without that page) it falls back to the app's own page.
 * iOS lets no app open the page of Location Services, so it opens the app's own page, and the text
 * on the screen says where to go (Settings, Privacy & Security, Location Services).
 */
export async function openLocationSettings(): Promise<void> {
  if (Platform.OS === "android") {
    try {
      await Linking.sendIntent("android.settings.LOCATION_SOURCE_SETTINGS");
      return;
    } catch {
      // Falls back to the app's own page below.
    }
  }
  await Linking.openSettings();
}
