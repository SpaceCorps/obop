import { afterEach, describe, expect, it, vi } from "vite-plus/test";
import {
  getLang,
  initI18n,
  personalLang,
  resetLanguage,
  setInstanceDefaults,
  setLanguage,
} from "./index.ts";
import { subscribe } from "./store.ts";

// A component renders when the store tells it the language changed, and what it reads while it
// renders (`useI18n()` reads `personalLang()`) must already be the new state. What happens, in what
// order, is recorded in `log`: a write to the storage or the cookie, and each time a subscriber runs.
const log: string[] = [];
/** What a subscriber that reads the saved choice, as `useI18n()` does, saw each time it ran. */
const seen: (string | null)[] = [];

/** A browser with a storage, a cookie jar and a document, all writing to `log`. */
function browser(saved?: string) {
  const store = new Map<string, string>();
  if (saved) store.set("obop-lang", saved);
  vi.stubGlobal("localStorage", {
    getItem: (k: string) => store.get(k) ?? null,
    setItem: (k: string, v: string) => {
      log.push(`storage:${k}=${v}`);
      store.set(k, v);
    },
    removeItem: (k: string) => {
      log.push(`storage:${k} removed`);
      store.delete(k);
    },
  });
  vi.stubGlobal("document", {
    documentElement: { lang: "", dir: "" },
    set cookie(v: string) {
      log.push(`cookie:${v.split(";")[0]}`);
    },
  });
  vi.stubGlobal("location", { search: "", protocol: "http:" });
  vi.stubGlobal("navigator", { languages: ["en-US"], language: "en-US" });
}

let unsubscribe = () => {};
function watch() {
  unsubscribe = subscribe(() => {
    seen.push(personalLang());
    log.push("subscribers");
  });
}

afterEach(async () => {
  unsubscribe();
  vi.unstubAllGlobals();
  log.length = 0;
  seen.length = 0;
  await setInstanceDefaults({});
  await initI18n({ loaders: [] });
  await setLanguage("en", false);
});

describe("choosing a language", () => {
  it("is saved before the subscribers run, so they see the choice", async () => {
    browser();
    watch();
    await setLanguage("de");
    expect(getLang()).toBe("de");
    expect(seen).toEqual(["de"]);
    expect(log).toEqual(["storage:obop-lang=de", "cookie:obop_lang=de", "subscribers"]);
  });

  it("is seen by a person who had no choice and one who is changing it", async () => {
    browser();
    watch();
    expect(personalLang()).toBeNull();
    await setLanguage("fi");
    await setLanguage("sv");
    expect(seen).toEqual(["fi", "sv"]);
    expect(personalLang()).toBe("sv");
  });

  it("saves nothing when asked not to, and never saves a pseudo-locale", async () => {
    browser();
    watch();
    await setLanguage("de", false);
    await setLanguage("en-XA");
    await setLanguage("ar-XB");
    expect(getLang()).toBe("ar-XB");
    expect(personalLang()).toBeNull();
    expect(seen).toEqual([null, null, null]);
    expect(log).toEqual(["subscribers", "subscribers", "subscribers"]);
  });

  it("is not saved when its catalogue cannot be loaded, and nobody is told", async () => {
    browser("de");
    await initI18n({
      loaders: [{ "./es/messages.json": () => Promise.reject(new Error("offline")) }],
    });
    log.length = 0;
    watch();
    await expect(setLanguage("es")).rejects.toThrow("offline");
    expect(getLang()).toBe("de");
    expect(personalLang()).toBe("de");
    expect(seen).toEqual([]);
    expect(log).toEqual([]);
  });
});

describe("going back to the default", () => {
  it("forgets the choice before the subscribers run, so they see none", async () => {
    browser("de");
    await initI18n({ loaders: [] });
    expect(getLang()).toBe("de");
    watch();
    log.length = 0;
    await resetLanguage();
    expect(personalLang()).toBeNull();
    expect(getLang()).toBe("en"); // the browser asks for English
    expect(seen).toEqual([null]);
    expect(log[0]).toBe("storage:obop-lang removed");
    expect(log).toContain("subscribers");
    expect(log.indexOf("storage:obop-lang removed")).toBeLessThan(log.indexOf("subscribers"));
  });
});
