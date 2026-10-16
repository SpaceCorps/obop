import { afterEach, describe, expect, it } from "vite-plus/test";
import {
  argNames,
  fmt,
  getDir,
  getLang,
  hasPseudoChars,
  inSentence,
  initI18n,
  labelFor,
  parseStrict,
  pluralCases,
  provideCatalogue,
  pseudo,
  rich,
  setLanguage,
  setRegion,
  t,
  tagNames,
  tx,
  type Lang,
} from "../index.ts";

const RECORDS = "{count, plural, one {# record} other {# records}}";

async function use(lang: Lang, messages: Record<string, string> = {}) {
  provideCatalogue(lang, messages);
  await setLanguage(lang, false);
}

afterEach(async () => {
  await use("en");
  setRegion("auto");
});

describe("t", () => {
  it("returns the English text when nothing is translated", async () => {
    await initI18n({ loaders: [] });
    expect(t("Save")).toBe("Save");
    expect(t("Hello {name}", { name: "Ada" })).toBe("Hello Ada");
  });

  it("uses the catalogue, keeps unknown placeholders visible and ignores a malformed message", async () => {
    await use("de", { "Hello {name}": "Hallo {name}", "Broken {": "Kaputt {" });
    expect(t("Hello {name}", { name: "Ada" })).toBe("Hallo Ada");
    expect(t("Broken {", {})).toBe("Kaputt {");
    expect(t("Missing {x}", {} as never)).toBe("Missing {x}");
  });

  it("separates a context from the plain message", async () => {
    await use("de", { "field\u0004State": "Status", State: "Staat" });
    expect(tx("field", "State")).toBe("Status");
    expect(t("State")).toBe("Staat");
  });

  it("formats numbers inside plurals with the locale and keeps an apostrophe literal", async () => {
    await use("de", { [RECORDS]: "{count, plural, one {# Datensatz} other {# Datensätze}}" });
    expect(t(RECORDS, { count: 1234 })).toBe("1.234 Datensätze");
    await use("fr", { "It's {name}": "C'est {name}" });
    expect(t("It's {name}", { name: "x" })).toBe("C'est x");
  });

  it("quotes braces with an apostrophe", () => {
    expect(t("Use '{'name'}' here {n}", { n: 1 } as never)).toBe("Use {name} here 1");
  });

  it("selects", () => {
    const m = "{kind, select, a {Alpha} b {Beta} other {Other}}";
    expect(t(m, { kind: "b" })).toBe("Beta");
    expect(t(m, { kind: "zzz" })).toBe("Other");
  });
});

describe("plurals in every language", () => {
  const cases: [Lang, string, [number, string][]][] = [
    [
      "en",
      RECORDS,
      [
        [0, "0 records"],
        [1, "1 record"],
        [2, "2 records"],
      ],
    ],
    [
      "de",
      "{count, plural, one {# Datensatz} other {# Datensätze}}",
      [
        [0, "0 Datensätze"],
        [1, "1 Datensatz"],
        [2, "2 Datensätze"],
      ],
    ],
    [
      "fr",
      "{count, plural, one {# enregistrement} other {# enregistrements}}",
      [
        [0, "0 enregistrement"],
        [1, "1 enregistrement"],
        [2, "2 enregistrements"],
      ],
    ],
    [
      "es",
      "{count, plural, one {# registro} other {# registros}}",
      [
        [0, "0 registros"],
        [1, "1 registro"],
        [2, "2 registros"],
      ],
    ],
    [
      "sv",
      "{count, plural, one {# post} other {# poster}}",
      [
        [0, "0 poster"],
        [1, "1 post"],
        [2, "2 poster"],
      ],
    ],
    [
      "fi",
      "{count, plural, one {# tietue} other {# tietuetta}}",
      [
        [0, "0 tietuetta"],
        [1, "1 tietue"],
        [2, "2 tietuetta"],
        [11, "11 tietuetta"],
      ],
    ],
    [
      "pl",
      "{count, plural, one {# rekord} few {# rekordy} many {# rekordów} other {# rekordu}}",
      [
        [0, "0 rekordów"],
        [1, "1 rekord"],
        [2, "2 rekordy"],
        [4, "4 rekordy"],
        [5, "5 rekordów"],
        [12, "12 rekordów"],
        [13, "13 rekordów"],
        [14, "14 rekordów"],
        [21, "21 rekordów"],
        [22, "22 rekordy"],
        [24, "24 rekordy"],
        [25, "25 rekordów"],
        [112, "112 rekordów"],
        [1.5, "1,5 rekordu"],
      ],
    ],
  ];
  for (const [lang, message, rows] of cases)
    it(lang, async () => {
      await use(lang, { [RECORDS]: message });
      for (const [count, text] of rows) {
        // fr/de/pl use a non-breaking space or a dot for groups; none of these numbers needs one.
        expect(t(RECORDS, { count }), `${lang} ${count}`).toBe(text);
      }
    });

  it("an exact =0 case wins over the category", async () => {
    const m = "{count, plural, =0 {none} one {# item} other {# items}}";
    expect(t(m, { count: 0 })).toBe("none");
    expect(t(m, { count: 1 })).toBe("1 item");
  });

  it("a missing category falls back to other", async () => {
    await use("pl", { [RECORDS]: "{count, plural, one {# rekord} other {# rekordów}}" });
    expect(t(RECORDS, { count: 3 })).toBe("3 rekordów");
  });
});

