// The generated home of an app (`#/<app>/overview`): what needs the person, counted on the server.
// Every number comes from `<model>.search` with `aggregate` (D4), the command an agent calls, under
// the same access rules; each card opens the list that shows exactly the records it counts.
import { useEffect, useMemo, useState } from "react";
import { Button, Icon, fmt, getCurrency, t, tx, useI18n } from "@obop/ui";
import { aggregate, list, type ModelMeta, type ServerFilter } from "./api.ts";
import { KpiStrip, KpiTile } from "./charts/KpiTile.tsx";
import { AppIcon, appBlurb, type App } from "./Chrome.tsx";
import { useData } from "./data.tsx";
import { optionLabel } from "./format.ts";
import { modelTitle } from "./labels.ts";
import { appMenu, pluralTitle } from "./nav.ts";
import { toneClassOf, toneOf } from "./presentation.ts";
import {
  dueFieldOf,
  mainMoneyOf,
  openStates,
  quickFilters,
  quickState,
  statusOf,
  type QuickFilter,
} from "./quickFilters.ts";
import { encodeState, type Cond } from "./query.ts";
import { currencyGroupOf } from "./views/board.ts";
import { recentRecords } from "./recent.ts";
import { href, type Go, type Route } from "./route.ts";

export type AppOverviewProps = { app: App; route: Route; go: Go };

/** One "what needs me" card. */
type Tile = {
  key: string;
  /** The records it counts (the model's plural name). */
  overline: string;
  label: string;
  count: number;
  /** Money in each currency, already formatted. */
  money?: string;
  /** The badge class of its colour (t-ok, t-warn, t-bad, t-neutral, t-accent, t-1..t-6). */
  tone: string;
  icon?: "warning" | "user" | "clock";
  to: string;
  /** Lower comes first: overdue, then the person's own, then money waiting, then the rest. */
  rank: number;
};

/** How many cards show before "Show all". */
const SHOWN = 8;

/**
 * The main models other main models of the app point at (a project of its tasks, a fiscal year of
 * its periods): containers more than work to do, so their cards come after the others.
 */
function parentsOf(main: readonly ModelMeta[]): Set<string> {
  const names = new Set(main.map((m) => m.name));
  const out = new Set<string>();
  for (const m of main)
    for (const f of m.fields)
      if (f.kind === "ref" && f.to && f.to !== m.name && names.has(f.to)) out.add(f.to);
  return out;
}

/**
 * The record the overview's button makes: the first main model with a status that is not a
 * container and holds money or a number (a journal entry, an order), else one with a status that
 * is not a container, else the first main model.
 */
function primaryModel(main: readonly ModelMeta[]): ModelMeta | undefined {
  const parents = parentsOf(main);
  const able = main.filter((m) => m.can_create && !m.via_action);
  const working = able.filter((m) => statusOf(m) && !parents.has(m.name));
  return (
    working.find((m) => m.fields.some((f) => f.kind === "money" || f.sequence)) ??
    working[0] ??
    able[0]
  );
}

/**
 * How a group's key tells its currency: the record's own `currency`, or, for money whose currency
 * comes from a ref (`ui.currency_from` = "journal_id.currency"), the ref's id looked up in the
 * target's records. Bank lines of a SEK journal are then never added to euros.
 */
async function currencyResolver(
  token: string,
  model: ModelMeta,
  money: ModelMeta["fields"][number] | undefined,
): Promise<{ field?: string; of: (key: Record<string, unknown>) => unknown }> {
  const split = currencyGroupOf(model, money);
  if (!split) return { of: () => undefined };
  if (split.field === "currency") return { field: "currency", of: (k) => k.currency };
  const path =
    money?.ui?.currency_from ??
    model.fields.find((f) => f.kind === "money" && f.ui?.currency_from)?.ui?.currency_from ??
    "";
  const step = path.split(".")[1];
  const byId = new Map<string, unknown>();
  if (split.to && step) {
    const r = await list(token, split.to);
    if (!("error" in r)) for (const row of r.rows) byId.set(row.id, row[step]);
  }
  return { field: split.field, of: (k) => byId.get(String(k[split.field] ?? "")) };
}

/** Money per currency as one line: "€1,200.00 and SEK 4,000.00". */
function moneyLine(by: Map<string, number>): string | undefined {
  const parts = [...by].filter(([, n]) => n !== 0).map(([c, n]) => fmt.money(n, c));
  return parts.length ? fmt.list(parts, "conjunction") : undefined;
}

/** The person signed in (the name the sign-in gave). */
function signedIn(): string {
  try {
    return sessionStorage.getItem("obop-user") ?? "";
  } catch {
    return "";
  }
}

