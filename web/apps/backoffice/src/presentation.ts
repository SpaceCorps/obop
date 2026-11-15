/**
 * One presentation engine (F1 of the UI plan): how a model and its records are shown, decided in
 * one place so that every view agrees. Pure functions over `/api/meta`; no React, no fetching.
 *
 * Each function reads the D95 hint first, when the server sends one, and falls back to a heuristic
 * over the metadata. D95 (PROPOSED; D93 in the plan text) puts the hints into `/api/meta`; the
 * shapes are the types of api.ts, named here so the backend can fill them without renaming:
 *
 *   module  `home` ("<model>" | "overview"), `launcher` (false: not on the launcher), and from
 *           D55 `summary`, `categories`, `maturity`
 *   model   `ui`: ModelUi {
 *             title, subtitle      a template: "{code} {name}", "{number} | Draft · {journal_id} ·
 *                                  {date}"; `|` separates alternatives, the first whose fields
 *                                  are all set wins; `{a.b}` follows the ref `a` to field `b`
 *             list, card           field names in order; card_sum: the field a column sums
 *             quick_create         the fields of the quick-create form
 *             order                "date desc, number desc"
 *             default_view         list | kanban | calendar | pivot | graph | gallery | grid |
 *                                  timeline | tree
 *             menu                 main | configuration | reporting | hidden; menu_section, menu_order
 *             calendar             { start, end?, color? }; report: { rows?, cols?, measure? }
 *             section              [{ label, fields, collapsed?, show_in? }]
 *             filter               [{ label, where }]; public_url: "/blog/{slug}"
 *           }
 *           `guard`: ModelGuard (D77: immutable_when, mutable_fields, exit_to, no_delete_when,
 *           lock_date, children_of, append_only, message)
 *           `children`: ChildMeta[] ({ model, via, shape? }); `plural`, `plural_labels`
 *   field   `ui`: FieldUi { display, aggregate (sum | avg | none), tones, warn_before_days,
 *           hidden, currency_from ("journal_id.currency") }; `default` (a literal, "today",
 *           "today+N", "$user", "$employee", "$instance.currency", "$open:<model>"); `fill_from`
 *           ("product_id.list_price_minor"); `readonly`; `ref_filter`; `gapless` (F4, sent now)
 *   action  `ui`: ActionUi { primary, danger, confirm, done, group, needs, not_by }
 *
 * Every function is safe on metadata without any hint: that is what the server sends today.
 */
import { fmt } from "@obop/ui";
import type {
  ChildMeta,
  Command,
  FieldDisplay,
  FieldMeta,
  ModelMeta,
  ModuleMeta,
  Row,
  ServerOrder,
  ValueTone,
  ViewHint,
} from "./api.ts";
import {
  formatDate,
  instantShort,
  isEmpty,
  isStateName,
  optionLabel,
  shownLabel,
} from "./format.ts";
import { fieldTitle, modelTitle } from "./labels.ts";
import type { Cond } from "./query.ts";
import { governedNames, hiddenForRecord, notForRecord } from "./workflow.ts";

// ---- shared context ----------------------------------------------------------------------------

/** All models by name, for what crosses models (children, menus, paths). */
export type MetaIndex = ReadonlyMap<string, ModelMeta>;

export function indexModels(modules: readonly ModuleMeta[]): MetaIndex {
  const m = new Map<string, ModelMeta>();
  for (const mod of modules) for (const model of mod.models) m.set(model.name, model);
  return m;
}

/** What the data layer can tell a presentation function; every key is optional. */
export type PresentCtx = {
  /** The label of a referenced record (data.tsx `refText`). */
  refText?: (model: string, id: string) => string;
  /** A cached row of another model, to follow a path (`journal_id.currency`, `parent_id`). */
  rowOf?: (model: string, id: string) => Row | undefined;
  /** The instance's currency (Settings.currency). */
  instanceCurrency?: string;
  /** The module the model belongs to, for the model's own name. */
  moduleId?: string;
  /** The metadata of another model, to follow a path of more than one step. */
  modelOf?: (model: string) => ModelMeta | undefined;
};

// ---- field classes -----------------------------------------------------------------------------

/** Data classes that a list, a card, a title or a search never shows unless a hint names them. */
const PRIVATE_CLASSES = new Set(["secret", "confidential_hr", "special_category"]);
const isPrivate = (f: FieldMeta) => PRIVATE_CLASSES.has(f.class);

/** An identifier that means nothing to a person: a uuid, a token, a client's own id. */
export function isIdLike(f: Pick<FieldMeta, "name" | "kind">): boolean {
  const n = f.name;
  if (n === "id") return true;
  if (/(^|_)(uuid|guid|token|hash|checksum|integrity|idempotency_key|external_id)$/.test(n))
    return true;
  if (/^client_\w*_id$/.test(n)) return true;
  // A text field named *_id is a code from elsewhere (tax_id is a number, client_trip_id a uuid).
  return (f.kind === "text" || f.kind === "longtext") && n.endsWith("_id");
}

/** Numbers the server keeps for itself (sequence helpers, counters of points). */
const HELPER = /^(seq|sequence|last_seq|max_seq|expected_points|point_count|row_version)$/;

const fieldOf = (model: ModelMeta, name: string) => model.fields.find((f) => f.name === name);
const isGovernedField = (model: ModelMeta, f: FieldMeta) =>
  f.kind === "select" && (f.governed === true || governedNames(model).has(f.name));

