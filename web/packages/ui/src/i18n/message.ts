// ICU MessageFormat 1 subset: {name}, {n, number}, {n, plural, =0 {..} one {..} other {..}} with `#`,
// {x, select, a {..} other {..}}, and quoting with an apostrophe in front of { or }. A hand-written
// parser keeps the runtime small; see docs/brand/LOCALIZATION.md for why there is no library.

export type Node =
  | string
  | { kind: "arg"; name: string; style?: string }
  | { kind: "plural"; name: string; offset: number; cases: Record<string, Node[]> }
  | { kind: "select"; name: string; cases: Record<string, Node[]> }
  | { kind: "hash" };

class Parser {
  i = 0;
  s: string;
  constructor(s: string) {
    this.s = s;
  }

  parse(inPlural: boolean, until?: "}"): Node[] {
    const out: Node[] = [];
    let text = "";
    const flush = () => {
      if (text) out.push(text);
      text = "";
    };
    while (this.i < this.s.length) {
      const c = this.s[this.i]!;
      if (c === "'") {
        const n = this.s[this.i + 1];
        if (n === "'") {
          text += "'";
          this.i += 2;
        } else if (n === "{" || n === "}" || (inPlural && n === "#")) {
          // Quoted literal run: up to the next lone apostrophe.
          this.i++;
          while (this.i < this.s.length) {
            if (this.s[this.i] === "'") {
              if (this.s[this.i + 1] === "'") {
                text += "'";
                this.i += 2;
                continue;
              }
              this.i++;
              break;
            }
            text += this.s[this.i++];
          }
        } else {
          text += c;
          this.i++;
        }
      } else if (c === "{") {
        flush();
        out.push(this.argument());
      } else if (c === "}" && until) {
        break;
      } else if (c === "#" && inPlural) {
        flush();
        out.push({ kind: "hash" });
        this.i++;
      } else {
        text += c;
        this.i++;
      }
    }
    flush();
    return out;
  }

  private ws() {
    while (/\s/.test(this.s[this.i] ?? "")) this.i++;
  }

  private word(): string {
    const m = /^[^\s,{}]+/.exec(this.s.slice(this.i));
    if (!m) throw new Error(`bad message at ${this.i}: ${this.s}`);
    this.i += m[0].length;
    return m[0];
  }

  private argument(): Node {
    this.i++; // {
    this.ws();
    const name = this.word();
    this.ws();
    if (this.s[this.i] === "}") {
      this.i++;
      return { kind: "arg", name };
    }
    if (this.s[this.i] !== ",") throw new Error(`bad message at ${this.i}: ${this.s}`);
    this.i++;
    this.ws();
    const type = this.word();
    this.ws();
    if (type === "number" || type === "date" || type === "time" || type === "list") {
      let style: string | undefined;
      if (this.s[this.i] === ",") {
        this.i++;
        this.ws();
        style = this.word();
        this.ws();
      }
      this.expect("}");
      return { kind: "arg", name, style: style ? `${type}:${style}` : type };
    }
    if (this.s[this.i] === ",") this.i++;
    this.ws();
    let offset = 0;
    if (type === "plural" && this.s.startsWith("offset:", this.i)) {
      this.i += 7;
      offset = Number(this.word());
      this.ws();
    }
    const cases: Record<string, Node[]> = {};
    while (this.i < this.s.length && this.s[this.i] !== "}") {
      const key = this.word();
      this.ws();
      this.expect("{");
      cases[key] = this.parse(type === "plural", "}");
      this.expect("}");
      this.ws();
    }
    this.expect("}");
    if (!("other" in cases)) throw new Error(`missing "other" in: ${this.s}`);
    return type === "plural"
      ? { kind: "plural", name, offset, cases }
      : { kind: "select", name, cases };
  }

  private expect(ch: string) {
    if (this.s[this.i] !== ch) throw new Error(`expected ${ch} at ${this.i}: ${this.s}`);
    this.i++;
  }
}

const cache = new Map<string, Node[]>();

/** Parse (and cache) a message. A malformed message renders as itself. */
export function parseMessage(s: string): Node[] {
  let n = cache.get(s);
  if (!n) {
    try {
      n = new Parser(s).parse(false);
    } catch {
      n = [s];
    }
    cache.set(s, n);
  }
  return n;
}

/** Strict parse for the catalogue check: throws on a malformed message. */
export function parseStrict(s: string): Node[] {
  return new Parser(s).parse(false);
}

/** Everything the formatter needs from the active locale. */
export type FormatEnv<R = string> = {
  plural: (n: number) => Intl.LDMLPluralRule;
  number: (n: number, style?: string) => string;
  /** Format a non-number argument. */
  value: (v: unknown, style?: string) => R;
  /** Literal text (a pseudo-locale rewrites it). */
  text: (s: string) => string;
  join: (parts: (string | R)[]) => R;
};

export function formatNodes<R>(
  nodes: Node[],
  vars: Record<string, unknown>,
  env: FormatEnv<R>,
  hash?: string,
): R {
  const parts: (string | R)[] = [];
  for (const n of nodes) {
    if (typeof n === "string") parts.push(env.text(n));
    else if (n.kind === "hash") parts.push(hash ?? "#");
    else if (n.kind === "arg") {
      const v = vars[n.name];
      if (v === undefined) parts.push(`{${n.name}}`);
      else if (typeof v === "number") parts.push(env.number(v, n.style));
      else parts.push(env.value(v, n.style));
    } else if (n.kind === "plural") {
      const num = Number(vars[n.name]);
      const exact = n.cases[`=${num}`];
      const cat = exact ? undefined : env.plural(num - n.offset);
      const branch = exact ?? (cat && n.cases[cat]) ?? n.cases.other!;
      parts.push(formatNodes(branch, vars, env, env.number(num - n.offset)));
    } else {
      const key = String(vars[n.name]);
      parts.push(formatNodes(n.cases[key] ?? n.cases.other!, vars, env, hash));
    }
  }
  return env.join(parts);
}

/** The argument names a message uses, for the catalogue check. */
export function argNames(s: string): string[] {
  const out = new Set<string>();
  const walk = (nodes: Node[]) => {
    for (const n of nodes) {
      if (typeof n === "string" || n.kind === "hash") continue;
      out.add(n.name);
      if (n.kind !== "arg") for (const c of Object.values(n.cases)) walk(c);
    }
  };
  walk(parseStrict(s));
  return [...out].sort();
}

/** The case keys of every plural node (outermost first), for the catalogue check. */
export function pluralCases(s: string): string[][] {
  const out: string[][] = [];
  const walk = (nodes: Node[]) => {
    for (const n of nodes) {
      if (typeof n === "string" || n.kind === "hash" || n.kind === "arg") continue;
      if (n.kind === "plural") out.push(Object.keys(n.cases));
      for (const c of Object.values(n.cases)) walk(c);
    }
  };
  walk(parseStrict(s));
  return out;
}

/** Rich-text tag names (<b>, <link>) a message uses. */
export function tagNames(s: string): string[] {
  return [...new Set([...s.matchAll(/<\/?([a-z][a-z0-9]*)>/g)].map((m) => m[1]!))].sort();
}
