import { createObopClient } from "@obop/client";
import { afterEach, describe, expect, test, vi } from "vite-plus/test";
import {
  deviceStateText,
  formatDistance,
  integrityLines,
  unsentSentence,
} from "../core/present.ts";
import {
  formatDateTime,
  formatNumber,
  getLanguage,
  onLanguageChange,
  plainNumber,
  setLanguage,
  t,
  tOn,
  tp,
} from "./index.ts";
import { LANGUAGES } from "./languages.ts";

describe("the plural tables and number symbols are the rules of CLDR", () => {
  // Node has the CLDR data in `Intl`; the app does not rely on it (Hermes lacks PluralRules), so the
  // tables of `languages.ts` are compared with it here, where it is available.
  for (const language of Object.values(LANGUAGES)) {
    test(`${language.code}: the categories and plural() agree with Intl.PluralRules`, () => {
      const rules = new Intl.PluralRules(language.tag);
      expect([...rules.resolvedOptions().pluralCategories].sort()).toEqual(
        [...language.categories].sort(),
      );
      for (let count = 0; count <= 1000; count++) {
        expect(language.plural(count), String(count)).toBe(rules.select(count));
      }
      for (const count of [-1, 0.5, 1.5, 2.5, 11.5]) {
        expect(language.plural(count), String(count)).toBe(rules.select(count));
      }
    });

    test(`${language.code}: the decimal and group symbols agree with Intl.NumberFormat`, () => {
      const parts = new Intl.NumberFormat(language.tag).formatToParts(12345.5);
      const part = (type: string) => parts.find((p) => p.type === type)?.value;
      expect(part("decimal")).toBe(language.symbols.decimal);
      expect(part("group")).toBe(language.symbols.group);
      const negative = new Intl.NumberFormat(language.tag).formatToParts(-1);
      expect(negative.find((p) => p.type === "minusSign")?.value).toBe(language.symbols.minus);
    });
  }

  test("the names of the languages, as the language choice shows them, are their own", () => {
    // Nothing else in the app says these: a driver on an English phone looks for "Svenska".
    expect(LANGUAGES.sv.name).toBe("Svenska");
    expect(LANGUAGES.en.name).toBe("English");
    for (const language of Object.values(LANGUAGES)) {
      expect(language.name, language.code).not.toBe(language.messages["language.system"]);
    }
  });
});

describe("the active language", () => {
  afterEach(() => {
    setLanguage("en");
    vi.unstubAllGlobals();
  });

  test("is English until it is set", () => {
    expect(getLanguage()).toBe("en");
    expect(t("trip.title")).toBe("Trip");
  });

  test("changes what t and tOn say, and back again", () => {
    setLanguage("sv");
    expect(getLanguage()).toBe("sv");
    expect(t("trip.title")).toBe("Resa");
    expect(t("home.open.message", { when: "idag" })).toBe(
      "Startade idag. Stoppa den eller kassera den.",
    );
    expect(tOn("ios", "guide.body")).toContain("appväxlaren");
    expect(tOn("android", "guide.body")).toContain("Obegränsad");
    expect(tOn("ios", "recording.stop")).toBe("Stopp");
    setLanguage("en");
    expect(t("trip.title")).toBe("Trip");
    expect(tOn("ios", "guide.body")).toContain("app switcher");
  });

  test("tells the listeners once per change, not for the same language, and stops when asked", () => {
    const heard: string[] = [];
    const stop = onLanguageChange(() => heard.push(getLanguage()));
    setLanguage("sv");
    setLanguage("sv");
    setLanguage("en");
    expect(heard).toEqual(["sv", "en"]);
    stop();
    setLanguage("sv");
    expect(heard).toEqual(["sv", "en"]);
  });

  test("a key the active catalogue lacks is read from English, never undefined", () => {
    setLanguage("sv");
    const messages = LANGUAGES.sv.messages as Record<string, string>;
    const saved = messages["trip.title"];
    delete messages["trip.title"];
    try {
      expect(t("trip.title")).toBe("Trip");
    } finally {
      messages["trip.title"] = saved!;
    }
  });
});

