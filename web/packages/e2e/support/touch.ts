import type { Page } from "@playwright/test";

/** The smallest side of a control a finger must hit (WCAG 2.5.5, and the rule of the ui kit). */
export const TOUCH = 44;

export type TableFinding = {
  /** A short path to the table. */
  sel: string;
  caption: string;
  /** The table's own box scrolls sideways: `scrollWidth - clientWidth` of its wrapper. */
  scrolls: number;
  /** The rows are blocks (a cell spans its row) instead of table rows. */
  stacked: boolean;
};

export type Small = { sel: string; text: string; w: number; h: number; inline: boolean };

export type Audit = {
  vw: number;
  /** The screen reports a touch pointer: `(pointer: coarse)` matches. */
  coarse?: boolean;
  /** How far the page itself scrolls sideways (0 when it does not). */
  pageOverflow: number;
  tables: TableFinding[];
  small: Small[];
};

/** Controls a finger must hit; a checkbox or a radio is measured by its label. */
const CONTROLS =
  "button, a[href], summary, select, input:not([type=hidden]), textarea, [role=button], [role=tab], [role=menuitem], [role=menuitemradio], [role=menuitemcheckbox], [role=switch], [role=checkbox], [role=radio], [role=option], .linklike";

/**
 * What a phone or a tablet screen shows that is wrong, from element boxes (never from the source):
 * the page scrolls sideways, a table's own box does, and which controls are under 44 px in either
 * direction. A screen still sliding in is waited for first.
 */
export async function audit(page: Page): Promise<Audit> {
  await page.evaluate(() =>
    // An animation that never ends (a spinner, a skeleton's shimmer) is not waited for.
    Promise.all(
      document
        .getAnimations()
        .filter((a) => a.effect?.getComputedTiming().endTime !== Infinity)
        .map((a) => a.finished.catch(() => null)),
    ),
  );
  await page.waitForTimeout(250);
  return page.evaluate(
    ([controls]) => {
      const vw = document.documentElement.clientWidth;
      const path = (el: Element): string => {
        const parts: string[] = [];
        for (
          let e: Element | null = el;
          e && e !== document.body && parts.length < 3;
          e = e.parentElement
        ) {
          const cls = [...e.classList].slice(0, 2).join(".");
          parts.unshift(
            `${e.tagName.toLowerCase()}${e.id ? `#${e.id}` : ""}${cls ? `.${cls}` : ""}`,
          );
        }
        return parts.join(" > ");
      };
      const hidden = (el: Element) => {
        for (let e: Element | null = el; e; e = e.parentElement) {
          if (e.classList.contains("sr-only")) return true;
          const s = getComputedStyle(e);
          if (s.display === "none" || s.visibility === "hidden" || s.clipPath === "inset(50%)")
            return true;
          if (e.hasAttribute("inert") || e.getAttribute("aria-hidden") === "true") return true;
        }
        return false;
      };
      const out: Audit = {
        vw,
        coarse: matchMedia("(pointer: coarse)").matches,
        pageOverflow: Math.max(0, document.documentElement.scrollWidth - vw),
        tables: [],
        small: [],
      };
      for (const table of document.querySelectorAll<HTMLElement>("table")) {
        const r = table.getBoundingClientRect();
        if (!r.width || hidden(table)) continue;
        let box: HTMLElement = table;
        for (let p = table.parentElement; p; p = p.parentElement) {
          const o = getComputedStyle(p).overflowX;
          if (o === "auto" || o === "scroll" || o === "hidden") {
            box = p;
            break;
          }
        }
        const row = table.querySelector("tbody > tr");
        out.tables.push({
          sel: path(table),
          caption: table.querySelector("caption")?.textContent?.trim() ?? "",
          scrolls: box === table ? 0 : Math.max(0, box.scrollWidth - box.clientWidth),
          stacked: row ? getComputedStyle(row).display !== "table-row" : false,
        });
      }
      const seen = new Set<Element>();
      for (const el of document.querySelectorAll<HTMLElement>(controls)) {
        if (hidden(el)) continue;
        let target: HTMLElement = el;
        // A field inside its label is hit through the label (a tap on the label focuses it), and a
        // checkbox, a radio or a colour picker is its label's box.
        if (el instanceof HTMLInputElement || el instanceof HTMLSelectElement) {
          target = (el.closest("label") as HTMLElement | null) ?? el;
        }
        if (seen.has(target)) continue;
        seen.add(target);
        const r = target.getBoundingClientRect();
        if (!r.width || !r.height) continue;
        // A file input's own box is not the control: its button is, and the page labels it.
        if (el instanceof HTMLInputElement && el.type === "file") continue;
        // A hit area drawn by a pseudo-element (a ring round a small avatar, a margin round a
        // checkbox in a table cell) counts: it is what a finger lands on.
        let w = r.width;
        let h = r.height;
        for (const pseudo of ["::before", "::after"]) {
          const ps = getComputedStyle(target, pseudo);
          if (ps.content !== "none" && ps.position === "absolute") {
            w = Math.max(w, parseFloat(ps.width) || 0);
            h = Math.max(h, parseFloat(ps.height) || 0);
          }
        }
        if (w >= 44 - 0.5 && h >= 44 - 0.5) continue;
        // A link inside a sentence is not a target of its own (WCAG 2.5.8 exempts it).
        const inline =
          getComputedStyle(el).display === "inline" &&
          [...(el.parentElement?.childNodes ?? [])].some(
            (n) => n.nodeType === 3 && (n.textContent ?? "").trim().length > 3,
          );
        out.small.push({
          sel: path(target),
          text: (target.textContent || target.getAttribute("aria-label") || "").trim().slice(0, 40),
          w: Math.round(w * 10) / 10,
          h: Math.round(h * 10) / 10,
          inline,
        });
      }
      return out;
    },
    [CONTROLS],
  );
}

