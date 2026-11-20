import { fmt, getLang, isPseudo, t, type Factor } from "@obop/ui";
import { NETWORK_MESSAGE, NetworkError, request, serverFault } from "./net.ts";
import { resolveShownOptions } from "./shownOption.ts";

/** Translations the server may add to metadata: language code -> text (D34; absent today). */
export type Translations = Record<string, string>;

export type Command = {
  id: string;
  description: string;
  /** Display label per language; falls back to the client table, then the humanised name. */
  labels?: Translations;
  descriptions?: Translations;
  needs_approval: boolean;
  /** The select field this action moves (a transition), else null. */
  field?: string | null;
  /** The state the record must be in, or null for any state. */
  from?: string | null;
  /** The state the action moves the record to. */
  to?: string | null;
  /** Whether no action of the model, whoever may run it, leaves `to` (#102); set by the server. */
  one_way?: boolean;
  /** D95 hints for the record panel (PROPOSED); absent until the package declares them. */
  ui?: ActionUi;
};

// ---- D95: UI hints in /api/meta (PROPOSED) -----------------------------------------------------
// Every key is optional: the server sends a hint only when a package declares it, and the back
// office falls back to the heuristics of presentation.ts, where the shapes are documented too.

/** How a value is drawn (it changes the display, never what is stored). */
export type FieldDisplay =
  | "time_of_day"
  | "duration_minutes"
  | "percent"
  | "percent_bp"
  | "km_from_m"
  | "epoch_seconds"
  | "markdown"
  | "tel"
  | "email"
  | "url"
  | "address";
/** The meaning of a select option's colour. */
export type ValueTone = "ok" | "bad" | "warn" | "info" | "neutral";
/** The kinds of view a model can open on. */
export type ViewHint =
  | "list"
  | "kanban"
  | "calendar"
  | "pivot"
  | "graph"
  | "gallery"
  | "grid"
  | "timeline"
  | "tree";
/** Where a model sits in its app's sidebar. */
export type MenuPlace = "main" | "configuration" | "reporting" | "hidden";

/** `[model.field.ui]`. */
export type FieldUi = {
  display?: FieldDisplay;
  /** Whether totals add the field up: `none` for a price or a rate. */
  aggregate?: "sum" | "avg" | "none";
  /** Option value -> tone; overrides the word lists of presentation.ts `toneOf`. */
  tones?: Record<string, ValueTone>;
  /** A due-like date turns amber this many days before it (red once past). */
  warn_before_days?: number;
  /** Left out of lists, cards and the form (still readable through the API). */
  hidden?: boolean;
  /** Where the currency of a money field comes from: a path such as `journal_id.currency`. */
  currency_from?: string;
  /**
   * A condition on the saved record (the display subset, displayCondition.ts) while which the form
   * shows the field read-only. A hint only: the server holds its own rule.
   */
  readonly_when?: string;
};

/**
 * `default` of a field: a literal, or one of `today`, `today+N`, `$user`, `$employee`,
 * `$instance.currency`, `$open:<model>`.
 */
export type FieldDefault = string | number | boolean;

/** `[[model.ui.section]]`. */
export type UiSection = {
  label: string;
  fields: string[];
  collapsed?: boolean;
  /** Shown only while the record's state is one of these. */
  show_in?: string[];
  /** The label per language from the package's label files (R17), absent when none translates it. */
  labels?: Record<string, string>;
};
/**
 * `[[model.ui.filter]]`: a saved filter offered by the control panel; `labels` is its label per
 * language from the package's label files (U10), absent when none translates it.
 */
export type UiFilter = { label: string; where: string; labels?: Record<string, string> };

/** `[model.ui]`. */
export type ModelUi = {
  /** A template such as `{code} {name}`; alternatives separated by `|`, the first fully set wins. */
  title?: string;
  subtitle?: string;
  /** Field names, in order. */
  list?: string[];
  card?: string[];
  /** The money or number field the kanban column sums. */
  card_sum?: string;
  /** The fields the quick-create form asks for. */
  quick_create?: string[];
  /** `date desc, number desc`. */
  order?: string;
  default_view?: ViewHint;
  menu?: MenuPlace;
  menu_section?: string;
  menu_order?: number;
  calendar?: { start: string; end?: string; color?: string };
  report?: { rows?: string; cols?: string; measure?: string };
  section?: UiSection[];
  filter?: UiFilter[];
  /** The label of one of `filter` that a list opened without a search of its own starts with. */
  default_filter?: string;
  /** Where a record is seen on the public site: `/blog/{slug}`. */
  public_url?: string;
};

