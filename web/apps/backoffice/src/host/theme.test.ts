// @vitest-environment jsdom
// The ground colour the page reports to the host. jsdom has no canvas and computes no `oklch()`, so
// the test gives each element its computed background as a text and reads colours from a table:
// what is under test is which element is measured, the `#rrggbb` text, light or dark, and when a
// report is sent.
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import {
  hex,
  luminance,
  measureTheme,
  schemeOf,
  startThemeReporter,
  type Rgba,
  type ThemeMessage,
} from "./theme.ts";

const LIGHT = "oklch(0.962 0.005 265)";
const DARK = "oklch(0.155 0.012 265)";
const CLEAR = "rgba(0, 0, 0, 0)";
const VEIL = "rgba(0, 0, 0, 0.5)";

/** What a canvas would read for the colours of this test; anything else is not a colour. */
const TABLE: Record<string, Rgba> = {
  [LIGHT]: [243, 244, 247, 255],
  [DARK]: [10, 11, 16, 255],
  [CLEAR]: [0, 0, 0, 0],
  [VEIL]: [0, 0, 0, 128],
  "rgb(255, 255, 255)": [255, 255, 255, 255],
};
const colour = (css: string): Rgba | null => TABLE[css] ?? null;

let grounds: Map<Element, string>;

/** The page: `<body>` and one screen in `#root`, each with the background the test gives it. */
function page(body: string, screen: string | null): HTMLElement | null {
  document.body.innerHTML = '<div id="root"></div>';
  grounds.set(document.body, body);
  if (screen === null) return null;
  const first = document.createElement("div");
  document.getElementById("root")!.append(first);
  grounds.set(first, screen);
  return first;
}

beforeEach(() => {
  grounds = new Map();
  // The page's own window: what `document.defaultView` is, which the code under test reads.
  vi.spyOn(document.defaultView!, "getComputedStyle").mockImplementation(
    (element: Element) =>
      ({ backgroundColor: grounds.get(element) ?? CLEAR }) as CSSStyleDeclaration,
  );
});
afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = "";
  delete document.documentElement.dataset.theme;
});

describe("the colour", () => {
  it("is written as #rrggbb in lower case", () => {
    expect(hex([243, 244, 247])).toBe("#f3f4f7");
    expect(hex([0, 0, 0])).toBe("#000000");
    expect(hex([255, 10.6, 1])).toBe("#ff0b01");
  });

  it("is dark where white marks read better than black ones", () => {
    expect(luminance([0, 0, 0])).toBe(0);
    expect(luminance([255, 255, 255])).toBeCloseTo(1, 5);
    expect(schemeOf([243, 244, 247])).toBe("light");
    expect(schemeOf([10, 11, 16])).toBe("dark");
    // A mid grey is still a light ground: black on it has the higher contrast.
    expect(schemeOf([128, 128, 128])).toBe("light");
    expect(schemeOf([90, 90, 90])).toBe("dark");
  });
});

describe("what is measured", () => {
  it("is the first child of #root, whatever its computed text is", () => {
    page("rgb(255, 255, 255)", DARK);
    expect(measureTheme(document, colour)).toEqual({ scheme: "dark", background: "#0a0b10" });
  });

  it("is the body when the first child is transparent, and when there is no child yet", () => {
    page(LIGHT, CLEAR);
    expect(measureTheme(document, colour)).toEqual({ scheme: "light", background: "#f3f4f7" });
    page(DARK, null);
    expect(measureTheme(document, colour)).toEqual({ scheme: "dark", background: "#0a0b10" });
  });

  it("is the child over the body when the child lets the body through", () => {
    page("rgb(255, 255, 255)", VEIL);
    // Half black over white: about #7f7f7f.
    expect(measureTheme(document, colour)).toEqual({ scheme: "light", background: "#7f7f7f" });
  });

  it("is nothing when nothing paints a ground or the text is not a colour", () => {
    page(CLEAR, CLEAR);
    expect(measureTheme(document, colour)).toBeNull();
    page("not a colour", null);
    expect(measureTheme(document, colour)).toBeNull();
  });
});

describe("the reporter", () => {
  let sent: ThemeMessage[];
  let stop: () => void;
  const start = (send?: (theme: ThemeMessage) => unknown) => {
    stop = startThemeReporter({
      doc: document,
      colour,
      send: send ?? ((theme) => void sent.push(theme)),
    });
  };
  /** Long enough for a mutation to be delivered and for the frame after it. */
  const settle = () => new Promise((done) => setTimeout(done, 60));

  beforeEach(() => {
    sent = [];
  });
  afterEach(() => stop());

  it("reports the ground once at the start", async () => {
    page(LIGHT, LIGHT);
    start();
    await vi.waitFor(() => expect(sent).toEqual([{ scheme: "light", background: "#f3f4f7" }]));
    await settle();
    expect(sent).toHaveLength(1);
  });

  it("reports again when an attribute of <html> changed the ground, and not when it did not", async () => {
    const screen = page(LIGHT, LIGHT)!;
    start();
    await vi.waitFor(() => expect(sent).toHaveLength(1));
    // An attribute that leaves the ground as it is: looked at, not reported.
    document.documentElement.dataset.density = "compact";
    await settle();
    expect(sent).toHaveLength(1);
    grounds.set(screen, DARK);
    document.documentElement.dataset.theme = "dark";
    await vi.waitFor(() => expect(sent).toHaveLength(2));
    expect(sent[1]).toEqual({ scheme: "dark", background: "#0a0b10" });
    delete document.documentElement.dataset.density;
  });

  it("reports again when React puts another screen into #root", async () => {
    page(LIGHT, LIGHT);
    start();
    await vi.waitFor(() => expect(sent).toHaveLength(1));
    const next = document.createElement("main");
    grounds.set(next, DARK);
    document.getElementById("root")!.replaceChildren(next);
    await vi.waitFor(() => expect(sent).toHaveLength(2));
    expect(sent[1]!.scheme).toBe("dark");
  });

  it("looks again when the system switches between light and dark", async () => {
    const listeners: (() => void)[] = [];
    // jsdom has no matchMedia; the page's window gets one for this test.
    const view = document.defaultView as unknown as { matchMedia?: unknown };
    view.matchMedia = (query: string) => ({
      media: query,
      matches: false,
      addEventListener: (_: string, listener: () => void) => void listeners.push(listener),
      removeEventListener: () => {},
    });
    const screen = page(LIGHT, LIGHT)!;
    start();
    await vi.waitFor(() => expect(sent).toHaveLength(1));
    expect(listeners).toHaveLength(1);
    grounds.set(screen, DARK);
    listeners[0]!();
    await vi.waitFor(() => expect(sent).toHaveLength(2));
    delete view.matchMedia;
  });

  it("sends a report again at the next look when the host did not take it", async () => {
    page(LIGHT, LIGHT);
    let refuse = true;
    start((theme) => {
      sent.push(theme);
      if (refuse) throw new Error("refused");
    });
    await vi.waitFor(() => expect(sent).toHaveLength(1));
    await settle();
    refuse = false;
    document.documentElement.dataset.theme = "light";
    await vi.waitFor(() => expect(sent).toHaveLength(2));
    expect(sent[1]).toEqual(sent[0]);
  });

  it("is quiet after it was stopped", async () => {
    const screen = page(LIGHT, LIGHT)!;
    start();
    await vi.waitFor(() => expect(sent).toHaveLength(1));
    stop();
    grounds.set(screen, DARK);
    document.documentElement.dataset.theme = "dark";
    await settle();
    expect(sent).toHaveLength(1);
  });
});