/** The status field of a model: a governed select, else a select named state, stage or status. */
export function stateFieldOf(model: ModelMeta): FieldMeta | undefined {
  return (
    model.fields.find((f) => isGovernedField(model, f) && isStateName(f.name)) ??
    model.fields.find((f) => isGovernedField(model, f)) ??
    model.fields.find((f) => f.kind === "select" && isStateName(f.name))
  );
}

/** The first required ref, else the first ref. */
function mainRef(model: ModelMeta, skip: ReadonlySet<string> = new Set()): FieldMeta | undefined {
  const refs = model.fields.filter((f) => f.kind === "ref" && !skip.has(f.name) && !isPrivate(f));
  return refs.find((f) => f.required) ?? refs[0];
}

/** The first required date (or datetime), else the first one. */
function mainDate(model: ModelMeta, skip: ReadonlySet<string> = new Set()): FieldMeta | undefined {
  const dates = model.fields.filter(
    (f) => (f.kind === "date" || f.kind === "datetime") && !skip.has(f.name) && !isPrivate(f),
  );
  return dates.find((f) => f.required) ?? dates[0];
}

const DUE = /^(due_on|due_date|deadline|expires_on|expires_at|valid_until|expected_on|close_on)$/;

// ---- values as text ----------------------------------------------------------------------------

/** "14 Nov", with the year when it is not this year. */
function shortDate(v: unknown): string {
  const s = String(v);
  if (!/^\d{4}-\d{2}-\d{2}/.test(s)) return formatDate(v);
  const sameYear = s.slice(0, 4) === String(new Date().getFullYear());
  return fmt.date(s.slice(0, 10), {
    day: "numeric",
    month: "short",
    ...(sameYear ? {} : { year: "numeric" }),
  });
}

/** Follow `a.b.c` from `row` through cached rows; undefined when a step is not known. */
function valueAt(model: ModelMeta, row: Row, path: string, ctx: PresentCtx): unknown {
  const [head, ...rest] = path.split(".");
  if (!head) return undefined;
  const v = row[head];
  if (rest.length === 0) return v;
  const f = fieldOf(model, head);
  if (!f || f.kind !== "ref" || !f.to || isEmpty(v) || !ctx.rowOf) return undefined;
  const next = ctx.rowOf(f.to, String(v));
  const target = ctx.modelOf?.(f.to);
  if (!next) return undefined;
  // Without the target's metadata only one more step is followed (a plain value).
  if (!target) return rest.length === 1 ? next[rest[0]!] : undefined;
  return valueAt(target, next, rest.join("."), ctx);
}

/** A field's value as words for a title or a second line; "" when it is empty. */
function valueText(model: ModelMeta, row: Row, name: string, ctx: PresentCtx): string {
  if (name.includes(".")) {
    const v = valueAt(model, row, name, ctx);
    return isEmpty(v) ? "" : String(v);
  }
  const f = fieldOf(model, name);
  const v = row[name];
  if (isEmpty(v)) return "";
  if (!f) return String(v);
  switch (f.kind) {
    case "ref":
      return f.to ? (ctx.refText?.(f.to, String(v)) ?? "") : "";
    case "date":
      return shortDate(v);
    case "datetime":
      return instantShort(v)?.text ?? String(v);
    case "select":
      return optionLabel(String(v), f);
    case "money":
      return fmt.money(v, currencyOf(row, model, ctx, f));
    case "bool":
      return v === true ? fieldTitle(f) : "";
    case "image":
    case "file":
      return "";
    default: {
      const s = shownLabel(f, v, row) ?? String(v);
      return f.kind === "longtext" ? firstLine(s) : s;
    }
  }
}

const firstLine = (s: string) => {
  const line = s.split("\n").find((l) => l.trim()) ?? "";
  return line.length > 80 ? `${line.slice(0, 79)}…` : line.trim();
};

/**
 * Fill a template: `{field}` and `{ref.field}` placeholders, alternatives separated by `|`. The
 * first alternative whose placeholders are all set wins; null when none is.
 */
export function fillTemplate(
  model: ModelMeta,
  row: Row,
  template: string,
  ctx: PresentCtx = {},
): string | null {
  for (const alt of template.split("|")) {
    let ok = true;
    const text = alt.replace(/\{([\w.]+)\}/g, (_, name: string) => {
      const v = valueText(model, row, name, ctx);
      if (!v) ok = false;
      return v;
    });
    if (ok && text.trim()) return text.trim();
  }
  return null;
}

/** The field names a template reads, first alternative first. */
export function templateFields(template: string): string[] {
  return [...template.matchAll(/\{([\w.]+)\}/g)].map((m) => m[1]!.split(".")[0]!);
}

// ---- titles ------------------------------------------------------------------------------------

/** The text fields a heuristic title may use. */
const titleText = (model: ModelMeta) =>
  model.fields.filter(
    (f) =>
      (f.kind === "text" || f.kind === "longtext") &&
      f.name !== "currency" &&
      f.name !== "slug" &&
      !isIdLike(f) &&
      !isPrivate(f),
  );

