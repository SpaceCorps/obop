import { useCallback, useEffect, useRef, useState } from "react";
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

const STEPS = ["Database", "Admin account", "Service", "Build and start", "Address", "Ready"];

function stepOf(i: Instance): number {
  if (i.status === "running") return STEPS.length;
  const p = i.phase;
  if (p.startsWith("Waiting for the address")) return 4;
  if (p.startsWith("Building")) return 3;
  if (p.startsWith("Creating the service")) return 2;
  if (p.startsWith("Creating your admin")) return 1;
  return 0;
}

/** Live progress of an instance being set up or torn down. */
function Progress({ instance }: { instance: Instance }) {
  const started = useRef(Date.now());
  const [, tick] = useState(0);
  useEffect(() => {
    const t = setInterval(() => tick((n) => n + 1), 1000);
    return () => clearInterval(t);
  }, []);
  const secs = Math.floor((Date.now() - started.current) / 1000);
  const elapsed = `${Math.floor(secs / 60)}m ${String(secs % 60).padStart(2, "0")}s`;
  if (instance.status === "deleting") {
    return (
      <div className="progress" role="status">
        <span className="spinner" aria-hidden="true" /> {instance.phase || "Deleting"}{" "}
        <span className="hint">({elapsed})</span>
      </div>
    );
  }
  const step = stepOf(instance);
  return (
    <div className="progress" role="status">
      <ol className="steps-live">
        {STEPS.map((label, n) => (
          <li key={label} className={n < step ? "done" : n === step ? "now" : ""}>
            {n < step ? "✓" : n === step ? <span className="spinner" aria-hidden="true" /> : "·"}{" "}
            {label}
          </li>
        ))}
      </ol>
      <div className="hint">
        {instance.phase || "Queued"} · {elapsed} elapsed. A first build takes a few minutes.
      </div>
    </div>
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
  const busy = instances.some((i) => i.status === "provisioning" || i.status === "deleting");
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
                {i.status === "failed" && `Failed: ${i.error}`}
              </div>
              {(i.status === "provisioning" || i.status === "deleting") && (
                <Progress instance={i} />
              )}
            </div>
            <button
              disabled={i.status === "provisioning" || i.status === "deleting"}
              onClick={async () => {
                if (!confirm(`Delete ${i.name} and all of its data?`)) return;
                // Show the change at once; the teardown continues in the background.
                setInstances((all) =>
                  all.map((x) =>
                    x.id === i.id ? { ...x, status: "deleting", phase: "Queued" } : x,
                  ),
                );
                const r = await remove(token, i.id);
                if (!r.ok) setError(r.error);
                await refresh();
              }}
            >
              {i.status === "deleting" ? "Deleting…" : "Delete"}
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
