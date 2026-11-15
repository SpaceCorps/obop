/**
 * The heading id VitePress gives a Markdown heading (its slugify, from @mdit-vue/shared). The app
 * pages draw "What it is for" in the layout instead of in the Markdown, and its id has to stay what
 * it was, so links to it keep working; keep this in step with VitePress when it is upgraded.
 */
const char = (code: number) => String.fromCharCode(code);
// Control characters, the combining marks U+0300 to U+036F, and the characters that become "-".
const CONTROL = new RegExp(`[${char(0)}-${char(31)}]`, "g");
const COMBINING = new RegExp(`[${char(0x300)}-${char(0x36f)}]`, "g");
// In a character class a literal backslash is written as two of them.
const BACKSLASH = char(92) + char(92);
const SPECIAL = new RegExp(`[\\s~\`!@#$%^&*()\\-_+=[\\]{}|${BACKSLASH};:"'“”‘’<>,.?/]+`, "g");

export const slugify = (text: string): string =>
  text
    .normalize("NFKD")
    .replace(COMBINING, "")
    .replace(CONTROL, "")
    .replace(SPECIAL, "-")
    .replace(/-{2,}/g, "-")
    .replace(/^-+|-+$/g, "")
    .replace(/^(\d)/, "_$1")
    .toLowerCase();
