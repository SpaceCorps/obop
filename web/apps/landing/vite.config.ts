import { appTileHtml, iconSvg, logoSvg } from "@obop/ui/static";
import { defineConfig } from "../../config/index.ts";

const attr = (tag: string, name: string) => new RegExp(`${name}="([^"]*)"`).exec(tag)?.[1];

/**
 * The landing page is static HTML, so the logo, icons and app tiles are written as placeholders
 * (<ui-logo>, <ui-icon>, <ui-app>) and replaced here with the same inline SVG the React apps draw.
 */
function prerender() {
  return {
    name: "obop-prerender",
    transformIndexHtml(html: string) {
      return html
        .replace(/<ui-logo([^>]*)>\s*<\/ui-logo\s*>/g, (_m, a: string) =>
          logoSvg(
            (attr(a, "variant") ?? "full") as "full" | "mark" | "word",
            Number(attr(a, "size") ?? 24),
            attr(a, "title") ?? "obop",
          ),
        )
        .replace(/<ui-icon([^>]*)>\s*<\/ui-icon\s*>/g, (_m, a: string) =>
          iconSvg(attr(a, "name") ?? "app", Number(attr(a, "size") ?? 20), attr(a, "label")),
        )
        .replace(/<ui-app([^>]*)>\s*<\/ui-app\s*>/g, (_m, a: string) =>
          appTileHtml(attr(a, "id") ?? "", attr(a, "name") ?? "", Number(attr(a, "size") ?? 44)),
        );
    },
  };
}

export default defineConfig({ plugins: [prerender()] });