/** The title and the fields it was made of. */
function titleParts(model: ModelMeta, row: Row, ctx: PresentCtx): { text: string; used: string[] } {
  const hint = model.ui?.title;
  if (hint) {
    const text = fillTemplate(model, row, hint, ctx);
    if (text) return { text, used: templateFields(hint) };
  }
  const code = fieldOf(model, "code");
  const name = fieldOf(model, "name");
  if (code && name && !isEmpty(row.code) && !isEmpty(row.name))
    return { text: `${String(row.code)} ${String(row.name)}`, used: ["code", "name"] };
  const texts = titleText(model).filter((f) => f.kind === "text");
  for (const f of [...texts.filter((x) => x.required), ...texts.filter((x) => x.sequence)]) {
    const v = valueText(model, row, f.name, ctx);
    if (v) return { text: v, used: [f.name] };
  }
  // A record known by what it is about and when: "Baumann Bau GmbH · 14 Nov".
  const ref = mainRef(model);
  const date = mainDate(model);
  const parts = [ref, date].filter((f): f is FieldMeta => !!f);
  const shown = parts.map((f) => valueText(model, row, f.name, ctx)).filter(Boolean);
  const state = stateFieldOf(model);
  const numbered = model.fields.find((f) => f.sequence);
  // A draft has no number yet: "Draft · Baumann Bau GmbH · 14 Nov".
  if (
    state &&
    numbered &&
    isEmpty(row[numbered.name]) &&
    String(row[state.name] ?? "") === (state.options?.[0] ?? "") &&
    shown.length
  )
    return {
      text: [optionLabel(String(row[state.name]), state), ...shown].join(" · "),
      used: parts.map((f) => f.name),
    };
  if (shown.length) return { text: shown.join(" · "), used: parts.map((f) => f.name) };
  for (const f of titleText(model)) {
    const v = valueText(model, row, f.name, ctx);
    if (v) return { text: v, used: [f.name] };
  }
  for (const f of model.fields.filter((x) => x.kind === "ref" && !isPrivate(x))) {
    const v = valueText(model, row, f.name, ctx);
    if (v) return { text: v, used: [f.name] };
  }
  return { text: modelTitle(model, ctx.moduleId ?? ""), used: [] };
}

/**
 * The name of a record for a person: never an id, a currency code or a uuid. The `ui.title`
 * template first; then `{code} {name}`; the required text field or the number; the main ref and
 * date ("Draft · " first for a draft without its number); any other text; any ref; the model's
 * own name. A record with a `parent_id` to its own model is shown as a path ("Contracts / Rental")
 * when `ctx.rowOf` knows the parents.
 */
export function titleOf(model: ModelMeta, row: Row, ctx: PresentCtx = {}): string {
  const own = titleParts(model, row, ctx).text;
  const parent = fieldOf(model, "parent_id");
  if (!parent || parent.to !== model.name || !ctx.rowOf) return own;
  const path = [own];
  const seen = new Set([row.id]);
  let at = row;
  for (let i = 0; i < 5; i++) {
    const pid = at.parent_id;
    if (isEmpty(pid) || seen.has(String(pid))) break;
    const up = ctx.rowOf(model.name, String(pid));
    if (!up) break;
    seen.add(up.id);
    path.unshift(titleParts(model, up, ctx).text);
    at = up;
  }
  return path.join(" / ");
}

/** The fields a title is usually made of, for columns and cards (no row needed). */
export function titleFieldsOf(model: ModelMeta): string[] {
  const hint = model.ui?.title;
  if (hint) return templateFields(hint.split("|")[0] ?? "");
  if (fieldOf(model, "code") && fieldOf(model, "name")) return ["code", "name"];
  const texts = titleText(model).filter((f) => f.kind === "text");
  const t = texts.find((f) => f.required) ?? texts.find((f) => f.sequence);
  if (t) return [t.name];
  const parts = [mainRef(model), mainDate(model)].filter((f): f is FieldMeta => !!f);
  if (parts.length) return parts.map((f) => f.name);
  const any = titleText(model)[0];
  return any ? [any.name] : [];
}

/** The line under a title: `ui.subtitle`, else the main ref and the main date not in the title. */
export function subtitleOf(model: ModelMeta, row: Row, ctx: PresentCtx = {}): string {
  const hint = model.ui?.subtitle;
  if (hint) return fillTemplate(model, row, hint, ctx) ?? "";
  const used = new Set(titleParts(model, row, ctx).used);
  const parts = [mainRef(model, used), mainDate(model, used)]
    .filter((f): f is FieldMeta => !!f)
    .map((f) => valueText(model, row, f.name, ctx))
    .filter(Boolean);
  return parts.join(" · ");
}

const SECOND_LINE = ["sku", "code", "number", "reference", "vendor_reference", "email"];

/** The grey second line of a picker option: a code, else an amount, else a date, else a ref. */
export function pickerSecondLine(model: ModelMeta, row: Row, ctx: PresentCtx = {}): string {
  const used = new Set(titleParts(model, row, ctx).used);
  for (const name of SECOND_LINE) {
    const f = fieldOf(model, name);
    if (f && !used.has(name) && !isPrivate(f)) {
      const v = valueText(model, row, name, ctx);
      if (v) return v;
    }
  }
  const money = model.fields.find((f) => f.kind === "money" && !isPrivate(f) && !used.has(f.name));
  const date = mainDate(model, used);
  const ref = mainRef(model, used);
  for (const f of [money, date, ref]) {
    if (!f) continue;
    const v = valueText(model, row, f.name, ctx);
    if (v) return v;
  }
  return "";
}

