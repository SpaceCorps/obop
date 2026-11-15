// The reducer of the app's screens: boot, address, probing, web, error, and what lies over the
// page (the host bar, "This app", the cover). It is pure: timers, storage, the probe and the
// WebView are the caller's, which sends what happened as events and reads what to do from the
// state. Everything a timer or a WebView reports carries a number (a ticket, the mount), so an
// event of something that is no longer current changes nothing.
//
// Three rules the tests hold:
//   - No event of a page (a navigation, a load, an error, a bridge command) hides the host bar or
//     removes the way to "This app". Only the person's touch and the two seconds do.
//   - The watchdog is armed by the app's own actions only (a mount, Retry, a reload the app asks
//     for) and cleared by the next load end or load error. A navigation never arms it: on Android
//     every route change of the page is reported as a load start with no load end after it.
//   - A foreign document on top silences the bridge. After the first load of a mount it also gets
//     the WebView mounted again; before it, the error screen says where the address forwards to.
import { originOf } from "@obop/host-bridge";
import {
  addressText,
  hostAndPort,
  parseAddress,
  startAddress,
  type Address,
  type AddressProblem,
} from "./address.ts";
import type { Build } from "./build.ts";
import { decideNavigation, isStartDocument, schemeOf, topVerdict } from "./nav-policy.ts";
import type { ProbeProblem, ProbeVerdict } from "./probe.ts";
import { decodeServerRecord, type ServerRecord } from "./server-record.ts";
import type { PageTheme } from "./theme.ts";

/** The host bar stays at least this long, and then until the person next touches the page. */
export const BAR_MIN_MS = 2_000;
/** How long the app waits for a load it started itself before it shows the error screen. */
export const WATCHDOG_MS = 30_000;
/**
 * How long a load end must stand before the error screen goes after Retry. Android reports a
 * failed load as a load end followed by the error, so a load end alone does not say that the page
 * is there.
 */
export const SETTLE_MS = 300;
/** The longest text of the WebView's own error description that the error screen shows. */
const MAX_DETAIL_CHARS = 200;

export type Screen = "boot" | "address" | "probing" | "web" | "error";

/** What the address screen says under its field. */
export type AddressNote =
  | { readonly kind: "address"; readonly problem: AddressProblem }
  | { readonly kind: "probe"; readonly problem: ProbeProblem; readonly host: string }
  /** The address forwards to `address`, which passed the address rule: offered, not stored. */
  | {
      readonly kind: "forwarded";
      readonly host: string;
      readonly origin: string;
      readonly address: string;
    }
  /** The phone did not store the address. */
  | { readonly kind: "storage" };

/** Why the error screen is shown. */
export type WebProblem =
  | { readonly kind: "no_answer"; readonly detail: string }
  | { readonly kind: "tls"; readonly detail: string }
  | { readonly kind: "server_fault"; readonly status: number }
  | { readonly kind: "timeout" }
  /** The start address leads to another origin; `to` is that origin, or the scheme of the address. */
  | { readonly kind: "forwards"; readonly to: string };

export interface BarState {
  readonly shown: boolean;
  /** The ticket of this showing; the caller's two-second timer carries it. */
  readonly timer: number;
  /** The person touched the page since the bar was shown. */
  readonly touched: boolean;
  /** The two seconds are over. */
  readonly minElapsed: boolean;
}

export interface ShellState {
  readonly build: Build;
  readonly screen: Screen;
  /** The text of the address field. */
  readonly field: string;
  readonly note: AddressNote | null;
  /** The address being checked; the caller's probe carries `id`. */
  readonly probe: { readonly id: number; readonly address: Address } | null;
  readonly server: ServerRecord | null;
  /** The React key of the WebView: a new number is a new WebView. */
  readonly mount: number;
  /** Whose document is on top, from navigation events only. */
  readonly top: "instance" | "foreign";
  /** A document of the instance was on top in this mount. */
  readonly committed: boolean;
  /** A load of the instance has ended in this mount, and no load error came since. */
  readonly loadedOnce: boolean;
  readonly canGoBack: boolean;
  /** The ticket of the armed watchdog, or `null`. */
  readonly watchdog: number | null;
  /** How many reloads the app has asked for; the caller reloads the WebView when it grows. */
  readonly reloads: number;
  /** Retry was tapped and its load has not ended in an error or stood for `SETTLE_MS`. */
  readonly retrying: boolean;
  /** The ticket of the settle timer after a load end during Retry, or `null`. */
  readonly settle: number | null;
  readonly problem: WebProblem | null;
  readonly settingsOpen: boolean;
  readonly bar: BarState;
  /** What the page reported with `host.theme`, or `null` before the first report of a mount. */
  readonly theme: PageTheme | null;
  /** An address to hand to the system browser; the caller opens it when `id` is new. */
  readonly external: { readonly id: number; readonly url: string } | null;
  /** The last ticket handed out. */
  readonly ticket: number;
}

