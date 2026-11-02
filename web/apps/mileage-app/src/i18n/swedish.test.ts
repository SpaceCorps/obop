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
    });
  }
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

  test("without Intl the number still has the Swedish symbols, and the date is still text", () => {
    vi.stubGlobal("Intl", {});
    setLanguage("sv");
    expect(formatNumber(12.34, 1)).toBe("12,3");
    expect(formatNumber(12345.67, 1)).toBe("12 345,7");
    expect(formatNumber(-1234.5, 1)).toBe("-1 234,5");
    expect(formatNumber(1000)).toBe("1 000");
    expect(formatNumber(5)).toBe("5");
    expect(tp("unit.points", 3)).toBe("3 punkter");
    expect(tp("unit.points", 1)).toBe("1 punkt");
    expect(formatDateTime("2026-10-15T09:00:00Z")).toMatch(/2026/);
    setLanguage("en");
    expect(formatNumber(12345.67, 1)).toBe("12,345.7");
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
