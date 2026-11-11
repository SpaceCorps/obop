// What the page does with the brand its shell carries (docs/spec/branding.md, sections 4 and 6),
// before anything is drawn and before anyone has signed in.
import { DEFAULT_THEME, applyTheme, pageBrand, type PageBrand } from "@obop/ui";

/**
 * With a brand: the tab title is the name (the shell has written it into `<title>` already; the page
 * sets it again, so it stays the name before and after sign-in whatever else touches the title) and
 * the sign-in screens take the accent the theme stores, when it stores one. The signed-in app
 * applies the theme as usual and its accent is the same. With no brand nothing changes.
 */
export function applyBrandToPage(
  brand: PageBrand | null = pageBrand(),
  root: HTMLElement | undefined = typeof document === "undefined"
    ? undefined
    : document.documentElement,
): void {
  if (!brand || !root) return;
  root.ownerDocument.title = brand.name;
  if (brand.accent) applyTheme({ ...DEFAULT_THEME, accent: brand.accent }, root);
}
