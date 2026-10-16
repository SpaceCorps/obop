/**
 * A small Markdown reader for the module guide. It produces a tree, never HTML: the renderer
 * (Guide.tsx) makes React elements from it, so everything in the source is text and nothing can
 * inject markup. Supported: headings, paragraphs, bullet and numbered lists (one nested level),
 * tables, fenced code blocks, block quotes, rules, and inline code, bold, italic and links.
 */

export type Inline =
  | { t: "text"; v: string }
  | { t: "code"; v: string }
  | { t: "strong"; c: Inline[] }
  | { t: "em"; c: Inline[] }
  | { t: "link"; href: string; c: Inline[] };

export type ListItem = { c: Inline[]; sub?: Block };
export type Align = "left" | "right" | "center" | null;

export type Block =
  | { t: "heading"; level: number; c: Inline[]; text: string; id: string }
  | { t: "p"; c: Inline[] }
  | { t: "list"; ordered: boolean; items: ListItem[] }
  | { t: "table"; head: Inline[][]; align: Align[]; rows: Inline[][][] }
  | { t: "code"; lang: string; text: string }
  | { t: "quote"; c: Block[] }
  | { t: "hr" };

/** Only these links are ever made: web pages, mail, and anchors inside the page. */
export function safeHref(url: string): string | null {
  const u = url.trim();
  if (/^(https?:\/\/|mailto:)/i.test(u)) return u;
  if (u.startsWith("#")) return u;
  return null;
}

/** Inline formatting of one run of text. Unclosed markers stay as plain text. */
export function parseInline(src: string): Inline[] {
  const out: Inline[] = [];
  let text = "";
  const flush = () => {
    if (text) out.push({ t: "text", v: text });
    text = "";
  };
  let i = 0;
  while (i < src.length) {
    const c = src[i]!;
    if (c === "\\" && i + 1 < src.length && /[\\`*_{}[\]()#+\-.!|<>]/.test(src[i + 1]!)) {
      text += src[i + 1];
      i += 2;
      continue;
    }
    if (c === "`") {
      let n = 1;
      while (src[i + n] === "`") n++;
      const fence = "`".repeat(n);
      const end = src.indexOf(fence, i + n);
      if (end > 0) {
        flush();
        out.push({ t: "code", v: src.slice(i + n, end).replace(/^ (.*) $/, "$1") });
        i = end + n;
        continue;
      }
    }
    if ((c === "*" || c === "_") && src[i + 1] === c) {
      const end = src.indexOf(c + c, i + 2);
      if (end > i + 2) {
        flush();
        out.push({ t: "strong", c: parseInline(src.slice(i + 2, end)) });
        i = end + 2;
        continue;
      }
    }
    if (c === "*" || (c === "_" && !/\w/.test(src[i - 1] ?? " "))) {
      const end = src.indexOf(c, i + 1);
      if (end > i + 1 && !/\s/.test(src[i + 1]!) && !/\s/.test(src[end - 1]!)) {
        flush();
        out.push({ t: "em", c: parseInline(src.slice(i + 1, end)) });
        i = end + 1;
        continue;
      }
    }
    if (c === "[") {
      const close = src.indexOf("](", i);
      const end = close > 0 ? src.indexOf(")", close) : -1;
      if (close > 0 && end > close) {
        const href = safeHref(src.slice(close + 2, end));
        const inner = parseInline(src.slice(i + 1, close));
        flush();
        // An unsafe or relative target is not a link: the words stay, the destination is dropped.
        if (href) out.push({ t: "link", href, c: inner });
        else out.push(...inner);
        i = end + 1;
        continue;
      }
    }
    text += c;
    i++;
  }
  flush();
  return out;
}

/** The words of some inline content, for headings, search and tooltips. */
export function plain(c: Inline[]): string {
  return c
    .map((x) => (x.t === "text" || x.t === "code" ? x.v : plain(x.c)))
    .join("")
    .replace(/\s+/g, " ")
    .trim();
}

export function slugify(text: string): string {
  return (
    text
      .toLowerCase()
      .replace(/[^\p{L}\p{N}\s-]/gu, "")
      .trim()
      .replace(/\s+/g, "-")
      .replace(/-+/g, "-") || "section"
  );
}

/** Split a table row on `|`, except inside code spans and after a backslash. */
function cells(line: string): string[] {
  let s = line.trim();
  if (s.startsWith("|")) s = s.slice(1);
  if (s.endsWith("|") && !s.endsWith("\\|")) s = s.slice(0, -1);
  const out: string[] = [];
  let cur = "";
  let inCode = false;
  for (let i = 0; i < s.length; i++) {
    const c = s[i]!;
    if (c === "\\" && s[i + 1] === "|") {
      cur += "|";
      i++;
    } else if (c === "`") {
      inCode = !inCode;
      cur += c;
    } else if (c === "|" && !inCode) {
      out.push(cur.trim());
      cur = "";
    } else cur += c;
  }
  out.push(cur.trim());
  return out;
}

const isRule = (l: string) => /^\s*([-*_])(\s*\1){2,}\s*$/.test(l);
const isSep = (l: string) =>
  /^\s*\|?\s*:?-{2,}:?\s*(\|\s*:?-{2,}:?\s*)*\|?\s*$/.test(l) && l.includes("-");
const bullet = /^(\s*)([-*+])\s+(.*)$/;
const numbered = /^(\s*)(\d+)[.)]\s+(.*)$/;

