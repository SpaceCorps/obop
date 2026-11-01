import { useEffect, useMemo, useState } from "react";
import {
  AppIcon as TileIcon,
  Avatar,
  Dialog,
  Icon,
  IconButton,
  Kbd,
  LANGS,
  LANG_INFO,
  Logo,
  Menu,
  compareText,
  inSentence,
  t,
  tx,
  useI18n,
  type MenuItem,
} from "@obop/ui";
import type { ModuleMeta, Settings } from "./api.ts";
import { appColor } from "./theme.ts";
import { modulesHref } from "./modules/route.ts";
import { href } from "./route.ts";
import { boardHref } from "./dashboards/route.ts";
import { appTitle, modelTitle } from "./labels.ts";

export type App = { id: string; name: string; color: string; module: ModuleMeta };

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

/** Where an app opens: its first list, or for Dashboards the overview. */
export function appHref(app: App): string {
  return app.id === "dashboards" && app.module.models.some((m) => m.name === "dashboards_dashboard")
    ? boardHref(app.id)
    : href({ app: app.id, model: app.module.models[0]?.name });
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

/** Short description of what an app holds, from the labels of its first two records. */
export function appBlurb(app: App): string {
  if (app.id === "customize") return t("Theme, apps and modules");
  const labels = app.module.models.slice(0, 2).map((m) => modelTitle(m, app.module.id));
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
  const languages: MenuItem[] = [
    { heading: t("Language") },
    ...LANGS.map((l) => ({
      label: LANG_INFO[l].native,
      icon: (i18n.lang === l ? "check" : undefined) as "check" | undefined,
      onSelect: () => void i18n.setLanguage(l),
    })),
  ];
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
      <a className="brand" href="#/">
        {current ? (
          <>
            <Logo variant="mark" size={22} title="" />
            <span className="brand-sep" aria-hidden="true">
              /
            </span>
            <span className="brand-app">{current.name}</span>
          </>
        ) : (
          <>
            <Logo variant="full" size={24} title="" />
            <span className="sr-only">obop</span>
          </>
        )}
      </a>
      <button className="searchpill" onClick={props.onPalette}>
        <Icon name="search" size={16} />
        <span>{t("Search apps, records and actions")}</span>
        <Kbd>⌘K</Kbd>
      </button>
      <div className="systray">
        <IconButton icon="settings" label={t("Theme")} onClick={props.onTheme} />
        <Menu
          ariaLabel={t("Account")}
          align="end"
          triggerClassName="avatar-btn"
          trigger={<Avatar name={props.email || "?"} size={30} />}
          items={[
            { heading: props.email || t("Signed in") },
            { label: t("Theme"), icon: "sun", onSelect: props.onTheme },
            { label: t("Security"), icon: "shield-check", onSelect: props.onSecurity },
            { label: t("Brand book"), icon: "sparkle", onSelect: props.onBrand },
            "separator",
            ...languages,
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
  const shown = apps.filter((a) => a.name.toLowerCase().includes(q.toLowerCase()));
  const recents = recent
    .map((id) => apps.find((a) => a.id === id))
    .filter((a): a is App => !!a)
    .slice(0, 4);
  const first = appHref;
  return (
    <Dialog bare title={t("Apps")} size="lg" className="launcher" onClose={onClose}>
      <div className="searchfield">
        <Icon name="search" size={16} />
        <input
          data-autofocus
          placeholder={t("Search apps")}
          value={q}
          onChange={(e) => setQ(e.target.value)}
          aria-label={t("Search apps")}
        />
      </div>
      <div className="launcher-body">
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

type Hit = { key: string; title: string; hint: string; to: string; app?: App };

export function Palette({ apps, onClose }: { apps: App[]; onClose: () => void }) {
  const [q, setQ] = useState("");
  const [sel, setSel] = useState(0);
  const { lang } = useI18n();
  const hits = useMemo<Hit[]>(() => {
    const all: Hit[] = [];
    for (const a of apps) {
      all.push({
        key: `app-${a.id}`,
        title: a.name,
        hint: t("App"),
        to: appHref(a),
        app: a,
      });
      for (const m of a.module.models) {
        const l = modelTitle(m, a.module.id);
        all.push({
          key: `m-${m.name}`,
          title: `${a.name} › ${l}`,
          hint: t("List"),
          to: href({ app: a.id, model: m.name }),
          app: a,
        });
        if (m.can_create)
          all.push({
            key: `n-${m.name}`,
            title: t("New {model}", { model: inSentence(l) }),
            hint: t("Create"),
            to: href({ app: a.id, model: m.name, id: "new" }),
            app: a,
          });
      }
    }
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
    all.push({
      key: "guide",
      title: t("How to write a custom module"),
      hint: t("Guide"),
      to: href({ app: "customize", model: "guide", tab: "guide" }),
    });
    const needle = q.toLowerCase().trim();
    // Every word has to match somewhere in the title, so "write module" finds "How to write a custom module".
    const words = needle.split(/\s+/).filter(Boolean);
    return all.filter((h) => words.every((w) => h.title.toLowerCase().includes(w))).slice(0, 12);
  }, [apps, q, lang]);
  useEffect(() => setSel(0), [q]);
  const go = (h: Hit | undefined) => {
    if (!h) return;
    location.hash = h.to;
    onClose();
  };
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
        <Kbd>Esc</Kbd>
      </div>
      <ul role="listbox" aria-label={t("Results")}>
        {hits.map((h, i) => (
          <li
            key={h.key}
            role="option"
            aria-selected={i === sel}
            onMouseEnter={() => setSel(i)}
            onClick={() => go(h)}
          >
            {h.app ? (
              <AppIcon app={h.app} size={22} />
            ) : (
              <span className="pal-ico">
                <Icon name="settings" size={14} />
              </span>
            )}
            <span>{h.title}</span>
            <em>{h.hint}</em>
          </li>
        ))}
        {hits.length === 0 && <li className="hint empty-li">{t("Nothing found.")}</li>}
      </ul>
    </Dialog>
  );
}
