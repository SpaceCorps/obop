// The Fleet home (`#/fleet/overview`): where the vehicles are (available, on the road, in the
// workshop), the driving licences that expire within 30 days or have expired, and the planned
// services, soonest first; Odoo's fleet reminders for the everyday case. Every figure is a
// `<model>.search` (with `aggregate` for the counts), the call an agent makes, under the same
// access rules: a reader who may not see licence dates gets no licence card.
import { useEffect, useMemo, useState } from "react";
import { Button, Icon, fmt, getCurrency, t } from "@obop/ui";
import { aggregate, search, type Row } from "../../api.ts";
import { KpiStrip, KpiTile } from "../../charts/KpiTile.tsx";
import { AppIcon, appBlurb, type App } from "../../Chrome.tsx";
import { useData } from "../../data.tsx";
import { optionLabel } from "../../format.ts";
import { appMenu, pluralTitle } from "../../nav.ts";
import { encodeState, type Cond } from "../../query.ts";
import { today as todayOf } from "../../quickFilters.ts";
import { href } from "../../route.ts";
import {
  LICENCE_WARN_DAYS,
  addDays,
  daysUntil,
  licenceQuery,
  urgencyOf,
  type Urgency,
} from "./reminders.ts";
import "./fleet.css";

const VEHICLE = "fleet_vehicle";
const DRIVER = "fleet_vehicle_driver";
const SERVICE = "fleet_vehicle_service";
/** How many planned services the card lists. */
const SHOWN = 6;

type Figures = {
  states: Map<string, number>;
  /** Null: the reader may not see licence dates (or drivers). */
  licences: Row[] | null;
  planned: Row[];
  plannedTotal: number;
  /** What done services cost this year, by currency (minor units). */
  spent: number | null;
};

type State =
  | { kind: "loading" }
  | { kind: "error"; message: string }
  | ({ kind: "ready" } & Figures);

