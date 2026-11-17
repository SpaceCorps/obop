// Dates of a week (Monday first) as `YYYY-MM-DD`, computed at noon so a change of summer time
// never moves a day.
const pad = (n: number) => String(n).padStart(2, "0");
const iso = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
const noon = (day: string) => new Date(`${day}T12:00:00`);

export function addDays(day: string, n: number): string {
  const d = noon(day);
  d.setDate(d.getDate() + n);
  return iso(d);
}

/** The Monday of the week `day` is in. */
export function mondayOf(day: string): string {
  const wd = (noon(day).getDay() + 6) % 7;
  return addDays(day, -wd);
}

/** The seven days from `monday`. */
export function weekDays(monday: string): string[] {
  return Array.from({ length: 7 }, (_, i) => addDays(monday, i));
}
