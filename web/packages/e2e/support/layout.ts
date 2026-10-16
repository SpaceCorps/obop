import type { Page } from "@playwright/test";

/**
 * Layout problems on the current screen, found with element boxes: the page scrolls sideways, a
 * control sticks out of the viewport (unless it sits in a container that scrolls), or the text of a
 * button, tab, label or heading is cut off (its content is wider than its box and it clips). Text in
 * a `lang="en"` island (the guide, the brand book) is skipped.
 */
export async function layoutProblems(page: Page): Promise<string[]> {
  // A panel that is still sliding in widens the page for a moment.
  await page.evaluate(() =>
    Promise.all(document.getAnimations().map((a) => a.finished.catch(() => null))),
  );
  return page.evaluate(() => {
    const vw = document.documentElement.clientWidth;
    const out: string[] = [];
    if (document.documentElement.scrollWidth > vw + 1)
      out.push(`the page scrolls sideways: ${document.documentElement.scrollWidth} > ${vw}`);
    const sel =
      "button, [role=tab], [role=radio], a.btn, select, input:not([type=hidden]), label.check, .badge, .chip, h1, h2, h3, h4, th, .app-tile, .menu-item, .fp-item";
    const scrolls = (el: Element) => {
      for (let p = el.parentElement; p; p = p.parentElement) {
        const o = getComputedStyle(p).overflowX;
        if ((o === "auto" || o === "scroll") && p.scrollWidth > p.clientWidth) return true;
      }
      return false;
    };
    for (const el of document.querySelectorAll<HTMLElement>(sel)) {
      const r = el.getBoundingClientRect();
      if (!r.width || !r.height || el.closest("[lang=en], [hidden], [aria-hidden=true]")) continue;
      const cs = getComputedStyle(el);
      if (cs.visibility === "hidden" || cs.display === "none") continue;
      const name = `${el.tagName.toLowerCase()} "${(el.textContent || el.getAttribute("aria-label") || "").trim().slice(0, 40)}"`;
      const cut =
        el.scrollWidth > el.clientWidth + 1 &&
        cs.overflowX !== "visible" &&
        !(el instanceof HTMLInputElement) &&
        !(el instanceof HTMLSelectElement);
      if (cut) out.push(`text is cut off in ${name}: ${el.scrollWidth} > ${el.clientWidth}`);
      if ((r.right > vw + 1 || r.left < -1) && !scrolls(el))
        out.push(
          `${name} is outside the screen: ${Math.round(r.left)}..${Math.round(r.right)} of ${vw}`,
        );
    }
    return [...new Set(out)];
  });
}
