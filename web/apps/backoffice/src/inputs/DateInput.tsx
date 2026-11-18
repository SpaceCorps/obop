// Dates and instants (plan 2.7 item 4). A date reads and is typed in the region's own order
// (25.11.2026 in Germany, 11/25/2026 in the US; ISO always works) with a calendar beside it. An
// instant is a date and a time on the instance's wall clock, without seconds, with the calendar and
// a time box; the zone is named only when it is not the browser's own. What is typed with an offset
// (`+02:00`, `Z`) still means that instant.
import {
  useEffect,
  useId,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
  type RefObject,
} from "react";
import { fmt, t, validDate } from "@obop/ui";
import { getTime } from "../api.ts";
import { fromZonedInput } from "../instants.ts";
import { Icon } from "../ui.tsx";
import { useTypedText, type BaseInputProps } from "./common.ts";
import { clockText, parseClock, parseRegionDate, regionDate, regionPattern } from "./display.ts";
import { useAnchored } from "./popover.ts";

const pad = (n: number) => String(n).padStart(2, "0");
const isoOf = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
const dateOf = (iso: string) => {
  const [y, m, d] = iso.split("-").map(Number) as [number, number, number];
  return new Date(y, m - 1, d);
};
const todayIso = () => isoOf(new Date());

/** The zone of this browser, to tell whether the instance's needs saying. */
function browserZone(): string {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone;
  } catch {
    return "UTC";
  }
}

/** A month to choose a day from: arrows move, Enter picks, Escape closes; today is marked. */
export function CalendarGrid({
  value,
  onPick,
  onClose,
  label,
}: {
  /** The chosen day (ISO), or "". */
  value: string;
  onPick: (iso: string) => void;
  onClose: () => void;
  label: string;
}) {
  const start = validDate(value) ? value : todayIso();
  const [focus, setFocus] = useState(start);
  const grid = useRef<HTMLDivElement>(null);
  const f = dateOf(focus);
  const month = new Date(f.getFullYear(), f.getMonth(), 1);
  const first = fmt.firstDay() % 7; // 0 = Sunday
  const lead = (month.getDay() - first + 7) % 7;
  const days: Date[] = [];
  for (let i = 0; i < 42; i++)
    days.push(new Date(month.getFullYear(), month.getMonth(), 1 - lead + i));
  const weeks = days[35]!.getMonth() === month.getMonth() ? 6 : 5;
  const today = todayIso();

  useEffect(() => {
    grid.current?.querySelector<HTMLButtonElement>(`[data-day="${focus}"]`)?.focus();
  }, [focus]);

  const move = (n: number) => {
    const d = dateOf(focus);
    d.setDate(d.getDate() + n);
    setFocus(isoOf(d));
  };
  const moveMonth = (n: number) => {
    const d = dateOf(focus);
    const day = d.getDate();
    d.setDate(1);
    d.setMonth(d.getMonth() + n);
    const last = new Date(d.getFullYear(), d.getMonth() + 1, 0).getDate();
    d.setDate(Math.min(day, last));
    setFocus(isoOf(d));
  };
  const keys = (e: KeyboardEvent) => {
    const k: Record<string, () => void> = {
      ArrowLeft: () => move(-1),
      ArrowRight: () => move(1),
      ArrowUp: () => move(-7),
      ArrowDown: () => move(7),
      PageUp: () => moveMonth(-1),
      PageDown: () => moveMonth(1),
      Home: () => move(-((dateOf(focus).getDay() - first + 7) % 7)),
      End: () => move(6 - ((dateOf(focus).getDay() - first + 7) % 7)),
    };
    const act = k[e.key];
    if (act) {
      e.preventDefault();
      act();
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onClose();
    }
  };
  const weekday = (i: number) =>
    fmt.dateOf(new Date(2026, 0, 4 + ((first + i) % 7)), { weekday: "narrow" });
  const dayName = (i: number) =>
    fmt.dateOf(new Date(2026, 0, 4 + ((first + i) % 7)), { weekday: "long" });

  return (
    <div className="calgrid" role="group" aria-label={label} onKeyDown={keys}>
      <div className="calhead">
        <button
          type="button"
          className="icon-btn"
          aria-label={t("Previous month")}
          onClick={() => moveMonth(-1)}
        >
          <Icon name="chevron-left" size={16} />
        </button>
        <strong aria-live="polite">{fmt.dateOf(month, { month: "long", year: "numeric" })}</strong>
        <button
          type="button"
          className="icon-btn"
          aria-label={t("Next month")}
          onClick={() => moveMonth(1)}
        >
          <Icon name="chevron-right" size={16} />
        </button>
      </div>
      <div className="calweek" aria-hidden="true">
        {Array.from({ length: 7 }, (_, i) => (
          <span key={i} title={dayName(i)}>
            {weekday(i)}
          </span>
        ))}
      </div>
      <div className="caldays" ref={grid}>
        {days.slice(0, weeks * 7).map((d) => {
          const iso = isoOf(d);
          const out = d.getMonth() !== month.getMonth();
          return (
            <button
              key={iso}
              type="button"
              data-day={iso}
              tabIndex={iso === focus ? 0 : -1}
              className={out ? "out" : undefined}
              aria-label={fmt.dateOf(d, { dateStyle: "full" })}
              aria-pressed={iso === value}
              aria-current={iso === today ? "date" : undefined}
              onClick={() => onPick(iso)}
            >
              {d.getDate()}
            </button>
          );
        })}
      </div>
      <div className="calfoot">
        <button type="button" className="linkbtn" onClick={() => onPick(today)}>
          {t("Today")}
        </button>
      </div>
    </div>
  );
}

