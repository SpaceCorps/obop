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
  /**
   * Only for `text` and `longtext` (D52 amendment): `model.field` of a select. The value is drawn with
   * the label of the option it equals there, while `shows_option_when` (an expression on the same
   * record, evaluated here) holds. Only the display changes; see shownOption.ts.
   */
  shows_option_of?: string;
  shows_option_when?: string;
};
/** Row-level access rules of a model (D58): which records the person sees. */
export type AccessMeta = {
  own?: string | null;
  via?: string | null;
  public_field?: string | null;
  shared_with?: string | null;
  see_all_group?: string[];
};
export type ModelMeta = {
  name: string;
  description: string;
  labels?: Translations;
  descriptions?: Translations;
  can_create: boolean;
  /** Whether this principal may create many records at once (bulk_create, D70). */
  can_bulk_create?: boolean;
  can_update: boolean;
  /** Whether this principal may delete records of the model (its write group). */
  can_delete: boolean;
  fields: FieldMeta[];
  actions: Command[];
  /** Present when the person sees only some of the records (D58). */
  access?: AccessMeta;
};
export type ModuleMeta = {
  id: string;
  name: string;
  labels?: Translations;
  descriptions?: Translations;
  /** The one-line card text per language (D52); absent when untranslated. */
  summaries?: Translations;
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

export type Reply =
  | { kind: "ok"; value: unknown; denied: string[] }
  | { kind: "approval"; planId: string }
  | { kind: "error"; status: number; message: string };

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
  } | null;
  if (!body || !Array.isArray(body.modules)) return "failed";
  if (body.core_version) coreVersion = body.core_version;
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
  | { kind: "mfa"; mfaToken: string; methods: string[] }
  | { kind: "error"; status: number; code: string };

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
    };
    if (res.ok && json.mfa_required && json.mfa_token)
      return { kind: "mfa", mfaToken: json.mfa_token, methods: json.methods ?? ["totp"] };
    if (res.ok && json.token)
      return {
        kind: "session",
        token: json.token,
        enrolment: !!json.mfa_enrolment_required,
        ...(json.user ? { user: json.user } : {}),
      };
    return { kind: "error", status: res.status, code: json.code ?? "" };
  } catch {
    return { kind: "error", status: 0, code: "network" };
  }
}

export function login(username: string, password: string): Promise<LoginResult> {
  return loginStep("/api/auth/login", { username, password });
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
