/**
 * The logo and the icons as SVG/HTML strings, for pages that are not React (the landing page is
 * pre-rendered at build time). Same data as the components, so nothing can drift.
 */
import { appColor, appIconName } from "./app-colors.ts";
import { hexToOklch, oklchToHex } from "./colors.ts";
import { ICONS, type Shape } from "./icon-data.ts";
import {
  LOGO_BOX,
  MARK,
  MARK_STROKE,
  WORDMARK,
  WORD_STROKE,
  type LogoShape,
  type LogoVariant,
} from "./logo-data.ts";

const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;");

function shapeSvg(s: Shape): string {
  if (s.startsWith("c:")) {
    const [cx, cy, r] = s.slice(2).split(",");
    return `<circle cx="${cx}" cy="${cy}" r="${r}"/>`;
  }
  if (s.startsWith("d:")) {
    const [cx, cy, r] = s.slice(2).split(",");
    return `<circle cx="${cx}" cy="${cy}" r="${r}" fill="currentColor" stroke="none"/>`;
  }
  if (s.startsWith("r:")) {
    const [x, y, w, h, rx] = s.slice(2).split(",");
    return `<rect x="${x}" y="${y}" width="${w}" height="${h}" rx="${rx}"/>`;
  }
  return `<path d="${s}"/>`;
}

/** An obop icon as inline SVG. Decorative unless `label` is given. */
export function iconSvg(name: string, size = 20, label?: string, strokeWidth = 1.75): string {
  const shapes = ICONS[name] ?? ICONS["app"]!;
  const a11y = label
    ? `role="img" aria-label="${esc(label)}"`
    : `aria-hidden="true" focusable="false"`;
  return `<svg class="ico" viewBox="0 0 24 24" width="${size}" height="${size}" fill="none" stroke="currentColor" stroke-width="${strokeWidth}" stroke-linecap="round" stroke-linejoin="round" ${a11y}>${shapes.map(shapeSvg).join("")}</svg>`;
}

function logoShape(s: LogoShape): string {
  return s.t === "path"
    ? `<path d="${s.d}"/>`
    : `<circle cx="${s.cx}" cy="${s.cy}" r="${s.r}"${s.dot ? ' fill="currentColor" stroke="none"' : ""}/>`;
}

/** The obop logo as inline SVG in `currentColor`. */
export function logoSvg(variant: LogoVariant = "full", size = 24, title = "obop"): string {
  const [w, h] = LOGO_BOX[variant];
  const mark = `<g stroke-width="${MARK_STROKE}">${MARK.map(logoShape).join("")}</g>`;
  const word = WORDMARK.map(logoShape).join("");
  const body =
    variant === "mark"
      ? mark
      : variant === "word"
        ? word
        : `<g transform="translate(0 3)" stroke-width="${MARK_STROKE}">${MARK.map(logoShape).join("")}</g><g transform="translate(42 0)">${word}</g>`;
  const a11y = title
    ? `role="img" aria-label="${esc(title)}"`
    : `aria-hidden="true" focusable="false"`;
  return `<svg class="logo" viewBox="0 0 ${w} ${h}" width="${(size * w) / h}" height="${size}" fill="none" stroke="currentColor" stroke-width="${WORD_STROKE}" stroke-linecap="round" stroke-linejoin="round" ${a11y}>${body}</svg>`;
}

/** An app tile (gradient square, white glyph) as HTML. */
export function appTileHtml(id: string, name: string, size = 44, color?: string): string {
  const base = color ?? appColor(id, name);
  const lch = hexToOklch(base);
  const tile = lch.l > 0.62 ? oklchToHex({ ...lch, l: 0.62 }) : base;
  return `<span class="app-icon" style="--tile:${tile};width:${size}px;height:${size}px;border-radius:${Math.round(size * 0.28)}px" aria-hidden="true">${iconSvg(appIconName(id, name), Math.round(size * 0.58))}</span>`;
}
