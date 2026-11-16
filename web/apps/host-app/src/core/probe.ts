// The check of a typed address: one GET of the instance's API description, without a token, and
// what its answer means.
//
// What the check proves: that obop software answers at the address. What it does not prove: whose
// server it is. The document is open to everyone, so any server can return a copy; only the person
// knows which host is theirs, and the host bar tells them which one the app is on.
import { originOf } from "@obop/host-bridge";
import { addressText, isCleartext, parseAddress, type Address } from "./address.ts";

/** The document every obop instance serves to every caller, signed in or not. */
export const PROBE_PATH = "/api/openapi.json";
/** How long the app waits for the answer. */
export const PROBE_DEADLINE_MS = 8_000;
/** A longer answer is not read as the document (an instance's is some tens of thousands). */
export const PROBE_MAX_BODY_CHARS = 4_000_000;
/** The longest core version the app keeps. */
const MAX_VERSION_CHARS = 64;

export function probeUrl(origin: string): string {
  return `${origin}${PROBE_PATH}`;
}

/** What came back, as the native side reports it. `final` is the address after every redirect. */
export interface ProbeAnswer {
  readonly status: number;
  readonly contentType: string;
  readonly body: string;
  readonly final: string;
}
export type ProbeOutcome =
  | { readonly answered: false }
  | ({ readonly answered: true } & ProbeAnswer);

export type ProbeProblem =
  /** No answer in time, no route, or a TLS failure. */
  | "no_answer"
  /** Something answers, but not with the document. */
  | "not_obop"
  /**
   * A 5xx: the server, or a proxy in front of it, answers with a fault. A proxy answers 502 or 504
   * while the instance behind it restarts, so this is not "not an obop server".
   */
  | "busy"
  /** The address forwards to one that is not encrypted, or to one the address rule refuses. */
  | "forward_cleartext";

export type ProbeVerdict =
  | { readonly kind: "accepted"; readonly coreVersion: string }
  /** The address forwards to `address` (as a person types it), which passed the address rule. */
  | { readonly kind: "forwarded"; readonly address: string; readonly origin: string }
  | { readonly kind: "refused"; readonly problem: ProbeProblem };

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** The core version when `body` is the API description of an obop instance, else `null`. */
function documentVersion(body: string): string | null {
  if (body.length > PROBE_MAX_BODY_CHARS) return null;
  let value: unknown;
  try {
    value = JSON.parse(body);
  } catch {
    return null;
  }
  if (!isRecord(value)) return null;
  if (typeof value.openapi !== "string") return null;
  const info = value.info;
  if (!isRecord(info) || typeof info.version !== "string") return null;
  if (info.version === "" || info.version.length > MAX_VERSION_CHARS) return null;
  if (!isRecord(value["x-mcp"])) return null;
  const paths = value.paths;
  if (!isRecord(paths) || !Object.prototype.hasOwnProperty.call(paths, "/api/auth/login")) {
    return null;
  }
  return info.version;
}

/**
 * What one answer means for the address `requested`.
 *
 * A final address on another origin is a forward, and a forwarded address is treated as newly
 * typed: it goes through `parseAddress` with the same build options, so the scheme rule and the
 * table of local addresses apply to it. A change from https to http is never followed. The verdict
 * `forwarded` only offers the address; nothing is stored until a probe of it ended on itself.
 */
export function classifyProbe(
  input: ProbeAnswer & { readonly requested: Address; readonly cleartext: boolean },
): ProbeVerdict {
  const { status, contentType, body, requested, final, cleartext } = input;
  // A native side that does not say where the answer came from says nothing about a forward; the
  // WebView's own rule then catches a start address that leaves its origin.
  const finalOrigin = final === "" ? requested.origin : originOf(final);
  if (finalOrigin === null) return { kind: "refused", problem: "not_obop" };
  if (finalOrigin !== requested.origin) {
    if (!isCleartext(requested.origin) && isCleartext(finalOrigin)) {
      return { kind: "refused", problem: "forward_cleartext" };
    }
    const forwarded = parseAddress(`${finalOrigin}${requested.path}`, { cleartext });
    if (!forwarded.ok) {
      return {
        kind: "refused",
        problem: isCleartext(finalOrigin) ? "forward_cleartext" : "not_obop",
      };
    }
    return { kind: "forwarded", address: addressText(forwarded), origin: forwarded.origin };
  }
  if (status >= 500 && status <= 599) return { kind: "refused", problem: "busy" };
  if (status !== 200 || !/\bjson\b/i.test(contentType)) {
    return { kind: "refused", problem: "not_obop" };
  }
  const coreVersion = documentVersion(body);
  if (coreVersion === null) return { kind: "refused", problem: "not_obop" };
  return { kind: "accepted", coreVersion };
}

/** The verdict of one probe: no answer at all, or what the answer means. */
export function probeVerdict(
  outcome: ProbeOutcome,
  requested: Address,
  cleartext: boolean,
): ProbeVerdict {
  if (!outcome.answered) return { kind: "refused", problem: "no_answer" };
  return classifyProbe({ ...outcome, requested, cleartext });
}
