import { I18nManager, Platform, Settings } from "react-native";

/**
 * The languages of the phone, the preferred one first, from React Native itself (no extra native
 * module, so nothing changes in the Android and iOS projects; `expo-localization` would give the
 * whole list on both systems and the per-app language settings, ISSUES L49).
 *
 * - iOS: `NSUserDefaults` key `AppleLanguages`, read through `Settings` (`["sv-SE", "en-SE"]`;
 *   the app's own language setting of iOS 13 and later is written there too). `AppleLocale` when
 *   the list is empty. `I18nManager` has no `localeIdentifier` on iOS (`RCTI18nManager.mm`).
 * - Android: `I18nManager.getConstants().localeIdentifier`, the first locale of the configuration
 *   (`sv_SE`); the app sees one entry.
 *
 * Both values are read once when the app's process starts (the constants of the native modules), so
 * a change of the phone's language is seen at the next start of the app. Read from the source of
 * React Native 0.88 in `node_modules`; not run on a phone.
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
