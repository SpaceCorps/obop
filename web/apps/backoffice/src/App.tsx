import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { loadSettings, login, meta, type ModuleMeta, type Settings } from "./api.ts";
import { AppIcon, Launcher, Palette, TopBar, appBlurb, appHref, buildApps } from "./Chrome.tsx";
import { Customize } from "./Customize.tsx";
import { DashboardsApp } from "./dashboards/DashboardsApp.tsx";
import { BOARD, boardHref } from "./dashboards/route.ts";
import { DataProvider, type Tone } from "./data.tsx";
import { modelLabel } from "./format.ts";
import { ModelPage } from "./ModelPage.tsx";
import { href, nav, useRoute } from "./route.ts";
import {
  Button,
  ConfirmDialog,
  Icon,
  Input,
  Logo,
  Notice,
  Skeleton,
  ToastRegion,
  type ConfirmRequest,
  type ToastItem,
} from "./ui.tsx";
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
  const [busy, setBusy] = useState(false);
  return (
    <main className="signin-wrap">
      <aside className="signin-brand">
        <h1 className="signin-logo-lg">
          <Logo variant="full" size={28} title="obop" />
        </h1>
        <p className="signin-claim">One business, one platform.</p>
        <ul>
          <li>
            <Icon name="users" size={18} /> Every app shares one set of contacts and products
          </li>
          <li>
            <Icon name="shield-check" size={18} /> Personal data is masked unless you may see it
          </li>
          <li>
            <Icon name="sparkle" size={18} /> Agents can do what you can, and ask before they act
          </li>
        </ul>
      </aside>
      <form
        className="signin"
        onSubmit={async (e) => {
          e.preventDefault();
          const f = new FormData(e.currentTarget);
          const user = String(f.get("username"));
          setBusy(true);
          setError(null);
          const t = await login(user, String(f.get("password")));
          setBusy(false);
          if (t) onDone(t, user);
          else setError("Invalid username or password");
        }}
      >
        <h1 className="signin-logo">
          <Logo variant="full" size={30} title="obop" />
        </h1>
        <div className="signin-head">
          <h2>Sign in</h2>
          <p className="hint">Sign in to your instance.</p>
        </div>
        <Input label="Username" name="username" autoComplete="username" required autoFocus />
        <Input
          label="Password"
          name="password"
          type="password"
          autoComplete="current-password"
          required
        />
        {error && <Notice tone="danger" title={error} />}
        <Button type="submit" variant="primary" size="lg" loading={busy}>
          Sign in
        </Button>
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
  const [menu, setMenu] = useState(false);
  const [toasts, setToasts] = useState<ToastItem[]>([]);
  const [confirmReq, setConfirmReq] = useState<ConfirmRequest | null>(null);
  const toastId = useRef(0);

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

  const dismiss = useCallback((id: number) => setToasts((t) => t.filter((x) => x.id !== id)), []);
  const notify = useCallback(
    (text: string, tone: Tone = "ok") => {
      const id = ++toastId.current;
      setToasts((t) => [...t.slice(-3), { id, text, tone }]);
      setTimeout(() => dismiss(id), tone === "ok" ? 3500 : 7000);
    },
    [dismiss],
  );
  const confirm = useCallback(
    (message: string, confirmLabel = "Discard", cancelLabel = "Keep editing") =>
      new Promise<boolean>((resolve) =>
        setConfirmReq({
          message,
          confirmLabel,
          cancelLabel,
          resolve: (ok) => {
            setConfirmReq(null);
            resolve(ok);
          },
        }),
      ),
    [],
  );
  useEffect(() => {
    nav.ask = (m) => confirm(m, "Leave and discard", "Keep editing");
    return () => {
      nav.ask = null;
    };
  }, [confirm]);

  // The menu collapses on phones; picking something closes it.
  useEffect(() => setMenu(false), [route.app, route.model]);

  const isAdmin = modules.some((m) => m.id === "platform");
  const apps = useMemo(() => buildApps(modules, settings, isAdmin), [modules, settings, isAdmin]);
  const app = apps.find((a) => a.id === route.app);
  // Everyone may set their own theme and read the brand book; the rest of Customize is for admins.
  const personalCustomize =
    route.app === "customize" &&
    !isAdmin &&
    ["theme", "brand", "guide"].includes(route.tab || route.model);
  const hasBoards = !!app?.module.models.some((m) => m.name === "dashboards_dashboard");
  const onBoard =
    app?.id === "dashboards" && hasBoards && (route.model === "" || route.model === BOARD);
  const model = app?.module.models.find((m) => m.name === route.model) ?? app?.module.models[0];

  const linkFor = useCallback(
    (name: string, opts?: { id?: string; st?: string }) => {
      const a = apps.find(
        (x) => x.id !== "customize" && x.module.models.some((m) => m.name === name),
      );
      return a ? href({ app: a.id, model: name, id: opts?.id, st: opts?.st }) : null;
    },
    [apps],
  );

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
  if (!ready)
    return (
      <div className="canvas">
        <div className="topbar" />
        <div className="workspace">
          <section className="main panel">
            <Skeleton rows={8} cols={4} />
          </section>
        </div>
      </div>
    );

  return (
    <DataProvider
      token={token}
      modules={modules}
      apps={apps}
      notify={notify}
      confirm={confirm}
      linkFor={linkFor}
    >
      <div className="canvas">
        <TopBar
          apps={apps}
          current={app}
          email={email}
          menuOpen={menu}
          hasMenu={!!app && app.id !== "customize"}
          onMenu={() => setMenu(!menu)}
          onLauncher={() => setLauncher(true)}
          onPalette={() => setPalette(true)}
          onSignOut={signOut}
          onTheme={() => go({ app: "customize", model: "theme", tab: "theme" })}
          onBrand={() => go({ app: "customize", model: "brand", tab: "brand" })}
        />
        <div className="workspace">
          {app && app.id !== "customize" && (
            <nav
              id="app-menu"
              className={`sidebar${menu ? " open" : ""}`}
              aria-label={`${app.name} menu`}
              onClick={() => setMenu(false)}
            >
              <div className="side-head">
                <AppIcon app={app} size={32} />
                <strong>{app.name}</strong>
              </div>
              {hasBoards && app.id === "dashboards" && (
                <a
                  href={boardHref(app.id)}
                  aria-current={onBoard ? "page" : undefined}
                  title="Tiles drawn from your dashboards"
                >
                  Overview
                </a>
              )}
              <span className="overline side-label" aria-hidden="true">
                Records
              </span>
              {app.module.models.map((m) => (
                <a
                  key={m.name}
                  href={href({ app: app.id, model: m.name })}
                  aria-current={!onBoard && model?.name === m.name ? "page" : undefined}
                  title={m.description}
                >
                  {modelLabel(m.name, app.module.id)}
                </a>
              ))}
            </nav>
          )}
          {app?.id === "customize" || personalCustomize ? (
            <Customize
              token={token}
              tab={route.tab || route.model}
              section={route.view}
              anchor={route.section}
              apps={apps}
              modules={modules}
              settings={settings}
              isAdmin={isAdmin}
              go={(tab) => go({ app: "customize", model: tab, tab })}
              goSection={(view) => go({ app: "customize", model: "brand", tab: "brand", view })}
              goAnchor={(section) =>
                go({ app: "customize", model: "guide", tab: "guide", section }, { replace: false })
              }
              onSettings={(s) => {
                setSettings(s);
                applyTheme(resolveTheme(s.theme));
              }}
              onModulesChanged={() => void refresh()}
              notify={notify}
            />
          ) : onBoard && app ? (
            <DashboardsApp app={app} route={route} go={go} owner={email} />
          ) : app && model ? (
            <ModelPage
              key={model.name}
              app={app}
              model={model}
              route={{ ...route, model: model.name }}
              go={go}
            />
          ) : (
            <Home apps={apps} />
          )}
        </div>
        {launcher && (
          <Launcher apps={apps} recent={recentApps()} onClose={() => setLauncher(false)} />
        )}
        {palette && <Palette apps={apps} onClose={() => setPalette(false)} />}
        {confirmReq && <ConfirmDialog req={confirmReq} />}
        <ToastRegion items={toasts} onDismiss={dismiss} />
      </div>
    </DataProvider>
  );
}

