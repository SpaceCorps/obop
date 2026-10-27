import type { ExpoConfig } from "expo/config";

// PLACEHOLDER: "org.obop.mileage" is a stand-in application id and bundle id for the spike. It is
// not registered anywhere and no store account, signing key or publication exists (Q37 of
// docs/DECISIONS.md). Once an app is on a store the id is hard to change, so the owner rules first.
const APPLICATION_ID = "org.obop.mileage";

const config: ExpoConfig = {
  name: "obop Mileage",
  slug: "obop-mileage",
  scheme: "obop-mileage",
  version: "0.0.0",
  orientation: "portrait",
  userInterfaceStyle: "automatic",
  android: { package: APPLICATION_ID },
  ios: { bundleIdentifier: APPLICATION_ID },
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
    "expo-sqlite",
  ],
};

export default config;
