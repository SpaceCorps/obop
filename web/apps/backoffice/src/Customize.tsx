import { Suspense, lazy, useId, useState } from "react";
import { saveSettings, type ModuleMeta, type Settings, type Theme } from "./api.ts";
import { AppIcon, type App } from "./Chrome.tsx";
import { DetailPanel } from "./modules/DetailPanel.tsx";
import { ModulesPage } from "./modules/ModulesPage.tsx";
import type { ModulesRoute } from "./modules/types.ts";
import { useModulesView } from "./modules/view.ts";
import { BuildTab } from "./ModuleBuilder.tsx";
import { SecurityTab } from "./Security.tsx";
import { UpdatesPage } from "./updates/UpdatesPage.tsx";
import { href } from "./route.ts";
import { ContrastReport } from "./brand/ContrastReport.tsx";
import { ACCENTS, applyTheme, hasPersonalTheme, rememberTheme, resolveTheme } from "./theme.ts";
import {
  Badge,
  Button,
  Chip,
  Icon,
  Notice,
  LanguageSelect,
  RegionSelect,
  Segmented,
  Select,
  Skeleton,
  Switch,
  Tabs,
  fmt,
  isPseudo,
  msg,
  panelId,
  setInstanceDefaults,
  t,
  useI18n,
} from "@obop/ui";

const GuidePage = lazy(() => import("./Guide.tsx").then((m) => ({ default: m.Guide })));
const GUIDE_LINK = href({ app: "customize", model: "guide", tab: "guide" });

const BrandBook = lazy(() =>
  import("./brand/BrandBook.tsx").then((m) => ({ default: m.BrandBook })),
);

type Props = {
  token: string;
  /** Ends this browser's session (after "sign out everywhere"). */
  onSignOut?: () => void;
  tab: string;
  section: string;
  /** The heading of the guide that the address names. */
  anchor: string;
  apps: App[];
  modules: ModuleMeta[];
  settings: Settings;
  isAdmin: boolean;
  /** Who is signed in (the name the sign-in gave). */
  me?: string;
  /** Where the Modules tab is (its list, the open module and the panel tab). */
  modulesRoute: ModulesRoute;
  goModules: (r: Partial<ModulesRoute>) => void;
  go: (tab: string) => void;
  goSection: (section: string) => void;
  goAnchor: (id: string) => void;
  onSettings: (s: Settings) => void;
  onModulesChanged: () => void;
  notify: (t: string, tone?: "ok" | "error" | "info") => void;
};

const TABS = [
  { id: "theme", label: msg("Theme"), icon: "sun" },
  { id: "apps", label: msg("Apps"), icon: "grid" },
  { id: "modules", label: msg("Modules"), icon: "plug" },
  { id: "updates", label: msg("Updates"), icon: "arrow-up" },
  { id: "build", label: msg("Build a module"), icon: "edit" },
  { id: "security", label: msg("Security"), icon: "shield-check" },
  { id: "brand", label: msg("Brand book"), icon: "sparkle" },
  { id: "guide", label: msg("Guide"), icon: "help" },
] as const;

export function Customize(p: Props) {
  // Everyone gets their own theme and the read-only brand book; the rest changes the instance.
  const tabs = TABS.filter(
    (x) =>
      p.isAdmin || x.id === "theme" || x.id === "security" || x.id === "brand" || x.id === "guide",
  );
  const tab = tabs.some((x) => x.id === p.tab) ? p.tab : "theme";
  const current = tabs.find((x) => x.id === tab)!;
  const view = useModulesView({
    token: p.token,
    enabled: tab === "modules" && p.isAdmin,
    meta: p.modules,
    route: p.modulesRoute,
    go: p.goModules,
    notify: p.notify,
    onChanged: p.onModulesChanged,
  });
  return (
    <>
      <section className="main panel customize" aria-label={t("Customize")}>
        <div className="controlpanel">
          <div className="cp-top">
            <div className="crumbs">
              <span>{t("Customize")}</span>
              <Icon name="chevron-right" size={12} />
              <strong>{t(current.label)}</strong>
            </div>
          </div>
          <Tabs
            label={t("Customize")}
            idPrefix="cz"
            value={tab as (typeof TABS)[number]["id"]}
            onChange={(id) => p.go(id)}
            tabs={tabs.map((x) => ({ id: x.id, label: t(x.label), icon: x.icon }))}
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
          {tab === "modules" && (
            <ModulesPage view={view} guideHref={GUIDE_LINK} goBuild={() => p.go("build")} />
          )}
          {tab === "updates" && <UpdatesPage token={p.token} notify={p.notify} />}
          {tab === "build" && <BuildTab {...p} />}
          {tab === "security" && <SecurityTab {...p} />}
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
      {tab === "modules" && view.selected && (
        <DetailPanel
          key={view.selected.row.id}
          row={view.selected.row}
          kind={view.selected.kind}
          panel={p.modulesRoute.panel}
          setPanel={(panel) => p.goModules({ panel })}
          onClose={() => p.goModules({ id: "" })}
          core={view.core}
          title={view.nameOf(view.selected.row.id)}
          description={view.descriptionOf(view.selected.row)}
          nameOf={view.nameOf}
          index={view.rowsById}
          hrefFor={view.hrefFor}
          actions={view.actions}
        />
      )}
    </>
  );
}

