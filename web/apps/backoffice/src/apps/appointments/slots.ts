// The slots of a day for one resource: every 30 minutes from 08:00 to 18:00, wider when a booking
// lies outside those hours; a slot is free when an appointment of `length` minutes starting there
// overlaps no booking and ends by midnight. On today's date a slot that has already begun is past.
export const APPOINTMENT = "appointments_appointment";
export const STEP = 30;
const OPEN = 8 * 60;
const CLOSE = 18 * 60;

export type Booking = { id: string; title: string; start: number; end: number };
/**
 * `free`: a booking of the chosen length fits there. `short`: the half hour itself is free, but the
 * chosen length runs into a booking (or past midnight). `booked`: another booking covers it.
 * `past`: the day is today and the slot began before the current minute.
 */
export type Slot = { start: number; free: boolean; kind: "free" | "short" | "booked" | "past" };

/** Whether `b` overlaps the time from `start` to `end` (touching ends do not overlap). */
export const overlaps = (b: Booking, start: number, end: number): boolean =>
  b.start < end && start < Math.max(b.end, b.start + 1);

/**
 * `now`: the current minute of the day when the day shown is today (see `minuteIn`); slots that
 * start before it are past and cannot be chosen. Leave it out for any other day.
 */
export function freeSlots(taken: readonly Booking[], length: number, now?: number): Slot[] {
  const len = Math.max(STEP, length);
  let from = OPEN;
  let to = CLOSE;
  for (const b of taken) {
    from = Math.min(from, Math.floor(b.start / STEP) * STEP);
    to = Math.max(to, Math.ceil(b.end / STEP) * STEP);
  }
  to = Math.min(to, 1440);
  const out: Slot[] = [];
  for (let s = from; s + STEP <= to; s += STEP) {
    if (now !== undefined && s < now) {
      out.push({ start: s, free: false, kind: "past" });
      continue;
    }
    const free = s + len <= 1440 && !taken.some((b) => overlaps(b, s, s + len));
    const booked = taken.some((b) => overlaps(b, s, s + STEP));
    out.push({ start: s, free, kind: free ? "free" : booked ? "booked" : "short" });
  }
  return out;
}

/** The current minute of the day (0 to 1439) in `zone`; the browser's own where the zone is unknown. */
export function minuteIn(zone: string, now = new Date()): number {
  try {
    const p = new Intl.DateTimeFormat("en-GB", {
      timeZone: zone,
      hour: "2-digit",
      minute: "2-digit",
      hourCycle: "h23",
    }).formatToParts(now);
    const h = Number(p.find((x) => x.type === "hour")?.value);
    const m = Number(p.find((x) => x.type === "minute")?.value);
    if (Number.isInteger(h) && Number.isInteger(m)) return (h % 24) * 60 + m;
  } catch {
    /* an unknown zone */
  }
  return now.getHours() * 60 + now.getMinutes();
}

const clock = new Intl.DateTimeFormat(undefined, {
  hour: "2-digit",
  minute: "2-digit",
  timeZone: "UTC",
});

/**
 * A time of day in the browser's own format, the one the Start and End inputs show (`10:30` or
 * `10:30 AM`), so the slots and the inputs of the form read alike.
 */
export const slotTime = (minutes: number): string =>
  clock.format(new Date(Date.UTC(2000, 0, 1, Math.floor(minutes / 60), minutes % 60)));
