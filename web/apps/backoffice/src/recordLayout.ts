// How the record form is laid out (lane L5 of the UI plan, 2.6): its sections, the key figures of
// the header, the lock of a frozen record and whether the record opens as a full page. Pure
// functions over `/api/meta` and a row, so they are tested without a browser. Each reads the D95
// hint first (`ui.section`, `guard`, `children`, `readonly`, action `ui`) and falls back to a
// heuristic, because no server sends the hints yet.
import { t } from "@obop/ui";
import type { Command, FieldMeta, ModelMeta, Row } from "./api.ts";
import { isEmpty, optionLabel } from "./format.ts";
import { actionTitle } from "./labels.ts";
import { childrenOf, titleFieldsOf, toneOf, type MetaIndex } from "./presentation.ts";

/** A group of fields on the form. */
export type FormSection = {
  /** Stable for React and for the person's open or closed choice. */
  key: string;
  /** The heading; empty for the one section of a short form. */
  label: string;
  fields: FieldMeta[];
  /** Drawn closed until opened (an empty optional group, system values, sensitive data). */
  collapsed: boolean;
  /** Some field of the section holds personal data: one note for the section. */
  personal: boolean;
};

/** A field the person never writes: the server computes, numbers or sets it. */
export const serverSet = (f: FieldMeta): boolean => !!(f.computed || f.sequence || f.readonly);

/** The name prefixes that make a group of their own ("site_street", "site_city"). */
const PREFIXES: [RegExp, () => string][] = [
  [/^site_/, () => t("Site")],
  [/^requester_/, () => t("Requester")],
  [/^attendee_/, () => t("Attendee")],
  [/^signed_|^signer_/, () => t("Signature")],
  [/^seo_/, () => t("Search engines")],
];

/** A form this short is drawn as one group without headings. */
const SHORT_FORM = 8;

type Bucket = "main" | "amounts" | "details" | "notes" | "private" | "sensitive" | "system";

function bucketOf(f: FieldMeta, title: ReadonlySet<string>): Bucket {
  if (f.class === "special_category") return "sensitive";
  if (f.class === "confidential_hr" || (f.class === "financial" && f.kind !== "money"))
    return "private";
  if (f.sequence || f.readonly) return "system";
  if (f.kind === "money" || (f.kind === "decimal" && f.computed)) return "amounts";
  if (f.computed) return "system";
  if (f.kind === "longtext" && !title.has(f.name)) return "notes";
  if (
    title.has(f.name) ||
    f.kind === "ref" ||
    f.kind === "date" ||
    f.kind === "datetime" ||
    f.kind === "select" ||
    f.required
  )
    return "main";
  return "details";
}

const BUCKET_LABEL: Record<Bucket, () => string> = {
  main: () => t("Main"),
  amounts: () => t("Amounts"),
  details: () => t("Details"),
  notes: () => t("Notes"),
  private: () => t("Private"),
  sensitive: () => t("Sensitive"),
  system: () => t("Set by the system"),
};
const BUCKET_ORDER: Bucket[] = [
  "main",
  "amounts",
  "details",
  "notes",
  "private",
  "sensitive",
  "system",
];

/** The fields an action asks for when it runs (`ui.needs`): they wait under More on a new record. */
export function laterFields(model: ModelMeta): Set<string> {
  const out = new Set<string>();
  for (const a of model.actions) for (const n of a.ui?.needs ?? []) out.add(n);
  return out;
}

export type SectionCtx = {
  creating: boolean;
  row: Row | null;
  /** The state the record is in (the initial one for a new record), for `show_in`. */
  state?: string;
  /** The value the form holds now, to tell an empty group from a filled one. */
  filled: (f: FieldMeta) => boolean;
};

/**
 * The sections of the form, in order. `ui.section` hints come first (a section with `show_in`
 * only in those states); every field they do not name goes to the fallback groups: Main (title,
 * refs, selects, dates, required), Amounts (money), groups by name prefix (site_, requester_,
 * attendee_, signed_, seo_), Details, Notes (long text), Private (HR and financial details),
 * Sensitive (special category, closed) and Set by the system (numbers, read-only and computed
 * values, closed). A form of fewer than eight fields is one group without a heading. An optional
 * group whose fields are all empty is closed; on a new record every group without a required
 * field is, and the fields an action asks for later wait in a closed More group.
 */