/** The D77 guard of a model, as meta exposes it: when its records are frozen. */
export type ModelGuard = {
  /** Records whose `field` is one of `values` are frozen. */
  immutable_when?: { field: string; values: string[] };
  /** Stored fields that stay editable on a frozen record. */
  mutable_fields?: string[];
  /** The only moves a frozen record may make. */
  exit_to?: { to: string; action: string; may_change?: string[] }[];
  no_delete_when?: { field: string; values: string[] };
  /** Records dated on or before the lock date are frozen. */
  lock_date?: { field: string; closed_by?: string; setting?: string };
  /** A ref field of this model: a record is frozen while its parent is. */
  children_of?: string;
  /** Records can be created and read, never changed or deleted. */
  append_only?: boolean;
  /** What the lock banner says ("Posted entries cannot be changed. Reverse it to correct it."). */
  message?: string;
  /** The message per language, from `[labels.<lang>.model.<m>.guard]` (D52). */
  message_labels?: Record<string, string>;
};

/**
 * A guard of `mutable_fields` alone (no `immutable_when`, `lock_date`, `children_of` or
 * `append_only`) freezes every saved record: only those fields change once it is created, as the
 * server holds it (`obop_commands` `engine/frozen.rs`, `Frozen::Created`).
 */
export const fixedOnceCreated = (g: ModelGuard): boolean =>
  !!g.mutable_fields?.length &&
  !g.immutable_when &&
  !g.lock_date &&
  !g.children_of &&
  !g.append_only;

/** A child model drawn inside its parent's form (lines, a compact list, images, translations). */
export type ChildMeta = {
  model: string;
  /** The child's ref field that points at the parent. */
  via: string;
  shape?: "lines" | "list" | "images" | "translations";
};

/** `[model.action.ui]`. */
export type ActionUi = {
  /** The main button of the record in the states this action leaves. */
  primary?: boolean;
  /** Goes to the overflow menu behind a confirm. */
  danger?: boolean;
  /** The question asked before it runs. */
  confirm?: string;
  /** The toast after it ran. */
  done?: string;
  /** `confirm` and `done` per language from the package's label files (R17). */
  confirm_labels?: Record<string, string>;
  done_labels?: Record<string, string>;
  /** Actions with the same group show as one entry (the variant valid for the state runs). */
  group?: string;
  /** Fields the action input may carry, asked for in a small dialog (`reject_reason`). */
  needs?: string[];
  /** A field naming a person who may not run the action on this record (`driver`). */
  not_by?: string;
};

export type Row = Record<string, unknown> & { id: string };

export type Kind =
  | "text"
  | "longtext"
  | "number"
  | "money"
  | "date"
  | "datetime"
  | "decimal"
  | "bool"
  | "select"
  | "ref"
  | "image"
  | "file";