describe("Swedish sentences with a count", () => {
  afterEach(() => setLanguage("en"));

  test("one and other, and 0 is other", () => {
    setLanguage("sv");
    expect(tp("unit.trips", 1)).toBe("1 resa");
    expect(tp("unit.trips", 2)).toBe("2 resor");
    expect(tp("unit.trips", 0)).toBe("0 resor");
    expect(tp("recording.points", 1)).toBe("1 punkt sparad på den här telefonen");
    expect(tp("recording.points", 5)).toBe("5 punkter sparade på den här telefonen");
    expect(tp("recording.quiet.message", 1)).toContain("är 1 minut gammal");
    expect(tp("recording.quiet.message", 4)).toContain("är 4 minuter gammal");
    expect(tp("home.waiting.message", 1)).toBe(
      "1 resa finns på den här telefonen men inte på servern än. Den är säker.",
    );
    expect(tp("home.waiting.message", 3)).toBe(
      "3 resor finns på den här telefonen men inte på servern än. De är säkra.",
    );
  });

  test("the count is written the way Swedish writes numbers", () => {
    setLanguage("sv");
    expect(tp("unit.points", 12345)).toBe("12 345 punkter");
  });

  test("a trip count and a point count read as one sentence in Swedish", () => {
    setLanguage("sv");
    expect(unsentSentence("account.leave.pending.message", { trips: 1, points: 6 })).toBe(
      "1 resa, med en rutt på 6 punkter, finns inte helt på servern. Om du behåller den laddas den upp nästa gång du loggar in på den här servern med samma konto. Ingen som loggar in med ett annat konto kan se eller ladda upp den.",
    );
    expect(unsentSentence("foreign.message", { trips: 2, points: 96 })).toContain(
      "2 resor (rutter på sammanlagt 96 punkter) från en annan inloggning",
    );
    expect(unsentSentence("foreign.message", { trips: 1, points: 1 })).toContain(
      "1 resa (en rutt på 1 punkt) från en annan inloggning",
    );
  });

  test("the checks of the server are Swedish sentences, an unknown flag stays as it is", () => {
    setLanguage("sv");
    expect(integrityLines("seq_gap:1 low_accuracy:3 future_flag:2")).toEqual([
      "1 lucka i numreringen av punkterna",
      "3 punkter uteslöts på grund av låg noggrannhet",
      "future_flag:2",
    ]);
  });

  test("the state of the phone on the instance is a word, an unknown state is shown as it is", () => {
    expect(deviceStateText("active")).toBe("Active");
    setLanguage("sv");
    expect(deviceStateText("active")).toBe("Aktiv");
    expect(deviceStateText("revoked")).toBe("Borttagen");
    expect(deviceStateText("suspended")).toBe("suspended");
    expect(deviceStateText("")).toBe("");
  });
});

