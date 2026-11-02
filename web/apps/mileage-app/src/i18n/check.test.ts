import { describe, expect, test } from "vite-plus/test";
import { checkCatalogue, placeholdersOf, pluralBases, type CheckedCatalogue } from "./check.ts";
import { en, type PluralCategory, type Translation } from "./en.ts";
import { LANGUAGES } from "./languages.ts";
import { sv } from "./sv.ts";

const source: CheckedCatalogue = { code: "en", categories: ["one", "other"], messages: en };

/** The Swedish catalogue as the check gets it, with `messages` replaced for a mutation. */
const swedish = (
  messages: Record<string, string> = { ...sv },
  more: Partial<CheckedCatalogue> = {},
): CheckedCatalogue => ({
  code: "sv",
  categories: ["one", "other"],
  messages,
  identical: LANGUAGES.sv.identical,
  ...more,
});

/** A copy of Swedish with one key changed (`undefined` removes it). */
function mutated(key: string, text: string | undefined): Record<string, string> {
  const copy: Record<string, string> = { ...sv };
  if (text === undefined) delete copy[key];
  else copy[key] = text;
  return copy;
}

describe("the completeness check", () => {
  test("every language of the app is complete against English, and English against itself", () => {
    for (const language of Object.values(LANGUAGES)) {
      const problems = checkCatalogue(source, {
        code: language.code,
        categories: language.categories,
        messages: language.messages,
        identical: language.identical,
      });
      expect(problems, language.code).toEqual([]);
    }
  });

  test("the sentences with a count are found, and they are many", () => {
    const bases = pluralBases(en);
    expect(bases.has("unit.points")).toBe(true);
    expect(bases.has("home.waiting.message")).toBe(true);
    expect(bases.size).toBeGreaterThanOrEqual(15);
    // A plural form is never also a plain key of the source.
    for (const base of bases) expect(Object.keys(en)).not.toContain(base);
  });

  test("placeholders are read the way the app fills them", () => {
    expect(placeholdersOf("{b} then {a} and {a}")).toEqual(["{a}", "{a}", "{b}"]);
    expect(placeholdersOf("no placeholder")).toEqual([]);
    expect(placeholdersOf("{avstånd}")).toEqual([]);
  });

  test("the check is not a pass-through: Swedish with nothing wrong has no problems at all", () => {
    expect(checkCatalogue(source, swedish())).toEqual([]);
  });

  test("a placeholder is counted, not only named: a dropped or an added repeat is reported", () => {
    // No sentence of the English catalogue uses a name twice today, so a made-up source shows it.
    const twice: CheckedCatalogue = {
      code: "en",
      categories: ["one", "other"],
      messages: { "a.b": "{n} of {n}, then {m}" },
    };
    const target = (text: string): CheckedCatalogue => ({
      code: "xx",
      categories: ["one", "other"],
      messages: { "a.b": text },
    });
    expect(checkCatalogue(twice, target("{n} av {n}, sedan {m}"))).toEqual([]);
    expect(checkCatalogue(twice, target("{n} av alla, sedan {m}"))).toEqual([
      'xx: "a.b" has the placeholders [{m} {n}] where the source has [{m} {n} {n}]',
    ]);
    expect(checkCatalogue(twice, target("{n} av {n} och {n}, sedan {m}"))).toEqual([
      'xx: "a.b" has the placeholders [{m} {n} {n} {n}] where the source has [{m} {n} {n}]',
    ]);
  });
});

