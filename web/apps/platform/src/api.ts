import type { Factor } from "@obop/ui";

export type Instance = {
  id: string;
  name: string;
  slug: string;
  status: "provisioning" | "running" | "failed" | "deleting";
  url: string;
  error: string;
  /** What the background job is doing right now. */
  phase: string;
};

type Result<T> =
  | { ok: true; value: T }
  | { ok: false; error: string; status: number; code: string };

async function request<T>(
  token: string,
  method: string,
  path: string,
  body?: object,
): Promise<Result<T>> {
  const res = await fetch(path, {
    method,
    headers: {
      "content-type": "application/json",
      ...(token ? { authorization: `Bearer ${token}` } : {}),
    },
    body: body ? JSON.stringify(body) : undefined,
  });
  const json = await res.json().catch(() => ({}));
  return res.ok
    ? { ok: true, value: json as T }
    : { ok: false, error: json.error ?? res.statusText, status: res.status, code: json.code ?? "" };
}

export const signup = (email: string, password: string) =>
  request<{ token: string }>("", "POST", "/api/platform/signup", { email, password });
/** A sign-in step: a session, or the demand for a second factor (D53). */
export type LoginStep = { token?: string; mfa_required?: boolean; mfa_token?: string };
export const login = (email: string, password: string) =>
  request<LoginStep>("", "POST", "/api/platform/login", { email, password });
export const loginMfa = (mfaToken: string, factor: Factor) =>
  request<LoginStep>("", "POST", "/api/platform/login/mfa", { mfa_token: mfaToken, ...factor });
export const list = (token: string) =>
  request<{ instances: Instance[] }>(token, "GET", "/api/platform/instances");
export const create = (token: string, name: string) =>
  request<Instance>(token, "POST", "/api/platform/instances", { name });
export const remove = (token: string, id: string) =>
  request<Instance>(token, "POST", `/api/platform/instances/${id}/delete`, {});
