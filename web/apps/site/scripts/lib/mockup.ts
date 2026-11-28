/**
 * Design previews in a page (`<figure class="mockup">`, styled by `.vitepress/theme/mockup.css`):
 * static HTML pictures of a screen. The page shows the HTML; the Markdown copy (and the search
 * text, which is read from it) gets one line per preview instead, made of the figure's label and
 * caption, so the copy stays readable for people's tools and AI agents.
 */

const FIGURE = /<figure class="mockup"[^>]*>[\s\S]*?<\/figure>/g;

const text = (html: string) =>
  html
    .replace(/<[^>]+>/g, " ")
    .replace(/\s+/g, " ")
    .trim();

/** The Markdown copy of a body: each design preview becomes "> **Label:** caption". */
export function mockupMirror(md: string): string {
  return md.replace(FIGURE, (fig) => {
    const label = text(/<div class="mockup-label">([\s\S]*?)<\/div>/.exec(fig)?.[1] ?? "");
    const caption = text(/<figcaption>([\s\S]*?)<\/figcaption>/.exec(fig)?.[1] ?? "");
    if (!label || !caption) throw new Error("a design preview needs a label and a figcaption");
    return `> **${label}:** ${caption}`;
  });
}
