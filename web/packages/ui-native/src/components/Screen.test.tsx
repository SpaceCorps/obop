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

const { Screen } = await import("./Screen.tsx");
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
