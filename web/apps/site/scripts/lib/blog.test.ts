import { describe, expect, test } from "vite-plus/test";
import { blogMarkdown, isPost, localPosts, postDate, postEyebrow } from "./blog.ts";

describe("blog", () => {
  test("a post is a page one level under /blog/", () => {
    expect(isPost("/blog/e-invoicing/")).toBe(true);
    expect(isPost("/blog/")).toBe(false);
    expect(isPost("/blog/a/b/")).toBe(false);
    expect(isPost("/docs/a/")).toBe(false);
  });

  test("a post needs a real date", () => {
    expect(postDate({ date: "2026-11-25" }, "f.md")).toBe("2026-11-25");
    expect(() => postDate({}, "f.md")).toThrow("f.md");
    expect(() => postDate({ date: "25.11.2026" }, "f.md")).toThrow();
    expect(() => postDate({ date: "2026-13-40" }, "f.md")).toThrow();
  });

  test("the index lists the newest post first", () => {
    const md = blogMarkdown([
      { path: "/blog/old/", title: "Old", description: "a", date: "2026-01-01" },
      { path: "/blog/new/", title: "New", description: "b", date: "2026-11-25", author: "X" },
    ]);
    expect(md.indexOf("[New]")).toBeLessThan(md.indexOf("[Old]"));
    expect(md).toContain("2026-11-25 · X");
    expect(postEyebrow({ date: "2026-01-01" })).toBe("2026-01-01");
  });

  test("a language lists a translated post by its own title, the others by the English one", () => {
    const posts = [
      { path: "/blog/a/", title: "A", description: "a", date: "2026-11-25" },
      { path: "/blog/b/", title: "B", description: "b", date: "2026-11-24" },
    ];
    const de = localPosts(posts, (p) =>
      p === "/blog/a/"
        ? { title: "A auf Deutsch", description: "a de", date: "2026-11-25" }
        : undefined,
    );
    expect(de[0]).toEqual({
      path: "/blog/a/",
      title: "A auf Deutsch",
      description: "a de",
      date: "2026-11-25",
    });
    expect(de[1]).toBe(posts[1]);
  });

  test("an empty blog says so", () => {
    expect(blogMarkdown([])).toBe("No posts yet.");
  });
});
