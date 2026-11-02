import { PLURAL_CATEGORIES, type PluralCategory } from "./en.ts";

/**
 * The completeness check of a catalogue against the English source. The tests run it for every
 * language of `languages.ts`; nothing in the app imports it, so it is not in the bundle.
 *
 * `Translation<...>` in `en.ts` makes the compiler report a missing or an extra key. This check
 * says what a type cannot: a placeholder that differs from the source, an empty text, a text with
 * a stray brace, a text that was left the same as the English one, and it knows the plural
 * categories of each language, so a language with `few` and `many` (Polish) is checked for those
 * forms and a language without them for their absence.
 */
export interface CheckedCatalogue {
  code: string;
  /** The plural categories the language has; `other` must be among them. */
  categories: readonly PluralCategory[];
  messages: Readonly<Record<string, string>>;
  /** Keys whose text may be the same as the source's. Only used for a language other than the source's. */
  identical?: readonly string[];
}

const CATEGORY_SUFFIX = new RegExp(`\\.(${PLURAL_CATEGORIES.join("|")})$`);

/** The `{name}` placeholders of a text, sorted (a name twice is listed twice). */
export function placeholdersOf(text: string): string[] {
  return (text.match(/\{\w+\}/g) ?? []).sort();
}

/** The names `x` for which the catalogue has `x.other`: the sentences that depend on a count. */
export function pluralBases(messages: Readonly<Record<string, string>>): Set<string> {
  const bases = new Set<string>();
  for (const key of Object.keys(messages)) {
    if (key.endsWith(".other")) bases.add(key.slice(0, -".other".length));
  }
  return bases;
}

/** `[base, category]` when the key is a plural form of one of the bases, else `null`. */
function formOf(key: string, bases: ReadonlySet<string>): [string, PluralCategory] | null {
  const match = CATEGORY_SUFFIX.exec(key);
  if (!match) return null;
  const base = key.slice(0, match.index);
  return bases.has(base) ? [base, match[1] as PluralCategory] : null;
}

/**
 * Everything wrong with `target` as a translation of `source`, one sentence per problem; an empty
 * list means complete. Checking a catalogue against itself checks its own plural forms.
 */
export function checkCatalogue(source: CheckedCatalogue, target: CheckedCatalogue): string[] {
  const problems: string[] = [];
  const say = (text: string) => problems.push(`${target.code}: ${text}`);

  if (!target.categories.includes("other")) say(`the plural categories lack "other"`);
  for (const category of target.categories) {
    if (!PLURAL_CATEGORIES.includes(category)) say(`unknown plural category "${category}"`);
  }

  const bases = pluralBases(source.messages);
  const expected = new Map<string, string>(); // key of the target -> key of the source to compare with
  for (const key of Object.keys(source.messages)) {
    if (formOf(key, bases) === null) expected.set(key, key);
  }
  for (const base of bases) {
    for (const category of new Set([...target.categories, "other" as const])) {
      expected.set(`${base}.${category}`, `${base}.other`);
    }
  }

  for (const [key, reference] of expected) {
    const text = target.messages[key];
    if (text === undefined) {
      say(formOf(key, bases) ? `missing plural form "${key}"` : `missing key "${key}"`);
      continue;
    }
    // A plural form is compared with the source's form of the same category when it has one
    // (`one` with `one`), else with `other`.
    const sourceText = source.messages[key] ?? source.messages[reference]!;
    if (text.trim() === "") {
      say(`empty text for "${key}"`);
      continue;
    }
    if (text !== text.trim()) say(`"${key}" starts or ends with a space`);
    const own = placeholdersOf(text).join(" ");
    const wanted = placeholdersOf(source.messages[reference]!).join(" ");
    if (own !== wanted) {
      say(`"${key}" has the placeholders [${own}] where the source has [${wanted}]`);
    }
    if (/[{}]/.test(text.replace(/\{\w+\}/g, ""))) say(`"${key}" has a stray brace`);
    if (
      target.code !== source.code &&
      text === sourceText &&
      !(target.identical ?? []).includes(key)
    ) {
      say(`"${key}" is the same as the source: untranslated, or add it to the identical list`);
    }
  }

  for (const key of Object.keys(target.messages)) {
    if (expected.has(key)) continue;
    const form = formOf(key, bases);
    say(
      form
        ? `plural form "${key}" is not needed (${target.code} has ${target.categories.join(", ")})`
        : `unknown key "${key}" (not in the source)`,
    );
  }

  if (target.code !== source.code) {
    for (const key of target.identical ?? []) {
      if (expected.has(key) && target.messages[key] !== source.messages[key]) {
        say(`"${key}" is on the identical list but differs from the source`);
      }
      if (!expected.has(key)) say(`"${key}" is on the identical list but is not a key`);
    }
  }

  return problems.sort();
}
