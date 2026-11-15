import { buildAuthorizeUrl } from "@obop/client";
import { describe, expect, test } from "vite-plus/test";
import type { LanguageCode } from "./index.ts";
import {
  CHOICES,
  createLanguageStore,
  knownLanguage,
  languageOfTag,
  parseChoice,
  resolveLanguage,
  supportedPhoneLanguage,
  type LanguagePorts,
} from "./language.ts";

describe("the tags of the phone", () => {
  test("Swedish and English are found in the spellings of iOS and Android", () => {
    for (const tag of ["sv", "sv-SE", "sv_SE", "SV", "sv-FI", "sv_SE_#Latn", " sv-SE "]) {
      expect(languageOfTag(tag), tag).toBe("sv");
    }
    for (const tag of ["en", "en-GB", "en_US", "EN-us"]) expect(languageOfTag(tag), tag).toBe("en");
  });

  test("any other language, and anything that is not a tag, is none", () => {
    for (const tag of ["de-DE", "nb_NO", "fi", "pl-PL", "svx", "", "  ", "-", undefined, null, 5]) {
      expect(languageOfTag(tag), String(tag)).toBeNull();
    }
  });
});

describe("the choice", () => {
  test("is one of system, en, sv; anything stored that is not one of them follows the phone", () => {
    expect(CHOICES).toEqual(["system", "en", "sv"]);
    for (const choice of CHOICES) expect(parseChoice(choice)).toBe(choice);
    for (const junk of ["", "SV", "sv-SE", "klingon", null, undefined, 3, {}]) {
      expect(parseChoice(junk), JSON.stringify(junk) ?? "undefined").toBe("system");
    }
  });

  test("following the phone: Swedish or English when the phone is set to one of them, else English", () => {
    expect(resolveLanguage("system", ["sv-SE"])).toBe("sv");
    expect(resolveLanguage("system", ["sv_SE"])).toBe("sv");
    expect(resolveLanguage("system", ["en-US"])).toBe("en");
    expect(resolveLanguage("system", ["de-DE"])).toBe("en");
    expect(resolveLanguage("system", [])).toBe("en");
    // The first language of the list that the app has decides (iOS is expected to pick its own localisations the
    // same way, so the system dialogs and the app agree); a blank entry is skipped.
    expect(resolveLanguage("system", ["de-DE", "sv-SE"])).toBe("sv");
    expect(resolveLanguage("system", ["nb-NO", "fi-FI", "sv-SE", "en-GB"])).toBe("sv");
    expect(resolveLanguage("system", ["de-DE", "en-GB", "sv-SE"])).toBe("en");
    expect(resolveLanguage("system", ["sv-SE", "en-GB"])).toBe("sv");
    expect(resolveLanguage("system", ["en-GB", "sv-SE"])).toBe("en");
    expect(resolveLanguage("system", ["", "sv"])).toBe("sv");
    expect(resolveLanguage("system", ["de-DE", "fr-FR"])).toBe("en");
  });

  test("the first language of the list that the app has, or none", () => {
    expect(supportedPhoneLanguage(["de-DE", "sv-SE"])).toBe("sv");
    expect(supportedPhoneLanguage(["de-DE", "fr-FR"])).toBeNull();
    expect(supportedPhoneLanguage([])).toBeNull();
  });

  test("a chosen language wins over the phone", () => {
    expect(resolveLanguage("en", ["sv-SE"])).toBe("en");
    expect(resolveLanguage("sv", ["en-US"])).toBe("sv");
    expect(resolveLanguage("sv", [])).toBe("sv");
  });
});