export type FieldMeta = {
  name: string;
  description: string;
  labels?: Translations;
  descriptions?: Translations;
  /** Only for `select`: option value -> language -> label (D52); absent when untranslated. */
  option_labels?: Record<string, Translations>;
  class: string;
  personal_data: boolean;
  /** This caller does not read the field in clear (an agent and personal data): no sort or filter by it. */
  masked?: boolean;
  kind: Kind;
  required: boolean;
  /** Only for `select`: the allowed values, in their natural order (for example statuses). */
  options?: string[];
  /** Only for `ref`: the model the id points at. */
  to?: string;
  /** Only for `image` and `file`: the largest upload in bytes. */
  max_bytes?: number;
  /** Only for `decimal` (D72): digits in all, digits after the point, and the import rounding. */
  precision?: number;
  scale?: number;
  round?: "half_even" | "half_up" | "down";
  /**
   * Only for `text` and `longtext` (D90): the most characters (Unicode code points, so "å" and an
   * emoji count one each) a written value has, which is the default of the kind when the package
   * declares none, and the fewest, when it declares a minimum. Absent for computed and numbered
   * fields.
   */
  max_length?: number;
  min_length?: number;
  /** Only for `text` (K45, U3): the named format a written value has. */
  pattern?: "currency_code" | "iban" | "vat_id";
  /**
   * Only for `number`, `money` (minor units) and `decimal` (a canonical string at the scale of the
   * field) (D90): the inclusive bounds the package declares.
   */
  min?: number | string;
  max?: number | string;
  /** The model the field belongs to (set when the metadata is loaded). */
  model?: string;
  /** Moves only through the model's actions, also for someone who can run none of them (D48). */
  governed?: boolean;
  /** Computed by the server when the record is read; never sent (D57). */
  computed?: boolean;
  /** No two records may share the value (D57). */
  unique?: boolean;
  /** A number the server assigns; never sent (D57). */
  sequence?: boolean;
  /** The number comes from a gapless series (posted entries, receipts); absent: it may skip. */
  gapless?: boolean;
  /**
   * Only for `text` and `longtext` (D52 amendment): `model.field` of a select. The value is drawn with
   * the label of the option it equals there, while `shows_option_when` (an expression on the same
   * record, evaluated here) holds. Only the display changes; see shownOption.ts.
   */
  shows_option_of?: string;
  shows_option_when?: string;
  /** D95 (PROPOSED): display hints. */
  ui?: FieldUi;
  /** D95: what the server fills in on create when the field is missing. */
  default?: FieldDefault;
  /** D95: a path such as `product_id.list_price_minor` the field is filled from on create. */
  fill_from?: string;
  /** D95: set by the server only; refused on create and update. */
  readonly?: boolean;
  /** D95: an expression a picked record must satisfy (`active = true && project_id = $project_id`). */
  ref_filter?: string;
};
/** Row-level access rules of a model (D58): which records the person sees. */
export type AccessMeta = {
  own?: string | null;
  via?: string | null;
  public_field?: string | null;
  shared_with?: string | null;
  see_all_group?: string[];
  /** Whether this caller sees every record (holds a `see_all` group, or is the administrator). */
  sees_all?: boolean;
};
export type ModelMeta = {
  name: string;
  description: string;
  labels?: Translations;
  descriptions?: Translations;
  can_create: boolean;
  /** Whether this principal may create many records at once (bulk_create, D70). */
  can_bulk_create?: boolean;
  /**
   * Set by the back office, not by the server: the person may create records of this model, but
   * only through the button of another screen (`ACTION_ONLY_MODELS`), so `can_create` and
   * `can_bulk_create` are turned off here and the generic New and Import are not offered.
   */
  via_action?: boolean;
  can_update: boolean;
  /** Whether this principal may delete records of the model (its write group). */
  can_delete: boolean;
  fields: FieldMeta[];
  actions: Command[];
  /** Present when the person sees only some of the records (D58). */
  access?: AccessMeta;
  /** D95 (PROPOSED): presentation hints. */
  ui?: ModelUi;
  /** D77 as D95 exposes it: when records are frozen. */
  guard?: ModelGuard;
  /** D95: the child models drawn inside the form; absent: the client's mirror of `resolve_child`. */
  children?: ChildMeta[];
  /** D95: the English plural ("Journal entries"); absent: humanised from the name. */
  plural?: string;
  /** D95: the plural per language (D52). */
  plural_labels?: Translations;
};
export type ModuleMeta = {
  id: string;
  name: string;
  labels?: Translations;
  descriptions?: Translations;
  /** The one-line card text per language (D52); absent when untranslated. */
  summaries?: Translations;
  /** The English one-line card text (D55); absent when the module declares none. */
  summary?: string;
  /** D55: finance, sales, supply-chain, people, services, marketing, productivity, platform, other. */
  categories?: string[];
  /** D55: how settled the module is. */
  maturity?: "stable" | "beta" | "experimental";
  /** D95 (PROPOSED): where the app opens: a model name or `overview`. */
  home?: string;
  /** D95: false keeps the app off the launcher (it lives under another app). */
  launcher?: boolean;
  /** Models in the module's declaration order (F4), never alphabetical. */
  models: ModelMeta[];
};