/** The text fields a search for a record of this model looks in (pickers, the palette). */
export function searchFieldsOf(model: ModelMeta): string[] {
  const out: string[] = [];
  const add = (name: string) => {
    const f = fieldOf(model, name);
    if (f && f.kind === "text" && !isPrivate(f) && !isIdLike(f) && !out.includes(name))
      out.push(name);
  };
  for (const n of titleFieldsOf(model)) add(n);
  for (const f of model.fields) if (f.sequence) add(f.name);
  for (const n of ["name", "title", "subject", ...SECOND_LINE]) add(n);
  const first = titleText(model).find((f) => f.kind === "text");
  if (out.length === 0 && first) out.push(first.name);
  return out;
}

// ---- numbers -----------------------------------------------------------------------------------

/** How a field is drawn: the `ui.display` hint, else what its name says. */
export function displayOf(field: FieldMeta): FieldDisplay | undefined {
  const hint = field.ui?.display;
  if (hint) return hint;
  const n = field.name;
  if (field.kind === "number" || field.kind === "decimal") {
    if (/(^|_)(start|end|from|to)_minute$|_minute_of_day$|^minute_of_day$/.test(n))
      return "time_of_day";
    if (/(^|_)(duration|minutes)$|_minutes$/.test(n)) return "duration_minutes";
    if (n.endsWith("_bp")) return "percent_bp";
    if (/(^|_)(percent|progress|completion|share)(_pct)?$|_pct$/.test(n)) return "percent";
    if (/(distance|length)_m$/.test(n)) return "km_from_m";
    if (/(^|_)epoch(_s|_seconds)?$|_epoch$/.test(n)) return "epoch_seconds";
  }
  if (field.kind === "text") {
    if (/(^|_)email$/.test(n)) return "email";
    if (/(^|_)(phone|mobile|tel|telephone)$/.test(n)) return "tel";
    if (/(^|_)(url|website|homepage|link)$/.test(n)) return "url";
  }
  if (field.kind === "longtext") {
    if (/(^|_)address$/.test(n)) return "address";
    if (/(^|_)(body|content|markdown|body_md)$/.test(n)) return "markdown";
  }
  return undefined;
}

/** Names whose amounts are not added up: unit prices, rates, levels, counters, positions. */
const NOT_SUMMED = /(price|rate|cost|level|seq|sequence|lat|lon|odometer|_days$)/;

/**
 * Whether totals add the field up. `ui.aggregate` decides when set (`sum`; `avg` and `none` do
 * not sum). Otherwise money and decimals do, unless a price, rate, cost, level, position or
 * counter, and not on a configuration model; numbers do when named quantity, qty or days; a
 * duration does; a time of day and a percentage never do.
 */
export function summable(field: FieldMeta, model?: ModelMeta, configuration = false): boolean {
  const hint = field.ui?.aggregate;
  if (hint) return hint === "sum";
  const display = displayOf(field);
  if (display === "duration_minutes") return true;
  if (display === "time_of_day" || display === "percent" || display === "percent_bp") return false;
  if (field.kind === "money" || field.kind === "decimal") {
    if (configuration || model?.ui?.menu === "configuration") return false;
    return !NOT_SUMMED.test(field.name);
  }
  if (field.kind === "number") return /(^|_)(quantity|qty|days)$/.test(field.name);
  return false;
}

const ISO_CURRENCY = /^[A-Z]{3}$/;

/**
 * The currency of a record's money: its own `currency` field; else the `ui.currency_from` path of
 * the money field (`journal_id.currency`, followed through cached rows); else the instance's
 * currency. Undefined only when none is known; never the empty string, so an empty value is never
 * a group of its own.
 */
export function currencyOf(
  row: Row | Record<string, unknown> | undefined,
  model: ModelMeta,
  ctx: PresentCtx = {},
  field?: FieldMeta,
): string | undefined {
  const own = row?.currency;
  if (typeof own === "string" && ISO_CURRENCY.test(own)) return own;
  const from =
    field?.ui?.currency_from ??
    model.fields.find((f) => f.kind === "money" && f.ui?.currency_from)?.ui?.currency_from;
  if (from && row && "id" in row) {
    const v = valueAt(model, row as Row, from, ctx);
    if (typeof v === "string" && ISO_CURRENCY.test(v)) return v;
  }
  return ctx.instanceCurrency && ISO_CURRENCY.test(ctx.instanceCurrency)
    ? ctx.instanceCurrency
    : undefined;
}

// ---- tones -------------------------------------------------------------------------------------

const OK = new Set([
  "done",
  "won",
  "posted",
  "confirmed",
  "closed",
  "available",
  "running",
  "resolved",
  "active",
  "paid",
  "received",
  "delivered",
  "shipped",
  "finished",
  "completed",
  "published",
  "sent",
  "signed",
  "connected",
  "approved",
  "reached",
  "handled",
  "opted_in",
  "succeeded",
  "reconciled",
]);
const BAD = new Set([
  "lost",
  "cancelled",
  "canceled",
  "failed",
  "rejected",
  "missed",
  "no_show",
  "refused",
  "declined",
  "expired",
  "revoked",
  "withdrawn",
  "error",
  "bounced",
  "opted_out",
  "suspended",
  "offboarded",
  "urgent",
  "critical",
]);
const WARN = new Set([
  "doing",
  "in_service",
  "maintenance",
  "in_progress",
  "qualified",
  "pending",
  "submitted",
  "requested",
  "in_review",
  "en_route",
  "scheduled",
  "paused",
  "high",
  "major",
  "past_due",
  "overdue",
]);
const NEUTRAL = new Set([
  "draft",
  "new",
  "open",
  "todo",
  "planned",
  "none",
  "",
  "archived",
  "superseded",
  "low",
  "minor",
  "normal",
  "discarded",
  "reversed",
]);
/** Words that carry a meaning in any select (priority and severity), not only in a status. */
const LEVELS = new Set(["urgent", "critical", "high", "major", "low", "minor", "normal"]);

