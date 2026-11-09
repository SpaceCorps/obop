// @vitest-environment jsdom
// The two places of the top bar that point at the product's documents: the item "Brand book" of the
// account menu and the entry "How to write a custom module" of the command palette. Both ask
// `productDocsVisible()` (docs/spec/branding.md, section 6) and are drawn without it while the
// instance hides them. The other five places are held by `productDocs.test.ts` and by the Playwright
// spec `brand-backoffice.spec.ts`.
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { Palette, TopBar } from "./Chrome.tsx";
import { setProductDocsHidden } from "./productDocs.ts";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});
afterEach(() => {
  act(() => root.unmount());
  container.remove();
  setProductDocsHidden(false);
});

const none = () => {};
function bar() {
  return (
    <TopBar
      apps={[]}
      current={undefined}
      email="ann@example.test"
      menuOpen={false}
      hasMenu={false}
      onMenu={none}
      onLauncher={none}
      onPalette={none}
      onSignOut={none}
      onTheme={none}
      onBrand={none}
      onSecurity={none}
    />
  );
}

/** The labels of the account menu, after opening it. */
async function menuItems(): Promise<string[]> {
  await act(async () => root.render(bar()));
  const trigger = container.querySelector<HTMLButtonElement>("button.avatar-btn");
  expect(trigger).not.toBeNull();
  await act(async () => trigger!.click());
  return [...document.querySelectorAll('[role="menuitem"]')].map((el) => el.textContent ?? "");
}

describe("the account menu", () => {
  it("offers the Brand book while the instance shows its documents", async () => {
    const items = await menuItems();
    expect(items).toContain("Theme");
    expect(items).toContain("Security");
    expect(items).toContain("Brand book");
  });

  it("does not offer it while they are hidden, and keeps the rest", async () => {
    setProductDocsHidden(true);
    const items = await menuItems();
    expect(items).not.toContain("Brand book");
    expect(items).toEqual(expect.arrayContaining(["Theme", "Security", "Sign out"]));
  });
});

describe("the command palette", () => {
  const customize = {
    id: "customize",
    name: "Customize",
    color: "#6d3fd0",
    module: { id: "platform", name: "Platform", models: [] },
  };

  async function hits(query: string): Promise<string[]> {
    await act(async () => root.render(<Palette apps={[customize]} onClose={vi.fn()} />));
    const input = document.querySelector<HTMLInputElement>(
      'input[aria-label], input[role="combobox"], input',
    );
    expect(input).not.toBeNull();
    await act(async () => {
      const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
      set.call(input, query);
      input!.dispatchEvent(new Event("input", { bubbles: true }));
    });
    return [
      ...document.querySelectorAll('[role="option"], .palette li, .palette a, .palette button'),
    ].map((el) => el.textContent ?? "");
  }

  it("finds the guide while the instance shows its documents", async () => {
    expect((await hits("custom module")).join("|")).toContain("How to write a custom module");
  });

  it("does not find it while they are hidden", async () => {
    setProductDocsHidden(true);
    expect((await hits("custom module")).join("|")).not.toContain("How to write a custom module");
  });
});
