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
 * The language to show. A chosen language is used as it is. Following the phone, the phone's first
 * language decides: Swedish or English when the phone is set to one of them, otherwise English. (The
 * phone's list has more entries on iOS; Android shows only the first one to the app, and the rule
 * is the same on both.)
 */
export function resolveLanguage(choice: Choice, phone: readonly string[]): LanguageCode {
  if (choice !== "system") return choice;
  const first = phone.find((tag) => tag.trim() !== "");
  return languageOfTag(first) ?? "en";
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
  /** Applies the phone's language at once, then the stored choice when it has been read. */
  start: () => Promise<void>;
  /** Applies the choice at once and stores it. Answers whether it was stored (it is applied either way). */
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
