import {
  ACCENTS,
  DEFAULT_THEME,
  applyTheme as applyUiTheme,
  appColor as brandColor,
} from "@obop/ui";
import type { Theme } from "./api.ts";

// Palette, accent check and the attribute-driven theme come from @obop/ui; this file only decides
// where a person's own choice is stored.
export { ACCENTS, DEFAULT_THEME };

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
  applyUiTheme(t);
}

/** Colour of an app's tile: the one an admin chose, else the brand colour of its glyph. */
export function appColor(id: string, chosen?: string, name = ""): string {
  return brandColor(id, name, chosen);
}
