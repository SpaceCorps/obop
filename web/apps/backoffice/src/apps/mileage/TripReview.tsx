// The review of a trip (UI plan 3.6), above the form of a Mileage trip: the route drawn from the
// trip's points (an inline drawing: no map tiles, so no position leaves the instance), who drove,
// when, how long and how far, and what the server's check of the track found, in a sentence. The
// points come from `mileage_point.search`, the call an agent makes; Confirm and Reject stay the
// trip's own action buttons.
import { useEffect, useMemo, useState } from "react";
import { Icon, fmt, t } from "@obop/ui";
import { LOAD_LIMIT, getTime, search, type Row } from "../../api.ts";
import type { RecordExtProps } from "../../extensions.ts";
import { useData } from "../../data.tsx";
import { optionLabel } from "../../format.ts";
import { knownZone } from "../../instants.ts";
import { accepted, checkNotes, drawRoute, kmOf, minutesBetween, pointsOf } from "./track.ts";
import "./mileage.css";

const POINT = "mileage_point";
const W = 640;
const H = 300;

type Points = { kind: "loading" } | { kind: "none" } | { kind: "ready"; rows: Row[] };

const inZone = (iso: unknown, o: Intl.DateTimeFormatOptions): string => {
  if (typeof iso !== "string" || !iso) return "";
  const zone = getTime().zone;
  try {
    return new Intl.DateTimeFormat(fmt.locale, {
      ...o,
      timeZone: knownZone(zone) ? zone : "UTC",
    }).format(new Date(iso));
  } catch {
    return "";
  }
};

/** "1 h 44 min", "12 min". */
function duration(min: number): string {
  const h = Math.floor(min / 60);
  const m = min % 60;
  return h
    ? t("{h} h {m} min", { h: fmt.integer(h), m: fmt.integer(m) })
    : t("{m} min", { m: fmt.integer(m) });
}

export function TripReview({ model, row }: RecordExtProps) {
  const d = useData();
  const [points, setPoints] = useState<Points>({ kind: "loading" });
  const state = String(row.state ?? "");

  useEffect(() => {
    let live = true;
    setPoints({ kind: "loading" });
    void search(d.token, POINT, {
      filter: [{ field: "trip", op: "eq", value: row.id }],
      order: [{ field: "seq", dir: "asc" }],
      limit: LOAD_LIMIT,
    }).then((r) => {
      if (!live) return;
      setPoints(
        "error" in r || !r.rows.length ? { kind: "none" } : { kind: "ready", rows: r.rows },
      );
    });
    return () => {
      live = false;
    };
  }, [d.token, row.id]);

  const pts = useMemo(() => (points.kind === "ready" ? pointsOf(points.rows) : []), [points]);
  const drawn = useMemo(() => drawRoute(pts, W, H, 24), [pts]);
  const poor = pts.filter((p) => !accepted(p)).length;
  const km = kmOf(row.distance_m);
  const minutes = minutesBetween(row.started_at, row.ended_at);
  const purposeField = model.fields.find((f) => f.name === "purpose");
  // A private trip is never reviewed, and the server keeps no route of it once it has ended.
  const isPrivate = row.purpose === "private";
  const day = inZone(row.started_at, {
    weekday: "short",
    day: "numeric",
    month: "short",
    year: "numeric",
  });
  const from = inZone(row.started_at, { hour: "numeric", minute: "2-digit" });
  const to = inZone(row.ended_at, { hour: "numeric", minute: "2-digit" });

  // What the check found, as sentences; the counts come from the server (`integrity`), the number
  // of points from the points read here.
  const total = Number(row.point_count ?? pts.length) || pts.length;
  const notes = checkNotes(row.integrity, total, state !== "recording" && km !== null);

  const facts: [string, string][] = [
    [t("Driver"), String(row.driver ?? "")],
    [t("Date"), day],
    [t("Time"), from && to ? `${from} – ${to}` : from],
    [t("Duration"), minutes !== null ? duration(minutes) : ""],
    [t("Purpose"), row.purpose ? optionLabel(String(row.purpose), purposeField) : ""],
    [t("Vehicle"), String(row.vehicle ?? "")],
  ];

  return (
    <section className="trip-review" aria-label={t("Trip review")}>
      <div className="trip-head">
        <div className="trip-km">
          <strong className="tabular">
            {km !== null
              ? fmt.number(km, { minimumFractionDigits: 1, maximumFractionDigits: 1 })
              : "—"}
          </strong>
          <span>{t("km")}</span>
        </div>
        {state === "in_review" && (
          <p className="trip-ask">
            {t("Check the route and the details, then confirm or reject the trip.")}
          </p>
        )}
        {isPrivate && (state === "ended" || state === "rejected") && (
          <p className="trip-ask" data-testid="trip-private-note">
            {t(
              "A private trip is not submitted for review: no reviewer reads it. If the trip was for work, change its purpose.",
            )}
          </p>
        )}
      </div>

      <figure className="trip-map">
        {points.kind === "loading" ? (
          <div className="trip-map-wait" aria-busy="true" aria-label={t("Loading")} />
        ) : drawn.path ? (
          <svg
            viewBox={`0 0 ${W} ${H}`}
            role="img"
            aria-label={t("The route of the trip, from the green start to the end")}
            preserveAspectRatio="xMidYMid meet"
          >
            {drawn.dropped.map(([x, y], i) => (
              <circle key={i} className="trip-dropped" cx={x} cy={y} r={3} />
            ))}
            <path className="trip-line-halo" d={drawn.path} />
            <path className="trip-line" d={drawn.path} />
            {drawn.start && (
              <circle className="trip-start" cx={drawn.start[0]} cy={drawn.start[1]} r={7} />
            )}
            {drawn.end && <circle className="trip-end" cx={drawn.end[0]} cy={drawn.end[1]} r={7} />}
          </svg>
        ) : (
          <p className="trip-map-none">
            <Icon name="info" size={16} />
            {state === "recording"
              ? t("The trip is still being recorded.")
              : isPrivate
                ? t("A private trip keeps no route: only its distance and times.")
                : t("No route: this trip has no points.")}
          </p>
        )}
        {drawn.path && (
          <figcaption className="trip-legend">
            <span>
              <i className="dot trip-start" aria-hidden="true" />
              {t("Start")}
            </span>
            <span>
              <i className="dot trip-end" aria-hidden="true" />
              {t("End")}
            </span>
            {poor > 0 && (
              <span>
                <i className="dot trip-dropped" aria-hidden="true" />
                {t("Left out")}
              </span>
            )}
          </figcaption>
        )}
      </figure>

      <dl className="trip-facts">
        {facts
          .filter(([, v]) => v)
          .map(([k, v]) => (
            <div key={k}>
              <dt>{k}</dt>
              <dd>{v}</dd>
            </div>
          ))}
      </dl>

      {notes.length > 0 && (
        <ul className="trip-notes">
          {notes.map((n) => (
            <li key={n.text} className={`trip-note ${n.tone}`}>
              <Icon name={n.tone === "ok" ? "check-circle" : "warning"} size={16} />
              <span>{n.text}</span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
