import { inSentence, t } from "@obop/ui";
import type { Command, FieldMeta, ModelMeta, Reply } from "./api.ts";
import { isEmpty, optionLabel } from "./format.ts";
import { actionTitle } from "./labels.ts";
import { serverMessage } from "./serverMessage.ts";

/**
 * Governed workflows (D48). A `select` field is governed when some action moves it. The server
 * only lets such a field change as an implicit or explicit run of a declared action, so the UI
 * offers exactly the moves the server would accept, and explains the rest.
 *
 * `/api/meta` lists only the actions the signed-in principal may run, so "no action listed" means
 * "this person cannot move it there", whatever the reason.
 */

/** The actions that move `field`, with the state they lead to. */
export function actionsOn(model: ModelMeta, field: string): Command[] {
  return model.actions.filter((a) => a.field === field && !!a.to);
}

export function isGoverned(model: ModelMeta, field: FieldMeta): boolean {
  return (
    field.kind === "select" && (field.governed === true || actionsOn(model, field.name).length > 0)
  );
}

/**
 * A field that is governed for someone who cannot run any of its actions still is governed: the
 * server decides. The meta lists only runnable actions, so a select named like a status that has
 * no listed action may or may not be governed; the UI treats it as free and lets the server say.
 */
export const governedNames = (model: ModelMeta): Set<string> =>
  new Set(model.actions.filter((a) => a.field && a.to).map((a) => a.field!));

/** The state a record starts in: the first option. */
export const initialOption = (f: FieldMeta): string => f.options?.[0] ?? "";

export type Move = {
  /** Whether the person can move the record from `from` to `to` right now. */
  allowed: boolean;
  /** The action to run, when allowed. */
  action?: Command;
  /** Why not, in a sentence (empty when allowed or when it is the current state). */
  reason: string;
  /** Running it needs a human's approval when an agent does it. */
  needsApproval: boolean;
  /** `to` equals the current state: nothing to do. */
  current: boolean;
};

/**
 * Two actions of one model may lead along the same move and be told apart only by the record. A
 * sign request is sent by `send` when it is made from a template and by `send_document` when it is
 * about a Document in Documents (D92): the server refuses `send` for a request with a document and
 * no template, so the status bar and the kanban must run `send_document` for it. No registry flag
 * says which action is for which record, so the back office names the pair.
 */
const SEND_FOR_DOCUMENT = "sign_request.send_document";
const SEND_FOR_TEMPLATE = "sign_request.send";
/** Marks a request signed by hand. The server refuses it for a request about a Document (D92). */
const SIGN_BY_HAND = "sign_request.sign";

const hasDocument = (row: Record<string, unknown>) => !isEmpty(row.document_id);

/**
 * Whether a sign request is about a Document, so that only the signer can sign it, through the
 * signing link. That is a request that points at a document, or one that holds a document's hash
 * (the server takes it when the request is sent) and was not made from a template: a request that
 * had its document cleared still has the hash. A request from a template may carry a hash typed by
 * hand under sign 0.2.0; staff still close that one by hand, so the hash alone does not decide.
 * The server decides in the end; this only keeps the back office from offering what it refuses.
 */
export const isDocumentRequest = (row: Record<string, unknown>): boolean =>
  hasDocument(row) || (!isEmpty(row.document_hash) && isEmpty(row.template_id));

/** The action to run among those that lead along the same move, for this record. */
function forRecord(model: ModelMeta, candidates: Command[], row: Record<string, unknown>) {
  if (model.name !== "sign_request") return undefined;
  const want = hasDocument(row) ? SEND_FOR_DOCUMENT : SEND_FOR_TEMPLATE;
  return candidates.find((a) => a.id === want);
}

/**
 * Pick the action for a move: an exact `from` wins over an open one (D48). When several actions
 * match exactly, the record may decide which one (`forRecord`); without a record the first one
 * wins, as before.
 */
export function pickAction(
  model: ModelMeta,
  field: string,
  from: string | null,
  to: string,
  row?: Record<string, unknown>,
): Command | undefined {
  const leading = actionsOn(model, field).filter((a) => a.to === to);
  const exact = leading.filter((a) => a.from === from);
  if (exact.length > 1 && row) {
    const chosen = forRecord(model, exact, row);
    if (chosen) return chosen;
  }
  return exact[0] ?? leading.find((a) => isEmpty(a.from));
}

/**
 * An action that is not offered at all for this record: marking a request signed by hand when it is
 * about a Document, because the signer signs it through the link and the server refuses the manual
 * step. Its button is left out of the panel; the step in the status bar and the kanban are greyed
 * out with `notForRecord`.
 */