function wordTone(v: string): ValueTone | null {
  if (OK.has(v)) return "ok";
  if (BAD.has(v)) return "bad";
  if (WARN.has(v)) return "warn";
  if (NEUTRAL.has(v)) return "neutral";
  return null;
}

/**
 * The meaning of a select value's colour, or null for "no meaning" (a neutral tint from the
 * palette). `ui.tones` decides when it names the value. The word lists apply to the status of a
 * model (governed, or named state, stage or status) and to priority and severity words; any other
 * select gets a tint, so a category never looks like an alarm.
 */
export function toneOf(
  field: FieldMeta | undefined,
  value: string,
  model?: ModelMeta,
): ValueTone | null {
  const v = value.toLowerCase();
  const hinted = field?.ui?.tones?.[value];
  if (hinted) return hinted;
  const status =
    !field ||
    isStateName(field.name) ||
    field.governed === true ||
    (model ? governedNames(model).has(field.name) : false);
  if (status) return wordTone(v);
  return LEVELS.has(v) ? wordTone(v) : null;
}

/** The badge class of a value: `t-ok`, `t-bad`, `t-warn`, `t-info`, `t-neutral`, or a tint `t-1`..`t-6`. */
export function toneClassOf(
  field: FieldMeta | undefined,
  value: string,
  model?: ModelMeta,
): string {
  const tone = toneOf(field, value, model);
  if (tone) return `t-${tone}`;
  const i = field?.options?.indexOf(value) ?? 0;
  return `t-${(Math.max(i, 0) % 6) + 1}`;
}

// ---- lists and cards ---------------------------------------------------------------------------

/** Whether a field may be a list column or a card line at all (before any hint). */
function showable(model: ModelMeta, f: FieldMeta): boolean {
  if (f.ui?.hidden) return false;
  if (f.kind === "longtext" || f.kind === "image" || f.kind === "file") return false;
  if (f.name === "currency" || f.name === "slug" || HELPER.test(f.name)) return false;
  if (isPrivate(f) || isIdLike(f)) return false;
  if (f.kind === "bool" && f.computed) return false;
  // Coordinates and checksums the server keeps (a trip's start_lat) are for maps, not columns.
  if (/(^|_)(lat|lon|lng|latitude|longitude)$/.test(f.name)) return false;
  return !!fieldOf(model, f.name);
}

/** Rough widths in pixels, for deciding how many columns fit. */
function widthOf(f: FieldMeta, title: boolean): number {
  if (title) return 220;
  switch (f.kind) {
    case "ref":
      return 160;
    case "date":
      return 110;
    case "datetime":
      return 150;
    case "money":
    case "decimal":
      return 120;
    case "bool":
      return 80;
    case "select":
      return 120;
    case "number":
      return 90;
    default:
      return 150;
  }
}

/**
 * The columns of a list at `widthPx`, in order: the title, the main ref, the main date, money,
 * quantity, then the rest in declared order, with the status pinned last. `ui.list` replaces the
 * ranking. Long text, currency codes, images, slugs, sequence helpers, private classes, computed
 * booleans, identifiers and (when `rows` are given) columns empty on every row are left out. As
 * the width shrinks, columns go from the lowest rank; the title, the first money column and the
 * status always stay.
 */
export function listColumns(
  model: ModelMeta,
  widthPx: number,
  opts: { fields?: FieldMeta[]; rows?: readonly Row[] } = {},
): FieldMeta[] {
  const fields = opts.fields ?? model.fields;
  const byName = (n: string) => fields.find((f) => f.name === n);
  const state = stateFieldOf(model);
  const stateField = state && byName(state.name);
  const titleNames = titleFieldsOf(model).filter((n) => byName(n));
  let ranked: FieldMeta[];
  if (model.ui?.list?.length) {
    ranked = model.ui.list.map(byName).filter((f): f is FieldMeta => !!f && f !== stateField);
  } else {
    const pool = fields.filter((f) => showable(model, f) && f !== stateField);
    const taken = new Set<string>();
    const take = (f: FieldMeta | undefined) => {
      if (f && !taken.has(f.name) && pool.includes(f)) {
        taken.add(f.name);
        return [f];
      }
      return [];
    };
    const title = titleNames.flatMap((n) => take(byName(n)));
    // The title is at least one column, even when its field is not showable (a long text).
    if (title.length === 0) {
      const first = titleNames.map(byName).find((f) => f && !isPrivate(f));
      if (first) {
        title.push(first);
        taken.add(first.name);
      }
    }
    const ref = take(
      pool.find((f) => f.kind === "ref" && f.required && !taken.has(f.name)) ??
        pool.find((f) => f.kind === "ref" && !taken.has(f.name)),
    );
    const date = take(
      pool.find(
        (f) => (f.kind === "date" || f.kind === "datetime") && f.required && !taken.has(f.name),
      ) ?? pool.find((f) => (f.kind === "date" || f.kind === "datetime") && !taken.has(f.name)),
    );
    const money = pool.filter((f) => f.kind === "money" && !taken.has(f.name)).flatMap(take);
    const qty = pool
      .filter((f) => f.kind === "number" && /(^|_)(quantity|qty)$/.test(f.name))
      .flatMap(take);
    const rest = pool.filter((f) => !taken.has(f.name));
    ranked = [...title, ...ref, ...date, ...money, ...qty, ...rest];
  }
  const rows = opts.rows;
  const keepAlways = new Set<FieldMeta>();
  if (ranked[0]) keepAlways.add(ranked[0]);
  const firstMoney = ranked.find((f) => f.kind === "money");
  if (firstMoney) keepAlways.add(firstMoney);
  if (rows && rows.length)
    ranked = ranked.filter((f) => keepAlways.has(f) || rows.some((r) => !isEmpty(r[f.name])));
  // Fit the width: drop from the lowest rank, keeping the title, the first money and the status.
  const budget = Math.max(0, widthPx - 48 - (stateField ? widthOf(stateField, false) : 0));
  const out = [...ranked];
  const total = () => out.reduce((s, f, i) => s + widthOf(f, i === 0), 0);
  for (let i = out.length - 1; i >= 0 && total() > budget; i--)
    if (!keepAlways.has(out[i]!)) out.splice(i, 1);
  return stateField ? [...out, stateField] : out;
}