/** A word of this many characters or more may break (a file name, an address, a token). */
const LONG_WORD = 20;

/**
 * The words of one table that a line break cuts in two, as `caption: "word"`. A table that "fits its
 * box" can still be unreadable: a column crushed to a letter or two breaks "Legal basis" into
 * "Leg / al / bas / is". Every word of every visible cell is measured with a `Range` (more than one
 * line box = broken); a stacked cell's label (generated from `data-label`) is measured by its
 * longest word against the cell's width. Words of 20 characters or more are exempt.
 */
export async function brokenWords(page: Page, caption: string): Promise<string[]> {
  return page.evaluate(
    ([cap, limit]) => {
      const table = [...document.querySelectorAll("table")].find(
        (t) => t.querySelector("caption")?.textContent?.trim() === cap,
      );
      if (!table) return [`${cap}: no such table`];
      const hidden = (el: Element) => {
        for (let e: Element | null = el; e; e = e.parentElement) {
          const s = getComputedStyle(e);
          if (s.display === "none" || s.visibility === "hidden" || s.clipPath === "inset(50%)")
            return true;
        }
        return false;
      };
      const broken: string[] = [];
      const ruler = document.createElement("span");
      ruler.style.cssText = "position:absolute;visibility:hidden;white-space:nowrap";
      document.body.append(ruler);
      for (const cell of table.querySelectorAll<HTMLElement>("th, td")) {
        if (hidden(cell)) continue;
        // The words of the whole cell, across its text nodes (a framework may split "--text-" and
        // "display" into two nodes of one element).
        const nodes: { node: Text; start: number }[] = [];
        let all = "";
        const walker = document.createTreeWalker(cell, NodeFilter.SHOW_TEXT);
        for (let n = walker.nextNode(); n; n = walker.nextNode()) {
          // Text of two elements is two words, even when no space lies between them.
          if (nodes.length && nodes[nodes.length - 1]!.node.parentElement !== n.parentElement)
            all += " ";
          nodes.push({ node: n as Text, start: all.length });
          all += n.textContent ?? "";
        }
        const at = (pos: number, end: boolean) => {
          for (let i = nodes.length - 1; i >= 0; i--) {
            const { node, start } = nodes[i]!;
            if (pos > start || (pos === start && !end)) return [node, pos - start] as const;
          }
          return [nodes[0]!.node, 0] as const;
        };
        for (const m of all.matchAll(/\S+/g)) {
          if (m[0].length >= (limit as number)) continue;
          const range = document.createRange();
          range.setStart(...at(m.index!, false));
          range.setEnd(...at(m.index! + m[0].length, true));
          const tops = [...range.getClientRects()]
            .filter((r) => r.width > 0)
            .map((r) => Math.round(r.top));
          if (tops.length && Math.max(...tops) - Math.min(...tops) > 2)
            broken.push(`${cap}: "${m[0]}"`);
        }
        const label = cell.getAttribute("data-label");
        if (label && getComputedStyle(cell, "::before").content !== "none") {
          const st = getComputedStyle(cell, "::before");
          ruler.style.font = st.font;
          for (const w of label.split(/\s+/)) {
            ruler.textContent = w;
            if (
              w.length < (limit as number) &&
              ruler.getBoundingClientRect().width > cell.clientWidth
            )
              broken.push(`${cap}: label "${w}"`);
          }
        }
      }
      ruler.remove();
      return broken;
    },
    [caption, LONG_WORD] as const,
  );
}
