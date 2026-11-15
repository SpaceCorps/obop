// The records a person opened lately, kept in this browser only (a convenience, like the recent
// apps): the palette shows them before anything is typed and an app's overview lists its own.
// D75 has no change history yet, so "recently changed" cannot be offered; this is "recently opened".

export type RecentRecord = {
  app: string;
  model: string;
  id: string;
  /** The record's title when it was opened. */
  title: string;
  /** When it was opened, in milliseconds since the epoch. */
  at: number;
};

const KEY = "obop-recent-records";
const MAX = 12;

export function recentRecords(): RecentRecord[] {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? "[]") as unknown;
    return Array.isArray(v)
      ? v.filter(
          (r): r is RecentRecord =>
            !!r &&
            typeof r === "object" &&
            typeof (r as RecentRecord).model === "string" &&
            typeof (r as RecentRecord).id === "string" &&
            typeof (r as RecentRecord).app === "string",
        )
      : [];
  } catch {
    return [];
  }
}

/** Put a record first in the list (once), dropping the oldest past the limit. */
export function rememberRecord(r: Omit<RecentRecord, "at">): void {
  if (!r.id || r.id === "new") return;
  const rest = recentRecords().filter((x) => !(x.model === r.model && x.id === r.id));
  try {
    localStorage.setItem(KEY, JSON.stringify([{ ...r, at: Date.now() }, ...rest].slice(0, MAX)));
  } catch {
    /* storage unavailable: nothing is remembered */
  }
}

/** Forget a record (it was deleted, or it can no longer be read). */
export function forgetRecord(model: string, id: string): void {
  try {
    localStorage.setItem(
      KEY,
      JSON.stringify(recentRecords().filter((x) => !(x.model === model && x.id === id))),
    );
  } catch {
    /* storage unavailable */
  }
}
