// Appointments: the free times of the chosen resource on the chosen day, inside the booking form.
// It reads the day's other bookings of that resource (`appointments_appointment.search`, the call
// an agent makes for the same question) and shows the 30-minute slots from 08:00 to 18:00 (wider
// when a booking lies outside); a free slot fills Start and End, keeping the length already set.
// When the times chosen overlap another booking it says which, before the form is saved.
import { useEffect, useState } from "react";
import { Icon, t } from "@obop/ui";
import { getTime, search, type Row } from "../../api.ts";
import { useData } from "../../data.tsx";
import type { RecordSectionProps } from "../../extensions.ts";
import { todayIn } from "../../filters.ts";
import { formatDate, validDate } from "../../format.ts";
import { useFieldForm } from "../../inputs/form.ts";
import { APPOINTMENT, freeSlots, minuteIn, overlaps, slotTime, type Booking } from "./slots.ts";
import "./appointments.css";

const str = (v: unknown) => (typeof v === "string" ? v : "");
const minute = (v: unknown): number | null => {
  const n = typeof v === "number" ? v : typeof v === "string" && v.trim() ? Number(v) : NaN;
  return Number.isInteger(n) ? n : null;
};

export function SlotPicker(p: RecordSectionProps) {
  const d = useData();
  const form = useFieldForm();
  const resource = str(form?.peek("resource_id"));
  const day = str(form?.peek("day"));
  const start = minute(form?.peek("start_minute"));
  const end = minute(form?.peek("end_minute"));
  const [taken, setTaken] = useState<Booking[] | null>(null);
  const self = p.row?.id ?? "";
  const ready = !!resource && validDate(day);

  useEffect(() => {
    if (!ready) return;
    let live = true;
    setTaken(null);
    void search(d.token, APPOINTMENT, {
      filter: [
        { field: "resource_id", op: "eq", value: resource },
        { field: "day", op: "eq", value: day },
        { field: "state", op: "ne", value: "cancelled" },
      ],
      limit: 500,
    }).then((r) => {
      if (!live) return;
      const rows: Row[] = "error" in r ? [] : r.rows;
      setTaken(
        rows
          .filter((x) => x.id !== self)
          .map((x) => ({
            id: x.id,
            title: str(x.title),
            start: minute(x.start_minute) ?? 0,
            end: minute(x.end_minute) ?? 0,
          })),
      );
    });
    return () => {
      live = false;
    };
  }, [d.token, resource, day, self, ready, d.mutations]);

  if (!form || !p.canEdit) return null;
  const locked = p.row && ["done", "cancelled", "no_show"].includes(str(p.row.state));
  if (locked) return null;

  const length = start !== null && end !== null && end > start ? end - start : 30;
  const clash =
    taken && start !== null && end !== null && end > start
      ? taken.filter((b) => overlaps(b, start, end))
      : [];
  // On today's date (in the instance's zone) the slots that have begun are past.
  const zone = getTime().zone;
  const now = day === todayIn(zone) ? minuteIn(zone) : undefined;
  const slots = taken ? freeSlots(taken, length, now) : [];

  return (
    <section className="fsec ap-slots" aria-labelledby="ap-slots-head">
      <h3 className="fsec-head ap-slots-head" id="ap-slots-head">
        <Icon name="clock" size={16} />
        {t("Free times")}
      </h3>
      {!ready ? (
        <p className="hint">{t("Choose a resource and a day to see the free times.")}</p>
      ) : (
        <>
          {start !== null && end !== null && end <= start && (
            <p className="ap-warn" role="alert">
              <Icon name="warning" size={15} />
              {t("The end is not after the start.")}
            </p>
          )}
          {clash.map((b) => (
            <p key={b.id} className="ap-warn" role="alert">
              <Icon name="warning" size={15} />
              {t("Clashes with {title}, {from} to {to}", {
                title: b.title || t("another booking"),
                from: slotTime(b.start),
                to: slotTime(b.end),
              })}
            </p>
          ))}
          <p className="hint ap-slots-sub">
            {taken === null
              ? t("Loading")
              : t(
                  "{resource} on {day}: {count, plural, =0 {fully booked} one {# free slot} other {# free slots}}",
                  {
                    resource: d.refText("appointments_resource", resource),
                    day: formatDate(day),
                    count: slots.filter((s) => s.free).length,
                  },
                )}
          </p>
          {taken !== null && (
            <div className="ap-slot-grid" role="group" aria-label={t("Free times")}>
              {slots.map((s) => {
                const chosen = start === s.start && end === s.start + length;
                return (
                  <button
                    key={s.start}
                    type="button"
                    className={`ap-slot is-${s.kind}${chosen ? " is-chosen" : ""}`}
                    disabled={!s.free || p.busy}
                    aria-pressed={chosen}
                    title={
                      s.kind === "booked"
                        ? t("Booked")
                        : s.kind === "past"
                          ? t("Passed")
                          : s.kind === "short"
                            ? t("Free, but too short for {length} minutes", { length })
                            : undefined
                    }
                    onClick={() =>
                      form.fill({
                        start_minute: String(s.start),
                        end_minute: String(s.start + length),
                      })
                    }
                  >
                    {slotTime(s.start)}
                  </button>
                );
              })}
            </div>
          )}
          {slots.some((s) => s.kind === "past") && (
            <p className="hint ap-slots-legend">{t("Earlier times today have passed.")}</p>
          )}
          {slots.some((s) => s.kind === "short") && (
            <p className="hint ap-slots-legend">
              {t(
                "Struck through: booked. Dashed: free, but too short for {length} minutes from there.",
                { length },
              )}
            </p>
          )}
        </>
      )}
    </section>
  );
}
