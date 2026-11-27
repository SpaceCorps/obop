import { isValidElement, type ReactElement, type ReactNode } from "react";
import { describe, expect, test, vi } from "vite-plus/test";

/**
 * The frame of a screen without a renderer: the screen is called as a function and its element
 * tree is read. React Native and the theme are stand-ins; nothing is drawn.
 */
vi.mock("react-native", () => ({
  KeyboardAvoidingView: function KeyboardAvoidingView() {
    return null;
  },
  Platform: { OS: "android" },
  ScrollView: function ScrollView() {
    return null;
  },
  useWindowDimensions: () => ({ width: 390, height: 844 }),
  View: function View() {
    return null;
  },
}));
vi.mock("../theme.tsx", () => ({
  useTheme: () => ({
    space: [0, 4, 8, 12, 16, 24],
    c: { bg: "#fff", surface: "#fff", border: "#ccc" },
  }),
  useInsets: () => ({ top: 0, bottom: 0, left: 0, right: 0 }),
  useWords: () => ({ back: "Back" }),
}));

const { Fade, FADE_BANDS, Screen } = await import("./Screen.tsx");
const { Button } = await import("./Button.tsx");
const { Text } = await import("./Text.tsx");

/** Every element of a tree, depth first. */
function elements(node: ReactNode): ReactElement[] {
  if (Array.isArray(node)) return node.flatMap(elements);
  if (!isValidElement(node)) return [];
  const props = node.props as { children?: ReactNode };
  return [node, ...elements(props.children)];
}

function backButton(props: Partial<Parameters<typeof Screen>[0]>) {
  const tree = Screen({ title: "Account", onBack: () => {}, children: null, ...props });
  const buttons = elements(tree).filter((e) => e.type === Button);
  expect(buttons).toHaveLength(1);
  return buttons[0]!.props as { label: string; fit?: boolean };
}

describe("the back link of a screen (L26)", () => {
  test("stands at the start of the header, as wide as its word, by default", () => {
    expect(backButton({})).toMatchObject({ label: "Back", fit: true });
  });

  test("a screen may still ask for it across the header", () => {
    expect(backButton({ backAtStart: false }).fit).toBe(false);
  });

  test("a screen without onBack has none", () => {
    const tree = Screen({ title: "Home", children: null });
    expect(elements(tree).filter((e) => e.type === Button)).toHaveLength(0);
  });
});

describe("the footer of a screen", () => {
  const footerStyle = (props: Partial<Parameters<typeof Screen>[0]>) => {
    const tree = Screen({ title: "Home", footer: <Text>Start</Text>, children: null, ...props });
    const frame = elements(tree).find(
      (e) => (e.props as { testID?: string }).testID === "screen-footer",
    );
    expect(frame).toBeDefined();
    return (frame!.props as { style: Record<string, unknown> }).style;
  };

  test("has a ground and a line above it by default", () => {
    expect(footerStyle({})).toMatchObject({ backgroundColor: "#fff", borderTopWidth: 1 });
  });

  test("a bare footer has neither, nor a padding at its top", () => {
    const style = footerStyle({ footerBare: true });
    expect(style.backgroundColor).toBeUndefined();
    expect(style.borderTopWidth).toBeUndefined();
    expect(style.paddingTop).toBe(0);
    expect(footerStyle({}).paddingTop).toBeUndefined();
  });
});

describe("a fading ground", () => {
  const bandsOf = (toward: "bottom" | "top") => {
    const fill = Fade({ color: "#0a0c11", toward }) as ReactElement;
    const style = (fill.props as { style: Record<string, unknown> }).style;
    const bands = elements((fill.props as { children: ReactNode }).children).map(
      (e) => (e.props as { style: { backgroundColor: string; opacity: number } }).style,
    );
    return { style, bands };
  };

  test("is plain bands of the colour, each less opaque than the one before", () => {
    const { bands } = bandsOf("bottom");
    expect(bands).toHaveLength(FADE_BANDS);
    expect(bands.every((band) => band.backgroundColor === "#0a0c11")).toBe(true);
    const opacities = bands.map((band) => band.opacity);
    expect(opacities[0]).toBeGreaterThan(0.95);
    expect(opacities.at(-1)).toBeLessThan(0.05);
    expect(opacities.every((o, i) => i === 0 || o < opacities[i - 1]!)).toBe(true);
  });

  test("fills its parent, takes no touch, and turns round for a footer", () => {
    expect(bandsOf("bottom").style).toMatchObject({
      position: "absolute",
      top: 0,
      bottom: 0,
      flexDirection: "column",
    });
    expect(bandsOf("top").style.flexDirection).toBe("column-reverse");
    const fill = Fade({ color: "#000000", toward: "top" }) as ReactElement;
    expect((fill.props as { pointerEvents: string }).pointerEvents).toBe("none");
  });
});

describe("a floating header", () => {
  const byTestId = (tree: ReactNode, id: string) =>
    elements(tree).find((e) => (e.props as { testID?: string }).testID === id);
  const styleOf = (element: ReactElement | undefined) =>
    (element!.props as { style: Record<string, unknown> }).style;

  test("lies over the content, drawn after it, on a ground that fades out downward", () => {
    const tree = Screen({ title: "Trips", floatingHeaderHeight: 40, children: null });
    const header = byTestId(tree, "screen-header");
    expect(styleOf(header)).toMatchObject({ position: "absolute", top: 0 });
    expect(styleOf(header).backgroundColor).toBeUndefined();
    const fill = elements((header!.props as { children: ReactNode }).children).find(
      (e) => e.type === Fade,
    );
    expect(fill?.props).toMatchObject({ color: "#fff", toward: "bottom" });
    const all = elements(tree);
    expect(all.indexOf(header!)).toBeGreaterThan(all.indexOf(byTestId(tree, "screen-body")!));
  });

  test("the content starts under it and the frame keeps no room at its top", () => {
    const tree = Screen({ title: "Trips", floatingHeaderHeight: 40, children: null });
    // The top padding of the header (12), its height and the space under it (16).
    expect(styleOf(byTestId(tree, "screen-body")).paddingTop).toBe(12 + 40 + 16);
    expect(styleOf(tree as ReactElement).paddingTop).toBe(0);
  });

  test("without it the header is in the flow and the content has no padding at its top", () => {
    const tree = Screen({ title: "Trips", children: null });
    expect(byTestId(tree, "screen-header")).toBeUndefined();
    expect(styleOf(byTestId(tree, "screen-body")).paddingTop).toBeUndefined();
  });
});

describe("the title of a screen", () => {
  test("is its text by default", () => {
    const tree = Screen({ title: "Trip", children: null });
    const titles = elements(tree).filter((e) => e.type === Text);
    expect(titles.map((e) => (e.props as { children: ReactNode }).children)).toEqual(["Trip"]);
  });

  test("a slot takes the place of the text, for a title changed in place", () => {
    const slot = <input data-testid="slot" />;
    const tree = Screen({ title: "Trip", titleSlot: slot, children: null });
    expect(elements(tree).filter((e) => e.type === Text)).toHaveLength(0);
    expect(elements(tree)).toContain(slot);
  });
});
