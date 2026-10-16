import { getLang, t } from "@obop/ui";
import type { Reply } from "./api.ts";

/** Who still points at a record: `sale_order (1): id4`. At most five ids are named per model. */
export type RefHit = { model: string; count: number; ids: string[] };

/**
 * Reads the server's refusal to delete a referenced record (D49):
 * `cannot delete party P: still referenced by accounting_move (3): id1, id2, id3; sale_order (1): id4`.
 * Returns null for any other message.
 */
export function parseReferences(message: string): RefHit[] | null {
  const at = message.indexOf("still referenced by ");
  if (at < 0) return null;
  const hits: RefHit[] = [];
  for (const part of message.slice(at + "still referenced by ".length).split("; ")) {
    const m = /^([a-z0-9_]+) \((\d+)\):\s*(.*)$/.exec(part.trim());
    if (!m) continue;
    hits.push({
      model: m[1]!,
      count: Number(m[2]),
      ids: m[3]!
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean),
    });
  }
  return hits.length ? hits : null;
}

export type DeleteResult = {
  id: string;
  label: string;
  /** deleted, waiting for a human (agents), refused because referenced, or failed for another reason. */
  status: "deleted" | "approval" | "referenced" | "failed";
  message?: string;
  planId?: string;
  refs?: RefHit[];
};

/** Delete records one by one so each gets its own result; stops for nothing. */
export async function deleteRecords(
  run: (id: string, body?: object) => Promise<Reply>,
  model: string,
  records: { id: string; label: string }[],
): Promise<DeleteResult[]> {
  const out: DeleteResult[] = [];
  for (const rec of records) {
    const r = await run(`${model}.delete`, { id: rec.id });
    if (r.kind === "ok") out.push({ ...rec, status: "deleted" });
    else if (r.kind === "approval") out.push({ ...rec, status: "approval", planId: r.planId });
    else {
      const refs = parseReferences(r.message);
      out.push(
        refs
          ? { ...rec, status: "referenced", message: r.message, refs }
          : { ...rec, status: "failed", message: r.message },
      );
    }
  }
  return out;
}

/** "lead" to "leads", "party" to "parties" (English only; see `nounFor`). */
export function plural(noun: string, n: number): string {
  if (n === 1) return noun;
  if (/[^aeiou]y$/.test(noun)) return `${noun.slice(0, -1)}ies`;
  if (/(s|x|z|ch|sh)$/.test(noun)) return `${noun}es`;
  return `${noun}s`;
}

/**
 * The word for `n` records in a sentence. English inflects the model's own noun ("2 parties");
 * other languages cannot be derived from a singular, so they say "record(s)" in the form the
 * number needs (Polish and Finnish have different forms for 1, 2-4 and 5 or more).
 */
export function nounFor(noun: string, n: number): string {
  if (getLang() === "en") return plural(noun, n);
  return t("{count, plural, one {record} other {records}}", { count: n });
}

/** The sentence a toast says about a batch of results. */
export function summarize(results: DeleteResult[], noun: string): string {
  const n = (s: DeleteResult["status"]) => results.filter((r) => r.status === s).length;
  const parts: string[] = [];
  if (n("deleted"))
    parts.push(
      t("{count} {noun} deleted", { count: n("deleted"), noun: nounFor(noun, n("deleted")) }),
    );
  if (n("approval"))
    parts.push(
      t("{count} waiting for a human to approve (nothing deleted yet)", { count: n("approval") }),
    );
  if (n("referenced")) parts.push(t("{count} still in use", { count: n("referenced") }));
  if (n("failed")) parts.push(t("{count} failed", { count: n("failed") }));
  return parts.join(", ");
}