export type ShellEvent =
  /** The stored item was read (`null`: none). */
  | { readonly type: "booted"; readonly stored: string | null }
  | { readonly type: "field_changed"; readonly text: string }
  /** Connect was tapped. */
  | { readonly type: "connect" }
  /** The probe ended without an accepted address. */
  | { readonly type: "probe_done"; readonly probe: number; readonly verdict: ProbeVerdict }
  /** The probe accepted the address and the phone stored `record`. */
  | { readonly type: "connected"; readonly probe: number; readonly record: ServerRecord }
  /** The probe accepted the address and the phone could not store it. */
  | { readonly type: "store_failed"; readonly probe: number }
  /** The button under "This address forwards to" was tapped. */
  | { readonly type: "use_forwarded" }
  /** The WebView reported a load start or a navigation state. */
  | {
      readonly type: "nav";
      readonly mount: number;
      readonly url: string;
      readonly canGoBack: boolean;
    }
  | { readonly type: "load_end"; readonly mount: number; readonly url: string }
  | {
      readonly type: "load_error";
      readonly mount: number;
      readonly code: number;
      readonly description: string;
    }
  | {
      readonly type: "http_error";
      readonly mount: number;
      readonly url: string;
      readonly status: number;
    }
  /** The WebView asked whether to load an address that `requestVerdict` called `forwarded`. */
  | { readonly type: "forwarded"; readonly mount: number; readonly url: string }
  /** Android: the render process of the WebView is gone. */
  | { readonly type: "process_gone"; readonly mount: number }
  /** iOS: the content process of the WebView ended. */
  | { readonly type: "content_terminated"; readonly mount: number }
  | { readonly type: "watchdog_fired"; readonly ticket: number }
  | { readonly type: "settled"; readonly ticket: number }
  /** Retry was tapped on the error screen. */
  | { readonly type: "retry" }
  /** The person touched the page (the wrapper of the WebView saw a touch start). */
  | { readonly type: "page_touched" }
  | { readonly type: "bar_min_elapsed"; readonly ticket: number }
  /** The app came back to the foreground. */
  | { readonly type: "foreground" }
  /** The page reported its theme with `host.theme`. */
  | { readonly type: "theme"; readonly theme: PageTheme }
  | { readonly type: "open_settings" }
  | { readonly type: "close_settings" }
  /** Change server was confirmed and the stored item removed. */
  | { readonly type: "change_server" };

const NO_BAR: BarState = { shown: false, timer: 0, touched: false, minElapsed: false };

export function initialState(build: Build): ShellState {
  return {
    build,
    screen: "boot",
    field: "",
    note: null,
    probe: null,
    server: null,
    mount: 0,
    top: "instance",
    committed: false,
    loadedOnce: false,
    canGoBack: false,
    watchdog: null,
    reloads: 0,
    retrying: false,
    settle: null,
    problem: null,
    settingsOpen: false,
    bar: NO_BAR,
    theme: null,
    external: null,
    ticket: 0,
  };
}

type WebState = ShellState & { readonly server: ServerRecord };

/** The WebView is mounted, or would be: the page or the error screen over it. */
function onWeb(state: ShellState): state is WebState {
  return (state.screen === "web" || state.screen === "error") && state.server !== null;
}

/** The event is of the WebView that is mounted now. */
function live(state: ShellState, mount: number): state is WebState {
  return onWeb(state) && state.mount === mount;
}

/** The host bar is shown from now: at every mount, at every return to the app. */
function showBar(state: ShellState): ShellState {
  const ticket = state.ticket + 1;
  return {
    ...state,
    ticket,
    bar: { shown: true, timer: ticket, touched: false, minElapsed: false },
  };
}

/** The watchdog runs from now. Only the app's own actions call this. */
function arm(state: ShellState): ShellState {
  const ticket = state.ticket + 1;
  return { ...state, ticket, watchdog: ticket };
}

/** A new WebView at the start address. `top` is left as it is: only a load end makes it `instance` again. */
function remount(state: ShellState): ShellState {
  return showBar(
    arm({
      ...state,
      mount: state.mount + 1,
      committed: false,
      loadedOnce: false,
      canGoBack: false,
      theme: null,
    }),
  );
}

function fail(state: ShellState, problem: WebProblem): ShellState {
  return { ...state, screen: "error", problem, watchdog: null, retrying: false, settle: null };
}

function toAddress(state: ShellState, field: string, note: AddressNote | null): ShellState {
  return {
    ...initialState(state.build),
    screen: "address",
    field,
    note,
    mount: state.mount,
    reloads: state.reloads,
    ticket: state.ticket,
  };
}

