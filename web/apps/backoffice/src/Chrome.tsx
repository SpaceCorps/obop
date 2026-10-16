import { useEffect, useMemo, useState } from "react";
import { AppIcon as TileIcon, Avatar, Dialog, Icon, IconButton, Kbd, Logo, Menu } from "@obop/ui";
import type { ModuleMeta, Settings } from "./api.ts";
import { appColor } from "./theme.ts";
import { href } from "./route.ts";
import { boardHref } from "./dashboards/route.ts";
import { modelLabel } from "./format.ts";

export type App = { id: string; name: string; color: string; module: ModuleMeta };

/** Apply the instance's launcher layout: order, hidden apps, renamed apps and chosen colours. */
export function buildApps(modules: ModuleMeta[], settings: Settings, isAdmin: boolean): App[] {
  const layout = settings.apps ?? {};
  const all = modules
    .filter((m) => m.id !== "platform")
    .map((m) => {
      const name = layout.names?.[m.id] ?? m.name;
      return { id: m.id, name, color: appColor(m.id, layout.colors?.[m.id], name), module: m };
    });
  const order = layout.order ?? [];
  all.sort((a, b) => {
    const ia = order.indexOf(a.id);
    const ib = order.indexOf(b.id);
    return (ia < 0 ? 999 : ia) - (ib < 0 ? 999 : ib);
  });
  const hidden = new Set(layout.hidden ?? []);
  const apps = all.filter((a) => !hidden.has(a.id) || isAdmin);
  if (isAdmin) {
    const platform = modules.find((m) => m.id === "platform");
    if (platform)
      apps.push({
        id: "customize",
        name: "Customize",
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
  if (app.id === "customize") return "Theme, apps and modules";
  const labels = app.module.models.slice(0, 2).map((m) => modelLabel(m.name, app.module.id));
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
}) {
  const { current } = props;
  return (
    <header className="topbar">
      <IconButton
        icon="grid"
        label="Open the app launcher"
        onClick={props.onLauncher}
        iconSize={20}
      />
      {props.hasMenu && (
        <IconButton
          className="menu-btn"
          icon="panel-left"
          label="Toggle the menu"
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
        <span>Search apps, records and actions</span>
        <Kbd>⌘K</Kbd>
      </button>
      <div className="systray">
        <IconButton icon="settings" label="Theme" onClick={props.onTheme} />
        <Menu
          ariaLabel="Account"
          align="end"
          triggerClassName="avatar-btn"
          trigger={<Avatar name={props.email || "?"} size={30} />}
          items={[
            { heading: props.email || "Signed in" },
            { label: "Theme", icon: "sun", onSelect: props.onTheme },
            { label: "Brand book", icon: "sparkle", onSelect: props.onBrand },
            "separator",
            { label: "Sign out", icon: "log-out", onSelect: props.onSignOut },
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
    <Dialog bare title="Apps" size="lg" className="launcher" onClose={onClose}>
      <div className="searchfield">
        <Icon name="search" size={16} />
        <input
          data-autofocus
          placeholder="Search apps"
          value={q}
          onChange={(e) => setQ(e.target.value)}
          aria-label="Search apps"
        />
      </div>
      <div className="launcher-body">
        {!q && recents.length > 0 && (
          <section aria-label="Recent apps">
            <h3 className="overline">Recent</h3>
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
        <section aria-label={q ? "Results" : "All apps"}>
          <h3 className="overline">{q ? "Results" : "All apps"}</h3>
          <div className="app-grid">
            {shown.map((a) => (
              <a key={a.id} href={first(a)} onClick={onClose} className="app-tile">
                <AppIcon app={a} size={56} />
                <span>{a.name}</span>
              </a>
            ))}
            {shown.length === 0 && <p className="hint">No app matches.</p>}
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
  const hits = useMemo<Hit[]>(() => {
    const all: Hit[] = [];
    for (const a of apps) {
      all.push({
        key: `app-${a.id}`,
        title: a.name,
        hint: "App",
        to: appHref(a),
        app: a,
      });
      for (const m of a.module.models) {
        const l = modelLabel(m.name, a.module.id);
        all.push({
          key: `m-${m.name}`,
          title: `${a.name} › ${l}`,
          hint: "List",
          to: href({ app: a.id, model: m.name }),
          app: a,
        });
        if (m.can_create)
          all.push({
            key: `n-${m.name}`,
            title: `New ${l.toLowerCase()}`,
            hint: "Create",
            to: href({ app: a.id, model: m.name, id: "new" }),
            app: a,
          });
      }
    }
    all.push({
      key: "theme",
      title: "Change theme",
      hint: "Customize",
      to: href({ app: "customize", model: "theme" }),
    });
    all.push({
      key: "guide",
      title: "How to write a custom module",
      hint: "Guide",
      to: href({ app: "customize", model: "guide", tab: "guide" }),
    });
    const needle = q.toLowerCase().trim();
    return all.filter((h) => !needle || h.title.toLowerCase().includes(needle)).slice(0, 12);
  }, [apps, q]);
  useEffect(() => setSel(0), [q]);
  const go = (h: Hit | undefined) => {
    if (!h) return;
    location.hash = h.to;
    onClose();
  };
  return (
    <Dialog bare placement="top" title="Command palette" className="palette" onClose={onClose}>
      <div className="searchfield">
        <Icon name="search" size={16} />
        <input
          data-autofocus
          placeholder="Type to search…"
          aria-label="Search apps, records and actions"
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
      <ul role="listbox" aria-label="Results">
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
        {hits.length === 0 && <li className="hint empty-li">Nothing found.</li>}
      </ul>
    </Dialog>
  );
}