/**
 * The cards of one model: a card per open state with its count and money (records without a
 * currency count in the instance's, never as a currency of their own), then its overdue records,
 * the person's own open ones and low stock. Also the model's total, for the list of records.
 */
async function tilesOf(
  token: string,
  app: App,
  model: ModelMeta,
  user: string,
  parent: boolean,
): Promise<{ tiles: Tile[]; total: number | null }> {
  const later = parent ? 3 : 0;
  const plural = pluralTitle(model, app.module.id);
  const listHref = (facets: Cond[]) =>
    href({ app: app.id, model: model.name, st: encodeState({ facets, group: [] }) });
  const status = statusOf(model);
  const money = mainMoneyOf(model);
  const tiles: Tile[] = [];
  const cur = await currencyResolver(token, model, money);
  const grouped = await aggregate(token, model.name, {
    group_by: status
      ? [{ field: status.name }, ...(money && cur.field ? [{ field: cur.field }] : [])]
      : [],
    measures: money ? [{ op: "count" }, { op: "sum", field: money.name }] : [{ op: "count" }],
  });
  if ("error" in grouped) return { tiles, total: null };
  const instance = getCurrency();
  if (status) {
    for (const state of openStates(model)) {
      const groups = grouped.groups.filter((g) => g.key[status.name] === state);
      const count = groups.reduce((n, g) => n + g.count, 0);
      if (!count) continue;
      const by = new Map<string, number>();
      if (money)
        for (const g of groups) {
          const raw = cur.of(g.key);
          const c = typeof raw === "string" && /^[A-Z]{3}$/.test(raw) ? raw : instance;
          by.set(c, (by.get(c) ?? 0) + (g.measures[`sum:${money.name}`] ?? 0));
        }
      tiles.push({
        key: `${model.name}-${state}`,
        overline: plural,
        label: optionLabel(state, status),
        count,
        money: money ? moneyLine(by) : undefined,
        tone: toneClassOf(status, state, model),
        to: listHref([{ op: "is", f: status.name, v: [state] }]),
        rank: (money && by.size ? 2 : toneOf(status, state, model) === "warn" ? 3 : 4) + later,
      });
    }
  }
  // Overdue, the person's own open records and low stock: one count each, with the list's filter.
  const quick = quickFilters(model, user);
  const pick = (k: QuickFilter["key"]) => quick.find((q) => q.key === k);
  const extra: {
    q: Pick<QuickFilter, "facets" | "filter">;
    id: string;
    label: string;
    tone: string;
    icon: Tile["icon"];
    rank: number;
  }[] = [];
  const overdue = pick("overdue");
  if (overdue && dueFieldOf(model))
    extra.push({
      q: overdue,
      id: "overdue",
      label: tx("filter", "Overdue"),
      tone: "t-bad",
      icon: "clock",
      rank: 0 + later,
    });
  const mine = pick("mine");
  const open = pick("open");
  if (mine)
    extra.push({
      q: open
        ? { facets: [...mine.facets, ...open.facets], filter: [...mine.filter, ...open.filter] }
        : mine,
      id: "mine",
      label: tx("filter", "Mine"),
      tone: "t-accent",
      icon: "user",
      rank: 1,
    });
  const low = model.fields.find((f) => f.name === "below_reorder" && f.kind === "bool");
  if (low)
    extra.push({
      q: {
        facets: [{ op: "bool", f: low.name, v: true }],
        filter: [{ field: low.name, op: "eq", value: true } satisfies ServerFilter],
      },
      id: "low",
      label: t("Low stock"),
      tone: "t-warn",
      icon: "warning",
      rank: 1,
    });
  const counted = await Promise.all(
    extra.map((x) =>
      aggregate(token, model.name, { filter: x.q.filter, measures: [{ op: "count" }] }),
    ),
  );
  counted.forEach((r, i) => {
    const x = extra[i]!;
    if ("error" in r || !r.total) return;
    tiles.push({
      key: `${model.name}-${x.id}`,
      overline: plural,
      label: x.label,
      count: r.total,
      tone: x.tone,
      icon: x.icon,
      to: href({ app: app.id, model: model.name, st: quickState(x.q) }),
      rank: x.rank,
    });
  });
  return { tiles, total: grouped.total };
}

const newHref = (app: App, m: ModelMeta) => href({ app: app.id, model: m.name, id: "new" });

