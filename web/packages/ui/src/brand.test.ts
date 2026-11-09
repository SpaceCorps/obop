import { describe, expect, it } from "vite-plus/test";
import {
  BRAND_META,
  brandLogoUrl,
  brandMessage,
  pageBrand,
  parseBrand,
  recoveryFileName,
} from "./brand.ts";
import fixtures from "./brand.fixtures.json";
import type { PageDocument } from "./i18n/store.ts";

/** A page whose only meta tag is the brand one, with this content (or none at all). */
function page(content: string | null | undefined): PageDocument {
  return {
    querySelector: (selector) => {
      expect(selector).toBe(`meta[name="${BRAND_META}"]`);
      return content === undefined
        ? null
        : { getAttribute: (n) => (n === "content" ? content : null) };
    },
  };
}

const tag = (o: object) => JSON.stringify(o);

describe("parseBrand", () => {
  it("reads what the server writes", () => {
    const b = parseBrand(
      tag({
        name: "MOHV",
        slug: "mohv",
        messages: { en: "Welcome", sv: "Välkommen" },
        v: "0123456789abcdef",
        accent: "#C2410C",
      }),
    );
    expect(b).toEqual({
      name: "MOHV",
      slug: "mohv",
      messages: { en: "Welcome", sv: "Välkommen" },
      v: "0123456789abcdef",
      accent: "#c2410c",
    });
  });

  it("needs only a name and a slug", () => {
    expect(parseBrand(tag({ name: "MOHV", slug: "mohv", messages: {}, v: "" }))).toEqual({
      name: "MOHV",
      slug: "mohv",
      messages: {},
      v: "",
    });
    expect(parseBrand(tag({ name: "MOHV", slug: "mohv" }))?.messages).toEqual({});
  });

  it("answers null for anything the server does not write", () => {
    for (const bad of [
      null,
      undefined,
      "",
      "not json",
      "[]",
      "null",
      "42",
      tag({ slug: "mohv" }),
      tag({ name: "", slug: "mohv" }),
      tag({ name: "   ", slug: "mohv" }),
      tag({ name: 7, slug: "mohv" }),
      tag({ name: "MOHV" }),
      tag({ name: "MOHV", slug: "" }),
      tag({ name: "MOHV", slug: "9x" }),
      tag({ name: "MOHV", slug: "Foo/Bar" }),
      tag({ name: "MOHV", slug: "mohv-" }),
      tag({ name: "MOHV", slug: "a".repeat(31) }),
      tag({ name: "MOHV", slug: "mohv", v: "../x" }),
      tag({ name: "MOHV", slug: "mohv", v: 5 }),
    ])
      expect(parseBrand(bad as string), String(bad)).toBeNull();
  });

  it("accepts the shortest and the longest slug", () => {
    expect(parseBrand(tag({ name: "A", slug: "a" }))?.slug).toBe("a");
    expect(parseBrand(tag({ name: "A", slug: "a".repeat(30) }))?.slug).toBe("a".repeat(30));
    expect(parseBrand(tag({ name: "A", slug: "a-b-1" }))?.slug).toBe("a-b-1");
  });

  it("drops what it cannot use and keeps the rest", () => {
    const b = parseBrand(
      tag({
        name: "MOHV",
        slug: "mohv",
        messages: { en: "Hi", xx: "no such language", sv: "", de: "   ", fi: 3, __proto__: "x" },
        accent: "red",
      }),
    );
    expect(b?.messages).toEqual({ en: "Hi" });
    expect(b?.accent).toBeUndefined();
    expect(parseBrand(tag({ name: "M", slug: "m", messages: ["en"] }))?.messages).toEqual({});
    expect(parseBrand(tag({ name: "M", slug: "m", accent: "#abc" }))?.accent).toBeUndefined();
    expect(parseBrand(tag({ name: "M", slug: "m", accent: "#12345g" }))?.accent).toBeUndefined();
  });
});

describe("pageBrand", () => {
  it("reads the tag of the page", () => {
    const b = pageBrand(page(tag({ name: "MOHV", slug: "mohv", messages: {}, v: "ab" })));
    expect(b?.name).toBe("MOHV");
  });

  it("has no brand without a tag, with an empty one or with one that is not the server's", () => {
    expect(pageBrand(page(undefined))).toBeNull();
    expect(pageBrand(page(""))).toBeNull();
    expect(pageBrand(page("{"))).toBeNull();
    expect(pageBrand()).toBeNull(); // the unit tests run in Node: there is no document
    expect(
      pageBrand({
        querySelector: () => {
          throw new Error("no such page");
        },
      }),
    ).toBeNull();
  });

  it("answers the same object for the same tag and a new one for a new tag", () => {
    const one = tag({ name: "MOHV", slug: "mohv" });
    const a = pageBrand(page(one));
    expect(pageBrand(page(one))).toBe(a);
    expect(pageBrand(page(tag({ name: "Acme", slug: "acme" })))?.name).toBe("Acme");
  });
});

describe("brandMessage", () => {
  // The cases the Rust test reads too: the page script and the server choose alike.
  for (const [i, c] of fixtures.message.entries())
    it(`case ${i + 1}: ${JSON.stringify(c.messages)} shown in ${c.lang}`, () => {
      const brand = parseBrand(tag({ name: "X", slug: "x", messages: c.messages }));
      expect(brandMessage(brand, c.lang)).toBe(c.want);
    });

  it("shows nothing with no brand", () => {
    expect(brandMessage(null, "en")).toBe("");
  });

  it("covers a message of every language, each in its own language and in none else", () => {
    const messages = { en: "e", de: "d", fr: "f", pl: "p", fi: "i", sv: "s", es: "o" };
    const brand = parseBrand(tag({ name: "X", slug: "x", messages }));
    for (const [lang, text] of Object.entries(messages))
      expect(brandMessage(brand, lang)).toBe(text);
  });
});

describe("the files and addresses", () => {
  it("names the recovery codes by the slug with a brand and obop without", () => {
    expect(recoveryFileName(parseBrand(tag({ name: "MOHV", slug: "mohv" })))).toBe(
      "mohv-recovery-codes.txt",
    );
    expect(recoveryFileName(null)).toBe("obop-recovery-codes.txt");
  });

  it("puts the version of the logo in the query of its route", () => {
    expect(brandLogoUrl({ v: "0123456789abcdef" })).toBe("/api/brand/logo.svg?v=0123456789abcdef");
    expect(brandLogoUrl({ v: "" })).toBe("/api/brand/logo.svg");
  });
});
