import type { ExpoConfig } from "expo/config";
import { withAppBuildGradle, withInfoPlist, type ConfigPlugin } from "expo/config-plugins";
import { identity } from "./config/identity.ts";
import { iosBundleId, withoutDevClientEntries, withoutFetchMode } from "./config/ios.ts";
import { NATIVE_TEXTS } from "./config/native-texts.ts";

// The id is a placeholder until the owner rules (config/identity.ts). A test build for iOS signed
// with a free Apple ID sets OBOP_IOS_BUNDLE_ID to another id (config/ios.ts).
const APPLICATION_ID = identity.applicationId;
const IOS_BUNDLE_ID = iosBundleId(process.env.OBOP_IOS_BUNDLE_ID, APPLICATION_ID);

// Android 8.0 (API 26) is the first version where expo-location starts the foreground service that
// keeps a trip recording and shows its notification (docs/spec/mobile.md; the same minimum as the
// Mileage app). The template's default is 24, so the app raises its own minimum.
const MIN_SDK = 26;
const withMinSdk: ConfigPlugin = (config) =>
  withAppBuildGradle(config, (c) => {
    c.modResults.contents = c.modResults.contents.replace(
      /minSdkVersion\s+rootProject\.ext\.minSdkVersion/,
      `minSdkVersion Math.max(rootProject.ext.minSdkVersion, ${MIN_SDK})`,
    );
    return c;
  });

// The one background mode on iOS is `location`: the `fetch` that expo-task-manager adds is taken out
// (config/ios.ts).
//
// `expo-dev-client` is in the plugins for the development build (`vp run start`). Its iOS entries in
// Info.plist (the Bonjour service `_expo._tcp` and the URL scheme `exp+rutsa`) belong to that build
// only and are removed from every other one; OBOP_DEV_CLIENT=1 at `expo prebuild` keeps them for an
// iOS development build. Mods run in the reverse order of their definition and the plugins of the
// `plugins` list are applied after this wrapper, so this one runs after them (as in the Mileage app).
const DEV_CLIENT = process.env.OBOP_DEV_CLIENT === "1";
const withIosPlistCleanup: ConfigPlugin = (config) =>
  withInfoPlist(config, (c) => {
    c.modResults.UIBackgroundModes = withoutFetchMode(c.modResults.UIBackgroundModes);
    if (!DEV_CLIENT) c.modResults = withoutDevClientEntries(c.modResults);
    return c;
  });

const config: ExpoConfig = {
  name: NATIVE_TEXTS.en.appName,
  slug: "rutsa",
  version: identity.version,
  orientation: "portrait",
  // Light or dark as the phone is set; on Android this needs expo-system-ui.
  userInterfaceStyle: "automatic",
  android: {
    package: APPLICATION_ID,
    versionCode: 1,
    // The trips are a record of where the person drove, kept on this phone only. A cloud backup
    // would carry the database without its key, which stays in the Keystore of this phone, so
    // Android's backup is off.
    allowBackup: false,
    // expo-task-manager schedules a persisted job when a location arrives and Android refuses that
    // without this permission: the first emulator run of the Mileage app's release build crashed
    // with "Requested job cannot be persisted without holding
    // android.permission.RECEIVE_BOOT_COMPLETED" at the first fix. No library of the app declares
    // it. It is a normal permission (no prompt).
    permissions: ["android.permission.RECEIVE_BOOT_COMPLETED"],
    // Permissions that libraries add to the merged manifest and the app never uses: the overlay of
    // the development menu, old external storage access, vibration, and the biometric prompt of
    // secure-store (the key is stored without a fingerprint). The same list as the Mileage app;
    // blocking one that no library adds changes nothing.
    blockedPermissions: [
      "android.permission.SYSTEM_ALERT_WINDOW",
      "android.permission.READ_EXTERNAL_STORAGE",
      "android.permission.WRITE_EXTERNAL_STORAGE",
      "android.permission.VIBRATE",
      "android.permission.USE_BIOMETRIC",
      "android.permission.USE_FINGERPRINT",
    ],
  },
  ios: {
    bundleIdentifier: IOS_BUNDLE_ID,
  },
  plugins: [
    "expo-dev-client",
    // The key of the database is stored without a fingerprint or Face ID, so the plugin writes no
    // Face ID text on iOS. An app lock with Face ID (docs/spec/rutsa.md, BR-35) would turn it on.
    ["expo-secure-store", { faceIDPermission: false }],
    // SQLCipher: the routes are encrypted on the phone. `core/open-db.ts` checks
    // `PRAGMA cipher_version` at every start and refuses to run on plain SQLite.
    ["expo-sqlite", { useSQLCipher: true }],
    [
      "expo-location",
      {
        // A trip is started with a tap (docs/spec/mobile.md 4.2): a foreground service of type
        // location, started while the app is visible, keeps it recording with the screen off and
        // needs no ACCESS_BACKGROUND_LOCATION. Automatic start (stage 4) would need it.
        isAndroidForegroundServiceEnabled: true,
        isAndroidBackgroundLocationEnabled: false,
        // iOS: adds the background mode `location` and nothing else; it does not ask for "Always".
        // Without the mode a recording cannot start (ERR_LOCATION_UPDATES_UNAVAILABLE).
        isIosBackgroundLocationEnabled: true,
        // The texts of "Always", motion and a temporary full accuracy are left out: the app asks
        // for none of them. As in the Mileage app.
        locationAlwaysAndWhenInUsePermission: false,
        locationAlwaysPermission: false,
        motionUsagePermission: false,
        locationFullAccuracyPermission: false,
        locationWhenInUsePermission: NATIVE_TEXTS.en.locationWhenInUse,
      },
    ],
  ],
};

export default withIosPlistCleanup(withMinSdk(config));
