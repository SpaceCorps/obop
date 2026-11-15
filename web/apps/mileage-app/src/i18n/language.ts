import { LANGUAGE_CODES, type LanguageCode } from "./languages.ts";

/**
 * The choice of the language (docs/spec/mobile.md 8.9): follow the phone (the default), or one of
 * the languages of the app. Pure logic with the phone behind ports, so it runs in `vp test`; the
 * phone's side is `native/language.ts`.
 */
export type Choice = "system" | LanguageCode;

/** In the order the language choice shows them. */
export const CHOICES: readonly Choice[] = ["system", ...LANGUAGE_CODES];

/** A stored text as a choice; anything that is not one of the choices means "follow the phone". */
export function parseChoice(text: unknown): Choice {
  return CHOICES.find((choice) => choice === text) ?? "system";
}

/**
 * The language of a tag the phone reports: `sv`, `sv-SE`, `sv_SE` (Android), `SV_SE_#Latn`. `null`
 * when the app has no catalogue for it.
 */
export function languageOfTag(tag: unknown): LanguageCode | null {
  if (typeof tag !== "string") return null;
  const primary = tag
    .trim()
    .split(/[-_.@#]/)[0]
    ?.toLowerCase();
  return LANGUAGE_CODES.find((code) => code === primary) ?? null;
}

/**
 * The first language of the phone's list that the app has a catalogue for, or `null`. This is, from
 * Apple's documentation and general knowledge and not seen on a phone, the rule iOS applies itself
 * when it picks the `.lproj` of an app (the first preferred language the app is localised in), so
 * on an iPhone set to German and then Swedish the app and the system's permission dialogs are
 * expected to be Swedish both. On Android the list has one entry, because React Native 0.88
 * reports only the first locale of the configuration (`I18nManagerModule.kt` reads
 * `getLocales(...)[0]`), not because Android has only one: the rule has nothing to choose between
 * there until the list is read in full (ISSUES L49).
 */
export function supportedPhoneLanguage(phone: readonly string[]): LanguageCode | null {
  for (const tag of phone) {
    const code = languageOfTag(tag);
    if (code !== null) return code;
  }
  return null;
}

/**
 * The language to show. A chosen language is used as it is. Following the phone, it is the first
 * language of the phone's list that the app has (Swedish or English), otherwise English.
 */
export function resolveLanguage(choice: Choice, phone: readonly string[]): LanguageCode {
  if (choice !== "system") return choice;
  return supportedPhoneLanguage(phone) ?? "en";
}

/**
 * The language the app positively knows the person wants: the chosen one, or, following the phone,
 * the phone's FIRST language when the app has a catalogue for it. `null` otherwise: the phone is
 * set to a language the app does not have (German, say), so the app shows English because it has
 * nothing else, and that says nothing about what the person reads. The sign-in and consent pages of
 * the instance (which exist in more languages than the app) are asked for a language only when
 * this is not `null`; otherwise the browser's own language decides, as it did before the app
 * had a language choice (docs/spec/mobile.md 8.9, `native/auth.ts`).
 */
export function knownLanguage(choice: Choice, phone: readonly string[]): LanguageCode | null {
  if (choice !== "system") return choice;
  return languageOfTag(phone.find((tag) => tag.trim() !== ""));
}

/** What the store needs from the phone. */
export interface LanguagePorts {
  /** The phone's languages, the preferred one first. May throw or be empty. */
  phone(): readonly string[];
  /** The stored choice (text), or `null`. May reject. */
  load(): Promise<string | null>;
  save(text: string): Promise<void>;
  /** Makes the language the one the sentences, numbers and dates use (`setLanguage` of `index.ts`). */
  apply(code: LanguageCode): void;
}

export interface LanguageStore {
  // Properties, not methods: the hooks hand them to `useSyncExternalStore` unbound.
  choice: () => Choice;
  language: () => LanguageCode;
  /**
   * `knownLanguage` of the current choice and the phone. `null` when the phone's FIRST language is
   * not one the app has: the app may still show Swedish or English because a later language of the
   * list matches (`language()` says which), or English because nothing does. The browser then
   * decides the language of the instance's sign-in pages.
   */
  known: () => LanguageCode | null;
  /** Applies the phone's language at once, then the stored choice when it has been read. */
  start: () => Promise<void>;
  /** Applies the choice at once and stores it. Answers whether it was stored (it is applied either way); the Account screen tells the person when it was not. */
  choose: (choice: Choice) => Promise<boolean>;
  /** `listener` is called when the choice or the language changed. */
  subscribe: (listener: () => void) => () => void;
}

export function createLanguageStore(ports: LanguagePorts): LanguageStore {
  let choice: Choice = "system";
  let language: LanguageCode = "en";
  // A choice made while the stored one is still being read wins over the stored one.
  let chosen = false;
  const listeners = new Set<() => void>();

  const phone = (): readonly string[] => {
    try {
      return ports.phone();
    } catch {
      return [];
    }
  };

  const update = (next: Choice) => {
    const before = [choice, language];
    choice = next;
    language = resolveLanguage(choice, phone());
    ports.apply(language);
    if (before[0] !== choice || before[1] !== language) {
      for (const listener of listeners) listener();
    }
  };

  return {
    choice: () => choice,
    language: () => language,
    known: () => knownLanguage(choice, phone()),
    async start() {
      update(choice);
      let stored: string | null = null;
      try {
        stored = await ports.load();
      } catch {
        // Unreadable: the app follows the phone.
      }
      if (!chosen && stored !== null) update(parseChoice(stored));
    },
    async choose(next) {
      chosen = true;
      update(next);
      try {
        await ports.save(next);
        return true;
      } catch {
        return false;
      }
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
  };
}
