import { describe, expect, test } from "vite-plus/test";
import { DEFAULT_LANGUAGE, NATIVE_TEXTS } from "./native-texts.ts";

describe("the native texts", () => {
  test("the default language is English and has texts", () => {
    expect(DEFAULT_LANGUAGE).toBe("en");
    expect(NATIVE_TEXTS[DEFAULT_LANGUAGE]).toBeDefined();
  });

  test("no text is empty or holds a placeholder", () => {
    for (const [language, texts] of Object.entries(NATIVE_TEXTS)) {
      for (const [key, text] of Object.entries(texts)) {
        expect(text.trim(), `${language} ${key}`).not.toBe("");
        expect(text, `${language} ${key}`).not.toMatch(/[{}]/);
      }
    }
  });
});