/** What a card shows, as field names; every key but `title` may be missing. */
export type CardLayout = {
  /** The fields of the title (titleOf draws it). */
  title: string[];
  /** A small number above the title (the record's number). */
  overline?: string;
  /** Up to two refs under the title. */
  subtitle: string[];
  /** The main amount, right-aligned. */
  money?: string;
  /** Select values as badges (priority, category), at most two; never the status. */
  badges: string[];
  /** The status, when the model has one. */
  state?: string;
  /** A due date or the main date, as a chip. */
  date?: string;
  /** An image field shown as the avatar; absent: initials of the title. */
  avatar?: string;
  /** A percentage drawn as a progress bar. */
  progress?: string;
};

/**
 * The card of a record on a board, a gallery or a phone list. `ui.card` lists the fields (any
 * class); otherwise it is chosen by role and never uses financial, confidential_hr or
 * special_category fields.
 */
export function cardLayout(model: ModelMeta): CardLayout {
  const state = stateFieldOf(model);
  const hint = model.ui?.card;
  const title = titleFieldsOf(model);
  const pool = hint?.length
    ? hint.map((n) => fieldOf(model, n)).filter((f): f is FieldMeta => !!f)
    : model.fields.filter(
        (f) => !isPrivate(f) && f.class !== "financial" && !isIdLike(f) && !f.ui?.hidden,
      );
  const used = new Set(title);
  const pick = (ok: (f: FieldMeta) => boolean) => {
    const f = pool.find((x) => !used.has(x.name) && ok(x));
    if (f) used.add(f.name);
    return f?.name;
  };
  const overline = pick((f) => !!f.sequence);
  const subtitle = [pick((f) => f.kind === "ref" && f.required), pick((f) => f.kind === "ref")]
    .filter((n): n is string => !!n)
    .slice(0, 2);
  const money = pick((f) => f.kind === "money" && summable(f, model));
  const date =
    pick((f) => (f.kind === "date" || f.kind === "datetime") && DUE.test(f.name)) ??
    pick((f) => f.kind === "date" || f.kind === "datetime");
  const avatar = pick((f) => f.kind === "image");
  const progress = pick((f) => {
    const d = displayOf(f);
    return d === "percent" || d === "percent_bp";
  });
  const badges = [
    pick((f) => f.kind === "select" && f !== state),
    pick((f) => f.kind === "select" && f !== state),
  ].filter((n): n is string => !!n);
  return {
    title,
    ...(overline ? { overline } : {}),
    subtitle,
    ...(money ? { money } : {}),
    badges,
    ...(state ? { state: state.name } : {}),
    ...(date ? { date } : {}),
    ...(avatar ? { avatar } : {}),
    ...(progress ? { progress } : {}),
  };
}

// ---- views and order ---------------------------------------------------------------------------

const START = /^(start|starts|start_on|start_at|starts_at|start_date|begins_at|date)$|^start_/;

/**
 * The view a model opens on: `ui.default_view`; else a board for a status of at most seven steps
 * on a model whose numbers are not gapless (CRM, Helpdesk, Projects; not journal entries or
 * receipts); else a calendar for a model planned by date and time of day (appointments, shifts);
 * else the list. A view not in `available` (when given) falls back to the list.
 */
export function defaultView(model: ModelMeta, available?: readonly string[]): ViewHint {
  const ok = (v: ViewHint) => !available || available.includes(v);
  const hint = model.ui?.default_view;
  if (hint && ok(hint)) return hint;
  const state = stateFieldOf(model);
  const gapless = model.fields.some((f) => f.gapless);
  if (
    state &&
    isGovernedField(model, state) &&
    (state.options?.length ?? 99) <= 7 &&
    !gapless &&
    ok("kanban")
  )
    return "kanban";
  const startDate = model.fields.find(
    (f) => (f.kind === "date" || f.kind === "datetime") && f.required && START.test(f.name),
  );
  const timeOfDay = model.fields.some((f) => displayOf(f) === "time_of_day");
  if (startDate && (timeOfDay || startDate.kind === "datetime") && ok("calendar"))
    return "calendar";
  return "list";
}

