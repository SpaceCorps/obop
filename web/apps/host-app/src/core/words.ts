// Which sentence of the catalogue goes with what the reducer says. Kept apart from the screens so
// that `vp test` holds every sentence to its case.
import { hostAndPort } from "./address.ts";
import type { Build } from "./build.ts";
import type { MessageKey } from "../i18n/index.ts";
import type { AddressNote, WebProblem } from "./shell.ts";
import { WATCHDOG_MS } from "./shell.ts";

export interface Sentence {
  readonly key: MessageKey;
  readonly params: Record<string, string | number>;
}

/** The sentence under the address field. A forward that is offered is a banner, not this. */
export function addressSentence(note: AddressNote | null, build: Build): Sentence | null {
  if (note === null || note.kind === "forwarded") return null;
  if (note.kind === "storage") return { key: "address.storage", params: {} };
  if (note.kind === "probe") {
    return { key: `address.probe.${note.problem}`, params: { host: note.host } };
  }
  if (note.problem === "scheme" && build.cleartext) {
    return { key: "address.problem.scheme.test", params: {} };
  }
  return { key: `address.problem.${note.problem}`, params: {} };
}

/** The sentence of the error screen, and what the phone itself reported when it said something. */
export function errorSentence(
  problem: WebProblem,
  origin: string,
): { readonly main: Sentence; readonly hint: Sentence | null; readonly detail: string | null } {
  const host = hostAndPort(origin);
  switch (problem.kind) {
    case "no_answer":
      return {
        main: { key: "error.no_answer", params: { host } },
        hint: null,
        detail: problem.detail === "" ? null : problem.detail,
      };
    case "tls":
      return {
        main: { key: "error.tls", params: { host } },
        hint: null,
        detail: problem.detail === "" ? null : problem.detail,
      };
    case "server_fault":
      return {
        main: { key: "error.server_fault", params: { host, status: problem.status } },
        hint: null,
        detail: null,
      };
    case "timeout":
      return {
        main: { key: "error.timeout", params: { host, seconds: WATCHDOG_MS / 1000 } },
        hint: null,
        detail: null,
      };
    case "forwards":
      return {
        main: { key: "error.forwards", params: { host, to: problem.to } },
        hint: { key: "error.forwards.hint", params: { to: problem.to } },
        detail: null,
      };
  }
}
