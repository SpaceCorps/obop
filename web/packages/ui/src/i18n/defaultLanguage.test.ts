import { afterEach, describe, expect, it, vi } from "vite-plus/test";
import {
  DEFAULT_LANGUAGE_META,
  getLang,
  initI18n,
  instanceLang,
  pageDefaultLang,
  personalLang,
  resetLanguage,
  setInstanceDefaults,
  setLanguage,
  type PageDocument,
} from "../index.ts";

/** A page whose only meta tag is the default-language one, with this content (or none at all). */
function page(content: string | null | undefined): PageDocument {
  return {
    querySelector: (selector) => {
      expect(selector).toBe(`meta[name="${DEFAULT_LANGUAGE_META}"]`);
      return content === undefined
        ? null
        : { getAttribute: (n) => (n === "content" ? content : null) };
    },
  };
}

/** The storage, address and browser language of the page the test pretends to run in. */
function browser(opts: { saved?: string; query?: string; languages?: string[] } = {}) {
  const store = new Map<string, string>();
  if (opts.saved) store.set("obop-lang", opts.saved);
  vi.stubGlobal("localStorage", {
    getItem: (k: string) => store.get(k) ?? null,
    setItem: (k: string, v: string) => void store.set(k, v),
    removeItem: (k: string) => void store.delete(k),
  });
  vi.stubGlobal("location", { search: opts.query ?? "", protocol: "http:" });
  vi.stubGlobal("navigator", { languages: opts.languages ?? ["en-US"], language: "en-US" });
}

afterEach(async () => {
  vi.unstubAllGlobals();
  // Forget the instance settings and the page's default; back to English.
  await setInstanceDefaults({});
  await initI18n({ loaders: [], doc: page(undefined) });
  await setLanguage("en", false);
});

describe("pageDefaultLang", () => {
  it("reads a shipped language code from the meta tag", () => {
    for (const code of ["en", "de", "fr", "pl", "fi", "sv", "es"])
      expect(pageDefaultLang(page(code))).toBe(code);
    // Padding and case are tolerated; the server writes neither.
    expect(pageDefaultLang(page(" SV "))).toBe("sv");
  });

  it("is null when the page has no tag or the tag says anything else", () => {
    expect(pageDefaultLang(page(undefined))).toBeNull();
    for (const bad of ["", "  ", "xx", "pt", "sv-SE", "swedish", "sv,fi", "en-XA", "ar-XB", "<sv>"])
      expect(pageDefaultLang(page(bad)), bad).toBeNull();
    // A tag without a content attribute.
    expect(pageDefaultLang(page(null))).toBeNull();
  });

  it("is null without a document, and when the page cannot be queried", () => {
    expect(pageDefaultLang()).toBeNull(); // the unit tests run in Node: there is no document
    expect(
      pageDefaultLang({
        querySelector: () => {
          throw new Error("no");
        },
      }),
    ).toBeNull();
  });
});

describe("the instance default of the page before sign-in", () => {
  it("is the language of the first render when nothing else has been chosen", async () => {
    browser({ languages: ["en-US", "en"] });
    await initI18n({ loaders: [], doc: page("sv") });
    expect(getLang()).toBe("sv");
    expect(instanceLang()).toBe("sv");
  });

  it("is not used when the page has no tag: the browser decides, then English", async () => {
    browser({ languages: ["de-AT"] });
    await initI18n({ loaders: [], doc: page(undefined) });
    expect(getLang()).toBe("de");
    expect(instanceLang()).toBeNull();
    browser({ languages: ["ja", "tlh"] });
    await initI18n({ loaders: [], doc: page("") });
    expect(getLang()).toBe("en");
  });

  it("yields to the signed-in settings, and returns when they are empty", async () => {
    browser();
    await initI18n({ loaders: [], doc: page("sv") });
    await setInstanceDefaults({ language: "fi" });
    expect(getLang()).toBe("fi");
    expect(instanceLang()).toBe("fi");
    // An administrator who sets the same language as the server's, or none, changes nothing for the page.
    await setInstanceDefaults({ language: "sv" });
    expect(getLang()).toBe("sv");
    await setInstanceDefaults({ language: "" });
    expect(getLang()).toBe("sv");
    expect(instanceLang()).toBe("sv");
    await setInstanceDefaults({ language: "xx" }); // not a language we ship: as empty
    expect(getLang()).toBe("sv");
  });

  it("never beats ?lang or the person's own saved choice", async () => {
    browser({ query: "?lang=pl", saved: "de" });
    await initI18n({ loaders: [], doc: page("sv") });
    expect(getLang()).toBe("pl");
    await setInstanceDefaults({ language: "fi" });
    expect(getLang()).toBe("pl");

    browser({ saved: "de" });
    expect(personalLang()).toBe("de");
    await initI18n({ loaders: [], doc: page("sv") });
    expect(getLang()).toBe("de");
    await setInstanceDefaults({ language: "fi" });
    expect(getLang()).toBe("de");
    await setInstanceDefaults({ language: "" });
    expect(getLang()).toBe("de");
  });

  it("is what 'use the instance default' returns to", async () => {
    browser({ saved: "de" });
    await initI18n({ loaders: [], doc: page("sv") });
    expect(getLang()).toBe("de");
    // Forgetting the personal choice (the stub's storage is the only place it lives).
    await resetLanguage();
    expect(getLang()).toBe("sv");
  });
});
