import type { ExpoConfig } from "expo/config";
import { withAndroidManifest, withInfoPlist, type ConfigPlugin } from "expo/config-plugins";
import { identity } from "./config/identity.ts";
import { IOS_USAGE_TEXTS, iosBundleId, withoutDevClientEntries } from "./config/ios.ts";

// PLACEHOLDER: "org.obop.app.test" is a stand-in application id and bundle id for every build of
// this slice. It is not registered anywhere and no store account, signing key or publication
// exists (Q37 of docs/DECISIONS.md). Once an app is on a store the id is hard to change, so the
// owner rules first.
//
// On iOS the bundle id of a build can be another one (OBOP_IOS_BUNDLE_ID). The reason is Apple's
// signing: an App ID that was first signed in a free personal team can later not be registered by
// the SpaceCorps team.
const APPLICATION_ID = identity.applicationId;
const IOS_BUNDLE_ID = iosBundleId(process.env.OBOP_IOS_BUNDLE_ID, APPLICATION_ID);

// Three variables make a build, and each is needed twice with the same value: at `expo prebuild`
// (the manifest) and at the Gradle or Xcode build (the build phase of expo-constants evaluates this
// file again and writes the `extra` the app reads).
//
//   OBOP_ALLOW_CLEARTEXT=1   the build may speak plain http, and only to this device or the local
//                            network (the address rule of the app is the second check). Without it
//                            a build connects over https only: on Android the manifest says so
//                            (`usesCleartextTraffic`), on both systems the address rule does. iOS
//                            has no per-build switch: the Expo template puts
//                            NSAllowsArbitraryLoads=false and NSAllowsLocalNetworking=true into
//                            every build and the app adds no exception, so there the address rule
//                            is the only guard.
//   OBOP_WEBVIEW_DEBUG=1     the WebView is open to DevTools. A variable of its own: "speaks plain
//                            http" and "open to DevTools" are separate facts.
//   OBOP_DEFAULT_SERVER      the address the address field starts with.
//
// A build with either of the first two is a TEST build, and the app says so on its host bar. No
// build of this slice may be signed in to an instance with real data: every APK is signed with the
// public debug key of the template, a test build puts the password and the token on the network in
// clear and can be read through DevTools.
const CLEARTEXT = process.env.OBOP_ALLOW_CLEARTEXT === "1";
const WEBVIEW_DEBUG = process.env.OBOP_WEBVIEW_DEBUG === "1";
const DEFAULT_SERVER = process.env.OBOP_DEFAULT_SERVER ?? "";

// `expo-dev-client` is in the plugins for the development build (`vp run start`). Its iOS entries in
// Info.plist (the Bonjour service `_expo._tcp` and the URL scheme `exp+obop-host`) belong to that
// build only and are removed from every other one; OBOP_DEV_CLIENT=1 keeps them.
const DEV_CLIENT = process.env.OBOP_DEV_CLIENT === "1";

const withCleartext: ConfigPlugin = (config) =>
  withAndroidManifest(config, (c) => {
    const application = c.modResults.manifest.application?.[0];
    if (application) application.$["android:usesCleartextTraffic"] = CLEARTEXT ? "true" : "false";
    return c;
  });

// Mods run in the reverse order of their definition and the plugins of the `plugins` list are
// applied after this wrapper, so this one runs after all of them.
const withIosPlistCleanup: ConfigPlugin = (config) =>
  withInfoPlist(config, (c) => {
    if (!DEV_CLIENT) c.modResults = withoutDevClientEntries(c.modResults);
    return c;
  });

const config: ExpoConfig = {
  name: identity.name,
  slug: identity.slug,
  // The app has a scheme and ignores every address it is opened with: there is nothing to open
  // before a session exists, and a link to a record comes later.
  scheme: identity.scheme,
  version: identity.version,
  // The page lays itself out for both ways of holding a phone.
  orientation: "default",
  // The page follows the scheme of the system when its theme is "system"; expo-system-ui is what
  // makes Android report it to the app.
  userInterfaceStyle: "automatic",
  android: {
    package: APPLICATION_ID,
    versionCode: identity.versionCode,
    // The storage of the WebView (the page's localStorage, cookies) would go into a cloud backup.
    // The switch does not stop a phone-to-phone transfer, and iOS has no such switch.
    allowBackup: false,
    // The window is resized when the keyboard opens, so that the page can keep the field in view.
    softwareKeyboardLayoutMode: "resize",
    // The app asks for nothing. INTERNET comes from the template.
    permissions: [],
    // Permissions that libraries add to the merged manifest and the app never uses: the overlay of
    // the development menu, old external storage access, vibration and the biometric prompt of
    // secure-store. And four the app must not hold: with CAMERA undeclared the chooser of a file
    // field offers the camera app by intent and no prompt appears; the page gets no camera, no
    // microphone and no location of its own.
    blockedPermissions: [
      "android.permission.SYSTEM_ALERT_WINDOW",
      "android.permission.READ_EXTERNAL_STORAGE",
      "android.permission.WRITE_EXTERNAL_STORAGE",
      "android.permission.VIBRATE",
      "android.permission.USE_BIOMETRIC",
      "android.permission.USE_FINGERPRINT",
      "android.permission.CAMERA",
      "android.permission.RECORD_AUDIO",
      "android.permission.ACCESS_FINE_LOCATION",
      "android.permission.ACCESS_COARSE_LOCATION",
    ],
  },
  ios: {
    bundleIdentifier: IOS_BUNDLE_ID,
    // No ATS exception and no Bonjour keys: the app does no discovery (the ones of the development
    // client are removed, see DEV_CLIENT).
    infoPlist: { ...IOS_USAGE_TEXTS },
  },
  extra: {
    cleartextAllowed: CLEARTEXT,
    webviewDebug: WEBVIEW_DEBUG,
    defaultServer: DEFAULT_SERVER,
  },
  plugins: [
    "expo-dev-client",
    // The app stores the address of the server without a fingerprint or Face ID
    // (`requireAuthentication` is never set), so the plugin writes no Face ID usage text on iOS.
    ["expo-secure-store", { faceIDPermission: false }],
  ],
};

export default withIosPlistCleanup(withCleartext(config));