describe("the language the app positively knows (what the sign-in pages are asked for)", () => {
  test("a chosen language is known, whatever the phone says", () => {
    expect(knownLanguage("en", ["de-DE"])).toBe("en");
    expect(knownLanguage("sv", ["de-DE"])).toBe("sv");
    expect(knownLanguage("sv", [])).toBe("sv");
  });

  test("following the phone: known when the phone first language is one the app has", () => {
    expect(knownLanguage("system", ["sv-SE"])).toBe("sv");
    expect(knownLanguage("system", ["en-US", "sv-SE"])).toBe("en");
    expect(knownLanguage("system", ["", "sv"])).toBe("sv");
  });

  test("a phone in a language the app does not have is not known as English", () => {
    // The app shows English because it has nothing else; that is not what the person reads.
    expect(resolveLanguage("system", ["de-DE"])).toBe("en");
    expect(knownLanguage("system", ["de-DE"])).toBeNull();
    expect(knownLanguage("system", ["fr_FR"])).toBeNull();
    expect(knownLanguage("system", [])).toBeNull();
    // A later language the app has is shown (iOS own rule), but the first one is German: the
    // browser Accept-Language, which lists German first, decides the pages of the instance.
    expect(resolveLanguage("system", ["de-DE", "sv-SE"])).toBe("sv");
    expect(knownLanguage("system", ["de-DE", "sv-SE"])).toBeNull();
  });

  describe("the address of the authorization page", () => {
    const base = {
      baseUrl: "https://acme.example.com",
      clientId: "c1",
      redirectUri: "org.obop.mileage:/cb",
      challenge: "ch",
      state: "st",
      scope: ["openid"],
    };
    /** The wiring of `native/auth.ts` (`lang: languageStore.known() ?? undefined`), with the phone behind the ports. */
    const langOf = async (phone: string[], stored: string | null = null) => {
      const store = createLanguageStore(setup({ phone, stored }).ports);
      await store.start();
      const url = new URL(buildAuthorizeUrl({ ...base, lang: store.known() ?? undefined }));
      return url.searchParams.get("lang");
    };

    test("a German phone sends no lang, so the browser decides (the instance has German pages)", async () => {
      expect(await langOf(["de-DE"])).toBeNull();
      expect(await langOf(["fr_FR"])).toBeNull();
      expect(await langOf(["pl-PL"])).toBeNull();
      expect(await langOf([])).toBeNull();
    });

    test("a Swedish phone sends lang=sv, an English one lang=en", async () => {
      expect(await langOf(["sv-SE"])).toBe("sv");
      expect(await langOf(["en-GB"])).toBe("en");
    });

    test("a chosen language is sent, also on a phone in another language", async () => {
      expect(await langOf(["de-DE"], "en")).toBe("en");
      expect(await langOf(["de-DE"], "sv")).toBe("sv");
      expect(await langOf(["sv-SE"], "en")).toBe("en");
    });

    test("choosing follow the phone again takes the language away from the address", async () => {
      const store = createLanguageStore(setup({ phone: ["de-DE"] }).ports);
      await store.start();
      await store.choose("sv");
      expect(store.known()).toBe("sv");
      await store.choose("system");
      expect(store.known()).toBeNull();
      expect(store.language()).toBe("en");
    });
  });
});

/** Ports with a phone and a stored value, and the record of what was applied and stored. */
function setup(options: { phone?: string[]; stored?: string | null } = {}) {
  const state = {
    stored: options.stored ?? null,
    applied: [] as LanguageCode[],
    saved: [] as string[],
  };
  const ports: LanguagePorts = {
    phone: () => options.phone ?? [],
    load: async () => state.stored,
    save: async (text) => {
      state.saved.push(text);
      state.stored = text;
    },
    apply: (code) => state.applied.push(code),
  };
  return { state, ports };
}