let coreVersion = "";
let platformZone = "UTC";
let tzdbVersion = "";
/** The instance's time zone and the version of its pinned IANA database (D72), from `/api/meta`. */
export function getTime(): { zone: string; tzdb: string } {
  return { zone: platformZone, tzdb: tzdbVersion };
}
/** Set what `/api/meta` said about time; the meta call and the tests of zone-aware code use it. */
export function setTime(zone: string | undefined, tzdb = ""): void {
  platformZone = zone || "UTC";
  tzdbVersion = tzdb;
}
/** The running core version, from the last `/api/meta` (empty until it has been read). */
export function getCoreVersion(): string {
  return coreVersion;
}
let instanceCurrency = "";
/**
 * The instance currency (D95: `currency` of `/api/meta`, `OBOP_CURRENCY` on the server, EUR by
 * default); empty until the metadata has been read or from an older server. Pass it as
 * presentation.ts `PresentCtx.instanceCurrency`.
 */
export function getInstanceCurrency(): string {
  return instanceCurrency;
}

export type Reply =
  | { kind: "ok"; value: unknown; denied: string[] }
  | { kind: "approval"; planId: string }
  | { kind: "error"; status: number; message: string };

/**
 * Models whose records are made only by a command that returns something the generic form would
 * throw away. `sign_link.create` mints a one-time link and returns it once, and the server keeps
 * only its hash: made from the generic New form, the link is lost and an active link nobody has
 * stays behind (D92, docs/SIGN.md). The button "Create signing link" on a sign request is the one
 * place that shows it. No registry flag says this yet, so the back office names the model.
 * `outbound_mail` is queued only by `outbound_mail.send` (docs/spec/mail.md); the server refuses
 * its generated create and import.
 */
export const ACTION_ONLY_MODELS: ReadonlySet<string> = new Set(["sign_link", "outbound_mail"]);

/** Turn off the generic New and Import of the models in `ACTION_ONLY_MODELS`, and remember the right. */
export function markActionOnly(modules: ModuleMeta[]): void {
  for (const m of modules)
    for (const model of m.models)
      if (ACTION_ONLY_MODELS.has(model.name)) {
        model.via_action = model.can_create;
        model.can_create = false;
        model.can_bulk_create = false;
      }
}

export async function call(token: string, id: string, body: object = {}): Promise<Reply> {
  const [model, command] = id.split(".");
  return send(token, `/api/${model}/${command}`, body);
}

export async function approve(token: string, planId: string): Promise<Reply> {
  return send(token, `/api/approve/${planId}`, {});
}

/**
 * `null` means the token was rejected: the caller signs out. `"enrolment"` means the session may
 * only set up two-factor authentication, which an administrator policy requires (D53). `"failed"`
 * means the server could not be reached or answered with an error: the caller offers a retry.
 */
export async function meta(
  token: string,
): Promise<ModuleMeta[] | null | "enrolment" | "password" | "failed"> {
  let res: Response;
  try {
    res = await request(
      "/api/meta",
      { headers: { authorization: `Bearer ${token}` } },
      { retry: true },
    );
  } catch {
    return "failed";
  }
  if (res.status === 401) return null;
  if (res.status === 403) {
    const body = (await res.json().catch(() => ({}))) as { code?: string };
    if (body.code === "mfa_enrolment_required") return "enrolment";
    if (body.code === "password_change_required") return "password";
  }
  if (!res.ok) return "failed";
  const body = (await res.json().catch(() => null)) as {
    modules: ModuleMeta[];
    core_version?: string;
    time?: { zone?: string; tzdb_version?: string };
    currency?: string;
  } | null;
  if (!body || !Array.isArray(body.modules)) return "failed";
  if (body.core_version) coreVersion = body.core_version;
  if (body.currency) instanceCurrency = body.currency;
  setTime(body.time?.zone, body.time?.tzdb_version);
  const modules = body.modules;
  // An older server may not send the kind yet: money by name, everything else text.
  for (const m of modules)
    for (const model of m.models)
      for (const f of model.fields) {
        f.kind ??= f.name.endsWith("_minor") ? "money" : "text";
        f.required ??= false;
        f.model = model.name;
      }
  for (const m of modules)
    for (const model of m.models) {
      model.can_delete ??= false;
      model.actions ??= [];
    }
  markActionOnly(modules);
  resolveShownOptions(modules);
  return modules;
}

