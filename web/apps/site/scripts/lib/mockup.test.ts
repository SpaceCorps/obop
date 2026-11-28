import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, test } from "vite-plus/test";
import { mockupMirror } from "./mockup.ts";
import { SITE_ROOT } from "./paths.ts";

const FIG = `<figure class="mockup">
<div class="mockup-label">Design preview</div>
<div class="mockup-card"><span class="mockup-btn">Discard</span></div>
<figcaption>Receiving: <em>sample</em> data.</figcaption>
</figure>`;

describe("design previews", () => {
  test("the Markdown copy gets one line per preview: its label and caption", () => {
    const out = mockupMirror(`Before.\n\n${FIG}\n\nAfter.`);
    expect(out).toBe("Before.\n\n> **Design preview:** Receiving: sample data.\n\nAfter.");
  });

  test("a preview without a label or caption fails the build", () => {
    expect(() =>
      mockupMirror('<figure class="mockup"><figcaption>x</figcaption></figure>'),
    ).toThrow();
  });

  test("every preview in the posts is drawn without a control, a link, a script or an image", () => {
    const content = join(SITE_ROOT, "content");
    for (const locale of readdirSync(content)) {
      const dir = join(content, locale, "blog");
      let files: string[];
      try {
        files = readdirSync(dir).filter((f) => f.endsWith(".md"));
      } catch {
        continue;
      }
      for (const f of files) {
        const md = readFileSync(join(dir, f), "utf8");
        for (const fig of md.match(/<figure class="mockup"[\s\S]*?<\/figure>/g) ?? []) {
          expect(fig, `${locale}/${f}`).not.toMatch(/<(button|a|input|script|img|svg)\b/);
          // A check is an icon and words: every item has an icon, hidden from screen readers,
          // and a spoken word for the state.
          for (const li of fig.match(/<li[\s\S]*?<\/li>/g) ?? []) {
            expect(li, `${locale}/${f}`).toMatch(/class="mockup-icon" aria-hidden="true">[^<]+</);
            expect(li, `${locale}/${f}`).toMatch(/class="sr-only">[^<]+</);
          }
          expect(mockupMirror(fig)).toMatch(/^> \*\*.+:\*\* .+/);
        }
      }
    }
  });
});
