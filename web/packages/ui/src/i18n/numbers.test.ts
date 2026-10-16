import { describe, expect, it } from "vite-plus/test";
import {
  LANG_INFO,
  REGIONS,
  currencyExponent,
  formatMoney,
  formatPlainDate,
  formatRelative,
  minorToDecimal,
  moneyInputText,
  parseMoneyIn as parseMoney,
  parseNumber,
  separators,
} from "../index.ts";

const LOCALES = [...new Set([...Object.values(LANG_INFO).map((l) => l.format), ...REGIONS])];
const AMOUNTS = [0, 1, 5, 99, 100, 1250, 12345, 100000, 123456789, 99999999999, -1250, -5];

describe("minor units", () => {
  it("builds exact decimal strings", () => {
    expect(minorToDecimal(1250, 2)).toBe("12.50");
    expect(minorToDecimal(5, 2)).toBe("0.05");
    expect(minorToDecimal(-5, 2)).toBe("-0.05");
    expect(minorToDecimal(1234, 0)).toBe("1234");
    expect(minorToDecimal(1234, 3)).toBe("1.234");
  });

  it("knows the exponent of a currency", () => {
    expect(currencyExponent("EUR")).toBe(2);
    expect(currencyExponent("JPY")).toBe(0);
    expect(currencyExponent("BHD")).toBe(3);
  });
});

describe("money round trips in every locale", () => {
  for (const loc of LOCALES) {
    for (const cur of ["EUR", "USD", "CHF", "PLN", "SEK", "JPY"]) {
      it(`${loc} ${cur}: edit text and displayed text parse back to the same minor units`, () => {
        for (const minor of AMOUNTS) {
          const edit = moneyInputText(minor, cur, loc);
          expect(parseMoney(edit, cur, loc)?.minor, `${loc} ${cur} edit "${edit}"`).toBe(minor);
          const shown = formatMoney(minor, cur, loc);
          const back = parseMoney(shown, cur, loc);
          expect(back?.minor, `${loc} ${cur} shown "${shown}"`).toBe(minor);
        }
      });
    }
  }
});

describe("typed amounts", () => {
  const ok = (text: string, locale: string, minor: number, ambiguous = false) => {
    const r = parseMoney(text, "EUR", locale);
    expect(r, `${locale} "${text}"`).not.toBeNull();
    expect(r!.minor, `${locale} "${text}"`).toBe(minor);
    expect(r!.ambiguous, `${locale} "${text}" ambiguous`).toBe(ambiguous);
  };

  it("reads a decimal comma in the locales that use one", () => {
    for (const loc of ["de-DE", "fr-FR", "pl-PL", "fi-FI", "sv-SE", "es-ES"]) {
      ok("12,50", loc, 1250);
      ok("0,05", loc, 5);
      ok("-12,50", loc, -1250);
      ok("12", loc, 1200);
      ok(" 12,5 € ", loc, 1250);
    }
  });

  it("reads a decimal point everywhere, too (the numpad)", () => {
    for (const loc of LOCALES) ok("12.50", loc, 1250);
  });

  it("reads group marks of each locale", () => {
    ok("1.250,00", "de-DE", 125000);
    ok("1.250,00", "es-ES", 125000);
    ok("1,250.00", "en-US", 125000);
    ok("1 250,50", "fr-FR", 125050);
    ok("1 250,50", "fr-FR", 125050);
    ok("1 250,50", "pl-PL", 125050);
    ok("1 250,50", "fi-FI", 125050);
    ok("1'250.50", "de-CH", 125050);
    ok("1’250.50", "de-CH", 125050);
    ok("1.234.567,89", "de-DE", 123456789);
  });

  it("treats three digits after a group mark as a group, and shows the reading when unsure", () => {
    ok("1.250", "de-DE", 125000);
    ok("1,250", "en-US", 125000);
    // The mark is not the locale's group mark: still read as thousands, but flagged for a preview.
    ok("1,250", "de-DE", 125000, true);
    ok("1.250", "fr-FR", 125000, true);
    ok("1.250", "de-CH", 125000, true);
    ok("12,505", "de-DE", 1250500, true);
  });

  it("normalises full-width digits and the Unicode minus", () => {
    ok("１２，５０", "de-DE", 1250);
    ok("−12,50", "de-DE", -1250);
  });

  it("rejects what is not an amount", () => {
    for (const bad of ["", " ", "abc", "12,5,5", "12,5055", "1,2,3", "12.34.56,7", "--1", "1 2"]) {
      expect(parseMoney(bad, "EUR", "de-DE"), `"${bad}"`).toBeNull();
    }
    expect(parseMoney("12,5", "JPY", "de-DE")).toBeNull();
  });

  it("takes three decimals for a currency that has them", () => {
    expect(parseMoney("1,234", "BHD", "de-DE")?.minor).toBe(1234);
  });

  it("parses plain numbers with a limit on decimals", () => {
    expect(parseNumber("1.5", "en-US", 6)).toMatchObject({ int: "1", frac: "5" });
    expect(parseNumber("1,5", "de-DE", 6)).toMatchObject({ int: "1", frac: "5" });
    expect(parseNumber("0,500", "de-DE", 6)).toMatchObject({ int: "0", frac: "500" });
    expect(parseNumber("1,2345678", "de-DE", 6)).toBeNull();
  });
});

describe("separators and dates", () => {
  it("reports the separators of a locale", () => {
    expect(separators("de-DE")).toEqual({ decimal: ",", group: "." });
    expect(separators("en-US")).toEqual({ decimal: ".", group: "," });
    expect(separators("pl-PL").decimal).toBe(",");
  });

  it("formats a plain date without shifting it by a time zone", () => {
    expect(formatPlainDate("2026-03-05", "en-US")).toBe("Mar 5, 2026");
    expect(formatPlainDate("2026-03-05", "de-DE")).toMatch(/^5\. März 2026$/);
    expect(formatPlainDate("2026-03-05", "fi-FI")).toMatch(/^5\.3\.2026$/);
    expect(formatPlainDate("2026-13-45", "en-US")).toBe("2026-13-45");
    expect(formatPlainDate(null, "en-US")).toBe("—");
  });

  it("formats relative times", () => {
    const now = Date.UTC(2026, 2, 5, 12);
    expect(formatRelative(now - 3 * 86400_000, "en-US", now)).toBe("3 days ago");
    expect(formatRelative(now - 3 * 86400_000, "de-DE", now)).toBe("vor 3 Tagen");
    expect(formatRelative(now + 2 * 3600_000, "fr-FR", now)).toBe("dans 2 heures");
  });
});
