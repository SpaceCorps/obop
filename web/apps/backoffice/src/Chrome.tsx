import { useEffect, useMemo, useRef, useState } from "react";
import {
  AppIcon as TileIcon,
  Avatar,
  Button,
  Dialog,
  Icon,
  IconButton,
  InstanceLogo,
  Kbd,
  LANGS,
  LANG_INFO,
  Menu,
  Spinner,
  compareText,
  getLang,
  inSentence,
  isPseudo,
  labelFor,
  pageBrand,
  pseudo,
  t,
  tx,
  useI18n,
  type IconName,
  type MenuItem,
} from "@obop/ui";
import { call, search, type ModelMeta, type ModuleMeta, type Row, type Settings } from "./api.ts";
import type { useData } from "./data.tsx";
import { appColor } from "./theme.ts";
import { seamedName } from "./brandName.ts";
import { modulesHref } from "./modules/route.ts";
import { href, nav, type Route } from "./route.ts";
import { appHomeOf } from "./extensions.ts";
import { hostMenuItems } from "./host/menu.ts";
import "./registrations.ts";
import { productDocsVisible } from "./productDocs.ts";
import { actionTitle, appTitle, modelTitle } from "./labels.ts";
import { appMenu, groupApps, homeHref, onLauncher, pluralTitle } from "./nav.ts";
import { nextActions, searchFieldsOf, titleOf } from "./presentation.ts";
import { recentRecords } from "./recent.ts";
import { serverMessage } from "./serverMessage.ts";
import { shortcut } from "./keys.ts";
import { notForRecord } from "./workflow.ts";

export type App = { id: string; name: string; color: string; module: ModuleMeta };

/** The data layer, when the palette is drawn inside it (App.tsx); tests draw it without. */
type Data = ReturnType<typeof useData>;

/** A phone or a touch screen: no Escape key, so a dialog without a close button needs a Cancel. */
const NO_ESCAPE_QUERY = "(max-width: 40rem), (hover: none) and (pointer: coarse)";

/** Whether a media query matches as a dialog opens; false where it cannot be asked (tests). */
function matchesNow(query: string): boolean {
  return typeof matchMedia === "function" && matchMedia(query).matches;
}

/** The width at which the top bar is a phone's (style.css): the app's name may take two lines. */
const NARROW_BAR = "(max-width: 40rem)";

/** Whether the top bar is a phone's, following a turn of the phone or a resized window. */
function useNarrowBar(): boolean {
  const [narrow, setNarrow] = useState(() => matchesNow(NARROW_BAR));
  useEffect(() => {
    if (typeof matchMedia !== "function") return;
    const mq = matchMedia(NARROW_BAR);
    const on = () => setNarrow(mq.matches);
    mq.addEventListener("change", on);
    return () => mq.removeEventListener("change", on);
  }, []);
  return narrow;
}

/** Apply the instance's launcher layout: order, hidden apps, renamed apps and chosen colours. */
export function buildApps(modules: ModuleMeta[], settings: Settings, isAdmin: boolean): App[] {
  const layout = settings.apps ?? {};
  const all = modules
    .filter((m) => m.id !== "platform")
    .map((m) => {
      // A name an admin typed is kept as typed; the module's own name is translated.
      const name = layout.names?.[m.id] ?? appTitle(m);
      // The tile colour and glyph come from the English name so they do not change with the language.
      return { id: m.id, name, color: appColor(m.id, layout.colors?.[m.id], m.name), module: m };
    });
  const order = layout.order ?? [];
  all.sort((a, b) => {
    const ia = order.indexOf(a.id);
    const ib = order.indexOf(b.id);
    return (ia < 0 ? 999 : ia) - (ib < 0 ? 999 : ib) || compareText(a.name, b.name);
  });
  const hidden = new Set(layout.hidden ?? []);
  const apps = all.filter((a) => !hidden.has(a.id) || isAdmin);
  if (isAdmin) {
    const platform = modules.find((m) => m.id === "platform");
    if (platform)
      apps.push({
        id: "customize",
        name: tx("app", "Customize"),
        color: appColor("customize"),
        module: platform,
      });
  }
  return apps;
}

/**
 * Where an app opens: the page its registration names (extensions.ts: the Dashboards overview),
 * else its home (nav.ts): the generated overview of an app with two or more main models, else its
 * main model. The module's D95 `home` decides when it is set.
 */
