/**
 * How a call to an obop instance fails, in one class, and when repeating it is safe.
 *
 * The server answers a failure with JSON `{ error, code?, hint?, applied? }` and sets `Retry-After`
 * and `X-Obop-Applied` on a 503 (D74, docs/DEPLOY.md "What a 503 means"). Nothing here reads a
 * message to decide anything: `status`, `code` and `applied` do.
 */

/** What became of the request's own changes when the store refused it (D74). */
export type Applied = "none" | "unknown" | "partial";

const APPLIED: readonly string[] = ["none", "unknown", "partial"];

/** The word as the server wrote it, or `undefined` when it is absent or is not one of the three. */
export function parseApplied(value: unknown): Applied | undefined {
  return typeof value === "string" && APPLIED.includes(value) ? (value as Applied) : undefined;
}

/**
 * The seconds a `Retry-After` header asks for: whole seconds, or an HTTP date (counted from now).
 * `undefined` when the header is absent or unreadable.
 */
export function parseRetryAfter(
  value: string | null | undefined,
  now = Date.now(),
): number | undefined {
  if (value == null) return undefined;
  const text = value.trim();
  if (/^\d+$/.test(text)) return Number(text);
  const at = Date.parse(text);
  return Number.isNaN(at) ? undefined : Math.max(0, Math.ceil((at - now) / 1000));
}

/** Commands that only read: a repeat cannot change anything (`get`, `list` and `search`). */
const READ_COMMANDS: readonly string[] = ["get", "list", "search"];

/** Whether a call only reads: any GET, or a POST of `get`, `list` or `search` of a model. */
export function isReadCall(method: string, path: string): boolean {
  if (method.toUpperCase() === "GET" || method.toUpperCase() === "HEAD") return true;
  const parts = path.split("?")[0]!.split("/").filter(Boolean);
  // /api/<model>/<command>
  return parts.length === 3 && parts[0] === "api" && READ_COMMANDS.includes(parts[2]!);
}

/** What the client knows about the call that failed. */
export interface CallInfo {
  method: string;
  /** The path of the operation, for example `/api/party/list`. */
  path: string;
  /** The call only reads (see `isReadCall`). */
  read: boolean;
}

export interface ObopApiErrorInit extends Partial<CallInfo> {
  status: number;
  message: string;
  code?: string;
  hint?: string;
  applied?: Applied;
  retryAfterSeconds?: number;
  /** The parsed JSON body of the answer, or its text when it was not JSON. */
  body?: unknown;
  cause?: unknown;
}

/**
 * Every failed call rejects with this: an answer with a status of 400 or more, or (as the subclass
 * `ObopNetworkError`) no answer at all.
 *
 * - `status`: the HTTP status, 0 when there was no answer.
 * - `code`: the stable machine word of the server (`not_found`, `precondition_failed`,
 *   `storage_busy`, `mfa_enrolment_required` and so on) when it sent one. The older refusals of a
 *   command carry none: only `message` says what was wrong.
 * - `hint`: what the server suggests doing about it.
 * - `applied`: on a 503, what became of the request's own changes (`none`: nothing, `unknown`: a
 *   change in flight may have been applied, `partial`: part of it was). From the body, else the
 *   `X-Obop-Applied` header; `undefined` on every other answer.
 * - `retryAfterSeconds`: the wait the server asked for.
 */
export class ObopApiError extends Error {
  readonly status: number;
  readonly code: string | undefined;
  readonly hint: string | undefined;
  readonly applied: Applied | undefined;
  readonly retryAfterSeconds: number | undefined;
  readonly body: unknown;
  readonly method: string | undefined;
  readonly path: string | undefined;
  /** The failed call only reads, so repeating it is always safe. */
  readonly read: boolean;

  constructor(init: ObopApiErrorInit) {
    super(init.message, init.cause === undefined ? undefined : { cause: init.cause });
    this.name = "ObopApiError";
    this.status = init.status;
    this.code = init.code;
    this.hint = init.hint;
    this.applied = init.applied;
    this.retryAfterSeconds = init.retryAfterSeconds;
    this.body = init.body;
    this.method = init.method;
    this.path = init.path;
    this.read = init.read ?? false;
  }

