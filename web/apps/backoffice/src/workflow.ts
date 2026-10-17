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

/** Pick the action for a move: an exact `from` wins over an open one (D48). */
export function pickAction(
  model: ModelMeta,
  field: string,
  from: string | null,
  to: string,
): Command | undefined {
  const leading = actionsOn(model, field).filter((a) => a.to === to);
  return leading.find((a) => a.from === from) ?? leading.find((a) => isEmpty(a.from));
}

export function canMove(
  model: ModelMeta,
  field: FieldMeta,
  from: string | null | undefined,
  to: string,
  noun: string,
): Move {
  const cur = isEmpty(from) ? null : String(from);
  if (cur === to) return { allowed: false, reason: "", needsApproval: false, current: true };
  const action = pickAction(model, field.name, cur, to);
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
  | { kind: "error"; message: string };

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
  const m = canMove(model, field, row[field.name] as string | undefined, to, noun);
  if (m.current) return { kind: "ok", row };
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
  return { kind: "error", message: serverMessage(r.message) };
}
