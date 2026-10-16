import { describe, expect, it } from "vite-plus/test";
import {
  headingNumber,
  parseInline,
  parseMarkdown,
  plain,
  safeHref,
  searchSections,
  sections,
  toc,
} from "./markdown.ts";

describe("blocks", () => {
  it("headings get unique anchors and the number is read from the text", () => {
    const b = parseMarkdown("# Title\n\n## 6. Workflow\n\n### 6.1 Governed\n\n### 6.1 Governed\n");
    const ids = b.filter((x) => x.t === "heading").map((x) => (x.t === "heading" ? x.id : ""));
    expect(ids).toEqual(["title", "6-workflow", "61-governed", "61-governed-1"]);
    expect(headingNumber("6.1 Governed")).toBe("6.1");
    expect(headingNumber("Tiers")).toBeUndefined();
    expect(toc(b).map((t) => t.level)).toEqual([2, 3, 3]);
  });

  it("paragraphs join wrapped lines; lists keep their items", () => {
    const b = parseMarkdown("one\ntwo\n\n- a\n- b\n\n1. x\n2. y\n");
    expect(b.map((x) => x.t)).toEqual(["p", "list", "list"]);
    expect(plain((b[0] as unknown as { c: never }).c)).toBe("one two");
    expect(b[1]).toMatchObject({ ordered: false });
    expect((b[1] as { items: unknown[] }).items).toHaveLength(2);
    expect(b[2]).toMatchObject({ ordered: true });
  });

  it("tables split on pipes except inside code", () => {
    const b = parseMarkdown("| Key | Value |\n|---|---:|\n| `a|b` | 2 |\n");
    expect(b[0]).toMatchObject({ t: "table" });
    const t = b[0] as { head: unknown[]; align: unknown[]; rows: unknown[][][] };
    expect(t.head).toHaveLength(2);
    expect(t.align).toEqual([null, "right"]);
    expect(plain(t.rows[0]![0] as never)).toBe("a|b");
  });

  it("fenced code keeps its text and language, and does not interpret what is inside", () => {
    const b = parseMarkdown('```toml\n[[model]]\n# not a heading\nname = "x"\n```\n');
    expect(b).toEqual([
      { t: "code", lang: "toml", text: '[[model]]\n# not a heading\nname = "x"' },
    ]);
  });
});

describe("inline", () => {
  it("code, strong and emphasis", () => {
    const i = parseInline("a `b` **c** *d*");
    expect(i.map((x) => x.t)).toEqual(["text", "code", "text", "strong", "text", "em"]);
  });

  it("underscores inside identifiers are not emphasis", () => {
    expect(plain(parseInline("the sale_order_id field"))).toBe("the sale_order_id field");
  });

  it("everything is text: markup in the source stays as characters", () => {
    const i = parseInline("<img src=x onerror=alert(1)> and <script>alert(1)</script>");
    expect(i.every((x) => x.t === "text")).toBe(true);
    expect(plain(i)).toContain("<script>alert(1)</script>");
  });

  it("only web, mail and in-page links are made; others keep their words and lose the target", () => {
    expect(safeHref("https://example.test/a")).toBe("https://example.test/a");
    expect(safeHref("mailto:a@b.test")).toBe("mailto:a@b.test");
    expect(safeHref("#section")).toBe("#section");
    expect(safeHref("javascript:alert(1)")).toBeNull();
    expect(safeHref("data:text/html,<script>")).toBeNull();
    expect(safeHref("DECISIONS.md")).toBeNull();
    const bad = parseInline("[click](javascript:alert(1))");
    expect(bad.some((x) => x.t === "link")).toBe(false);
    expect(plain(bad)).toBe("click");
    expect(parseInline("[ok](https://example.test)")[0]).toMatchObject({ t: "link" });
  });
});

describe("search", () => {
  const doc = parseMarkdown(
    "# Guide\n\n## 1. Approvals\n\nAn agent only proposes; a human approves.\n\n## 2. Fields\n\nA field has a kind.\n",
  );
  const all = sections(doc);

  it("sections are cut at headings and carry their words", () => {
    expect(all.map((s) => s.title)).toEqual(["1. Approvals", "2. Fields"]);
    expect(all[0]!.text).toContain("proposes");
  });

  it("finds sections containing every word, best first, with an excerpt", () => {
    const hits = searchSections(all, "human approves");
    expect(hits.map((h) => h.title)).toEqual(["1. Approvals"]);
    expect(hits[0]!.snippet).toContain("human");
    expect(searchSections(all, "kind field").map((h) => h.title)).toEqual(["2. Fields"]);
    expect(searchSections(all, "zzz")).toEqual([]);
    expect(searchSections(all, "   ")).toEqual([]);
  });
});
