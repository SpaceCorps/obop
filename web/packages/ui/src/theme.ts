import { accentVars, hexToOklch } from "./colors.ts";

export type ThemeMode = "system" | "light" | "dark";
export type ThemeDensity = "comfortable" | "compact";
export type ThemeRadius = "sharp" | "soft" | "round";
export type ThemeSettings = {
  accent: string;
  mode: ThemeMode;
  density: ThemeDensity;
  radius: ThemeRadius;
};

/** Preset accents offered in the theme picker. The default is the first. */
export const ACCENTS = [
  { name: "Violet", value: "#6d3fd0" },
  { name: "Indigo", value: "#3b5bdb" },
  { name: "Teal", value: "#0b8a8f" },
  { name: "Green", value: "#2b8a3e" },
  { name: "Amber", value: "#d9730d" },
  { name: "Rose", value: "#d6336c" },
  { name: "Slate", value: "#495670" },
];

export const DEFAULT_THEME: ThemeSettings = {
  accent: ACCENTS[0]!.value,
  mode: "system",
  density: "comfortable",
  radius: "soft",
};

export const RADII = { sharp: "4px", soft: "12px", round: "20px" } as const;

/**
 * Put a theme on the document: an attribute change per axis, plus the accent's custom properties.
 * `--accent` and `--radius` are set inline so the person's choice always wins over the defaults
 * in tokens.css.
 */
export function applyTheme(t: ThemeSettings, root: HTMLElement = document.documentElement) {
  for (const [k, v] of Object.entries(accentVars(t.accent))) root.style.setProperty(k, v);
  root.style.setProperty("--accent-h", String(Math.round(hexToOklch(t.accent).h)));
  root.style.setProperty("--radius", RADII[t.radius]);
  root.dataset.density = t.density;
  root.dataset.radius = t.radius;
  if (t.mode === "system") delete root.dataset.theme;
  else root.dataset.theme = t.mode;
}
