// Planning: the week plan, where the Shifts page opens (`#/planning/planning_shift`, and
// `?view=week`). A row per employee and an "Open shifts" row on top, a column per day of the week,
// the shifts in their cells; a click on an empty cell opens a new shift for that person and day, a
// click on a shift opens it. On a phone the week is a list by day. "Publish week" runs
// `planning_shift.publish` on each shift of the week that is not published yet, one call per shift,
// as an agent or the command line would (OR-4). The data: `planning_shift.search` for the week and
// `hr_employee.search` for the rows.
import { useEffect, useMemo, useState } from "react";
import { Button, Dialog, IconButton, Segmented, t } from "@obop/ui";
import { search, type Row } from "../../api.ts";
import { useData } from "../../data.tsx";
import type { ModelPageProps } from "../../extensions.ts";
import { formatDate, isoDate, optionLabel } from "../../format.ts";
import { clockText } from "../../inputs/display.ts";
import { useMedia } from "../../media.ts";
import {
  EMPLOYEE,
  NARROW,
  SHIFT,
  WEEK,
  rememberWeek,
  rememberedWeek,
  setNewShift,
} from "./links.ts";
import { addDays, mondayOf, weekDays } from "./week.ts";
import "./planning.css";

const str = (v: unknown) => (typeof v === "string" ? v : "");
const OPEN_ROW = "";

