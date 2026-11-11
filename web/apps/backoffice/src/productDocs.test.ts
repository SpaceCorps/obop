import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { afterEach, describe, expect, it, vi } from "vite-plus/test";
import {
  PRODUCT_DOC_TABS,
  isPersonalTab,
  productDocsVisible,
  readProductDocsHidden,
  setProductDocsHidden,
  visibleTabs,
} from "./productDocs.ts";

afterEach(() => {
  setProductDocsHidden(false);
  vi.unstubAllGlobals();
});

const TABS = [
  { id: "theme" },
  { id: "apps" },
  { id: "identity" },
  { id: "modules" },
  { id: "updates" },
  { id: "build" },
  { id: "security" },
  { id: "brand" },
  { id: "guide" },
];
const ids = (tabs: { id: string }[]) => tabs.map((t) => t.id);

describe("the predicate", () => {
  it("is true until the switch is read as on, and follows it", () => {
    expect(productDocsVisible()).toBe(true);
    setProductDocsHidden(true);
    expect(productDocsVisible()).toBe(false);
    setProductDocsHidden(false);
    expect(productDocsVisible()).toBe(true);
  });
});

describe("the tabs of Customize", () => {
  it("show an administrator everything, and the two documents only while they are shown", () => {
    expect(ids(visibleTabs(TABS, { isAdmin: true, docs: true }))).toEqual(ids(TABS));
    const hidden = ids(visibleTabs(TABS, { isAdmin: true, docs: false }));
    expect(hidden).toEqual(ids(TABS).filter((id) => id !== "brand" && id !== "guide"));
    expect(hidden[0]).toBe("theme"); // the first tab that is shown: where a hidden address goes
  });

  it("show anyone else their theme and security, and the two documents while they are shown", () => {
    expect(ids(visibleTabs(TABS, { isAdmin: false, docs: true }))).toEqual([
      "theme",
      "security",
      "brand",
      "guide",
    ]);
    expect(ids(visibleTabs(TABS, { isAdmin: false, docs: false }))).toEqual(["theme", "security"]);
  });

  it("show no one the Identity tab but an administrator", () => {
    expect(ids(visibleTabs(TABS, { isAdmin: false, docs: true }))).not.toContain("identity");
    expect(ids(visibleTabs(TABS, { isAdmin: true, docs: false }))).toContain("identity");
  });

  it("agree with the test the app makes for a person without the right", () => {
    for (const docs of [true, false])
      for (const t of TABS)
        expect(isPersonalTab(t.id, docs), `${t.id} ${docs}`).toBe(
          visibleTabs(TABS, { isAdmin: false, docs }).some((x) => x.id === t.id),
        );
  });

  it("name exactly the Brand book and the Guide as the product documents", () => {
    expect([...PRODUCT_DOC_TABS].sort()).toEqual(["brand", "guide"]);
  });
});

describe("reading the switch", () => {
  const answer = (status: number, body: unknown) =>
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response(JSON.stringify(body), { status })),
    );

  it("asks instance_brand.get and takes the boolean true as on", async () => {
    answer(200, { value: { hide_product_docs: true, set: true }, denied: [] });
    expect(await readProductDocsHidden("tok")).toBe(true);
    const [url, init] = (fetch as ReturnType<typeof vi.fn>).mock.calls[0]!;
    expect(String(url)).toMatch(/^\/api\/instance_brand\/get\?/);
    expect(init.headers.authorization).toBe("Bearer tok");
  });

  it("takes anything else as off", async () => {
    for (const v of [false, "true", 1, null, undefined]) {
      answer(200, { value: { hide_product_docs: v }, denied: [] });
      expect(await readProductDocsHidden("tok"), String(v)).toBe(false);
    }
    answer(200, { value: { set: false }, denied: [] });
    expect(await readProductDocsHidden("tok")).toBe(false);
  });

  it("changes nothing when the answer cannot be read", async () => {
    answer(500, { error: "boom" });
    expect(await readProductDocsHidden("tok")).toBeUndefined();
    answer(404, { error: "no such command" });
    expect(await readProductDocsHidden("tok")).toBeUndefined();
    answer(200, { value: null, denied: [] });
    expect(await readProductDocsHidden("tok")).toBeUndefined();
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => {
        throw new Error("offline");
      }),
    );
    expect(await readProductDocsHidden("tok")).toBeUndefined();
  });
});

// The seven places that show the Brand book and the Guide (docs/spec/branding.md, section 6) are
// each held by the predicate. A new link to either, in a file that knows nothing of the predicate,
// is the way an eighth place appears: this reads the sources and says so.
const SRC = new URL(".", import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1");

function sources(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) {
      if (name !== "brand") sources(p, out); // the Brand book documents itself
    } else if (/\.tsx?$/.test(name) && !/\.test\.tsx?$/.test(name) && name !== "Guide.tsx")
      out.push(p);
  }
  return out;
}

describe("the places that point at the Brand book or the Guide", () => {
  const POINTS_AT = /(?:tab|model):\s*"(?:guide|brand)"|\/customize\/(?:guide|brand)/;
  const places = sources(SRC).filter((p) => POINTS_AT.test(readFileSync(p, "utf8")));

  it("are the files this test knows, so that a new one is looked at", () => {
    expect(places.map((p) => relative(SRC, p).replaceAll("\\", "/")).sort()).toEqual([
      "App.tsx",
      "Chrome.tsx",
      "Customize.tsx",
      "ModuleBuilder.tsx",
    ]);
  });

  it("all use the predicate or what is built on it", () => {
    for (const p of places)
      expect(
        /from "\.\/productDocs\.ts"/.test(readFileSync(p, "utf8")),
        `${relative(SRC, p)} points at the Brand book or the Guide and does not ask productDocs.ts`,
      ).toBe(true);
  });

  it("are each held, one by one, at the call that shows them", () => {
    const text = (file: string) => readFileSync(join(SRC, file), "utf8");
    // 1. the tab filter; 2. personalCustomize
    expect(text("Customize.tsx")).toMatch(
      /visibleTabs\(TABS, \{ isAdmin: p\.isAdmin, docs: productDocsVisible\(\) \}\)/,
    );
    expect(text("App.tsx")).toMatch(/isPersonalTab\(route\.tab \|\| route\.model, docsVisible\)/);
    // 3. the account-menu item
    expect(text("Chrome.tsx")).toMatch(/\.\.\.\(docs\s*\?\s*\[\{ label: t\("Brand book"\)/);
    // 4. the palette entry
    expect(text("Chrome.tsx")).toMatch(/if \(guideShown\)\s*all\.push\(\{\s*key: "guide"/);
    // 5. the Theme-tab notice and button
    expect(text("Customize.tsx")).toMatch(
      /productDocsVisible\(\) && \(\s*<>\s*<Notice tone="info" title=\{t\("Looking for the rules\?"\)\}/,
    );
    // 6. the link on the Modules page
    expect(text("modules/ModulesPage.tsx")).toMatch(
      /productDocsVisible\(\) && \(\s*<>\s*\{" "\}\s*<a className="reflink" href=\{guideHref\}>/,
    );
    // 7. the link on the Build tab
    expect(text("ModuleBuilder.tsx")).toMatch(
      /productDocsVisible\(\) && \(\s*<>\s*\{" "\}\s*<a\s+className="reflink"/,
    );
  });
});