export function appHref(app: App): string {
  if (app.id === "customize") return href({ app: "customize", model: "theme", tab: "theme" });
  return appHomeOf(app) ?? homeHref(app);
}

export function AppIcon({
  app,
  size = 44,
}: {
  app: App | { name: string; color: string; id?: string };
  size?: number;
}) {
  return (
    <TileIcon id={"id" in app ? app.id : undefined} name={app.name} color={app.color} size={size} />
  );
}

/**
 * Short description of what an app holds: the module's one-line summary in the active language
 * (D55, D52), else its English summary, else the names of its main records.
 */
export function appBlurb(app: App): string {
  if (app.id === "customize") return t("Theme, apps and modules");
  const m = app.module;
  const own = labelFor({ labels: m.summaries }, undefined, "");
  if (own) return own;
  // The English summary, accented in the translators' pseudo-locale like every untranslated text.
  if (m.summary) return isPseudo(getLang()) ? pseudo(m.summary) : m.summary;
  const main = appMenu(m).main;
  const labels = (main.length ? main : m.models).slice(0, 3).map((x) => pluralTitle(x, m.id));
  return labels.join(", ");
}

export function TopBar(props: {
  apps: App[];
  current: App | undefined;
  email: string;
  menuOpen: boolean;
  hasMenu: boolean;
  onMenu: () => void;
  onLauncher: () => void;
  onPalette: () => void;
  onSignOut: () => void;
  onTheme: () => void;
  onBrand: () => void;
  onSecurity: () => void;
}) {
  const { current } = props;
  const i18n = useI18n();
  const narrow = useNarrowBar();
  const docs = productDocsVisible();
  const languages: MenuItem = {
    label: t("Language"),
    icon: "globe",
    // The current language by its own name; the translators' pseudo-locales are not named.
    value: isPseudo(i18n.lang) ? undefined : LANG_INFO[i18n.lang]?.native,
    items: LANGS.map((l) => ({
      label: LANG_INFO[l].native,
      icon: i18n.lang === l ? ("check" as const) : undefined,
      onSelect: () => void i18n.setLanguage(l),
    })),
  };
  return (
    <header className="topbar">
      <IconButton
        icon="grid"
        label={t("Open the app launcher")}
        onClick={props.onLauncher}
        iconSize={20}
      />
      {props.hasMenu && (
        <IconButton
          className="menu-btn"
          icon="panel-left"
          label={t("Toggle the menu")}
          aria-expanded={props.menuOpen}
          aria-controls="app-menu"
          onClick={props.onMenu}
        />
      )}
      <a className="brand" href="#/" title={current ? undefined : pageBrand()?.name}>
        {current ? (
          <>
            <InstanceLogo variant="mark" size={22} title="" />
            <span className="brand-sep" aria-hidden="true">
              /
            </span>
            <span className="brand-app">
              {/* On a phone a long compound breaks at its seams instead of being cut (Q18). */}
              {narrow ? seamedName(current.name) : current.name}
            </span>
          </>
        ) : (
          <>
            <InstanceLogo variant="full" size={24} title="" />
            {/* With a brand the logo shows its name as text; without one the link is named like this. */}
            {pageBrand() ? null : <span className="sr-only">obop</span>}
          </>
        )}
      </a>
      <button
        className="searchpill"
        // The words are hidden on a phone; the name stays for a screen reader.
        aria-label={t("Search apps, records and actions")}
        onClick={props.onPalette}
      >
        <Icon name="search" size={16} />
        <span aria-hidden="true">{t("Search apps, records and actions")}</span>
        <Kbd>{shortcut("K")}</Kbd>
      </button>
      <div className="systray">
        <Menu
          ariaLabel={t("Account")}
          align="end"
          triggerClassName="avatar-btn"
          trigger={<Avatar name={props.email || "?"} size={30} />}
          items={[
            { heading: props.email || t("Signed in") },
            { label: t("Theme"), icon: "sun", onSelect: props.onTheme },
            languages,
            { label: t("Security"), icon: "shield-check", onSelect: props.onSecurity },
            ...(docs
              ? [{ label: t("Brand book"), icon: "sparkle" as const, onSelect: props.onBrand }]
              : []),
            ...hostMenuItems(),
            "separator",
            { label: t("Sign out"), icon: "log-out", onSelect: props.onSignOut },
          ]}
        />
      </div>
    </header>
  );
}

