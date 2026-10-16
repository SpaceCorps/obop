import { useCallback, useEffect, useMemo, useState } from "react";
import { call, saveSettings, type Settings, type Theme } from "./api.ts";
import { AppIcon, type App } from "./Chrome.tsx";
import {
  appColor,
  ACCENTS,
  hasPersonalTheme,
  rememberTheme,
  resolveTheme,
  applyTheme,
} from "./theme.ts";
import { href } from "./route.ts";

type Props = {
  token: string;
  tab: string;
  apps: App[];
  settings: Settings;
  isAdmin: boolean;
  go: (tab: string) => void;
  onSettings: (s: Settings) => void;
  onModulesChanged: () => void;
  notify: (t: string) => void;
};

const TABS = [
  ["theme", "Theme"],
  ["apps", "Apps"],
  ["modules", "Modules"],
  ["build", "Build a module"],
] as const;

export function Customize(p: Props) {
  const tab = TABS.some(([id]) => id === p.tab) ? p.tab : "theme";
  return (
    <section className="main panel customize">
      <div className="controlpanel">
        <div className="crumbs">
          <span>Customize</span>
          <span aria-hidden="true">›</span>
          <strong>{TABS.find(([id]) => id === tab)![1]}</strong>
        </div>
        <div className="switcher" role="tablist" aria-label="Customize">
          {TABS.map(([id, name]) => (
            <button
              key={id}
              role="tab"
              aria-selected={tab === id}
              aria-pressed={tab === id}
              onClick={() => p.go(id)}
            >
              {name}
            </button>
          ))}
        </div>
      </div>
      <div className="content pad">
        {tab === "theme" && <ThemeTab {...p} />}
        {tab === "apps" && <AppsTab {...p} />}
        {tab === "modules" && <ModulesTab {...p} />}
        {tab === "build" && <BuildTab {...p} />}
      </div>
    </section>
  );
}

function Segmented<T extends string>({
  value,
  options,
  onChange,
  name,
}: {
  value: T;
  options: [T, string][];
  onChange: (v: T) => void;
  name: string;
}) {
  return (
    <div className="switcher" role="radiogroup" aria-label={name}>
      {options.map(([v, l]) => (
        <button
          key={v}
          role="radio"
          aria-checked={value === v}
          aria-pressed={value === v}
          onClick={() => onChange(v)}
        >
          {l}
        </button>
      ))}
    </div>
  );
}

