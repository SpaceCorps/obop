/**
 * What the board, the gallery and the cards decide, as pure functions (no React, no fetching), so
 * that every rule is tested once: which grouping may be dragged, which column is folded, which
 * amount a column adds up and in which currency, what a card shows, what a quick create asks for,
 * and when a date reads as overdue. Every function reads the D95 hint first, when the server sends
 * one, and falls back to the heuristics of presentation.ts.
 */
import { fmt, getCurrency } from "@obop/ui";
import type { FieldMeta, ModelMeta, Row } from "../api.ts";
import { isEmpty, money } from "../format.ts";
import { shownNumber } from "../inputs/display.ts";
import { fieldTitle } from "../labels.ts";
import {
  cardLayout,
  displayOf,
  summable,
  titleFieldsOf,
  type CardLayout,
} from "../presentation.ts";
import { keyOf } from "../useAggregate.ts";
import { actionsOn, governedNames, initialOption, isGoverned } from "../workflow.ts";

/** Data classes a card, a sum or a quick form never shows unless a hint names the field. */
const PRIVATE = new Set(["secret", "confidential_hr", "special_category"]);

const fieldIn = (fields: readonly FieldMeta[], name: string | undefined) =>
  name ? fields.find((f) => f.name === name) : undefined;

// ---- dragging and folding ----------------------------------------------------------------------

/**
 * Whether cards may be dragged between the columns of a grouping: only when the column is a choice
 * the record makes about itself, a select (a status, a priority) or a yes/no. Never a ref: dropping
 * a stock item on another product's column would hand the stock to that product (B9). A computed or
 * read-only field is never written.
 */
export function draggableBy(f: FieldMeta | undefined): boolean {
  return !!f && (f.kind === "select" || f.kind === "bool") && !f.computed && !f.readonly;
}

/** Steps after which a record is finished: their columns fold when they hold many cards. */
const TERMINAL = new Set([
  "done",
  "won",
  "lost",
  "cancelled",
  "canceled",
  "closed",
  "resolved",
  "rejected",
  "refused",
  "declined",
  "expired",
  "revoked",
  "completed",
  "finished",
  "delivered",
  "received",
  "paid",
  "reconciled",
  "archived",
  "discarded",
  "signed",
  "no_show",
  "offboarded",
  "reversed",
  "superseded",
  "withdrawn",
  "failed",
  "retired",
]);

/**
 * Whether a column holds finished records: its value is a word for an end (done, cancelled, paid),
 * or, on a governed status, an action leads to it and none the person may run leads out of it.
 */
export function isTerminal(model: ModelMeta, f: FieldMeta, key: string): boolean {
  if (!key || f.kind !== "select") return false;
  if (TERMINAL.has(key.toLowerCase())) return true;
  if (!isGoverned(model, f) || key === initialOption(f)) return false;
  const acts = actionsOn(model, f.name);
  const reached = acts.some((a) => a.to === key);
  const leaves = acts.some((a) => a.to !== key && (a.from === key || isEmpty(a.from)));
  return reached && !leaves;
}

/** A finished column with more cards than this starts folded. */
export const FOLD_ABOVE = 10;

/** The fold choices a person made on one board: columns folded, and folded columns opened again. */
export type FoldPrefs = { fold: string[]; open: string[] };

/** The stored choices; a list (the earlier form of the preference) is the folded columns. */
export function readFold(raw: unknown): FoldPrefs {
  if (Array.isArray(raw)) return { fold: raw.filter((k) => typeof k === "string"), open: [] };
  if (raw && typeof raw === "object") {
    const o = raw as Partial<FoldPrefs>;
    const strings = (v: unknown) =>
      Array.isArray(v) ? v.filter((k): k is string => typeof k === "string") : [];
    return { fold: strings(o.fold), open: strings(o.open) };
  }
  return { fold: [], open: [] };
}

/** Whether a column is folded: the person's choice, else folded when it starts so (`auto`). */
export function isFolded(p: FoldPrefs, key: string, auto: boolean): boolean {
  if (p.fold.includes(key)) return true;
  if (p.open.includes(key)) return false;
  return auto;
}

/** The choices after the person folds or unfolds a column. */
export function toggleFold(p: FoldPrefs, key: string, auto: boolean): FoldPrefs {
  const without = (l: string[]) => l.filter((k) => k !== key);
  if (isFolded(p, key, auto))
    return { fold: without(p.fold), open: auto ? [...without(p.open), key] : without(p.open) };
  return { fold: auto ? without(p.fold) : [...without(p.fold), key], open: without(p.open) };
}

// ---- amounts -----------------------------------------------------------------------------------

const MAIN_MONEY = /^(total|amount|grand_total|total_amount)(_minor)?$/;

