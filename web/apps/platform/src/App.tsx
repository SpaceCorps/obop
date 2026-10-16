import { useCallback, useEffect, useRef, useState } from "react";
import { AppIcon, Badge, Button, ConfirmDialog, Icon, Input, Logo, Notice } from "@obop/ui";
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
  const [busy, setBusy] = useState(false);
  return (
    <main className="auth-wrap">
      <aside className="auth-brand">
        <h1 className="auth-logo-lg">
          <Logo variant="full" size={28} title="obop" />
        </h1>
        <p className="auth-claim">One business, one platform.</p>
        <ul>
          <li>
            <Icon name="website" size={18} /> Your own instance, with its own database and address
          </li>
          <li>
            <Icon name="shield-check" size={18} /> Personal data masked unless you may see it
          </li>
          <li>
            <Icon name="sparkle" size={18} /> Agents that propose, humans that approve
          </li>
        </ul>
        <p className="hint">Free while in beta. No card needed.</p>
      </aside>
      <form
        className="auth"
        onSubmit={async (e) => {
          e.preventDefault();
          const f = new FormData(e.currentTarget);
          const go = mode === "signup" ? signup : login;
          setBusy(true);
          setError(null);
          const r = await go(String(f.get("email")), String(f.get("password")));
          setBusy(false);
          if (r.ok) onToken(r.value.token);
          else setError(r.error);
        }}
      >
        <h1 className="auth-logo">
          <Logo variant="full" size={30} title="obop" />
        </h1>
        <div className="auth-head">
          <h2>{mode === "signup" ? "Create your account" : "Sign in"}</h2>
          <p className="hint">One business, one platform. Sign up and get your own instance.</p>
        </div>
        <Input label="Email" name="email" type="email" autoComplete="email" required autoFocus />
        <Input
          label="Password"
          name="password"
          type="password"
          autoComplete={mode === "signup" ? "new-password" : "current-password"}
          minLength={12}
          required
          hint={mode === "signup" ? "At least 12 characters." : undefined}
        />
        {error && <Notice tone="danger" title={error} />}
        <Button type="submit" variant="primary" size="lg" loading={busy}>
          {mode === "signup" ? "Create account" : "Sign in"}
        </Button>
        <Button
          variant="ghost"
          onClick={() => {
            setError(null);
            setMode(mode === "signup" ? "login" : "signup");
          }}
        >
          {mode === "signup" ? "I already have an account" : "Create an account"}
        </Button>
      </form>
    </main>
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
            <span className="step-mark" aria-hidden="true">
              {n < step ? (
                <Icon name="check" size={12} strokeWidth={2.5} />
              ) : n === step ? (
                <span className="spinner" />
              ) : null}
            </span>
            {label}
          </li>
        ))}
      </ol>
      <p className="hint">
        {instance.phase || "Queued"} · {elapsed} elapsed. A first build takes a few minutes.
      </p>
    </div>
  );
}

const STATUS = {
  running: ["success", "Running"],
  provisioning: ["info", "Setting up"],
  failed: ["danger", "Failed"],
  deleting: ["warning", "Deleting"],
} as const;

export function App() {
  const [token, setTokenState] = useState(stored);
  const [instances, setInstances] = useState<Instance[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [toDelete, setToDelete] = useState<Instance | null>(null);
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
    <div className="shell">
      <header className="p-top">
        <h1 className="p-brand">
          <a href="/">
            <Logo variant="full" size={24} title="obop" />
          </a>
        </h1>
        <span className="grow" />
        <Button variant="ghost" icon="log-out" onClick={() => setToken("")}>
          Sign out
        </Button>
      </header>
      <main className="platform">
        <div className="p-head">
          <h2>Your instances</h2>
          <p className="hint">Sign in to an instance with the same email and password.</p>
        </div>
        {instances.length === 0 && (
          <div className="p-empty panel">
            <span className="empty-art" aria-hidden="true">
              <Icon name="website" size={26} />
            </span>
            <p className="p-empty-title">You have no instances yet.</p>
            <p className="hint">Name one below. It is ready in a few minutes.</p>
          </div>
        )}
        <ul className="instances">
          {instances.map((i) => {
            const [tone, text] = STATUS[i.status];
            const working = i.status === "provisioning" || i.status === "deleting";
            return (
              <li key={i.id} className="panel">
                <div className="inst-main">
                  <AppIcon id="website" name={i.name} size={40} />
                  <div className="inst-text">
                    <div className="inst-title">
                      <strong>{i.name}</strong>
                      <Badge tone={tone}>{text}</Badge>
                    </div>
                    <div className="hint">
                      {i.status === "running" && (
                        <a href={i.url} target="_blank" rel="noreferrer">
                          {i.url}
                        </a>
                      )}
                      {i.status === "failed" && `Failed: ${i.error}`}
                    </div>
                  </div>
                  <Button variant="ghost" disabled={working} onClick={() => setToDelete(i)}>
                    {i.status === "deleting" ? "Deleting…" : "Delete"}
                  </Button>
                </div>
                {working && <Progress instance={i} />}
              </li>
            );
          })}
        </ul>
        <form
          className="create panel"
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
          <Input
            label="New instance"
            name="name"
            placeholder="Acme Trading"
            required
            minLength={3}
            maxLength={30}
            hint="3 to 30 characters. It becomes part of the address."
          />
          <Button type="submit" variant="primary" icon="plus">
            Create
          </Button>
        </form>
        {error && <Notice tone="danger" title={error} />}
      </main>
      {toDelete && (
        <ConfirmDialog
          title="Delete instance?"
          confirmLabel="Delete instance"
          onClose={() => setToDelete(null)}
          onConfirm={() => {
            const i = toDelete;
            setToDelete(null);
            // Show the change at once; the teardown continues in the background.
            setInstances((all) =>
              all.map((x) => (x.id === i.id ? { ...x, status: "deleting", phase: "Queued" } : x)),
            );
            void (async () => {
              const r = await remove(token, i.id);
              if (!r.ok) setError(r.error);
              await refresh();
            })();
          }}
        >
          <p>
            Delete <strong>{toDelete.name}</strong> and all of its data? This cannot be undone.
          </p>
        </ConfirmDialog>
      )}
    </div>
  );
}
