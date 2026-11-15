// Inputs for numbers that mean something more (plan 2.7 item 2): a time of day, a duration, a
// percentage, basis points, metres shown as kilometres and seconds since 1970. Each one keeps the
// stored unit and only changes how the value is read and typed.
import { useId, type ReactNode } from "react";
import { fmt, t } from "@obop/ui";
import { getTime } from "../api.ts";
import { decimalInput, splitDecimal } from "../decimals.ts";
import { epochToInstant, fromZonedInput, instantToEpoch, toZonedInput } from "../instants.ts";
import { isInt, useTypedText, type BaseInputProps } from "./common.ts";
import {
  clockText,
  durationClock,
  durationText,
  kmInput,
  parseBasisPoints,
  parseClock,
  parseDuration,
  parseKm,
  percentShare,
} from "./display.ts";

/** A box with a unit after the number ("%", "km"), like the money field's symbol. */
function Unit({ unit, children }: { unit: string; children: ReactNode }) {
  return (
    <div className="unitin">
      {children}
      <span aria-hidden="true">{unit}</span>
    </div>
  );
}

/** A time of day kept as minutes after midnight: the browser's time box, in steps of 15 minutes. */
export function TimeInput({ value, onChange, readOnly, common }: BaseInputProps) {
  const v = String(value);
  return (
    <input
      {...common}
      type="time"
      step={900}
      className="num timein"
      value={isInt(v) ? clockText(v) : ""}
      readOnly={readOnly}
      onChange={(e) => {
        const m = parseClock(e.target.value);
        onChange(m === null ? e.target.value : String(m));
      }}
    />
  );
}

/** A duration kept as minutes: reads "1 h 30", takes 1:30, 1.5h or 90. */
export function DurationInput({ value, onChange, readOnly, common, compact }: BaseInputProps) {
  const hintId = useId();
  const box = useTypedText(
    value,
    (v) => (isInt(v) ? durationText(v) : v),
    (v) => (isInt(v) ? durationClock(v) : v),
    (typed) => {
      const m = parseDuration(typed);
      return m === null ? typed : String(m);
    },
    onChange,
  );
  const read = box.focused ? parseDuration(box.text) : null;
  const say = read !== null && box.text.trim() !== durationClock(read);
  return (
    <div className="durationfield">
      <input
        {...common}
        aria-describedby={
          [common["aria-describedby"], say ? hintId : ""].filter(Boolean).join(" ") || undefined
        }
        type="text"
        inputMode="text"
        className="num"
        placeholder={readOnly ? "" : t("1:30")}
        title={t("Type 1:30, 1.5h or 90 (minutes)")}
        value={box.text}
        readOnly={readOnly}
        onFocus={box.onFocus}
        onBlur={box.onBlur}
        onChange={(e) => box.onChange(e.target.value)}
      />
      {say && !compact && (
        <small className="hint" id={hintId} role="status">
          {t("Read as {duration}", { duration: durationText(read) })}
        </small>
      )}
    </div>
  );
}

/** A share of 0 to 100, with a percent sign and a bar under the box. */
export function PercentInput({
  field,
  value,
  onChange,
  readOnly,
  common,
  compact,
}: BaseInputProps) {
  const share = percentShare(String(value));
  return (
    <div className="percentfield">
      <Unit unit="%">
        <input
          {...common}
          type="number"
          step={1}
          min={field.min ?? 0}
          max={field.max ?? 100}
          className="num"
          value={String(value)}
          readOnly={readOnly}
          onChange={(e) => onChange(e.target.value)}
        />
      </Unit>
      {share !== null && !compact && <PercentBar share={share} />}
    </div>
  );
}

/** A small bar of a share between 0 and 1. */
export function PercentBar({ share, label }: { share: number; label?: string }) {
  return (
    <span className="pctbar" aria-hidden={label ? undefined : true} title={label}>
      <span style={{ inlineSize: `${Math.round(share * 100)}%` }} />
    </span>
  );
}

/** Basis points shown and typed as a percentage: 1900 is 19 %. */
export function BasisPointsInput({ value, onChange, readOnly, common }: BaseInputProps) {
  const asPercent = (v: string) =>
    isInt(v) ? fmt.number(Number(v) / 100, { maximumFractionDigits: 2, useGrouping: false }) : v;
  const box = useTypedText(
    value,
    asPercent,
    asPercent,
    (typed) => {
      const bp = parseBasisPoints(typed);
      return bp === null ? typed : String(bp);
    },
    onChange,
  );
  return (
    <Unit unit="%">
      <input
        {...common}
        type="text"
        inputMode="decimal"
        className="num"
        placeholder={readOnly ? "" : fmt.number(19)}
        value={box.text}
        readOnly={readOnly}
        onFocus={box.onFocus}
        onBlur={box.onBlur}
        onChange={(e) => box.onChange(e.target.value)}
      />
    </Unit>
  );
}

/** Metres shown and typed as kilometres. */
export function KmInput({ field, value, onChange, readOnly, common }: BaseInputProps) {
  const decimal = field.kind === "decimal";
  const mark = fmt.separators().decimal;
  // The form holds a decimal with the locale's mark ("200,15"); a number as digits.
  const metres = (v: string) => (decimal ? v.replace(mark, ".") : v);
  const show = (v: string) => {
    const m = metres(v);
    return splitDecimal(m) ? kmInput(m) : v;
  };
  const box = useTypedText(
    value,
    show,
    show,
    (typed) => {
      const m = parseKm(typed, decimal ? (field.scale ?? 0) : null);
      if (m === null) return typed;
      return decimal ? decimalInput(m, mark) : m;
    },
    onChange,
  );
  return (
    <Unit unit="km">
      <input
        {...common}
        type="text"
        inputMode="decimal"
        className="num"
        value={box.text}
        readOnly={readOnly}
        onFocus={box.onFocus}
        onBlur={box.onBlur}
        onChange={(e) => box.onChange(e.target.value)}
      />
    </Unit>
  );
}

/** Seconds since 1970 shown and typed as a date and time in the instance's zone. */
export function EpochInput({ value, onChange, readOnly, common }: BaseInputProps) {
  const zone = getTime().zone;
  const show = (v: string) => (isInt(v) ? toZonedInput(epochToInstant(Number(v)), zone) : v);
  const box = useTypedText(
    value,
    show,
    show,
    (typed) => {
      const r = fromZonedInput(typed, zone);
      const s = "value" in r ? instantToEpoch(r.value) : null;
      return s === null ? typed : String(s);
    },
    onChange,
  );
  return (
    <input
      {...common}
      type="text"
      className="num"
      placeholder={readOnly ? "" : "2026-03-29 14:30"}
      value={box.text}
      readOnly={readOnly}
      onFocus={box.onFocus}
      onBlur={box.onBlur}
      onChange={(e) => box.onChange(e.target.value)}
    />
  );
}
