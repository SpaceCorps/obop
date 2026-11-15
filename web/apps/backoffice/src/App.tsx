import { Suspense, lazy, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { describeFor, setInstanceDefaults, t, tx, useI18n, type ToastAction } from "@obop/ui";
import {
  call,
  discardSsoReturn,
  loadSettings,
  meta,
  restorePlace,
  type ModuleMeta,
  type Row,
  type Settings,
} from "./api.ts";
import {
  AppIcon,
  Launcher,
  Palette,
  TopBar,
  appBlurb,
  appHref,
  buildApps,
  type App as AppT,
} from "./Chrome.tsx";
import { DataProvider, useData, type Tone } from "./data.tsx";
import { ErrorBoundary, OfflineBanner, OfflineNotice } from "./ErrorBoundary.tsx";
import { modelPageOf, useAppPages, type NavLink } from "./extensions.ts";
import { useHostNotices } from "./host/notices.ts";
import { ModelPage } from "./ModelPage.tsx";
import {
  appHome,
  appMenu,
  groupApps,
  hasOverview,
  homeModel,
  onLauncher,
  pluralTitle,
} from "./nav.ts";
import { titleOf } from "./presentation.ts";
import { quickFilters, quickState, today } from "./quickFilters.ts";
import { quickFilters as listFilters, withoutCovered } from "./filters.ts";
import type { Cond } from "./query.ts";
import { rememberRecord } from "./recent.ts";
import { SetupCard } from "./SetupCard.tsx";
import { parseModulesRoute } from "./modules/route.ts";
import {
  PRODUCT_DOC_TABS,
  isPersonalTab,
  readProductDocsHidden,
  setProductDocsHidden,
  useProductDocsVisible,
} from "./productDocs.ts";
import { OVERVIEW, href, isOverview, nav, overviewHref, useRoute, type Route } from "./route.ts";
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
  // When each toast was shown, so a change of page clears the old ones and keeps a fresh one.
  const toastAt = useRef(new Map<number, number>());

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

  // The bar a phone or an installed app draws around the page takes the colour of the page, in the
  // theme that is applied (light, dark, or the system's).
  useEffect(() => {
    if (!ready || typeof matchMedia !== "function") return;
    const sync = () => {
      const canvas = document.querySelector(".canvas");
      if (!canvas) return;
      const bg = getComputedStyle(canvas).backgroundColor;
      for (const m of document.querySelectorAll('meta[name="theme-color"]')) {
        m.setAttribute("content", bg);
        m.removeAttribute("media");
      }
    };
    const frame = requestAnimationFrame(sync);
    const mq = matchMedia("(prefers-color-scheme: dark)");
    mq.addEventListener("change", sync);
    return () => {
      cancelAnimationFrame(frame);
      mq.removeEventListener("change", sync);
    };
  }, [ready, settings]);

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

  const dismiss = useCallback((id: number) => {
    toastAt.current.delete(id);
    setToasts((t) => t.filter((x) => x.id !== id));
  }, []);
  const notify = useCallback(
    (text: string, tone: Tone = "ok", opts?: { action?: ToastAction }) => {
      const id = ++toastId.current;
      toastAt.current.set(id, Date.now());
      // One success at a time, the newest: a run of saves does not stack a column of toasts.
      // Errors and notes stay (at most three) until they time out or are dismissed.
      setToasts((t) => [
        ...t.filter((x) => tone !== "ok" || x.tone !== "ok").slice(-2),
        { id, text, tone, ...(opts?.action ? { action: opts.action } : {}) },
      ]);
      setTimeout(() => dismiss(id), tone === "ok" && !opts?.action ? 3500 : 7000);
    },
    [dismiss],
  );
  useHostNotices(notify);
  // A new page clears what an older one said; a toast from the last moment (the save that moved
  // here) stays.
  useEffect(() => {
    const now = Date.now();
    setToasts((t) => {
      const keep = t.filter((x) => now - (toastAt.current.get(x.id) ?? 0) < 1200);
      return keep.length === t.length ? t : keep;
    });
  }, [route.app, route.model]);
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
  // An app's address without a model opens its home: the overview, or its home model. An address
  // of a model the app does not have (an old link) opens the home model.
  const atHome =
    !!app && app.id !== "customize" && !route.model && appHome(app.module) === OVERVIEW;
  const model =
    app?.module.models.find((m) => m.name === route.model) ??
    (app && app.id !== "customize" ? homeModel(app.module) : undefined);
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
                  <AppNav
                    app={app}
                    route={route}
                    current={!extPage && !isOverview(route) && !atHome ? model?.name : undefined}
                    overview={!extPage && (isOverview(route) || atHome)}
                    user={email}
                  />
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
              ) : app && (isOverview(route) || atHome) ? (
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
                <Home
                  apps={apps}
                  isAdmin={isAdmin}
                  token={token}
                  settings={settings}
                  onSettings={setSettings}
                />
              )}
            </Suspense>
          </ErrorBoundary>
        </div>
        {launcher && (
          <Launcher apps={apps} recent={recentApps()} onClose={() => setLauncher(false)} />
        )}
        {palette && <PaletteHost apps={apps} route={route} onClose={() => setPalette(false)} />}
        <RecordTracker apps={apps} route={route} />
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

