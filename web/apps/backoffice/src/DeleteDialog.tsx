import { useState } from "react";
import { ConfirmDialog } from "@obop/ui";
import type { ModelMeta } from "./api.ts";
import { useData } from "./data.tsx";
import { modelLabel } from "./format.ts";
import {
  deleteRecords,
  plural as pluralOf,
  summarize,
  type DeleteResult,
  type RefHit,
} from "./remove.ts";

type Props = {
  model: ModelMeta;
  /** "lead": used in sentences, singular. */
  noun: string;
  records: { id: string; label: string }[];
  onClose: () => void;
  /** Called after at least one record was removed or proposed, so the page can refresh. */
  onDone: (results: DeleteResult[]) => void;
};

/** Where the blockers of a delete live: a link when this person may open that model. */
function Blockers({ hits, close }: { hits: RefHit[]; close: () => void }) {
  const d = useData();
  return (
    <ul className="blockers">
      {hits.map((h) => {
        const info = d.models.get(h.model);
        const name = info ? modelLabel(h.model, info.module.id) : h.model;
        const visible = !!info && d.linkTo(h.model) !== null;
        return (
          <li key={h.model}>
            <strong>
              {name} ({h.count})
            </strong>
            {visible ? ": " : ": you cannot open this app. "}
            {h.ids.map((id, i) => {
              const to = visible ? d.linkTo(h.model, { id }) : null;
              return (
                <span key={id}>
                  {i > 0 && ", "}
                  {to ? (
                    <a href={to} onClick={close}>
                      {d.refText(h.model, id)}
                    </a>
                  ) : (
                    <code>{id.slice(0, 8)}</code>
                  )}
                </span>
              );
            })}
            {h.count > h.ids.length && ` and ${h.count - h.ids.length} more`}
          </li>
        );
      })}
    </ul>
  );
}

/**
 * Delete one or many records of a model, in the app (never `confirm()`). It lists what blocks a
 * delete (the records that still reference it), explains an agent's approval flow, and ends with
 * a per-record result when more than one record was involved or something did not work.
 */
export function DeleteDialog({ model, noun, records, onClose, onDone }: Props) {
  const d = useData();
  const [busy, setBusy] = useState(false);
  const [results, setResults] = useState<DeleteResult[] | null>(null);
  const many = records.length > 1;
  const plural = (n: number) => pluralOf(noun, n);

  async function run() {
    setBusy(true);
    const out = await deleteRecords(d.run, model.name, records);
    setBusy(false);
    const worked = out.some((r) => r.status === "deleted" || r.status === "approval");
    if (worked) onDone(out);
    const clean = out.every((r) => r.status === "deleted");
    const approvals = out.filter((r) => r.status === "approval");
    if (clean) {
      d.notify(
        many
          ? `${out.length} ${plural(out.length)} deleted`
          : `${noun.charAt(0).toUpperCase()}${noun.slice(1)} deleted`,
      );
      return onClose();
    }
    if (out.every((r) => r.status === "approval")) {
      d.notify(
        `Approval needed: deleting ${many ? `${out.length} ${plural(out.length)}` : `${noun} “${out[0]!.label}”`} was proposed as plan ${approvals[0]!.planId!.slice(0, 8)}. Nothing is deleted until a human approves it.`,
        "info",
      );
      return onClose();
    }
    setResults(out);
  }

  // Single record that could not be deleted: say why right in the dialog, with links to the blockers.
  const single = results?.length === 1 ? results[0]! : null;

  if (results)
    return (
      <ConfirmDialog
        title={single ? `Could not delete ${noun}` : "Delete finished"}
        cancelLabel="Close"
        onClose={onClose}
        error={
          single ? (
            single.refs ? (
              <>
                <p>“{single.label}” is still used by other records. Remove or change them first.</p>
                <Blockers hits={single.refs} close={onClose} />
              </>
            ) : (
              <p>{single.message}</p>
            )
          ) : undefined
        }
      >
        {!single && (
          <>
            <p role="status">{summarize(results, noun)}.</p>
            <ul className="results">
              {results.map((r) => (
                <li key={r.id} data-status={r.status}>
                  <strong>{r.label}</strong>{" "}
                  {r.status === "deleted" && <span className="ok-text">deleted</span>}
                  {r.status === "approval" && (
                    <span>waiting for approval (plan {r.planId?.slice(0, 8)})</span>
                  )}
                  {r.status === "referenced" && r.refs && (
                    <>
                      <span className="bad-text">still in use</span>
                      <Blockers hits={r.refs} close={onClose} />
                    </>
                  )}
                  {r.status === "failed" && <span className="bad-text">{r.message}</span>}
                </li>
              ))}
            </ul>
          </>
        )}
      </ConfirmDialog>
    );

  return (
    <ConfirmDialog
      title={many ? `Delete ${records.length} ${plural(records.length)}?` : `Delete ${noun}?`}
      confirmLabel={many ? `Delete ${records.length} ${plural(records.length)}` : "Delete"}
      busy={busy}
      onConfirm={() => void run()}
      onClose={onClose}
    >
      <p>
        {many ? (
          <>
            Delete {records.length} {plural(records.length)}? This cannot be undone.
          </>
        ) : (
          <>
            Delete {noun} “{records[0]?.label}”? This cannot be undone.
          </>
        )}
      </p>
      {many && records.length <= 8 && (
        <ul className="results">
          {records.map((r) => (
            <li key={r.id}>{r.label}</li>
          ))}
        </ul>
      )}
      <p className="hint">
        Records that other records still point at are kept, and you will see which ones. An agent
        can only propose a delete; a human approves it.
      </p>
    </ConfirmDialog>
  );
}
