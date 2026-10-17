import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { setInstanceDefaults, t } from "@obop/ui";
import { loadSettings, meta, type ModuleMeta, type Settings } from "./api.ts";
import { AppIcon, Launcher, Palette, TopBar, appBlurb, appHref, buildApps } from "./Chrome.tsx";
import { Customize } from "./Customize.tsx";
import { DashboardsApp } from "./dashboards/DashboardsApp.tsx";
import { BOARD, boardHref } from "./dashboards/route.ts";
import { DataProvider, type Tone } from "./data.tsx";
import { modelTitle } from "./labels.ts";
import { ModelPage } from "./ModelPage.tsx";
import { href, nav, useRoute } from "./route.ts";
import { EnrolmentGate } from "./Security.tsx";
import { SignIn } from "./SignIn.tsx";
import {
  ConfirmDialog,
  Icon,
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

export function App() {
  const [token, setTokenState] = useState(() => read("obop-token"));
  const [email, setEmail] = useState(() => read("obop-user"));
  const [modules, setModules] = useState<ModuleMeta[]>([]);
  const [settings, setSettings] = useState<Settings>({});
  const [ready, setReady] = useState(false);
  // The session may only set up two-factor authentication (an administrator policy, D53).
  const [limited, setLimited] = useState(false);
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
    setLimited(false);
  }, []);

  const refresh = useCallback(async () => {
    if (!token) return;
    const [m, s] = await Promise.all([meta(token), loadSettings(token)]);
    if (m === null) return signOut();
    setLimited(m === "enrolment");
    if (m === "enrolment") return setReady(true);
    setModules(m);
    setSettings(s);
    applyTheme(resolveTheme(s.theme));
    void setInstanceDefaults({ language: s.language, currency: s.currency });
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
    (message: string, confirmLabel = t("Discard"), cancelLabel = t("Keep editing")) =>
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
    nav.ask = (m) => confirm(m, t("Leave and discard"), t("Keep editing"));
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
    ["theme", "security", "brand", "guide"].includes(route.tab || route.model);
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
  if (ready && limited)
    return (
      <EnrolmentGate
        token={token}
        user={email}
        onDone={() => {
          setReady(false);
          void refresh();
        }}
        onSignOut={signOut}
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
              aria-label={t("{app} menu", { app: app.name })}
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
                  title={t("Tiles drawn from your dashboards")}
                >
                  {t("Overview")}
                </a>
              )}
              <span className="overline side-label" aria-hidden="true">
                {t("Records")}
              </span>
              {app.module.models.map((m) => (
                <a
                  key={m.name}
                  href={href({ app: app.id, model: m.name })}
                  aria-current={!onBoard && model?.name === m.name ? "page" : undefined}
                  title={m.description}
                >
                  {modelTitle(m, app.module.id)}
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
          <h1>{t("Your apps")}</h1>
          <p className="hint">
            {t("Everything shares one database, one set of permissions and one API.")}
          </p>
        </div>
        <div className="searchfield">
          <Icon name="search" size={16} />
          <input
            placeholder={t("Filter apps")}
            aria-label={t("Filter apps")}
            value={q}
            onChange={(e) => setQ(e.target.value)}
          />
        </div>
      </header>
      <div className="home-body">
        {!q && recent.length > 0 && (
          <section aria-label={t("Recently opened")}>
            <h2 className="overline">{t("Recently opened")}</h2>
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
        <section aria-label={t("All apps")}>
          <h2 className="overline">{q ? t("Results") : t("All apps")}</h2>
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
          {shown.length === 0 && <p className="hint">{t("No app matches.")}</p>}
        </section>
      </div>
    </section>
  );
}
