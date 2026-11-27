import { describe, expect, test } from "vite-plus/test";
import { en } from "./en.ts";
import { t, tOn, tp } from "./index.ts";

describe("sentences", () => {
  test("a sentence is read by its key", () => {
    expect(t("recording.notification.title")).toBe("Recording a trip");
    expect(t("day.today")).toBe("Today");
  });

  test("placeholders are filled, and one without a value is left visible", () => {
    expect(t("duration.hoursMinutes", { hours: 1, minutes: 5 })).toBe("1 h 5 min");
    expect(t("duration.minutes", { minutes: 42 })).toBe("42 min");
    expect(t("duration.minutes")).toBe("{minutes} min");
  });

  test("a count takes the form English uses for it", () => {
    expect(tp("trips.count", 0)).toBe("0 trips");
    expect(tp("trips.count", 1)).toBe("1 trip");
    expect(tp("trips.count", 2)).toBe("2 trips");
    expect(tp("trips.count", 21)).toBe("21 trips");
  });
});

describe("sentences for one system", () => {
  test("iOS reads the .ios version where there is one, Android the common one", () => {
    expect(tOn("ios", "guide.body")).toBe(en["guide.body.ios"]);
    expect(tOn("android", "guide.body")).toBe(en["guide.body"]);
  });

  test("a sentence without an .ios version is the same on both", () => {
    expect(tOn("ios", "guide.title")).toBe(en["guide.title"]);
    expect(tOn("android", "guide.title")).toBe(en["guide.title"]);
  });
});

describe("the catalogue", () => {
  test("every .ios sentence has the common sentence it stands in for", () => {
    for (const key of Object.keys(en).filter((k) => k.endsWith(".ios"))) {
      expect(Object.keys(en), key).toContain(key.slice(0, -".ios".length));
    }
  });

  const entries = Object.entries(en) as [string, string][];

  test("no sentence is empty or has spaces around it", () => {
    for (const [key, text] of entries) {
      expect(text.trim(), key).not.toBe("");
      expect(text, key).toBe(text.trim());
    }
  });

  test("a placeholder is a word in braces, and every brace belongs to one", () => {
    for (const [key, text] of entries) {
      const withoutPlaceholders = text.replace(/\{\w+\}/g, "");
      expect(withoutPlaceholders, key).not.toMatch(/[{}]/);
    }
  });

  test("a key with plural forms has both forms, and each form shows the count", () => {
    const plural = entries.filter(([key]) => /\.(one|other)$/.test(key));
    const bases = new Set(plural.map(([key]) => key.replace(/\.(one|other)$/, "")));
    for (const base of bases) {
      for (const form of ["one", "other"]) {
        const text = en[`${base}.${form}` as keyof typeof en];
        expect(text, `${base}.${form}`).toBeDefined();
        expect(text, `${base}.${form}`).toContain("{count}");
      }
    }
  });
});
