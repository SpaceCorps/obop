import { useRef, useState, type ReactNode } from "react";
import { ConfirmDialog, fmt, t } from "@obop/ui";
import type { Row } from "./api.ts";
import { BULK_CAP, inBatches, tally, type Collected, type Outcome } from "./bulk.ts";
import { serverMessage } from "./serverMessage.ts";

type Props = {
  title: string;
  /** What the confirmation says is about to happen to the records. */
  question: (count: number) => ReactNode;
  confirmLabel: (count: number) => string;
  /** The records matching the search: how many, and a way to fetch them in pages. */
  total: number;
  collect: (o: {
    cap: number;
    cancelled: () => boolean;
    onProgress: (done: number, total: number) => void;
  }) => Promise<Collected>;
  /** Change one record. */
  apply: (row: Row) => Promise<Outcome>;
  labelOf: (row: Row) => string;
  /** "records" in a sentence ("12 leads"); the generic noun of the language. */
  noun: (n: number) => string;
  onClose: () => void;
  /** At least one record changed or was proposed, so the page can refresh. */
  onDone: () => void;
};

type Phase =
  | { at: "confirm" }
  | { at: "collect"; done: number; total: number }
  | { at: "apply"; done: number; total: number }
  | {
      at: "done";
      results: Outcome[];
      cancelled: boolean;
      stopped: boolean;
      untouched: number;
      error?: string;
    };

const SHOWN = 100;

/**
 * A set-field or delete over every record a search matches (D1). It says how many records are
 * involved before it starts, collects them in pages (at most `BULK_CAP`), applies the change in
 * batches with a progress bar and a Cancel button, and ends with the result per record.
 */
export function BulkDialog(p: Props) {
  const [phase, setPhase] = useState<Phase>({ at: "confirm" });
  const stop = useRef(false);
  const [stopping, setStopping] = useState(false);
  const count = Math.min(p.total, BULK_CAP);
  const capped = p.total > BULK_CAP;

  async function run() {
    stop.current = false;
    setPhase({ at: "collect", done: 0, total: count });
    const got = await p.collect({
      cap: BULK_CAP,
      cancelled: () => stop.current,
      onProgress: (done, total) => setPhase({ at: "collect", done, total }),
    });
    if ("error" in got)
      return setPhase({
        at: "done",
        results: [],
        cancelled: false,
        stopped: false,
        untouched: 0,
        error: got.error,
      });
    if (got.cancelled)
      return setPhase({
        at: "done",
        results: [],
        cancelled: true,
        stopped: false,
        untouched: got.rows.length,
      });
    const items = got.rows.map((row) => ({ id: row.id, label: p.labelOf(row), row }));
    setPhase({ at: "apply", done: 0, total: items.length });
    const out = await inBatches(items, (item) => p.apply(item.row), {
      cancelled: () => stop.current,
      onProgress: (done, total) => setPhase({ at: "apply", done, total }),
    });
    if (out.results.some((r) => r.status === "done" || r.status === "approval")) p.onDone();
    setPhase({
      at: "done",
      results: out.results,
      cancelled: out.cancelled,
      stopped: out.stopped,
      untouched: items.length - out.results.length,
    });
  }

  if (phase.at === "confirm")
    return (
      <ConfirmDialog
        title={p.title}
        confirmLabel={p.confirmLabel(count)}
        onConfirm={() => void run()}
        onClose={p.onClose}
      >
        <p>{p.question(count)}</p>
        {capped && (
          <p role="note" className="hint">
            {t(
              "This search matches {total} records. A bulk action handles at most {cap} at a time, so it will run on the first {cap}. Narrow the search to include the rest.",
              { total: fmt.integer(p.total), cap: fmt.integer(BULK_CAP) },
            )}
          </p>
        )}
        <p className="hint">
          {t(
            "Records that cannot be changed are skipped and listed afterwards. An agent can only propose changes; a human approves them.",
          )}
        </p>
      </ConfirmDialog>
    );

  if (phase.at === "collect" || phase.at === "apply") {
    const collecting = phase.at === "collect";
    return (
      <ConfirmDialog
        title={p.title}
        cancelLabel={stopping ? t("Stopping…") : t("Cancel")}
        onClose={() => {
          stop.current = true;
          setStopping(true);
        }}
      >
        <p role="status" aria-live="polite">
          {collecting
            ? t("Collecting the matching records: {done} of {total}", {
                done: fmt.integer(phase.done),
                total: fmt.integer(phase.total),
              })
            : t("Working on record {done} of {total}", {
                done: fmt.integer(phase.done),
                total: fmt.integer(phase.total),
              })}
        </p>
        <progress
          className="bulk-progress"
          max={Math.max(1, phase.total)}
          value={phase.done}
          aria-label={t("Progress")}
        />
        <p className="hint">
          {t("Cancel stops after the batch in progress. Records already changed stay changed.")}
        </p>
      </ConfirmDialog>
    );
  }

  const n = tally(phase.results);
  const problems = phase.results.filter((r) => r.status !== "done");
  return (
    <ConfirmDialog
      title={phase.error ? t("Could not collect the records") : t("Bulk action finished")}
      cancelLabel={t("Close")}
      onClose={p.onClose}
      error={phase.error ? <p>{serverMessage(phase.error)}</p> : undefined}
    >
      {!phase.error && (
        <>
          <p role="status" data-testid="bulk-summary">
            {t("{done} done, {approval} waiting for approval, {failed} failed.", {
              done: fmt.integer(n.done),
              approval: fmt.integer(n.approval),
              failed: fmt.integer(n.failed + n.referenced),
            })}
          </p>
          {phase.cancelled && (
            <p>
              {t("Cancelled: {count} {noun} were not touched.", {
                count: fmt.integer(phase.untouched),
                noun: p.noun(phase.untouched),
              })}
            </p>
          )}
          {phase.stopped && (
            <p role="alert">
              {t(
                "The connection was lost, so the run stopped. {count} {noun} were not touched. Reconnect and run it again.",
                { count: fmt.integer(phase.untouched), noun: p.noun(phase.untouched) },
              )}
            </p>
          )}
          {problems.length > 0 && (
            <>
              <ul className="results">
                {problems.slice(0, SHOWN).map((r) => (
                  <li key={r.id} data-status={r.status}>
                    <strong>{r.label}</strong>{" "}
                    {r.status === "approval" ? (
                      <span>
                        {t("waiting for approval (plan {plan})", {
                          plan: r.planId?.slice(0, 8) ?? "",
                        })}
                      </span>
                    ) : (
                      <span className="bad-text">{serverMessage(r.message)}</span>
                    )}
                  </li>
                ))}
              </ul>
              {problems.length > SHOWN && (
                <p className="hint">
                  {t("and {count} more", { count: fmt.integer(problems.length - SHOWN) })}
                </p>
              )}
            </>
          )}
        </>
      )}
    </ConfirmDialog>
  );
}