export function WeekBoard(p: ModelPageProps) {
  const d = useData();
  const narrow = useMedia(NARROW);
  const today = isoDate(new Date());
  // The week the person was planning comes back after a shift is opened and closed.
  const [monday, setMondayState] = useState(() => rememberedWeek() ?? mondayOf(today));
  const setMonday = (m: string) => {
    rememberWeek(m);
    setMondayState(m);
  };
  const [asking, setAsking] = useState(false);
  const days = useMemo(() => weekDays(monday), [monday]);
  const [shifts, setShifts] = useState<Row[] | null>(null);
  const [people, setPeople] = useState<Row[]>([]);
  const [error, setError] = useState("");
  const [publishing, setPublishing] = useState(false);
  const stateField = p.model.fields.find((f) => f.name === "state");
  const canCreate = p.model.can_create;
  const publishCmd = p.model.actions.find((c) => c.id === `${SHIFT}.publish`);

  useEffect(() => {
    let live = true;
    setShifts(null);
    void search(d.token, SHIFT, {
      filter: [
        { field: "work_date", op: "gte", value: days[0]! },
        { field: "work_date", op: "lte", value: days[6]! },
      ],
      order: [
        { field: "work_date", dir: "asc" },
        { field: "start_minute", dir: "asc" },
      ],
      limit: 2000,
    }).then((r) => {
      if (!live) return;
      setError("error" in r ? r.error : "");
      setShifts("error" in r ? [] : r.rows);
    });
    return () => {
      live = false;
    };
  }, [d.token, days, d.mutations]);

  useEffect(() => {
    if (!d.models.has(EMPLOYEE)) return;
    let live = true;
    void search(d.token, EMPLOYEE, {
      filter: [{ field: "state", op: "ne", value: "offboarded" }],
      limit: 500,
    }).then((r) => {
      if (live && !("error" in r)) setPeople(r.rows);
    });
    return () => {
      live = false;
    };
  }, [d.token, d.models]);

  // Rows: the open shifts, then every employee who has not left, plus anyone with a shift this week
  // the employee search did not return (a person who left, or one the reader may not list).
  const rows = useMemo(() => {
    const named = new Map<string, string>();
    for (const e of people) named.set(e.id, d.recordLabel(EMPLOYEE, e));
    for (const s of shifts ?? []) {
      const id = str(s.employee_id);
      if (id && !named.has(id)) named.set(id, d.refText(EMPLOYEE, id));
    }
    const list = [...named].sort((a, b) => a[1].localeCompare(b[1]));
    return [[OPEN_ROW, t("Open shifts")] as [string, string], ...list];
  }, [people, shifts, d, d.lang]);

  const byCell = useMemo(() => {
    const m = new Map<string, Row[]>();
    for (const s of shifts ?? []) {
      const k = `${str(s.employee_id)}|${str(s.work_date)}`;
      const l = m.get(k);
      if (l) l.push(s);
      else m.set(k, [s]);
    }
    return m;
  }, [shifts]);

  const unpublished = (shifts ?? []).filter((s) => s.state === "open");
  const openCount = (shifts ?? []).filter((s) => !s.employee_id).length;

  const openShift = (s: Row) => p.go({ app: p.app.id, model: SHIFT, id: s.id, full: "1" });
  const newShift = (employee: string, day: string) => {
    setNewShift({ work_date: day, ...(employee ? { employee_id: employee } : {}) });
    p.go({ app: p.app.id, model: SHIFT, id: "new", full: "1" });
  };

  const publishWeek = async () => {
    setAsking(false);
    const todo = unpublished;
    if (!todo.length) return;
    setPublishing(true);
    let done = 0;
    let failed = "";
    try {
      for (const s of todo) {
        const r = await d.run(`${SHIFT}.publish`, { id: s.id });
        if (r.kind === "ok" || r.kind === "approval") done++;
        else failed ||= r.message;
      }
    } finally {
      setPublishing(false);
    }
    if (failed)
      d.notify(
        t("Published {done} of {count}. {reason}", {
          done,
          count: todo.length,
          reason: failed,
        }),
        "error",
      );
    else
      d.notify(
        t("{count, plural, one {# shift} other {# shifts}} published", { count: done }),
        "ok",
      );
  };

  const chip = (s: Row) => {
    const state = str(s.state);
    const who = str(s.employee_id);
    return (
      <button
        type="button"
        key={s.id}
        className={`pl-shift pl-${state || "open"}`}
        onClick={() => openShift(s)}
        title={optionLabel(state, stateField)}
      >
        <span className="pl-shift-time">
          {clockText(s.start_minute)}–{clockText(s.end_minute)}
        </span>
        <span className="pl-shift-name">{str(s.name)}</span>
        {narrow && (
          <span className="pl-shift-who">{who ? d.refText(EMPLOYEE, who) : t("Open shift")}</span>
        )}
        <span className="pl-shift-state">{optionLabel(state, stateField)}</span>
      </button>
    );
  };

  const weekLabel = `${formatDate(days[0])} – ${formatDate(days[6])}`;
  const dayName = (day: string, style: "short" | "long") =>
    new Date(`${day}T12:00:00`).toLocaleDateString(undefined, { weekday: style });
  const dayNum = (day: string) =>
    new Date(`${day}T12:00:00`).toLocaleDateString(undefined, { day: "numeric", month: "short" });

  return (
    <section className="main panel overview pl-week" aria-labelledby="pl-week-title">
      <div className="content">
        <header className="pl-head">
          <div className="pl-title">
            <h1 id="pl-week-title">{t("Week plan")}</h1>
            <p className="hint">
              {shifts === null
                ? t("Loading")
                : t(
                    "{count, plural, one {# shift} other {# shifts}} · {open} open · {draft} not published",
                    { count: shifts.length, open: openCount, draft: unpublished.length },
                  )}
            </p>
          </div>
          <Segmented
            label={t("View")}
            value="week"
            onChange={(v) => p.go({ app: p.app.id, model: SHIFT, view: v })}
            options={[
              { value: "week", label: t("Week"), icon: "columns" },
              { value: "calendar", label: t("Calendar"), icon: "calendar" },
              { value: "list", label: t("List"), icon: "list" },
            ]}
            iconOnlyNarrow
          />
        </header>
        <div className="pl-bar">
          <div className="pl-nav">
            <IconButton
              icon="chevron-left"
              label={t("Previous week")}
              onClick={() => setMonday(addDays(monday, -7))}
            />
            <Button variant="ghost" onClick={() => setMonday(mondayOf(today))}>
              {t("This week")}
            </Button>
            <IconButton
              icon="chevron-right"
              label={t("Next week")}
              onClick={() => setMonday(addDays(monday, 7))}
            />
            <span className="pl-range">{weekLabel}</span>
          </div>
          {publishCmd && (
            <Button
              variant="primary"
              icon="check"
              loading={publishing}
              disabled={!unpublished.length || publishing}
              onClick={() => setAsking(true)}
            >
              {unpublished.length
                ? t("Publish week ({count})", { count: unpublished.length })
                : t("Week published")}
            </Button>
          )}
        </div>
        {error && <p className="hint">{error}</p>}
        {asking && (
          <Dialog
            role="alertdialog"
            size="sm"
            title={t("Publish week")}
            className="confirm"
            describedBy="pl-publish-msg"
            onClose={() => setAsking(false)}
            footer={
              <>
                <Button data-autofocus onClick={() => setAsking(false)}>
                  {t("Cancel")}
                </Button>
                <Button variant="primary" onClick={() => void publishWeek()}>
                  {t("Publish")}
                </Button>
              </>
            }
          >
            <p id="pl-publish-msg" className="confirm-msg">
              {t(
                "Publish {count, plural, one {# shift} other {# shifts}} of this week? The employees see them and can confirm.",
                { count: unpublished.length },
              )}
            </p>
          </Dialog>
        )}

        {narrow ? (
          <ol className="pl-days">
            {days.map((day) => {
              const list = (shifts ?? []).filter((s) => s.work_date === day);
              return (
                <li key={day} className={day === today ? "is-today" : undefined}>
                  <div className="pl-day-head">
                    <h2>
                      {dayName(day, "long")} <span className="hint">{dayNum(day)}</span>
                    </h2>
                    {canCreate && (
                      <IconButton
                        icon="plus"
                        label={t("Add a shift on {day}", { day: formatDate(day) })}
                        onClick={() => newShift("", day)}
                      />
                    )}
                  </div>
                  {list.length ? (
                    <div className="pl-day-list">{list.map(chip)}</div>
                  ) : (
                    <p className="hint pl-none">{t("No shifts")}</p>
                  )}
                </li>
              );
            })}
          </ol>
        ) : (
          <div className="pl-scroll" role="region" aria-label={weekLabel} tabIndex={0}>
            <table className="pl-grid" aria-busy={shifts === null}>
              <thead>
                <tr>
                  <th scope="col" className="pl-who">
                    {t("Employee")}
                  </th>
                  {days.map((day) => (
                    <th key={day} scope="col" className={day === today ? "is-today" : undefined}>
                      <span className="pl-dname">{dayName(day, "short")}</span>
                      <span className="pl-dnum">{dayNum(day)}</span>
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {rows.map(([id, name]) => (
                  <tr key={id || "open"} className={id ? undefined : "pl-open-row"}>
                    <th scope="row" className="pl-who">
                      {name}
                    </th>
                    {days.map((day) => {
                      const list = byCell.get(`${id}|${day}`) ?? [];
                      return (
                        <td key={day} className={day === today ? "is-today" : undefined}>
                          <div className="pl-cell">
                            {list.map(chip)}
                            {canCreate && (
                              <button
                                type="button"
                                className="pl-add"
                                aria-label={t("Add a shift for {name} on {day}", {
                                  name,
                                  day: formatDate(day),
                                })}
                                onClick={() => newShift(id, day)}
                              >
                                +
                              </button>
                            )}
                          </div>
                        </td>
                      );
                    })}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </section>
  );
}

export const isWeekView = (view: string) => view === "" || view === WEEK;