/** The most rows any view that needs "all of them" (kanban, pivot, graph, calendar) loads. */
export const LOAD_LIMIT = 5000;

/** One server-side condition of `<model>.search` (D49). */
export type ServerOp = "eq" | "ne" | "contains" | "in" | "gt" | "gte" | "lt" | "lte" | "is_null";
export type ServerFilter = {
  field: string;
  op: ServerOp;
  value?: unknown;
  /** On a date or datetime: compare the period it falls in (`value` is its label, D72). */
  bucket?: "day" | "week" | "iso_week" | "month" | "year";
  /** With `bucket` on a datetime: an IANA zone name; the instance's when absent. */
  zone?: string;
};
export type ServerOrder = { field: string; dir: "asc" | "desc" };
export type SearchQuery = {
  filter?: ServerFilter[];
  order?: ServerOrder[];
  limit?: number;
  offset?: number;
  /** BCP-47 tag that picks the server's text collation; filled in from the active locale. */
  lang?: string;
};
export type SearchResult = { rows: Row[]; total: number; denied: string[] };

/** One page of a model, filtered and ordered on the server. `error` carries the server's message. */
export async function search(
  token: string,
  model: string,
  query: SearchQuery,
): Promise<SearchResult | { error: string; status: number }> {
  // Text sorts the way this browser would (D3): the server needs to know the language.
  const q = query.order?.length && !query.lang ? { ...query, lang: fmt.locale } : query;
  const r = await call(token, `${model}.search`, q);
  if (r.kind === "ok") {
    const v = r.value as { rows: Row[]; total: number };
    return { rows: v.rows, total: v.total, denied: r.denied };
  }
  return r.kind === "error"
    ? { error: r.message, status: r.status }
    : { error: t("Needs approval"), status: 202 };
}

/** One entry of `group_by`: a field, or a date field with a bucket. */
export type AggGroupBy = {
  field: string;
  bucket?: "day" | "week" | "iso_week" | "month" | "year";
  /** For a datetime: an IANA zone name; the instance's zone when absent (D72). */
  zone?: string;
};
/**
 * One measure: `count`, or `sum`, `min` or `max` of a number, money or decimal field; `avg` of a
 * decimal field, with the digits and rounding of the result.
 */
export type AggMeasure =
  | { op: "count" }
  | { op: "sum" | "min" | "max"; field: string }
  | { op: "avg"; field: string; scale?: number; round?: "half_even" | "half_up" | "down" };
export type AggGroup = {
  /** Group field -> value (`null` for empty; a date bucket is `YYYY-MM-DD` or `YYYY-MM`). */
  key: Record<string, unknown>;
  count: number;
  /**
   * `sum:amount_minor` -> number (`min` and `max` of nothing are `null`). The views only ask for
   * number and money measures; the server also answers a decimal field's measure as its exact
   * decimal string (D72), which no view asks for yet.
   */
  measures: Record<string, number | null>;
};
export type AggResult = { groups: AggGroup[]; total: number; denied: string[] };

/**
 * Groups and totals computed on the server over every record that matches `filter` (D4), with the
 * same access rules as `search`: a field the person cannot read answers `unknown field`.
 */
export async function aggregate(
  token: string,
  model: string,
  q: { filter?: ServerFilter[]; group_by?: AggGroupBy[]; measures?: AggMeasure[] },
): Promise<AggResult | { error: string; status: number }> {
  const body: Record<string, unknown> = {
    aggregate: {
      group_by: (q.group_by ?? []).map((g) => (g.bucket ? g : g.field)),
      measures: q.measures ?? [{ op: "count" }],
    },
    lang: fmt.locale,
  };
  if (q.filter?.length) body.filter = q.filter;
  const r = await call(token, `${model}.search`, body);
  if (r.kind === "ok") {
    const v = r.value as { groups?: AggGroup[]; total: number };
    if (!Array.isArray(v.groups))
      return { error: "aggregate is not supported by this server", status: 501 };
    return { groups: v.groups, total: v.total, denied: r.denied };
  }
  return r.kind === "error"
    ? { error: r.message, status: r.status }
    : { error: t("Needs approval"), status: 202 };
}

