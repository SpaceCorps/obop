// @vitest-environment jsdom
// The sign-in screen of an instance with a brand (docs/spec/branding.md, sections 6 and 13): the
// logo and the name stand where obop's did, the slogan and the three bullets are gone, the message
// sits under the hint line, and nothing the screen says or names says `obop`. With no brand the
// screen is what it was. The page is the shell's: the brand arrives as the content of a meta tag.
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { provideCatalogue, setLanguage, type Lang } from "@obop/ui";
import { SignIn } from "./SignIn.tsx";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

const BRAND = {
  name: "MOHV",
  slug: "mohv",
  messages: { en: "Welcome to the team" },
  v: "0123456789abcdef",
};

function setBrand(content: object | null) {
  document.head.querySelectorAll('meta[name="obop-brand"]').forEach((m) => m.remove());
  if (!content) return;
  const meta = document.createElement("meta");
  meta.name = "obop-brand";
  meta.content = JSON.stringify(content);
  document.head.append(meta);
}

async function show(lang: Lang = "en") {
  provideCatalogue(lang, {});
  await setLanguage(lang, false);
  await act(async () => root.render(<SignIn onDone={vi.fn()} />));
}

/** Every piece of text a person or a screen reader meets on the screen. */
function everything(): string[] {
  const out = [container.textContent ?? ""];
  for (const el of container.querySelectorAll("*"))
    for (const a of ["aria-label", "alt", "title", "placeholder", "name", "href", "src"])
      if (el.getAttribute(a)) out.push(`${a}=${el.getAttribute(a)}`);
  return out;
}

beforeEach(() => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => new Response(JSON.stringify({ enabled: false }), { status: 200 })),
  );
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});
afterEach(async () => {
  act(() => root.unmount());
  container.remove();
  setBrand(null);
  vi.unstubAllGlobals();
  await setLanguage("en", false);
});

describe("with a brand", () => {
  it("shows the logo and the name where obop's stood, and no slogan and no bullets", async () => {
    setBrand(BRAND);
    await show();
    const heads = [...container.querySelectorAll("h1")].map((h) => h.textContent);
    expect(heads).toEqual(["MOHV", "MOHV"]); // the aside, and the card for narrow screens
    for (const img of container.querySelectorAll("img"))
      expect(img.getAttribute("src")).toBe("/api/brand/logo.svg?v=0123456789abcdef");
    expect(container.querySelectorAll("img").length).toBe(2);
    expect(container.querySelector("svg.logo")).toBeNull();
    expect(container.querySelector(".signin-claim")).toBeNull();
    expect(container.querySelectorAll(".signin-brand li").length).toBe(0);
    expect(container.textContent).not.toContain("One business, one platform.");
  });

  it("says nothing that names the product, in text or in a name", async () => {
    setBrand(BRAND);
    await show();
    for (const piece of everything()) expect(piece.toLowerCase(), piece).not.toContain("obop");
  });

  it("puts the message under the hint line", async () => {
    setBrand(BRAND);
    await show();
    const head = container.querySelector(".signin-head")!;
    const lines = [...head.querySelectorAll("p")].map((p) => p.textContent);
    expect(lines).toEqual(["Sign in to your instance.", "Welcome to the team"]);
  });

  it("shows the English message on a Swedish screen that has no Swedish one", async () => {
    setBrand(BRAND);
    await show("sv");
    const head = container.querySelector(".signin-head")!;
    expect([...head.querySelectorAll("p")].map((p) => p.textContent)[1]).toBe(
      "Welcome to the team",
    );
  });

  it("shows the message of the language when there is one", async () => {
    setBrand({ ...BRAND, messages: { en: "Welcome to the team", sv: "Välkommen till laget" } });
    await show("sv");
    const head = container.querySelector(".signin-head")!;
    expect([...head.querySelectorAll("p")].map((p) => p.textContent)[1]).toBe(
      "Välkommen till laget",
    );
  });

  it("shows no message line when none is set", async () => {
    setBrand({ ...BRAND, messages: {} });
    await show();
    expect(container.querySelectorAll(".signin-head p").length).toBe(1);
  });

  it("treats a tag that is not the server's as no brand", async () => {
    document.head.insertAdjacentHTML("beforeend", '<meta name="obop-brand" content="{not json">');
    await show();
    expect(container.querySelector(".signin-claim")).not.toBeNull();
    document.head.querySelectorAll('meta[name="obop-brand"]').forEach((m) => m.remove());
  });
});

describe("with no brand", () => {
  it("is the screen it was: obop's mark, the slogan and three bullets", async () => {
    await show();
    expect(container.querySelectorAll("svg.logo").length).toBe(2);
    expect(container.querySelector('svg.logo[aria-label="obop"]')).not.toBeNull();
    expect(container.querySelector("img")).toBeNull();
    expect(container.querySelector(".signin-claim")?.textContent).toBe(
      "One business, one platform.",
    );
    expect(container.querySelectorAll(".signin-brand li").length).toBe(3);
    expect(container.querySelectorAll(".signin-head p").length).toBe(1);
  });
});