const CURRENCIES = [
  "EUR",
  "USD",
  "GBP",
  "CHF",
  "PLN",
  "SEK",
  "DKK",
  "NOK",
  "CZK",
  "HUF",
  "CAD",
  "AUD",
  "JPY",
];

function LanguageSection({ settings, isAdmin, token, onSettings, notify }: Props) {
  const i18n = useI18n();
  const id = useId();
  const [currency, setCurrency] = useState(settings.currency ?? "EUR");
  const names = new Intl.DisplayNames([isPseudo(i18n.lang) ? "en" : i18n.lang], {
    type: "currency",
  });
  return (
    <section className="stack" aria-labelledby={`${id}-h`}>
      <h3 id={`${id}-h`} className="overline">
        {t("Language & region")}
      </h3>
      <LanguageSelect value={i18n.lang} onChange={(l) => void i18n.setLanguage(l)} />
      <RegionSelect
        value={i18n.region}
        onChange={i18n.setRegion}
        hint={t("Numbers, dates and money, independent of the language.")}
      />
      <p className="hint" aria-live="polite">
        {t("Example: {number} · {date} · {money}", {
          number: fmt.number(1234567.89),
          date: fmt.date("2026-03-05"),
          money: fmt.money(123450),
        })}
      </p>
      {isAdmin && (
        <Select
          label={t("Currency of this instance")}
          hint={t("Used for amounts unless a record has a currency of its own.")}
          value={currency}
          onChange={(e) => setCurrency(e.target.value)}
        >
          {CURRENCIES.map((c) => (
            <option key={c} value={c}>
              {c} · {names.of(c)}
            </option>
          ))}
        </Select>
      )}
      <div className="actionrow">
        {i18n.personal && (
          <Button variant="ghost" onClick={() => void i18n.resetLanguage()}>
            {t("Use the instance default")}
          </Button>
        )}
        {isAdmin && (
          <Button
            onClick={async () => {
              const next: Settings = {
                ...settings,
                language: isPseudo(i18n.lang) ? settings.language : i18n.lang,
                currency,
              };
              const r = await saveSettings(token, next);
              if (r.kind === "ok") {
                onSettings(next);
                await setInstanceDefaults({ language: next.language, currency });
                notify(t("Set as the default for everyone"));
              } else notify(t("Could not save"), "error");
            }}
          >
            {t("Set instance default")}
          </Button>
        )}
      </div>
    </section>
  );
}