/**
 * The order a list starts in: `ui.order` ("date desc, number desc"); else the first required date,
 * newest first; else the number, highest first; else the id, newest first (ids are UUIDv7).
 */
export function defaultOrder(model: ModelMeta): ServerOrder[] {
  const hint = model.ui?.order;
  if (hint) {
    const out: ServerOrder[] = [];
    for (const part of hint.split(",")) {
      const [field, dir] = part.trim().split(/\s+/);
      if (field && (field === "id" || fieldOf(model, field)))
        out.push({ field, dir: dir?.toLowerCase() === "desc" ? "desc" : "asc" });
    }
    if (out.length) return out;
  }
  const date = model.fields.find(
    (f) => (f.kind === "date" || f.kind === "datetime") && f.required && !isPrivate(f),
  );
  if (date) return [{ field: date.name, dir: "desc" }];
  const seq = model.fields.find((f) => f.sequence);
  if (seq) return [{ field: seq.name, dir: "desc" }];
  return [{ field: "id", dir: "desc" }];
}

// ---- apps and menus ----------------------------------------------------------------------------

/**
 * The models drawn inside a parent's form: `children` from the server; else the rule of
 * `resolve_child` (obop-registry expr.rs) mirrored: a model named `<parent>_<x>` that has exactly
 * one ref to the parent, which is its `via`. Shapes: `images` and `translations` by the name,
 * `lines` otherwise.
 */
export function childrenOf(model: ModelMeta, index: MetaIndex): ChildMeta[] {
  if (model.children) return model.children;
  const out: ChildMeta[] = [];
  const prefix = `${model.name}_`;
  for (const [name, child] of index) {
    if (!name.startsWith(prefix) || name === model.name) continue;
    const refs = child.fields.filter((f) => f.kind === "ref" && f.to === model.name);
    if (refs.length !== 1) continue;
    const rest = name.slice(prefix.length);
    const shape = /^images?$/.test(rest)
      ? "images"
      : /^translations?$/.test(rest)
        ? "translations"
        : "lines";
    out.push({ model: name, via: refs[0]!.name, shape });
  }
  return out;
}

/** The models of a module grouped for the sidebar, each group in declaration order. */
export type ModuleMenu = {
  main: ModelMeta[];
  configuration: ModelMeta[];
  reporting: ModelMeta[];
  /** Child models (reached through their parent) and models a hint hides. */
  hidden: ModelMeta[];
};

/**
 * The sidebar of an app. `ui.menu` places a model when set. Otherwise a child model (childrenOf
 * any model) is hidden, and a model is configuration when it has no status and a model of the same
 * module that has one points at it (SLA, plan, role, till, account, journal, tax). The rest is
 * main. Each group keeps the declaration order, then `ui.menu_order`.
 */
export function menuOf(module: ModuleMeta, index: MetaIndex): ModuleMenu {
  const out: ModuleMenu = { main: [], configuration: [], reporting: [], hidden: [] };
  const children = new Set<string>();
  for (const m of index.values()) for (const c of childrenOf(m, index)) children.add(c.model);
  const stateful = module.models.filter((m) => {
    const s = stateFieldOf(m);
    return !!s && isGovernedField(m, s);
  });
  for (const m of module.models) {
    const place = m.ui?.menu;
    if (place) {
      out[place].push(m);
      continue;
    }
    if (children.has(m.name)) {
      out.hidden.push(m);
      continue;
    }
    const s = stateFieldOf(m);
    const hasState = !!s && isGovernedField(m, s);
    const target = stateful.some(
      (o) => o !== m && o.fields.some((f) => f.kind === "ref" && f.to === m.name),
    );
    (!hasState && target ? out.configuration : out.main).push(m);
  }
  const order = (a: ModelMeta, b: ModelMeta) => (a.ui?.menu_order ?? 0) - (b.ui?.menu_order ?? 0);
  for (const k of Object.keys(out) as (keyof ModuleMenu)[]) out[k].sort(order);
  return out;
}

/**
 * Where an app opens: the module's `home` (a model or "overview"); else "overview" when the app
 * has two or more main models; else its main model (its first model when it has none).
 */
export function homeOf(module: ModuleMeta, index: MetaIndex): string {
  if (module.home) return module.home;
  const menu = menuOf(module, index);
  if (menu.main.length >= 2) return "overview";
  return menu.main[0]?.name ?? module.models[0]?.name ?? "";
}

// ---- actions -----------------------------------------------------------------------------------

const DANGER = /^(cancel|refuse|reject|mark_lost|discard|erase|revoke|retire|delete|void)(_|$)/;

/** The buttons of a record, as the record panel and the kanban card show them. */
export type NextActions = {
  /** The one filled button: the next step. */
  primary?: Command;
  /** The other actions valid now. */
  secondary: Command[];
  /** Cancel, refuse, discard and the like: in the overflow menu, behind a confirm. */
  danger: Command[];
};

const shortName = (a: Command) => a.id.slice(a.id.indexOf(".") + 1);

/**
 * The actions valid for `row` now: those without a `from`, or whose `from` is the record's state,
 * and that do not lead where the record already is. One entry per `ui.group` (the first valid
 * variant). Danger actions (cancel, refuse, reject, mark_lost, discard, erase, revoke, retire, or
 * `ui.danger`, or a move to a bad state) go apart. The primary is `ui.primary`, else the first
 * move forward: to a later option that is not a bad state.
 */
