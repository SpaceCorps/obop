/**
 * An app tile whose size and corners the page's CSS decides. `appTileHtml` (@obop/ui) draws a tile
 * at a fixed pixel size with 28 percent corners, inline; the apps pages want rem sizes (they follow
 * the reader's text size) and the squircle's 22 percent, without `!important`. This swaps the three
 * inline declarations for a custom property the page sets (`--icon-size`, `size` px as the fallback).
 * It throws when the tile no longer has the shape it expects, so a change in @obop/ui fails the
 * build (and its test) instead of quietly drawing fixed-size icons.
 */
const FIXED = /;width:\d+px;height:\d+px;border-radius:\d+px/;

export function fluidTile(html: string, size: number): string {
  if (!FIXED.test(html)) throw new Error("fluidTile: the tile markup from @obop/ui has changed");
  return html.replace(
    FIXED,
    `;inline-size:var(--icon-size,${size}px);block-size:var(--icon-size,${size}px);border-radius:22.37%`,
  );
}
