import type { Theme } from "./api.ts";

export const ACCENTS = [
  { name: "Violet", value: "#6d3fd0" },
  { name: "Indigo", value: "#3b5bdb" },
  { name: "Teal", value: "#0b8a8f" },
  { name: "Green", value: "#2b8a3e" },
  { name: "Amber", value: "#d9730d" },
  { name: "Rose", value: "#d6336c" },
  { name: "Slate", value: "#495670" },
];

export const DEFAULT_THEME: Required<Theme> = {
  accent: "#6d3fd0",
  mode: "system",
  density: "comfortable",
  radius: "soft",
};

const RADII = { sharp: "4px", soft: "12px", round: "20px" } as const;

function storedTheme(): Theme {
  try {
    return JSON.parse(localStorage.getItem("obop-theme") ?? "{}") as Theme;
  } catch {
    return {};
  }
}

export function rememberTheme(t: Theme | null) {
  try {
    if (t) localStorage.setItem("obop-theme", JSON.stringify(t));
    else localStorage.removeItem("obop-theme");
  } catch {
    /* storage unavailable: the theme lasts until reload */
  }
}

export function hasPersonalTheme(): boolean {
  return Object.keys(storedTheme()).length > 0;
}

/** Instance default, then the person's own choice on top. */
export function resolveTheme(instance: Theme | undefined): Required<Theme> {
  return { ...DEFAULT_THEME, ...instance, ...storedTheme() };
}

export function applyTheme(t: Required<Theme>) {
  const root = document.documentElement;
  root.style.setProperty("--accent", t.accent);
  root.style.setProperty("--radius", RADII[t.radius]);
  root.dataset.density = t.density;
  if (t.mode === "system") delete root.dataset.theme;
  else root.dataset.theme = t.mode;
}

const KNOWN: Record<string, string> = {
  base: "#3b5bdb",
  product: "#d9730d",
  crm: "#2b8a3e",
  sales: "#d6336c",
  invoicing: "#0b8a8f",
  customize: "#495670",
  fleet: "#5f3dc4",
  helpdesk: "#e8590c",
  projects: "#1c7ed6",
};
const PALETTE = [
  "#6d3fd0",
  "#c2255c",
  "#1098ad",
  "#5c940d",
  "#e67700",
  "#364fc7",
  "#a61e4d",
  "#0b7285",
];

/** Stable colour for an app that has none chosen. */
export function appColor(id: string, chosen?: string): string {
  if (chosen) return chosen;
  if (KNOWN[id]) return KNOWN[id];
  let h = 0;
  for (const c of id) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return PALETTE[h % PALETTE.length]!;
}
