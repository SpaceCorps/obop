/**
 * The texts that the system shows and the app's JavaScript does not: the name under the icon and
 * the two usage texts of iOS (the dialog of the location permission, the dialog of the local
 * network). English is the text of the build itself (`name`, `infoPlist`, the `expo-location`
 * option); every other language becomes the `locales` of the Expo config, which `expo prebuild`
 * writes as `<lang>.lproj/InfoPlist.strings` on iOS and `res/values-b+<lang>/strings.xml` on
 * Android (docs/spec/mobile.md 8.9). The system picks them by the language of the phone, not by the
 * language chosen in the app.
 *
 * What is not here: the text of the Android permission dialogs is the system's own; the text of the
 * notification of the recording is set from JavaScript (`src/i18n`), so it follows the language of
 * the app. `org.obop.mileage` and the store entries are not texts of this file (Q37).
 *
 * The Swedish texts are MACHINE-DRAFTED and wait for the native review (docs/spec/mobile.md 8.9).
 */
export type NativeLanguage = "en" | "sv";

export interface NativeTexts {
  /** The name under the icon: `CFBundleDisplayName` on iOS, `app_name` on Android. */
  appName: string;
  /** `NSLocationWhenInUseUsageDescription`: the text of the system dialog that asks for the location. */
  locationWhenInUse: string;
  /** `NSLocalNetworkUsageDescription`: the text of the system dialog for the local network. */
  localNetwork: string;
}

export const DEFAULT_LANGUAGE: NativeLanguage = "en";

export const NATIVE_TEXTS: Readonly<Record<NativeLanguage, NativeTexts>> = {
  en: {
    appName: "obop Mileage",
    locationWhenInUse:
      "obop Mileage records the route of a trip that you start, to measure its distance. It keeps recording while the screen is off, until you stop the trip.",
    localNetwork:
      "obop Mileage connects to the obop server whose address you enter. If that server is on your local network, iOS asks you to allow this.",
  },
  sv: {
    // The name of the app is the same in every language (the label tables of the module call it
    // "appen Mileage" too). PROPOSED, for the native reviewer: "obop Körjournal", the name of the
    // module on the server, as the name under the icon.
    appName: "obop Mileage",
    locationWhenInUse:
      "obop Mileage registrerar rutten för en resa som du startar, för att mäta sträckan. Den fortsätter att registrera medan skärmen är släckt, tills du stoppar resan.",
    localNetwork:
      "obop Mileage ansluter till den obop-server vars adress du anger. Om servern finns i ditt lokala nätverk ber iOS dig tillåta det.",
  },
};

/** The `locales` of the Expo config: one entry for each language except the default one. */
export function expoLocales(
  texts: Readonly<Record<NativeLanguage, NativeTexts>> = NATIVE_TEXTS,
): Record<string, { ios: Record<string, string>; android: Record<string, string> }> {
  const locales: Record<string, { ios: Record<string, string>; android: Record<string, string> }> =
    {};
  for (const [language, text] of Object.entries(texts)) {
    if (language === DEFAULT_LANGUAGE) continue;
    locales[language] = {
      ios: {
        CFBundleDisplayName: text.appName,
        NSLocationWhenInUseUsageDescription: text.locationWhenInUse,
        NSLocalNetworkUsageDescription: text.localNetwork,
      },
      android: { app_name: text.appName },
    };
  }
  return locales;
}
