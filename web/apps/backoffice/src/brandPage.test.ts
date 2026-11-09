// @vitest-environment jsdom
// What the page does with its brand before anything is drawn: the tab title is the name, and the
// accent the theme stores colours the sign-in screens (docs/spec/branding.md, sections 4 and 6).
import { afterEach, describe, expect, it } from "vite-plus/test";
import { accentVars, parseBrand } from "@obop/ui";
import { applyBrandToPage } from "./brandPage.ts";

const brand = (extra: object = {}) =>
  parseBrand(JSON.stringify({ name: "MOHV", slug: "mohv", v: "ab", ...extra }));

afterEach(() => {
  document.title = "";
  document.documentElement.removeAttribute("style");
  delete document.documentElement.dataset.theme;
});

describe("applyBrandToPage", () => {
  it("sets the tab title to the name", () => {
    document.title = "obop";
    applyBrandToPage(brand(), document.documentElement);
    expect(document.title).toBe("MOHV");
  });

  it("keeps the accent of the stock theme when the theme stores none", () => {
    applyBrandToPage(brand(), document.documentElement);
    expect(document.documentElement.style.getPropertyValue("--accent")).toBe("");
  });

  it("puts the stored accent on the page when there is one", () => {
    applyBrandToPage(brand({ accent: "#c2410c" }), document.documentElement);
    const style = document.documentElement.style;
    for (const [name, value] of Object.entries(accentVars("#c2410c")))
      expect(style.getPropertyValue(name), name).toBe(value);
    expect(accentVars("#c2410c")["--accent"]).not.toBe(accentVars("#6d3fd0")["--accent"]);
  });

  it("does nothing with no brand", () => {
    document.title = "obop";
    applyBrandToPage(null, document.documentElement);
    expect(document.title).toBe("obop");
    expect(document.documentElement.getAttribute("style")).toBeNull();
  });

  it("reads the tag of the page when it is given none", () => {
    document.head.insertAdjacentHTML(
      "beforeend",
      `<meta name="obop-brand" content='{"name":"Acme","slug":"acme","messages":{},"v":""}'>`,
    );
    applyBrandToPage();
    expect(document.title).toBe("Acme");
    document.head.querySelectorAll('meta[name="obop-brand"]').forEach((m) => m.remove());
  });
});
