/**
 * Test support: reads the source of the screens and finds text the driver would read that did not
 * come from the catalogue, and the keys the sources use. Nothing in the app imports it.
 *
 * It reads text, not a syntax tree: the sources are TSX written in one style (props as
 * `name={...}` or `name="..."`); it matches brackets and quotes so that it can read the whole value
 * of a prop and every argument of a dialog, but it does not parse. That is enough to catch the
 * mistake it exists for (a sentence typed into a screen, which stays English in a Swedish app); its
 * blind spots are listed on `bypassesOf` and in docs/ISSUES.md L55. A text that is not a sentence
 * (a `testID`, a style, an enum word such as `variant="ghost"`) is not looked at: only the props
 * below that carry words.
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

/**
 * Props that carry words only as a JSX attribute (`<Field error="..." />`). As a key of an object
 * the name is also a state or a field (`{ error: false }`), so the key scan does not read it.
 */
const JSX_ONLY_PROPS = ["error"];

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

const CLOSER: Record<string, string> = { "(": ")", "[": "]", "{": "}" };

/** The index just after the string literal that starts at `start` (a quote), or -1. `${...}` of a template is code. */
function endOfString(source: string, start: number): number {
  const quote = source[start]!;
  for (let i = start + 1; i < source.length; i++) {
    const c = source[i]!;
    if (c === "\\") {
      i++;
    } else if (c === quote) {
      return i + 1;
    } else if (quote === "`" && c === "$" && source[i + 1] === "{") {
      const close = closing(source, i + 1);
      if (close < 0) return -1;
      i = close;
    }
  }
  return -1;
}

/** The index of the bracket that closes the one at `open` (strings are skipped), or -1. */
function closing(source: string, open: number): number {
  const stack = [CLOSER[source[open]!]!];
  for (let i = open + 1; i < source.length; i++) {
    const c = source[i]!;
    if (c === '"' || c === "'" || c === "`") {
      const end = endOfString(source, i);
      if (end < 0) return -1;
      i = end - 1;
    } else if (CLOSER[c] !== undefined) {
      stack.push(CLOSER[c]!);
    } else if (c === stack[stack.length - 1]) {
      stack.pop();
      if (stack.length === 0) return i;
    }
  }
  return -1;
}

/**
 * The string literals of an expression that are not inside brackets: `busy ? "Waiting" : "Start"`
 * gives both, `t("a")` and `[{ text: "x" }]` give none (a call passes data to a function; the
 * buttons of a dialog are read by the key scan). A literal that is compared (`x === "ios"`) is a
 * value, not a text. `null` when the brackets or strings of the expression do not match.
 */
function topLevelLiterals(expression: string): string[] | null {
  const out: string[] = [];
  for (let i = 0; i < expression.length; i++) {
    const c = expression[i]!;
    if (CLOSER[c] !== undefined) {
      const end = closing(expression, i);
      if (end < 0) return null;
      i = end;
    } else if (c === '"' || c === "'" || c === "`") {
      const end = endOfString(expression, i);
      if (end < 0) return null;
      const raw = expression.slice(i + 1, end - 1);
      const before = expression.slice(0, i).trimEnd();
      const after = expression.slice(end).trimStart();
      const compared = /(?:===|!==|==|!=)$/.test(before) || /^(?:===|!==|==|!=)/.test(after);
      if (!compared) out.push(c === "`" ? templateText(raw) : raw);
      i = end - 1;
    }
  }
  return out;
}

/** The arguments of a call, split at the commas that are not inside brackets or strings. */
function splitArguments(list: string): string[] | null {
  const parts: string[] = [];
  let from = 0;
  for (let i = 0; i < list.length; i++) {
    const c = list[i]!;
    if (CLOSER[c] !== undefined) {
      const end = closing(list, i);
      if (end < 0) return null;
      i = end;
    } else if (c === '"' || c === "'" || c === "`") {
      const end = endOfString(list, i);
      if (end < 0) return null;
      i = end - 1;
    } else if (c === ",") {
      parts.push(list.slice(from, i));
      from = i + 1;
    }
  }
  parts.push(list.slice(from));
  return parts;
}

export interface Finding {
  line: number;
  kind: string;
  text: string;
}

const lineOf = (source: string, index: number): number => source.slice(0, index).split("\n").length;

