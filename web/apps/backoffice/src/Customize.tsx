import { Suspense, lazy, useCallback, useEffect, useId, useState } from "react";
import { call, saveSettings, type ModuleMeta, type Settings, type Theme } from "./api.ts";
import { AppIcon, type App } from "./Chrome.tsx";
import { BuildTab } from "./ModuleBuilder.tsx";
import { href } from "./route.ts";
import { ContrastReport } from "./brand/ContrastReport.tsx";
import {
  ACCENTS,
  appColor,
  applyTheme,
  hasPersonalTheme,
  rememberTheme,
  resolveTheme,
} from "./theme.ts";
import {
  Badge,
  Button,
  Chip,
  Icon,
  Input,
  Notice,
  Segmented,
  Skeleton,
  Switch,
  Tabs,
  panelId,
} from "@obop/ui";

const GuidePage = lazy(() => import("./Guide.tsx").then((m) => ({ default: m.Guide })));
const GUIDE_LINK = href({ app: "customize", model: "guide", tab: "guide" });

const BrandBook = lazy(() =>
  import("./brand/BrandBook.tsx").then((m) => ({ default: m.BrandBook })),
);

type Props = {
  token: string;
  tab: string;
  section: string;
  /** The heading of the guide that the address names. */
  anchor: string;
  apps: App[];
  modules: ModuleMeta[];
  settings: Settings;
  isAdmin: boolean;
  go: (tab: string) => void;
  goSection: (section: string) => void;
  goAnchor: (id: string) => void;
  onSettings: (s: Settings) => void;
  onModulesChanged: () => void;
  notify: (t: string, tone?: "ok" | "error" | "info") => void;
};

const TABS = [
  { id: "theme", label: "Theme", icon: "sun" },
  { id: "apps", label: "Apps", icon: "grid" },
  { id: "modules", label: "Modules", icon: "plug" },
  { id: "build", label: "Build a module", icon: "edit" },
  { id: "brand", label: "Brand book", icon: "sparkle" },
  { id: "guide", label: "Guide", icon: "help" },
] as const;

export function Customize(p: Props) {
  // Everyone gets their own theme and the read-only brand book; the rest changes the instance.
  const tabs = TABS.filter(
    (t) => p.isAdmin || t.id === "theme" || t.id === "brand" || t.id === "guide",
  );
  const tab = tabs.some((t) => t.id === p.tab) ? p.tab : "theme";
  const current = tabs.find((t) => t.id === tab)!;
  return (
    <section className="main panel customize" aria-label="Customize">
      <div className="controlpanel">
        <div className="cp-top">
          <div className="crumbs">
            <span>Customize</span>
            <Icon name="chevron-right" size={12} />
            <strong>{current.label}</strong>
          </div>
        </div>
        <Tabs
          label="Customize"
          idPrefix="cz"
          value={tab as (typeof TABS)[number]["id"]}
          onChange={(id) => p.go(id)}
          tabs={tabs.map((t) => ({ id: t.id, label: t.label, icon: t.icon }))}
        />
      </div>
      <div
        className="content pad"
        role="tabpanel"
        id={panelId("cz")}
        aria-labelledby={`cz-tab-${tab}`}
      >
        {tab === "theme" && <ThemeTab {...p} />}
        {tab === "apps" && <AppsTab {...p} />}
        {tab === "modules" && <ModulesTab {...p} />}
        {tab === "build" && <BuildTab {...p} />}
        {tab === "guide" && (
          <Suspense fallback={<Skeleton rows={6} cols={3} />}>
            <GuidePage anchor={p.anchor} goAnchor={p.goAnchor} notify={p.notify} />
          </Suspense>
        )}
        {tab === "brand" && (
          <Suspense fallback={<Skeleton rows={6} cols={3} />}>
            <BrandBook section={p.section} goSection={p.goSection} notify={p.notify} />
          </Suspense>
        )}
      </div>
    </section>
  );
}

