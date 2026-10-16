export type Command = { id: string; description: string; needs_approval: boolean };
export type Row = Record<string, unknown> & { id: string };

export type FieldMeta = {
  name: string;
  description: string;
  class: string;
  personal_data: boolean;
};
export type ModelMeta = {
  name: string;
  description: string;
  can_create: boolean;
  can_update: boolean;
  fields: FieldMeta[];
  actions: Command[];
};
export type ModuleMeta = { id: string; name: string; models: ModelMeta[] };

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
  return res.ok ? (await res.json()).modules : [];
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
export type Settings = { theme?: Theme; apps?: AppLayout };