function enterWeb(state: ShellState, record: ServerRecord): ShellState {
  return remount({
    ...initialState(state.build),
    screen: "web",
    server: record,
    mount: state.mount,
    reloads: state.reloads,
    ticket: state.ticket,
  });
}

/** Where a foreign address leads, for the error screen: its origin, or its scheme. */
function destinationOf(url: string): string {
  return originOf(url) ?? `${schemeOf(url) ?? "?"}:`;
}

function isHttp(url: string): boolean {
  const scheme = schemeOf(url);
  return scheme === "http" || scheme === "https";
}

/** A TLS failure as the two systems report one: Android in words, iOS by its error codes. */
function isTlsFailure(code: number, description: string): boolean {
  return /^SSL error/i.test(description) || (code <= -1200 && code >= -1206);
}

export function reduce(state: ShellState, event: ShellEvent): ShellState {
  switch (event.type) {
    case "booted": {
      if (state.screen !== "boot") return state;
      const record = decodeServerRecord(event.stored);
      if (record === null) return toAddress(state, state.build.defaultServer, null);
      // The address rule runs on the stored address at every start: a build that speaks https
      // only does not open an http address that a test build stored.
      const checked = parseAddress(addressText(record), { cleartext: state.build.cleartext });
      if (!checked.ok) {
        return toAddress(state, addressText(record), { kind: "address", problem: checked.problem });
      }
      return enterWeb(state, record);
    }
    case "field_changed":
      if (state.screen !== "address") return state;
      return { ...state, field: event.text, note: null };
    case "connect": {
      if (state.screen !== "address") return state;
      const parsed = parseAddress(state.field, { cleartext: state.build.cleartext });
      if (!parsed.ok) return { ...state, note: { kind: "address", problem: parsed.problem } };
      const id = state.ticket + 1;
      return {
        ...state,
        ticket: id,
        screen: "probing",
        note: null,
        probe: { id, address: { origin: parsed.origin, path: parsed.path } },
      };
    }
    case "probe_done": {
      if (state.screen !== "probing" || state.probe?.id !== event.probe) return state;
      const host = hostAndPort(state.probe.address.origin);
      const { verdict } = event;
      // An accepted address comes as `connected`, after the phone stored it.
      if (verdict.kind === "accepted") return state;
      const note: AddressNote =
        verdict.kind === "forwarded"
          ? { kind: "forwarded", host, origin: verdict.origin, address: verdict.address }
          : { kind: "probe", problem: verdict.problem, host };
      return { ...state, screen: "address", probe: null, note };
    }
    case "connected":
      if (state.screen !== "probing" || state.probe?.id !== event.probe) return state;
      return enterWeb(state, event.record);
    case "store_failed":
      if (state.screen !== "probing" || state.probe?.id !== event.probe) return state;
      return { ...state, screen: "address", probe: null, note: { kind: "storage" } };
    case "use_forwarded": {
      // The button only puts the offered address into the field and probes it again: an address
      // is stored only when a probe of it ended on itself.
      if (state.screen !== "address" || state.note?.kind !== "forwarded") return state;
      return reduce({ ...state, field: state.note.address, note: null }, { type: "connect" });
    }
    case "nav": {
      if (!live(state, event.mount)) return state;
      const verdict = topVerdict(event.url, state.server.origin);
      if (verdict === "neutral") return { ...state, canGoBack: event.canGoBack };
      if (verdict === "instance") {
        return { ...state, canGoBack: event.canGoBack, committed: true };
      }
      // A foreign document is on top. From here the bridge answers nothing.
      if (!state.loadedOnce) {
        // The start address itself led there: nothing is mounted again.
        return fail(
          { ...state, top: "foreign", canGoBack: false },
          { kind: "forwards", to: destinationOf(event.url) },
        );
      }
      const again = remount({
        ...state,
        top: "foreign",
        screen: "web",
        problem: null,
        retrying: false,
        settle: null,
      });
      if (!isHttp(event.url)) return again;
      const id = again.ticket + 1;
      return { ...again, ticket: id, external: { id, url: event.url } };
    }
    case "load_end": {
      if (!live(state, event.mount)) return state;
      const cleared: ShellState = { ...state, watchdog: null };
      if (originOf(event.url) !== state.server.origin) return cleared;
      const loaded: ShellState = { ...cleared, top: "instance", committed: true, loadedOnce: true };
      if (state.screen !== "error" || !state.retrying) return loaded;
      const ticket = loaded.ticket + 1;
      return { ...loaded, ticket, settle: ticket };
    }
    case "load_error": {
      if (!live(state, event.mount)) return state;
      const detail = event.description.slice(0, MAX_DETAIL_CHARS);
      return fail(
        { ...state, loadedOnce: false },
        isTlsFailure(event.code, event.description)
          ? { kind: "tls", detail }
          : { kind: "no_answer", detail },
      );
    }
    case "http_error": {
      if (!live(state, event.mount)) return state;
      if (event.status < 500 || !isStartDocument(event.url, startAddress(state.server))) {
        return state;
      }
      return fail(state, { kind: "server_fault", status: event.status });
    }
    case "forwarded":
      if (!live(state, event.mount) || state.committed) return state;
      return fail(state, { kind: "forwards", to: destinationOf(event.url) });
    case "process_gone":
      if (!live(state, event.mount)) return state;
      return remount({ ...state, screen: "web", problem: null, retrying: false, settle: null });
    case "content_terminated":
      if (!live(state, event.mount)) return state;
      return arm({ ...state, reloads: state.reloads + 1 });
    case "watchdog_fired":
      if (!onWeb(state) || state.watchdog !== event.ticket) return state;
      return fail(state, { kind: "timeout" });
    case "settled":
      if (state.screen !== "error" || state.settle !== event.ticket) return state;
      return { ...state, screen: "web", problem: null, retrying: false, settle: null };
    case "retry": {
      if (state.screen !== "error" || state.retrying) return state;
      const trying: ShellState = { ...state, retrying: true, settle: null };
      // A reload keeps the WebView and what the page holds in it. Without a document of the
      // instance to reload (a foreign one on top, or nothing yet) the WebView is mounted again.
      if (state.top === "foreign" || !state.committed) return remount(trying);
      return arm({ ...trying, reloads: state.reloads + 1 });
    }
    case "page_touched":
      if (!state.bar.shown) return state;
      if (state.bar.minElapsed) return { ...state, bar: { ...state.bar, shown: false } };
      return { ...state, bar: { ...state.bar, touched: true } };
    case "bar_min_elapsed":
      if (!state.bar.shown || state.bar.timer !== event.ticket) return state;
      if (state.bar.touched) return { ...state, bar: { ...state.bar, shown: false } };
      return { ...state, bar: { ...state.bar, minElapsed: true } };
    case "foreground":
      return onWeb(state) ? showBar(state) : state;
    case "theme":
      if (!onWeb(state) || state.top !== "instance") return state;
      return { ...state, theme: event.theme };
    case "open_settings":
      return onWeb(state) ? { ...state, settingsOpen: true } : state;
    case "close_settings":
      return state.settingsOpen ? { ...state, settingsOpen: false } : state;
    case "change_server":
      if (!onWeb(state)) return state;
      return toAddress(state, state.build.defaultServer, null);
  }
}

