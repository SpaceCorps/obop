/**
 * How a call to an obop instance fails, in one class, and when repeating it is safe.
 *
 * The server answers a failure with JSON `{ error, code?, hint?, assert_key?, applied? }` and sets `Retry-After`
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

/** The longest wait the client believes a `Retry-After` for: one hour. */
export const MAX_RETRY_AFTER_SECONDS = 60 * 60;

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
// IMF-fixdate, the HTTP date a server should send: "Tue, 27 Oct 2026 12:00:30 GMT".
const IMF_FIXDATE =
  /^(?:Mon|Tue|Wed|Thu|Fri|Sat|Sun), (\d{2}) (Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec) (\d{4}) (\d{2}):(\d{2}):(\d{2}) GMT$/;

/**
 * The seconds a `Retry-After` header asks for, as HTTP defines it: delta-seconds (digits only) or
 * an HTTP date in the IMF-fixdate form (counted from `now`; a date in the past is 0). Anything
 * else (empty, negative, fractional, in exponent form, any other date format) is `undefined`: a
 * header nobody can read is not a request to retry at once. A wait over `MAX_RETRY_AFTER_SECONDS`
 * (a proxy's mistake, or a number too large for a timer) is cut to it. The dates are read by hand,
 * not with `Date.parse`, which accepts all sorts of text and differs between JavaScript engines.
 */
export function parseRetryAfter(
  value: string | null | undefined,
  now = Date.now(),
): number | undefined {
  if (value == null) return undefined;
  const text = value.trim();
  let seconds: number;
  const date = IMF_FIXDATE.exec(text);
  if (/^\d+$/.test(text)) {
    seconds = Number(text);
  } else if (date) {
    const day = Number(date[1]);
    const hour = Number(date[4]);
    const second = Number(date[6]);
    const at = new Date(
      Date.UTC(Number(date[3]), MONTHS.indexOf(date[2]!), day, hour, Number(date[5]), second),
    );
    // 31 Feb, 25:00 and the like would roll over into another date: unreadable.
    if (at.getUTCDate() !== day || at.getUTCHours() !== hour || second > 59) return undefined;
    seconds = Math.max(0, Math.ceil((at.getTime() - now) / 1000));
  } else {
    return undefined;
  }
  return Math.min(seconds, MAX_RETRY_AFTER_SECONDS);
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
  assertKey?: string;
  applied?: Applied;
  retryAfterSeconds?: number;
  /** The parsed JSON body of the answer, or its text when it was not JSON. */
  body?: unknown;
  cause?: unknown;
}

/**
 * Every failed call rejects with this: an answer with a status of 400 or more, a success status
 * whose body is not an obop answer (`code` `invalid_response`, see `invalidResponse`), or (as the
 * subclass `ObopNetworkError`) no answer at all.
 *
 * - `status`: the HTTP status, 0 when there was no answer.
 * - `code`: the stable machine word of the server (`not_found`, `precondition_failed`,
 *   `storage_busy`, `mfa_enrolment_required` and so on) when it sent one. The older refusals of a
 *   command carry none: only `message` says what was wrong.
 * - `hint`: what the server suggests doing about it.
 * - `assertKey`: with `code` `assertion_failed`, the key the package gave the assertion that
 *   refused (when it has one), so an app that translates by itself does not match the text. The
 *   text is in the language `getLanguage` asked for where the package translated it.
 * - `applied`: on a 503, what became of the request's own changes (`none`: nothing, `unknown`: a
 *   change in flight may have been applied, `partial`: part of it was). From the body, else the
 *   `X-Obop-Applied` header; `undefined` on every other answer.
 * - `retryAfterSeconds`: the wait the server asked for.
 */
export class ObopApiError extends Error {
  readonly status: number;
  readonly code: string | undefined;
  readonly hint: string | undefined;
  readonly assertKey: string | undefined;
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
    this.assertKey = init.assertKey;
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
      assertKey: str(json?.assert_key),
      applied: parseApplied(json?.applied) ?? parseApplied(response.headers.get("x-obop-applied")),
      retryAfterSeconds: parseRetryAfter(response.headers.get("retry-after")),
      body,
      ...call,
    });
  }
}

/**
 * The server answered with a success status, but what it sent is not what an obop instance sends
 * (not JSON, or JSON of another shape): typically the address is not an obop instance, or a proxy
 * or captive portal answered in its place. `code` is `invalid_response`, `status` is the one the
 * server sent and `body` is the start of the text. A write that gets this was answered, so it is
 * not repeated blindly: `isSafeToRetry` is false for every status below 500.
 */
export function invalidResponse(response: Response, text: string, call?: CallInfo): ObopApiError {
  return new ObopApiError({
    status: response.status,
    message: "The server did not answer like an obop instance. Is the address right?",
    code: "invalid_response",
    body: text.slice(0, 2000),
    ...call,
  });
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
 * Whether the same call may be sent again without risking a change twice.
 *
 * **From docs/DEPLOY.md "What a 503 means" (D74)**, the three words of `applied`:
 *
 * - `none`: nothing was changed; "repeating the request is safe";
 * - `unknown` (`storage_unavailable`): a change in flight may or may not have been applied, "read
 *   before you repeat a write". A call that carries an idempotency key (`bulk_create` has one, a
 *   single create or update has none) can be repeated without reading: the repeat returns the
 *   first answer or creates the rows once. A call that only reads can always be repeated;
 * - `partial`: a change was committed and a step it needed was refused: "do not repeat a partial
 *   request blindly: read the records it touched and finish or undo the rest". So never here,
 *   with or without a key (a keyed repeat of `bulk_create` returns the stored first answer and
 *   finishes nothing).
 *
 * **Chosen by this client, not written in DEPLOY.md:** a 5xx with no word (a gateway, a crash) and
 * a lost connection are treated like `unknown` for a write (a lost connection of a read is
 * `none`); a 429 was refused before it ran and can be repeated after `retryAfterSeconds`; every
 * other status (400, 401, 403, 404, 409, 410, 412, a 2xx that could not be read ...) never,
 * because a repeat gets the same answer or does the work twice.
 *
 * `idempotent` says the call carries an idempotency key; it defaults to whether the failed call
 * only read (`err.read`). Anything that is not an `ObopApiError` is not safe to repeat.
 */
export function isSafeToRetry(err: unknown, opts: { idempotent?: boolean } = {}): boolean {
  if (!(err instanceof ObopApiError)) return false;
  if (err.status === 429) return true;
  if (err.status !== 0 && err.status < 500) return false;
  if (err.applied === "partial") return false;
  if (err.applied === "none") return true;
  return opts.idempotent ?? err.read;
}

/**
 * How long to wait before repeating, in milliseconds: what the server asked for (at most
 * `MAX_RETRY_AFTER_SECONDS`), else `fallbackMs`. A value that is not a number of seconds from zero
 * up is not what the server asked for.
 */
export function retryDelayMs(err: ObopApiError, fallbackMs = 1000): number {
  const s = err.retryAfterSeconds;
  if (s === undefined || !Number.isFinite(s) || s < 0) return fallbackMs;
  return Math.min(s, MAX_RETRY_AFTER_SECONDS) * 1000;
}