/** Rows of a model for the views that need all of them, up to `LOAD_LIMIT`, and the real total. */
export async function list(
  token: string,
  model: string,
): Promise<{ rows: Row[]; denied: string[]; total: number } | { error: string }> {
  const r = await search(token, model, { limit: LOAD_LIMIT });
  if ("error" in r) {
    // Admin-only models outside the generated set have `list` but no `search`.
    if (r.status === 404) {
      const l = await call(token, `${model}.list`);
      if (l.kind === "ok") {
        const rows = l.value as Row[];
        return { rows, denied: l.denied, total: rows.length };
      }
    }
    return { error: r.error };
  }
  return { rows: r.rows, denied: r.denied, total: r.total };
}

/** Commands that only read: safe to repeat when the connection drops. */
const READS = new Set(["get", "list", "search"]);

/**
 * The language the interface shows, as the server knows it: a pseudo-locale (the translators'
 * tooling) is accented English, so it is `en`.
 */
function shownLanguage(): string {
  const lang = getLang();
  return isPseudo(lang) ? "en" : lang;
}

/**
 * The language the interface shows, as `Accept-Language`, so that the server words a refusal (the
 * message of an assertion, which a package can translate) in it, not in the browser's language
 * (the person may have chosen another one in the app).
 */
export function languageHeader(): Record<string, string> {
  return { "accept-language": shownLanguage() };
}

/**
 * `path` with `lang=<the language the interface shows>` in its query string; a query it has is
 * kept and a `lang` in it is replaced. The server picks the language of a refusal from `?lang=`,
 * then the `obop_lang` cookie, then `Accept-Language`. The header alone is not enough here: the
 * cookie is the same-origin page's, it can differ from what the interface shows (an instance
 * default the person never chose, a link with `?lang=`, an HttpOnly cookie of the forum that
 * `document.cookie` cannot overwrite) and it outranks the header. The query outranks the cookie.
 */
export function withLanguage(path: string): string {
  const at = path.indexOf("?");
  const params = new URLSearchParams(at < 0 ? "" : path.slice(at + 1));
  params.set("lang", shownLanguage());
  return `${at < 0 ? path : path.slice(0, at)}?${params}`;
}

async function send(token: string, path: string, body: object): Promise<Reply> {
  const idempotent = READS.has(path.split("/").pop() ?? "");
  let res: Response;
  try {
    res = await request(
      withLanguage(path),
      {
        method: "POST",
        headers: {
          authorization: `Bearer ${token}`,
          "content-type": "application/json",
          ...languageHeader(),
        },
        body: JSON.stringify(body),
      },
      { retry: idempotent },
    );
  } catch (e) {
    if (e instanceof NetworkError) return { kind: "error", status: 0, message: NETWORK_MESSAGE };
    throw e;
  }
  const json = await res.json().catch(() => ({}));
  if (res.status === 202) return { kind: "approval", planId: json.plan_id };
  if (!res.ok) {
    // A proxy or a crash answers with HTML or nothing: say that the server had a problem.
    const message = json.error ?? (res.status >= 500 ? serverFault(res.status) : res.statusText);
    return { kind: "error", status: res.status, message: message || serverFault(res.status) };
  }
  return { kind: "ok", value: json.value, denied: json.denied ?? [] };
}

/** What a sign-in step produced (D53). */
export type LoginResult =
  | { kind: "session"; token: string; enrolment: boolean; user?: string }
  | { kind: "mfa"; mfaToken: string; methods: string[]; user?: string }
  | { kind: "error"; status: number; code: string; message?: string };

