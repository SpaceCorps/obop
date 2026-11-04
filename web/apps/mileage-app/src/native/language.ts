import { createLanguageStore } from "../i18n/language.ts";
import { setLanguage } from "../i18n/index.ts";
import { LANGUAGE_KEY_NAME } from "./config.ts";
import { keychain } from "./keychain.ts";
import { phoneLanguageTags } from "./phone-language.ts";

/**
 * The language of the app: the phone's language until the driver chooses one in the Account screen,
 * then the choice. The choice is stored under one key of the secure store (`keychain.ts`): a
 * setting, not a secret, but the secure store is the one place the app keeps a value outside the
 * encrypted database, and the choice is wanted before the database opens (the loading screen and
 * the screen "Failed" are shown without it). `index.ts` of the app starts the store when the app
 * loads and does not wait for it: the phone's language is used from the first render until the
 * stored choice has been read, which is one read of the secure store. A Swedish choice on an
 * English phone can therefore show the loading screen in English for that moment (not measured).
 */
export const languageStore = createLanguageStore({
  phone: phoneLanguageTags,
  load: () => keychain.get(LANGUAGE_KEY_NAME),
  save: (text) => keychain.set(LANGUAGE_KEY_NAME, text),
  apply: setLanguage,
});
