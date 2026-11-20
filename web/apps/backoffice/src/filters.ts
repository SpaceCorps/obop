/**
 * The filters the control panel offers without anyone defining them (UI plan 2.9, item 4), and the
 * fields its search box offers. Pure functions over `/api/meta`; ControlPanel and FilterPanel draw
 * them.
 *
 * Generated, in the order the panel lists them:
 * - the package's saved filters (`ui.filter`, D95: `{ label, where }`), when `where` is a
 *   conjunction the browser can read (see `parseWhere`); a filter it cannot read is not offered;
 * - Open: the states of the governed status field that still have a move forward (from the action
 *   graph);
 * - Overdue (Expired for an expiry date): a due-like date before today on a record that is open;
 * - Mine: the person field (the owner the server already knows from `access.own`, else a text
 *   field named owner, assignee, requested_by and the like) equals the signed-in user name;
 * - Archived: `active` is off;
 * - date ranges on the main dates: Today, This week, This month, Last 30 days, This year.
 *
 * A generated filter is plain conditions of query.ts, so the URL, favourites, the server query
 * (D49) and the chips need nothing new. A range is stored with the days it means when it is picked.
 */
import { getLang, labelFor, msg, t } from "@obop/ui";
import type { FieldMeta, ModelMeta } from "./api.ts";
import { optionLabel } from "./format.ts";
import { fieldTitle } from "./labels.ts";
import { isIdLike, searchFieldsOf, stateFieldOf, toneOf } from "./presentation.ts";
import type { Cond } from "./query.ts";

/** Data classes never offered as a search target (an IBAN, a health note, a national id). */
const PRIVATE = new Set(["secret", "confidential_hr", "special_category", "financial"]);

/** Whether the search box may offer a field as a target. */
export const searchableField = (f: FieldMeta) => !PRIVATE.has(f.class) && !isIdLike(f);

/**
 * At most five fields the search box offers ("Search Customer for: …"): the fields a record is
 * searched by (its title, number, name, code), then references, then selections. Never a private
 * class, never an identifier.
 */
export function searchTargets(model: ModelMeta, fields: FieldMeta[], max = 5): FieldMeta[] {
  const shown = new Map(fields.map((f) => [f.name, f]));
  const out: FieldMeta[] = [];
  const add = (f: FieldMeta | undefined) => {
    if (!f || out.includes(f) || !shown.has(f.name) || !searchableField(f)) return;
    if (!["text", "longtext", "select", "ref"].includes(f.kind)) return;
    out.push(f);
  };
  for (const n of searchFieldsOf(model)) add(shown.get(n));
  for (const f of fields) if (f.sequence) add(f);
  for (const f of fields) if (f.kind === "ref" && f.required) add(f);
  for (const f of fields) if (f.kind === "ref") add(f);
  for (const f of fields) if (f.kind === "select") add(f);
  return out.slice(0, max);
}

// ---- conditions ----------------------------------------------------------------------------------

/** Two conditions mean the same records (the values of `is` and `isnot` in any order). */
export function sameCond(a: Cond, b: Cond): boolean {
  if (a.op !== b.op) return false;
  if ((a.op === "is" || a.op === "isnot") && (b.op === "is" || b.op === "isnot")) {
    if (a.f !== b.f || a.v.length !== b.v.length) return false;
    const set = new Set(a.v);
    return b.v.every((v) => set.has(v));
  }
  if (a.op === "range" && b.op === "range")
    return a.f === b.f && (a.min ?? "") === (b.min ?? "") && (a.max ?? "") === (b.max ?? "");
  return JSON.stringify(a) === JSON.stringify(b);
}

/** A quick filter of the panel: its words and the conditions it adds. */
export type QuickFilter = {
  key: string;
  /** Translated already. */
  label: string;
  conds: Cond[];
  kind: "saved" | "open" | "overdue" | "mine" | "archived" | "date";
  /** A date range: the field it is on, and the range's own name ("This month"). */
  field?: string;
  range?: string;
};

/** Every condition of the filter is in the search. */
export const isOn = (facets: readonly Cond[], q: QuickFilter) =>
  q.conds.every((c) => facets.some((x) => sameCond(x, c)));

/** A condition on the same field that the filter's own replaces (a second range, a second Open). */
const clashes = (a: Cond, b: Cond) =>
  "f" in a &&
  "f" in b &&
  a.f === b.f &&
  (a.op === b.op || (a.op === "is" && b.op === "isnot") || (a.op === "isnot" && b.op === "is"));

