import { appTileHtml } from "@obop/ui/static";
import { describe, expect, test } from "vite-plus/test";
import { fluidTile } from "./tile.ts";

describe("fluidTile", () => {
  test("swaps the fixed pixel size and the 28 percent corners for the page's custom property", () => {
    const tile = fluidTile(appTileHtml("invoicing", "Invoicing", 96), 96);
    expect(tile).toContain("inline-size:var(--icon-size,96px)");
    expect(tile).toContain("block-size:var(--icon-size,96px)");
    expect(tile).toContain("border-radius:22.37%");
    expect(tile).not.toMatch(/[;"]width:\d+px/);
    expect(tile).not.toMatch(/border-radius:\d+px/);
    // The tile's colour and glyph are untouched.
    expect(tile).toContain("--tile:");
    expect(tile).toContain("<svg");
  });

  test("fails loudly when @obop/ui draws the tile differently", () => {
    expect(() => fluidTile('<span class="app-icon" style="--tile:#fff">x</span>', 48)).toThrow(
      /has changed/,
    );
  });
});
