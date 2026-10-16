import { useCallback, useEffect, useRef, useState } from "react";
import {
  AppIcon,
  Badge,
  Button,
  ConfirmDialog,
  Icon,
  Input,
  LanguageMenu,
  Logo,
  Notice,
  msg,
  rich,
  t,
} from "@obop/ui";
import { create, list, login, remove, signup, type Instance } from "./api.ts";
import { clock, phaseText, serverMessage } from "./serverMessage.ts";

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

function Auth({ onToken }: { onToken: (token: string) => void }) {
  const [mode, setMode] = useState<"signup" | "login">("signup");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  return (
    <main className="auth-wrap">
      <aside className="auth-brand">
        <h1 className="auth-logo-lg">
          <Logo variant="full" size={28} title="obop" />
        </h1>
        <p className="auth-claim">{t("One business, one platform.")}</p>
        <ul>
          <li>
            <Icon name="website" size={18} />{" "}
            {t("Your own instance, with its own database and address")}
          </li>
          <li>
            <Icon name="shield-check" size={18} /> {t("Personal data masked unless you may see it")}
          </li>
          <li>
            <Icon name="sparkle" size={18} /> {t("Agents that propose, humans that approve")}
          </li>
        </ul>
        <p className="hint">{t("Free while in beta. No card needed.")}</p>
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
          else setError(serverMessage(r.error));
        }}
      >
        <h1 className="auth-logo">
          <Logo variant="full" size={30} title="obop" />
        </h1>
        <div className="auth-head">
          <h2>{mode === "signup" ? t("Create your account") : t("Sign in")}</h2>
          <p className="hint">
            {t("One business, one platform. Sign up and get your own instance.")}
          </p>
        </div>
        <Input
          label={t("Email")}
          name="email"
          type="email"
          autoComplete="email"
          required
          autoFocus
        />
        <Input
          label={t("Password")}
          name="password"
          type="password"
          autoComplete={mode === "signup" ? "new-password" : "current-password"}
          minLength={12}
          required
          hint={mode === "signup" ? t("At least 12 characters.") : undefined}
        />
        {error && <Notice tone="danger" title={error} />}
        <Button type="submit" variant="primary" size="lg" loading={busy}>
          {mode === "signup" ? t("Create account") : t("Sign in")}
        </Button>
        <Button
          variant="ghost"
          onClick={() => {
            setError(null);
            setMode(mode === "signup" ? "login" : "signup");
          }}
        >
          {mode === "signup" ? t("I already have an account") : t("Create an account")}
        </Button>
        <LanguageMenu align="start" />
      </form>
    </main>
  );
}

const STEPS = [
  msg("Database"),
  msg("Admin account"),
  msg("Service"),
  msg("Build and start"),
  msg("Address"),
  msg("Ready"),
];

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
    const timer = setInterval(() => tick((n) => n + 1), 1000);
    return () => clearInterval(timer);
  }, []);
  const secs = Math.floor((Date.now() - started.current) / 1000);
  const elapsed = clock(String(Math.floor(secs / 60)), String(secs % 60).padStart(2, "0"));
  if (instance.status === "deleting") {
    return (
      <div className="progress" role="status">
        <span className="spinner" aria-hidden="true" /> {phaseText(instance.phase || "Deleting")}{" "}
        <span className="hint">({elapsed})</span>
      </div>
    );
  }
  const step = stepOf(instance);
  return (
    <div className="progress" role="status">
      <ol className="steps-live">
        {STEPS.map((step_, n) => (
          <li key={step_} className={n < step ? "done" : n === step ? "now" : ""}>
            <span className="step-mark" aria-hidden="true">
              {n < step ? (
                <Icon name="check" size={12} strokeWidth={2.5} />
              ) : n === step ? (
                <span className="spinner" />
              ) : null}
            </span>
            {t(step_)}
          </li>
        ))}
      </ol>
      <p className="hint">
        {t("{phase} · {elapsed} elapsed. A first build takes a few minutes.", {
          phase: phaseText(instance.phase || "Queued"),
          elapsed,
        })}
      </p>
    </div>
  );
}

const STATUS = {
  running: ["success", msg("Running")],
  provisioning: ["info", msg("Setting up")],
  failed: ["danger", msg("Failed")],
  deleting: ["warning", msg("Deleting")],
} as const;

export function App() {
  const [token, setTokenState] = useState(stored);
  const [instances, setInstances] = useState<Instance[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [toDelete, setToDelete] = useState<Instance | null>(null);
  const setToken = useCallback((next: string) => {
    remember(next);
    setTokenState(next);
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
    const timer = setInterval(() => void refresh(), 2000);
    return () => clearInterval(timer);
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
        <LanguageMenu />
        <Button variant="ghost" icon="log-out" onClick={() => setToken("")}>
          {t("Sign out")}
        </Button>
      </header>
      <main className="platform">
        <div className="p-head">
          <h2>{t("Your instances")}</h2>
          <p className="hint">{t("Sign in to an instance with the same email and password.")}</p>
        </div>
        {instances.length === 0 && (
          <div className="p-empty panel">
            <span className="empty-art" aria-hidden="true">
              <Icon name="website" size={26} />
            </span>
            <p className="p-empty-title">{t("You have no instances yet.")}</p>
            <p className="hint">{t("Name one below. It is ready in a few minutes.")}</p>
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
                      <Badge tone={tone}>{t(text)}</Badge>
                    </div>
                    <div className="hint">
                      {i.status === "running" && (
                        <a href={i.url} target="_blank" rel="noreferrer">
                          {i.url}
                        </a>
                      )}
                      {i.status === "failed" &&
                        t("Failed: {error}", { error: serverMessage(i.error) })}
                    </div>
                  </div>
                  <Button variant="ghost" disabled={working} onClick={() => setToDelete(i)}>
                    {i.status === "deleting" ? t("Deleting…") : t("Delete")}
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
            } else setError(serverMessage(r.error));
            await refresh();
          }}
        >
          <Input
            label={t("New instance")}
            name="name"
            placeholder={t("Acme Trading")}
            required
            minLength={3}
            maxLength={30}
            hint={t("3 to 30 characters. It becomes part of the address.")}
          />
          <Button type="submit" variant="primary" icon="plus">
            {t("Create")}
          </Button>
        </form>
        {error && <Notice tone="danger" title={error} />}
      </main>
      {toDelete && (
        <ConfirmDialog
          title={t("Delete instance?")}
          confirmLabel={t("Delete instance")}
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
              if (!r.ok) setError(serverMessage(r.error));
              await refresh();
            })();
          }}
        >
          <p>
            {rich(
              "Delete <b>{name}</b> and all of its data? This cannot be undone.",
              { name: toDelete.name },
              { b: (c) => <strong>{c}</strong> },
            )}
          </p>
        </ConfirmDialog>
      )}
    </div>
  );
}