describe("rich text", () => {
  it("wraps tagged text and lets a translation reorder tags and placeholders", async () => {
    await use("de", { "Delete <b>{name}</b>?": "<b>{name}</b> löschen?" });
    const out = rich("Delete <b>{name}</b>?", { name: "Acme" }, { b: (c) => ["[", c, "]"] });
    // The element tree is opaque; flatten what it renders.
    const flat = JSON.stringify(out);
    expect(flat.indexOf("Acme")).toBeLessThan(flat.indexOf("löschen"));
    expect(flat).not.toContain("<b>");
  });
});

describe("message tooling", () => {
  it("reads placeholders, plural categories and tags", () => {
    expect(argNames("{a} and {b, plural, one {{c} item} other {# items}}")).toEqual([
      "a",
      "b",
      "c",
    ]);
    expect(pluralCases("{n, plural, one {x} few {y} other {z}}")).toEqual([
      ["one", "few", "other"],
    ]);
    expect(tagNames("<b>x</b> <kbd>y</kbd>")).toEqual(["b", "kbd"]);
    expect(() => parseStrict("{n, plural, one {x}}")).toThrow();
  });
});

describe("labelFor and inSentence", () => {
  it("prefers the server's label for the language, then the base language, then the fallback", () => {
    const o = { name: "lead", labels: { de: "Interessent", "fr-CA": "Piste" } };
    expect(labelFor(o, "de", "Lead")).toBe("Interessent");
    expect(labelFor(o, "de-AT", "Lead")).toBe("Interessent");
    expect(labelFor(o, "fr", "Lead")).toBe("Lead");
    expect(labelFor({ name: "lead" }, "de", "Lead")).toBe("Lead");
    expect(labelFor(undefined, "de", "x")).toBe("x");
  });

  it("lower-cases a noun in a sentence except in German and for acronyms", async () => {
    expect(inSentence("Lead")).toBe("lead");
    expect(inSentence("CRM")).toBe("CRM");
    await use("de");
    expect(inSentence("Lead")).toBe("Lead");
  });
});

describe("pseudo-locale", () => {
  it("accents letters, grows words and leaves tags alone", () => {
    expect(pseudo("Save")).not.toBe("Save");
    expect(hasPseudoChars(pseudo("Save"))).toBe(true);
    expect(pseudo("<b>x</b>")).toContain("<b>");
    expect(pseudo("Contacts").length).toBeGreaterThan("Contacts".length);
  });

  it("translates every message through it", async () => {
    await use("en-XA");
    expect(hasPseudoChars(t("Save"))).toBe(true);
    expect(hasPseudoChars(t("Hello {name}", { name: "Ada" }))).toBe(true);
    expect(t("Hello {name}", { name: "Ada" })).toContain("Ada");
    expect(getLang()).toBe("en-XA");
    expect(getDir()).toBe("ltr");
    await use("ar-XB");
    expect(getDir()).toBe("rtl");
  });
});

describe("format locale", () => {
  it("follows the language, and the region overrides it", async () => {
    await use("de");
    expect(fmt.money(123450, "EUR")).toBe("1.234,50 €");
    setRegion("de-CH");
    expect(fmt.money(123450, "CHF")).toMatch(/^CHF\u00a01['’]234\.50$/);
    setRegion("en-US");
    expect(fmt.money(123450, "EUR")).toBe("€1,234.50");
  });
});
