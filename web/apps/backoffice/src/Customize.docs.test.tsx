// @vitest-environment jsdom
// The three places of Customize that point at the product's documents, drawn by the real screens:
// the notice and button of the Theme tab (to the Brand book), the link on the Modules page and the
// link on the Build tab (to the Guide). All three ask `productDocsVisible()` (docs/spec/branding.md,
// section 6) and are drawn without it while the instance hides the documents. The top bar and the
// palette are held by `Chrome.docs.test.tsx`, the tab filter by `productDocs.test.ts`.
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { Customize } from "./Customize.tsx";
import { setProductDocsHidden } from "./productDocs.ts";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => new Response(JSON.stringify({ value: [], denied: [] }), { status: 200 })),
  );
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});
afterEach(() => {
  act(() => root.unmount());
  container.remove();
  setProductDocsHidden(false);
  vi.unstubAllGlobals();
});

const none = () => {};

/** Customize on one tab, as an administrator. */
async function open(tab: string) {
  await act(async () =>
    root.render(
      <Customize
        token="tok"
        tab={tab}
        section=""
        anchor=""
        apps={[]}
        modules={[]}
        settings={{}}
        isAdmin
        me="ann@example.test"
        modulesRoute={{ tab: "installed", id: "", panel: "overview" }}
        goModules={none}
        go={none}
        goSection={none}
        goAnchor={none}
        onSettings={none}
        onModulesChanged={none}
        notify={none}
      />,
    ),
  );
  for (let i = 0; i < 4; i++) await act(async () => void (await Promise.resolve()));
  return container.textContent ?? "";
}

describe("the Theme tab", () => {
  it("points at the Brand book while the instance shows its documents", async () => {
    const text = await open("theme");
    expect(text).toContain("Looking for the rules?");
    expect(text).toContain("Open the Brand book");
  });

  it("does not while they are hidden", async () => {
    setProductDocsHidden(true);
    const text = await open("theme");
    expect(text).not.toContain("Looking for the rules?");
    expect(text).not.toContain("Open the Brand book");
    expect(text).toContain("Contrast"); // the rest of the tab is there
  });
});

describe("the Modules page", () => {
  it("links to the Guide while the instance shows its documents", async () => {
    expect(await open("modules")).toContain("How to write a custom module?");
  });

  it("does not while they are hidden", async () => {
    setProductDocsHidden(true);
    const text = await open("modules");
    expect(text).not.toContain("How to write a custom module?");
    expect(text).toContain("Core"); // the page itself is there
  });
});

describe("the Build tab", () => {
  it("links to the Guide while the instance shows its documents", async () => {
    expect(await open("build")).toContain("How to write a custom module?");
  });

  it("does not while they are hidden", async () => {
    setProductDocsHidden(true);
    const text = await open("build");
    expect(text).not.toContain("How to write a custom module?");
    expect(text).toContain("Describe a model and install it as a module");
  });
});

describe("the address of a tab that is not shown", () => {
  /** The tabs drawn and the one that is open, as a person sees them. */
  const seen = () => ({
    tabs: [...container.querySelectorAll('[role="tab"]')].map((el) => el.textContent),
    open: container.querySelector(".crumbs strong")?.textContent,
  });

  it("opens the first tab that is shown, and the two documents are not among them", async () => {
    setProductDocsHidden(true);
    for (const asked of ["guide", "brand", "nothing-of-the-kind"]) {
      await open(asked);
      const { tabs, open: now } = seen();
      expect(now, asked).toBe("Theme");
      expect(tabs, asked).not.toContain("Guide");
      expect(tabs, asked).not.toContain("Brand book");
      expect(tabs, asked).toContain("Identity");
    }
  });

  it("opens the document that was asked for while they are shown", async () => {
    await open("brand");
    expect(seen().open).toBe("Brand book");
    expect(seen().tabs).toEqual(expect.arrayContaining(["Brand book", "Guide"]));
    await open("guide");
    expect(seen().open).toBe("Guide");
  });
});
