import { Suspense, lazy, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { describeFor, setInstanceDefaults, t, useI18n } from "@obop/ui";
import {
  discardSsoReturn,
  loadSettings,
  meta,
  restorePlace,
  type ModuleMeta,
  type Settings,
} from "./api.ts";
import { AppIcon, Launcher, Palette, TopBar, appBlurb, appHref, buildApps } from "./Chrome.tsx";
import { DataProvider, type Tone } from "./data.tsx";
import { ErrorBoundary, OfflineBanner, OfflineNotice } from "./ErrorBoundary.tsx";
import { modelPageOf, useAppPages, type NavLink } from "./extensions.ts";
import { modelTitle } from "./labels.ts";
import { ModelPage } from "./ModelPage.tsx";
import { parseModulesRoute } from "./modules/route.ts";
import {
  PRODUCT_DOC_TABS,
  isPersonalTab,
  readProductDocsHidden,
  setProductDocsHidden,
  useProductDocsVisible,
} from "./productDocs.ts";
import { href, isOverview, nav, useRoute } from "./route.ts";
import { EnrolmentGate } from "./Security.tsx";
import { PasswordGate } from "./SecurityAccount.tsx";
import "./registrations.ts";
import { SignIn } from "./SignIn.tsx";
import {
  ConfirmDialog,
  EmptyState,
  Icon,
  Skeleton,
  ToastRegion,
  type ConfirmRequest,
  type ToastItem,
} from "./ui.tsx";
import { applyTheme, resolveTheme } from "./theme.ts";

// The big screens load on demand, so the first paint is small and a route shows a skeleton while its
// code arrives.
const Customize = lazy(() => import("./Customize.tsx").then((m) => ({ default: m.Customize })));
const AppOverview = lazy(() =>
  import("./AppOverview.tsx").then((m) => ({ default: m.AppOverview })),
);

/** What every route shows while it loads. */
function RouteSkeleton() {
  return (
    <section className="main panel" aria-busy="true" aria-label={t("Loading")}>
      <Skeleton rows={8} cols={4} />
    </section>
  );
}

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
  // The first read of the metadata failed (no connection, or the server answered with an error).
  const [loadFailed, setLoadFailed] = useState(false);
  // The session may only set up two-factor authentication (an administrator policy, D53).
  const [limited, setLimited] = useState(false);
  // The session may only choose a new password (an administrator reset it, B7).
  const [mustChange, setMustChange] = useState(false);
  const [route, go] = useRoute();
  // The Brand book and the Guide are hidden for an instance that asked for it (`productDocs.ts`).
  const docsVisible = useProductDocsVisible();
  const [launcher, setLauncher] = useState(false);
  const [palette, setPalette] = useState(false);
  const [menu, setMenu] = useState(false);
  const [toasts, setToasts] = useState<ToastItem[]>([]);
  const [confirmReq, setConfirmReq] = useState<ConfirmRequest | null>(null);
  const toastId = useRef(0);

  // A return from Microsoft in a tab that already holds a session has nobody to handle it.
  useEffect(() => {
    if (token) discardSsoReturn();
  }, [token]);

  const signOut = useCallback(() => {
    write("obop-token", "");
    write("obop-user", "");
    setTokenState("");
    setReady(false);
    setLimited(false);
    setMustChange(false);
    // The switch belonged to the instance the person signed in to; the next sign-in reads it again.
    setProductDocsHidden(false);
    // The settings belonged to the person who signed in: the sign-in screen that follows speaks the
    // language of the server's default (or the browser's), not the one an administrator set.
    void setInstanceDefaults({});
  }, []);

  const refresh = useCallback(async () => {
    if (!token) return;
    const [m, s, hideDocs] = await Promise.all([
      meta(token),
      loadSettings(token),
      readProductDocsHidden(token),
    ]);
    if (m === null) return signOut();
    if (m === "failed") {
      // Keep what is on screen after a failed refresh; show the recovery screen on a first load.
      setLoadFailed(true);
      return;
    }
    setLoadFailed(false);
    setLimited(m === "enrolment");
    setMustChange(m === "password");
    if (m === "enrolment" || m === "password") return setReady(true);
    // Before the first screen: the Brand book and the Guide are never drawn and then taken away. A
    // read that failed changes nothing.
    if (hideDocs !== undefined) setProductDocsHidden(hideDocs);
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
    nav.ask = (message, leave, stay) => confirm(message, leave, stay);
    return () => {
      nav.ask = null;
    };
  }, [confirm]);

  // The menu collapses on phones; picking something closes it.
  useEffect(() => setMenu(false), [route.app, route.model]);

  // An address of the Brand book or the Guide, while they are hidden, opens the first tab that is
  // shown (Theme) instead, and the address says so.
  useEffect(() => {
    if (
      ready &&
      !docsVisible &&
      route.app === "customize" &&
      PRODUCT_DOC_TABS.includes(route.tab || route.model)
    )
      go({ app: "customize", model: "theme", tab: "theme" }, { replace: true });
  }, [ready, docsVisible, route.app, route.tab, route.model, go]);

  // The platform module is what Customize is made of. Everyone sees it now (the `device` model is
  // theirs to list), so an administrator is told by `module`, the list of installed modules, which
  // only an administrator may read.
  const isAdmin = modules.some(
    (m) => m.id === "platform" && m.models.some((x) => x.name === "module"),
  );
  // The names of the apps and their order are translated and sorted when the list is built, so it is
  // built again when the language or the region changes (the root renders again, but a memo keeps its value).
  const { lang, locale } = useI18n();
  const apps = useMemo(
    () => buildApps(modules, settings, isAdmin),
    [modules, settings, isAdmin, lang, locale],
  );
  const app = apps.find((a) => a.id === route.app);
  // Everyone may set their own theme and read the brand book; the rest of Customize is for admins.
  const personalCustomize =
    route.app === "customize" && !isAdmin && isPersonalTab(route.tab || route.model, docsVisible);
  // What the open app adds to the shell: sidebar entries and pages of its own (extensions.ts).
  const ext = useAppPages(
    app && app.id !== "customize" ? { app, route, go, token, email, ready } : null,
  );
  const extPage = ext.page ?? null;
  const model = app?.module.models.find((m) => m.name === route.model) ?? app?.module.models[0];
  const Page = (model && modelPageOf(model.name)) ?? ModelPage;

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
          // Back to the deep link the person began with, as a password sign-in keeps it.
          restorePlace();
        }}
      />
    );
  if (ready && mustChange)
    return (
      <PasswordGate
        token={token}
        user={email}
        onDone={() => {
          setReady(false);
          void refresh();
        }}
        onSignOut={signOut}
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
  if (!ready && loadFailed)
    return (
      <div className="canvas">
        <div className="topbar" />
        <div className="workspace">
          <section className="main panel">
            <OfflineNotice />
            <EmptyState
              icon="warning"
              title={t("Could not load your workspace")}
              hint={t(
                "The server did not answer, or answered with an error. Your data is safe. Check the connection and try again.",
              )}
              action={{
                label: t("Try again"),
                onClick: () => {
                  setLoadFailed(false);
                  void refresh();
                },
              }}
              secondary={{ label: t("Sign out"), onClick: signOut }}
            />
          </section>
        </div>
      </div>
    );
  if (!ready)
    return (
      <div className="canvas">
        <div className="topbar" />
        <div className="workspace">
          <RouteSkeleton />
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
          onSecurity={() => go({ app: "customize", model: "security", tab: "security" })}
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
              {ext.nav ?? (
                <>
                  {ext.navBefore?.map((l) => (
                    <NavEntry key={l.key} link={l} />
                  ))}
                  <span className="overline side-label" aria-hidden="true">
                    {t("Records")}
                  </span>
                  {app.module.models.map((m) => (
                    <a
                      key={m.name}
                      href={href({ app: app.id, model: m.name })}
                      aria-current={
                        !extPage && !isOverview(route) && model?.name === m.name
                          ? "page"
                          : undefined
                      }
                      title={describeFor(m)}
                    >
                      {modelTitle(m, app.module.id)}
                    </a>
                  ))}
                  {ext.navAfter?.map((l) => (
                    <NavEntry key={l.key} link={l} />
                  ))}
                </>
              )}
            </nav>
          )}
          <ErrorBoundary scope="page" resetKey={`${route.app}/${route.model}/${route.tab}`}>
            <Suspense fallback={<RouteSkeleton />}>
              {app?.id === "customize" || personalCustomize ? (
                <Customize
                  token={token}
                  onSignOut={signOut}
                  tab={route.tab || route.model}
                  section={route.view}
                  anchor={route.section}
                  apps={apps}
                  modules={modules}
                  settings={settings}
                  isAdmin={isAdmin}
                  me={email}
                  modulesRoute={parseModulesRoute(route)}
                  goModules={(patch) => {
                    const next = { ...parseModulesRoute(route), ...patch };
                    go({
                      app: "customize",
                      model: "modules",
                      tab: "modules",
                      view: next.tab === "installed" ? "" : next.tab,
                      id: next.id,
                      section: next.panel === "overview" ? "" : next.panel,
                    });
                  }}
                  go={(tab) => go({ app: "customize", model: tab, tab })}
                  goSection={(view) => go({ app: "customize", model: "brand", tab: "brand", view })}
                  goAnchor={(section) =>
                    go(
                      { app: "customize", model: "guide", tab: "guide", section },
                      { replace: false },
                    )
                  }
                  onSettings={(s) => {
                    setSettings(s);
                    applyTheme(resolveTheme(s.theme));
                  }}
                  onModulesChanged={() => void refresh()}
                  notify={notify}
                />
              ) : extPage ? (
                extPage
              ) : app && isOverview(route) ? (
                <AppOverview app={app} route={route} go={go} />
              ) : app && model ? (
                <Page
                  key={model.name}
                  app={app}
                  model={model}
                  route={{ ...route, model: model.name }}
                  go={go}
                />
              ) : (
                <Home apps={apps} />
              )}
            </Suspense>
          </ErrorBoundary>
        </div>
        {launcher && (
          <Launcher apps={apps} recent={recentApps()} onClose={() => setLauncher(false)} />
        )}
        {palette && <Palette apps={apps} onClose={() => setPalette(false)} />}
        {confirmReq && <ConfirmDialog req={confirmReq} />}
        <OfflineBanner />
        <ToastRegion items={toasts} onDismiss={dismiss} />
      </div>
    </DataProvider>
  );
}

/** A sidebar entry an app adds (extensions.ts). */
function NavEntry({ link }: { link: NavLink }) {
  return (
    <a href={link.href} aria-current={link.current ? "page" : undefined} title={link.title}>
      {link.label}
    </a>
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
