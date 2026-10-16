// Social cards (1200x630 PNG) drawn as SVG and rasterised with resvg (prebuilt, no native build).
// Geist ships as WOFF; resvg needs TrueType, so the WOFF1 wrapper (zlib per table) is unpacked here.
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { inflateSync } from "node:zlib";
import { Resvg } from "@resvg/resvg-js";
import { logoSvg } from "@obop/ui/static";
import { GEN, SITE_ROOT } from "./paths.ts";

const require = createRequire(import.meta.url);

export function woffToSfnt(woff: Buffer): Buffer {
  if (woff.toString("latin1", 0, 4) !== "wOFF") throw new Error("not a WOFF file");
  const flavor = woff.readUInt32BE(4);
  const n = woff.readUInt16BE(12);
  const tables = [];
  for (let i = 0; i < n; i++) {
    const o = 44 + i * 20;
    const offset = woff.readUInt32BE(o + 4);
    const comp = woff.readUInt32BE(o + 8);
    const orig = woff.readUInt32BE(o + 12);
    const raw = woff.subarray(offset, offset + comp);
    tables.push({
      tag: woff.subarray(o, o + 4),
      checksum: woff.readUInt32BE(o + 16),
      data: comp < orig ? inflateSync(raw) : raw,
    });
  }
  let entrySelector = 0;
  while (1 << (entrySelector + 1) <= n) entrySelector++;
  const searchRange = (1 << entrySelector) * 16;
  const head = Buffer.alloc(12 + n * 16);
  head.writeUInt32BE(flavor, 0);
  head.writeUInt16BE(n, 4);
  head.writeUInt16BE(searchRange, 6);
  head.writeUInt16BE(entrySelector, 8);
  head.writeUInt16BE(n * 16 - searchRange, 10);
  const parts: Buffer[] = [head];
  let offset = head.length;
  tables.forEach((t, i) => {
    const o = 12 + i * 16;
    t.tag.copy(head, o);
    head.writeUInt32BE(t.checksum, o + 4);
    head.writeUInt32BE(offset, o + 8);
    head.writeUInt32BE(t.data.length, o + 12);
    const pad = (4 - (t.data.length % 4)) % 4;
    parts.push(t.data, Buffer.alloc(pad));
    offset += t.data.length + pad;
  });
  return Buffer.concat(parts);
}

let fontFiles: string[] | undefined;
function fonts(): string[] {
  if (fontFiles) return fontFiles;
  const dir = join(dirname(require.resolve("@fontsource/geist/package.json")), "files");
  const out = join(GEN, "fonts");
  mkdirSync(out, { recursive: true });
  fontFiles = [];
  for (const name of ["latin-400", "latin-600", "latin-ext-400", "latin-ext-600"]) {
    const dest = join(out, `geist-${name}.ttf`);
    writeFileSync(dest, woffToSfnt(readFileSync(join(dir, `geist-${name}-normal.woff`))));
    fontFiles.push(dest);
  }
  return fontFiles;
}

const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

/** Greedy word wrap by an average glyph width (Geist is close to 0.56em at weight 600). */
function wrap(text: string, size: number, width: number, maxLines: number): string[] {
  const per = size * 0.57;
  const max = Math.floor(width / per);
  const lines: string[] = [];
  let cur = "";
  for (const w of text.split(/\s+/)) {
    if ((cur + " " + w).trim().length > max && cur) {
      lines.push(cur);
      cur = w;
    } else cur = (cur + " " + w).trim();
  }
  if (cur) lines.push(cur);
  if (lines.length > maxLines) {
    const kept = lines.slice(0, maxLines);
    kept[maxLines - 1] = `${kept[maxLines - 1]!.replace(/[ ,.;:]+$/, "")}...`;
    return kept;
  }
  return lines;
}

export interface Card {
  title: string;
  description: string;
  label: string;
}

export function cardSvg(c: Card): string {
  const logo = logoSvg("full", 44, "").replace("<svg ", '<svg x="72" y="64" ').replace(/currentColor/g, "#f2f1f7");
  const title = wrap(c.title, 74, 1000, 3);
  const desc = wrap(c.description, 30, 980, 2);
  const ty = 250;
  const titleLines = title
    .map((l, i) => `<text x="72" y="${ty + i * 84}" font-size="74" font-weight="600" letter-spacing="-2" fill="#f7f6fb">${esc(l)}</text>`)
    .join("");
  const dy = ty + title.length * 84 + 24;
  const descLines = desc
    .map((l, i) => `<text x="72" y="${dy + i * 42}" font-size="30" font-weight="400" fill="#b3b1c6">${esc(l)}</text>`)
    .join("");
  return `<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="630" viewBox="0 0 1200 630" font-family="Geist">
<defs>
<radialGradient id="g1" cx="85%" cy="-10%" r="70%"><stop offset="0" stop-color="#8c64f7" stop-opacity=".55"/><stop offset="1" stop-color="#8c64f7" stop-opacity="0"/></radialGradient>
<radialGradient id="g2" cx="0%" cy="100%" r="60%"><stop offset="0" stop-color="#6d3fd0" stop-opacity=".35"/><stop offset="1" stop-color="#6d3fd0" stop-opacity="0"/></radialGradient>
</defs>
<rect width="1200" height="630" fill="#14131a"/><rect width="1200" height="630" fill="url(#g1)"/><rect width="1200" height="630" fill="url(#g2)"/>
${logo}
<text x="1128" y="94" font-size="26" font-weight="600" text-anchor="end" fill="#9b78f0">${esc(c.label)}</text>
${titleLines}${descLines}
<rect x="72" y="570" width="64" height="4" rx="2" fill="#8c64f7"/>
</svg>`;
}

export function renderPng(svg: string): Buffer {
  const r = new Resvg(svg, {
    fitTo: { mode: "width", value: 1200 },
    font: { fontFiles: fonts(), loadSystemFonts: false, defaultFontFamily: "Geist" },
  });
  return r.render().asPng();
}

/** The favicon tile as a 180 px PNG for iOS and as the Organization logo. */
export function appleTouchIcon(): Buffer {
  const svg = readFileSync(join(SITE_ROOT, "public/favicon.svg"), "utf8")
    .replace(/<style>[\s\S]*?<\/style>/, "")
    .replace('class="t"', 'fill="#6d3fd0"');
  return new Resvg(svg, { fitTo: { mode: "width", value: 180 } }).render().asPng();
}