/**
 * The generated sidebar of an app (nav.ts `appMenu`): Overview when the app has one, the main
 * models in the order the module declares them under their plural names, the quick searches of a
 * single-model app, Reporting, and Configuration folded away. Child models (order lines, BOM lines)
 * are reached through their parent and are not listed, unless one is open now.
 */

/**
 * The quick searches of a one-list app's sidebar: the package's saved filters the browser can
 * read, then Open, Overdue, Mine and This month where no saved filter already covers them.
 */
function sidebarFilters(
  model: ModuleMeta["models"][number],
  user: string,
): { key: string; label: string; facets: Cond[]; saved?: boolean }[] {
  const saved = listFilters(model, model.fields, { user, today: today() })
    .filter((q) => q.kind === "saved")
    .map((q) => ({ key: q.key, label: q.label, facets: q.conds, conds: q.conds, kind: "saved" }));
  const generated = quickFilters(model, user).map((q) => ({
    key: q.key,
    label: q.label,
    facets: q.facets,
    conds: q.facets,
    kind: q.key,
  }));
  return withoutCovered([...saved, ...generated]).map((q) => ({
    key: q.key,
    label: q.label,
    facets: q.facets,
    ...(q.kind === "saved" ? { saved: true } : {}),
  }));
}

function AppNav({
  app,
  route,
  current,
  overview,
  user,
}: {
  app: AppT;
  route: Route;
  /** The model whose page is shown, when a model page is shown. */
  current: string | undefined;
  /** The overview page is shown. */
  overview: boolean;
  user: string;
}) {
  const menu = appMenu(app.module);
  const withOverview = hasOverview(app.module);
  const inConfig = menu.configuration.some((m) => m.name === current);
  const [configOpen, setConfigOpen] = useState(inConfig);
  useEffect(() => {
    if (inConfig) setConfigOpen(true);
  }, [inConfig]);
  // A single list: its quick searches stand in for the one lonely link of the sidebar.
  const single = !withOverview && menu.main.length === 1 ? menu.main[0] : undefined;
  // The package's saved filters first (D95 `ui.filter`), then the generated ones they leave open.
  const quick = single ? sidebarFilters(single, user) : [];
  const quickOn = single && route.model === single.name ? route.st : "";
  const onQuick = quick.some((q) => quickState(q) === quickOn);
  const link = (m: ModuleMeta["models"][number]) => (
    <a
      key={m.name}
      href={href({ app: app.id, model: m.name })}
      aria-current={current === m.name && !(single === m && onQuick) ? "page" : undefined}
      title={describeFor(m)}
    >
      {pluralTitle(m, app.module.id)}
    </a>
  );
  const hiddenOpen = menu.hidden.find((m) => m.name === current);
  return (
    <>
      {withOverview && (
        <a href={overviewHref(app.id)} aria-current={overview ? "page" : undefined}>
          {t("Overview")}
        </a>
      )}
      {menu.main.map(link)}
      {hiddenOpen && link(hiddenOpen)}
      {quick.length > 0 && single && (
        <>
          <span className="overline side-label" aria-hidden="true">
            {t("Quick filters")}
          </span>
          {quick.map((q) => {
            const st = quickState(q);
            return (
              <a
                key={q.key}
                href={href({ app: app.id, model: single.name, st })}
                aria-current={current === single.name && quickOn === st ? "page" : undefined}
              >
                {q.saved ? q.label : tx("filter", q.label)}
              </a>
            );
          })}
        </>
      )}
      {menu.reporting.length > 0 && (
        <>
          <span className="overline side-label" aria-hidden="true">
            {t("Reporting")}
          </span>
          {menu.reporting.map(link)}
        </>
      )}
      {menu.configuration.length > 0 && (
        <div className={`side-group${configOpen ? " open" : ""}`}>
          <button
            type="button"
            className="side-toggle"
            aria-expanded={configOpen}
            onClick={(e) => {
              // The phone menu closes on any click inside it; this one only folds the group.
              e.stopPropagation();
              setConfigOpen(!configOpen);
            }}
          >
            <span>{t("Configuration")}</span>
            <Icon name="chevron-down" size={14} />
          </button>
          {configOpen && menu.configuration.map(link)}
        </div>
      )}
    </>
  );
}