function ThemeTab(props: Props) {
  const { settings, isAdmin, token, onSettings, notify, go } = props;
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
            {t("Colour")}
          </h3>
          <div className="swatches">
            {ACCENTS.map((a) => (
              <button
                key={a.value}
                className="swatch"
                style={{ background: a.value }}
                aria-label={t(a.name)}
                title={t(a.name)}
                aria-pressed={theme.accent.toLowerCase() === a.value}
                onClick={() => set({ accent: a.value })}
              />
            ))}
            <label className="swatch custom" title={t("Custom colour")}>
              <input
                type="color"
                value={theme.accent}
                onChange={(e) => set({ accent: e.target.value })}
                aria-label={t("Custom colour")}
              />
            </label>
          </div>
        </section>
        <section className="stack">
          <h3 className="overline">{t("Mode")}</h3>
          <Segmented
            label={t("Mode")}
            value={theme.mode}
            onChange={(mode) => set({ mode })}
            options={[
              { value: "system", label: t("System"), icon: "settings" },
              { value: "light", label: t("Light"), icon: "sun" },
              { value: "dark", label: t("Dark"), icon: "moon" },
            ]}
          />
        </section>
        <section className="stack">
          <h3 className="overline">{t("Density")}</h3>
          <Segmented
            label={t("Density")}
            value={theme.density}
            onChange={(density) => set({ density })}
            options={[
              { value: "comfortable", label: t("Comfortable") },
              { value: "compact", label: t("Compact") },
            ]}
          />
        </section>
        <section className="stack">
          <h3 className="overline">{t("Corners")}</h3>
          <Segmented
            label={t("Corners")}
            value={theme.radius}
            onChange={(radius) => set({ radius })}
            options={[
              { value: "sharp", label: t("Sharp") },
              { value: "soft", label: t("Soft") },
              { value: "round", label: t("Round") },
            ]}
          />
        </section>
        <LanguageSection {...props} />
        <div className="actionrow">
          <Button
            variant="primary"
            onClick={() => (rememberTheme(theme), notify(t("Saved for you")))}
          >
            {t("Save for me")}
          </Button>
          {isAdmin && (
            <Button
              onClick={async () => {
                const next = { ...settings, theme };
                const r = await saveSettings(token, next);
                if (r.kind === "ok") {
                  onSettings(next);
                  rememberTheme(null);
                  notify(t("Set as the default for everyone"));
                } else notify(t("Could not save"), "error");
              }}
            >
              {t("Set for everyone")}
            </Button>
          )}
          {hasPersonalTheme() && (
            <Button
              variant="ghost"
              onClick={() => {
                rememberTheme(null);
                const next = resolveTheme(settings.theme);
                setTheme(next);
                applyTheme(next);
                notify(t("Back to the instance default"));
              }}
            >
              {t("Reset mine")}
            </Button>
          )}
        </div>
      </div>
      <div className="stack">
        <div className="preview panel" aria-label={t("Preview")}>
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
                <b>{t("Preview")}</b>
                <Badge tone="accent">{t("open")}</Badge>
                <Button variant="primary" size="sm">
                  {t("Primary")}
                </Button>
              </div>
              <div className="mini-card">
                <b>{t("Another card")}</b>
                <Button size="sm">{t("Secondary")}</Button>
              </div>
              <div className="mini-card">
                <Chip tone="accent">{t("Filter")}</Chip>
                <Switch label={t("Switch")} defaultChecked />
              </div>
            </div>
          </div>
        </div>
        <ContrastReport accent={theme.accent} />
        <Notice tone="info" title={t("Looking for the rules?")}>
          {t(
            "The Brand book shows every colour, type size, icon and component in the theme you just picked.",
          )}
        </Notice>
        <div>
          <Button variant="ghost" iconEnd="arrow-right" onClick={() => go("brand")}>
            {t("Open the Brand book")}
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
    else notify(t("Could not save"), "error");
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
      <Notice tone="info">
        {t("Only an administrator can change the app launcher for everyone.")}
      </Notice>
    );
  return (
    <div className="stack">
      <p className="hint">
        {t("Order, rename, recolour or hide the apps everyone sees in the launcher.")}
      </p>
      <ul className="rows">
        {real.map((a, i) => (
          <li key={a.id} className="row">
            <AppIcon app={a} size={36} />
            <input
              // The field keeps what it was made with, so it is made again when the name changes
              // (another language); else leaving it would save the old name for everyone.
              key={a.name}
              defaultValue={a.name}
              aria-label={t("Name of {name}", { name: a.name })}
              onBlur={(e) =>
                e.target.value !== a.name &&
                void save({ ...layout, names: { ...layout.names, [a.id]: e.target.value } })
              }
            />
            <input
              type="color"
              value={a.color.startsWith("#") ? a.color : "#6d3fd0"}
              aria-label={t("Colour of {name}", { name: a.name })}
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
              {t("Shown")}
            </label>
            <span className="grow" />
            <Button
              variant="ghost"
              size="sm"
              icon="arrow-up"
              onClick={() => move(i, -1)}
              disabled={i === 0}
              aria-label={t("Move {name} up", { name: a.name })}
            />
            <Button
              variant="ghost"
              size="sm"
              icon="arrow-down"
              onClick={() => move(i, 1)}
              disabled={i === real.length - 1}
              aria-label={t("Move {name} down", { name: a.name })}
            />
          </li>
        ))}
      </ul>
      <div>
        <Button onClick={() => save({})}>{t("Reset the launcher")}</Button>
      </div>
    </div>
  );
}
