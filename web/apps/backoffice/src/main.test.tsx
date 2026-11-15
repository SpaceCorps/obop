// @vitest-environment jsdom
// The entry of the back office puts the brand of the page in place before it draws anything: the tab
// title is the name and the sign-in screens take the accent the theme stores (docs/spec/branding.md,
// sections 6 and 12). `brandPage.test.ts` holds the function; this is the one test that runs the
// entry, so a `main.tsx` that stops calling it is a failing test.
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";

// The entry is imported afresh by each test. What costs time is transforming the modules it pulls
// in, once per test file: the whole of `@obop/ui` and the error screen with the data layer behind
// it come to hundreds of modules, which took 20 seconds on a machine that was running the other test
// files too (the default is five), and a test that timed out leaves its import running, which
// renders into the next test. So the entry gets only what it reads: the brand of the page and the
// theme, as they are (`brandPage.ts` runs on them), and stand-ins for the rest. The timeout stays as
// a margin.
const SLOW = 60_000;

const render = vi.fn();
vi.mock("react-dom/client", () => ({ createRoot: () => ({ render }) }));
vi.mock("./App.tsx", () => ({ App: () => null }));
vi.mock("./ErrorBoundary.tsx", () => ({
  ErrorBoundary: ({ children }: { children: unknown }) => children,
}));
vi.mock("@obop/ui", async () => ({
  ...(await import("../../../packages/ui/src/brand.ts")),
  ...(await import("../../../packages/ui/src/theme.ts")),
  // The catalogues are not the business of this test: they are loaded when the entry runs.
  initI18n: () => Promise.resolve(),
  I18nProvider: ({ children }: { children: unknown }) => children,
}));

const TAG = (extra: object) =>
  `<meta name="obop-brand" content='${JSON.stringify({ name: "MOHV", slug: "mohv", messages: {}, v: "ab", ...extra })}'>`;

beforeEach(() => {
  vi.resetModules();
  render.mockClear();
  document.body.innerHTML = '<div id="root"></div>';
});

afterEach(() => {
  document.title = "";
  document.documentElement.removeAttribute("style");
  document.head.querySelectorAll('meta[name="obop-brand"]').forEach((m) => m.remove());
});

describe("the entry of the back office", { timeout: SLOW }, () => {
  it("puts the name and the stored accent on the page before the first render", async () => {
    document.head.insertAdjacentHTML("beforeend", TAG({ accent: "#c2410c" }));
    document.title = "obop";
    let atRender: [string, string] | undefined;
    render.mockImplementation(() => {
      atRender = [document.title, document.documentElement.style.getPropertyValue("--accent")];
    });
    await import("./main.tsx");
    await vi.waitFor(() => expect(render).toHaveBeenCalledTimes(1));
    expect(document.title).toBe("MOHV");
    expect(document.documentElement.style.getPropertyValue("--accent")).toBe("#c2410c");
    expect(atRender?.[0]).toBe("MOHV");
    expect(atRender?.[1]).toBe("#c2410c");
  });

  it("leaves the page as it was when the shell carries no brand", async () => {
    document.title = "obop";
    await import("./main.tsx");
    await vi.waitFor(() => expect(render).toHaveBeenCalledTimes(1));
    expect(document.title).toBe("obop");
    expect(document.documentElement.getAttribute("style")).toBeNull();
  });
});
