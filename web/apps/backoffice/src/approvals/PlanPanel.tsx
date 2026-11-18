// One plan on screen: what it changes, who asked and when, and the two decisions (D97, rule 10).
// It is the record panel's shape (`.record`): a panel beside the list on a laptop, a sheet over it
// on a tablet, the whole screen on a phone, with Reject and Approve in a footer that stays in view.
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import {
  Button,
  ConfirmDialog,
  Dialog,
  IconButton,
  Skeleton,
  Textarea,
  t,
  useFocusTrap,
} from "@obop/ui";
import { useData } from "../data.tsx";
import { optionLabel } from "../format.ts";
import { PHONE_QUERY, SHEET_QUERY, useMedia, useScrollLock } from "../media.ts";
import { serverMessage } from "../serverMessage.ts";
import {
  REASON_MAX,
  planApprove,
  planGet,
  planReject,
  reasonLength,
  type PlanChange,
  type PlanDetail,
  type PlanFail,
} from "./planApi.ts";
import { plansChanged } from "./usePlanCount.ts";
import {
  alreadyDecidedSentence,
  approveLabel,
  askOf,
  dateOf,
  decidedSentence,
  goneSentence,
  thingOf,
  valueText,
  whenOf,
  type Names,
} from "./words.ts";

type View =
  | { kind: "loading" }
  | { kind: "gone" }
  | { kind: "failed" }
  | { kind: "plan"; plan: PlanDetail };

/** A sentence about the last decision tried: announced, and it takes focus. */
type Note = { tone: "danger" | "warning" | "info"; text: string };

type Act = "approve" | "reject";