export function nextActions(model: ModelMeta, row: Row): NextActions {
  const valid: Command[] = [];
  const groups = new Set<string>();
  const fits = (a: Command) => {
    if (hiddenForRecord(model, a, row) || notForRecord(model, a, row)) return false;
    const f = a.field ? fieldOf(model, a.field) : undefined;
    const at = f ? String(row[f.name] ?? "") : "";
    return !(f && a.from && a.from !== at) && !(f && a.to && a.to === at);
  };
  for (const a of model.actions) {
    if (hiddenForRecord(model, a, row)) continue;
    const f = a.field ? fieldOf(model, a.field) : undefined;
    const at = f ? String(row[f.name] ?? "") : "";
    if (f && a.from && a.from !== at) continue;
    if (f && a.to && a.to === at) continue;
    const g = a.ui?.group;
    if (g) {
      if (groups.has(g)) continue;
      // A variant the server refuses for this record (`notForRecord`) gives way to a later one.
      if (
        notForRecord(model, a, row) &&
        model.actions.some((b) => b !== a && b.ui?.group === g && fits(b))
      )
        continue;
      groups.add(g);
    }
    valid.push(a);
  }
  const toTone = (a: Command) => {
    const f = a.field ? fieldOf(model, a.field) : undefined;
    return a.to ? toneOf(f, a.to, model) : null;
  };
  const isDanger = (a: Command) =>
    a.ui?.danger ?? (DANGER.test(shortName(a)) || toTone(a) === "bad");
  const danger = valid.filter(isDanger);
  const rest = valid.filter((a) => !isDanger(a));
  const forward = (a: Command) => {
    const f = a.field ? fieldOf(model, a.field) : undefined;
    if (!f || !a.to) return false;
    const opts = f.options ?? [];
    const from = opts.indexOf(String(row[f.name] ?? ""));
    const tone = toTone(a);
    return opts.indexOf(a.to) > from && tone !== "bad" && tone !== "neutral";
  };
  // Never a step the server refuses for this record (it stays, greyed out with why).
  const runnable = rest.filter((a) => !notForRecord(model, a, row));
  const primary = runnable.find((a) => a.ui?.primary) ?? runnable.find(forward);
  return { ...(primary ? { primary } : {}), secondary: rest.filter((a) => a !== primary), danger };
}

// ---- new records -------------------------------------------------------------------------------

/** What a new record's defaults may use. */
export type NewCtx = {
  /** The search of the list the form opens from: single-value facets become values. */
  facets?: readonly Cond[];
  /** Values the caller sets itself (a kanban column, a quick create); they win. */
  route?: Record<string, unknown>;
  /** The signed-in user name, for `$user`. */
  user?: string;
  instanceCurrency?: string;
  /** Today as `YYYY-MM-DD` (tests set it); the browser's local date when absent. */
  today?: string;
};

const localToday = () => {
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
};

function addDays(day: string, n: number): string {
  const d = new Date(`${day}T00:00:00Z`);
  d.setUTCDate(d.getUTCDate() + n);
  return d.toISOString().slice(0, 10);
}

/**
 * The values a new record starts with, weakest first: the heuristics (a required date is today,
 * `active` is true, `currency` is the instance's), the server's `default` hints (the ones the
 * browser can know: literals, `today`, `today+N`, `$user`, `$instance.currency`; the server fills
 * the rest), the single-value facets of the list on a ref, select or bool, then `ctx.route`.
 * Governed, computed, numbered and read-only fields are never set.
 */
export function newDefaults(model: ModelMeta, ctx: NewCtx = {}): Record<string, unknown> {
  const today = ctx.today ?? localToday();
  const out: Record<string, unknown> = {};
  const settable = (f: FieldMeta) =>
    !f.computed && !f.sequence && !f.readonly && !isGovernedField(model, f);
  for (const f of model.fields) {
    if (!settable(f)) continue;
    if (f.kind === "date" && f.required) out[f.name] = today;
    if (f.kind === "bool" && f.name === "active") out[f.name] = true;
    if (f.name === "currency" && f.kind === "text" && ctx.instanceCurrency)
      out[f.name] = ctx.instanceCurrency;
  }
  for (const f of model.fields) {
    if (!settable(f) || f.default === undefined) continue;
    const d = f.default;
    if (typeof d !== "string") out[f.name] = d;
    else if (d === "today") out[f.name] = today;
    else if (/^today\+\d+$/.test(d)) out[f.name] = addDays(today, Number(d.slice(6)));
    else if (d === "$user") {
      if (ctx.user) out[f.name] = ctx.user;
    } else if (d === "$instance.currency") {
      if (ctx.instanceCurrency) out[f.name] = ctx.instanceCurrency;
    } else if (!d.startsWith("$")) out[f.name] = d;
  }
  for (const c of ctx.facets ?? []) {
    const f = "f" in c ? fieldOf(model, c.f) : undefined;
    if (!f || !settable(f)) continue;
    if (c.op === "is" && c.v.length === 1 && (f.kind === "ref" || f.kind === "select"))
      out[f.name] = c.v[0];
    else if (c.op === "bool" && f.kind === "bool") out[f.name] = c.v;
  }
  return { ...out, ...ctx.route };
}