/** Turn a quick filter on (replacing what it clashes with) or off. */
export function toggleQuick(facets: readonly Cond[], q: QuickFilter): Cond[] {
  if (isOn(facets, q)) return facets.filter((x) => !q.conds.some((c) => sameCond(x, c)));
  const kept = facets.filter((x) => !q.conds.some((c) => clashes(x, c)));
  return [...kept, ...q.conds];
}

/**
 * The words of a chip when it is exactly a quick filter's condition: "Open", "Mine",
 * "Date: This month". Undefined: the chip says what it is (query.ts describeCond).
 */
export function quickLabel(c: Cond, quick: readonly QuickFilter[]): string | undefined {
  for (const q of quick) {
    if (q.kind === "overdue") {
      // Overdue is a date and Open: the date part is named Overdue, the state part Open.
      const date = q.conds[0];
      if (date && sameCond(date, c)) return q.label;
      continue;
    }
    if (q.conds.length === 1 && sameCond(q.conds[0]!, c)) return q.label;
  }
  return undefined;
}

// ---- days ----------------------------------------------------------------------------------------

/** Today in `zone` as YYYY-MM-DD; the browser's day where the zone is unknown. */
export function todayIn(zone: string, now = new Date()): string {
  try {
    const p = new Intl.DateTimeFormat("en-CA", {
      timeZone: zone,
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
    }).formatToParts(now);
    const get = (k: string) => p.find((x) => x.type === k)?.value ?? "";
    const s = `${get("year")}-${get("month")}-${get("day")}`;
    if (/^\d{4}-\d{2}-\d{2}$/.test(s)) return s;
  } catch {
    /* an unknown zone */
  }
  const d = now;
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** `day` moved by `n` days. */
export function addDays(day: string, n: number): string {
  const d = new Date(`${day}T00:00:00Z`);
  d.setUTCDate(d.getUTCDate() + n);
  return d.toISOString().slice(0, 10);
}

const lastOfMonth = (day: string) => {
  const [y, m] = day.split("-").map(Number) as [number, number];
  return new Date(Date.UTC(y, m, 0)).toISOString().slice(0, 10);
};

/** The ranges every date field offers, in the order the panel lists them. */
export const RANGES = [
  { id: "today", label: msg("Today") },
  { id: "week", label: msg("This week") },
  { id: "month", label: msg("This month") },
  { id: "last30", label: msg("Last 30 days") },
  { id: "year", label: msg("This year") },
] as const;

/** The first and last day of a named range around `today`. `firstDay`: 1 Monday ... 7 Sunday. */
export function rangeDays(id: string, today: string, firstDay = 1): { min: string; max: string } {
  switch (id) {
    case "today":
      return { min: today, max: today };
    case "week": {
      const dow = new Date(`${today}T00:00:00Z`).getUTCDay(); // 0 Sunday
      const back = (dow - (firstDay % 7) + 7) % 7;
      const min = addDays(today, -back);
      return { min, max: addDays(min, 6) };
    }
    case "month":
      return { min: `${today.slice(0, 7)}-01`, max: lastOfMonth(today) };
    case "last30":
      return { min: addDays(today, -29), max: today };
    default:
      return { min: `${today.slice(0, 4)}-01-01`, max: `${today.slice(0, 4)}-12-31` };
  }
}

// ---- the generated filters -----------------------------------------------------------------------

const DUE = /^(due_on|due_date|due_at|deadline|date_deadline|expected_on|close_on|closing_on)$/;
const EXPIRY = /^(expires_on|expires_at|expiry_date|valid_until|valid_to)$/;
const PERSON =
  /^(owner|user|user_name|username|login|requested_by|created_by|salesperson|assignee|assigned_to|responsible)$/;

const isDate = (f: FieldMeta) => f.kind === "date" || f.kind === "datetime";

/**
 * The open states of the model's governed status field: those that still have a move forward (an
 * action from the state, or from any state, to a later option that is not a bad end such as
 * Cancelled). A bad state is never open. Null when the model has no such field, no action moves it,
 * or every state (or none) would be open, so the filter would mean nothing.
 */
export function openStates(model: ModelMeta): { field: FieldMeta; states: string[] } | null {
  const field = stateFieldOf(model);
  if (!field?.options?.length) return null;
  const moves = model.actions.filter((a) => a.field === field.name && a.to);
  if (moves.length === 0) return null;
  const opts = field.options;
  const bad = (s: string) => toneOf(field, s, model) === "bad";
  const open = opts.filter(
    (s) =>
      !bad(s) &&
      moves.some(
        (a) =>
          (a.from == null || a.from === s) &&
          a.to !== s &&
          opts.indexOf(a.to!) > opts.indexOf(s) &&
          !bad(a.to!),
      ),
  );
  if (open.length === 0 || open.length === opts.length) return null;
  return { field, states: open };
}

/** What the generated filters need besides the model. */
export type QuickCtx = {
  /** The signed-in user name ("Mine"); none: no Mine filter. */
  user?: string;
  /** YYYY-MM-DD in the instance's zone. */
  today: string;
  /** The first day of the week, 1 Monday ... 7 Sunday (fmt.firstDay()). */
  firstDay?: number;
};

/**
 * The date fields that get ranges, at most two, never a private class: required dates first, then
 * a due or expiry date, then the rest in declared order.
 */
export function rangeFields(fields: FieldMeta[]): FieldMeta[] {
  const dates = fields.filter((f) => isDate(f) && !PRIVATE.has(f.class));
  const due = (f: FieldMeta) => DUE.test(f.name) || EXPIRY.test(f.name);
  const ranked = [
    ...dates.filter((f) => f.required),
    ...dates.filter((f) => !f.required && due(f)),
    ...dates.filter((f) => !f.required && !due(f)),
  ];
  return ranked.slice(0, 2);
}

/** The date ranges of one field. */
export function dateFilters(f: FieldMeta, ctx: QuickCtx): QuickFilter[] {
  return RANGES.map((r) => {
    const { min, max } = rangeDays(r.id, ctx.today, ctx.firstDay);
    return {
      key: `date:${f.name}:${r.id}`,
      label: t("{field}: {range}", { field: fieldTitle(f), range: t(r.label) }),
      range: t(r.label),
      field: f.name,
      conds: [{ op: "range", f: f.name, min, max }],
      kind: "date" as const,
    };
  });
}

/**
 * The quick filters of a model, without the date ranges (see `dateFilters`): saved ones from the
 * package, then Open, Overdue, Mine and Archived, each only where the model has what it needs.
 */
export function quickFilters(model: ModelMeta, fields: FieldMeta[], ctx: QuickCtx): QuickFilter[] {
  const out: QuickFilter[] = [];
  const shown = new Map(fields.map((f) => [f.name, f]));
  (model.ui?.filter ?? []).forEach((s, i) => {
    const conds = parseWhere(s.where, model, ctx);
    if (conds?.length && conds.every((c) => !("f" in c) || shown.has(c.f)))
      out.push({
        key: `saved:${i}`,
        // The package's own translation of the label (U10), else its English label.
        label: labelFor({ labels: s.labels ?? {} }, getLang(), s.label),
        conds,
        kind: "saved",
      });
  });
  const open = openStates(model);
  const openCond: Cond | undefined =
    open && shown.has(open.field.name)
      ? { op: "is", f: open.field.name, v: open.states }
      : undefined;
  if (openCond) out.push({ key: "open", label: t("Open"), conds: [openCond], kind: "open" });
  const due =
    fields.find((f) => isDate(f) && DUE.test(f.name)) ??
    fields.find((f) => isDate(f) && EXPIRY.test(f.name));
  if (due)
    out.push({
      key: "overdue",
      label: EXPIRY.test(due.name) ? t("Expired") : t("Overdue"),
      conds: [
        { op: "range", f: due.name, max: addDays(ctx.today, -1) },
        ...(openCond ? [openCond] : []),
      ],
      kind: "overdue",
    });
  const ownName = model.access?.own ?? undefined;
  const person =
    (ownName && shown.get(ownName)?.kind === "text" ? shown.get(ownName) : undefined) ??
    fields.find((f) => f.kind === "text" && PERSON.test(f.name));
  if (person && ctx.user)
    out.push({
      key: "mine",
      label: t("Mine"),
      conds: [{ op: "is", f: person.name, v: [ctx.user] }],
      kind: "mine",
    });
  const active = shown.get("active");
  if (active?.kind === "bool")
    out.push({
      key: "archived",
      label: t("Archived"),
      conds: [{ op: "bool", f: "active", v: false }],
      kind: "archived",
    });
  return withoutCovered(out);
}

/**
 * The conditions a list of `model` starts with when it is opened without a search of its own: the
 * saved filter that `ui.default_filter` names, if the browser can read it over the fields shown;
 * `null` otherwise. The person removes it like any filter.
 */
export function defaultFilter(model: ModelMeta, fields: FieldMeta[], ctx: QuickCtx): Cond[] | null {
  const label = model.ui?.default_filter;
  if (!label) return null;
  const i = (model.ui?.filter ?? []).findIndex((s) => s.label === label);
  const q = quickFilters(model, fields, ctx).find((x) => x.key === `saved:${i}`);
  if (!q) return null;
  // A reader who sees a field masked (an agent) cannot filter on it: the server refuses that.
  const masked = (c: Cond) =>
    "f" in c && model.fields.some((f) => f.name === c.f && f.masked === true);
  return q.conds.some(masked) ? null : q.conds;
}

/**
 * Generated filters the package already saves are left out: one with the same conditions, or
 * whose name starts a saved one's ("Mine" beside a saved "Mine", "Open" beside "Open shifts").
 */
export function withoutCovered<T extends { kind?: string; label: string; conds: Cond[] }>(
  list: readonly T[],
): T[] {
  const saved = list.filter((q) => q.kind === "saved");
  const same = (a: Cond[], b: Cond[]) => JSON.stringify(a) === JSON.stringify(b);
  const word = (s: string) => s.trim().toLowerCase();
  return list.filter(
    (q) =>
      q.kind === "saved" ||
      !saved.some(
        (s) =>
          same(s.conds, q.conds) ||
          word(s.label) === word(q.label) ||
          word(s.label).startsWith(`${word(q.label)} `),
      ),
  );
}

/** The select fields offered as option filters: the status first, then up to two more. */
export function optionFields(model: ModelMeta, fields: FieldMeta[]): FieldMeta[] {
  const state = stateFieldOf(model);
  const selects = fields.filter(
    (f) => f.kind === "select" && (f.options?.length ?? 0) > 0 && !PRIVATE.has(f.class),
  );
  const first = selects.find((f) => f.name === state?.name);
  return [...(first ? [first] : []), ...selects.filter((f) => f !== first)].slice(0, 3);
}

/** The yes/no fields offered as filters: up to two, not `active` (it has Archived). */
export const boolFields = (fields: FieldMeta[]) =>
  fields
    .filter((f) => f.kind === "bool" && f.name !== "active" && !PRIVATE.has(f.class))
    .slice(0, 2);

// ---- saved filters: the part of `where` the browser reads ----------------------------------------

type Tok = { t: "id" | "str" | "num" | "op" | "punct"; v: string };

function lex(src: string): Tok[] | null {
  const out: Tok[] = [];
  let i = 0;
  while (i < src.length) {
    const c = src[i]!;
    if (/\s/.test(c)) {
      i++;
      continue;
    }
    if (c === '"' || c === "'") {
      const end = src.indexOf(c, i + 1);
      if (end < 0) return null;
      out.push({ t: "str", v: src.slice(i + 1, end) });
      i = end + 1;
      continue;
    }
    const two = src.slice(i, i + 2);
    if (["&&", "||", "==", "!=", "<=", ">="].includes(two)) {
      out.push({ t: "op", v: two });
      i += 2;
      continue;
    }
    if ("<>=!".includes(c)) {
      out.push({ t: "op", v: c === "=" ? "==" : c });
      i++;
      continue;
    }
    if ("(),[]".includes(c)) {
      out.push({ t: "punct", v: c });
      i++;
      continue;
    }
    const m = /^(\$?[A-Za-z_][\w.]*(?:[+-]\d+)?|-?\d+(?:\.\d+)?)/.exec(src.slice(i));
    if (!m) return null;
    const v = m[1]!;
    out.push({ t: /^-?\d/.test(v) ? "num" : "id", v });
    i += v.length;
  }
  return out;
}

/** A literal of `where`, resolved: `$user`, `today`, `today+7`, `$today` become values. */
function literal(tok: Tok | undefined, ctx: QuickCtx): string | number | boolean | null {
  if (!tok) return null;
  if (tok.t === "str") return tok.v;
  if (tok.t === "num") return Number(tok.v);
  if (tok.t !== "id") return null;
  if (tok.v === "true") return true;
  if (tok.v === "false") return false;
  if (tok.v === "$user") return ctx.user ?? null;
  const day = /^\$?today(?:([+-])(\d+))?$/.exec(tok.v);
  if (day) return addDays(ctx.today, (day[1] === "-" ? -1 : 1) * Number(day[2] ?? 0));
  return null;
}

/** One comparison as a condition of query.ts, or null when the browser cannot express it. */
function compare(f: FieldMeta, op: string, v: string | number | boolean): Cond | null {
  if (f.kind === "bool" && typeof v === "boolean") {
    if (op === "==") return { op: "bool", f: f.name, v };
    if (op === "!=") return { op: "bool", f: f.name, v: !v };
    return null;
  }
  if ((f.kind === "select" || f.kind === "ref" || f.kind === "text") && typeof v === "string") {
    if (op === "==") return { op: "is", f: f.name, v: [v] };
    if (op === "!=") return { op: "isnot", f: f.name, v: [v] };
    return null;
  }
  const dateLike = isDate(f) && typeof v === "string" && /^\d{4}-\d{2}-\d{2}$/.test(v);
  const intLike = f.kind === "number" && typeof v === "number" && Number.isInteger(v);
  if (!dateLike && !intLike) return null;
  const step = (x: string | number, n: number) =>
    typeof x === "number" ? String(x + n) : addDays(x, n);
  switch (op) {
    case "==":
      return { op: "range", f: f.name, min: String(v), max: String(v) };
    case "<":
      return { op: "range", f: f.name, max: step(v as string | number, -1) };
    case "<=":
      return { op: "range", f: f.name, max: String(v) };
    case ">":
      return { op: "range", f: f.name, min: step(v as string | number, 1) };
    case ">=":
      return { op: "range", f: f.name, min: String(v) };
    default:
      return null;
  }
}

/**
 * The conditions of a saved filter's `where`, or null when the browser cannot express it. It reads
 * a conjunction (`&&` or `and`) of: `field == value`, `!=`, `<`, `<=`, `>`, `>=` (dates and whole
 * numbers), `is_set(field)`, `!is_set(field)`, a bare yes/no field or its negation, and a bracketed
 * `field == a || field == b` on one field. Values are "text", numbers, true, false, `$user`,
 * `today`, `today+N`, `today-N`.
 */
export function parseWhere(where: string, model: ModelMeta, ctx: QuickCtx): Cond[] | null {
  const toks = lex(where);
  if (!toks?.length) return null;
  const field = (name: string | undefined) =>
    name ? model.fields.find((f) => f.name === name) : undefined;
  const out: Cond[] = [];
  let i = 0;
  const peek = () => toks[i];
  const isAnd = (tk: Tok | undefined) =>
    !!tk && ((tk.t === "op" && tk.v === "&&") || (tk.t === "id" && tk.v === "and"));
  /** One comparison `field op literal`, a bare or negated yes/no field, or is_set. */
  const atom = (): Cond | null => {
    let neg = false;
    if (peek()?.t === "op" && peek()!.v === "!") {
      neg = true;
      i++;
    }
    const head = toks[i++];
    if (head?.t !== "id") return null;
    if (head.v === "is_set") {
      if (toks[i++]?.v !== "(") return null;
      const f = field(toks[i++]?.v);
      if (toks[i++]?.v !== ")" || !f) return null;
      if (neg) return { op: "empty", f: f.name };
      return f.kind === "select" || f.kind === "ref" || f.kind === "text"
        ? { op: "isnot", f: f.name, v: [""] }
        : null;
    }
    const f = field(head.v);
    if (!f) return null;
    const next = peek();
    if (next?.t === "op" && ["==", "!=", "<", "<=", ">", ">="].includes(next.v)) {
      if (neg) return null;
      i++;
      const tok = toks[i++];
      // `field = null` is empty, `field != null` is set.
      if (tok?.t === "id" && tok.v === "null") {
        if (next.v === "==") return { op: "empty", f: f.name };
        if (next.v === "!=" && (f.kind === "select" || f.kind === "ref" || f.kind === "text"))
          return { op: "isnot", f: f.name, v: [""] };
        return null;
      }
      // A bare word on a select is one of its options ("state = in_review").
      if (tok?.t === "id" && f.kind === "select" && f.options?.includes(tok.v))
        return compare(f, next.v, tok.v);
      const v = literal(tok, ctx);
      return v === null ? null : compare(f, next.v, v);
    }
    return f.kind === "bool" ? { op: "bool", f: f.name, v: !neg } : null;
  };
  const clause = (): Cond | null => {
    if (peek()?.v !== "(") return atom();
    i++;
    const alts: Cond[] = [];
    for (;;) {
      const c = atom();
      if (!c) return null;
      alts.push(c);
      const tk = toks[i++];
      if (tk?.v === ")") break;
      if (!(tk?.t === "op" && tk.v === "||")) return null;
    }
    if (alts.length === 1) return alts[0]!;
    // Only alternatives of one field: one `is` with every value.
    const f = "f" in alts[0]! ? alts[0]!.f : "";
    if (!alts.every((a) => a.op === "is" && a.f === f)) return null;
    return { op: "is", f, v: alts.flatMap((a) => (a.op === "is" ? a.v : [])) };
  };
  for (;;) {
    const c = clause();
    if (!c) return null;
    out.push(c);
    if (i >= toks.length) break;
    if (!isAnd(toks[i++])) return null;
  }
  return out;
}

/** "State: Draft" for an option filter's checkbox. */
export const optionText = (f: FieldMeta, o: string) =>
  t("{field}: {values}", { field: fieldTitle(f), values: optionLabel(o, f) });
