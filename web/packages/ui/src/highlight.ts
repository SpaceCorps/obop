/**
 * A small, safe syntax highlighter. It returns tokens, never markup, so the caller renders them
 * as text nodes and nothing in the source can inject HTML.
 *
 * It knows TOML (obop.toml manifests), JSON and shell well enough for the module guide; any other
 * language is returned as plain text.
 */
export type TokenClass = "comment" | "string" | "number" | "keyword" | "key" | "table" | "punct";
export type Token = { text: string; cls?: TokenClass };

const STRING = /^"(?:[^"\\\n]|\\.)*"?|^'[^'\n]*'?/;
const NUMBER = /^-?\d[\d_]*(?:\.\d+)?(?:[eE][+-]?\d+)?\b/;

function push(out: Token[], text: string, cls?: TokenClass) {
  if (!text) return;
  const last = out[out.length - 1];
  if (last && last.cls === cls) last.text += text;
  else out.push({ text, cls });
}

/** Tokens of one line of TOML. */
function toml(line: string): Token[] {
  const out: Token[] = [];
  const header = /^(\s*)(\[\[?[^\]\n]*\]\]?)(.*)$/.exec(line);
  if (header) {
    push(out, header[1]!);
    push(out, header[2]!, "table");
    out.push(...value(header[3]!));
    return out;
  }
  const kv = /^(\s*)([A-Za-z0-9_.-]+|"[^"\n]*")(\s*=\s*)(.*)$/.exec(line);
  if (kv) {
    push(out, kv[1]!);
    push(out, kv[2]!, "key");
    push(out, kv[3]!, "punct");
    out.push(...value(kv[4]!));
    return out;
  }
  return value(line);
}

/** Tokens of a value or any free text: strings, numbers, words, comments. */
function value(src: string): Token[] {
  const out: Token[] = [];
  let rest = src;
  while (rest) {
    const c = rest[0]!;
    let m: RegExpExecArray | null;
    if (c === "#") {
      push(out, rest, "comment");
      break;
    } else if ((m = STRING.exec(rest))) {
      push(out, m[0], "string");
    } else if ((m = NUMBER.exec(rest))) {
      push(out, m[0], "number");
    } else if ((m = /^(?:true|false|null)\b/.exec(rest))) {
      push(out, m[0], "keyword");
    } else if ("[]{},=:".includes(c)) {
      push(out, c, "punct");
      m = [c] as unknown as RegExpExecArray;
    } else {
      m = /^[^"'#\s[\]{},=:]+|^\s+/.exec(rest) ?? ([c] as unknown as RegExpExecArray);
      push(out, m[0]);
    }
    rest = rest.slice(m[0].length);
  }
  return out;
}

function json(line: string): Token[] {
  const out: Token[] = [];
  let rest = line;
  while (rest) {
    const m = STRING.exec(rest);
    if (m) {
      const after = rest.slice(m[0].length);
      push(out, m[0], /^\s*:/.test(after) ? "key" : "string");
      rest = after;
      continue;
    }
    const n = NUMBER.exec(rest) ?? /^(?:true|false|null)\b/.exec(rest);
    if (n) {
      push(out, n[0], /^\d|^-/.test(n[0]) ? "number" : "keyword");
      rest = rest.slice(n[0].length);
      continue;
    }
    const p = /^[^"'\d\-tfn]+|^./.exec(rest)!;
    push(out, p[0], /^[[\]{},:]+$/.test(p[0]) ? "punct" : undefined);
    rest = rest.slice(p[0].length);
  }
  return out;
}

function shell(line: string): Token[] {
  const out: Token[] = [];
  const hash = line.search(/(^|\s)#/);
  const code = hash < 0 ? line : line.slice(0, hash + (line[hash] === "#" ? 0 : 1));
  const comment = hash < 0 ? "" : line.slice(code.length);
  let rest = code;
  while (rest) {
    const m = STRING.exec(rest);
    if (m) {
      push(out, m[0], "string");
      rest = rest.slice(m[0].length);
      continue;
    }
    const w = /^[^"'\s]+|^\s+|^./.exec(rest)!;
    push(out, w[0], /^--?[a-z][\w-]*$/i.test(w[0]) ? "keyword" : undefined);
    rest = rest.slice(w[0].length);
  }
  push(out, comment, "comment");
  return out;
}

const LANGS: Record<string, (line: string) => Token[]> = {
  toml,
  json,
  sh: shell,
  bash: shell,
  shell: shell,
  zsh: shell,
};

/** The language names this highlighter knows. */
export const HIGHLIGHT_LANGS = Object.keys(LANGS);

/** Tokens per line of `code`. Unknown languages come back as one plain token per line. */
export function highlight(code: string, lang: string): Token[][] {
  const fn = LANGS[lang.toLowerCase()];
  return code.split("\n").map((l) => (fn ? fn(l) : [{ text: l }]));
}
