/**
 * The texts that the system shows and the app's own code does not: `expo prebuild` writes them into
 * the native projects (Info.plist on iOS, strings.xml on Android), so they are known before the app
 * runs. The texts of the screens and of the recording notification are the app's (`src/i18n`).
 *
 * English only for now. Another language is one more entry here and an entry in the `locales` of
 * the Expo config, as in `mileage-app`.
 */
export type NativeLanguage = "en";

export interface NativeTexts {
  /** The name under the icon: `CFBundleDisplayName` on iOS, `app_name` on Android. */
  appName: string;
}

export const DEFAULT_LANGUAGE: NativeLanguage = "en";

export const NATIVE_TEXTS: Readonly<Record<NativeLanguage, NativeTexts>> = {
  en: {
    appName: "Rutsa",
  },
};