function Home({ apps }: { apps: ReturnType<typeof buildApps> }) {
  const [q, setQ] = useState("");
  const recent = recentApps()
    .map((id) => apps.find((a) => a.id === id))
    .filter((a): a is (typeof apps)[number] => !!a)
    .slice(0, 4);
  const shown = apps.filter((a) => a.name.toLowerCase().includes(q.trim().toLowerCase()));
  const first = appHref;
  return (
    <section className="main panel home">
      <header className="home-head">
        <div>
          <h1>Your apps</h1>
          <p className="hint">
            Everything shares one database, one set of permissions and one API.
          </p>
        </div>
        <div className="searchfield">
          <Icon name="search" size={16} />
          <input
            placeholder="Filter apps"
            aria-label="Filter apps"
            value={q}
            onChange={(e) => setQ(e.target.value)}
          />
        </div>
      </header>
      <div className="home-body">
        {!q && recent.length > 0 && (
          <section aria-label="Recently opened">
            <h2 className="overline">Recently opened</h2>
            <div className="app-grid small">
              {recent.map((a) => (
                <a key={a.id} className="app-tile row" href={first(a)}>
                  <AppIcon app={a} size={36} />
                  <span>{a.name}</span>
                </a>
              ))}
            </div>
          </section>
        )}
        <section aria-label="All apps">
          <h2 className="overline">{q ? "Results" : "All apps"}</h2>
          <div className="app-grid">
            {shown.map((a) => (
              <a key={a.id} className="app-tile card" href={first(a)}>
                <AppIcon app={a} size={48} />
                <span className="app-tile-text">
                  <strong>{a.name}</strong>
                  <small>{appBlurb(a)}</small>
                </span>
              </a>
            ))}
          </div>
          {shown.length === 0 && <p className="hint">No app matches.</p>}
        </section>
      </div>
    </section>
  );
}