export function Launcher({
  apps,
  recent,
  onClose,
}: {
  apps: App[];
  recent: string[];
  onClose: () => void;
}) {
  const [q, setQ] = useState("");
  // On a touch screen focus starts on the grid, so the keyboard does not cover the apps the launcher
  // was opened to tap; with a mouse and keyboard the search takes the first key at once.
  const [touch] = useState(() => matchesNow("(pointer: coarse)"));
  const [cancel] = useState(() => matchesNow(NO_ESCAPE_QUERY));
  // In the order of the home screen: by what the apps are for, then the instance's own order.
  const listed = groupApps(apps.filter(onLauncher)).flatMap((g) => g.apps);
  const shown = listed.filter((a) => a.name.toLowerCase().includes(q.toLowerCase()));
  const recents = recent
    .map((id) => listed.find((a) => a.id === id))
    .filter((a): a is App => !!a)
    .slice(0, 4);
  const first = appHref;
  return (
    <Dialog bare title={t("Apps")} size="lg" className="launcher" onClose={onClose}>
      <div className="searchfield">
        <Icon name="search" size={16} />
        <input
          data-autofocus={touch ? undefined : true}
          placeholder={t("Search apps")}
          value={q}
          onChange={(e) => setQ(e.target.value)}
          aria-label={t("Search apps")}
        />
        {cancel && (
          <Button variant="ghost" className="search-cancel" onClick={onClose}>
            {t("Cancel")}
          </Button>
        )}
      </div>
      <div
        className="launcher-body"
        data-autofocus={touch || undefined}
        tabIndex={touch ? -1 : undefined}
      >
        {!q && recents.length > 0 && (
          <section aria-label={t("Recent apps")}>
            <h3 className="overline">{t("Recent")}</h3>
            <div className="app-grid small">
              {recents.map((a) => (
                <a key={a.id} href={first(a)} onClick={onClose} className="app-tile row">
                  <AppIcon app={a} size={36} />
                  <span>{a.name}</span>
                </a>
              ))}
            </div>
          </section>
        )}
        <section aria-label={q ? t("Results") : t("All apps")}>
          <h3 className="overline">{q ? t("Results") : t("All apps")}</h3>
          <div className="app-grid">
            {shown.map((a) => (
              <a key={a.id} href={first(a)} onClick={onClose} className="app-tile">
                <AppIcon app={a} size={56} />
                <span>{a.name}</span>
              </a>
            ))}
            {shown.length === 0 && <p className="hint">{t("No app matches.")}</p>}
          </div>
        </section>
      </div>
    </Dialog>
  );
}

// ---- command palette -----------------------------------------------------------------------------

type Hit = {
  key: string;
  title: string;
  hint: string;
  /** Where it goes; absent for an action, which runs `run` instead. */
  to?: string;
  run?: () => void;
  app?: App;
  icon?: IconName;
  /** The heading it is listed under (records are grouped by app). */
  group?: string;
};

/** A record found by `<model>.search`, titled when it is drawn (the titles of refs load later). */
type Found = { app: App; model: ModelMeta; row: Row };

/** How many records one model contributes, and how many the palette shows in all. */
const PER_MODEL = 3;
const MAX_RECORDS = 30;
/** Search fields asked per model: one request each, so the title and its number are enough. */
const FIELDS_PER_MODEL = 2;

/** Models whose records the palette searches: the main and configuration models of each app. */
function searchable(apps: App[]): { app: App; model: ModelMeta; fields: string[] }[] {
  const out: { app: App; model: ModelMeta; fields: string[] }[] = [];
  for (const app of apps) {
    if (app.id === "customize") continue;
    const menu = appMenu(app.module);
    for (const model of [...menu.main, ...menu.configuration]) {
      const fields = searchFieldsOf(model).slice(0, FIELDS_PER_MODEL);
      if (fields.length) out.push({ app, model, fields });
    }
  }
  return out;
}

/**
 * Records whose search fields contain `q`, through the permission-checked `<model>.search` of each
 * searchable model (the same command an agent calls), at most PER_MODEL per model. Answers that
 * arrive for an older query are dropped by the caller.
 */