async function loginStep(path: string, body: object): Promise<LoginResult> {
  try {
    const res = await fetch(path, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    });
    const json = (await res.json().catch(() => ({}))) as {
      token?: string;
      mfa_required?: boolean;
      mfa_token?: string;
      mfa_enrolment_required?: boolean;
      methods?: string[];
      user?: string;
      code?: string;
      error?: string;
    };
    if (res.ok && json.mfa_required && json.mfa_token)
      return {
        kind: "mfa",
        mfaToken: json.mfa_token,
        methods: json.methods ?? ["totp"],
        ...(json.user ? { user: json.user } : {}),
      };
    if (res.ok && json.token)
      return {
        kind: "session",
        token: json.token,
        enrolment: !!json.mfa_enrolment_required,
        ...(json.user ? { user: json.user } : {}),
      };
    return {
      kind: "error",
      status: res.status,
      code: json.code ?? "",
      ...(typeof json.error === "string" ? { message: json.error } : {}),
    };
  } catch {
    return { kind: "error", status: 0, code: "network" };
  }
}

export function login(username: string, password: string): Promise<LoginResult> {
  return loginStep("/api/auth/login", { username, password });
}

/**
 * The login the server signed in with `token`, from the two-factor status (`user`), or null when
 * it cannot be read. A password sign-in's answer does not name the login, and `$user` of a filter
 * or a read-only hint compares with the server's (docs/ISSUES.md U11).
 */
export async function signedInLogin(token: string): Promise<string | null> {
  try {
    const res = await fetch("/api/auth/mfa", { headers: { authorization: `Bearer ${token}` } });
    if (!res.ok) return null;
    const json = (await res.json()) as { user?: unknown };
    return typeof json.user === "string" && json.user ? json.user : null;
  } catch {
    return null;
  }
}

/**
 * What the address says after the browser came back from Microsoft (D91): `#oidc=<one-time code>`
 * or `#oidc_error=<word>`. The server sends the browser to these two fragments and nothing else;
 * a fragment is not sent to any server, and it is cleared from the address as soon as it is read.
 */
export type SsoReturn = { code: string } | { error: SsoErrorWord };
export type SsoErrorWord = "cancelled" | "unavailable" | "busy" | "failed";

export function parseSsoReturn(hash: string): SsoReturn | null {
  const code = /^#oidc=(obopsso_[0-9a-f]{20,128})$/.exec(hash);
  if (code?.[1]) return { code: code[1] };
  const err = /^#oidc_error=([a-z]{1,16})$/.exec(hash);
  if (!err) return null;
  const word = err[1];
  // Any word this page does not know is the general failure.
  return {
    error: word === "cancelled" || word === "unavailable" || word === "busy" ? word : "failed",
  };
}

