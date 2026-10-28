// The condition of a text field's `shows_option_of` (D52 amendment, docs/MODULES.md 3.3 and 3.8): an
// expression of the expression language that the back office evaluates in the browser on the record
// it shows, because the server never evaluates it (the API answers what it always did).
//
// This is the part of the language the install gate lets through (`check_display_subset` in
// crates/kernel/obop-registry/src/expr.rs):
//
//   or     := and ( "||" and )*
//   and    := not ( "&&" not )*
//   not    := "!" not | cmp
//   cmp    := value ( ("==" | "!=" | "<" | "<=" | ">" | ">=") value )?
//   value  := INTEGER | "text" | true | false | field | is_set(field) | "(" or ")"
//
// and nothing else: no arithmetic, no minus sign, no decimals or instants, no `if`, `concat` or
// aggregates. The meaning is the server's: an empty value is `0` against a number, `""` against
// text and false as a condition; text compares by code points (the server's byte order); `&&` and
// `||` stop early; `!` binds looser than a comparison (`!a == b` is `!(a == b)`). Both sides read
// displayCondition.fixtures.json, so a change on one side fails the test of the other.
//
// One thing the fixtures cannot pin: a number or money value above 2^53 - 1 (the gate limits only the
// literals) reaches the browser as a JSON number, which is already a double, so it is right against a
// literal (it rounds to at least 2^53) and two such values compared with each other can differ from the
// server's exact answer. The server reads the same file with exact integers, so a case for it would
// fail one of the two sides.
//
// A source this parser does not understand compiles to `null`; the caller then never relabels.

type Val = null | number | string | boolean;

type Node =
  | { t: "lit"; v: Val }
  | { t: "field"; name: string }
  | { t: "isSet"; name: string }
  | { t: "not"; a: Node }
  | { t: "bin"; op: CmpOp | "&&" | "||"; a: Node; b: Node };
type CmpOp = "==" | "!=" | "<" | "<=" | ">" | ">=";

type Tok =
  | { k: "int"; v: number }
  | { k: "str"; v: string }
  | { k: "id"; v: string }
  | { k: "op"; v: string };

/** The largest whole number the gate allows in a condition (2^53 - 1). */
const MAX_INT = Number.MAX_SAFE_INTEGER;

/** Rust's `char::is_whitespace`: the Unicode White_Space property (not JavaScript's `\s`). */
function isSpace(c: number): boolean {
  return (
    (c >= 0x09 && c <= 0x0d) ||
    c === 0x20 ||
    c === 0x85 ||
    c === 0xa0 ||
    c === 0x1680 ||
    (c >= 0x2000 && c <= 0x200a) ||
    c === 0x2028 ||
    c === 0x2029 ||
    c === 0x202f ||
    c === 0x205f ||
    c === 0x3000
  );
}

/** Rust's `char::is_control`: the general category Cc. */
const isControl = (c: number) => c <= 0x1f || (c >= 0x7f && c <= 0x9f);
const isDigit = (c: number | undefined) => c !== undefined && c >= 0x30 && c <= 0x39;
const isLower = (c: number | undefined) => c !== undefined && c >= 0x61 && c <= 0x7a;
const isAlpha = (c: number | undefined) =>
  c !== undefined && (isLower(c) || (c >= 0x41 && c <= 0x5a));

/** The tokens of `src`, or `null` for anything the language (or this subset) does not have. */
function lex(src: string): Tok[] | null {
  const cs = Array.from(src, (ch) => ch.codePointAt(0)!);
  const out: Tok[] = [];
  let i = 0;
  while (i < cs.length) {
    const c = cs[i]!;
    if (isSpace(c)) {
      i++;
    } else if (isDigit(c)) {
      const start = i;
      while (isDigit(cs[i])) i++;
      // `1.5` is a decimal and `1x`, `1.` and `1_` are not numbers: none is in the subset.
      if (cs[i] === 0x2e || cs[i] === 0x5f || isAlpha(cs[i])) return null;
      const n = Number(String.fromCodePoint(...cs.slice(start, i)));
      if (!Number.isSafeInteger(n) || n > MAX_INT) return null;
      out.push({ k: "int", v: n });
    } else if (c === 0x22) {
      let s = "";
      i++;
      for (;;) {
        const d = cs[i];
        if (d === undefined) return null; // unterminated
        if (d === 0x22) {
          i++;
          break;
        }
        if (d === 0x5c) {
          const e = cs[i + 1];
          if (e !== 0x22 && e !== 0x5c) return null; // only \" and \\ may be escaped
          s += String.fromCodePoint(e);
          i += 2;
        } else if (isControl(d)) {
          return null;
        } else {
          s += String.fromCodePoint(d);
          i++;
        }
      }
      out.push({ k: "str", v: s });
    } else if (isLower(c) || c === 0x5f) {
      let s = "";
      while (isLower(cs[i]) || isDigit(cs[i]) || cs[i] === 0x5f)
        s += String.fromCodePoint(cs[i++]!);
      out.push({ k: "id", v: s });
    } else {
      const two = String.fromCodePoint(...cs.slice(i, i + 2));
      if (["==", "!=", "<=", ">=", "&&", "||"].includes(two)) {
        out.push({ k: "op", v: two });
        i += 2;
      } else if (["<", ">", "!", "(", ")"].includes(String.fromCodePoint(c))) {
        out.push({ k: "op", v: String.fromCodePoint(c) });
        i++;
      } else {
        return null; // `+ - * / [ ] , .` belong to the language but not to the subset
      }
    }
  }
  return out;
}

