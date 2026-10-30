import type { ExpoConfig } from "expo/config";
import {
  withAndroidManifest,
  withAppBuildGradle,
  withInfoPlist,
  type ConfigPlugin,
} from "expo/config-plugins";
import {
  iosBundleId,
  recordThinningOff,
  withoutDevClientEntries,
  withoutFetchMode,
} from "./config/ios.ts";

// PLACEHOLDER: "org.obop.mileage" is a stand-in application id and bundle id. It is not registered
// anywhere and no store account, signing key or publication exists (Q37 of docs/DECISIONS.md).
// Once an app is on a store the id is hard to change, so the owner rules first.
//
// On iOS the sign-in scheme stays `APPLICATION_ID` but the bundle id of a build can be another one
// (OBOP_IOS_BUNDLE_ID; `org.obop.mileage.test` for the test builds). The reason is Apple's signing:
// an App ID that was first signed in a free personal team can later not be registered by the
// SpaceCorps team, so a test build must not take the id that the store build will have (lead
// decision of the iOS port; the owner can override it, Q37).
const APPLICATION_ID = "org.obop.mileage";
const IOS_BUNDLE_ID = iosBundleId(process.env.OBOP_IOS_BUNDLE_ID, APPLICATION_ID);

// A build for a test instance on the same network (the emulator reaches the host as 10.0.2.2, a
// phone reaches it over the local network) may speak plain http. Only a build made with
// OBOP_ALLOW_CLEARTEXT=1 does; OBOP_DEFAULT_SERVER fills the address field of the sign-in. A
// build made without them connects over https only, and the check is in two places: on Android the
// manifest (`usesCleartextTraffic`), on both systems the address check of the sign-in
// (`src/native/auth.ts`, which also runs on a stored address when the app starts). iOS has no
// per-build switch for http: the Expo template puts NSAllowsArbitraryLoads=false and
// NSAllowsLocalNetworking=true into every build's Info.plist and the app adds no exception, so what
// keeps a release build from speaking http to a LAN address is the address check, and
// `scripts/ios/gate.mjs` fails a build made for release that still has `cleartextAllowed` set (the
// Xcode build phase of expo-constants evaluates this file again with the environment of that build,
// so a forgotten .env.local can turn it on).
const CLEARTEXT = process.env.OBOP_ALLOW_CLEARTEXT === "1";
const DEFAULT_SERVER = process.env.OBOP_DEFAULT_SERVER ?? "";

// A test build made with OBOP_RECORD_THINNING=off stores every fix the phone delivers (no thinning,
// src/core/thin.ts), so that one drive shows the cadence of the system and the thinning rules can be
// replayed over it (scripts/emulator/thinning-replay.mjs). The app honours it only where plain http
// is allowed too, i.e. in a test build, and `scripts/ios/gate.mjs` fails a release build that
// carries it.
const THINNING_OFF = recordThinningOff(process.env.OBOP_RECORD_THINNING);

// `expo-dev-client` is in the plugins for the development build (`vp run start`). Its iOS entries in
// Info.plist (the Bonjour service `_expo._tcp` and the URL scheme `exp+obop-mileage`) belong to that
// build only and are removed from every other one; OBOP_DEV_CLIENT=1 keeps them. The plugin stays in
// the list because it also configures Android, which this change does not touch.
const DEV_CLIENT = process.env.OBOP_DEV_CLIENT === "1";

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

// The one background mode the app needs on iOS is `location` (the expo-location option below). The
// plugin of expo-task-manager also adds `fetch`, which the app never uses, and an unused mode is a
// store review question. Mods run in the reverse order of their definition and the plugins of the
// `plugins` list are applied after this wrapper, so this one runs after all of them (checked in the
// generated Info.plist, docs/spec/mobile.md 8.8). The same place removes the entries of the
// development client unless OBOP_DEV_CLIENT=1.
const withIosPlistCleanup: ConfigPlugin = (config) =>
  withInfoPlist(config, (c) => {
    c.modResults.UIBackgroundModes = withoutFetchMode(c.modResults.UIBackgroundModes);
    if (!DEV_CLIENT) c.modResults = withoutDevClientEntries(c.modResults);
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
    // without the key, so Android's backup is off. iOS has no such switch: the database file is in
    // the app's Documents folder, which an iCloud or Finder backup carries, while the key (a
    // Keychain item of the class "this device only") does not travel with it. The app recovers from
    // that at start (`src/core/open-outbox.ts`) instead of excluding the file from the backup.
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
  ios: {
    bundleIdentifier: IOS_BUNDLE_ID,
    infoPlist: {
      // iOS 14 and later ask the person for local network access the first time the app reaches an
      // address on the local network (a test instance), and show this text. No Bonjour keys: the
      // app does no discovery (the ones of the development client are removed, see DEV_CLIENT).
      NSLocalNetworkUsageDescription:
        "obop Mileage connects to the obop server whose address you enter. If that server is on your local network, iOS asks you to allow this.",
    },
  },
  extra: { cleartextAllowed: CLEARTEXT, defaultServer: DEFAULT_SERVER, thinningOff: THINNING_OFF },
  plugins: [
    "expo-dev-client",
    [
      "expo-location",
      {
        // Tap-to-start trips (docs/spec/mobile.md 4.2): a foreground service of type location
        // started while the app is visible needs no ACCESS_BACKGROUND_LOCATION.
        isAndroidForegroundServiceEnabled: true,
        isAndroidBackgroundLocationEnabled: false,
        // iOS: this option adds the background mode `location` and nothing else. Despite its name
        // it does not ask for "Always" access: `startLocationUpdatesAsync` checks that the mode is
        // declared and that the foreground ("When In Use") permission is granted, and the app asks
        // for no more (lead decision of the iOS port, docs/spec/mobile.md 4.2). With the mode off,
        // starting a trip fails with ERR_LOCATION_UPDATES_UNAVAILABLE.
        isIosBackgroundLocationEnabled: true,
        // The plugin writes default strings for the keys of "Always" access, motion activity and a
        // temporary full-accuracy request, none of which the app uses; `false` leaves each key out.
        // The Always keys can be left out because expo-location builds the
        // `requestAlwaysAuthorization` selector at run time, so a build without them is not
        // flagged (if an App Store upload ever complains, restore them with a truthful text).
        locationAlwaysAndWhenInUsePermission: false,
        locationAlwaysPermission: false,
        motionUsagePermission: false,
        locationFullAccuracyPermission: false,
        locationWhenInUsePermission:
          "obop Mileage records the route of a trip that you start, to measure its distance. It keeps recording while the screen is off, until you stop the trip.",
      },
    ],
    // The app stores its keys without a fingerprint or Face ID (`requireAuthentication` is never
    // set), so the plugin does not write a Face ID usage text on iOS.
    ["expo-secure-store", { faceIDPermission: false }],
    // SQLCipher: the route stays encrypted on the phone until the server has it. The app checks
    // `PRAGMA cipher_version` at start and refuses to run on a plain SQLite.
    ["expo-sqlite", { useSQLCipher: true }],
    "expo-web-browser",
  ],
};

export default withIosPlistCleanup(withMinSdk(withCleartext(config)));
