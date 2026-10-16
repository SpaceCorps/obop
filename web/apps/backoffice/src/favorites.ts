import type { ViewState } from "./query.ts";

/** A saved search: filters, grouping, sort and view, per model and per instance. */
export type Favorite = {
  id: string;
  name: string;
  view: string;
  state: ViewState;
  isDefault?: boolean;
};

const key = (model: string) => `obop-fav:${location.origin}:${model}`;

export function loadFavorites(model: string): Favorite[] {
  try {
    const v = JSON.parse(localStorage.getItem(key(model)) ?? "[]") as Favorite[];
    return Array.isArray(v) ? v : [];
  } catch {
    return [];
  }
}

export function saveFavorites(model: string, list: Favorite[]) {
  try {
    localStorage.setItem(key(model), JSON.stringify(list));
  } catch {
    /* storage unavailable: favorites last until reload */
  }
}

/** Small helper for per-model view preferences (view, columns, folded columns). */
export function readPref<T>(k: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(k);
    return raw === null ? fallback : (JSON.parse(raw) as T);
  } catch {
    return fallback;
  }
}

export function writePref(k: string, v: unknown) {
  try {
    localStorage.setItem(k, JSON.stringify(v));
  } catch {
    /* ignore */
  }
}
