export type Instance = {
  id: string;
  name: string;
  slug: string;
  status: "provisioning" | "running" | "failed";
  url: string;
  error: string;
};

type Result<T> = { ok: true; value: T } | { ok: false; error: string };

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
    : { ok: false, error: json.error ?? res.statusText };
}

export const signup = (email: string, password: string) =>
  request<{ token: string }>("", "POST", "/api/platform/signup", { email, password });
export const login = (email: string, password: string) =>
  request<{ token: string }>("", "POST", "/api/platform/login", { email, password });
export const list = (token: string) =>
  request<{ instances: Instance[] }>(token, "GET", "/api/platform/instances");
export const create = (token: string, name: string) =>
  request<Instance>(token, "POST", "/api/platform/instances", { name });
export const remove = (token: string, id: string) =>
  request<{ deleted: boolean }>(token, "POST", `/api/platform/instances/${id}/delete`, {});