/** The button that opens a calendar beside a date box, and the popover it opens. */
function CalendarButton({
  anchor,
  value,
  label,
  onPick,
  children,
}: {
  anchor: RefObject<HTMLElement | null>;
  value: string;
  label: string;
  onPick: (iso: string) => void;
  /** More controls under the month (the time of an instant). */
  children?: (close: () => void) => ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const pop = useRef<HTMLDivElement>(null);
  const btn = useRef<HTMLButtonElement>(null);
  useAnchored(open, anchor, pop, { maxHeight: 420, minWidth: 280, matchWidth: false });
  useEffect(() => {
    if (!open) return;
    const on = (e: MouseEvent) => {
      const path = e.composedPath();
      if (pop.current && !path.includes(pop.current) && !path.includes(btn.current!))
        setOpen(false);
    };
    addEventListener("mousedown", on);
    return () => removeEventListener("mousedown", on);
  }, [open]);
  const close = () => {
    setOpen(false);
    btn.current?.focus();
  };
  return (
    <>
      <button
        ref={btn}
        type="button"
        className="icon-btn calbtn"
        aria-label={label}
        aria-expanded={open}
        aria-haspopup="dialog"
        onClick={() => setOpen((o) => !o)}
      >
        <Icon name="calendar" size={16} />
      </button>
      {open && (
        <div ref={pop} className="cal-pop" popover="manual" role="dialog" aria-label={label}>
          <CalendarGrid
            value={value}
            label={label}
            onClose={close}
            onPick={(iso) => {
              onPick(iso);
              if (!children) close();
            }}
          />
          {children?.(close)}
        </div>
      )}
    </>
  );
}

/** A date (ISO in the form), read and typed in the region's order, with a calendar. */
export function DateInput({ value, onChange, readOnly, common, compact }: BaseInputProps) {
  const wrap = useRef<HTMLDivElement>(null);
  const box = useTypedText(
    value,
    (v) => (validDate(v) ? regionDate(v) : v),
    (v) => (validDate(v) ? regionDate(v) : v),
    (typed) => (typed.trim() ? (parseRegionDate(typed) ?? typed) : ""),
    onChange,
  );
  const v = String(value);
  return (
    <div className={`datefield${compact ? " compact" : ""}`} ref={wrap}>
      <input
        {...common}
        type="text"
        inputMode="numeric"
        autoComplete="off"
        placeholder={readOnly ? "" : regionPattern()}
        value={box.text}
        readOnly={readOnly}
        onFocus={box.onFocus}
        onBlur={box.onBlur}
        onChange={(e) => box.onChange(e.target.value)}
      />
      {!readOnly && (
        <CalendarButton
          anchor={wrap}
          value={validDate(v) ? v : ""}
          label={t("Open the calendar")}
          onPick={(iso) => onChange(iso)}
        />
      )}
    </div>
  );
}

/** "2026-03-29 14:30" or "29.03.2026 14:30" as the text an instant field holds; null otherwise. */
export function readZoned(typed: string, zone: string): string | null {
  const s = typed.trim();
  if (!s) return "";
  if ("value" in fromZonedInput(s, zone)) return s;
  const m = /^(.+?)[ ,T]+(\d{1,2}[:.]\d{2})$/.exec(s);
  if (!m) return null;
  const day = parseRegionDate(m[1]!);
  const min = parseClock(m[2]!.replace(".", ":"));
  if (!day || min === null) return null;
  return `${day} ${clockText(min)}`;
}

/** An instant as the region reads it, to the minute: "29.03.2026 14:30". */
function zonedText(form: string): string {
  const m = /^(\d{4}-\d{2}-\d{2}) (\d{2}:\d{2})(:\d{2}(\.\d+)?)?(Z)?$/.exec(form);
  if (!m || m[5]) return form;
  return `${regionDate(m[1]!)} ${m[2]}`;
}

/**
 * The time box of the calendar's popover: the 24-hour clock the form's own boxes use (S5), not the
 * browser's time box, which an English browser draws as "09:00 AM". Takes 14:30, 14,30, 1430 or 14
 * when it is left or Enter is pressed.
 */
function ClockBox({ value, onPick }: { value: string; onPick: (hhmm: string) => void }) {
  const [text, setText] = useState(value);
  useEffect(() => setText(value), [value]);
  const take = () => {
    const min = parseClock(text);
    if (min === null) setText(value);
    else if (clockText(min) !== value) onPick(clockText(min));
    else setText(value);
  };
  return (
    <input
      type="text"
      inputMode="decimal"
      autoComplete="off"
      className="num timein"
      title={t("Type 14:30, 1430 or 14")}
      value={text}
      onChange={(e) => setText(e.target.value)}
      onBlur={take}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          take();
        }
      }}
    />
  );
}

