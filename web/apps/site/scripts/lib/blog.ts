/**
 * The blog: one Markdown file per post under content/en/blog/ (the index is content/en/blog/index.md).
 * A post's front matter needs `date` (YYYY-MM-DD) besides `title` and `description`; `author` is
 * optional. The index lists the posts newest first through the token %%blog%%.
 */

export interface Post {
  /** "/blog/e-invoicing-for-germany-and-sweden/" */
  path: string;
  title: string;
  description: string;
  date: string;
  author?: string;
}

const DATE = /^\d{4}-\d{2}-\d{2}$/;

export const isPost = (path: string) => /^\/blog\/[^/]+\/$/.test(path);

/** Checks a post's date; throws with the file name so the build says which post is wrong. */
export function postDate(fm: Record<string, string>, file: string): string {
  const d = fm["date"];
  if (!d || !DATE.test(d) || Number.isNaN(Date.parse(`${d}T00:00:00Z`)))
    throw new Error(`${file}: a blog post needs date: YYYY-MM-DD`);
  return d;
}

/** The line above a post's title: its date, and its author when there is one. */
export const postEyebrow = (p: Pick<Post, "date" | "author">) =>
  p.author ? `${p.date} · ${p.author}` : p.date;

/**
 * The posts as one language lists them: a translated post's own title and description, the
 * English ones where the post is not translated. The date and author always come from English.
 */
export const localPosts = (
  posts: Post[],
  own: (path: string) => Record<string, string> | undefined,
): Post[] =>
  posts.map((p) => {
    const fm = own(p.path);
    return fm
      ? { ...p, title: fm["title"] ?? p.title, description: fm["description"] ?? p.description }
      : p;
  });

/** Markdown for the index: newest first, ties by title. */
export function blogMarkdown(posts: Post[]): string {
  if (posts.length === 0) return "No posts yet.";
  return [...posts]
    .sort((a, b) => b.date.localeCompare(a.date) || a.title.localeCompare(b.title))
    .map((p) => `### [${p.title}](${p.path})\n\n${postEyebrow(p)}\n\n${p.description}`)
    .join("\n\n");
}