describe("mutations of the Swedish catalogue: each one is reported", () => {
  test("a deleted key", () => {
    expect(checkCatalogue(source, swedish(mutated("trip.title", undefined)))).toEqual([
      'sv: missing key "trip.title"',
    ]);
  });

  test("a deleted iOS version", () => {
    expect(checkCatalogue(source, swedish(mutated("guide.body.ios", undefined)))).toEqual([
      'sv: missing key "guide.body.ios"',
    ]);
  });

  test("a deleted plural form", () => {
    expect(checkCatalogue(source, swedish(mutated("unit.points.one", undefined)))).toEqual([
      'sv: missing plural form "unit.points.one"',
    ]);
    expect(checkCatalogue(source, swedish(mutated("unit.points.other", undefined)))).toEqual([
      'sv: missing plural form "unit.points.other"',
    ]);
  });

  test("a plural form that Swedish does not have", () => {
    const problems = checkCatalogue(source, swedish(mutated("unit.points.few", "{count} punkter")));
    expect(problems).toEqual([
      'sv: plural form "unit.points.few" is not needed (sv has one, other)',
    ]);
  });

  test("a key that English does not have, also a misspelt one", () => {
    expect(checkCatalogue(source, swedish(mutated("trip.titel", "Resa")))).toEqual([
      'sv: unknown key "trip.titel" (not in the source)',
    ]);
    const renamed = mutated("trip.title", undefined);
    renamed["trip.titel"] = "Resa";
    expect(checkCatalogue(source, swedish(renamed)).sort()).toEqual([
      'sv: missing key "trip.title"',
      'sv: unknown key "trip.titel" (not in the source)',
    ]);
  });

  test("a placeholder that is lost, renamed or added", () => {
    const lost = checkCatalogue(source, swedish(mutated("unit.trips.other", "resor")));
    expect(lost).toEqual([
      'sv: "unit.trips.other" has the placeholders [] where the source has [{count}]',
    ]);
    const renamed = checkCatalogue(source, swedish(mutated("home.last", "{sträcka}, {state}")));
    expect(renamed).toHaveLength(2); // the placeholder is gone, and the brace is stray
    const added = checkCatalogue(source, swedish(mutated("common.back", "Tillbaka {to}")));
    expect(added).toEqual([
      'sv: "common.back" has the placeholders [{to}] where the source has []',
    ]);
    // One plural form that forgot {count} is found although the other has it.
    const one = checkCatalogue(source, swedish(mutated("unit.points.one", "en punkt")));
    expect(one).toEqual([
      'sv: "unit.points.one" has the placeholders [] where the source has [{count}]',
    ]);
  });

  test("an empty text, a stray brace, a space at the end", () => {
    expect(checkCatalogue(source, swedish(mutated("common.close", "  ")))).toEqual([
      'sv: empty text for "common.close"',
    ]);
    expect(checkCatalogue(source, swedish(mutated("common.close", "Stäng }")))).toEqual([
      'sv: "common.close" has a stray brace',
    ]);
    expect(checkCatalogue(source, swedish(mutated("common.close", "Stäng ")))).toEqual([
      'sv: "common.close" starts or ends with a space',
    ]);
  });

  test("a text left in English", () => {
    expect(checkCatalogue(source, swedish(mutated("trip.title", en["trip.title"])))).toEqual([
      'sv: "trip.title" is the same as the source: untranslated, or add it to the identical list',
    ]);
    // A plural form is compared with the form of its own category.
    expect(
      checkCatalogue(source, swedish(mutated("unit.points.one", en["unit.points.one"]))),
    ).toEqual([
      'sv: "unit.points.one" is the same as the source: untranslated, or add it to the identical list',
    ]);
  });

  test("an entry of the identical list that is wrong", () => {
    const stale = swedish(undefined, { identical: [...LANGUAGES.sv.identical, "trip.title"] });
    expect(checkCatalogue(source, stale)).toEqual([
      'sv: "trip.title" is on the identical list but differs from the source',
    ]);
    const unknown = swedish(undefined, { identical: [...LANGUAGES.sv.identical, "no.such.key"] });
    expect(checkCatalogue(source, unknown)).toEqual([
      'sv: "no.such.key" is on the identical list but is not a key',
    ]);
  });

  test("the plural categories of a language must make sense", () => {
    expect(checkCatalogue(source, swedish(undefined, { categories: ["one"] }))).toContain(
      'sv: the plural categories lack "other"',
    );
    expect(
      checkCatalogue(
        source,
        swedish(undefined, { categories: ["one", "other", "lots" as PluralCategory] }),
      ),
    ).toContain('sv: unknown plural category "lots"');
  });
});

/** A made-up Polish: every text starts with a marker, the sentences with a count have one, few, many, other. */
function polish(): CheckedCatalogue {
  const messages: Record<string, string> = {};
  const bases = pluralBases(en);
  for (const [key, text] of Object.entries(en)) {
    const base = key.replace(/\.(one|other)$/, "");
    if (bases.has(base)) continue;
    messages[key] = `[pl] ${text}`;
  }
  for (const base of bases) {
    for (const category of ["one", "few", "many", "other"]) {
      messages[`${base}.${category}`] =
        `[pl ${category}] ${(en as Record<string, string>)[`${base}.other`]}`;
    }
  }
  return { code: "pl", categories: ["one", "few", "many", "other"], messages };
}

describe("a language with more plural forms can be added", () => {
  test("a Polish catalogue with few and many passes the check that knows them", () => {
    expect(checkCatalogue(source, polish())).toEqual([]);
  });

  test("the same catalogue fails as Swedish: few and many are not needed there", () => {
    const problems = checkCatalogue(source, {
      ...polish(),
      code: "sv",
      categories: ["one", "other"],
    });
    expect(problems).toContain(
      'sv: plural form "unit.points.few" is not needed (sv has one, other)',
    );
    expect(problems).toContain(
      'sv: plural form "unit.points.many" is not needed (sv has one, other)',
    );
  });

  test("Swedish fails as Polish: few and many are missing for every sentence with a count", () => {
    const problems = checkCatalogue(
      source,
      swedish(undefined, { code: "pl", categories: ["one", "few", "many", "other"] }),
    );
    const bases = pluralBases(en);
    expect(problems.filter((p) => p.includes("missing plural form"))).toHaveLength(bases.size * 2);
    expect(problems).toContain('pl: missing plural form "unit.points.few"');
  });

  test("a Polish catalogue that forgets one form is reported", () => {
    const messages = { ...polish().messages };
    delete messages["unit.trips.many"];
    expect(checkCatalogue(source, { ...polish(), messages })).toEqual([
      'pl: missing plural form "unit.trips.many"',
    ]);
  });
});

describe("the types check the catalogue when the compiler runs (`vp check`)", () => {
  test("a catalogue must have every key, and only the keys of English", () => {
    // @ts-expect-error: one key is not a catalogue; the compiler names the missing ones
    const tooSmall: Translation<"one"> = { "common.back": "Tillbaka" };
    // @ts-expect-error: Swedish has no few and many, so it is not a Polish catalogue
    const notPolish: Translation<"one" | "few" | "many"> = sv;
    // @ts-expect-error: a key that English does not have
    const extra: Translation<"one"> = { ...sv, "trip.titel": "Resa" };
    const ok: Translation<"one"> = sv;
    // The assignments above only matter to the compiler; the values are not read.
    expect([tooSmall, notPolish, extra, ok]).toHaveLength(4);
  });
});
