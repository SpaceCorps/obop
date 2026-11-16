import type { ExpoConfig } from "expo/config";
import { withAppBuildGradle, type ConfigPlugin } from "expo/config-plugins";
import { identity } from "./config/identity.ts";
import { iosBundleId } from "./config/ios.ts";
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
};

export default withMinSdk(config);
