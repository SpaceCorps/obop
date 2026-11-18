/**
 * The week grid (plan 3.5, Timesheets): one line per task, one column per day of a week, hours in
 * the cells, with day and line totals. A model fits it when it has a ref, a date and minutes shown
 * as a duration (gridModel.ts `gridSpecOf`); the package asks for it with `default_view = "grid"`.
 *
 * Every keystroke that counts is a command (OR-4): typing hours into an empty cell runs
 * `<model>.create`, changing them `<model>.update` (the minutes alone), clearing them
 * `<model>.delete`, and "Submit week" runs the status action from the first option (`submit`) on
 * each draft of the week, one call each. A cell with more than one entry, or one that is no longer
 * a draft, is not typed over: it opens the entry instead.
 *
 * On a phone the week is a strip of seven days with their totals, and the lines of the chosen day
 * are a list with one large box each.
 */
import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { fmt, t } from "@obop/ui";
import { getTime, search, type Command, type Row, type ServerFilter } from "../api.ts";
import { useData } from "../data.tsx";
import { todayIn, addDays } from "../filters.ts";
import { durationClock, durationText } from "../inputs/display.ts";
import { RefPicker } from "../inputs/RefPicker.tsx";
import { actionTitle, fieldTitle } from "../labels.ts";
import { PHONE_QUERY, useMedia } from "../media.ts";
import { serverMessage } from "../serverMessage.ts";
import { Button, Icon, Segmented } from "../ui.tsx";
import {
  buildGrid,
  cellEditable,
  cellPlan,
  daySumText,
  gridSpecOf,
  minutesOf,
  parseCellHours,
  seesOthers,
  submittable,
  weekDays,
  weekStart,
  type GridLine,
  type GridSpec,
} from "./gridModel.ts";
import type { ViewProps } from "./types.ts";
import "./grid.css";

type Committed = "saved" | "same" | "failed";

const me = (): string => {
  try {
    return sessionStorage.getItem("obop-user") ?? "";
  } catch {
    return "";
  }
};

const dayDate = (day: string) => new Date(`${day}T12:00:00Z`);
const weekdayShort = (day: string) =>
  fmt.dateOf(dayDate(day), { weekday: "short", timeZone: "UTC" });
const dayNumber = (day: string) => fmt.dateOf(dayDate(day), { day: "numeric", timeZone: "UTC" });
const dayLong = (day: string) =>
  fmt.dateOf(dayDate(day), { weekday: "long", day: "numeric", month: "long", timeZone: "UTC" });
const hours = (m: number) => (m ? durationText(m) : "");

/** "Nov 9 – 15, 2026": the week as a heading. */
function weekHeading(days: readonly string[]): string {
  const a = dayDate(days[0]!);
  const b = dayDate(days[6]!);
  try {
    return new Intl.DateTimeFormat(fmt.locale, {
      day: "numeric",
      month: "short",
      year: "numeric",
      timeZone: "UTC",
    }).formatRange(a, b);
  } catch {
    return `${days[0]} – ${days[6]}`;
  }
}

/** The status action that moves a draft on (`submit`): from the first option of the status field. */
function submitActionOf(actions: readonly Command[], spec: GridSpec): Command | undefined {
  if (!spec.state) return undefined;
  const first = spec.state.options![0];
  const from = actions.filter((a) => a.field === spec.state!.name && a.from === first);
  return from.find((a) => a.ui?.primary) ?? from[0];
}

