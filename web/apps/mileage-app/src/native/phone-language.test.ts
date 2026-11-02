import { beforeEach, describe, expect, test, vi } from "vite-plus/test";
import { knownLanguage, resolveLanguage } from "../i18n/language.ts";

const rn = vi.hoisted(() => ({
  Platform: { OS: "ios" },
  Settings: { get: vi.fn() },
  I18nManager: { getConstants: vi.fn() },
}));
vi.mock("react-native", () => rn);

const { phoneLanguageTags } = await import("./phone-language.ts");

beforeEach(() => {
  rn.Settings.get.mockReset();
  rn.I18nManager.getConstants.mockReset();
});

describe("the languages of an iPhone (NSUserDefaults, through Settings)", () => {
  const ios = (values: Record<string, unknown>) => {
    rn.Platform.OS = "ios";
    rn.Settings.get.mockImplementation((key: string) => values[key]);
  };

  test("AppleLanguages, the preferred one first", () => {
    ios({ AppleLanguages: ["sv-SE", "en-SE"], AppleLocale: "sv_SE" });
    expect(phoneLanguageTags()).toEqual(["sv-SE", "en-SE"]);
    expect(resolveLanguage("system", phoneLanguageTags())).toBe("sv");
    ios({ AppleLanguages: ["en-US", "sv-SE"] });
    expect(resolveLanguage("system", phoneLanguageTags())).toBe("en");
    // German first, then Swedish: iOS is expected to pick sv.lproj (the permission dialogs) for this
    // phone, and so does the app (the first language of the list that it has). The sign-in pages
    // of the instance are not asked for Swedish: the first language is German, which the
    // browser's Accept-Language lists first.
    ios({ AppleLanguages: ["de-DE", "sv-SE"] });
    expect(resolveLanguage("system", phoneLanguageTags())).toBe("sv");
    expect(knownLanguage("system", phoneLanguageTags())).toBeNull();
    ios({ AppleLanguages: ["de-DE", "fr-FR"] });
    expect(resolveLanguage("system", phoneLanguageTags())).toBe("en");
  });

  test("AppleLocale when the list is empty or not a list; entries that are not text are dropped", () => {
    ios({ AppleLanguages: [], AppleLocale: "sv_SE" });
    expect(phoneLanguageTags()).toEqual(["sv_SE"]);
    ios({ AppleLanguages: "sv", AppleLocale: "en_GB" });
    expect(phoneLanguageTags()).toEqual(["en_GB"]);
    ios({ AppleLanguages: [3, "sv-SE", null] });
    expect(phoneLanguageTags()).toEqual(["sv-SE"]);
  });

  test("nothing known, or a native module that throws: no languages, so English", () => {
    ios({});
    expect(phoneLanguageTags()).toEqual([]);
    rn.Settings.get.mockImplementation(() => {
      throw new Error("no settings");
    });
    expect(phoneLanguageTags()).toEqual([]);
    expect(resolveLanguage("system", phoneLanguageTags())).toBe("en");
  });
});

describe("the language of an Android phone (I18nManager)", () => {
  const android = (localeIdentifier: unknown) => {
    rn.Platform.OS = "android";
    rn.I18nManager.getConstants.mockReturnValue({ localeIdentifier });
  };

  test("the locale of the configuration, as Locale.toString writes it", () => {
    android("sv_SE");
    expect(phoneLanguageTags()).toEqual(["sv_SE"]);
    expect(resolveLanguage("system", phoneLanguageTags())).toBe("sv");
    android("en_US");
    expect(resolveLanguage("system", phoneLanguageTags())).toBe("en");
    android("nb_NO");
    expect(resolveLanguage("system", phoneLanguageTags())).toBe("en");
    android("sv_SE_#Latn");
    expect(resolveLanguage("system", phoneLanguageTags())).toBe("sv");
  });

  test("no identifier, or a module that throws: no languages", () => {
    android(undefined);
    expect(phoneLanguageTags()).toEqual([]);
    android("");
    expect(phoneLanguageTags()).toEqual([]);
    rn.I18nManager.getConstants.mockImplementation(() => {
      throw new Error("no module");
    });
    expect(phoneLanguageTags()).toEqual([]);
  });

  test("Settings is not read on Android", () => {
    android("sv_SE");
    phoneLanguageTags();
    expect(rn.Settings.get).not.toHaveBeenCalled();
  });
});
