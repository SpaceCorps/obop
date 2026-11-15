import { useEffect, useLayoutEffect, useRef, useState, type PointerEvent } from "react";
import { fmt, t } from "@obop/ui";
import { hhmm, placeDay, snap, visibleHours, type CalEvent } from "./calendarModel.ts";
import { CardMenu } from "./CardMenu.tsx";

/** The height of one hour in the grid, in CSS pixels. */
const HOUR = 48;
/** A pointer that moved less than this is a click, not a drag. */
const SLOP = 4;

type Drag = {
  kind: "move" | "resize" | "create";
  ev?: CalEvent;
  dayIdx: number;
  x0: number;
  y0: number;
  s0: number;
  e0: number;
  moved: boolean;
};

export type TimeGridProps = {
  /** The seven days, `YYYY-MM-DD`. */
  days: string[];
  events: CalEvent[];
  today: string;
  selectedId: string;
  /** The text and the tone class of an event. */
  label: (ev: CalEvent) => string;
  tone: (ev: CalEvent) => string;
  /** Moving and resizing write the record (an `update`); absent: events only open. */
  onMove?: (ev: CalEvent, day: string, startMin: number, endMin: number) => void;
  /** Dragging over empty time opens a new record for that time; absent: nothing to drag. */
  onCreate?: (day: string, startMin: number, endMin: number) => void;
  onOpen: (id: string) => void;
};

/**
 * The week as a time grid: a column per day, events placed by their start and end, overlapping
 * ones side by side. With a mouse, an event is dragged to another time or day and its bottom edge
 * to another end, which writes the record; dragging over empty time opens a new record for it. On
 * a touch screen events only open, so a finger scrolls the grid. Events without a time sit in the
 * row above the hours.
 */
