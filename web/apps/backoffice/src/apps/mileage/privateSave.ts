// Marking a trip private removes its route for good (docs/spec/mobile.md 5.6, rule A of
// 2026-11-18): the form asks before it saves that, as the app does, because the warning above the
// map is out of sight where the person picks the purpose.
import { t } from "@obop/ui";

/** The question to ask before a save that makes `row` private, else null. */
export function privateSaveQuestion(
  row: { purpose?: unknown },
  changes: Record<string, unknown>,
): string | null {
  if (changes.purpose !== "private" || row.purpose === "private") return null;
  return t(
    "A private trip keeps its distance and times, not its route: saving removes the route for good, also if the purpose is changed back later.",
  );
}