export function hiddenForRecord(
  model: ModelMeta,
  action: Command,
  row: Record<string, unknown>,
): boolean {
  return model.name === "sign_request" && action.id === SIGN_BY_HAND && isDocumentRequest(row);
}

/**
 * Why an action that is listed must not be run on this record although its state fits, or an empty
 * string. `send` is for a request made from a template and `send_document` for a request about a
 * document; the other one is greyed out with the reason instead of failing when it is pressed. The
 * same for marking a request signed by hand when it is about a document.
 */
export function notForRecord(
  model: ModelMeta,
  action: Command,
  row: Record<string, unknown>,
): string {
  if (model.name !== "sign_request") return "";
  if (hiddenForRecord(model, action, row))
    return t(
      "A request about a document is signed by the signer through the signing link. It cannot be marked signed by hand.",
    );
  const doc = hasDocument(row);
  if (action.id === SEND_FOR_TEMPLATE && doc && isEmpty(row.template_id))
    return t("This request is about a document. Use Send document.");
  if (action.id === SEND_FOR_DOCUMENT && !doc)
    return t("Send document needs a document. Choose one first.");
  return "";
}

export function canMove(
  model: ModelMeta,
  field: FieldMeta,
  from: string | null | undefined,
  to: string,
  noun: string,
  /** The record, when there is one: it may decide between two actions that lead the same way. */
  row?: Record<string, unknown>,
): Move {
  const cur = isEmpty(from) ? null : String(from);
  if (cur === to) return { allowed: false, reason: "", needsApproval: false, current: true };
  const action = pickAction(model, field.name, cur, to, row);
  // An action that the server would refuse for this very record is not a move the person can make.
  const why = action && row ? notForRecord(model, action, row) : "";
  if (action && why)
    return { allowed: false, action, reason: why, needsApproval: false, current: false };
  if (action)
    return {
      allowed: true,
      action,
      reason: "",
      needsApproval: action.needs_approval,
      current: false,
    };
  return {
    allowed: false,
    reason: t("No action moves a {noun} from {from} to {to}", {
      noun: inSentence(noun),
      from: cur ? optionLabel(cur, field) : t("no state"),
      to: optionLabel(to, field),
    }),
    needsApproval: false,
    current: false,
  };
}

/** The label of an action button: "Mark won". */
export const actionLabel = (a: Command) => actionTitle(a);

export type MoveOutcome =
  | { kind: "ok"; row: unknown }
  | { kind: "approval"; planId: string; message: string }
  | { kind: "error"; message: string; raw: string };

/** Text for the toast of a 202: names the plan and what it would do. */
export function moveApproval(
  noun: string,
  to: string,
  planId: string,
  action?: Command,
  field?: FieldMeta,
): string {
  const vars = { noun: inSentence(noun), state: optionLabel(to, field), plan: planId.slice(0, 8) };
  return action
    ? t(
        "Approval needed: moving {noun} to {state} ({action}) was proposed as plan {plan}. Nothing changes until a human approves it.",
        { ...vars, action: actionTitle(action) },
      )
    : t(
        "Approval needed: moving {noun} to {state} was proposed as plan {plan}. Nothing changes until a human approves it.",
        vars,
      );
}

/**
 * Move a record along a governed field. Prefers the action command, so the action's own handler
 * runs; falls back to `update` when only an implicit move exists (an older server).
 */
export async function moveRecord(
  run: (id: string, body?: object) => Promise<Reply>,
  model: ModelMeta,
  field: FieldMeta,
  row: { id: string } & Record<string, unknown>,
  to: string,
  noun: string,
): Promise<MoveOutcome> {
  const m = canMove(model, field, row[field.name] as string | undefined, to, noun, row);
  if (m.current) return { kind: "ok", row };
  // An action is known but not for this record (see `notForRecord`): say why, send nothing.
  if (m.action && !m.allowed) return { kind: "error", message: m.reason, raw: m.reason };
  const r = m.action
    ? await run(m.action.id, { id: row.id })
    : await run(`${model.name}.update`, { id: row.id, [field.name]: to });
  if (r.kind === "ok") return { kind: "ok", row: r.value };
  if (r.kind === "approval") {
    return {
      kind: "approval",
      planId: r.planId,
      message: moveApproval(noun, to, r.planId, m.action, field),
    };
  }
  // `raw` is the server's own sentence, for a caller that keeps it and words it when it is shown.
  return { kind: "error", message: serverMessage(r.message), raw: r.message };
}