export function sectionsOf(model: ModelMeta, fields: FieldMeta[], c: SectionCtx): FormSection[] {
  const byName = new Map(fields.map((f) => [f.name, f]));
  const used = new Set<string>();
  const out: FormSection[] = [];
  const personal = (fs: FieldMeta[]) => fs.some((f) => f.personal_data);
  const closedWhenEmpty = (fs: FieldMeta[]) =>
    c.creating ? !fs.some((f) => f.required || c.filled(f)) : !fs.some(c.filled);

  for (const [i, s] of (model.ui?.section ?? []).entries()) {
    const fs = s.fields.map((n) => byName.get(n)).filter((f): f is FieldMeta => !!f);
    fs.forEach((f) => used.add(f.name));
    if (s.show_in && c.state !== undefined && !s.show_in.includes(c.state)) continue;
    if (fs.length === 0) continue;
    out.push({
      key: `hint-${i}`,
      label: s.label,
      fields: fs,
      collapsed: !!s.collapsed || (i > 0 && closedWhenEmpty(fs)),
      personal: personal(fs),
    });
  }

  const rest = fields.filter((f) => !used.has(f.name));
  const later = c.creating ? laterFields(model) : new Set<string>();
  const now = rest.filter((f) => !later.has(f.name) || f.required);
  const waiting = rest.filter((f) => later.has(f.name) && !f.required);

  if (out.length === 0 && now.length < SHORT_FORM) {
    if (now.length)
      out.push({
        key: "all",
        label: "",
        fields: now,
        collapsed: false,
        personal: personal(now),
      });
  } else {
    const title = new Set(titleFieldsOf(model));
    const groups = new Map<string, { label: string; fields: FieldMeta[]; bucket?: Bucket }>();
    for (const f of now) {
      const pre = PREFIXES.find(([re]) => re.test(f.name));
      const b = bucketOf(f, title);
      const key = pre && b !== "sensitive" && b !== "private" ? `pre-${pre[0].source}` : b;
      const g = groups.get(key) ?? {
        label: pre && key.startsWith("pre-") ? pre[1]() : BUCKET_LABEL[b](),
        fields: [],
        ...(key.startsWith("pre-") ? {} : { bucket: b }),
      };
      g.fields.push(f);
      groups.set(key, g);
    }
    const order = [
      ...BUCKET_ORDER.slice(0, 2),
      ...[...groups.keys()].filter((k) => k.startsWith("pre-")),
      ...BUCKET_ORDER.slice(2),
    ];
    for (const key of order) {
      const g = groups.get(key);
      if (!g) continue;
      const first = out.length === 0;
      out.push({
        key,
        label: g.label,
        fields: g.fields,
        collapsed:
          g.bucket === "system" ||
          g.bucket === "sensitive" ||
          (!first && g.bucket !== "main" && closedWhenEmpty(g.fields)),
        personal: personal(g.fields),
      });
    }
  }
  if (waiting.length)
    out.push({
      key: "later",
      label: t("More"),
      fields: waiting,
      collapsed: true,
      personal: personal(waiting),
    });
  return out;
}

// ---- the header ---------------------------------------------------------------------------------

const TOTAL =
  /^(amount_)?total(_minor)?$|^(grand_total|amount|balance|amount_due|residual)(_minor)?$/;

/**
 * The money a header shows on the right: computed amounts and fields named like a total, at most
 * three, in declaration order (Untaxed, Tax, Total). Only fields with a value.
 */
export function keyFigures(model: ModelMeta, fields: FieldMeta[], row: Row | null): FieldMeta[] {
  if (!row) return [];
  return fields
    .filter((f) => f.kind === "money" && (f.computed || TOTAL.test(f.name)))
    .filter((f) => !isEmpty(row[f.name]))
    .slice(-3);
}

/** The image a header shows as the record's picture, when the model has one. */
export function avatarField(fields: FieldMeta[]): FieldMeta | undefined {
  return fields.find((f) => f.kind === "image" && f.class !== "special_category");
}