export function GridView(p: ViewProps) {
  const d = useData();
  const spec = useMemo(() => gridSpecOf(p.model, p.fields), [p.model, p.fields]);
  const phone = useMedia(PHONE_QUERY);
  const user = me();
  const today = todayIn(getTime().zone);
  const [monday, setMonday] = useState(() => weekStart(today));
  const days = useMemo(() => weekDays(monday), [monday]);
  const [picked, setPicked] = useState(today);
  const pickedDay = days.includes(picked) ? picked : days[0]!;
  // Only the signed-in person's entries, unless they choose to see everyone's (a manager).
  const [scope, setScope] = useState<"mine" | "all">("mine");
  const [extra, setExtra] = useState<{ ref: string; owner: string }[]>([]);
  const [adding, setAdding] = useState("");
  const [busy, setBusy] = useState(false);
  const [loaded, setLoaded] = useState<{ key: string; rows: Row[] } | null>(null);
  // Below 360 px the strip of days scrolls inside itself: keep the chosen day in view.
  const strip = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const s = strip.current;
    const tab = s?.querySelector<HTMLElement>('[aria-selected="true"]');
    if (s && tab && s.scrollWidth > s.clientWidth)
      s.scrollLeft = tab.offsetLeft - (s.clientWidth - tab.offsetWidth) / 2;
  }, [pickedDay, phone]);

  const mine = scope === "mine" && !!spec?.owner && !!user;
  const filter = useMemo<ServerFilter[] | null>(() => {
    if (!spec || !p.server) return null;
    const f: ServerFilter[] = [
      ...p.server.filter,
      { field: spec.day.name, op: "gte", value: days[0] },
      { field: spec.day.name, op: "lte", value: days[6] },
    ];
    if (mine) f.push({ field: spec.owner!.name, op: "eq", value: user });
    return f;
  }, [spec, p.server, days, mine, user]);
  const key = JSON.stringify(filter);

  useEffect(() => {
    if (!filter) return;
    let live = true;
    void search(d.token, p.model.name, { filter, limit: 1000 }).then((r) => {
      if (!live) return;
      if ("error" in r) d.notify(serverMessage(r.error), "error");
      else setLoaded({ key, rows: r.rows });
    });
    return () => {
      live = false;
    };
    // d.mutations: ask again after every write, ours or another screen's.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, d.token, d.mutations, p.model.name]);

  const rows = useMemo<Row[]>(() => {
    if (!spec) return [];
    if (filter) return loaded?.key === key ? loaded.rows : [];
    // The browser holds the records (a search the server cannot answer): narrow them here.
    return p.rows.filter((r) => !mine || r[spec.owner!.name] === user);
  }, [spec, filter, loaded, key, p.rows, mine, user]);
  const ready = !filter || loaded?.key === key;

  const owner = mine || !spec?.owner ? user : "";
  const grid = useMemo(
    () =>
      spec
        ? buildGrid(
            rows,
            spec,
            days,
            extra.filter((x) => !spec.owner || !mine || x.owner === user),
          )
        : null,
    [rows, spec, days, extra, mine, user],
  );
  const submit = spec ? submitActionOf(p.model.actions, spec) : undefined;
  // Each person submits their own week: a manager who looks at everyone's submits only theirs.
  const drafts =
    grid && spec
      ? submittable(grid, spec).filter((r) => !spec.owner || r[spec.owner.name] === user)
      : [];

  if (!spec || !grid) {
    return (
      <div className="tsgrid-empty">
        <p className="hint">
          {t("These records have no task, day and time to lay out as a week.")}
        </p>
      </div>
    );
  }

  const lineTitle = (l: GridLine) => d.refText(spec.line.to ?? "", l.ref);
  // What the line's record belongs to, under its name: the project of a task (its first ref).
  const lineModel = spec.line.to ? d.models.get(spec.line.to)?.model : undefined;
  const parent = lineModel?.fields.find((f) => f.kind === "ref" && f.to && f.to !== p.model.name);
  const parentLine = (l: GridLine) => {
    if (!parent?.to || !spec.line.to) return null;
    const id = d.store.peek(spec.line.to)?.find((r) => r.id === l.ref)?.[parent.name];
    return typeof id === "string" && id ? (
      <span className="tsgrid-parent">{d.refText(parent.to, id)}</span>
    ) : null;
  };

  /** Type `text` into the cell of line `l` on `day`. */
  const commit = async (l: GridLine, day: string, text: string): Promise<Committed> => {
    const typed = text.trim();
    const minutes = typed === "" ? null : parseCellHours(typed);
    if (typed !== "" && (minutes === null || minutes > 1440)) {
      d.notify(t("Type hours as 8, 1.5 or 1:30, at most 24."), "error");
      return "failed";
    }
    const plan = cellPlan(l.cells[day] ?? [], minutes, spec);
    if (plan.kind === "none") return "same";
    if (plan.kind === "locked") return "failed";
    setBusy(true);
    const m = p.model.name;
    const r =
      plan.kind === "create"
        ? await d.run(`${m}.create`, {
            [spec.line.name]: l.ref,
            [spec.day.name]: day,
            [spec.value.name]: plan.minutes,
            ...(spec.owner && l.owner && l.owner !== user ? { [spec.owner.name]: l.owner } : {}),
          })
        : plan.kind === "update"
          ? await d.run(`${m}.update`, { id: plan.id, [spec.value.name]: plan.minutes })
          : await d.run(`${m}.delete`, { id: plan.id });
    setBusy(false);
    if (r.kind === "error") {
      d.notify(serverMessage(r.message), "error");
      return "failed";
    }
    if (r.kind === "approval") {
      d.notify(t("Approval needed: nothing changes until a human approves it."), "info");
      return "failed";
    }
    return "saved";
  };

  const submitWeek = async () => {
    if (!submit || !drafts.length) return;
    setBusy(true);
    let done = 0;
    let failed = "";
    for (const r of drafts) {
      const out = await d.run(submit.id, { id: r.id });
      if (out.kind === "error") failed ||= serverMessage(out.message);
      else done += 1;
    }
    setBusy(false);
    if (failed) d.notify(failed, "error");
    if (done)
      d.notify(
        t("{count, plural, one {# entry submitted} other {# entries submitted}}", { count: done }),
        "ok",
      );
  };

  const addLine = (ref: string) => {
    setAdding("");
    if (!ref) return;
    setExtra((x) =>
      x.some((y) => y.ref === ref && y.owner === owner) ? x : [...x, { ref, owner }],
    );
  };

  // Only for a person who can see others' entries (a manager); for the rest Everyone is Mine.
  const scopeSwitch =
    spec.owner && seesOthers(p.model) ? (
      <Segmented
        label={t("Whose time")}
        value={scope}
        onChange={setScope}
        options={[
          { value: "mine", label: t("Mine"), icon: "user" },
          { value: "all", label: t("Everyone"), icon: "users" },
        ]}
      />
    ) : null;

  const submitButton =
    submit && p.canUpdate ? (
      <Button
        variant={drafts.length ? "primary" : "secondary"}
        icon="check"
        disabled={busy || drafts.length === 0}
        onClick={() => void submitWeek()}
      >
        {drafts.length
          ? t("{action} week ({count})", {
              action: actionTitle(submit),
              count: String(drafts.length),
            })
          : t("Nothing to submit")}
      </Button>
    ) : null;

  const adder = p.canCreate ? (
    <div className="tsgrid-add">
      <span className="tsgrid-add-label">
        <Icon name="plus" size={16} />
        {t("Add a line")}
      </span>
      <div className="tsgrid-add-pick">
        <RefPicker
          field={spec.line}
          value={adding}
          onChange={(v) => addLine(String(v ?? ""))}
          compact
        />
      </div>
    </div>
  ) : null;

  const nav = (
    <div className="viewbar tsgrid-bar">
      <div className="calnav" role="group" aria-label={t("Navigate")}>
        <button
          className="icon-btn"
          aria-label={t("Previous week")}
          onClick={() => setMonday(addDays(monday, -7))}
        >
          <Icon name="chevron-left" size={16} />
        </button>
        <button
          className="btn small"
          onClick={() => {
            setMonday(weekStart(today));
            setPicked(today);
          }}
        >
          {t("This week")}
        </button>
        <button
          className="icon-btn"
          aria-label={t("Next week")}
          onClick={() => setMonday(addDays(monday, 7))}
        >
          <Icon name="chevron-right" size={16} />
        </button>
      </div>
      <h2 className="calhead" aria-live="polite">
        {weekHeading(days)}
      </h2>
      <span className="grow" />
      {scopeSwitch}
      {!phone && submitButton}
    </div>
  );

  const ownerLine = (l: GridLine) =>
    spec.owner && (!mine || l.owner !== user) && l.owner ? (
      <span className="tsgrid-owner">{l.owner}</span>
    ) : null;

  const empty = grid.lines.length === 0 && (
    <p className="tsgrid-none hint">
      {ready ? t("No time recorded this week yet. Add a line to start.") : t("Loading…")}
    </p>
  );

  if (phone) {
    const lines = grid.lines;
    return (
      <div className="tsgrid tsgrid-phone" aria-busy={!ready || busy || undefined}>
        {nav}
        <div ref={strip} className="tsgrid-days" role="tablist" aria-label={t("Days of the week")}>
          {days.map((day) => (
            <button
              key={day}
              role="tab"
              aria-selected={day === pickedDay}
              className={`tsgrid-day${day === today ? " today" : ""}`}
              onClick={() => setPicked(day)}
            >
              <span className="tsgrid-dname">{weekdayShort(day)}</span>
              <span className="tsgrid-dnum">{dayNumber(day)}</span>
              <span className="tsgrid-dsum num">{daySumText(grid.dayTotals[day] ?? 0) || "·"}</span>
            </button>
          ))}
        </div>
        <div className="tsgrid-dayhead">
          <h3>{dayLong(pickedDay)}</h3>
          <span className="num">{durationText(grid.dayTotals[pickedDay] ?? 0)}</span>
        </div>
        {empty}
        <ul className="tsgrid-list">
          {lines.map((l) => (
            <li key={l.key} className="tsgrid-item">
              <div className="tsgrid-item-text">
                <span className="tsgrid-task">{lineTitle(l)}</span>
                {parentLine(l)}
                {ownerLine(l)}
              </div>
              <Cell
                line={l}
                day={pickedDay}
                spec={spec}
                label={t("{task} on {day}", { task: lineTitle(l), day: dayLong(pickedDay) })}
                disabled={!p.canUpdate}
                commit={commit}
                onOpen={p.onOpen}
              />
            </li>
          ))}
        </ul>
        {adder}
        <div className="tsgrid-foot">
          <span>
            {t("Week")} <strong className="num">{durationText(grid.total)}</strong>
          </span>
          {submitButton}
        </div>
      </div>
    );
  }

  return (
    <div className="tsgrid" aria-busy={!ready || busy || undefined}>
      {nav}
      <div className="tsgrid-scroll">
        <table className="tsgrid-table">
          <thead>
            <tr>
              <th scope="col" className="tsgrid-linehead">
                {fieldTitle(spec.line)}
              </th>
              {days.map((day, i) => (
                <th
                  key={day}
                  scope="col"
                  className={`tsgrid-dayhead-cell${day === today ? " today" : ""}${i >= 5 ? " weekend" : ""}`}
                >
                  <span className="tsgrid-dname">{weekdayShort(day)}</span>
                  <span className="tsgrid-dnum">{dayNumber(day)}</span>
                </th>
              ))}
              <th scope="col" className="tsgrid-total">
                {t("Total")}
              </th>
            </tr>
          </thead>
          <tbody>
            {grid.lines.map((l) => (
              <tr key={l.key}>
                <th scope="row" className="tsgrid-line">
                  <span className="tsgrid-task">{lineTitle(l)}</span>
                  {parentLine(l)}
                  {ownerLine(l)}
                </th>
                {days.map((day, i) => (
                  <td
                    key={day}
                    className={`${day === today ? "today" : ""}${i >= 5 ? " weekend" : ""}`}
                  >
                    <Cell
                      line={l}
                      day={day}
                      spec={spec}
                      label={t("{task} on {day}", { task: lineTitle(l), day: dayLong(day) })}
                      disabled={!p.canUpdate}
                      commit={commit}
                      onOpen={p.onOpen}
                    />
                  </td>
                ))}
                <td className="tsgrid-total num">{hours(l.total)}</td>
              </tr>
            ))}
          </tbody>
          <tfoot>
            <tr>
              <th scope="row">{t("Total")}</th>
              {days.map((day, i) => (
                <td
                  key={day}
                  className={`num${day === today ? " today" : ""}${i >= 5 ? " weekend" : ""}`}
                >
                  {hours(grid.dayTotals[day] ?? 0)}
                </td>
              ))}
              <td className="tsgrid-total num">{durationText(grid.total)}</td>
            </tr>
          </tfoot>
        </table>
        {empty}
      </div>
      {adder}
    </div>
  );
}

/** One cell: a box for hours, or the hours of entries that are not typed over (a link to them). */
function Cell(p: {
  line: GridLine;
  day: string;
  spec: GridSpec;
  label: string;
  disabled: boolean;
  commit: (l: GridLine, day: string, text: string) => Promise<Committed>;
  onOpen: (id: string) => void;
}) {
  const entries = p.line.cells[p.day] ?? [];
  const minutes = entries.reduce((n, r) => n + minutesOf(r, p.spec), 0);
  // Read as "7 h 45"; typed as "7:45" once the box has the focus.
  const shown = minutes ? durationClock(minutes) : "";
  const [text, setText] = useState(shown);
  const [focused, setFocused] = useState(false);
  // What was typed, shown until the server's answer changes the cell (or the write fails).
  const [pending, setPending] = useState<string | null>(null);
  const saving = useRef(false);
  // Escape puts the old value back: the blur that follows must not save what was typed.
  const cancelled = useRef(false);
  useEffect(() => setPending(null), [minutes]);

  if (!cellEditable(entries, p.spec) || p.disabled) {
    const one = entries.length === 1 ? entries[0]! : null;
    const why =
      entries.length > 1
        ? t("{count} entries: open the list to change them", { count: String(entries.length) })
        : t("No longer a draft: open it to see why");
    return entries.length ? (
      <button
        type="button"
        className="tsgrid-locked num"
        title={p.disabled ? p.label : why}
        aria-label={`${p.label}: ${durationText(minutes)}`}
        onClick={() => one && p.onOpen(one.id)}
        disabled={!one}
      >
        {!p.disabled && <Icon name="lock" size={12} />}
        {durationText(minutes)}
      </button>
    ) : (
      <span className="tsgrid-blank" aria-hidden="true" />
    );
  }

  const save = async () => {
    if (cancelled.current) {
      cancelled.current = false;
      return;
    }
    if (saving.current) return;
    saving.current = true;
    const typed = text;
    setPending(typed);
    const out = await p.commit(p.line, p.day, typed);
    saving.current = false;
    if (out !== "saved") setPending(null);
  };
  const onKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      e.preventDefault();
      e.currentTarget.blur();
    } else if (e.key === "Escape") {
      e.preventDefault();
      cancelled.current = true;
      setText(shown);
      setFocused(false);
      e.currentTarget.blur();
    }
  };
  return (
    <input
      className={`tsgrid-input num${minutes ? " filled" : ""}`}
      type="text"
      inputMode="decimal"
      aria-label={p.label}
      placeholder="0:00"
      title={t("Hours: 8, 1.5 or 1:30")}
      value={focused ? text : (pending ?? (minutes ? durationText(minutes) : ""))}
      onFocus={(e) => {
        setText(shown);
        setFocused(true);
        // Swap "1 h 30" for "1:30" and select it now, not after the next render: a render that
        // changes the value drops the selection, and what is typed next would be appended.
        const box = e.currentTarget;
        box.value = shown;
        box.select();
      }}
      onBlur={() => {
        setFocused(false);
        void save();
      }}
      onChange={(e) => setText(e.target.value)}
      onKeyDown={onKey}
    />
  );
}
