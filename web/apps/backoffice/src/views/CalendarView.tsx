import { useMemo, useState } from "react";
import { fmt, inSentence, t } from "@obop/ui";
import type { FieldMeta, Row } from "../api.ts";
import { useData } from "../data.tsx";
import { isoDate, validDate } from "../format.ts";
import { fieldTitle } from "../labels.ts";
import { Icon, toneClass } from "../ui.tsx";
import type { ViewProps } from "./types.ts";

/** First day of the week containing `d` (Monday in most of Europe, Sunday in the US): from the format locale. */
function weekStart(d: Date): Date {
  const x = new Date(d.getFullYear(), d.getMonth(), d.getDate());
  const first = fmt.firstDay() % 7; // 0 = Sunday ... 6 = Saturday
  x.setDate(x.getDate() - ((x.getDay() - first + 7) % 7));
  return x;
}
const addDays = (d: Date, n: number) => new Date(d.getFullYear(), d.getMonth(), d.getDate() + n);

export const dateFields = (fields: FieldMeta[]) => fields.filter((f) => f.kind === "date");

export function CalendarView(p: ViewProps) {
  const d = useData();
  const dfs = dateFields(p.fields);
  const [fname, setFname] = useState(dfs[0]?.name ?? "");
  const df = dfs.find((f) => f.name === fname) ?? dfs[0]!;
  const [mode, setMode] = useState<"month" | "week">(() =>
    matchMedia("(max-width: 40rem)").matches ? "week" : "month",
  );
  const [cursor, setCursor] = useState(() => new Date());
  const today = isoDate(new Date());
  const stateField = p.fields.find(
    (f) => f.kind === "select" && ["state", "stage", "status"].includes(f.name),
  );

  const byDay = useMemo(() => {
    const m = new Map<string, Row[]>();
    for (const r of p.rows) {
      const v = r[df.name];
      if (typeof v === "string" && validDate(v)) {
        const a = m.get(v);
        if (a) a.push(r);
        else m.set(v, [r]);
      }
    }
    return m;
  }, [p.rows, df.name]);
  const undated = p.rows.length - [...byDay.values()].reduce((n, a) => n + a.length, 0);

  const start =
    mode === "month"
      ? weekStart(new Date(cursor.getFullYear(), cursor.getMonth(), 1))
      : weekStart(cursor);
  const days = Array.from({ length: mode === "month" ? 42 : 7 }, (_, i) => addDays(start, i));
  const rowsInView = mode === "month" ? 6 : 1;
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
  const chip = (r: Row) => (
    <button
      key={r.id}
      className={`cal-ev ${stateField ? toneClass(stateField, String(r[stateField.name] ?? "")) : "t-1"}${r.id === p.selectedId ? " sel" : ""}`}
      onClick={() => p.onOpen(r.id)}
      title={d.recordLabel(p.model.name, r)}
    >
      <span className="evdot" aria-hidden="true" />
      <span className="evt">{d.recordLabel(p.model.name, r)}</span>
    </button>
  );

  return (
    <div className="calendar">
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
      {mode === "month" ? (
        <div
          className="monthgrid"
          role="grid"
          aria-label={heading}
          style={{ gridTemplateRows: `auto repeat(${rowsInView}, minmax(5.5rem, 1fr))` }}
        >
          <div role="row" className="dayhead-row">
            {days.slice(0, 7).map((day) => (
              <div key={day.getDay()} role="columnheader" className="dayhead">
                {fmt.dateOf(day, { weekday: "short" })}
              </div>
            ))}
          </div>
          {Array.from({ length: 6 }, (_, w) => (
            <div role="row" className="weekrow" key={w}>
              {days.slice(w * 7, w * 7 + 7).map((day) => {
                const iso = isoDate(day);
                const evs = byDay.get(iso) ?? [];
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
                    {evs.slice(0, 3).map(chip)}
                    {evs.length > 3 && (
                      <button
                        className="more"
                        onClick={() => {
                          setCursor(day);
                          setMode("week");
                        }}
                      >
                        {t("+{count} more", { count: evs.length - 3 })}
                      </button>
                    )}
                  </div>
                );
              })}
            </div>
          ))}
        </div>
      ) : (
        <ul className="weeklist">
          {days.map((day) => {
            const iso = isoDate(day);
            const evs = byDay.get(iso) ?? [];
            return (
              <li key={iso} className={iso === today ? "today" : ""}>
                <div className="wk-day">
                  <strong>{fmt.dateOf(day, { weekday: "short" })}</strong>
                  <span>{fmt.dateOf(day, { day: "numeric", month: "short" })}</span>
                </div>
                <div className="wk-evs">
                  {evs.map(chip)}
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