/**
 * Text in a source that a driver would read and that is not a call of the catalogue. Empty: none.
 *
 * What it reads: JSX text; a text prop (`label`, `title`, ... and `error`) whose value is a literal,
 * a template, or an expression with literals outside brackets (a ternary, `??`, `||`, `+`); a key
 * of an object that holds words (`{ text: "Cancel" }`); every argument of `Alert.alert(...)` and
 * `announce(...)` that has a literal outside brackets (the title AND the message); and a child
 * expression of a JSX element (`<Text>{busy ? "Saving" : t("a")}</Text>`). A finding of kind
 * `unbalanced` means the scanner could not match the brackets or quotes of what it was reading.
 *
 * What it does not read (docs/ISSUES.md L55): a text that reaches a prop through a variable
 * (`const text = "Hello"; <Banner message={text} />`), a literal inside brackets (a call such as
 * `upper("Hello")`, a parenthesised ternary), an object value that is a ternary (`{ text: a ? "x" :
 * "y" }`), a child expression that holds JSX, a prop that is not in the lists above, and any file
 * the callers do not give it.
 */
export function bypassesOf(source: string): Finding[] {
  const found: Finding[] = [];
  const add = (index: number, kind: string, text: string) =>
    found.push({ line: lineOf(source, index), kind, text: text.trim().replace(/\s+/g, " ") });
  const addLiterals = (index: number, kind: string, expression: string) => {
    const literals = topLevelLiterals(expression);
    if (literals === null) return add(index, "unbalanced", expression);
    for (const text of literals) if (WORD.test(text)) add(index, kind, text);
  };

  // JSX text: words between a tag and the next closing tag.
  for (const m of source.matchAll(/>([^<>{}]*)<\//g)) {
    if (WORD.test(m[1]!)) add(m.index!, "jsx text", m[1]!);
  }
  // A child expression of an element: <Text>{busy ? "Saving" : t("a")}</Text>. After `=>` the
  // brace is the body of a function, not a child.
  for (const m of source.matchAll(/(?<!=)>\s*\{/g)) {
    const open = m.index! + m[0].length - 1;
    const close = closing(source, open);
    if (close < 0) {
      add(open, "unbalanced", source.slice(open, open + 40));
      continue;
    }
    const expression = source.slice(open + 1, close);
    if (!expression.includes("<")) addLiterals(open, "jsx expression", expression);
  }
  // A text prop as a JSX attribute: label="Save", label='Save', label={"Save"}, label={`Saved ${n}`},
  // label={busy ? "Waiting" : "Start"}.
  const attributes = new RegExp(`\\b(?:${[...TEXT_PROPS, ...JSX_ONLY_PROPS].join("|")})=`, "g");
  for (const m of source.matchAll(attributes)) {
    const at = m.index! + m[0].length;
    const c = source[at];
    if (c === '"' || c === "'") {
      const end = endOfString(source, at);
      const text = end < 0 ? "" : source.slice(at + 1, end - 1);
      if (WORD.test(text)) add(m.index!, "prop", text);
    } else if (c === "{") {
      const close = closing(source, at);
      if (close < 0) add(m.index!, "unbalanced", source.slice(at, at + 40));
      else addLiterals(m.index!, "prop", source.slice(at + 1, close));
    }
  }
  // A key of an object that holds words: { text: "Cancel" }, { label: "Retry" }.
  const keyed = new RegExp(
    `\\b(?:${TEXT_PROPS.join("|")})\\s*:\\s*(?:"([^"]*)"|'([^']*)'|\`([^\`]*)\`)`,
    "g",
  );
  for (const m of source.matchAll(keyed)) {
    const text = m[1] ?? m[2] ?? templateText(m[3] ?? "");
    if (WORD.test(text)) add(m.index!, "object", text);
  }
  // A dialog or a spoken announcement: every argument is read, not only the first.
  for (const m of source.matchAll(/\b(?:Alert\.alert|announce)\(/g)) {
    const open = m.index! + m[0].length - 1;
    const close = closing(source, open);
    const args = close < 0 ? null : splitArguments(source.slice(open + 1, close));
    if (args === null) {
      add(m.index!, "unbalanced", source.slice(open, open + 40));
      continue;
    }
    // The buttons of a dialog (an array) are read by the key scan above.
    for (const arg of args)
      if (!arg.trimStart().startsWith("[")) addLiterals(m.index!, "call", arg);
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
