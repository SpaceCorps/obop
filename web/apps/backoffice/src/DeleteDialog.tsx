import { useState } from "react";
import { ConfirmDialog, getLang, t } from "@obop/ui";
import type { ModelMeta } from "./api.ts";
import { useData } from "./data.tsx";
import { modelTitle } from "./labels.ts";
import { serverMessage } from "./serverMessage.ts";
import { deleteRecords, nounFor, summarize, type DeleteResult, type RefHit } from "./remove.ts";

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
        const name = info ? modelTitle(info.model, info.module.id) : h.model;
        const visible = !!info && d.linkTo(h.model) !== null;
        return (
          <li key={h.model}>
            <strong>
              {visible
                ? t("{name} ({count}):", { name, count: h.count })
                : t("{name} ({count}): you cannot open this app.", { name, count: h.count })}
            </strong>{" "}
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
            {h.count > h.ids.length &&
              ` ${t("and {count} more", { count: h.count - h.ids.length })}`}
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
  const plural = (n: number) => nounFor(noun, n);
  const capitalNoun = noun.charAt(0).toLocaleUpperCase(getLang()) + noun.slice(1);

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
          ? t("{count} {noun} deleted", { count: out.length, noun: plural(out.length) })
          : t("{noun} deleted", { noun: capitalNoun }),
      );
      return onClose();
    }
    if (out.every((r) => r.status === "approval")) {
      const plan = approvals[0]!.planId!.slice(0, 8);
      d.notify(
        many
          ? t(
              "Approval needed: deleting {count} {noun} was proposed as plan {plan}. Nothing is deleted until a human approves it.",
              { count: out.length, noun: plural(out.length), plan },
            )
          : t(
              "Approval needed: deleting {noun} “{label}” was proposed as plan {plan}. Nothing is deleted until a human approves it.",
              { noun, label: out[0]!.label, plan },
            ),
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
        title={single ? t("Could not delete {noun}", { noun }) : t("Delete finished")}
        cancelLabel={t("Close")}
        onClose={onClose}
        error={
          single ? (
            single.refs ? (
              <>
                <p>
                  {t("“{label}” is still used by other records. Remove or change them first.", {
                    label: single.label,
                  })}
                </p>
                <Blockers hits={single.refs} close={onClose} />
              </>
            ) : (
              <p>{serverMessage(single.message)}</p>
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
                  {r.status === "deleted" && <span className="ok-text">{t("deleted")}</span>}
                  {r.status === "approval" && (
                    <span>
                      {t("waiting for approval (plan {plan})", {
                        plan: r.planId?.slice(0, 8) ?? "",
                      })}
                    </span>
                  )}
                  {r.status === "referenced" && r.refs && (
                    <>
                      <span className="bad-text">{t("still in use")}</span>
                      <Blockers hits={r.refs} close={onClose} />
                    </>
                  )}
                  {r.status === "failed" && (
                    <span className="bad-text">{serverMessage(r.message)}</span>
                  )}
                </li>
              ))}
            </ul>
          </>
        )}
      </ConfirmDialog>
    );

  return (
    <ConfirmDialog
      title={
        many
          ? t("Delete {count} {noun}?", { count: records.length, noun: plural(records.length) })
          : t("Delete {noun}?", { noun })
      }
      confirmLabel={
        many
          ? t("Delete {count} {noun}", { count: records.length, noun: plural(records.length) })
          : t("Delete")
      }
      busy={busy}
      onConfirm={() => void run()}
      onClose={onClose}
    >
      <p>
        {many ? (
          <>
            {t("Delete {count} {noun}? This cannot be undone.", {
              count: records.length,
              noun: plural(records.length),
            })}
          </>
        ) : (
          <>
            {t("Delete {noun} “{label}”? This cannot be undone.", {
              noun,
              label: records[0]?.label ?? "",
            })}
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
        {t(
          "Records that other records still point at are kept, and you will see which ones. An agent can only propose a delete; a human approves it.",
        )}
      </p>
    </ConfirmDialog>
  );
}