describe("Swedish numbers, distances and dates through Intl", () => {
  afterEach(() => {
    setLanguage("en");
    vi.unstubAllGlobals();
  });

  test("a number has a decimal comma and a no-break space between thousands", () => {
    setLanguage("sv");
    expect(formatNumber(1234.56, 1)).toBe("1 234,6");
    expect(formatNumber(12345)).toBe("12 345");
    expect(formatNumber(0.5, 1)).toBe("0,5");
    setLanguage("en");
    expect(formatNumber(1234.56, 1)).toBe("1,234.6");
  });

  test("a distance has the unit after a no-break space", () => {
    setLanguage("sv");
    expect(formatDistance("12345.67")).toBe("12,3 km");
    expect(formatDistance("999.4")).toBe("999 m");
    expect(formatDistance("1000")).toBe("1 km");
  });

  test("a date and time are written in Swedish, 24 hours", () => {
    setLanguage("sv");
    // 09:00 UTC is the 15th in every time zone from UTC-9 to UTC+14, so the day does not depend on
    // the machine; the hour does, and is only checked as a 24-hour clock.
    expect(formatDateTime("2026-10-15T09:00:00Z")).toMatch(/^15 okt\.? 2026,? \d{2}:\d{2}$/);
    setLanguage("en");
    expect(formatDateTime("2026-10-15T09:00:00Z")).toMatch(
      /^Oct 15, 2026,? \d{1,2}:\d{2}\s?[AP]M$/,
    );
  });

  test("without Intl the number still has the Swedish symbols, and the date is by hand", () => {
    vi.stubGlobal("Intl", {});
    setLanguage("sv");
    expect(formatNumber(12.34, 1)).toBe("12,3");
    expect(formatNumber(12345.67, 1)).toBe("12 345,7");
    expect(formatNumber(-1234.5, 1)).toBe("−1 234,5");
    expect(formatNumber(1000)).toBe("1 000");
    expect(formatNumber(5)).toBe("5");
    expect(tp("unit.points", 3)).toBe("3 punkter");
    expect(tp("unit.points", 1)).toBe("1 punkt");
    // Written by hand, in the time zone of the machine: only the day is stable across zones.
    expect(formatDateTime("2026-10-15T09:00:00Z")).toMatch(/^2026-10-15 \d{2}:\d{2}$/);
    setLanguage("en");
    expect(formatNumber(12345.67, 1)).toBe("12,345.7");
    expect(formatNumber(-0.5, 1)).toBe("-0.5");
  });

  test("an Intl without data for the language does not leak English into a Swedish app", () => {
    // What a valid tag with no data does in an engine: no error, the default locale (en-US).
    const resolved = { locale: "en-US" };
    vi.stubGlobal("Intl", {
      NumberFormat: class {
        resolvedOptions = () => resolved;
        format = (value: number) => `english ${value}`;
      },
      DateTimeFormat: class {
        resolvedOptions = () => resolved;
        format = () => "Oct 15, 2026, 9:00 AM";
      },
    });
    setLanguage("sv");
    expect(formatNumber(12345.67, 1)).toBe("12 345,7");
    expect(tp("unit.points", 2500)).toBe("2 500 punkter");
    expect(formatDateTime("2026-10-15T09:00:00Z")).toMatch(/^2026-10-15 \d{2}:\d{2}$/);
    // English has its data, so the same Intl is used as it is.
    setLanguage("en");
    expect(formatNumber(12345.67, 1)).toBe("english 12345.67");
    expect(formatDateTime("2026-10-15T09:00:00Z")).toBe("Oct 15, 2026, 9:00 AM");
  });

  test("an Intl that resolves to a region of the language is used (sv-SE for sv)", () => {
    vi.stubGlobal("Intl", {
      NumberFormat: class {
        resolvedOptions = () => ({ locale: "sv-SE" });
        format = () => "from Intl";
      },
      DateTimeFormat: class {
        resolvedOptions = () => ({ locale: "sv-SE" });
        format = () => "date from Intl";
      },
    });
    setLanguage("sv");
    expect(formatNumber(1)).toBe("from Intl");
    expect(formatDateTime("2026-10-15T09:00:00Z")).toBe("date from Intl");
  });

  test("an Intl that does not say what it resolved to is trusted", () => {
    vi.stubGlobal("Intl", {
      NumberFormat: class {
        resolvedOptions = () => ({});
        format = () => "from Intl";
      },
    });
    setLanguage("sv");
    expect(formatNumber(1)).toBe("from Intl");
  });

  test("a value that is not a date is returned as it was given, never as 'Invalid Date'", () => {
    setLanguage("sv");
    expect(formatDateTime("not a date")).toBe("not a date");
    expect(formatDateTime("")).toBe("");
    vi.stubGlobal("Intl", {});
    expect(formatDateTime("not a date")).toBe("not a date");
    expect(formatDateTime("")).toBe("");
  });

  test("the number by hand writes what Intl writes, for the numbers the app shows", () => {
    // Compared with the Intl of Node (ICU), where its data is available: rounding half away from
    // zero on the decimal value, grouping, the language's own symbols and minus sign.
    const values = [
      0, 5, 0.5, 0.15, 1.45, 8.35, 12.34, 12.35, 12.99, 2.675, 1.005, 999.4, 999.5, 1000, 1234.56,
      12345.67, 123456.789, 1234567.5, 20000, 99999.95, -0.5, -8.35, -1234.5, -1000,
    ];
    for (const code of ["en", "sv"] as const) {
      for (const digits of [0, 1, 2]) {
        for (const value of values) {
          const expected = new Intl.NumberFormat(LANGUAGES[code].tag, {
            maximumFractionDigits: digits,
          }).format(value);
          expect(plainNumber(value, digits, LANGUAGES[code]), `${code} ${value} ${digits}`).toBe(
            expected,
          );
        }
      }
    }
  });

  test("the number by hand for numbers the app never shows", () => {
    expect(plainNumber(Number.NaN, 1, LANGUAGES.sv)).toBe("NaN");
    expect(plainNumber(Number.POSITIVE_INFINITY, 1, LANGUAGES.sv)).toBe("Infinity");
    expect(plainNumber(1e21, 0, LANGUAGES.sv)).toBe("1e+21");
    // A negative that rounds to nothing has no sign (Intl writes "-0", which no screen wants).
    expect(plainNumber(-0.04, 1, LANGUAGES.sv)).toBe("0");
  });

  test("without Intl.PluralRules Swedish still has its two forms", () => {
    vi.stubGlobal("Intl", { ...Intl, PluralRules: undefined });
    setLanguage("sv");
    expect(tp("unit.points", 1)).toBe("1 punkt");
    expect(tp("unit.points", 2)).toBe("2 punkter");
    expect(tp("unit.points", 0)).toBe("0 punkter");
  });
});

describe("the language the server is told", () => {
  afterEach(() => setLanguage("en"));

  test("the client asks for it at every call, so a change reaches the next call", async () => {
    const seen: (string | null)[] = [];
    const client = createObopClient({
      baseUrl: "https://acme.example.com",
      getToken: () => "token",
      // The wiring of `native/runtime.ts` and `native/auth.ts`: the function itself.
      getLanguage,
      fetch: (async (request: Request) => {
        seen.push(request.headers.get("accept-language"));
        return new Response(JSON.stringify({ value: [] }), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }) as typeof fetch,
    });
    await client.call("mileage_trip", "list");
    setLanguage("sv");
    await client.call("mileage_trip", "list");
    setLanguage("en");
    await client.call("mileage_trip", "list");
    expect(seen).toEqual(["en", "sv", "en"]);
  });
});