export function FleetHome({ app }: { app: App }) {
  const d = useData();
  const [state, setState] = useState<State>({ kind: "loading" });
  const model = (name: string) => app.module.models.find((m) => m.name === name);
  const day = todayOf();

  useEffect(() => {
    let live = true;
    const licenceAsk = licenceQuery(
      app.module.models.find((m) => m.name === DRIVER),
      day,
    );
    void (async () => {
      const [states, licences, planned, spent] = await Promise.all([
        aggregate(d.token, VEHICLE, { group_by: [{ field: "state" }] }),
        licenceAsk ? search(d.token, DRIVER, licenceAsk) : { error: "", status: 403 },
        search(d.token, SERVICE, {
          filter: [{ field: "state", op: "eq", value: "planned" }],
          order: [{ field: "service_on", dir: "asc" }],
          limit: SHOWN,
        }),
        aggregate(d.token, SERVICE, {
          filter: [
            { field: "state", op: "eq", value: "done" },
            { field: "service_on", op: "gte", value: `${day.slice(0, 4)}-01-01` },
          ],
          measures: [{ op: "sum", field: "cost" }],
        }),
      ]);
      if (!live) return;
      if ("error" in states) return setState({ kind: "error", message: states.error });
      setState({
        kind: "ready",
        states: new Map(states.groups.map((g) => [String(g.key.state ?? ""), g.count])),
        licences: "error" in licences ? null : licences.rows,
        planned: "error" in planned ? [] : planned.rows,
        plannedTotal: "error" in planned ? 0 : planned.total,
        spent: "error" in spent ? null : Number(spent.groups[0]?.measures["sum:cost"] ?? 0) || 0,
      });
    })();
    return () => {
      live = false;
    };
  }, [d.token, d.mutations, day, app.module]);

  const menu = useMemo(() => appMenu(app.module), [app.module]);
  const listOf = (name: string, facets: Cond[], view = "") =>
    href({ app: app.id, model: name, st: encodeState({ facets, group: [] }), view });
  const stateField = model(VEHICLE)?.fields.find((f) => f.name === "state");
  const kindField = model(SERVICE)?.fields.find((f) => f.name === "kind");
  const serviceModel = model(SERVICE);
  const vehicleLabel = (id: unknown) => (id ? d.refText(VEHICLE, String(id)) : "");

  const tile = (s: string, tone?: "warn" | "info" | "ok") => (
    <KpiTile
      key={s}
      label={optionLabel(s, stateField)}
      value={state.kind === "ready" ? fmt.integer(state.states.get(s) ?? 0) : undefined}
      loading={state.kind === "loading"}
      caption={t("Vehicles")}
      tone={state.kind === "ready" && (state.states.get(s) ?? 0) > 0 ? tone : undefined}
      href={listOf(VEHICLE, [{ op: "is", f: "state", v: [s] }], "list")}
    />
  );

  return (
    <section
      className="main panel overview fleet-home"
      aria-label={t("{app} overview", { app: app.name })}
    >
      <div className="content">
        <header className="ov-head">
          <AppIcon app={app} size={48} />
          <div className="ov-title">
            <h1>{app.name}</h1>
            <p className="hint">{appBlurb(app)}</p>
          </div>
          {serviceModel?.can_create && (
            <div className="ov-actions">
              <Button
                variant="primary"
                icon="plus"
                onClick={() => (location.hash = href({ app: app.id, model: SERVICE, id: "new" }))}
              >
                {t("Plan a service")}
              </Button>
            </div>
          )}
        </header>

        {state.kind === "error" ? (
          <p className="hint">{state.message}</p>
        ) : (
          <>
            <section className="ov-section" aria-labelledby="fleet-where">
              <h2 id="fleet-where" className="overline">
                {t("Vehicles")}
              </h2>
              <KpiStrip label={t("Vehicles")}>
                {tile("available", "ok")}
                {tile("in_service", "info")}
                {tile("maintenance", "warn")}
                <KpiTile
                  label={t("Service costs this year")}
                  value={
                    state.kind === "ready" && state.spent !== null
                      ? fmt.money(state.spent, getCurrency())
                      : "—"
                  }
                  loading={state.kind === "loading"}
                  caption={t("Done services since 1 January")}
                  href={listOf(SERVICE, [{ op: "is", f: "state", v: ["done"] }], "list")}
                />
              </KpiStrip>
            </section>

            <section className="ov-section" aria-labelledby="fleet-remind">
              <h2 id="fleet-remind" className="overline">
                {t("Reminders")}
              </h2>
              <div className="fleet-cards">
                {(state.kind === "loading" || state.licences !== null) && (
                  <article className="fleet-card" aria-labelledby="fleet-lic">
                    <header>
                      <h3 id="fleet-lic">{t("Licences expiring")}</h3>
                      <a
                        href={listOf(DRIVER, [
                          { op: "is", f: "state", v: ["active"] },
                          {
                            op: "range",
                            f: "licence_expiry",
                            max: addDays(day, LICENCE_WARN_DAYS),
                          },
                        ])}
                      >
                        {t("Drivers")}
                        <Icon name="chevron-right" size={14} />
                      </a>
                    </header>
                    {state.kind === "loading" ? (
                      <Waiting />
                    ) : state.licences!.length === 0 ? (
                      <p className="fleet-calm">
                        <Icon name="check-circle" size={18} />
                        {t("No licence expires in the next {n} days.", { n: LICENCE_WARN_DAYS })}
                      </p>
                    ) : (
                      <ul className="fleet-rows">
                        {state.licences!.map((r) => (
                          <li key={r.id}>
                            <a href={href({ app: app.id, model: DRIVER, id: r.id })}>
                              <span className="fleet-row-main">{String(r.name ?? "")}</span>
                              <When
                                day={String(r.licence_expiry ?? "")}
                                today={day}
                                kind="licence"
                              />
                            </a>
                          </li>
                        ))}
                      </ul>
                    )}
                  </article>
                )}

                <article className="fleet-card" aria-labelledby="fleet-svc">
                  <header>
                    <h3 id="fleet-svc">{t("Planned services")}</h3>
                    <a href={listOf(SERVICE, [{ op: "is", f: "state", v: ["planned"] }], "list")}>
                      {state.kind === "ready" && state.plannedTotal > SHOWN
                        ? t("All {count}", { count: fmt.integer(state.plannedTotal) })
                        : t("Services")}
                      <Icon name="chevron-right" size={14} />
                    </a>
                  </header>
                  {state.kind === "loading" ? (
                    <Waiting />
                  ) : state.planned.length === 0 ? (
                    <p className="fleet-calm">
                      <Icon name="check-circle" size={18} />
                      {t("No service is planned.")}
                    </p>
                  ) : (
                    <ul className="fleet-rows">
                      {state.planned.map((r) => (
                        <li key={r.id}>
                          <a href={href({ app: app.id, model: SERVICE, id: r.id })}>
                            <span className="fleet-row-main">
                              {vehicleLabel(r.vehicle_id)}
                              <span className="fleet-row-sub">
                                {[
                                  optionLabel(String(r.kind ?? ""), kindField),
                                  String(r.workshop ?? ""),
                                ]
                                  .filter(Boolean)
                                  .join(" · ")}
                              </span>
                            </span>
                            <When day={String(r.service_on ?? "")} today={day} kind="service" />
                          </a>
                        </li>
                      ))}
                    </ul>
                  )}
                </article>
              </div>
            </section>
          </>
        )}

        <section className="ov-section" aria-labelledby="fleet-lists">
          <h2 id="fleet-lists" className="overline">
            {t("Records")}
          </h2>
          <ul className="ov-lists">
            {[...menu.main, ...menu.reporting].map((m) => (
              <li key={m.name}>
                <a href={href({ app: app.id, model: m.name })}>
                  <span>{pluralTitle(m, app.module.id)}</span>
                  <Icon name="chevron-right" size={14} />
                </a>
              </li>
            ))}
          </ul>
        </section>
      </div>
    </section>
  );
}

function Waiting() {
  return (
    <div className="fleet-wait" aria-busy="true" aria-label={t("Loading")}>
      <span className="skel" />
      <span className="skel short" />
    </div>
  );
}

/** A date with how close it is: red once past, amber while close. */
function When({ day, today, kind }: { day: string; today: string; kind: "licence" | "service" }) {
  if (!day) return null;
  const u: Urgency = urgencyOf(day, today, kind === "licence" ? LICENCE_WARN_DAYS : 7);
  const n = Math.abs(daysUntil(day, today));
  const date = fmt.date(day, { day: "numeric", month: "short", year: "numeric" });
  const words =
    kind === "licence"
      ? u === "past"
        ? n === 1
          ? t("Expired yesterday")
          : t("Expired {n} days ago", { n: fmt.integer(n) })
        : u === "today"
          ? t("Expires today")
          : n === 1
            ? t("Expires tomorrow")
            : t("Expires in {n} days", { n: fmt.integer(n) })
      : u === "past"
        ? n === 1
          ? t("A day late")
          : t("{n} days late", { n: fmt.integer(n) })
        : u === "today"
          ? t("Today")
          : n === 1
            ? t("Tomorrow")
            : t("In {n} days", { n: fmt.integer(n) });
  return (
    <span className={`fleet-when u-${u}`}>
      <span className="fleet-when-words">{words}</span>
      <span className="fleet-when-date">{date}</span>
    </span>
  );
}
