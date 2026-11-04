import { afterEach, describe, expect, test, vi } from "vite-plus/test";
import { en } from "../i18n/en.ts";
import { setLanguage, t } from "../i18n/index.ts";
import { screenSources } from "../test-support/app-sources.ts";

const platform = vi.hoisted(() => ({ OS: "ios" }));
vi.mock("react-native", () => ({ Platform: platform }));

const { tOs } = await import("./os-text.ts");

describe("tOs", () => {
  afterEach(() => setLanguage("en"));

  test("follows the system the app runs on", () => {
    platform.OS = "ios";
    expect(tOs("guide.body")).toContain("app switcher");
    platform.OS = "android";
    expect(tOs("guide.body")).toBe(t("guide.body"));
  });

  test("follows the language too: the iOS and the Android sentence in Swedish", () => {
    setLanguage("sv");
    platform.OS = "ios";
    expect(tOs("guide.body")).toContain("appväxlaren");
    expect(tOs("guide.body")).not.toContain("Obegränsad");
    platform.OS = "android";
    expect(tOs("guide.body")).toContain("Obegränsad");
    expect(tOs("guide.body")).toBe(t("guide.body"));
  });
});

describe("the screens and the iOS sentences of the catalogue", () => {
  const used = new Set<string>();
  // The screens, and App.tsx (the Failed screen is there).
  for (const { text } of screenSources()) {
    for (const match of text.matchAll(/\btOs\(\s*"([\w.]+)"/g)) used.add(match[1]!);
  }
  const iosBases = Object.keys(en)
    .filter((key) => key.endsWith(".ios"))
    .map((key) => key.replace(/\.ios$/, ""));

  test("a sentence that a screen reads through tOs has an iOS version", () => {
    expect(used.size).toBeGreaterThan(0);
    for (const key of used) expect(iosBases, key).toContain(key);
  });

  test("an iOS sentence is read by a screen (otherwise it is never shown)", () => {
    for (const base of iosBases) expect([...used], base).toContain(base);
  });
});