/** The WebView is on the screen tree: always on the web screens, except under "forwards to". */
export function webViewMounted(state: ShellState): boolean {
  // A foreign document that the start address led to is not kept running under the error screen.
  return onWeb(state) && !(state.screen === "error" && state.top === "foreign");
}

/**
 * Whether the bridge answers a message of the WebView with the key `mount`: that WebView is the
 * one mounted now and the document on top is the instance's. The caller asks again right before
 * it injects an answer.
 */
export function answering(state: ShellState, mount: number): boolean {
  return live(state, mount) && state.top === "instance";
}

export type RequestVerdict =
  /** The WebView loads it. */
  | "allow"
  /** Not loaded; the system opens it. */
  | "external"
  /** Not loaded; the start address forwards there (the caller sends the event `forwarded`). */
  | "forwarded"
  /** Not loaded, and nothing else. */
  | "drop";

/**
 * The answer to the WebView's question whether to load an address in the same window. An address
 * of another origin goes to the system browser once a document of the instance is on top; before
 * that it can only be where the start address forwards to, and it is not opened anywhere.
 */
export function requestVerdict(
  state: ShellState,
  request: { readonly mount: number; readonly url: string; readonly isTopFrame: boolean },
): RequestVerdict {
  if (!live(state, request.mount)) return "drop";
  const decision = decideNavigation({
    url: request.url,
    isTopFrame: request.isTopFrame,
    newWindow: false,
    origin: state.server.origin,
  });
  if (decision === "load") return "allow";
  if (decision === "refuse") return "drop";
  if (state.committed) return "external";
  return isHttp(request.url) ? "forwarded" : "drop";
}

export type WindowVerdict =
  /** An address of the instance: the one WebView goes there (`assignScript`). */
  | "assign"
  /** The system opens it. */
  | "external"
  | "drop";

/** What becomes of an address the page wanted in a new window. Only the instance's document is served. */
export function windowVerdict(
  state: ShellState,
  request: { readonly mount: number; readonly url: string },
): WindowVerdict {
  if (!live(state, request.mount) || state.top !== "instance") return "drop";
  const decision = decideNavigation({
    url: request.url,
    isTopFrame: true,
    newWindow: true,
    origin: state.server.origin,
  });
  if (decision === "load") return "assign";
  return decision === "external" ? "external" : "drop";
}
