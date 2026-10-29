import { readFileSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, test, vi } from "vite-plus/test";
import { en } from "../i18n/en.ts";
import { t } from "../i18n/index.ts";

const platform = vi.hoisted(() => ({ OS: "ios" }));
vi.mock("react-native", () => ({ Platform: platform }));

const { tOs } = await import("./os-text.ts");

describe("tOs", () => {
  test("follows the system the app runs on", () => {
    platform.OS = "ios";
    expect(tOs("guide.body")).toContain("app switcher");
    platform.OS = "android";
    expect(tOs("guide.body")).toBe(t("guide.body"));
  });
});

describe("the screens and the iOS sentences of the catalogue", () => {
  const dir = fileURLToPath(new URL("./screens/", import.meta.url));
  const used = new Set<string>();
  for (const file of readdirSync(dir).filter((name) => /\.tsx?$/.test(name))) {
    const source = readFileSync(dir + file, "utf8");
    for (const match of source.matchAll(/\btOs\(\s*"([\w.]+)"/g)) used.add(match[1]!);
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