class Parser {
  private pos = 0;
  constructor(private toks: Tok[]) {}

  done(): boolean {
    return this.pos >= this.toks.length;
  }
  private op(v: string): boolean {
    const t = this.toks[this.pos];
    if (t?.k === "op" && t.v === v) {
      this.pos++;
      return true;
    }
    return false;
  }

  or(): Node {
    let l = this.and();
    while (this.op("||")) l = { t: "bin", op: "||", a: l, b: this.and() };
    return l;
  }
  private and(): Node {
    let l = this.not();
    while (this.op("&&")) l = { t: "bin", op: "&&", a: l, b: this.not() };
    return l;
  }
  private not(): Node {
    if (this.op("!")) return { t: "not", a: this.not() };
    return this.cmp();
  }
  private cmp(): Node {
    const l = this.value();
    for (const op of ["==", "!=", "<=", ">=", "<", ">"] as const)
      if (this.op(op)) return { t: "bin", op, a: l, b: this.value() };
    return l;
  }
  private value(): Node {
    const t = this.toks[this.pos++];
    if (!t) throw new Error("the expression ends too soon");
    if (t.k === "int" || t.k === "str") return { t: "lit", v: t.v };
    if (t.k === "op") {
      if (t.v !== "(") throw new Error(`unexpected ${t.v}`);
      const e = this.or();
      if (!this.op(")")) throw new Error("expected )");
      return e;
    }
    const call = this.toks[this.pos];
    if (call?.k === "op" && call.v === "(") {
      if (t.v !== "is_set") throw new Error(`unknown function ${t.v}`);
      this.pos++;
      const f = this.toks[this.pos++];
      if (f?.k !== "id" || !this.op(")")) throw new Error("is_set takes one field");
      return { t: "isSet", name: f.v };
    }
    if (t.v === "true") return { t: "lit", v: true };
    if (t.v === "false") return { t: "lit", v: false };
    return { t: "field", name: t.v };
  }
}

/** A stored value as the expression language sees it (`to_v` in the server's engine). */
function value(v: unknown): Val {
  if (typeof v === "string" || typeof v === "boolean") return v;
  if (typeof v === "number" && Number.isInteger(v)) return v;
  return null;
}

const truthy = (v: Val) => (typeof v === "boolean" ? v : typeof v === "number" ? v !== 0 : false);
const asInt = (v: Val) => (typeof v === "number" ? v : v === true ? 1 : 0);
const asText = (v: Val) => (v === null ? "" : String(v));

/** Order by Unicode code point, which is the order of UTF-8 bytes the server compares in. */
function compareCodePoints(a: string, b: string): number {
  const x = Array.from(a, (ch) => ch.codePointAt(0)!);
  const y = Array.from(b, (ch) => ch.codePointAt(0)!);
  for (let i = 0; i < Math.min(x.length, y.length); i++) if (x[i] !== y[i]) return x[i]! - y[i]!;
  return x.length - y.length;
}

function run(n: Node, row: Record<string, unknown>): Val {
  switch (n.t) {
    case "lit":
      return n.v;
    case "field":
      return value(row[n.name]);
    case "isSet": {
      const v = value(row[n.name]);
      return v !== null && v !== "";
    }
    case "not":
      return !truthy(run(n.a, row));
    case "bin": {
      // `&&` and `||` stop early, so a guard such as `is_set(x) && ...` is meaningful.
      if (n.op === "&&") return truthy(run(n.a, row)) && truthy(run(n.b, row));
      if (n.op === "||") return truthy(run(n.a, row)) || truthy(run(n.b, row));
      const x = run(n.a, row);
      const y = run(n.b, row);
      const ord =
        typeof x === "string" || typeof y === "string"
          ? compareCodePoints(asText(x), asText(y))
          : asInt(x) - asInt(y);
      switch (n.op) {
        case "==":
          return ord === 0;
        case "!=":
          return ord !== 0;
        case "<":
          return ord < 0;
        case "<=":
          return ord <= 0;
        case ">":
          return ord > 0;
        default:
          return ord >= 0;
      }
    }
  }
}

/** Whether a condition holds on a record. */
export type Condition = (row: Record<string, unknown>) => boolean;

/**
 * The condition of `src` as a function of a record, or `null` when `src` is not in the subset or does
 * not parse (the install gate refuses such a source, so this is a server newer than the browser).
 */
export function compileCondition(src: string): Condition | null {
  const toks = lex(src);
  if (!toks || toks.length === 0) return null;
  const p = new Parser(toks);
  let root: Node;
  try {
    root = p.or();
  } catch {
    return null;
  }
  if (!p.done()) return null;
  // As on the server (assertions and `when` of effects): it holds when it is exactly `true`.
  return (row) => run(root, row) === true;
}
