import { describe, expect, test } from "vite-plus/test";
import type { LanguageCode } from "./index.ts";
import {
  CHOICES,
  createLanguageStore,
  languageOfTag,
  parseChoice,
  resolveLanguage,
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
    // The first language of the phone decides, as the rule says; a blank entry is skipped.
    expect(resolveLanguage("system", ["de-DE", "sv-SE"])).toBe("en");
    expect(resolveLanguage("system", ["sv-SE", "en-GB"])).toBe("sv");
    expect(resolveLanguage("system", ["", "sv"])).toBe("sv");
  });

  test("a chosen language wins over the phone", () => {
    expect(resolveLanguage("en", ["sv-SE"])).toBe("en");
    expect(resolveLanguage("sv", ["en-US"])).toBe("sv");
    expect(resolveLanguage("sv", [])).toBe("sv");
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