function ThemeTab({ settings, isAdmin, token, onSettings, notify }: Props) {
  const [theme, setTheme] = useState(() => resolveTheme(settings.theme));
  const set = (patch: Theme) => {
    const next = { ...theme, ...patch };
    setTheme(next);
    applyTheme(next); // live: the whole app changes as you pick
  };
  return (
    <div className="two">
      <div className="stack">
        <h3>Colour</h3>
        <div className="swatches">
          {ACCENTS.map((a) => (
            <button
              key={a.value}
              className="swatch"
              style={{ background: a.value }}
              aria-label={a.name}
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
        <h3>Mode</h3>
        <Segmented
          name="Mode"
          value={theme.mode}
          onChange={(mode) => set({ mode })}
          options={[
            ["system", "System"],
            ["light", "Light"],
            ["dark", "Dark"],
          ]}
        />
        <h3>Density</h3>
        <Segmented
          name="Density"
          value={theme.density}
          onChange={(density) => set({ density })}
          options={[
            ["comfortable", "Comfortable"],
            ["compact", "Compact"],
          ]}
        />
        <h3>Corners</h3>
        <Segmented
          name="Corners"
          value={theme.radius}
          onChange={(radius) => set({ radius })}
          options={[
            ["sharp", "Sharp"],
            ["soft", "Soft"],
            ["round", "Round"],
          ]}
        />
        <div className="actionrow">
          <button
            className="btn primary"
            onClick={() => (rememberTheme(theme), notify("Saved for you"))}
          >
            Save for me
          </button>
          {isAdmin && (
            <button
              className="btn"
              onClick={async () => {
                const next = { ...settings, theme };
                const r = await saveSettings(token, next);
                if (r.kind === "ok") {
                  onSettings(next);
                  rememberTheme(null);
                  notify("Set as the default for everyone");
                } else notify("Could not save");
              }}
            >
              Set for everyone
            </button>
          )}
          {hasPersonalTheme() && (
            <button
              className="btn"
              onClick={() => {
                rememberTheme(null);
                const t = resolveTheme(settings.theme);
                setTheme(t);
                applyTheme(t);
                notify("Back to the instance default");
              }}
            >
              Reset mine
            </button>
          )}
        </div>
      </div>
      <div className="preview panel" aria-label="Preview">
        <div className="mini-top">
          <span className="dot" />
          <span className="bar" />
        </div>
        <div className="mini-body">
          <div className="mini-card">
            <b>Preview</b>
            <span className="badge">open</span>
            <button className="btn primary">Primary</button>
          </div>
          <div className="mini-card">
            <b>Another card</b>
            <button className="btn">Secondary</button>
          </div>
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
    else notify("Could not save");
  };
  const move = (i: number, d: number) => {
    const ids = real.map((a) => a.id);
    const j = i + d;
    if (j < 0 || j >= ids.length) return;
    [ids[i], ids[j]] = [ids[j]!, ids[i]!];
    void save({ ...layout, order: ids });
  };
  if (!isAdmin)
    return <p className="hint">Only an administrator can change the app launcher for everyone.</p>;
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
            <button
              className="icon-btn"
              onClick={() => move(i, -1)}
              disabled={i === 0}
              aria-label={`Move ${a.name} up`}
            >
              ↑
            </button>
            <button
              className="icon-btn"
              onClick={() => move(i, 1)}
              disabled={i === real.length - 1}
              aria-label={`Move ${a.name} down`}
            >
              ↓
            </button>
          </li>
        ))}
      </ul>
      <button className="btn" onClick={() => save({})}>
        Reset the launcher
      </button>
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
    );
    await refresh();
    onModulesChanged();
  }
  const have = new Set(installed.map((p) => p.id));
  return (
    <div className="stack">
      <h3>Marketplace</h3>
      <div className="cards3">
        {market.map((p) => (
          <article key={p.id} className="card">
            <AppIcon app={{ name: p.name, color: appColor(p.id) }} size={40} />
            <div>
              <strong>{p.name}</strong>
              <p className="hint">{p.description}</p>
            </div>
            <button
              className="btn primary"
              disabled={have.has(p.id)}
              onClick={() =>
                act("marketplace_listing.install", { id: p.id }, `Installed ${p.name}`)
              }
            >
              {have.has(p.id) ? "Installed" : "Install"}
            </button>
          </article>
        ))}
      </div>
      <h3>Installed</h3>
      <ul className="rows">
        {installed.map((p) => (
          <li key={p.id} className="row">
            <strong>{p.name}</strong>
            <span className="hint">{p.description}</span>
            <span className="grow" />
            <span className="chip">{p.source}</span>
            <span className="hint">{p.version}</span>
            {p.source !== "builtin" && (
              <button
                className="btn"
                onClick={() => act("module.uninstall", { id: p.id }, `Removed ${p.name}`)}
              >
                Uninstall
              </button>
            )}
          </li>
        ))}
      </ul>
      <h3>Upload your own</h3>
      <p className="hint">
        An obop.toml file: models, fields with data classes, permission groups and actions. Or build
        one in the next tab.
      </p>
      <input
        type="file"
        accept=".toml"
        onChange={async (e) => {
          const file = e.target.files?.[0];
          if (file)
            await act("module.install", { manifest: await file.text() }, `Installed ${file.name}`);
          e.target.value = "";
        }}
      />
    </div>
  );
}

const slug = (s: string) =>
  s
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "");
const CLASSES = [
  "none",
  "internal",
  "public_directory",
  "confidential_hr",
  "special_category",
  "financial",
];

type DField = { name: string; description: string; class: string; personal: boolean };

export function manifest(
  name: string,
  modelName: string,
  fields: DField[],
  statuses: string[],
): string {
  const id = slug(name);
  const model = slug(modelName) === id ? id : `${id}_${slug(modelName)}`;
  const q = (s: string) => JSON.stringify(s);
  const lines = [
    "[module]",
    `id = ${q(id)}`,
    `version = "0.1.0"`,
    `name = ${q(name.trim())}`,
    `description = ${q(`${name.trim()}: ${modelName.trim()} records`)}`,
    `author = "Built in obop"`,
    "",
    "[[model]]",
    `name = ${q(model)}`,
    `description = ${q(modelName.trim())}`,
    `read_group = ${q(`${id}.user`)}`,
    `write_group = ${q(`${id}.manager`)}`,
  ];
  const all = [...fields];
  if (statuses.length > 1 && !all.some((f) => f.name === "state"))
    all.push({ name: "state", description: statuses.join(", "), class: "none", personal: false });
  for (const f of all) {
    lines.push(
      "",
      "  [[model.field]]",
      `  name = ${q(slug(f.name))}`,
      `  description = ${q(f.description || label(f.name))}`,
      `  class = ${q(f.class)}`,
    );
    if (f.personal) lines.push("  personal_data = true");
  }
  for (let i = 1; i < statuses.length; i++) {
    lines.push(
      "",
      "  [[model.action]]",
      `  name = ${q(`to_${slug(statuses[i]!)}`)}`,
      `  description = ${q(`Move from ${statuses[i - 1]} to ${statuses[i]}`)}`,
      `  field = "state"`,
      `  from = ${q(statuses[i - 1]!)}`,
      `  to = ${q(statuses[i]!)}`,
    );
  }
  return lines.join("\n") + "\n";
}

