// Planning: the names the screens share, and the values a new shift from the week plan starts with.
export const SHIFT = "planning_shift";
export const EMPLOYEE = "hr_employee";
/** The view name of the week plan (`#/planning/planning_shift?view=week`); empty opens it too. */
export const WEEK = "week";
/** Below this width the week is a list by day instead of a table. */
export const NARROW = "(max-width: 47.99rem)";

let pending: Record<string, unknown> | null = null;

/** The values the next new shift starts with (a cell of the week plan: the employee and the day). */
export function setNewShift(values: Record<string, unknown>): void {
  pending = values;
}

/** Take them once: the form asks when it opens on a new shift. */
export function takeNewShift(): Record<string, unknown> | null {
  const v = pending;
  pending = null;
  return v;
}

let week: string | null = null;

/** The Monday of the week the week plan shows, kept while the person opens and closes shifts. */
export function rememberWeek(monday: string): void {
  week = monday;
}

export function rememberedWeek(): string | null {
  return week;
}
