import { I18nManager, Platform, Settings } from "react-native";

/**
 * The languages of the phone, the preferred one first, from React Native itself (no extra native
 * module, so nothing changes in the Android and iOS projects; `expo-localization` would give the
 * whole list on Android too and the per-app language setting of Android 13, ISSUES L49).
 *
 * - iOS: `NSUserDefaults` key `AppleLanguages`, read through `Settings` (`["sv-SE", "en-SE"]`, the
 *   whole list; the app's own language setting of iOS 13 and later is written there too).
 *   `AppleLocale` when the list is empty. `I18nManager` has no `localeIdentifier` on iOS
 *   (`RCTI18nManager.mm`). `Settings` starts from the native constants and is kept current by
 *   the events of `NSUserDefaults`, so a value read after a change can be the new one.
 * - Android: `I18nManager.getConstants().localeIdentifier`, the first locale of the configuration
 *   (`sv_SE`); the app sees one entry. The constants are cached when the module loads, so a
 *   change of the phone's language is seen at the next start of the process.
 *
 * The app reads the language at start and again at every choice in the Account screen (the
 * "follow the phone" choice), not on its own when the phone's language changes; iOS is understood
 * to restart an app after such a change, which was not seen. Which entry of the list counts is the
 * rule of `i18n/language.ts` (`resolveLanguage`). Read from the source of React Native 0.88 in
 * `node_modules`; not run on a phone.
 */
export function phoneLanguageTags(): string[] {
  try {
    if (Platform.OS === "ios") {
      const list: unknown = Settings.get("AppleLanguages");
      const tags = Array.isArray(list)
        ? list.filter((tag): tag is string => typeof tag === "string")
        : [];
      if (tags.length > 0) return tags;
      const locale: unknown = Settings.get("AppleLocale");
      return typeof locale === "string" && locale !== "" ? [locale] : [];
    }
    const id = I18nManager.getConstants().localeIdentifier;
    return typeof id === "string" && id !== "" ? [id] : [];
  } catch {
    return [];
  }
}