// Read when this module loads, which is before anything is rendered, so that the router of the
// app never sees the fragment as a place to go to; the address is cleaned at once.
let ssoReturn: SsoReturn | null = (() => {
  try {
    const r = parseSsoReturn(location.hash);
    if (r || /^#oidc(_error)?=/.test(location.hash))
      history.replaceState(null, "", location.pathname + location.search);
    return r;
  } catch {
    return null;
  }
})();

/** The return from Microsoft that is waiting to be handled, once. */
export function takeSsoReturn(): SsoReturn | null {
  const r = ssoReturn;
  ssoReturn = null;
  return r;
}

/**
 * A return that nobody will handle: the tab already holds a session, so the sign-in page never
 * mounts. Dropped, so that it cannot surface after a later sign-out as a notice nobody caused.
 */
export function discardSsoReturn(): void {
  ssoReturn = null;
}

const SSO_PLACE = "obop-sso-place";

/**
 * The router address of a deep link, kept in this tab across the round trip to Microsoft (the
 * server returns to the root, and the fragment is cleaned). A password sign-in keeps the address
 * by itself; this gives the Microsoft sign-in the same.
 */
export function rememberPlace(): void {
  try {
    if (/^#\/[\w\-./?=&%:~]{1,512}$/.test(location.hash))
      sessionStorage.setItem(SSO_PLACE, location.hash);
    else sessionStorage.removeItem(SSO_PLACE);
  } catch {
    /* a private window: the person lands on the launcher */
  }
}

export function forgetPlace(): void {
  try {
    sessionStorage.removeItem(SSO_PLACE);
  } catch {
    /* ignore */
  }
}

/** Go back to the remembered place, once; nothing remembered, nothing happens. */
export function restorePlace(): void {
  try {
    const h = sessionStorage.getItem(SSO_PLACE);
    sessionStorage.removeItem(SSO_PLACE);
    if (h && /^#\/[\w\-./?=&%:~]{1,512}$/.test(h)) location.hash = h;
  } catch {
    /* ignore */
  }
}

/**
 * The waits, in milliseconds, before the sign-in page asks again whether Microsoft is offered
 * after the answer 429 (the read is limited per address and minute, N19: an office behind one
 * address could hit it, and a Microsoft-only person would see no button). Tests set them to 0.
 */
export const ssoRetry = { delays: [15_000, 20_000, 30_000] };

/** Whether the sign-in page offers Sign in with Microsoft. Nothing is offered on any doubt. */
export async function ssoEnabled(): Promise<boolean> {
  for (let attempt = 0; ; attempt++) {
    try {
      const res = await fetch("/api/auth/sso");
      if (res.status === 429 && attempt < ssoRetry.delays.length) {
        await new Promise((done) => setTimeout(done, ssoRetry.delays[attempt]));
        continue;
      }
      if (!res.ok) return false;
      const json = (await res.json().catch(() => ({}))) as { enabled?: boolean };
      return json.enabled === true;
    } catch {
      return false;
    }
  }
}

/**
 * Whether this instance has no account yet, so the sign-in page offers to create the first
 * administrator with the set-up code from the server log (D96). Nothing is offered on any doubt: a
 * refused or odd answer is "closed", and the normal sign-in form shows.
 */
export type SetupState =
  | { open: false }
  | { open: true; codeActive: boolean; needsSecretKey: boolean };

export async function setupStatus(): Promise<SetupState> {
  try {
    const res = await fetch("/api/auth/setup", { cache: "no-store" });
    if (!res.ok) return { open: false };
    const json = (await res.json().catch(() => ({}))) as {
      open?: unknown;
      code_active?: unknown;
      needs_secret_key?: unknown;
    };
    if (json.open !== true) return { open: false };
    return {
      open: true,
      codeActive: json.code_active === true,
      needsSecretKey: json.needs_secret_key === true,
    };
  } catch {
    return { open: false };
  }
}

/** Create the first administrator: the same answer as a password sign-in (D96). */
export function setupCreate(body: {
  code: string;
  username: string;
  password: string;
  instance_name?: string;
}): Promise<LoginResult> {
  return loginStep("/api/auth/setup", body);
}

/** Where the button sends the browser: the server starts the flow and redirects to Microsoft. */
export const SSO_START = "/api/auth/oidc/start";

/** Spend the one-time code: the same answer as the password step gives. */
export function ssoExchange(code: string): Promise<LoginResult> {
  return loginStep("/api/auth/oidc/exchange", { code });
}

/** The second step: the `mfa_token` of the first, and a code or a recovery code. */
export function loginMfa(mfaToken: string, factor: Factor): Promise<LoginResult> {
  return loginStep("/api/auth/login/mfa", { mfa_token: mfaToken, ...factor });
}

export async function loadSettings(token: string): Promise<Settings> {
  const r = await call(token, "setting.get", { id: "instance" });
  if (r.kind !== "ok") return {};
  try {
    return JSON.parse((r.value as { value: string }).value) as Settings;
  } catch {
    return {};
  }
}

export async function saveSettings(token: string, s: Settings): Promise<Reply> {
  return call(token, "setting.update", { id: "instance", value: JSON.stringify(s) });
}

export type Theme = {
  accent?: string;
  mode?: "system" | "light" | "dark";
  density?: "comfortable" | "compact";
  radius?: "sharp" | "soft" | "round";
};
export type AppLayout = {
  order?: string[];
  hidden?: string[];
  names?: Record<string, string>;
  colors?: Record<string, string>;
};
export type Settings = {
  theme?: Theme;
  apps?: AppLayout;
  /** Instance default language (a person's own choice wins). */
  language?: string;
  /** ISO 4217 code of the instance's currency; a record's own `currency` field wins. */
  currency?: string;
  /** Sign-in policy (D53). Absent keys take the server's defaults. */
  security?: { require_mfa_for_admins?: boolean; require_mfa_for_all?: boolean };
};