export function PlanPanel({
  id,
  names,
  onClose,
  onDecided,
  onStale,
}: {
  id: string;
  names: Names;
  onClose: () => void;
  /** A decision went through: back to the list, which reads again. */
  onDecided: () => void;
  /** The plan is not as the list shows it (decided by somebody else, failed, gone): read it again. */
  onStale?: () => void;
}) {
  const d = useData();
  const [view, setView] = useState<View>({ kind: "loading" });
  const [note, setNote] = useState<Note | null>(null);
  // Set when the plan turned out to be decided by somebody else (409): no button is offered.
  const [closed, setClosed] = useState<string | null>(null);
  const [busy, setBusy] = useState<Act | "reload" | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [rejecting, setRejecting] = useState(false);
  // A second press before React has drawn the busy button is dropped here (one call per decision).
  const inFlight = useRef(false);
  const panel = useRef<HTMLElement>(null);
  const noteRef = useRef<HTMLDivElement>(null);
  const sheet = useMedia(SHEET_QUERY);
  const phone = useMedia(PHONE_QUERY);
  useFocusTrap(panel, sheet);
  useScrollLock(sheet);

  /** Read the plan again; the answer replaces what is shown. */
  const reload = useCallback(async (): Promise<PlanDetail | null> => {
    const r = await planGet(d.token, id);
    if (r.ok) {
      setView({ kind: "plan", plan: r.value });
      return r.value;
    }
    setView(r.status === 404 ? { kind: "gone" } : { kind: "failed" });
    return null;
  }, [d.token, id]);

  useEffect(() => {
    void reload();
  }, [reload]);

  // The panel takes focus when it opens (its heading is read first); the page gives it back to the
  // row when it closes.
  useEffect(() => {
    panel.current?.focus({ preventScroll: true });
  }, []);
  // The panel does not close while a decision is on its way: its answer would have nowhere to go,
  // and a late success would close the plan the person opened next.
  const close = useCallback(() => {
    if (!inFlight.current) onClose();
  }, [onClose]);
  useEffect(() => {
    const on = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !e.defaultPrevented && !document.querySelector(".overlay")) close();
    };
    addEventListener("keydown", on);
    return () => removeEventListener("keydown", on);
  }, [close]);
  // A refused decision is announced (role="alert") and takes focus.
  useEffect(() => {
    if (note) noteRef.current?.focus({ preventScroll: false });
  }, [note]);

  const plan = view.kind === "plan" ? view.plan : null;

  /** The list and the count are read again: the plan is not what they last showed. */
  function stale() {
    plansChanged();
    onStale?.();
  }

  async function failed(r: PlanFail, act: Act, shown: PlanDetail) {
    if (r.status === 412 || r.code === "precondition_failed") {
      setBusy("reload");
      const now = await reload();
      setNote(
        now
          ? {
              tone: "warning",
              text: t("This record changed after you opened the request. Look at it again."),
            }
          : { tone: "info", text: goneSentence() },
      );
      if (!now) stale();
      return;
    }
    if (r.status === 409 && r.code === "already_decided") {
      const text = alreadyDecidedSentence(r, shown.expires_at);
      setClosed(text);
      setNote({ tone: "info", text });
      stale();
      return;
    }
    if (r.status === 404) {
      stale();
      // Not "not waiting any more" while the plan can still be read: it changed under the person
      // (the agent's owner lost the right), so it is shown again as it is now.
      setBusy("reload");
      const now = await reload();
      setNote(
        now
          ? {
              tone: "warning",
              text:
                act === "approve"
                  ? t("This request could not be approved as it stands. Look at it again.")
                  : t("This request could not be rejected as it stands. Look at it again."),
            }
          : { tone: "info", text: goneSentence() },
      );
      return;
    }
    if (r.status === 0 || r.status >= 500) {
      setBusy("reload");
      await reload();
      setNote({
        tone: "warning",
        text: t(
          "We could not reach the server. Check whether it went through before trying again.",
        ),
      });
      return;
    }
    if (act === "approve" && r.status === 400) {
      stale();
      // The command's own refusal: the plan is spent (failed), or a guard refused it and it waits.
      setBusy("reload");
      const now = await reload();
      const message = serverMessage(r.error) || t("no reason was given");
      setNote({
        tone: "danger",
        text:
          now?.status === "failed"
            ? t(
                "The system refused it: {message}. This request is closed. The agent can propose it again.",
                { message },
              )
            : now?.status === "waiting"
              ? t("The system refused it: {message}. The request is still waiting.", { message })
              : t("The system refused it: {message}.", { message }),
      });
      return;
    }
    setNote({
      tone: "danger",
      text:
        serverMessage(r.error) ||
        t("The server refused it (status {status}).", {
          status: r.status,
        }),
    });
  }

  async function approve(confirmed = false) {
    if (!plan || inFlight.current) return;
    if (plan.destructive && !confirmed) {
      setConfirming(true);
      return;
    }
    inFlight.current = true;
    setBusy("approve");
    setNote(null);
    const shown = plan;
    try {
      const r = await planApprove(d.token, shown.id, shown.seen_version);
      setConfirming(false);
      if (r.ok) {
        plansChanged();
        d.notify(shown.done ? t("Approved. {done}", { done: shown.done }) : t("Approved."));
        onDecided();
        return;
      }
      await failed(r, "approve", shown);
    } finally {
      inFlight.current = false;
      setBusy(null);
    }
  }

  async function reject(reason: string) {
    if (!plan || inFlight.current) return;
    inFlight.current = true;
    setBusy("reject");
    setNote(null);
    const shown = plan;
    try {
      const r = await planReject(d.token, shown.id, reason);
      setRejecting(false);
      if (r.ok) {
        plansChanged();
        d.notify(t("Rejected. The agent will see your reason."));
        onDecided();
        return;
      }
      await failed(r, "reject", shown);
    } finally {
      inFlight.current = false;
      setBusy(null);
    }
  }

  const waiting = !!plan && plan.status === "waiting" && !closed;
  const record = plan?.record_id ? plan.record_title || "" : "";
  const recordHref =
    plan?.record_id && plan.kind !== "delete" ? d.linkTo(plan.model, { id: plan.record_id }) : null;

  return (
    <>
      {sheet && <div className="record-scrim" aria-hidden="true" onClick={close} />}
      <aside
        ref={panel}
        className={`record panel appr-panel${sheet ? " sheet" : ""}${phone ? " phone" : ""}`}
        aria-label={t("Approval request")}
        role={sheet ? "dialog" : undefined}
        aria-modal={sheet || undefined}
        aria-busy={view.kind === "loading" || undefined}
        tabIndex={-1}
        data-plan-panel={id}
      >
        <header>
          {phone && (
            <IconButton
              icon="chevron-left"
              label={t("Back to Approvals")}
              tooltip={false}
              className="back"
              onClick={close}
            />
          )}
          <div className="rec-title">
            <h2>{plan ? askOf(plan, names) : t("Approval request")}</h2>
          </div>
          {!phone && <IconButton icon="close" label={t("Close")} tooltip={false} onClick={close} />}
        </header>
        <div className="appr-body">
          {view.kind === "loading" && <Skeleton rows={4} cols={2} />}
          {view.kind === "gone" && <p className="appr-closed">{goneSentence()}</p>}
          {view.kind === "failed" && (
            <div className="appr-closed">
              <p>{t("This request could not be loaded.")}</p>
              <Button onClick={() => void reload()}>{t("Try again")}</Button>
            </div>
          )}
          {note && (
            <div
              ref={noteRef}
              className={`appr-note ${note.tone}`}
              role="alert"
              tabIndex={-1}
              data-testid="plan-note"
            >
              {note.text}
            </div>
          )}
          {plan && (
            <>
              {plan.status === "waiting" && (
                <section className="appr-what" aria-labelledby={`appr-what-${id}`}>
                  <h3 id={`appr-what-${id}`} className="overline">
                    {t("What changes")}
                  </h3>
                  <WhatChanges plan={plan} names={names} record={record} />
                </section>
              )}
              <p className="appr-who">
                {proposedBy(plan, whenOf(plan.created_at))}
                {plan.status === "waiting" && plan.expires_at && (
                  <> {t("Expires on {date}.", { date: dateOf(plan.expires_at) })}</>
                )}
              </p>
              {plan.status !== "waiting" && !closed && (
                <p className="appr-closed" data-testid="plan-decided">
                  {decidedSentence(plan)}
                </p>
              )}
              {plan.status === "waiting" && !closed && !plan.can_approve && plan.can_reject && (
                <p className="appr-why" data-testid="plan-cannot-run">
                  {t(
                    "The agent's owner can no longer do this, so it cannot run. You can only reject it.",
                  )}
                </p>
              )}
              {plan.status === "waiting" && !closed && !plan.can_approve && !plan.can_reject && (
                <p className="appr-why" data-testid="plan-someone-else">
                  {t("Waiting for a person who can approve it.")}
                </p>
              )}
              {waiting && plan.can_approve && unseen(plan) && (
                <p className="appr-why" data-testid="plan-unseen">
                  {t("The rows are not shown here, so this cannot be approved from this page.")}
                </p>
              )}
              {recordHref && (
                <p>
                  <a className="appr-record" href={recordHref}>
                    {t("Open the record")}
                  </a>
                </p>
              )}
            </>
          )}
        </div>
        {plan && waiting && ((plan.can_approve && !unseen(plan)) || plan.can_reject) && (
          <footer className="appr-foot">
            {plan.can_reject && (
              <Button
                size="lg"
                disabled={busy !== null}
                loading={busy === "reject"}
                onClick={() => setRejecting(true)}
              >
                {t("Reject")}
              </Button>
            )}
            {plan.can_approve && !unseen(plan) && (
              <Button
                size="lg"
                variant="primary"
                disabled={busy !== null}
                loading={busy === "approve"}
                onClick={() => void approve()}
                data-testid="plan-approve"
              >
                {approveLabel(plan, names)}
              </Button>
            )}
          </footer>
        )}
        {confirming && plan && (
          <ConfirmDialog
            title={confirmQuestion(plan, names, record)}
            confirmLabel={plan.kind === "delete" ? t("Delete for good") : approveLabel(plan, names)}
            busy={busy === "approve"}
            onConfirm={() => void approve(true)}
            onClose={() => setConfirming(false)}
          >
            <p>{thingOf(plan, names)}</p>
          </ConfirmDialog>
        )}
        {rejecting && plan && (
          <RejectDialog
            busy={busy === "reject"}
            onCancel={() => setRejecting(false)}
            onReject={(reason) => void reject(reason)}
          />
        )}
      </aside>
    </>
  );
}