async function findRecords(token: string, apps: App[], q: string): Promise<Found[]> {
  const targets = searchable(apps);
  const found: Found[] = [];
  // A few requests at a time, in app order, so the first apps answer first.
  const queue = [...targets];
  const worker = async () => {
    for (let next = queue.shift(); next; next = queue.shift()) {
      const { app, model, fields } = next;
      const answers = await Promise.all(
        fields.map((field) =>
          search(token, model.name, {
            filter: [{ field, op: "contains", value: q }],
            limit: PER_MODEL,
          }),
        ),
      );
      const seen = new Set<string>();
      for (const r of answers) {
        if ("error" in r) continue;
        for (const row of r.rows) {
          if (seen.has(row.id) || seen.size >= PER_MODEL) continue;
          seen.add(row.id);
          found.push({ app, model, row });
        }
      }
    }
  };
  await Promise.all(Array.from({ length: 6 }, worker));
  // Back into app order (the workers finish in any order), then the cap.
  const rank = new Map(targets.map((x, i) => [x.model.name, i]));
  found.sort((a, b) => (rank.get(a.model.name) ?? 0) - (rank.get(b.model.name) ?? 0));
  return found.slice(0, MAX_RECORDS);
}

/**
 * The Customize entries of the palette. Each names a place in Customize, not a record type: the
 * generated "Customize › User account" entries all opened the Theme tab (B6).
 */
function customizeHits(): Hit[] {
  const at = (key: string, title: string, model: string): Hit => ({
    key,
    title,
    hint: t("Customize"),
    to: href({ app: "customize", model }),
    icon: "settings",
  });
  return [
    at("cz-people", t("People"), "user_account"),
    at("cz-devices", t("Devices"), "device"),
    at("cz-log", t("Security log"), "security_event"),
    at("cz-security", t("Security"), "security"),
    at("cz-identity", t("Identity"), "identity"),
    at("cz-apps", t("Apps"), "apps"),
  ];
}

