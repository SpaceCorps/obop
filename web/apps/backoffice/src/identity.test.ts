import { describe, expect, it } from "vite-plus/test";
import {
  LOGO_MAX_BYTES,
  artworkProblem,
  blankForm,
  changes,
  previewBrand,
  readBrand,
} from "./identity.ts";

/** What `instance_brand.get` answers for a row: the declared answer fields are JSON texts. */
const answer = (extra: Record<string, unknown> = {}) => ({
  name: "MOHV",
  slug: "",
  title_en: "MOHV Mileage",
  title_de: "",
  message_en: "Welcome",
  message_sv: "Välkommen",
  hide_product_docs: false,
  set: true,
  v: "0123456789abcdef",
  effective: JSON.stringify({
    name: { value: "MOHV", source: "record" },
    slug: { value: "mohv", source: "record" },
    logo: { value: "tile", source: "record" },
  }),
  fallbacks: "[]",
  ...extra,
});

describe("readBrand", () => {
  it("reads the stored fields, the switch, the version and what is in force", () => {
    const b = readBrand(answer({ hide_product_docs: true }))!;
    expect(b.set).toBe(true);
    expect(b.v).toBe("0123456789abcdef");
    expect(b.form.name).toBe("MOHV");
    expect(b.form.titles.en).toBe("MOHV Mileage");
    expect(b.form.titles.fi).toBe("");
    expect(b.form.messages.sv).toBe("Välkommen");
    expect(b.form.hideDocs).toBe(true);
    expect(b.slug).toBe("mohv");
    expect(b.logo).toBe("tile");
    expect(b.fallbacks).toEqual([]);
  });

  it("takes the switch as on only when it is the boolean true", () => {
    for (const v of ["true", 1, null, undefined, "on"])
      expect(readBrand(answer({ hide_product_docs: v }))!.form.hideDocs, String(v)).toBe(false);
  });

  it("reads an instance with no row", () => {
    const b = readBrand({ set: false, v: "", name: "", effective: "{}", fallbacks: "[]" })!;
    expect(b.set).toBe(false);
    expect(b.form).toEqual(blankForm());
    expect(b.logo).toBe("none");
  });

  it("lists the fields that fell back", () => {
    const b = readBrand(
      answer({
        fallbacks: JSON.stringify([
          { field: "title_sv", reason: "unsafe_character" },
          { field: "logo_svg", reason: "invalid_artwork" },
        ]),
      }),
    )!;
    expect(b.fallbacks.map((f) => f.field)).toEqual(["title_sv", "logo_svg"]);
  });

  it("copes with a field that is an object or a text that is not JSON", () => {
    expect(readBrand(answer({ effective: { slug: { value: "x" } } }))!.slug).toBe("x");
    expect(readBrand(answer({ effective: "{", fallbacks: "{" }))!.fallbacks).toEqual([]);
  });

  it("answers null for what is not an answer", () => {
    for (const bad of [null, undefined, "x", 3]) expect(readBrand(bad)).toBeNull();
  });
});

describe("changes", () => {
  const base = readBrand(answer())!.form;
  const edit = (f: (x: typeof base) => void) => {
    const next = structuredClone(base);
    f(next);
    return next;
  };

  it("is empty when nothing differs", () => {
    expect(changes(base, structuredClone(base), { kind: "keep" })).toEqual({});
  });

  it("sends only the fields that differ", () => {
    const now = edit((f) => {
      f.name = "MOHV AB";
      f.messages.de = "Willkommen";
      f.titles.en = "";
    });
    expect(changes(base, now, { kind: "keep" })).toEqual({
      name: "MOHV AB",
      message_de: "Willkommen",
      title_en: "",
    });
  });

  it("clears a text with the empty text and the artwork with the empty artwork", () => {
    const now = edit((f) => (f.messages.sv = ""));
    expect(changes(base, now, { kind: "remove" })).toEqual({ message_sv: "", logo_svg: "" });
  });

  it("sends a chosen file as the artwork", () => {
    expect(changes(base, base, { kind: "replace", text: "<svg/>", file: "a.svg" })).toEqual({
      logo_svg: "<svg/>",
    });
  });

  it("sends the switch as a boolean, and only when it changed", () => {
    expect(
      changes(
        base,
        edit((f) => (f.hideDocs = true)),
        { kind: "keep" },
      ),
    ).toEqual({
      hide_product_docs: true,
    });
    const on = edit((f) => (f.hideDocs = true));
    expect(changes(on, base, { kind: "keep" })).toEqual({ hide_product_docs: false });
  });

  it("sends a stored value that was not touched under no name at all", () => {
    // A stored text an older policy let in must not be sent again and refused.
    const now = edit((f) => (f.name = "MOHV 2"));
    expect(Object.keys(changes(base, now, { kind: "keep" }))).toEqual(["name"]);
  });
});

describe("the preview", () => {
  it("draws the card from the form", () => {
    const form = blankForm();
    form.name = " Acme ";
    form.messages.en = "Hello";
    form.messages.sv = "   ";
    const b = previewBrand(form, "ab");
    expect(b.name).toBe("Acme");
    expect(b.messages).toEqual({ en: "Hello" });
    expect(b.v).toBe("ab");
  });

  it("draws a placeholder for a name that is not typed yet", () => {
    expect(previewBrand(blankForm(), "").name).toBe("…");
  });
});

describe("artworkProblem", () => {
  it("accepts an SVG file up to 16 KiB", () => {
    expect(artworkProblem({ name: "logo.svg", size: 100, type: "image/svg+xml" })).toBeNull();
    expect(artworkProblem({ name: "LOGO.SVG", size: LOGO_MAX_BYTES, type: "" })).toBeNull();
  });
  it("refuses a larger file and a file of another kind", () => {
    expect(artworkProblem({ name: "logo.svg", size: LOGO_MAX_BYTES + 1 })).toBe("big");
    expect(artworkProblem({ name: "logo.png", size: 10, type: "image/png" })).toBe("kind");
  });
});
