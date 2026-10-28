import { createContext, useContext, useMemo, type ReactNode } from "react";
import { useColorScheme } from "react-native";
import { tokens, type ColorMode, type Palette } from "./tokens.native.ts";

/** Everything a component reads: the palette of the current mode and the scales (dp and sp). */
export type Theme = {
  mode: ColorMode;
  c: Palette;
  space: (typeof tokens)["space"];
  radius: (typeof tokens)["radius"];
  fontSize: (typeof tokens)["fontSize"];
  lineHeight: (typeof tokens)["lineHeight"];
  fontWeight: (typeof tokens)["fontWeight"];
  control: (typeof tokens)["control"];
};

export function makeTheme(mode: ColorMode): Theme {
  return {
    mode,
    c: tokens[mode],
    space: tokens.space,
    radius: tokens.radius,
    fontSize: tokens.fontSize,
    lineHeight: tokens.lineHeight,
    fontWeight: tokens.fontWeight,
    control: tokens.control,
  };
}

const ThemeContext = createContext<Theme>(makeTheme("light"));

/**
 * The theme of a subtree. Without `mode` it follows the system (light or dark); the stories force a
 * mode to show both.
 */
export function ThemeProvider({ mode, children }: { mode?: ColorMode; children: ReactNode }) {
  const system = useColorScheme();
  const resolved: ColorMode = mode ?? (system === "dark" ? "dark" : "light");
  const theme = useMemo(() => makeTheme(resolved), [resolved]);
  return <ThemeContext.Provider value={theme}>{children}</ThemeContext.Provider>;
}

export function useTheme(): Theme {
  return useContext(ThemeContext);
}

/** The space the system bars take, in dp. The app fills it from react-native-safe-area-context. */
export type Insets = { top: number; right: number; bottom: number; left: number };
const NO_INSETS: Insets = { top: 0, right: 0, bottom: 0, left: 0 };
const InsetsContext = createContext<Insets>(NO_INSETS);

export function InsetsProvider({ insets, children }: { insets: Insets; children: ReactNode }) {
  return <InsetsContext.Provider value={insets}>{children}</InsetsContext.Provider>;
}

export function useInsets(): Insets {
  return useContext(InsetsContext);
}