export function TimeGrid(p: TimeGridProps) {
  const body = useRef<HTMLDivElement>(null);
  const scroller = useRef<HTMLDivElement>(null);
  const drag = useRef<Drag | null>(null);
  const suppressClick = useRef(false);
  const [ghost, setGhost] = useState<{ id?: string; dayIdx: number; s: number; e: number } | null>(
    null,
  );
  const timedEvs = p.events.filter((e) => e.startMin !== undefined);
  const allDay = p.events.filter((e) => e.startMin === undefined);
  const hours = visibleHours(timedEvs);
  const weekKey = p.days[0];

  // Open on the working hours of the week (or of its events), once per week shown.
  // The events arrive after the week is drawn: it scrolls again when the first ones are there.
  const placedFor = useRef("");
  const ready = timedEvs.length > 0 ? `${weekKey}:events` : `${weekKey}:empty`;
  useLayoutEffect(() => {
    if (!scroller.current || placedFor.current === `${weekKey}:events`) return;
    scroller.current.scrollTop = hours.from * HOUR;
    placedFor.current = ready;
    // only when the week changes or its events first arrive
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready]);

  // How many events fit side by side depends on how wide a day is.
  const [colW, setColW] = useState(150);
  useEffect(() => {
    const el = body.current;
    if (!el || typeof ResizeObserver !== "function") return;
    const ro = new ResizeObserver(() => {
      const day = el.querySelector<HTMLElement>(".tg-day");
      if (day) setColW(day.clientWidth);
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  const maxCols = Math.max(2, Math.min(4, Math.floor(colW / 44)));

  // The line of the current time moves once a minute.
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const id = setInterval(() => setNow(new Date()), 60_000);
    return () => clearInterval(id);
  }, []);
  const nowMin = now.getHours() * 60 + now.getMinutes();

  const geometry = () => {
    const b = body.current!.getBoundingClientRect();
    const first = body.current!.querySelector<HTMLElement>(".tg-day")!.getBoundingClientRect();
    return { top: b.top, left: first.left, col: first.width };
  };
  const minuteAt = (clientY: number) => ((clientY - geometry().top) / HOUR) * 60;
  const dayAt = (clientX: number) => {
    const g = geometry();
    const rtl = getComputedStyle(body.current!).direction === "rtl";
    const i = Math.floor((clientX - g.left) / g.col);
    const idx = rtl ? p.days.length - 1 - i : i;
    return Math.max(0, Math.min(p.days.length - 1, idx));
  };

  const start = (e: PointerEvent, d: Drag) => {
    // A finger scrolls; only a mouse or a pen drags.
    if (e.pointerType === "touch" || e.button !== 0) return;
    drag.current = d;
    body.current?.setPointerCapture(e.pointerId);
  };
  const onMove = (e: PointerEvent) => {
    const d = drag.current;
    if (!d) return;
    if (!d.moved && Math.hypot(e.clientX - d.x0, e.clientY - d.y0) < SLOP) return;
    d.moved = true;
    const delta = snap(((e.clientY - d.y0) / HOUR) * 60 + 1440) - 1440;
    if (d.kind === "move") {
      const len = d.e0 - d.s0;
      const s = Math.max(0, Math.min(1440 - len, d.s0 + delta));
      setGhost({ id: d.ev!.row.id, dayIdx: dayAt(e.clientX), s, e: s + len });
    } else if (d.kind === "resize") {
      setGhost({
        id: d.ev!.row.id,
        dayIdx: d.dayIdx,
        s: d.s0,
        e: Math.max(d.s0 + 15, d.e0 + delta),
      });
    } else {
      const at = snap(minuteAt(e.clientY));
      setGhost({ dayIdx: d.dayIdx, s: Math.min(d.s0, at), e: Math.max(d.s0 + 15, at) });
    }
  };
  const onUp = () => {
    const d = drag.current;
    drag.current = null;
    const g = ghost;
    setGhost(null);
    if (!d) return;
    if (d.kind === "create") {
      const day = p.days[d.dayIdx]!;
      if (d.moved && g) p.onCreate?.(day, g.s, g.e);
      else p.onCreate?.(day, d.s0, Math.min(1440, d.s0 + 60));
      return;
    }
    // The pointer was held by the grid, so the click may not reach the event: open it here.
    suppressClick.current = true;
    setTimeout(() => (suppressClick.current = false), 0);
    if (!d.moved || !g) return p.onOpen(d.ev!.row.id);
    const day = p.days[g.dayIdx]!;
    if (day !== d.ev!.start || g.s !== d.s0 || g.e !== d.e0) p.onMove?.(d.ev!, day, g.s, g.e);
  };

  const block = (ev: CalEvent, col: number, cols: number) => {
    const g = ghost && ghost.id === ev.row.id ? ghost : null;
    const s = g ? g.s : ev.startMin!;
    const e = g ? g.e : ev.endMin!;
    const title = p.label(ev);
    const time = `${hhmm(s)}–${hhmm(e)}`;
    // Under 45 minutes the time and the title share one line.
    const short = e - s < 45;
    // In a narrow share of the day only the start time fits.
    const narrow = cols > 1;
    return (
      <button
        key={ev.row.id}
        type="button"
        className={`tg-ev ${p.tone(ev)}${ev.row.id === p.selectedId ? " sel" : ""}${g ? " dragging" : ""}${p.onMove ? " movable" : ""}${short ? " short" : ""}`}
        style={{
          insetBlockStart: (s / 60) * HOUR,
          blockSize: Math.max(20, ((e - s) / 60) * HOUR - 2),
          insetInlineStart: `calc(${(col / cols) * 100}% + 2px)`,
          inlineSize: `calc(${100 / cols}% - 4px)`,
        }}
        title={`${time} ${title}`}
        aria-label={`${time}, ${title}`}
        onClick={() => {
          if (!suppressClick.current) p.onOpen(ev.row.id);
        }}
        onPointerDown={(pe) => {
          // Not the day's own drag, which would make a new record.
          pe.stopPropagation();
          if (!p.onMove) return;
          start(pe, {
            kind: "move",
            ev,
            dayIdx: p.days.indexOf(ev.start),
            x0: pe.clientX,
            y0: pe.clientY,
            s0: ev.startMin!,
            e0: ev.endMin!,
            moved: false,
          });
        }}
      >
        <span className="tg-time">{short || narrow ? hhmm(s) : time}</span>
        <span className="tg-title">{title}</span>
        {p.onMove && (
          <span
            className="tg-resize"
            aria-hidden="true"
            onPointerDown={(pe) => {
              pe.stopPropagation();
              start(pe, {
                kind: "resize",
                ev,
                dayIdx: p.days.indexOf(ev.start),
                x0: pe.clientX,
                y0: pe.clientY,
                s0: ev.startMin!,
                e0: ev.endMin!,
                moved: false,
              });
            }}
          />
        )}
      </button>
    );
  };

  const header = (day: string) => {
    const d = new Date(`${day}T12:00:00`);
    return (
      <div key={day} className={`tg-dayhead${day === p.today ? " today" : ""}`} role="columnheader">
        <span>{fmt.dateOf(d, { weekday: "short" })}</span>
        <b>{d.getDate()}</b>
      </div>
    );
  };

  return (
    <div className="tg" role="grid" aria-label={t("Week")}>
      <div className="tg-head" role="row">
        <div className="tg-gutter" />
        {p.days.map(header)}
      </div>
      {allDay.length > 0 && (
        <div className="tg-allday" role="row">
          <div className="tg-gutter">{t("All day")}</div>
          {p.days.map((day) => (
            <div key={day} className="tg-allday-cell" role="gridcell">
              {allDay
                .filter((ev) => ev.start <= day && day <= ev.end)
                .map((ev) => (
                  <button
                    key={ev.row.id}
                    type="button"
                    className={`cal-ev ${p.tone(ev)}${ev.row.id === p.selectedId ? " sel" : ""}`}
                    onClick={() => p.onOpen(ev.row.id)}
                    title={p.label(ev)}
                  >
                    <span className="evdot" aria-hidden="true" />
                    <span className="evt">{p.label(ev)}</span>
                  </button>
                ))}
            </div>
          ))}
        </div>
      )}
      <div className="tg-scroll" ref={scroller}>
        <div
          className="tg-body"
          ref={body}
          style={{ blockSize: 24 * HOUR }}
          onPointerMove={onMove}
          onPointerUp={onUp}
          onPointerCancel={() => {
            drag.current = null;
            setGhost(null);
          }}
        >
          <div className="tg-hours" aria-hidden="true">
            {Array.from({ length: 24 }, (_, h) => (
              <span key={h} style={{ insetBlockStart: h * HOUR }}>
                {h === 0 ? "" : hhmm(h * 60)}
              </span>
            ))}
          </div>
          {p.days.map((day, i) => {
            const mine = timedEvs.filter((ev) => ev.start === day && ghost?.id !== ev.row.id);
            const moving =
              ghost?.id && ghost.dayIdx === i
                ? timedEvs.filter((ev) => ev.row.id === ghost.id)
                : [];
            return (
              <div
                key={day}
                className={`tg-day${day === p.today ? " today" : ""}`}
                role="gridcell"
                aria-label={fmt.dateOf(new Date(`${day}T12:00:00`), { dateStyle: "full" })}
                onPointerDown={(pe) => {
                  if (!p.onCreate) return;
                  const s = snap(minuteAt(pe.clientY), 30);
                  start(pe, {
                    kind: "create",
                    dayIdx: i,
                    x0: pe.clientX,
                    y0: pe.clientY,
                    s0: Math.min(1425, s),
                    e0: Math.min(1440, s + 60),
                    moved: false,
                  });
                }}
              >
                {(() => {
                  const { placed, overflow } = placeDay(mine, maxCols);
                  return (
                    <>
                      {placed.map((pl) => block(pl.ev, pl.col, pl.cols))}
                      {overflow.map((o) => (
                        <div
                          key={`more-${o.from}`}
                          className="tg-overflow"
                          style={{
                            insetBlockStart: (o.from / 60) * HOUR,
                            blockSize: Math.max(20, ((o.to - o.from) / 60) * HOUR - 2),
                            insetInlineStart: `calc(${(o.col / o.cols) * 100}% + 2px)`,
                            inlineSize: `calc(${100 / o.cols}% - 4px)`,
                          }}
                          onPointerDown={(pe) => pe.stopPropagation()}
                        >
                          <CardMenu
                            className="tg-more"
                            label={t("{count} more", { count: o.events.length })}
                            trigger={`+${o.events.length}`}
                            items={o.events.map((ev) => ({
                              label: `${hhmm(ev.startMin!)}–${hhmm(ev.endMin!)} ${p.label(ev)}`,
                              onSelect: () => p.onOpen(ev.row.id),
                            }))}
                          />
                        </div>
                      ))}
                    </>
                  );
                })()}
                {moving.map((ev) => block(ev, 0, 1))}
                {ghost && !ghost.id && ghost.dayIdx === i && (
                  <div
                    className="tg-new"
                    style={{
                      insetBlockStart: (ghost.s / 60) * HOUR,
                      blockSize: ((ghost.e - ghost.s) / 60) * HOUR,
                    }}
                  >
                    {hhmm(ghost.s)}–{hhmm(ghost.e)}
                  </div>
                )}
                {day === p.today && (
                  <div
                    className="tg-now"
                    style={{ insetBlockStart: (nowMin / 60) * HOUR }}
                    aria-hidden="true"
                  />
                )}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
