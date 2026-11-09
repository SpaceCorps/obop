// @vitest-environment jsdom
// The entry of the back office puts the brand of the page in place before it draws anything: the tab
// title is the name and the sign-in screens take the accent the theme stores (docs/spec/branding.md,
// sections 6 and 12). `brandPage.test.ts` holds the function; this is the one test that runs the
// entry, so a `main.tsx` that stops calling it is a failing test.
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";

// The entry is imported afresh by each test, and that transforms the whole graph of the app, which
// takes a few hundred milliseconds alone and several seconds on a machine that is running the other
// test files too: the default five seconds is not enough there, and a test that timed out leaves
// its import running, which renders into the next test.
const SLOW = 60_000;

const render = vi.fn();
vi.mock("react-dom/client", () => ({ createRoot: () => ({ render }) }));
vi.mock("./App.tsx", () => ({ App: () => null }));
vi.mock("@obop/ui", async (original) => ({
  ...(await original<typeof import("@obop/ui")>()),
  // The catalogues are not the business of this test: they are loaded when the entry runs.
  initI18n: () => Promise.resolve(),
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
