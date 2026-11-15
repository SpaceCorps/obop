import { useEffect, useMemo, useState } from "react";
import { fmt, inSentence, t } from "@obop/ui";
import { LOAD_LIMIT, search, type Row, type ServerFilter } from "../api.ts";
import { matches, PHONE_QUERY, useMedia } from "../media.ts";
import { useData } from "../data.tsx";
import { isoDate } from "../format.ts";
import { fieldTitle } from "../labels.ts";
import { toneClassOf } from "../presentation.ts";
import { serverMessage } from "../serverMessage.ts";
import { EmptyState, Icon } from "../ui.tsx";
import { cardTitle, usePresentCtx } from "./BoardCard.tsx";
import { boardLayout } from "./board.ts";
import {
  calendarSpec,
  dateFields,
  defaultStart,
  eventOf,
  hhmm,
  isGone,
  movable,
  movePatch,
  timed,
  weekSegments,
  type CalEvent,
  type CalSpec,
} from "./calendarModel.ts";
import { TimeGrid } from "./TimeGrid.tsx";
import type { ViewProps } from "./types.ts";

export { dateFields } from "./calendarModel.ts";

/** First day of the week containing `d` (Monday in most of Europe, Sunday in the US): from the format locale. */
function weekStart(d: Date): Date {
  const x = new Date(d.getFullYear(), d.getMonth(), d.getDate());
  const first = fmt.firstDay() % 7; // 0 = Sunday ... 6 = Saturday
  x.setDate(x.getDate() - ((x.getDay() - first + 7) % 7));
  return x;
}
const addDays = (d: Date, n: number) => new Date(d.getFullYear(), d.getMonth(), d.getDate() + n);

/** Bars a day of the month shows before "+n more". */
const LANES = 3;

type Fetched = {
  rows: Row[];
  /** Records in the visible range, which may be more than `rows` (the load limit). */
  inRange: number;
  undated: number;
  loaded: boolean;
  error?: string;
};

/**
 * The records of the visible days from the server (D4): the search narrowed to the records that
 * start in the range and, for a range of days (leave, a trip), those that started before it and end
 * in it or after it; plus how many records have no date at all. Asks again when the person moves to
 * another month or after any write.
 */
function useVisibleRecords(
  p: ViewProps,
  spec: CalSpec,
  from: string,
  to: string,
): Fetched & { retry: () => void } {
  const d = useData();
  const server = p.server;
  const [st, setSt] = useState<Fetched>({ rows: [], inRange: 0, undated: 0, loaded: false });
  const [attempt, setAttempt] = useState(0);
  const field = spec.start.name;
  const end = spec.end?.name;
  const key = JSON.stringify([p.model.name, server?.filter, field, end, from, to]);
  useEffect(() => {
    if (!server) return;
    let live = true;
    const base = server.filter;
    const starting: ServerFilter[] = [
      ...base,
      { field, op: "gte", value: from },
      { field, op: "lte", value: to },
    ];
    const spanning: ServerFilter[] | null = end
      ? [...base, { field, op: "lt", value: from }, { field: end, op: "gte", value: from }]
      : null;
    void Promise.all([
      search(d.token, p.model.name, {
        filter: starting,
        order: [{ field, dir: "asc" }],
        limit: LOAD_LIMIT,
      }),
      spanning
        ? search(d.token, p.model.name, { filter: spanning, limit: LOAD_LIMIT })
        : Promise.resolve(null),
      search(d.token, p.model.name, {
        filter: [...base, { field, op: "is_null", value: true }],
        limit: 1,
      }),
    ]).then(([inRange, before, undated]) => {
      if (!live) return;
      if ("error" in inRange) {
        setSt((s) => ({ ...s, loaded: true, error: inRange.error }));
        return;
      }
      const extra = before && !("error" in before) ? before.rows : [];
      const seen = new Set(inRange.rows.map((r) => r.id));
      setSt({
        rows: [...extra.filter((r) => !seen.has(r.id)), ...inRange.rows],
        inRange: inRange.total + (before && !("error" in before) ? before.total : 0),
        undated: "error" in undated ? 0 : undated.total,
        loaded: true,
      });
    });
    return () => {
      live = false;
    };
    // key stands for the model, search, date fields and range
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, d.token, d.mutations, attempt]);
  return { ...st, retry: () => setAttempt((n) => n + 1) };
}