/** "Proposed by Ada's agent, 2 hours ago." One sentence for each who, so each language can inflect it. */
function proposedBy(plan: PlanDetail, when: string): string {
  if (plan.own_agent) return t("Proposed by your own agent, {when}.", { when });
  return plan.owner
    ? t("Proposed by {name}'s agent, {when}.", { name: plan.owner, when })
    : t("Proposed by an agent, {when}.", { when });
}

/**
 * A bulk create whose rows the person is not shown (it was proposed as csv, or its rows are not
 * records): there is nothing to approve that they can read, so only Reject is offered.
 */
function unseen(plan: PlanDetail): boolean {
  if (plan.kind !== "bulk_create" || !(plan.rows_total && plan.rows_total > 0)) return false;
  const rows = (plan.rows ?? []) as unknown[];
  return !rows.some(Array.isArray);
}

/**
 * A record's name for a reference in the change. The model's records are read first; until they
 * are, or when this person may not read them, no short id is shown in their place.
 */
function refName(d: ReturnType<typeof useData>, model: string, id: string): string {
  if (!d.models.has(model)) return t("Not shown to you");
  const rows = d.store.peek(model);
  if (!rows) return d.store.errorOf(model) ? t("Not shown to you") : "…";
  return rows.some((r) => r.id === id) ? d.refText(model, id) : t("Not shown to you");
}

/** The question a destructive plan asks before it is approved: the action's own `ui.confirm` first. */
function confirmQuestion(plan: PlanDetail, names: Names, record: string): string {
  if (plan.confirm) return plan.confirm;
  if (plan.kind === "delete")
    return record
      ? t("Delete “{record}” for good? This cannot be undone.", { record })
      : t("Delete this {model} for good? This cannot be undone.", {
          model: names.model(plan.model),
        });
  return t("{do}? This cannot be undone.", { do: thingOf(plan, names) });
}

