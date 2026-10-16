import { useCallback, useEffect, useState } from "react";
import { create, list, login, remove, signup, type Instance } from "./api.ts";

function stored(): string {
  try {
    return sessionStorage.getItem("obop-platform-token") ?? "";
  } catch {
    return "";
  }
}

function remember(token: string) {
  try {
    if (token) sessionStorage.setItem("obop-platform-token", token);
    else sessionStorage.removeItem("obop-platform-token");
  } catch {
    /* storage unavailable: the session lasts until reload */
  }
}

function Auth({ onToken }: { onToken: (t: string) => void }) {
  const [mode, setMode] = useState<"signup" | "login">("signup");
  const [error, setError] = useState<string | null>(null);
  return (
    <form
      className="signin"
      onSubmit={async (e) => {
        e.preventDefault();
        const f = new FormData(e.currentTarget);
        const go = mode === "signup" ? signup : login;
        const r = await go(String(f.get("email")), String(f.get("password")));
        if (r.ok) onToken(r.value.token);
        else setError(r.error);
      }}
    >
      <h1>obop</h1>
      <p className="hint">One business, one platform. Sign up and get your own instance.</p>
      <label>
        Email
        <input name="email" type="email" autoComplete="email" required autoFocus />
      </label>
      <label>
        Password
        <input
          name="password"
          type="password"
          autoComplete={mode === "signup" ? "new-password" : "current-password"}
          minLength={12}
          required
        />
        {mode === "signup" && <span className="hint">At least 12 characters.</span>}
      </label>
      {error && <p role="alert">{error}</p>}
      <button type="submit">{mode === "signup" ? "Create account" : "Sign in"}</button>
      <button
        type="button"
        className="link"
        onClick={() => setMode(mode === "signup" ? "login" : "signup")}
      >
        {mode === "signup" ? "I already have an account" : "Create an account"}
      </button>
    </form>
  );
}

export function App() {
  const [token, setTokenState] = useState(stored);
  const [instances, setInstances] = useState<Instance[]>([]);
  const [error, setError] = useState<string | null>(null);
  const setToken = useCallback((t: string) => {
    remember(t);
    setTokenState(t);
  }, []);

  const refresh = useCallback(async () => {
    if (!token) return;
    const r = await list(token);
    if (r.ok) setInstances(r.value.instances);
    else setToken("");
  }, [token, setToken]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  // Poll while anything is still being set up.
  const busy = instances.some((i) => i.status === "provisioning");
  useEffect(() => {
    if (!busy) return;
    const t = setInterval(() => void refresh(), 2000);
    return () => clearInterval(t);
  }, [busy, refresh]);

  if (!token) return <Auth onToken={setToken} />;

  return (
    <main className="platform">
      <header>
        <h1>obop</h1>
        <button onClick={() => setToken("")}>Sign out</button>
      </header>
      <h2>Your instances</h2>
      <p className="hint">Sign in to an instance with the same email and password.</p>
      {instances.length === 0 && <p>You have no instances yet.</p>}
      <ul className="instances">
        {instances.map((i) => (
          <li key={i.id}>
            <div>
              <strong>{i.name}</strong>
              <div className="hint">
                {i.status === "running" && (
                  <a href={i.url} target="_blank" rel="noreferrer">
                    {i.url}
                  </a>
                )}
                {i.status === "provisioning" && "Setting up. This takes a minute."}
                {i.status === "failed" && `Failed: ${i.error}`}
              </div>
            </div>
            <button
              disabled={i.status === "provisioning"}
              onClick={async () => {
                if (!confirm(`Delete ${i.name} and all of its data?`)) return;
                const r = await remove(token, i.id);
                if (!r.ok) setError(r.error);
                await refresh();
              }}
            >
              Delete
            </button>
          </li>
        ))}
      </ul>
      <form
        className="create"
        onSubmit={async (e) => {
          e.preventDefault();
          const form = e.currentTarget;
          const name = String(new FormData(form).get("name"));
          const r = await create(token, name);
          if (r.ok) {
            setError(null);
            form.reset();
          } else setError(r.error);
          await refresh();
        }}
      >
        <label>
          New instance
          <input name="name" placeholder="Acme Trading" required minLength={3} maxLength={30} />
        </label>
        <button type="submit">Create</button>
      </form>
      {error && <p role="alert">{error}</p>}
    </main>
  );
}