/** An instant: a date and a time on the instance's wall clock, with a calendar and a time box. */
export function DatetimeInput({ value, onChange, readOnly, common, compact }: BaseInputProps) {
  const zone = getTime().zone;
  const hintId = useId();
  const wrap = useRef<HTMLDivElement>(null);
  const other = zone !== browserZone();
  const box = useTypedText(
    value,
    zonedText,
    zonedText,
    (typed) => readZoned(typed, zone) ?? typed,
    onChange,
  );
  const form = String(value);
  const m = /^(\d{4}-\d{2}-\d{2}) (\d{2}:\d{2})/.exec(form);
  const day = m?.[1] ?? "";
  const time = m?.[2] ?? "09:00";
  const describedBy =
    [common["aria-describedby"], other && !compact ? hintId : ""].filter(Boolean).join(" ") ||
    undefined;
  return (
    <div className={`datefield datetime${compact ? " compact" : ""}`} ref={wrap}>
      <input
        {...common}
        aria-describedby={describedBy}
        type="text"
        autoComplete="off"
        placeholder={readOnly ? "" : `${regionPattern()} 14:30`}
        title={t("A date and a time in {zone}; end with Z or +02:00 to say another zone", { zone })}
        value={box.text}
        readOnly={readOnly}
        onFocus={box.onFocus}
        onBlur={box.onBlur}
        onChange={(e) => box.onChange(e.target.value)}
      />
      {other && (
        <span
          className="zone"
          id={compact ? undefined : hintId}
          title={t("Time zone of the instance")}
        >
          {zone}
        </span>
      )}
      {!readOnly && (
        <CalendarButton
          anchor={wrap}
          value={day}
          label={t("Open the calendar")}
          onPick={(iso) => onChange(`${iso} ${time}`)}
        >
          {(close) => (
            <div className="caltime">
              <label>
                <span>{t("Time")}</span>
                <ClockBox
                  value={time}
                  onPick={(hhmm) => onChange(`${day || todayIso()} ${hhmm}`)}
                />
              </label>
              <button type="button" className="btn small" onClick={close}>
                {t("Done")}
              </button>
            </div>
          )}
        </CalendarButton>
      )}
    </div>
  );
}