export function parseMarkdown(src: string): Block[] {
  const lines = src.replace(/\r\n?/g, "\n").split("\n");
  const blocks: Block[] = [];
  const used = new Map<string, number>();
  const unique = (base: string) => {
    const n = used.get(base) ?? 0;
    used.set(base, n + 1);
    return n ? `${base}-${n}` : base;
  };

  const isBlockStart = (l: string) =>
    /^(#{1,6}\s|```|>\s?)/.test(l) || bullet.test(l) || numbered.test(l) || isRule(l);

  let i = 0;
  while (i < lines.length) {
    const line = lines[i]!;
    if (!line.trim()) {
      i++;
      continue;
    }
    const fence = /^(\s*)(`{3,}|~{3,})\s*([\w+-]*)\s*$/.exec(line);
    if (fence) {
      const marker = fence[2]!;
      const body: string[] = [];
      i++;
      while (i < lines.length && !lines[i]!.trim().startsWith(marker)) {
        // Remove the indentation of the opening fence from every line.
        body.push(lines[i]!.startsWith(fence[1]!) ? lines[i]!.slice(fence[1]!.length) : lines[i]!);
        i++;
      }
      i++; // the closing fence
      blocks.push({ t: "code", lang: fence[3] ?? "", text: body.join("\n") });
      continue;
    }
    const h = /^(#{1,6})\s+(.*?)\s*#*\s*$/.exec(line);
    if (h) {
      const c = parseInline(h[2]!);
      const text = plain(c);
      blocks.push({ t: "heading", level: h[1]!.length, c, text, id: unique(slugify(text)) });
      i++;
      continue;
    }
    if (isRule(line)) {
      blocks.push({ t: "hr" });
      i++;
      continue;
    }
    if (line.startsWith("|") && i + 1 < lines.length && isSep(lines[i + 1]!)) {
      const head = cells(line).map(parseInline);
      const align: Align[] = cells(lines[i + 1]!).map((s) =>
        s.startsWith(":") && s.endsWith(":")
          ? "center"
          : s.endsWith(":")
            ? "right"
            : s.startsWith(":")
              ? "left"
              : null,
      );
      const rows: Inline[][][] = [];
      i += 2;
      while (i < lines.length && lines[i]!.trim().startsWith("|")) {
        rows.push(cells(lines[i]!).map(parseInline));
        i++;
      }
      blocks.push({ t: "table", head, align, rows });
      continue;
    }
    if (/^>\s?/.test(line)) {
      const body: string[] = [];
      while (i < lines.length && /^>\s?/.test(lines[i]!)) {
        body.push(lines[i]!.replace(/^>\s?/, ""));
        i++;
      }
      blocks.push({ t: "quote", c: parseMarkdown(body.join("\n")) });
      continue;
    }
    const first = bullet.exec(line) ?? numbered.exec(line);
    if (first) {
      const ordered = !bullet.test(line);
      const baseIndent = first[1]!.length;
      const items: ListItem[] = [];
      while (i < lines.length) {
        const l = lines[i]!;
        const m = (ordered ? numbered : bullet).exec(l);
        if (m && m[1]!.length <= baseIndent + 1) {
          const parts = [m[3]!];
          i++;
          const sub: string[] = [];
          while (i < lines.length && lines[i]!.trim()) {
            const nxt = lines[i]!;
            const nest = bullet.exec(nxt) ?? numbered.exec(nxt);
            if (nest && nest[1]!.length > baseIndent + 1) sub.push(nxt);
            else if (nest) break;
            else if (sub.length) sub.push(nxt);
            else if (/^\s+\S/.test(nxt)) parts.push(nxt.trim()); // a wrapped line of the item
            else break;
            i++;
          }
          const item: ListItem = { c: parseInline(parts.join(" ")) };
          if (sub.length) {
            const inner = parseMarkdown(
              sub.map((s) => s.slice(Math.min(2, s.length - s.trimStart().length))).join("\n"),
            );
            if (inner.length) item.sub = inner[0];
          }
          items.push(item);
          // A blank line between items of one list does not end it.
          if (
            i < lines.length &&
            !lines[i]!.trim() &&
            (ordered ? numbered : bullet).test(lines[i + 1] ?? "")
          )
            i++;
        } else break;
      }
      blocks.push({ t: "list", ordered, items });
      continue;
    }
    // A paragraph runs until a blank line or the start of another block.
    const para = [line.trim()];
    i++;
    while (
      i < lines.length &&
      lines[i]!.trim() &&
      !isBlockStart(lines[i]!) &&
      !lines[i]!.startsWith("|")
    ) {
      para.push(lines[i]!.trim());
      i++;
    }
    blocks.push({ t: "p", c: parseInline(para.join(" ")) });
  }
  return blocks;
}

export type TocEntry = { level: number; id: string; text: string; number?: string };

/** The number a heading starts with ("6.1" in "6.1 Governed fields"), if any. */
export const headingNumber = (text: string): string | undefined =>
  /^(\d+(?:\.\d+)?)\.?\s/.exec(text)?.[1];

/** Headings of level 2 and 3, for the table of contents. */
export function toc(blocks: Block[]): TocEntry[] {
  return blocks
    .filter(
      (b): b is Extract<Block, { t: "heading" }> =>
        b.t === "heading" && b.level >= 2 && b.level <= 3,
    )
    .map((b) => ({ level: b.level, id: b.id, text: b.text, number: headingNumber(b.text) }));
}

export type Section = { id: string; title: string; level: number; text: string };

function blockText(b: Block): string {
  switch (b.t) {
    case "heading":
      return b.text;
    case "p":
      return plain(b.c);
    case "list":
      return b.items.map((x) => plain(x.c) + (x.sub ? ` ${blockText(x.sub)}` : "")).join(" ");
    case "table":
      return [...b.head, ...b.rows.flat()].map(plain).join(" ");
    case "code":
      return b.text;
    case "quote":
      return b.c.map(blockText).join(" ");
    default:
      return "";
  }
}

/** The document cut at every heading of level 2 or 3, each with its words, for search. */
export function sections(blocks: Block[]): Section[] {
  const out: Section[] = [];
  let cur: Section | null = null;
  for (const b of blocks) {
    if (b.t === "heading" && b.level >= 2 && b.level <= 3) {
      cur = { id: b.id, title: b.text, level: b.level, text: "" };
      out.push(cur);
    } else if (cur) cur.text += `${blockText(b)}\n`;
  }
  return out;
}

export type Hit = { id: string; title: string; snippet: string; score: number };

/** Sections that contain every word of the query, best first, with a short excerpt around it. */
export function searchSections(all: Section[], query: string): Hit[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (!words.length) return [];
  const hits: Hit[] = [];
  for (const s of all) {
    const hay = `${s.title}\n${s.text}`.toLowerCase();
    if (!words.every((w) => hay.includes(w))) continue;
    const inTitle = words.every((w) => s.title.toLowerCase().includes(w));
    const at = Math.max(0, s.text.toLowerCase().indexOf(words[0]!));
    const from = Math.max(0, at - 60);
    const raw = s.text
      .slice(from, from + 180)
      .replace(/\s+/g, " ")
      .trim();
    hits.push({
      id: s.id,
      title: s.title,
      snippet: `${from > 0 ? "…" : ""}${raw}${from + 180 < s.text.length ? "…" : ""}`,
      score: (inTitle ? 100 : 0) + words.reduce((n, w) => n + (hay.split(w).length - 1), 0),
    });
  }
  return hits.sort((a, b) => b.score - a.score);
}
