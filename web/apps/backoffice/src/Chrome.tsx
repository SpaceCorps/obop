import { useEffect, useMemo, useRef, useState } from "react";
import type { ModuleMeta, Settings } from "./api.ts";
import { appColor } from "./theme.ts";
import { href } from "./route.ts";
import { modelLabel } from "./format.ts";

export type App = { id: string; name: string; color: string; module: ModuleMeta };

/** Apply the instance's launcher layout: order, hidden apps, renamed apps and chosen colours. */
export function buildApps(modules: ModuleMeta[], settings: Settings, isAdmin: boolean): App[] {
  const layout = settings.apps ?? {};
  const all = modules
    .filter((m) => m.id !== "platform")
    .map((m) => ({
      id: m.id,
      name: layout.names?.[m.id] ?? m.name,
      color: appColor(m.id, layout.colors?.[m.id]),
      module: m,
    }));
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
      apps.push({ id: "customize", name: "Customize", color: "#495670", module: platform });
  }
  return apps;
}

export function AppIcon({
  app,
  size = 44,
}: {
  app: { name: string; color: string };
  size?: number;
}) {
  const initials = app.name
    .replace(/[^A-Za-z0-9 ]/g, "")
    .split(" ")
    .map((w) => w[0])
    .join("")
    .slice(0, 2);
  return (
    <span
      className="app-icon"
      style={{ background: app.color, width: size, height: size, fontSize: size * 0.36 }}
      aria-hidden="true"
    >
      {initials}
    </span>
  );
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
}) {
  const { current } = props;
  return (
    <header className="topbar panel">
      <button
        className="icon-btn"
        onClick={props.onLauncher}
        aria-label="Open the app launcher"
        title="Apps"
      >
        <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
          {[4, 11, 18].flatMap((x) =>
            [4, 11, 18].map((y) => (
              <rect
                key={`${x}-${y}`}
                x={x - 2}
                y={y - 2}
                width="4"
                height="4"
                rx="1"
                fill="currentColor"
              />
            )),
          )}
        </svg>
      </button>
      {props.hasMenu && (
        <button
          className="icon-btn menu-btn"
          onClick={props.onMenu}
          aria-label="Toggle the menu"
          aria-expanded={props.menuOpen}
          aria-controls="app-menu"
        >
          <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
            <path
              d="M4 6h16M4 12h16M4 18h16"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
            />
          </svg>
        </button>
      )}
      <a className="brand" href="#/">
        {current ? current.name : "obop"}
      </a>
      <button className="searchpill" onClick={props.onPalette}>
        <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
          <circle cx="11" cy="11" r="6.5" fill="none" stroke="currentColor" strokeWidth="2" />
          <path d="m16 16 4 4" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
        </svg>
        <span>Search apps, records and actions</span>
        <kbd>⌘K</kbd>
      </button>
      <div className="systray">
        <button className="icon-btn" onClick={props.onTheme} aria-label="Theme" title="Theme">
          <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
            <circle cx="12" cy="12" r="4" fill="none" stroke="currentColor" strokeWidth="2" />
            <path
              d="M12 2v3M12 19v3M2 12h3M19 12h3M5 5l2 2M17 17l2 2M19 5l-2 2M7 17l-2 2"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
            />
          </svg>
        </button>
        <UserMenu email={props.email} onSignOut={props.onSignOut} />
      </div>
    </header>
  );
}

function UserMenu({ email, onSignOut }: { email: string; onSignOut: () => void }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const on = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    addEventListener("mousedown", on);
    return () => removeEventListener("mousedown", on);
  }, []);
  return (
    <div className="usermenu" ref={ref}>
      <button
        className="avatar"
        onClick={() => setOpen(!open)}
        aria-haspopup="menu"
        aria-expanded={open}
      >
        {(email[0] ?? "?").toUpperCase()}
      </button>
      {open && (
        <div className="menu panel" role="menu">
          <div className="menu-head">{email || "Signed in"}</div>
          <button role="menuitem" onClick={onSignOut}>
            Sign out
          </button>
        </div>
      )}
    </div>
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
  useEffect(() => {
    const on = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    addEventListener("keydown", on);
    return () => removeEventListener("keydown", on);
  }, [onClose]);
  const first = (a: App) => href({ app: a.id, model: a.module.models[0]?.name });
  return (
    <div className="overlay" onMouseDown={onClose}>
      <div
        className="launcher panel"
        role="dialog"
        aria-label="Apps"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <input
          autoFocus
          placeholder="Search apps"
          value={q}
          onChange={(e) => setQ(e.target.value)}
          aria-label="Search apps"
        />
        {!q && recents.length > 0 && (
          <>
            <h3>Recent</h3>
            <div className="app-grid small">
              {recents.map((a) => (
                <a key={a.id} href={first(a)} onClick={onClose} className="app-tile">
                  <AppIcon app={a} size={36} />
                  <span>{a.name}</span>
                </a>
              ))}
            </div>
          </>
        )}
        <h3>{q ? "Results" : "All apps"}</h3>
        <div className="app-grid">
          {shown.map((a) => (
            <a key={a.id} href={first(a)} onClick={onClose} className="app-tile">
              <AppIcon app={a} size={56} />
              <span>{a.name}</span>
            </a>
          ))}
          {shown.length === 0 && <p className="hint">No app matches.</p>}
        </div>
      </div>
    </div>
  );
}

type Hit = { key: string; title: string; hint: string; to: string };

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
        to: href({ app: a.id, model: a.module.models[0]?.name }),
      });
      for (const m of a.module.models) {
        const l = modelLabel(m.name, a.module.id);
        all.push({
          key: `m-${m.name}`,
          title: `${a.name} › ${l}`,
          hint: "List",
          to: href({ app: a.id, model: m.name }),
        });
        if (m.can_create)
          all.push({
            key: `n-${m.name}`,
            title: `New ${l.toLowerCase()}`,
            hint: "Create",
            to: href({ app: a.id, model: m.name, id: "new" }),
          });
      }
    }
    all.push({
      key: "theme",
      title: "Change theme",
      hint: "Customize",
      to: href({ app: "customize", model: "theme" }),
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
    <div className="overlay top" onMouseDown={onClose}>
      <div
        className="palette panel"
        role="dialog"
        aria-label="Command palette"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <input
          autoFocus
          placeholder="Type to search…"
          aria-label="Search apps, records and actions"
          value={q}
          onChange={(e) => setQ(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") onClose();
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
        <ul role="listbox">
          {hits.map((h, i) => (
            <li
              key={h.key}
              role="option"
              aria-selected={i === sel}
              onMouseEnter={() => setSel(i)}
              onClick={() => go(h)}
            >
              <span>{h.title}</span>
              <em>{h.hint}</em>
            </li>
          ))}
          {hits.length === 0 && <li className="hint">Nothing found.</li>}
        </ul>
      </div>
    </div>
  );
}