function ThemeTab({ settings, isAdmin, token, onSettings, notify, go }: Props) {
  const [theme, setTheme] = useState(() => resolveTheme(settings.theme));
  const colorId = useId();
  const set = (patch: Theme) => {
    const next = { ...theme, ...patch };
    setTheme(next);
    applyTheme(next); // live: the whole app changes as you pick
  };
  return (
    <div className="two theme-tab">
      <div className="stack">
        <section className="stack" aria-labelledby={`${colorId}-h`}>
          <h3 id={`${colorId}-h`} className="overline">
            Colour
          </h3>
          <div className="swatches">
            {ACCENTS.map((a) => (
              <button
                key={a.value}
                className="swatch"
                style={{ background: a.value }}
                aria-label={a.name}
                title={a.name}
                aria-pressed={theme.accent.toLowerCase() === a.value}
                onClick={() => set({ accent: a.value })}
              />
            ))}
            <label className="swatch custom" title="Custom colour">
              <input
                type="color"
                value={theme.accent}
                onChange={(e) => set({ accent: e.target.value })}
                aria-label="Custom colour"
              />
            </label>
          </div>
        </section>
        <section className="stack">
          <h3 className="overline">Mode</h3>
          <Segmented
            label="Mode"
            value={theme.mode}
            onChange={(mode) => set({ mode })}
            options={[
              { value: "system", label: "System", icon: "settings" },
              { value: "light", label: "Light", icon: "sun" },
              { value: "dark", label: "Dark", icon: "moon" },
            ]}
          />
        </section>
        <section className="stack">
          <h3 className="overline">Density</h3>
          <Segmented
            label="Density"
            value={theme.density}
            onChange={(density) => set({ density })}
            options={[
              { value: "comfortable", label: "Comfortable" },
              { value: "compact", label: "Compact" },
            ]}
          />
        </section>
        <section className="stack">
          <h3 className="overline">Corners</h3>
          <Segmented
            label="Corners"
            value={theme.radius}
            onChange={(radius) => set({ radius })}
            options={[
              { value: "sharp", label: "Sharp" },
              { value: "soft", label: "Soft" },
              { value: "round", label: "Round" },
            ]}
          />
        </section>
        <div className="actionrow">
          <Button variant="primary" onClick={() => (rememberTheme(theme), notify("Saved for you"))}>
            Save for me
          </Button>
          {isAdmin && (
            <Button
              onClick={async () => {
                const next = { ...settings, theme };
                const r = await saveSettings(token, next);
                if (r.kind === "ok") {
                  onSettings(next);
                  rememberTheme(null);
                  notify("Set as the default for everyone");
                } else notify("Could not save", "error");
              }}
            >
              Set for everyone
            </Button>
          )}
          {hasPersonalTheme() && (
            <Button
              variant="ghost"
              onClick={() => {
                rememberTheme(null);
                const t = resolveTheme(settings.theme);
                setTheme(t);
                applyTheme(t);
                notify("Back to the instance default");
              }}
            >
              Reset mine
            </Button>
          )}
        </div>
      </div>
      <div className="stack">
        <div className="preview panel" aria-label="Preview">
          <div className="mini-top">
            <span className="dot" />
            <span className="bar" />
            <span className="mini-av" />
          </div>
          <div className="mini-grid">
            <div className="mini-side" aria-hidden="true">
              <i className="on" />
              <i />
              <i />
              <i />
            </div>
            <div className="mini-body">
              <div className="mini-card">
                <b>Preview</b>
                <Badge tone="accent">open</Badge>
                <Button variant="primary" size="sm">
                  Primary
                </Button>
              </div>
              <div className="mini-card">
                <b>Another card</b>
                <Button size="sm">Secondary</Button>
              </div>
              <div className="mini-card">
                <Chip tone="accent">Filter</Chip>
                <Switch label="Switch" defaultChecked />
              </div>
            </div>
          </div>
        </div>
        <ContrastReport accent={theme.accent} />
        <Notice tone="info" title="Looking for the rules?">
          The Brand book shows every colour, type size, icon and component in the theme you just
          picked.
        </Notice>
        <div>
          <Button variant="ghost" iconEnd="arrow-right" onClick={() => go("brand")}>
            Open the Brand book
          </Button>
        </div>
      </div>
    </div>
  );
}