/**
 * The amount a column adds up and a card shows large: `ui.card_sum`; else, among the money fields
 * that are summed (summable), not private and not a person's own money (financial and personal
 * data, a salary), the one named total or amount, else the one the card layout picks, else the
 * first. A field withheld from the person (not in `fields`) is never chosen.
 */
export function mainMoneyOf(model: ModelMeta, fields: readonly FieldMeta[]): FieldMeta | undefined {
  const hint = fieldIn(fields, model.ui?.card_sum);
  if (hint && (hint.kind === "money" || hint.kind === "number" || hint.kind === "decimal"))
    return hint;
  const pool = fields.filter(
    (f) =>
      f.kind === "money" &&
      summable(f, model) &&
      !PRIVATE.has(f.class) &&
      !(f.class === "financial" && f.personal_data) &&
      !f.ui?.hidden,
  );
  return (
    pool.find((f) => MAIN_MONEY.test(f.name)) ?? fieldIn(pool, cardLayout(model).money) ?? pool[0]
  );
}

const ISO = /^[A-Z]{3}$/;

/**
 * The field a sum is also grouped by to tell its currency: `currency`, or the first step of the
 * money field's `ui.currency_from` (`journal_id` of `journal_id.currency`), with the model that
 * step points at. Null for a sum that is not money, or money always in the instance's currency.
 */
export function currencyGroupOf(
  model: ModelMeta,
  sumField: FieldMeta | undefined,
): { field: string; to?: string } | null {
  if (sumField?.kind !== "money") return null;
  if (model.fields.some((f) => f.name === "currency" && (f.kind === "text" || f.kind === "select")))
    return { field: "currency" };
  const path =
    sumField.ui?.currency_from ??
    model.fields.find((f) => f.kind === "money" && f.ui?.currency_from)?.ui?.currency_from;
  const hop = path?.split(".")[0];
  const ref = hop ? model.fields.find((f) => f.name === hop && f.kind === "ref") : undefined;
  return ref ? { field: ref.name, ...(ref.to ? { to: ref.to } : {}) } : null;
}

/**
 * Per column: its record count and its sums, by currency for money (`currencyOfGroup` reads a
 * group's key, the `currency` key by default). A group without a currency is added to the
 * instance's currency (`fallback`), so "€7,428.00 and €1,345.00" cannot happen (B7). A sum that is
 * not money (hours, days, a quantity) has one sum under the key "".
 */
export function columnTotals(
  groups: {
    key: Record<string, unknown>;
    count: number;
    measures: Record<string, number | string | null>;
  }[],
  field: FieldMeta,
  sumField: FieldMeta | undefined,
  fallback: string = getCurrency(),
  currencyOfGroup: (key: Record<string, unknown>) => unknown = (k) => k.currency,
) {
  const out = new Map<string, { count: number; sums: Map<string, number> }>();
  for (const g of groups) {
    const k = keyOf(g.key[field.name]);
    const e = out.get(k) ?? { count: 0, sums: new Map<string, number>() };
    e.count += g.count;
    if (sumField) {
      const raw = sumField.kind === "money" ? currencyOfGroup(g.key) : "";
      const c =
        sumField.kind !== "money" ? "" : typeof raw === "string" && ISO.test(raw) ? raw : fallback;
      e.sums.set(c, (e.sums.get(c) ?? 0) + (Number(g.measures[`sum:${sumField.name}`]) || 0));
    }
    out.set(k, e);
  }
  return out;
}

/**
 * The same sums from records in the browser, by each record's currency (`currencyFor`, else its
 * own `currency`, else `fallback`); a sum that is not money under the key "".
 */
export function rowSums(
  rows: readonly Row[],
  sumField: FieldMeta | undefined,
  fallback: string = getCurrency(),
  currencyFor?: (row: Row) => string | undefined,
): Map<string, number> {
  const sums = new Map<string, number>();
  if (!sumField) return sums;
  for (const r of rows) {
    const v = Number(r[sumField.name]);
    if (!Number.isFinite(v) || isEmpty(r[sumField.name])) continue;
    const own = currencyFor?.(r) ?? r.currency;
    const c =
      sumField.kind !== "money" ? "" : typeof own === "string" && ISO.test(own) ? own : fallback;
    sums.set(c, (sums.get(c) ?? 0) + v);
  }
  return sums;
}

/**
 * The sentence of a sum: money per currency ("€12,400.00 and SEK 3,000.00"), a duration as hours,
 * metres as km, else the number and the field's name ("11 working days", "40 quantity").
 */