function label(n: string) {
  const s = n.replace(/_/g, " ");
  return s.charAt(0).toUpperCase() + s.slice(1);
}

function BuildTab({ token, onModulesChanged, notify }: Props) {
  const [name, setName] = useState("Fleet check");
  const [modelName, setModelName] = useState("Inspection");
  const [fields, setFields] = useState<DField[]>([
    { name: "title", description: "What was inspected", class: "none", personal: false },
    { name: "inspector", description: "Who did it", class: "internal", personal: true },
  ]);
  const [statuses, setStatuses] = useState("open, done");
  const [error, setError] = useState<string | null>(null);
  const st = statuses
    .split(",")
    .map((s) => slug(s))
    .filter(Boolean);
  const text = useMemo(
    () =>
      manifest(
        name || "My module",
        modelName || "Record",
        fields.filter((f) => f.name.trim()),
        st,
      ),
    [name, modelName, fields, st],
  );
  const id = slug(name);
  const upd = (i: number, patch: Partial<DField>) =>
    setFields(fields.map((f, j) => (j === i ? { ...f, ...patch } : f)));
  return (
    <div className="two">
      <div className="stack">
        <p className="hint">
          Describe a model and install it as a module. It gets the API, agent tools, permissions and
          privacy rules like every other app.
        </p>
        <label className="field">
          <span className="flabel">Module name</span>
          <input value={name} onChange={(e) => setName(e.target.value)} />
        </label>
        <label className="field">
          <span className="flabel">Record type</span>
          <input value={modelName} onChange={(e) => setModelName(e.target.value)} />
        </label>
        <h3>Fields</h3>
        <ul className="rows">
          {fields.map((f, i) => (
            <li key={i} className="row wrap">
              <input
                value={f.name}
                onChange={(e) => upd(i, { name: e.target.value })}
                placeholder="name"
                aria-label="Field name"
              />
              <input
                value={f.description}
                onChange={(e) => upd(i, { description: e.target.value })}
                placeholder="description"
                aria-label="Field description"
              />
              <select
                value={f.class}
                onChange={(e) => upd(i, { class: e.target.value })}
                aria-label="Data class"
              >
                {CLASSES.map((c) => (
                  <option key={c}>{c}</option>
                ))}
              </select>
              <label className="check">
                <input
                  type="checkbox"
                  checked={f.personal}
                  onChange={(e) => upd(i, { personal: e.target.checked })}
                />
                Personal
              </label>
              <button
                className="icon-btn"
                onClick={() => setFields(fields.filter((_, j) => j !== i))}
                aria-label="Remove field"
              >
                ×
              </button>
            </li>
          ))}
        </ul>
        <button
          className="btn"
          onClick={() =>
            setFields([...fields, { name: "", description: "", class: "none", personal: false }])
          }
        >
          Add field
        </button>
        <label className="field">
          <span className="flabel">Statuses (optional, comma separated)</span>
          <input
            value={statuses}
            onChange={(e) => setStatuses(e.target.value)}
            placeholder="open, in progress, done"
          />
        </label>
        {error && (
          <p role="alert" className="error">
            {error}
          </p>
        )}
        <div className="actionrow">
          <button
            className="btn primary"
            onClick={async () => {
              const r = await call(token, "module.install", { manifest: text });
              if (r.kind === "ok") {
                setError(null);
                notify(`Installed ${name}`);
                onModulesChanged();
                location.hash = href({ app: id });
              } else setError(r.kind === "error" ? r.message : "Waiting for a human to approve");
            }}
          >
            Create and install
          </button>
        </div>
      </div>
      <div className="stack">
        <h3>obop.toml</h3>
        <pre className="code">{text}</pre>
      </div>
    </div>
  );
}
