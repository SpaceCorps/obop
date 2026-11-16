// Stock moves made from a screen of the supply apps: the commands an agent calls (OR-4).
// `inventory_move.create` with the kind of move, then `inventory_move.validate` when the person may
// validate, so the stock changes at once; otherwise the move waits as a draft for a stock manager.
import { t } from "@obop/ui";
import { search, type ModelMeta, type Reply, type Row } from "../../api.ts";

export const MOVE = "inventory_move";
export const ITEM = "inventory_item";
export const LOCATION = "inventory_location";

export type MoveKind = "receipt" | "internal" | "scrap" | "adjustment";

type Run = (id: string, body?: object) => Promise<Reply>;

/** Whether the person may validate moves (the action is in their meta). */
export const mayValidate = (move: ModelMeta | undefined): boolean =>
  !!move?.actions.some((a) => a.id === `${MOVE}.validate`);

export type MoveOutcome =
  | { kind: "done"; id: string }
  | { kind: "draft"; id: string; why: string }
  | { kind: "error"; message: string };

/** Create a move and validate it when the person may; the move's id and how far it got. */
export async function makeMove(
  run: Run,
  move: ModelMeta | undefined,
  body: Record<string, unknown>,
): Promise<MoveOutcome> {
  const made = await run(`${MOVE}.create`, body);
  if (made.kind === "error") return { kind: "error", message: made.message };
  if (made.kind === "approval")
    return { kind: "error", message: t("The move was proposed and waits for approval.") };
  const id = String((made.value as Row).id);
  return validateMove(run, move, id);
}

/** Validate a draft move; a person who may not leaves it as a draft for a stock manager. */
export async function validateMove(
  run: Run,
  move: ModelMeta | undefined,
  id: string,
): Promise<MoveOutcome> {
  if (!mayValidate(move))
    return { kind: "draft", id, why: t("A stock manager validates it before the stock changes.") };
  const v = await run(`${MOVE}.validate`, { id });
  if (v.kind === "ok") return { kind: "done", id };
  if (v.kind === "approval")
    return { kind: "draft", id, why: t("Validating it waits for approval.") };
  return { kind: "error", message: v.message };
}

/** The first active location of a kind (receiving, scrap...), by name. */
export function firstLocation(rows: readonly Row[], kind: string): string {
  const of = rows
    .filter((r) => r.kind === kind && r.active !== false)
    .sort((a, b) => String(a.name ?? "").localeCompare(String(b.name ?? "")));
  return of[0]?.id ?? "";
}

/**
 * Validate the draft moves a document made (found by its number as their source document), when
 * the person may; a move of 0 units (nothing arrived on that line) is cancelled instead.
 */
export async function settleMoves(
  run: Run,
  token: string,
  move: ModelMeta | undefined,
  origin: string,
): Promise<{ done: number; waiting: number }> {
  const made = await search(token, MOVE, {
    filter: [
      { field: "origin", op: "eq", value: origin },
      { field: "state", op: "eq", value: "draft" },
    ],
    limit: 500,
  });
  let done = 0;
  let waiting = 0;
  if ("error" in made) return { done, waiting };
  for (const m of made.rows) {
    if (Number(m.quantity ?? 0) === 0) {
      await run(`${MOVE}.cancel`, { id: m.id });
      continue;
    }
    const out = await validateMove(run, move, m.id);
    if (out.kind === "done") done++;
    else waiting++;
  }
  return { done, waiting };
}