export function sumText(sums: Map<string, number>, f: FieldMeta): string {
  if (f.kind === "money")
    return fmt.list(
      [...sums].filter(([, n]) => n !== 0).map(([c, n]) => money(n, c || undefined)),
      "conjunction",
    );
  const n = [...sums.values()].reduce((a, b) => a + b, 0);
  const shown = shownNumber(displayOf(f), n);
  if (shown) return shown;
  const digits = f.kind === "decimal" ? (f.scale ?? 2) : 0;
  return `${fmt.number(n, { maximumFractionDigits: digits })} ${fieldTitle(f).toLowerCase()}`;
}

/** Whether a column's sums are worth a line: any of them is not zero (negative ones too, B8). */
export const showsSum = (sums: Map<string, number> | undefined): boolean =>
  !!sums && [...sums.values()].some((n) => n !== 0);

// ---- cards -------------------------------------------------------------------------------------

/** What a card of the board or the gallery shows: the card layout, fitted to the board. */
export type BoardLayout = CardLayout & {
  /** A number shown large where an amount would be (`ui.card_sum` on hours, km or days). */
  figure?: string;
  /** The end of a date range whose start is `date` ("14 – 18 Dec"). */
  dateEnd?: string;
  /**
   * The title as a template, when the card names the record otherwise than titleOf: a record known
   * by a ref and a date ("Nina Petersen · 14 Dec") is titled by the ref, and the date goes to the
   * date chip, with the end of its range.
   */
  titleTemplate?: string;
};

const START_END: [RegExp, string][] = [
  [/^start(?=_|$)/, "end"],
  [/_start(?=_|$)/, "_end"],
  [/^starts(?=_|$)/, "ends"],
  [/^from(?=_|$)/, "to"],
  [/_from$/, "_to"],
  [/^begin(?=_|$)/, "end"],
  [/^check_in(?=_|$)/, "check_out"],
];

/** The field that ends a range a date field starts (start_on → end_on, date_from → date_to). */
export function rangeEndOf(
  fields: readonly FieldMeta[],
  start: FieldMeta | undefined,
): FieldMeta | undefined {
  if (!start) return undefined;
  for (const [re, to] of START_END) {
    if (!re.test(start.name)) continue;
    const end = fields.find(
      (f) => f.name === start.name.replace(re, to) && (f.kind === start.kind || f.kind === "date"),
    );
    if (end) return end;
  }
  return undefined;
}

/**
 * The card of a record on a board grouped by `group` (absent: the gallery). The column already
 * names the group's value, so the card leaves that field out; the status shows as a badge only when
 * the board is grouped by something else. Fields withheld from the person are left out. Unless the
 * package lists the card's fields (`ui.card`), the large amount is `mainMoneyOf`, and a date that
 * starts a range shows its end too.
 */
export function boardLayout(
  model: ModelMeta,
  fields: readonly FieldMeta[],
  group?: FieldMeta,
): BoardLayout {
  const base = cardLayout(model);
  const ok = (n: string | undefined) =>
    !!n && n !== group?.name && fields.some((f) => f.name === n);
  const sum = mainMoneyOf(model, fields);
  // The large figure: an amount, or a number the package sums on the card (hours, km, days).
  const figure = sum && sum.kind !== "money" ? sum.name : undefined;
  const money = model.ui?.card?.length
    ? (base.money ?? (sum?.kind === "money" ? sum.name : undefined))
    : sum?.kind === "money"
      ? sum.name
      : undefined;
  const out: BoardLayout = {
    title: base.title.filter((n) => fields.some((f) => f.name === n)),
    subtitle: base.subtitle.filter(ok),
    badges: base.badges.filter(ok),
    facts: base.facts.filter((n) => ok(n) && n !== figure && n !== money),
  };
  if (ok(base.overline)) out.overline = base.overline;
  if (ok(money)) out.money = money;
  else if (ok(figure)) out.figure = figure;
  if (ok(base.state)) out.state = base.state;
  if (ok(base.date)) {
    out.date = base.date;
    const end = rangeEndOf(fields, fieldIn(fields, base.date));
    if (end && ok(end.name)) out.dateEnd = end.name;
    // The chip is the end of a range ("Dec 18"): it shows the whole range ("Dec 14 – 18").
    const start = fields.find(
      (f) =>
        (f.kind === "date" || f.kind === "datetime") && rangeEndOf(fields, f)?.name === base.date,
    );
    if (!end && start && fields.includes(start)) {
      out.date = start.name;
      out.dateEnd = base.date;
    }
  }
  if (ok(base.avatar)) out.avatar = base.avatar;
  if (ok(base.progress)) out.progress = base.progress;
  // A record named by a ref and a date: the ref is the title, the date (and its range) the chip.
  if (!model.ui?.title && out.title.length > 1) {
    const isDate = (n: string) => {
      const k = fieldIn(fields, n)?.kind;
      return k === "date" || k === "datetime";
    };
    const day = out.title.find(isDate);
    const rest = out.title.filter((n) => !isDate(n));
    if (day && rest.length) {
      out.titleTemplate = rest.map((n) => `{${n}}`).join(" ");
      out.title = rest;
      out.date = day;
      const end = rangeEndOf(fields, fieldIn(fields, day));
      if (end && fields.includes(end)) out.dateEnd = end.name;
      else delete out.dateEnd;
    }
  }
  return out;
}

