import { afterEach, describe, expect, test, vi } from "vite-plus/test";
import { formatDateTime, formatNumber, t, tOn, tp } from "./index.ts";
import { en } from "./en.ts";
import { formatDistance, integrityLines, unsentSentence } from "../core/present.ts";

describe("sentences with a count", () => {
  test("one and many are different sentences", () => {
    expect(tp("home.waiting.message", 1)).toBe(
      "1 trip is on this phone and not on the server yet. It is safe.",
    );
    expect(tp("home.waiting.message", 3)).toBe(
      "3 trips are on this phone and not on the server yet. They are safe.",
    );
    expect(tp("recording.points", 1)).toBe("1 point saved on this phone");
    expect(tp("recording.points", 0)).toBe("0 points saved on this phone");
    expect(tp("account.upload.pending", 1)).toBe("1 trip waiting");
    expect(tp("unit.trips", 2)).toBe("2 trips");
    expect(tp("unit.points", 1)).toBe("1 point");
    expect(tp("recording.quiet.message", 1)).toContain("1 minute old");
  });

  test("the count is written the way the language writes numbers", () => {
    expect(tp("unit.points", 12345)).toBe("12,345 points");
  });

  test("every plural has an other form, and the forms use the same placeholders", () => {
    const keys = Object.keys(en);
    const bases = new Set(
      keys.filter((k) => /\.(one|other)$/.test(k)).map((k) => k.replace(/\.(one|other)$/, "")),
    );
    expect(bases.size).toBeGreaterThan(0);
    for (const base of bases) {
      expect(keys).toContain(`${base}.other`);
      expect(keys).toContain(`${base}.one`);
      const holders = (text: string) => (text.match(/\{\w+\}/g) ?? []).sort().join(" ");
      const record = en as Record<string, string>;
      expect(holders(record[`${base}.one`]!), base).toBe(holders(record[`${base}.other`]!));
    }
  });

  test("a placeholder without a value stays visible", () => {
    expect(t("home.open.message")).toBe("Started {when}. Stop it or discard it.");
  });
});

describe("sentences for the system the app runs on", () => {
  const record = en as Record<string, string>;
  const iosKeys = Object.keys(en).filter((k) => k.endsWith(".ios"));

  test("iOS gets its own sentence where the catalogue has one, every other system the usual one", () => {
    expect(tOn("ios", "guide.body")).toContain("app switcher");
    expect(tOn("android", "guide.body")).toContain("Unrestricted in the Android settings");
    expect(tOn("android", "guide.body")).toBe(t("guide.body"));
    expect(tOn("web", "guide.body")).toBe(t("guide.body"));
  });

  test("a sentence without an iOS version is the same on iOS, and placeholders are filled", () => {
    expect(tOn("ios", "recording.stop")).toBe(t("recording.stop"));
    expect(tOn("ios", "home.open.message", { when: "today" })).toBe(
      "Started today. Stop it or discard it.",
    );
  });

  test("every iOS sentence belongs to a sentence of the catalogue and has its placeholders", () => {
    expect(iosKeys.length).toBeGreaterThan(0);
    const holders = (text: string) => (text.match(/{w+}/g) ?? []).sort().join(" ");
    for (const key of iosKeys) {
      const base = key.replace(/.ios$/, "");
      expect(Object.keys(en), key).toContain(base);
      expect(holders(record[key]!), key).toBe(holders(record[base]!));
    }
  });

  test("an iOS sentence does not name what only Android has", () => {
    for (const key of iosKeys) {
      expect(record[key], key).not.toMatch(
        /Android|Google|quick settings|notification|force-stop/i,
      );
    }
  });
});

describe("trips that are not completely on the server", () => {
  const keys = [
    "account.leave.pending.message",
    "account.leave.delete.message",
    "foreign.message",
    "foreign.delete.message",
  ] as const;

  test("one trip gets a singular verb and pronoun, several a plural one", () => {
    expect(unsentSentence("account.leave.pending.message", { trips: 1, points: 6 })).toBe(
      "1 trip, with 6 points of route, is not completely on the server. If you keep it, it is uploaded the next time you sign in to this server with the same account. Nobody who signs in with another account can see or upload it.",
    );
    expect(unsentSentence("account.leave.pending.message", { trips: 2, points: 96 })).toContain(
      "2 trips, with 96 points of route, are not completely on the server. If you keep them",
    );
    expect(unsentSentence("foreign.message", { trips: 1, points: 1 })).toContain(
      "1 trip, with 1 point of route, that was recorded",
    );
  });

  test("every sentence has both forms, and none keeps a placeholder", () => {
    for (const key of keys) {
      for (const trips of [1, 2, 0]) {
        const text = unsentSentence(key, { trips, points: 3 });
        expect(text, `${key} ${trips}`).not.toMatch(/\{\w+\}/);
      }
    }
  });
});

describe("numbers and distances", () => {
  test("distances come from the language, in kilometres from 1000 m", () => {
    expect(formatDistance("12345.67")).toBe("12.3 km");
    expect(formatDistance("999.4")).toBe("999 m");
    expect(formatDistance("1000")).toBe("1 km");
    expect(formatNumber(1234.56, 1)).toBe("1,234.6");
  });
});

describe("the checks of the server", () => {
  test("known flags are sentences, unknown ones are shown as they are", () => {
    expect(integrityLines("seq_gap:1 low_accuracy:3 future_flag:2")).toEqual([
      "1 gap in the numbering of the points",
      "3 points skipped for low accuracy",
      "future_flag:2",
    ]);
    expect(integrityLines("")).toEqual([]);
  });
});

/** Hermes, the JavaScript engine of the app, has no `Intl.PluralRules` on Android: the app crashed at start. */
describe("a runtime with less Intl than Node", () => {
  afterEach(() => vi.unstubAllGlobals());

  test("without Intl.PluralRules the rules of English are used", () => {
    vi.stubGlobal("Intl", { ...Intl, PluralRules: undefined });
    expect(tp("unit.points", 1)).toBe("1 point");
    expect(tp("unit.points", 2)).toBe("2 points");
    expect(tp("unit.points", 0)).toBe("0 points");
  });

  test("without Intl.NumberFormat and Intl.DateTimeFormat the text is still made", () => {
    vi.stubGlobal("Intl", {});
    expect(formatNumber(12.34, 1)).toBe("12.3");
    expect(tp("unit.points", 3)).toBe("3 points");
    expect(formatDateTime("2026-10-15T09:00:00Z")).toMatch(/2026/);
  });
});
