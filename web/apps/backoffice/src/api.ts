export type Command = { id: string; description: string; needs_approval: boolean };
export type Row = Record<string, string | number | boolean | null>;
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

export async function commands(token: string): Promise<Command[]> {
  const res = await fetch("/api/commands", { headers: { authorization: `Bearer ${token}` } });
  if (!res.ok) return [];
  return (await res.json()).commands;
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