export function Palette({
  apps,
  onClose,
  data,
  route,
}: {
  apps: App[];
  onClose: () => void;
  /** The data layer: with it the palette finds records and lists the open record's actions. */
  data?: Data;
  /** Where the person is: the open record's actions are offered first. */
  route?: Route;
}) {
  const [q, setQ] = useState("");
  const [sel, setSel] = useState(0);
  // A Cancel only where it is needed: hidden with CSS instead, it would stay the trap's last stop
  // and a desktop Tab would leave the palette.
  const [cancel] = useState(() => matchesNow(NO_ESCAPE_QUERY));
  const { lang } = useI18n();
  const guideShown = productDocsVisible();
  const token = data?.token ?? "";
  const needle = q.toLowerCase().trim();

  // ---- records found on the server (from two characters, 150 ms after the last key)
  const [found, setFound] = useState<{ q: string; rows: Found[] } | null>(null);
  const [searching, setSearching] = useState(false);
  const seq = useRef(0);
  useEffect(() => {
    if (!token || needle.length < 2) {
      seq.current++;
      setFound(null);
      setSearching(false);
      return;
    }
    const mine = ++seq.current;
    setSearching(true);
    const timer = window.setTimeout(() => {
      void findRecords(token, apps, needle).then((rows) => {
        if (mine !== seq.current) return;
        setFound({ q: needle, rows });
        setSearching(false);
      });
    }, 150);
    return () => window.clearTimeout(timer);
  }, [token, apps, needle]);

  // ---- the open record, for its actions
  const openApp = route ? apps.find((a) => a.id === route.app) : undefined;
  const openModel = openApp?.module.models.find((m) => m.name === route?.model);
  const openId = route?.id && route.id !== "new" ? route.id : "";
  const [openRow, setOpenRow] = useState<Row | null>(null);
  useEffect(() => {
    if (!data || !openModel || !openId) return setOpenRow(null);
    const cached = data.store.cached(openModel.name)?.find((r) => r.id === openId);
    if (cached) return setOpenRow(cached);
    let live = true;
    void call(data.token, `${openModel.name}.get`, { id: openId }).then((r) => {
      if (live && r.kind === "ok" && r.value && typeof r.value === "object")
        setOpenRow(r.value as Row);
    });
    return () => {
      live = false;
    };
    // The row is read once per palette; `data` changes with every load and must not re-ask.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [openModel, openId]);

  // A ref's label from what is loaded, starting the load of its model otherwise: the title is
  // drawn again when it lands. Never the id (presentation.ts titleOf).
  const refText = (model: string, id: string): string => {
    if (!data) return "";
    const rows = data.store.peek(model);
    const row = rows?.find((r) => r.id === id);
    return row ? data.recordLabel(model, row) : "";
  };
  const titled = (app: App, model: ModelMeta, row: Row) =>
    titleOf(model, row, {
      refText,
      moduleId: app.module.id,
      rowOf: (m, id) => data?.store.cached(m)?.find((r) => r.id === id),
    }) || pluralTitle(model, app.module.id);

  const runAction = async (a: ModelMeta["actions"][number], name: string) => {
    if (!data || !openRow) return;
    if (nav.dirty?.()) {
      data.notify(t("You have unsaved changes. Save or discard them first."), "info");
      return;
    }
    if (a.ui?.confirm && !(await data.confirm(a.ui.confirm, name, t("Cancel")))) return;
    const r = await data.run(a.id, { id: openRow.id });
    if (r.kind === "ok") data.notify(t("{name} done", { name }));
    else if (r.kind === "approval")
      data.notify(
        t(
          "Approval needed: “{name}” was proposed (plan {plan}). Nothing changes until a human approves it.",
          { name, plan: r.planId.slice(0, 8) },
        ),
        "info",
      );
    else data.notify(serverMessage(r.message), "error");
  };

  const hits = useMemo<Hit[]>(() => {
    const words = needle.split(/\s+/).filter(Boolean);
    const matches = (h: Hit) => words.every((w) => h.title.toLowerCase().includes(w));

    // Actions of the open record: the next step first, never the dangerous ones.
    const actions: Hit[] = [];
    if (openApp && openModel && openRow) {
      const name = titled(openApp, openModel, openRow);
      const next = nextActions(openModel, openRow);
      for (const a of [next.primary, ...next.secondary]) {
        if (!a || a.ui?.needs?.length || notForRecord(openModel, a, openRow)) continue;
        const label = actionTitle(a);
        actions.push({
          key: `act-${a.id}`,
          title: `${label} · ${name}`,
          hint: t("Action"),
          run: () => void runAction(a, label),
          app: openApp,
          icon: "bolt",
        });
      }
    }

    // Apps, lists, new records and the places in Customize.
    const all: Hit[] = [];
    for (const a of apps) {
      if (a.id === "customize") continue;
      if (onLauncher(a))
        all.push({ key: `app-${a.id}`, title: a.name, hint: t("App"), to: appHref(a), app: a });
      const menu = appMenu(a.module);
      for (const m of [...menu.main, ...menu.configuration, ...menu.reporting]) {
        all.push({
          key: `m-${m.name}`,
          title: `${a.name} › ${pluralTitle(m, a.module.id)}`,
          hint: t("List"),
          to: href({ app: a.id, model: m.name }),
          app: a,
        });
        if (m.can_create && menu.main.includes(m))
          all.push({
            key: `n-${m.name}`,
            title: t("New {model}", { model: inSentence(modelTitle(m, a.module.id)) }),
            hint: t("Create"),
            to: href({ app: a.id, model: m.name, id: "new" }),
            app: a,
          });
      }
    }
    const docs = apps.find((a) => a.id === "documents");
    if (docs)
      all.push({
        key: "drive",
        title: `${docs.name} › ${t("Google Drive")}`,
        hint: t("List"),
        to: href({ app: "documents", model: "drive" }),
      });
    if (apps.some((a) => a.id === "customize")) {
      all.push(
        {
          key: "modules",
          title: t("Modules"),
          hint: t("Customize"),
          to: modulesHref({}),
        },
        {
          key: "modules-market",
          title: t("Install a module from the marketplace"),
          hint: t("Customize"),
          to: modulesHref({ tab: "marketplace" }),
        },
        {
          key: "modules-updates",
          title: t("Module updates"),
          hint: t("Customize"),
          to: modulesHref({ tab: "updates" }),
        },
        ...customizeHits(),
      );
    }
    all.push({
      key: "theme",
      title: t("Change theme"),
      hint: t("Customize"),
      to: href({ app: "customize", model: "theme" }),
    });
    all.push({
      key: "language",
      title: t("Change language and region"),
      hint: t("Customize"),
      to: href({ app: "customize", model: "theme", tab: "theme" }),
    });
    if (guideShown)
      all.push({
        key: "guide",
        title: t("How to write a custom module"),
        hint: t("Guide"),
        to: href({ app: "customize", model: "guide", tab: "guide" }),
      });

    if (!words.length) {
      // Nothing typed: the open record's actions, the records opened lately, then the commands.
      const recent: Hit[] = recentRecords()
        .filter((r) => !(r.model === openModel?.name && r.id === openId))
        .flatMap((r): Hit[] => {
          const app = apps.find((a) => a.id === r.app);
          if (!app || !app.module.models.some((m) => m.name === r.model)) return [];
          return [
            {
              key: `recent-${r.model}-${r.id}`,
              title: r.title,
              hint: pluralTitle(
                app.module.models.find((m) => m.name === r.model)!,
                app.module.id,
              ),
              to: href({ app: r.app, model: r.model, id: r.id }),
              app,
              group: t("Recently opened"),
            },
          ];
        })
        .slice(0, 5);
      return [...actions, ...recent, ...all.slice(0, 12)];
    }

    // Typed: matching actions and commands first, then the records grouped by app.
    const records: Hit[] = (found?.q === needle ? found.rows : []).map(({ app, model, row }) => ({
      key: `r-${model.name}-${row.id}`,
      title: titled(app, model, row),
      hint: pluralTitle(model, app.module.id),
      to: href({ app: app.id, model: model.name, id: row.id }),
      app,
      group: app.name,
    }));
    return [...actions.filter(matches), ...all.filter(matches).slice(0, 8), ...records];
    // `data.version` re-titles found records when the labels of their refs arrive.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [apps, needle, lang, guideShown, found, openRow, openModel, openApp, data?.version]);

  useEffect(() => setSel(0), [q]);
  const go = (h: Hit | undefined) => {
    if (!h) return;
    onClose();
    if (h.run) h.run();
    else if (h.to) location.hash = h.to;
  };
  const waiting = searching && needle.length >= 2;
  return (
    <Dialog bare placement="top" title={t("Command palette")} className="palette" onClose={onClose}>
      <div className="searchfield">
        <Icon name="search" size={16} />
        <input
          data-autofocus
          placeholder={t("Type to search…")}
          aria-label={t("Search apps, records and actions")}
          value={q}
          onChange={(e) => setQ(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "ArrowDown") {
              e.preventDefault();
              setSel((s) => Math.min(s + 1, hits.length - 1));
            }
            if (e.key === "ArrowUp") {
              e.preventDefault();
              setSel((s) => Math.max(s - 1, 0));
            }
            if (e.key === "Enter") go(hits[sel]);
          }}
        />
        {waiting && <Spinner size={16} label={t("Searching records")} />}
        {/* "Esc" means nothing on a phone. */}
        {cancel ? (
          <Button variant="ghost" className="search-cancel" onClick={onClose}>
            {t("Cancel")}
          </Button>
        ) : (
          <Kbd>Esc</Kbd>
        )}
      </div>
      <ul role="listbox" aria-label={t("Results")}>
        {hits.map((h, i) => (
          <PaletteRow
            key={h.key}
            hit={h}
            heading={h.group && h.group !== hits[i - 1]?.group ? h.group : undefined}
            selected={i === sel}
            onHover={() => setSel(i)}
            onPick={() => go(h)}
          />
        ))}
        {hits.length === 0 && (
          <li className="hint empty-li">
            {waiting ? t("Searching records…") : t("Nothing found.")}
          </li>
        )}
      </ul>
    </Dialog>
  );
}

function PaletteRow({
  hit,
  heading,
  selected,
  onHover,
  onPick,
}: {
  hit: Hit;
  heading?: string;
  selected: boolean;
  onHover: () => void;
  onPick: () => void;
}) {
  const ref = useRef<HTMLLIElement>(null);
  // The chosen row stays in view while the arrow keys move through a long list.
  useEffect(() => {
    if (selected) ref.current?.scrollIntoView?.({ block: "nearest" });
  }, [selected]);
  return (
    <>
      {heading && (
        <li role="presentation" className="pal-group overline">
          {heading}
        </li>
      )}
      <li ref={ref} role="option" aria-selected={selected} onMouseEnter={onHover} onClick={onPick}>
        {hit.icon || !hit.app ? (
          <span className="pal-ico">
            <Icon name={hit.icon ?? "settings"} size={14} />
          </span>
        ) : (
          <AppIcon app={hit.app} size={22} />
        )}
        <span className="pal-title">{hit.title}</span>
        <em>{hit.hint}</em>
      </li>
    </>
  );
}