/** The first image field a gallery shows as the cover (withheld fields are not in `fields`). */
export const coverFieldOf = (fields: readonly FieldMeta[]): FieldMeta | undefined =>
  fields.find((f) => f.kind === "image" && !f.ui?.hidden);

const DUE =
  /^(due_on|due_date|due_at|deadline|expires_on|expires_at|valid_until|expected_on|close_on|end_on|ends_on)$/;

/**
 * How a date on a card reads against `today` (YYYY-MM-DD): "bad" once a due-like date has passed,
 * "warn" within `ui.warn_before_days` (default 7), else null. A finished record is never late.
 */
export function dueTone(
  f: FieldMeta | undefined,
  value: unknown,
  today: string,
  finished = false,
): "bad" | "warn" | null {
  if (!f || finished || isEmpty(value)) return null;
  const hinted = f.ui?.warn_before_days;
  if (hinted === undefined && !DUE.test(f.name)) return null;
  const day = String(value).slice(0, 10);
  if (!/^\d{4}-\d{2}-\d{2}$/.test(day)) return null;
  if (day < today) return "bad";
  const soon = new Date(`${today}T00:00:00Z`);
  soon.setUTCDate(soon.getUTCDate() + (hinted ?? 7));
  return day <= soon.toISOString().slice(0, 10) ? "warn" : null;
}

// ---- quick create ------------------------------------------------------------------------------

/** Kinds a quick form draws in a column; any other required field needs the full form. */
const INLINE = new Set([
  "text",
  "longtext",
  "ref",
  "select",
  "date",
  "datetime",
  "number",
  "money",
  "decimal",
  "bool",
]);

/** At most this many fields go into a quick form; a model that needs more opens the full form. */
export const QUICK_MAX = 4;

export type QuickPlan = {
  /** The fields the quick form asks for, the title first. */
  fields: FieldMeta[];
  /** The model needs more than a quick form can take: open the full form instead. */
  full: boolean;
};

/**
 * What the quick form of a column asks for: `ui.quick_create`, else the record's name (its required
 * text field, else a text field called name, title or subject) and every other required field that
 * no default fills (`preset`: the column's value, the search's facets, the server's defaults). The
 * server sets governed, computed, numbered and read-only fields, so they are never asked. When a
 * required field is an image or a file, or the form would need more than QUICK_MAX fields, the
 * full form opens instead.
 */
export function quickPlan(
  model: ModelMeta,
  fields: readonly FieldMeta[],
  preset: Record<string, unknown>,
): QuickPlan {
  const governed = governedNames(model);
  const settable = (f: FieldMeta) =>
    !f.computed &&
    !f.sequence &&
    !f.readonly &&
    !governed.has(f.name) &&
    !(f.kind === "select" && f.governed === true);
  const hint = model.ui?.quick_create;
  if (hint?.length) {
    const asked = hint
      .map((n) => fieldIn(fields, n))
      .filter((f): f is FieldMeta => !!f && settable(f) && INLINE.has(f.kind));
    const missing = fields.some(
      (f) =>
        f.required &&
        settable(f) &&
        isEmpty(preset[f.name]) &&
        !asked.includes(f) &&
        f.kind !== "bool",
    );
    return { fields: asked, full: missing || asked.length === 0 };
  }
  const texts = fields.filter((f) => f.kind === "text" && settable(f) && !PRIVATE.has(f.class));
  const named = new Set(titleFieldsOf(model));
  const title =
    texts.find((f) => f.required && named.has(f.name)) ??
    texts.find((f) => f.required) ??
    texts.find((f) => /^(name|title|subject)$/.test(f.name));
  const required = fields.filter(
    (f) => f !== title && f.required && settable(f) && f.kind !== "bool" && isEmpty(preset[f.name]),
  );
  const asked = [...(title ? [title] : []), ...required];
  // Nothing named and nothing required (an order is known by its number): ask whom it is for.
  if (asked.length === 0) {
    const refs = fields.filter((f) => f.kind === "ref" && settable(f) && !PRIVATE.has(f.class));
    const ref = refs.find((f) => f.required) ?? refs[0];
    if (ref && isEmpty(preset[ref.name])) asked.push(ref);
  }
  const full =
    required.some((f) => !INLINE.has(f.kind)) || asked.length > QUICK_MAX || asked.length === 0;
  return { fields: asked.filter((f) => INLINE.has(f.kind)), full };
}
