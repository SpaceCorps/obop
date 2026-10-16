import { t } from "@obop/ui";

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

export type Kind = "text" | "longtext" | "number" | "money" | "date" | "bool" | "select" | "ref";
export type FieldMeta = {
  name: string;
  description: string;
  labels?: Translations;
  descriptions?: Translations;
  class: string;
  personal_data: boolean;
  kind: Kind;
  required: boolean;
  /** Only for `select`: the allowed values, in their natural order (for example statuses). */
  options?: string[];
  /** Only for `ref`: the model the id points at. */
  to?: string;
};
export type ModelMeta = {
  name: string;
  description: string;
  labels?: Translations;
  descriptions?: Translations;
  can_create: boolean;
  can_update: boolean;
  /** Whether this principal may delete records of the model (its write group). */
  can_delete: boolean;
  fields: FieldMeta[];
  actions: Command[];
};
export type ModuleMeta = {
  id: string;
  name: string;
  labels?: Translations;
  descriptions?: Translations;
  models: ModelMeta[];
};

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

/** `null` means the token was rejected: the caller signs out. */
export async function meta(token: string): Promise<ModuleMeta[] | null> {
  const res = await fetch("/api/meta", { headers: { authorization: `Bearer ${token}` } });
  if (res.status === 401) return null;
  if (!res.ok) return [];
  const modules = (await res.json()).modules as ModuleMeta[];
  // An older server may not send the kind yet: money by name, everything else text.
  for (const m of modules)
    for (const model of m.models)
      for (const f of model.fields) {
        f.kind ??= f.name.endsWith("_minor") ? "money" : "text";
        f.required ??= false;
      }
  for (const m of modules)
    for (const model of m.models) {
      model.can_delete ??= false;
      model.actions ??= [];
    }
  return modules;
}

/** The most rows any view that needs "all of them" (kanban, pivot, graph, calendar) loads. */
export const LOAD_LIMIT = 5000;

/** One server-side condition of `<model>.search` (D49). */
export type ServerOp = "eq" | "ne" | "contains" | "in" | "gt" | "gte" | "lt" | "lte" | "is_null";
export type ServerFilter = { field: string; op: ServerOp; value?: unknown };
export type ServerOrder = { field: string; dir: "asc" | "desc" };
export type SearchQuery = {
  filter?: ServerFilter[];
  order?: ServerOrder[];
  limit?: number;
  offset?: number;
};
export type SearchResult = { rows: Row[]; total: number; denied: string[] };

/** One page of a model, filtered and ordered on the server. `error` carries the server's message. */
export async function search(
  token: string,
  model: string,
  query: SearchQuery,
): Promise<SearchResult | { error: string; status: number }> {
  const r = await call(token, `${model}.search`, query);
  if (r.kind === "ok") {
    const v = r.value as { rows: Row[]; total: number };
    return { rows: v.rows, total: v.total, denied: r.denied };
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

async function send(token: string, path: string, body: object): Promise<Reply> {
  const res = await fetch(path, {
    method: "POST",
    headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  const json = await res.json().catch(() => ({}));
  if (res.status === 202) return { kind: "approval", planId: json.plan_id };
  if (!res.ok) return { kind: "error", status: res.status, message: json.error ?? res.statusText };
  return { kind: "ok", value: json.value, denied: json.denied ?? [] };
}

export async function login(username: string, password: string): Promise<string | null> {
  const res = await fetch("/api/auth/login", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ username, password }),
  });
  return res.ok ? (await res.json()).token : null;
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
};