function AppsTab({ apps, settings, isAdmin, token, onSettings, notify }: Props) {
  const real = apps.filter((a) => a.id !== "customize");
  const layout = settings.apps ?? {};
  const hidden = new Set(layout.hidden ?? []);
  const save = async (next: Settings["apps"]) => {
    const s = { ...settings, apps: next };
    const r = await saveSettings(token, s);
    if (r.kind === "ok") onSettings(s);
    else notify("Could not save", "error");
  };
  const move = (i: number, d: number) => {
    const ids = real.map((a) => a.id);
    const j = i + d;
    if (j < 0 || j >= ids.length) return;
    [ids[i], ids[j]] = [ids[j]!, ids[i]!];
    void save({ ...layout, order: ids });
  };
  if (!isAdmin)
    return (
      <Notice tone="info">Only an administrator can change the app launcher for everyone.</Notice>
    );
  return (
    <div className="stack">
      <p className="hint">
        Order, rename, recolour or hide the apps everyone sees in the launcher.
      </p>
      <ul className="rows">
        {real.map((a, i) => (
          <li key={a.id} className="row">
            <AppIcon app={a} size={36} />
            <input
              defaultValue={a.name}
              aria-label={`Name of ${a.name}`}
              onBlur={(e) =>
                e.target.value !== a.name &&
                void save({ ...layout, names: { ...layout.names, [a.id]: e.target.value } })
              }
            />
            <input
              type="color"
              value={a.color.startsWith("#") ? a.color : "#6d3fd0"}
              aria-label={`Colour of ${a.name}`}
              onChange={(e) =>
                void save({ ...layout, colors: { ...layout.colors, [a.id]: e.target.value } })
              }
            />
            <label className="check">
              <input
                type="checkbox"
                checked={!hidden.has(a.id)}
                onChange={(e) =>
                  void save({
                    ...layout,
                    hidden: e.target.checked
                      ? [...hidden].filter((h) => h !== a.id)
                      : [...hidden, a.id],
                  })
                }
              />
              Shown
            </label>
            <span className="grow" />
            <Button
              variant="ghost"
              size="sm"
              icon="arrow-up"
              onClick={() => move(i, -1)}
              disabled={i === 0}
              aria-label={`Move ${a.name} up`}
            />
            <Button
              variant="ghost"
              size="sm"
              icon="arrow-down"
              onClick={() => move(i, 1)}
              disabled={i === real.length - 1}
              aria-label={`Move ${a.name} down`}
            />
          </li>
        ))}
      </ul>
      <div>
        <Button onClick={() => save({})}>Reset the launcher</Button>
      </div>
    </div>
  );
}

type Pkg = { id: string; name: string; version: string; description: string; source: string };

function ModulesTab({ token, onModulesChanged, notify }: Props) {
  const [installed, setInstalled] = useState<Pkg[]>([]);
  const [market, setMarket] = useState<Pkg[]>([]);
  const refresh = useCallback(async () => {
    const [a, b] = await Promise.all([
      call(token, "module.list"),
      call(token, "marketplace_listing.list"),
    ]);
    if (a.kind === "ok") setInstalled(a.value as Pkg[]);
    if (b.kind === "ok") setMarket(b.value as Pkg[]);
  }, [token]);
  useEffect(() => void refresh(), [refresh]);
  async function act(id: string, body: object, done: string) {
    const r = await call(token, id, body);
    notify(
      r.kind === "ok" ? done : r.kind === "error" ? r.message : "Waiting for a human to approve",
      r.kind === "error" ? "error" : "ok",
    );
    await refresh();
    onModulesChanged();
  }
  const have = new Set(installed.map((p) => p.id));
  return (
    <div className="stack gap-lg">
      <p className="hint">
        A module adds an app: models, fields, permissions and workflow steps, in one text file.{" "}
        <a className="reflink" href={GUIDE_LINK}>
          How to write a custom module?
        </a>
      </p>
      <section className="stack">
        <h3 className="overline">Marketplace</h3>
        <div className="cards3">
          {market.map((p) => (
            <article key={p.id} className="card">
              <AppIcon
                app={{ id: p.id, name: p.name, color: appColor(p.id, undefined, p.name) }}
                size={40}
              />
              <div>
                <strong>{p.name}</strong>
                <p className="hint">{p.description}</p>
              </div>
              <Button
                variant={have.has(p.id) ? "secondary" : "primary"}
                size="sm"
                disabled={have.has(p.id)}
                icon={have.has(p.id) ? "check" : undefined}
                onClick={() =>
                  act("marketplace_listing.install", { id: p.id }, `Installed ${p.name}`)
                }
              >
                {have.has(p.id) ? "Installed" : "Install"}
              </Button>
            </article>
          ))}
        </div>
      </section>
      <section className="stack">
        <h3 className="overline">Installed</h3>
        <ul className="rows">
          {installed.map((p) => (
            <li key={p.id} className="row">
              <strong>{p.name}</strong>
              <span className="hint">{p.description}</span>
              <span className="grow" />
              <Chip>{p.source}</Chip>
              <span className="hint tabular">{p.version}</span>
              {p.source !== "builtin" && (
                <Button
                  size="sm"
                  onClick={() => act("module.uninstall", { id: p.id }, `Removed ${p.name}`)}
                >
                  Uninstall
                </Button>
              )}
            </li>
          ))}
        </ul>
      </section>
      <section className="stack">
        <h3 className="overline">Upload your own</h3>
        <p className="hint">
          An obop.toml file: models, fields with data classes, permission groups and actions. Or
          build one in the next tab.
        </p>
        <Input
          type="file"
          accept=".toml"
          aria-label="Upload an obop.toml file"
          onChange={async (e) => {
            const file = e.target.files?.[0];
            if (file)
              await act(
                "module.install",
                { manifest: await file.text() },
                `Installed ${file.name}`,
              );
            e.target.value = "";
          }}
        />
      </section>
    </div>
  );
}