export function AppOverview({ app }: AppOverviewProps) {
  const d = useData();
  const { lang } = useI18n();
  const user = signedIn();
  const menu = appMenu(app.module);
  const main = menu.main;
  const [state, setState] = useState<{
    tiles: Tile[];
    totals: Map<string, number | null>;
    loaded: boolean;
  }>({ tiles: [], totals: new Map(), loaded: false });
  useEffect(() => {
    let live = true;
    const parents = parentsOf(main);
    void Promise.all(main.map((m) => tilesOf(d.token, app, m, user, parents.has(m.name)))).then(
      (all) => {
        if (!live) return;
        // Stable: within a rank the cards keep the order of the models and their states.
        const tiles = all.flatMap((x) => x.tiles);
        tiles.sort((a, b) => a.rank - b.rank);
        setState({
          tiles,
          totals: new Map(all.map((x, i) => [main[i]!.name, x.total])),
          loaded: true,
        });
      },
    );
    return () => {
      live = false;
    };
    // Counted again after any write (d.mutations) and in a new language (the cards' labels).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [d.token, app.module, user, d.mutations, lang]);

  const first = primaryModel(main);
  const [all, setAll] = useState(false);
  const tiles = all ? state.tiles : state.tiles.slice(0, SHOWN);
  const recent = useMemo(
    () =>
      recentRecords()
        .filter((r) => r.app === app.id && app.module.models.some((m) => m.name === r.model))
        .slice(0, 6),
    [app.id, app.module],
  );
  return (
    <section className="main panel overview" aria-label={t("{app} overview", { app: app.name })}>
      <div className="content">
        <header className="ov-head">
          <AppIcon app={app} size={48} />
          <div className="ov-title">
            <h1>{app.name}</h1>
            <p className="hint">{appBlurb(app)}</p>
          </div>
          {first && (
            <div className="ov-actions">
              <Button
                variant="primary"
                icon="plus"
                onClick={() => (location.hash = newHref(app, first))}
              >
                {t("New {model}", { model: modelTitle(first, app.module.id) })}
              </Button>
            </div>
          )}
        </header>

        <section className="ov-section" aria-labelledby="ov-needs">
          <h2 id="ov-needs" className="overline">
            {t("Needs attention")}
          </h2>
          {!state.loaded ? (
            <div className="kpistrip" aria-busy="true" aria-label={t("Loading")}>
              {[0, 1, 2].map((i) => (
                <KpiTile key={i} label="" loading className="ov-kpi" />
              ))}
            </div>
          ) : state.tiles.length ? (
            // The key figures of the charts lane (charts/KpiTile.tsx), with the state's colour as a band.
            <KpiStrip>
              {tiles.map((x) => (
                <KpiTile
                  key={x.key}
                  className={`ov-kpi ${x.tone}`}
                  label={x.label}
                  value={fmt.integer(x.count)}
                  more={x.money ? [x.money] : undefined}
                  caption={x.overline}
                  href={x.to}
                />
              ))}
              {state.tiles.length > SHOWN && (
                <button type="button" className="ov-card ov-more" onClick={() => setAll(!all)}>
                  {all
                    ? t("Show fewer")
                    : t("Show all ({count})", { count: String(state.tiles.length) })}
                </button>
              )}
            </KpiStrip>
          ) : (
            <p className="ov-calm">
              <Icon name="check-circle" size={18} />
              {t("Nothing waits for you here.")}
            </p>
          )}
        </section>

        <section className="ov-section" aria-labelledby="ov-lists">
          <h2 id="ov-lists" className="overline">
            {t("Records")}
          </h2>
          <ul className="ov-lists">
            {[...main, ...menu.reporting].map((m) => {
              const total = state.totals.get(m.name);
              return (
                <li key={m.name}>
                  <a href={href({ app: app.id, model: m.name })}>
                    <span>{pluralTitle(m, app.module.id)}</span>
                    {typeof total === "number" && <em>{fmt.integer(total)}</em>}
                    <Icon name="chevron-right" size={14} />
                  </a>
                </li>
              );
            })}
          </ul>
          {menu.configuration.length > 0 && (
            <p className="ov-config">
              <span className="hint">{t("Configuration")}</span>
              {menu.configuration.map((m) => (
                <a key={m.name} href={href({ app: app.id, model: m.name })}>
                  {pluralTitle(m, app.module.id)}
                </a>
              ))}
            </p>
          )}
        </section>

        {recent.length > 0 && (
          <section className="ov-section" aria-labelledby="ov-recent">
            <h2 id="ov-recent" className="overline">
              {t("Recently opened")}
            </h2>
            <ul className="ov-lists">
              {recent.map((r) => {
                const m = app.module.models.find((x) => x.name === r.model)!;
                return (
                  <li key={`${r.model}-${r.id}`}>
                    <a href={href({ app: app.id, model: r.model, id: r.id })}>
                      <span>{r.title}</span>
                      <em>{modelTitle(m, app.module.id)}</em>
                      <Icon name="chevron-right" size={14} />
                    </a>
                  </li>
                );
              })}
            </ul>
          </section>
        )}
      </div>
    </section>
  );
}
