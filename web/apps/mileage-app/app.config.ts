import type { ExpoConfig } from "expo/config";
import { withAndroidManifest, withAppBuildGradle, type ConfigPlugin } from "expo/config-plugins";

// PLACEHOLDER: "org.obop.mileage" is a stand-in application id and bundle id. It is not registered
// anywhere and no store account, signing key or publication exists (Q37 of docs/DECISIONS.md).
// Once an app is on a store the id is hard to change, so the owner rules first.
const APPLICATION_ID = "org.obop.mileage";

// A build for a test instance on the same network (the emulator reaches the host as 10.0.2.2, a
// phone reaches it over the local network) may speak plain http. Only a build made with
// OBOP_ALLOW_CLEARTEXT=1 does; OBOP_DEFAULT_SERVER fills the address field of the sign-in. A
// build made without them connects over https only, and the check is in two places: the manifest
// (`usesCleartextTraffic`) and the address check of the sign-in (`src/native/auth.ts`).
const CLEARTEXT = process.env.OBOP_ALLOW_CLEARTEXT === "1";
const DEFAULT_SERVER = process.env.OBOP_DEFAULT_SERVER ?? "";

const withCleartext: ConfigPlugin = (config) =>
  withAndroidManifest(config, (c) => {
    const application = c.modResults.manifest.application?.[0];
    if (application) application.$["android:usesCleartextTraffic"] = CLEARTEXT ? "true" : "false";
    return c;
  });

// Android 8.0 (API 26) is the first version where expo-location starts the foreground service that
// keeps a trip recording and shows its notification; on API 24 and 25 it starts nothing (the
// notification the disclosure promises would not exist and nothing would protect the process). The
// template's default is 24, so the app raises its own minimum; no library needs a lower one.
const MIN_SDK = 26;
const withMinSdk: ConfigPlugin = (config) =>
  withAppBuildGradle(config, (c) => {
    c.modResults.contents = c.modResults.contents.replace(
      /minSdkVersion\s+rootProject\.ext\.minSdkVersion/,
      `minSdkVersion Math.max(rootProject.ext.minSdkVersion, ${MIN_SDK})`,
    );
    return c;
  });

const config: ExpoConfig = {
  name: "obop Mileage",
  slug: "obop-mileage",
  // The redirect of the sign-in is `<scheme>:/oauth`, a private-use scheme in reverse domain form
  // (RFC 8252 section 7.1): the instance refuses any other form for a mobile client.
  scheme: APPLICATION_ID,
  version: "0.1.0",
  orientation: "portrait",
  userInterfaceStyle: "automatic",
  android: {
    package: APPLICATION_ID,
    versionCode: 1,
    // The database is encrypted with a key in the Keystore. A cloud backup would carry the file
    // without the key, so Android's backup is off.
    allowBackup: false,
    // expo-task-manager schedules a persisted job when a location arrives and Android refuses that
    // without this permission: the first emulator run of the release build crashed with "Requested
    // job cannot be persisted without holding android.permission.RECEIVE_BOOT_COMPLETED" at the
    // first fix. No library of the app declares it. It is a normal permission (no prompt).
    permissions: ["android.permission.RECEIVE_BOOT_COMPLETED"],
    // Permissions that libraries add to the merged manifest and the app never uses: the overlay of
    // the development menu, old external storage access, vibration, and the biometric prompt of
    // secure-store (the app stores its keys without asking for a fingerprint).
    blockedPermissions: [
      "android.permission.SYSTEM_ALERT_WINDOW",
      "android.permission.READ_EXTERNAL_STORAGE",
      "android.permission.WRITE_EXTERNAL_STORAGE",
      "android.permission.VIBRATE",
      "android.permission.USE_BIOMETRIC",
      "android.permission.USE_FINGERPRINT",
    ],
  },
  ios: { bundleIdentifier: APPLICATION_ID },
  extra: { cleartextAllowed: CLEARTEXT, defaultServer: DEFAULT_SERVER },
  plugins: [
    "expo-dev-client",
    [
      "expo-location",
      {
        // Tap-to-start trips (docs/spec/mobile.md 4.2): a foreground service of type location
        // started while the app is visible needs no ACCESS_BACKGROUND_LOCATION.
        isAndroidForegroundServiceEnabled: true,
        isAndroidBackgroundLocationEnabled: false,
        isIosBackgroundLocationEnabled: false,
      },
    ],
    "expo-secure-store",
    // SQLCipher: the route stays encrypted on the phone until the server has it. The app checks
    // `PRAGMA cipher_version` at start and refuses to run on a plain SQLite.
    ["expo-sqlite", { useSQLCipher: true }],
    "expo-web-browser",
  ],
};

export default withMinSdk(withCleartext(config));