describe("the store of the choice", () => {
  test("starts with the language of the phone at once, before the stored choice is read", async () => {
    const { ports, state } = setup({ phone: ["sv-SE"] });
    const store = createLanguageStore(ports);
    expect(store.language()).toBe("en"); // before start
    const started = store.start();
    expect(store.language()).toBe("sv"); // set synchronously by start()
    await started;
    expect(store.choice()).toBe("system");
    expect(store.language()).toBe("sv");
    expect(state.applied.at(-1)).toBe("sv");
  });

  test("a stored choice replaces the language of the phone when it has been read", async () => {
    const { ports, state } = setup({ phone: ["sv-SE"], stored: "en" });
    const store = createLanguageStore(ports);
    await store.start();
    expect(store.choice()).toBe("en");
    expect(store.language()).toBe("en");
    expect(state.applied).toEqual(["sv", "en"]);
  });

  test("a stored value that is not a choice, or that cannot be read, follows the phone", async () => {
    const bad = createLanguageStore(setup({ phone: ["sv-SE"], stored: "klingon" }).ports);
    await bad.start();
    expect(bad.choice()).toBe("system");
    expect(bad.language()).toBe("sv");

    const broken = createLanguageStore({
      ...setup({ phone: ["sv-SE"] }).ports,
      load: () => Promise.reject(new Error("keystore")),
    });
    await broken.start();
    expect(broken.choice()).toBe("system");
    expect(broken.language()).toBe("sv");
  });

  test("a phone that cannot tell its language, or throws, gives English", async () => {
    const none = createLanguageStore(setup({ phone: [] }).ports);
    await none.start();
    expect(none.language()).toBe("en");
    const raising = createLanguageStore({
      ...setup().ports,
      phone: () => {
        throw new Error("no native module");
      },
    });
    await raising.start();
    expect(raising.language()).toBe("en");
  });

  test("choosing applies at once, stores the choice, and tells the listeners", async () => {
    const { ports, state } = setup({ phone: ["en-US"] });
    const store = createLanguageStore(ports);
    await store.start();
    const heard: string[] = [];
    store.subscribe(() => heard.push(`${store.choice()}/${store.language()}`));

    const saving = store.choose("sv");
    expect(store.language()).toBe("sv"); // applied before the storing has finished
    expect(await saving).toBe(true);
    expect(state.saved).toEqual(["sv"]);
    expect(heard).toEqual(["sv/sv"]);

    await store.choose("system"); // back to the phone: English
    expect(store.choice()).toBe("system");
    expect(store.language()).toBe("en");
    expect(heard).toEqual(["sv/sv", "system/en"]);
    expect(state.saved).toEqual(["sv", "system"]);
  });

  test("the choice shown changes even when the language does not (Swedish phone, system to sv)", async () => {
    const store = createLanguageStore(setup({ phone: ["sv-SE"] }).ports);
    await store.start();
    let calls = 0;
    store.subscribe(() => calls++);
    await store.choose("sv");
    expect(store.choice()).toBe("sv");
    expect(store.language()).toBe("sv");
    expect(calls).toBe(1);
    await store.choose("sv"); // nothing changed: nobody is told
    expect(calls).toBe(1);
  });

  test("a choice that could not be stored is still applied, and the answer says so", async () => {
    const store = createLanguageStore({
      ...setup({ phone: ["en-US"] }).ports,
      save: () => Promise.reject(new Error("keystore")),
    });
    await store.start();
    expect(await store.choose("sv")).toBe(false);
    expect(store.language()).toBe("sv");
  });

  test("after a failed save the next start reads the choice that was saved before, else the phone's", async () => {
    // The note `language.unsaved` must not promise the phone's language: an earlier choice wins.
    const state = { stored: "sv" as string | null };
    const ports: LanguagePorts = {
      phone: () => ["en-US"],
      load: async () => state.stored,
      save: () => Promise.reject(new Error("keystore")),
      apply: () => undefined,
    };
    const first = createLanguageStore(ports);
    await first.start();
    expect(first.language()).toBe("sv");
    expect(await first.choose("en")).toBe(false);
    expect(first.language()).toBe("en");
    const restarted = createLanguageStore(ports);
    await restarted.start();
    expect(restarted.choice()).toBe("sv"); // the earlier saved choice, not the phone's English
    expect(restarted.language()).toBe("sv");
    // With nothing saved before, the phone's language is what the next start shows.
    state.stored = null;
    const fresh = createLanguageStore(ports);
    await fresh.start();
    expect(await fresh.choose("sv")).toBe(false);
    const again = createLanguageStore(ports);
    await again.start();
    expect(again.choice()).toBe("system");
    expect(again.language()).toBe("en");
  });

  test("a choice made while the stored one is still being read wins over it", async () => {
    let release: (value: string | null) => void = () => undefined;
    const store = createLanguageStore({
      ...setup({ phone: ["en-US"] }).ports,
      load: () => new Promise<string | null>((resolve) => (release = resolve)),
    });
    const started = store.start();
    await store.choose("sv");
    release("en"); // the older stored choice arrives late
    await started;
    expect(store.choice()).toBe("sv");
    expect(store.language()).toBe("sv");
  });

  test("a listener that has stopped is not called", async () => {
    const store = createLanguageStore(setup({ phone: ["en-US"] }).ports);
    await store.start();
    let calls = 0;
    const stop = store.subscribe(() => calls++);
    await store.choose("sv");
    stop();
    await store.choose("en");
    expect(calls).toBe(1);
  });
});