/** The palette inside the data layer: it searches records and runs the open record's actions. */
function PaletteHost(p: { apps: AppT[]; route: Route; onClose: () => void }) {
  const d = useData();
  return <Palette apps={p.apps} route={p.route} data={d} onClose={p.onClose} />;
}

/**
 * Remembers the records the person opens (recent.ts), with their titles, for the palette and the
 * overview pages. The title is made again while the record stays open and the labels of its refs
 * arrive.
 */
function RecordTracker({ apps, route }: { apps: AppT[]; route: Route }) {
  const d = useData();
  const open = route.id && route.id !== "new" ? route.id : "";
  const app = apps.find((a) => a.id === route.app && a.id !== "customize");
  const model = app?.module.models.find((m) => m.name === route.model);
  const [row, setRow] = useState<Row | null>(null);
  useEffect(() => {
    setRow(null);
    if (!open || !model) return;
    const cached = d.store.cached(model.name)?.find((r) => r.id === open);
    if (cached) return setRow(cached);
    let live = true;
    void call(d.token, `${model.name}.get`, { id: open }).then((r) => {
      if (live && r.kind === "ok" && r.value && typeof r.value === "object") setRow(r.value as Row);
    });
    return () => {
      live = false;
    };
    // Read once per record opened; the cache changing under it is not a new record.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, model]);
  useEffect(() => {
    if (!row || !app || !model || row.id !== open) return;
    const refText = (m: string, id: string) => {
      const r = d.store.peek(m)?.find((x) => x.id === id);
      return r ? d.recordLabel(m, r) : "";
    };
    const title =
      titleOf(model, row, { refText, moduleId: app.module.id }) ||
      pluralTitle(model, app.module.id);
    rememberRecord({ app: app.id, model: model.name, id: row.id, title });
    // d.version: the labels of refs that arrive make a better title.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [row, d.version]);
  return null;
}

function Home({
  apps,
  isAdmin,
  token,
  settings,
  onSettings,
}: {
  apps: AppT[];
  isAdmin: boolean;
  token: string;
  settings: Settings;
  onSettings: (s: Settings) => void;
}) {
  const [q, setQ] = useState("");
  const listed = apps.filter(onLauncher);
  const recent = recentApps()
    .map((id) => listed.find((a) => a.id === id))
    .filter((a): a is AppT => !!a)
    .slice(0, 4);
  const needle = q.trim().toLowerCase();
  const shown = listed.filter(
    (a) => a.name.toLowerCase().includes(needle) || appBlurb(a).toLowerCase().includes(needle),
  );
  const tile = (a: AppT) => (
    <a key={a.id} className="app-tile card" href={appHref(a)}>
      <AppIcon app={a} size={48} />
      <span className="app-tile-text">
        <strong>{a.name}</strong>
        <small>{appBlurb(a)}</small>
      </span>
    </a>
  );
  return (
    <section className="main panel home">
      <header className="home-head">
        <div>
          <h1>{t("Your apps")}</h1>
          <p className="hint">
            {t("Open an app, or search across all of them from the bar above.")}
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
        {isAdmin && !q && (
          <SetupCard
            token={token}
            settings={settings}
            onSettings={onSettings}
            hasContacts={apps.some((a) => a.id === "base")}
          />
        )}
        {!q && recent.length > 0 && (
          <section aria-label={t("Recently opened")}>
            <h2 className="overline">{t("Recently opened")}</h2>
            <div className="app-grid small">{recent.map(tile)}</div>
          </section>
        )}
        {q ? (
          <section aria-label={t("Results")}>
            <h2 className="overline">{t("Results")}</h2>
            <div className="app-grid">{shown.map(tile)}</div>
            {shown.length === 0 && <p className="hint">{t("No app matches.")}</p>}
          </section>
        ) : (
          groupApps(listed).map((g) => (
            <section key={g.id} aria-label={t(g.label)}>
              <h2 className="overline">{t(g.label)}</h2>
              <div className="app-grid">{g.apps.map(tile)}</div>
            </section>
          ))
        )}
      </div>
    </section>
  );
}
