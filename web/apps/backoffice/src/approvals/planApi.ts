// The five calls of the Approvals page (D97): `plan.count`, `plan.list`, `plan.get`, `plan.approve`
// and `plan.reject`, typed from the generated client. The back office's `call` (api.ts) keeps only
// the error's sentence; this page needs the rest of the answer (the code, who decided and when, the
// record's version now), so the calls read the whole body here.
//
// The types come straight from the generated schema of `@obop/client`. The back office does not
// depend on that package yet (adding it changes the lockfile); a type-only import is erased at
// build time, so nothing of the client is bundled.
import type { components } from "../../../../packages/client/src/generated/schema.d.ts";
import { languageHeader, withLanguage } from "../api.ts";
import { NetworkError, request } from "../net.ts";

export type PlanRow = components["schemas"]["PlanRow"];
export type PlanDetail = components["schemas"]["PlanDetail"];
export type PlanChange = components["schemas"]["PlanChange"];
export type PlanCount = components["schemas"]["PlanCount"];
export type PlanStatus = PlanRow["status"];
export type PlanKind = PlanRow["kind"];

/** What a refused call said: the status, the stable code and what the body adds for this page. */
export type PlanFail = {
  ok: false;
  /** 0 when the server could not be reached. */
  status: number;
  /** `not_found`, `precondition_failed`, `already_decided`, `bad_request`, `network`, ... */
  code: string;
  /** The server's sentence (shown through serverMessage.ts, never as an instruction). */
  error: string;
  /** `already_decided`: approved, rejected or expired. */
  state?: string;
  decided_by?: string | null;
  decided_at?: string | null;
  /** `precondition_failed`: the version the record has now. */
  current_version?: number | null;
};
export type PlanReply<T> = { ok: true; value: T } | PlanFail;

/** The three reads may be repeated after a dropped connection; a decision never is. */
const READS = new Set(["count", "list", "get"]);

async function post<T>(token: string, command: string, body: object): Promise<PlanReply<T>> {
  let res: Response;
  try {
    res = await request(
      withLanguage(`/api/plan/${command}`),
      {
        method: "POST",
        headers: {
          authorization: `Bearer ${token}`,
          "content-type": "application/json",
          ...languageHeader(),
        },
        body: JSON.stringify(body),
      },
      { retry: READS.has(command) },
    );
  } catch (e) {
    if (e instanceof NetworkError) return { ok: false, status: 0, code: "network", error: "" };
    throw e;
  }
  const json = (await res.json().catch(() => ({}))) as Record<string, unknown>;
  // A plan command never needs approval; an answer of 202 would be a server out of step.
  if (res.ok && res.status !== 202) return { ok: true, value: json.value as T };
  return {
    ok: false,
    status: res.status,
    code: typeof json.code === "string" ? json.code : res.status === 404 ? "not_found" : "",
    error: typeof json.error === "string" ? json.error : "",
    ...(typeof json.state === "string" ? { state: json.state } : {}),
    ...("decided_by" in json ? { decided_by: (json.decided_by as string | null) ?? null } : {}),
    ...("decided_at" in json ? { decided_at: (json.decided_at as string | null) ?? null } : {}),
    ...(typeof json.current_version === "number" ? { current_version: json.current_version } : {}),
  };
}

/** How many plans wait for this person (`waiting`), and whether they decide any (`can_decide`). */
export const planCount = (token: string) => post<PlanCount>(token, "count", {});

/** One page of the person's plans: waiting (oldest first) or decided (newest decision first). */
export const planList = (token: string, state: "waiting" | "decided") =>
  post<{ rows: PlanRow[]; total: number }>(token, "list", { state, limit: 50 });

/** One plan with its change, read as this person. */
export const planGet = (token: string, id: string) => post<PlanDetail>(token, "get", { id });

/**
 * Approve one plan. `seenVersion` is the version of the record the person was shown: a record that
 * changed since is refused with 412 and nothing runs.
 */
export const planApprove = (token: string, id: string, seenVersion: number | null | undefined) =>
  post<unknown>(
    token,
    "approve",
    typeof seenVersion === "number" ? { id, seen_version: seenVersion } : { id },
  );

/** Reject one plan, with the person's reason for the agent when they gave one. */
export const planReject = (token: string, id: string, reason: string) =>
  post<PlanRow>(token, "reject", reason.trim() ? { id, reason: reason.trim() } : { id });

/** The most characters a reason may have (the server counts characters, not UTF-16 units). */
export const REASON_MAX = 300;
/** Characters as the server counts them. */
export const reasonLength = (s: string): number => Array.from(s.trim()).length;