/**
 * The steps of a status bar: every option, except a bad end (Cancelled, Lost, Refused) that the
 * record is not in, which would read as a step to reach.
 */
export function stepsOf(model: ModelMeta, f: FieldMeta, current: string): string[] {
  return (f.options ?? []).filter((o) => o === current || toneOf(f, o, model) !== "bad");
}

// ---- locks --------------------------------------------------------------------------------------

export type Lock = {
  /** Why it is frozen, for the banner. */
  message: string;
  /** The fields that may still change (`guard.mutable_fields`). */
  mutable: ReadonlySet<string>;
};

/**
 * Whether a saved record is frozen (D77 `guard` as D95 sends it): `immutable_when` matches the
 * record, or the model is append only. The server enforces it; this only draws it.
 */
export function lockOf(model: ModelMeta, row: Row | null): Lock | null {
  const g = model.guard;
  if (!g || !row) return null;
  const w = g.immutable_when;
  const field = w ? model.fields.find((f) => f.name === w.field) : undefined;
  const hit = w && w.values.includes(String(row[w.field] ?? ""));
  if (!hit && !g.append_only) return null;
  const message =
    g.message ??
    (hit && w
      ? t("This record is {state} and can no longer be changed.", {
          state: optionLabel(String(row[w.field] ?? ""), field),
        })
      : t("Records of this kind are kept as written and can no longer be changed."));
  return { message, mutable: new Set(g.mutable_fields ?? []) };
}

/** Whether `guard.no_delete_when` (or the lock) keeps the record from being deleted. */
export function deleteBlocked(model: ModelMeta, row: Row | null): boolean {
  const g = model.guard;
  if (!g || !row) return false;
  const w = g.no_delete_when ?? g.immutable_when;
  if (w && w.values.includes(String(row[w.field] ?? ""))) return true;
  return !!g.append_only;
}

// ---- the page -----------------------------------------------------------------------------------

/** The size from which a record opens as a page instead of the side panel. */
const MANY_FIELDS = 14;

/** Whether a record of `model` opens as a full page: it has lines, or more than 14 fields. */
export function prefersFullPage(model: ModelMeta, index: MetaIndex): boolean {
  return (
    childrenOf(model, index).some((c) => c.shape !== "translations") ||
    model.fields.length > MANY_FIELDS
  );
}

// ---- actions ------------------------------------------------------------------------------------

/** Action labels that read like a button of the panel itself. */
const CLASHES = new Set(["close", "cancel", "delete", "save", "discard", "open", "back", "done"]);

/**
 * The label of an action button. One that reads like a button of the panel ("Close", "Cancel")
 * gets the model's name: "Close ticket", "Cancel order".
 */
export function actionLabelFor(a: Command, noun: string): string {
  const label = actionTitle(a);
  const short = a.id.slice(a.id.indexOf(".") + 1);
  return CLASHES.has(short) ? t("{action} {noun}", { action: label, noun }) : label;
}

/**
 * The values a copy of a record starts with: everything the person may write, without what the
 * server sets, governed states, unique values and files.
 */
export function copyValues(
  model: ModelMeta,
  fields: FieldMeta[],
  row: Row,
  governed: (f: FieldMeta) => boolean,
): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const f of fields) {
    if (serverSet(f) || governed(f) || f.unique || f.kind === "file") continue;
    if (f.class === "secret" || isEmpty(row[f.name])) continue;
    out[f.name] = row[f.name];
  }
  // A copy of "Website relaunch" reads "Website relaunch (copy)" where the title is free text.
  for (const n of titleFieldsOf(model)) {
    const f = fields.find((x) => x.name === n);
    if (f && f.kind === "text" && typeof out[n] === "string")
      out[n] = t("{name} (copy)", { name: out[n] as string });
  }
  return out;
}

/** The refs an action's answer set (a draft invoice it made), as links to offer. */
export function createdRefs(
  model: ModelMeta,
  before: Row,
  after: Record<string, unknown> | null | undefined,
): FieldMeta[] {
  if (!after || typeof after !== "object") return [];
  return model.fields.filter(
    (f) =>
      f.kind === "ref" &&
      !!f.to &&
      f.name in after &&
      !isEmpty(after[f.name]) &&
      after[f.name] !== before[f.name],
  );
}