/**
 * The calendar: a month of week rows, where a range of days is one bar across them, and a week,
 * which is a time grid for records with times of day (appointments, shifts, jobs) and a list of
 * days otherwise and on a phone. Events are named like their cards and coloured by their status;
 * records that are cancelled, refused or discarded are hidden until the person shows them. The week
 * starts on the region's first day.
 */
export function CalendarView(p: ViewProps) {
  const d = useData();
  const { model } = p;
  const ctx = usePresentCtx(d.models.get(model.name)?.module.id);
  const dfs = dateFields(p.fields);
  const [fname, setFname] = useState(() => defaultStart(model, p.fields)?.name ?? "");
  const spec = calendarSpec(model, p.fields, fname)!;
  const df = spec.start;
  const phone = useMedia(PHONE_QUERY);
  const [mode, setMode] = useState<"month" | "week">(() =>
    matches(PHONE_QUERY) || timed(calendarSpec(model, p.fields)) ? "week" : "month",
  );
  const [cursor, setCursor] = useState(() => new Date());
  const [showGone, setShowGone] = useState(false);
  const today = isoDate(new Date());
  const layout = useMemo(() => boardLayout(model, p.fields), [model, p.fields]);

  const start =
    mode === "month"
      ? weekStart(new Date(cursor.getFullYear(), cursor.getMonth(), 1))
      : weekStart(cursor);
  const days = Array.from({ length: mode === "month" ? 42 : 7 }, (_, i) => addDays(start, i));
  const isoDays = days.map(isoDate);
  const visible = useVisibleRecords(p, spec, isoDays[0]!, isoDays[isoDays.length - 1]!);
  // A moved event stays where it was dropped until the next answer of the server has it.
  const [moved, setMoved] = useState<Record<string, Record<string, unknown>>>({});
  const loadedRows = p.server ? visible.rows : p.rows;
  useEffect(() => setMoved({}), [loadedRows]);
  const rows = useMemo(
    () =>
      Object.keys(moved).length
        ? loadedRows.map((r) => (moved[r.id] ? { ...r, ...moved[r.id] } : r))
        : loadedRows,
    [loadedRows, moved],
  );

  const events = useMemo(
    () => rows.map((r) => eventOf(spec, r)).filter((e): e is CalEvent => !!e),
    // spec is rebuilt each render from stable inputs
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [rows, df.name, spec.end?.name, spec.startMinute?.name, spec.endMinute?.name],
  );
  const inView = events.filter(
    (e) => e.end >= isoDays[0]! && e.start <= isoDays[isoDays.length - 1]!,
  );
  const goneCount = inView.filter((e) => isGone(model, e.row)).length;
  const shown = showGone ? events : events.filter((e) => !isGone(model, e.row));
  const undated = p.server ? visible.undated : p.rows.length - events.length;

  const label = (e: CalEvent) => cardTitle(model, e.row, layout, ctx);
  const tone = (e: CalEvent) =>
    spec.color ? toneClassOf(spec.color, String(e.row[spec.color.name] ?? ""), model) : "t-accent";
  const text = (e: CalEvent, head = true) =>
    e.startMin !== undefined && head ? `${hhmm(e.startMin)} ${label(e)}` : label(e);

  const heading =
    mode === "month"
      ? fmt.dateOf(cursor, { month: "long", year: "numeric" })
      : t("{from} – {to}", {
          from: fmt.dateOf(days[0]!, { day: "numeric", month: "short" }),
          to: fmt.dateOf(days[6]!, { day: "numeric", month: "short", year: "numeric" }),
        });
  const step = (dir: number) =>
    setCursor(
      mode === "month"
        ? new Date(cursor.getFullYear(), cursor.getMonth() + dir, 1)
        : addDays(cursor, 7 * dir),
    );

  const create = (day: string) => p.canCreate && p.newWith({ [df.name]: day });
  const canMove = p.canUpdate && movable(spec);
  const chip = (e: CalEvent, extra = "", style?: Record<string, string | number>, head = true) => (
    <button
      key={e.row.id}
      className={`cal-ev ${tone(e)}${e.row.id === p.selectedId ? " sel" : ""}${extra}`}
      style={style}
      onClick={() => p.onOpen(e.row.id)}
      title={text(e)}
    >
      <span className="evdot" aria-hidden="true" />
      <span className="evt">{text(e, head)}</span>
    </button>
  );

  if (p.server && visible.error)
    return (
      <EmptyState
        title={t("Could not load the calendar")}
        hint={serverMessage(visible.error)}
        action={{ label: t("Try again"), onClick: visible.retry }}
      />
    );

  const month = () => (
    // Up to tablet width only this box scrolls sideways, not the bar above it (calendar.css).
    <div className="cal-scroll">
      <div
        className="monthgrid"
        role="grid"
        aria-label={heading}
        style={{ gridTemplateRows: `auto repeat(6, minmax(5.75rem, 1fr))` }}
      >
        <div role="row" className="dayhead-row">
          {days.slice(0, 7).map((day) => (
            <div key={day.getDay()} role="columnheader" className="dayhead">
              {fmt.dateOf(day, { weekday: "short" })}
            </div>
          ))}
        </div>
        {Array.from({ length: 6 }, (_, w) => {
          const segs = weekSegments(shown, isoDays[w * 7]!);
          return (
            <div role="row" className="weekrow" key={w}>
              {days.slice(w * 7, w * 7 + 7).map((day, c) => {
                const iso = isoDays[w * 7 + c]!;
                const here = segs.filter((s) => s.from <= c && c <= s.to);
                const lanes = Math.min(LANES, Math.max(0, ...here.map((s) => s.lane + 1)));
                const hidden = here.filter((s) => s.lane >= LANES).length;
                const out = day.getMonth() !== cursor.getMonth();
                return (
                  <div
                    role="gridcell"
                    key={iso}
                    className={`daycell${out ? " out" : ""}${iso === today ? " today" : ""}`}
                    aria-label={fmt.dateOf(day, { dateStyle: "full" })}
                  >
                    <button
                      className="daynum"
                      onClick={() => create(iso)}
                      disabled={!p.canCreate}
                      aria-label={
                        p.canCreate
                          ? t("New record on {date}", {
                              date: fmt.dateOf(day, { dateStyle: "full" }),
                            })
                          : fmt.dateOf(day, { dateStyle: "full" })
                      }
                    >
                      {day.getDate()}
                    </button>
                    {Array.from({ length: lanes }, (_, lane) => {
                      const s = here.find((x) => x.lane === lane);
                      if (!s) return <span key={`g${lane}`} className="cal-gap" />;
                      // A bar is drawn once, in its first day of the week, across the days it lasts.
                      if (s.from !== c) return <span key={`g${lane}`} className="cal-gap" />;
                      const span = s.to - s.from + 1;
                      return chip(
                        s.ev,
                        `${span > 1 ? " span" : ""}${s.head ? "" : " cont-start"}${s.tail ? "" : " cont-end"}`,
                        span > 1 ? { "--span": span } : undefined,
                        s.head,
                      );
                    })}
                    {hidden > 0 && (
                      <button
                        className="more"
                        onClick={() => {
                          setCursor(day);
                          setMode("week");
                        }}
                      >
                        {t("+{count} more", { count: hidden })}
                      </button>
                    )}
                  </div>
                );
              })}
            </div>
          );
        })}
      </div>
    </div>
  );

  const agenda = () => (
    <ul className="weeklist">
      {days.map((day, i) => {
        const iso = isoDays[i]!;
        const evs = shown
          .filter((e) => e.start <= iso && iso <= e.end)
          .sort((a, b) => (a.startMin ?? -1) - (b.startMin ?? -1));
        return (
          <li key={iso} className={iso === today ? "today" : ""}>
            <div className="wk-day">
              <strong>{fmt.dateOf(day, { weekday: "short" })}</strong>
              <span>{fmt.dateOf(day, { day: "numeric", month: "short" })}</span>
            </div>
            <div className="wk-evs">
              {evs.map((e) => chip(e, e.start < iso ? " cont-start" : ""))}
              {evs.length === 0 && <span className="dim">{t("Nothing planned")}</span>}
            </div>
            {p.canCreate && (
              <button
                className="icon-btn"
                aria-label={t("New record on {date}", { date: fmt.date(iso) })}
                onClick={() => create(iso)}
              >
                <Icon name="plus" size={14} />
              </button>
            )}
          </li>
        );
      })}
    </ul>
  );

  const grid = () => (
    <TimeGrid
      days={isoDays}
      events={shown}
      today={today}
      selectedId={p.selectedId}
      label={label}
      tone={tone}
      onOpen={p.onOpen}
      onMove={
        canMove
          ? (e, day, s, end) => {
              const patch = movePatch(spec, day, s, end);
              setMoved((m) => ({ ...m, [e.row.id]: patch }));
              void p.update(e.row.id, patch).then((err) => {
                if (!err) return;
                setMoved(({ [e.row.id]: _, ...rest }) => rest);
                d.notify(err, "error");
              });
            }
          : undefined
      }
      onCreate={p.canCreate ? (day, s, end) => p.newWith(movePatch(spec, day, s, end)) : undefined}
    />
  );

  return (
    <div className="calendar" aria-busy={p.server && !visible.loaded ? true : undefined}>
      <div className="viewbar">
        <div className="calnav" role="group" aria-label={t("Navigate")}>
          <button className="icon-btn" aria-label={t("Previous")} onClick={() => step(-1)}>
            <Icon name="chevron-left" size={16} />
          </button>
          <button className="btn small" onClick={() => setCursor(new Date())}>
            {t("Today")}
          </button>
          <button className="icon-btn" aria-label={t("Next")} onClick={() => step(1)}>
            <Icon name="chevron-right" size={16} />
          </button>
        </div>
        <h2 className="calhead" aria-live="polite">
          {heading}
        </h2>
        <span className="grow" />
        {(goneCount > 0 || showGone) && (
          <label className="check cal-gone">
            <input
              type="checkbox"
              checked={showGone}
              onChange={(e) => setShowGone(e.target.checked)}
            />
            <span>{t("Show cancelled ({count})", { count: goneCount })}</span>
          </label>
        )}
        {dfs.length > 1 && (
          <label className="inline-field">
            <span>{t("Date")}</span>
            <select value={df.name} onChange={(e) => setFname(e.target.value)}>
              {dfs.map((f) => (
                <option key={f.name} value={f.name}>
                  {fieldTitle(f)}
                </option>
              ))}
            </select>
          </label>
        )}
        <div className="switcher" role="group" aria-label={t("Calendar scale")}>
          {(["month", "week"] as const).map((m) => (
            <button key={m} aria-pressed={mode === m} onClick={() => setMode(m)}>
              {m === "month" ? t("Month") : t("Week")}
            </button>
          ))}
        </div>
      </div>
      {mode === "month" ? month() : timed(spec) && !phone ? grid() : agenda()}
      {p.server && visible.inRange > rows.length && (
        <p className="hint pad" role="status">
          {t("Showing the first {shown} of {total} records in this period.", {
            shown: rows.length,
            total: visible.inRange,
          })}
        </p>
      )}
      {undated > 0 && (
        <p className="hint pad">
          {t(
            "{count, plural, one {# record has no {field} and is not shown.} other {# records have no {field} and are not shown.}}",
            { count: undated, field: inSentence(fieldTitle(df)) },
          )}
        </p>
      )}
    </div>
  );
}
