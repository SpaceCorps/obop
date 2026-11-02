/**
 * Test support: reads the source of the screens and finds text the driver would read that did not
 * come from the catalogue, and the keys the sources use. Nothing in the app imports it.
 *
 * It reads text, not syntax: the sources are TSX written in one style (`prettier`-like formatting,
 * props as `name={...}` or `name="..."`). That is enough to catch the mistake it exists for (a
 * sentence typed into a screen, which stays English in a Swedish app) and its blind spots are listed
 * in docs/spec/mobile.md 8.9. A text that is not a sentence (a `testID`, a style, an enum word such
 * as `variant="ghost"`) is not looked at: only the props below that carry words.
 */

/** Props and keys of objects whose value the driver reads (or hears from a screen reader). */
const TEXT_PROPS = [
  "label",
  "title",
  "message",
  "hint",
  "helper",
  "subtitle",
  "placeholder",
  "value",
  "accessibilityLabel",
  "accessibilityHint",
  "notificationTitle",
  "notificationBody",
  "text",
];

const WORD = /[A-Za-zÅÄÖåäö]{2,}/;

/** A template literal's text without its `${...}` parts (braces inside a part are balanced). */
function templateText(template: string): string {
  let out = "";
  let depth = 0;
  for (let i = 0; i < template.length; i++) {
    const c = template[i]!;
    if (depth === 0 && c === "$" && template[i + 1] === "{") {
      depth = 1;
      i++;
    } else if (depth > 0) {
      if (c === "{") depth++;
      else if (c === "}") depth--;
    } else out += c;
  }
  return out;
}

export interface Finding {
  line: number;
  kind: string;
  text: string;
}

const lineOf = (source: string, index: number): number => source.slice(0, index).split("\n").length;

/** Text in a source that a driver would read and that is not a call of the catalogue. Empty: none. */
export function bypassesOf(source: string): Finding[] {
  const found: Finding[] = [];
  const add = (index: number, kind: string, text: string) =>
    found.push({ line: lineOf(source, index), kind, text: text.trim().replace(/\s+/g, " ") });

  // JSX text: words between a tag and the next closing tag.
  for (const m of source.matchAll(/>([^<>{}]*)<\//g)) {
    if (WORD.test(m[1]!)) add(m.index!, "jsx text", m[1]!);
  }
  // A prop with words as its literal value: label="Save", title={"Save"}, message={`Saved ${n}`}.
  const props = TEXT_PROPS.join("|");
  const literal = new RegExp(
    `\\b(?:${props})=(?:"([^"]*)"|\\{\\s*"([^"]*)"\\s*\\}|\\{\\s*'([^']*)'\\s*\\}|\\{\\s*\`([^\`]*)\`\\s*\\})`,
    "g",
  );
  for (const m of source.matchAll(literal)) {
    const text = m[1] ?? m[2] ?? m[3] ?? templateText(m[4] ?? "");
    if (WORD.test(text)) add(m.index!, "prop", text);
  }
  // A key of an object that holds words: { text: "Cancel" }, { label: "Retry" }.
  const keyed = new RegExp(`\\b(?:${props})\\s*:\\s*(?:"([^"]*)"|'([^']*)'|\`([^\`]*)\`)`, "g");
  for (const m of source.matchAll(keyed)) {
    const text = m[1] ?? m[2] ?? templateText(m[3] ?? "");
    if (WORD.test(text)) add(m.index!, "object", text);
  }
  // A dialog or a spoken announcement whose first argument is a literal.
  for (const m of source.matchAll(/\b(?:Alert\.alert|announce)\(\s*(["'`])([^"'`]*)\1/g)) {
    if (WORD.test(m[2]!)) add(m.index!, "call", m[2]!);
  }
  return found.sort((a, b) => a.line - b.line);
}

/**
 * The prefixes a source builds keys from with a template: t(`purpose.${p}`) gives `purpose`. The
 * key that comes out is only known when the app runs, so a test lists what each family may be.
 */
export function keyFamilies(source: string): Set<string> {
  const families = new Set<string>();
  for (const m of source.matchAll(/`([a-z_]+(?:\.[a-z_]+)*)\.\$\{/g)) families.add(m[1]!);
  return families;
}

/** Whether the source has the key (or the name of a family of plural forms) as a string literal. */
export function mentions(source: string, name: string): boolean {
  return source.includes(`"${name}"`);
}