  /** Make the error of a response that is not a success. Reads the body once. */
  static async fromResponse(response: Response, call?: CallInfo): Promise<ObopApiError> {
    let text = "";
    try {
      text = await response.text();
    } catch {
      // The body could not be read; the status still tells what happened.
    }
    let body: unknown = text;
    let json: Record<string, unknown> | undefined;
    try {
      const parsed: unknown = JSON.parse(text);
      body = parsed;
      if (parsed !== null && typeof parsed === "object" && !Array.isArray(parsed)) {
        json = parsed as Record<string, unknown>;
      }
    } catch {
      // A proxy or a crash may answer with HTML or nothing; keep the text.
    }
    const str = (v: unknown) => (typeof v === "string" && v !== "" ? v : undefined);
    const message =
      str(json?.error) ?? str(response.statusText) ?? `The server answered ${response.status}`;
    return new ObopApiError({
      status: response.status,
      message,
      code: str(json?.code),
      hint: str(json?.hint),
      applied: parseApplied(json?.applied) ?? parseApplied(response.headers.get("x-obop-applied")),
      retryAfterSeconds: parseRetryAfter(response.headers.get("retry-after")),
      body,
      ...call,
    });
  }
}

/**
 * The server could not be reached, or the connection broke before an answer came: `fetch` threw.
 * `status` is 0 and `code` is `network_error`. For a call that reads, `applied` is `none`; for a
 * call that writes it is `unknown`, because the request may have arrived and been applied.
 */
export class ObopNetworkError extends ObopApiError {
  constructor(cause: unknown, call?: CallInfo) {
    super({
      status: 0,
      message: "Could not reach the server",
      code: "network_error",
      applied: call?.read ? "none" : "unknown",
      cause,
      ...call,
    });
    this.name = "ObopNetworkError";
  }
}

/**
 * The call was understood but needs a person to approve it first (a 202 with a plan, D18). Only
 * agent tokens meet it; nothing was changed yet.
 */
export class ObopApprovalRequired extends ObopApiError {
  readonly planId: string;

  constructor(planId: string, call?: CallInfo) {
    super({
      status: 202,
      message: "A person has to approve this first",
      code: "approval_required",
      applied: "none",
      ...call,
    });
    this.name = "ObopApprovalRequired";
    this.planId = planId;
  }
}

/**
 * Whether the same call may be sent again without risking a change twice (the D74 rule of
 * docs/DEPLOY.md "What a 503 means"):
 *
 * - a 429 was refused before it ran: yes, after `retryAfterSeconds`;
 * - a 5xx or a lost connection that says `applied: none`: yes;
 * - the same with `applied: unknown` or `partial`, or no word at all (a gateway, a crash): only
 *   when the call carries an idempotency key or only reads (`idempotent`), because a plain repeat
 *   could change the data twice or finish a half-done change blindly;
 * - everything else (400, 401, 403, 404, 409, 410, 412 ...): no, a repeat gets the same answer.
 *
 * `idempotent` defaults to whether the failed call only read (`err.read`); pass `true` for a call
 * that carries an idempotency key. Anything that is not an `ObopApiError` is not safe to repeat.
 */
export function isSafeToRetry(err: unknown, opts: { idempotent?: boolean } = {}): boolean {
  if (!(err instanceof ObopApiError)) return false;
  if (err.status === 429) return true;
  if (err.status !== 0 && err.status < 500) return false;
  if (err.applied === "none") return true;
  return opts.idempotent ?? err.read;
}

/** How long to wait before repeating, in milliseconds: what the server asked for, else `fallbackMs`. */
export function retryDelayMs(err: ObopApiError, fallbackMs = 1000): number {
  return err.retryAfterSeconds === undefined ? fallbackMs : err.retryAfterSeconds * 1000;
}