/** The change, only the fields that differ; a hidden one is "Set, not shown to you". */
function WhatChanges({ plan, names, record }: { plan: PlanDetail; names: Names; record: string }) {
  const d = useData();
  const refText = (model: string, id: string) => refName(d, model, id);
  if (plan.kind === "delete")
    return (
      <p className="appr-delete">
        {record
          ? t("This deletes {record} for good.", { record: `“${record}”` })
          : t("This deletes the {model} for good.", { model: names.model(plan.model) })}
      </p>
    );
  const changes = plan.changes ?? [];
  const transition =
    plan.transition && !changes.some((c) => c.field === plan.transition?.field)
      ? plan.transition
      : null;
  const rows = (plan.rows ?? []) as (PlanChange[] | null)[];
  const pair = (c: PlanChange): ReactNode => {
    const f = names.fieldOf(plan.model, c.field);
    return (
      <div key={c.field} className="appr-change" data-field={c.field}>
        <dt>{f.title}</dt>
        <dd>
          {c.hidden ? (
            <span className="appr-hidden">{t("Set, not shown to you")}</span>
          ) : plan.kind === "create" || plan.kind === "bulk_create" ? (
            <span className="appr-to">{valueText(c.to, f.meta, refText)}</span>
          ) : (
            <>
              <span className="appr-from">{valueText(c.from, f.meta, refText)}</span>
              <span className="appr-arrow" aria-label={t("becomes")} role="img">
                {" → "}
              </span>
              <span className="appr-to">{valueText(c.to, f.meta, refText)}</span>
            </>
          )}
        </dd>
      </div>
    );
  };
  if (plan.kind === "bulk_create") {
    const total = plan.rows_total ?? rows.length;
    if (unseen(plan))
      return (
        <p className="hint">
          {t(
            "{count, plural, one {# record will be created, but its row is not shown here.} other {# records will be created, but their rows are not shown here.}}",
            { count: total },
          )}
        </p>
      );
    return (
      <>
        <p>
          {t(
            "{count, plural, one {# record will be created. The first one:} other {# records will be created. The first ones:}}",
            { count: total },
          )}
        </p>
        {rows.map((r, i) =>
          Array.isArray(r) ? (
            <dl key={i} className="appr-changes appr-bulk-row">
              {r.map(pair)}
            </dl>
          ) : (
            <p key={i} className="hint">
              {t("This row is not shown.")}
            </p>
          ),
        )}
      </>
    );
  }
  if (!changes.length && !transition)
    return <p className="hint">{t("No changed field that you may read.")}</p>;
  return (
    <dl className="appr-changes">
      {transition &&
        (() => {
          const f = names.fieldOf(plan.model, transition.field ?? "");
          return (
            <div className="appr-change" data-field={transition.field}>
              <dt>{f.title}</dt>
              <dd>
                <span className="appr-from">
                  {transition.from ? optionLabel(transition.from, f.meta) : t("Not set")}
                </span>
                <span className="appr-arrow" aria-label={t("becomes")} role="img">
                  {" → "}
                </span>
                <span className="appr-to">{optionLabel(transition.to ?? "", f.meta)}</span>
              </dd>
            </div>
          );
        })()}
      {changes.map(pair)}
    </dl>
  );
}

/** "Reject this request?" with the reason for the agent, at most 300 characters, counted. */
export function RejectDialog({
  busy,
  onCancel,
  onReject,
}: {
  busy: boolean;
  onCancel: () => void;
  onReject: (reason: string) => void;
}) {
  const [reason, setReason] = useState("");
  const n = reasonLength(reason);
  const over = n > REASON_MAX;
  return (
    <Dialog
      role="alertdialog"
      size="sm"
      title={t("Reject this request?")}
      className="appr-reject"
      onClose={busy ? () => {} : onCancel}
      footer={
        <>
          <Button data-autofocus onClick={onCancel} disabled={busy}>
            {t("Cancel")}
          </Button>
          <Button
            variant="danger"
            loading={busy}
            disabled={over}
            onClick={() => onReject(reason)}
            data-testid="plan-reject-confirm"
          >
            {t("Reject request")}
          </Button>
        </>
      }
    >
      <Textarea
        label={t("Tell the agent why (optional)")}
        name="plan_reject_reason"
        rows={4}
        value={reason}
        onChange={(e) => setReason(e.target.value)}
        hint={over ? undefined : t("{count} of {max} characters", { count: n, max: REASON_MAX })}
        error={
          over
            ? t("{count} of {max} characters: shorten it by {extra}.", {
                count: n,
                max: REASON_MAX,
                extra: n - REASON_MAX,
              })
            : undefined
        }
      />
    </Dialog>
  );
}
