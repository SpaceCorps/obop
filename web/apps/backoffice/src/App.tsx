import { useCallback, useEffect, useMemo, useState } from "react";
import { loadSettings, login, meta, type ModuleMeta, type Settings } from "./api.ts";
import { AppIcon, Launcher, Palette, TopBar, buildApps } from "./Chrome.tsx";
import { Customize } from "./Customize.tsx";
import { modelLabel } from "./format.ts";
import { ModelPage } from "./ModelPage.tsx";
import { href, useRoute } from "./route.ts";
import { applyTheme, resolveTheme } from "./theme.ts";

function read(key: string): string {
  try {
    return sessionStorage.getItem(key) ?? "";
  } catch {
    return "";
  }
}

function write(key: string, v: string) {
  try {
    if (v) sessionStorage.setItem(key, v);
    else sessionStorage.removeItem(key);
  } catch {
    /* storage unavailable: the session lasts until reload */
  }
}

function recentApps(): string[] {
  try {
    return JSON.parse(localStorage.getItem("obop-recent") ?? "[]") as string[];
  } catch {
    return [];
  }
}

function SignIn({ onDone }: { onDone: (token: string, user: string) => void }) {
  const [error, setError] = useState<string | null>(null);
  return (
    <main className="signin-wrap">
      <form
        className="signin panel"
        onSubmit={async (e) => {
          e.preventDefault();
          const f = new FormData(e.currentTarget);
          const user = String(f.get("username"));
          const t = await login(user, String(f.get("password")));
          if (t) onDone(t, user);
          else setError("Invalid username or password");
        }}
      >
        <h1>obop</h1>
        <p className="hint">Sign in to your instance.</p>
        <label className="field">
          <span className="flabel">Username</span>
          <input name="username" autoComplete="username" required autoFocus />
        </label>
        <label className="field">
          <span className="flabel">Password</span>
          <input name="password" type="password" autoComplete="current-password" required />
        </label>
        {error && (
          <p role="alert" className="error">
            {error}
          </p>
        )}
        <button type="submit" className="btn primary">
          Sign in
        </button>
      </form>
    </main>
  );
}

export function App() {
  const [token, setTokenState] = useState(() => read("obop-token"));
  const [email, setEmail] = useState(() => read("obop-user"));
  const [modules, setModules] = useState<ModuleMeta[]>([]);
  const [settings, setSettings] = useState<Settings>({});
  const [ready, setReady] = useState(false);
  const [route, go] = useRoute();
  const [launcher, setLauncher] = useState(false);
  const [palette, setPalette] = useState(false);
  const [toast, setToast] = useState<string | null>(null);

  const signOut = useCallback(() => {
    write("obop-token", "");
    write("obop-user", "");
    setTokenState("");
    setReady(false);
  }, []);

  const refresh = useCallback(async () => {
    if (!token) return;
    const [m, s] = await Promise.all([meta(token), loadSettings(token)]);
    if (m === null) return signOut();
    setModules(m);
    setSettings(s);
    applyTheme(resolveTheme(s.theme));
    setReady(true);
  }, [token, signOut]);

  useEffect(() => void refresh(), [refresh]);

  useEffect(() => {
    const on = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPalette((p) => !p);
      }
    };
    addEventListener("keydown", on);
    return () => removeEventListener("keydown", on);
  }, []);

  const notify = useCallback((t: string) => {
    setToast(t);
    setTimeout(() => setToast((c) => (c === t ? null : c)), 3500);
  }, []);

  const isAdmin = modules.some((m) => m.id === "platform");
  const apps = useMemo(() => buildApps(modules, settings, isAdmin), [modules, settings, isAdmin]);
  const app = apps.find((a) => a.id === route.app);
  const model = app?.module.models.find((m) => m.name === route.model) ?? app?.module.models[0];

  // Remember recently opened apps for the launcher.
  useEffect(() => {
    if (!app) return;
    try {
      localStorage.setItem(
        "obop-recent",
        JSON.stringify([app.id, ...recentApps().filter((x) => x !== app.id)].slice(0, 6)),
      );
    } catch {
      /* ignore */
    }
  }, [app]);

  if (!token)
    return (
      <SignIn
        onDone={(t, u) => {
          write("obop-token", t);
          write("obop-user", u);
          setEmail(u);
          setTokenState(t);
        }}
      />
    );
  if (!ready) return <p className="hint pad">Loading…</p>;

  return (
    <div className="canvas">
      <TopBar
        apps={apps}
        current={app}
        email={email}
        onLauncher={() => setLauncher(true)}
        onPalette={() => setPalette(true)}
        onSignOut={signOut}
        onTheme={() => go({ app: "customize", model: "theme", tab: "theme" })}
      />
      <div className="workspace">
        {app && app.id !== "customize" && (
          <nav className="sidebar panel" aria-label={`${app.name} menu`}>
            <div className="side-head">
              <AppIcon app={app} size={32} />
              <strong>{app.name}</strong>
            </div>
            {app.module.models.map((m) => (
              <a
                key={m.name}
                href={href({ app: app.id, model: m.name })}
                aria-current={model?.name === m.name}
                title={m.description}
              >
                {modelLabel(m.name, app.module.id)}
              </a>
            ))}
          </nav>
        )}
        {app?.id === "customize" ? (
          <Customize
            token={token}
            tab={route.tab}
            apps={apps}
            settings={settings}
            isAdmin={isAdmin}
            go={(tab) => go({ app: "customize", model: tab, tab })}
            onSettings={(s) => {
              setSettings(s);
              applyTheme(resolveTheme(s.theme));
            }}
            onModulesChanged={() => void refresh()}
            notify={notify}
          />
        ) : app && model ? (
          <ModelPage
            key={model.name}
            token={token}
            app={app}
            model={model}
            route={{ ...route, model: model.name }}
            go={go}
            notify={notify}
          />
        ) : (
          <Home apps={apps} />
        )}
      </div>
      {launcher && (
        <Launcher apps={apps} recent={recentApps()} onClose={() => setLauncher(false)} />
      )}
      {palette && <Palette apps={apps} onClose={() => setPalette(false)} />}
      {toast && (
        <div className="toast panel" role="status">
          {toast}
        </div>
      )}
    </div>
  );
}

function Home({ apps }: { apps: ReturnType<typeof buildApps> }) {
  return (
    <section className="main panel home">
      <h1>Your apps</h1>
      <p className="hint">Everything shares one database, one set of permissions and one API.</p>
      <div className="app-grid">
        {apps.map((a) => (
          <a
            key={a.id}
            className="app-tile"
            href={href({ app: a.id, model: a.module.models[0]?.name })}
          >
            <AppIcon app={a} size={64} />
            <span>{a.name}</span>
          </a>
        ))}
      </div>
    </section>
  );
}
