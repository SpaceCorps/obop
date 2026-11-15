// Where an address may go: into the WebView, to the system, or nowhere. The app decides every
// navigation itself with these functions and passes `originWhitelist={["*"]}` to the WebView,
// because the library's own list is a prefix match without an end: `https://erp.example.com` would
// also pass `https://erp.example.com.evil.net`. Origins are compared as exact texts, worked out by
// `originOf` of @obop/host-bridge.
import { originOf } from "@obop/host-bridge";

export type NavDecision =
  /** The WebView loads it (for a new window: the one WebView goes there). */
  | "load"
  /** The system opens it: the browser, the mail app, the dialler. */
  | "external"
  /** Dropped. */
  | "refuse";

export interface NavRequest {
  /** The address the page wants. */
  readonly url: string;
  /** The top document asks, not a frame. Android does not say, so the app passes `true` there. */
  readonly isTopFrame: boolean;
  /** The page asked for a new window (`target="_blank"`, `window.open`). */
  readonly newWindow: boolean;
  /** The stored origin of the instance. */
  readonly origin: string;
}

/** Schemes the system has an app for and that carry nothing of the session. */
const SYSTEM_SCHEMES: readonly string[] = ["mailto", "tel", "sms", "geo"];

/** The scheme of an address in lower case, or `null`. */
export function schemeOf(url: string): string | null {
  const match = /^([A-Za-z][A-Za-z0-9+.-]*):/.exec(url);
  return match === null ? null : (match[1] ?? "").toLowerCase();
}

/** `about:blank` or `about:srcdoc`, with or without a fragment or a query. */
function isEmptyDocument(url: string): boolean {
  return /^about:(blank|srcdoc)([?#].*)?$/i.test(url);
}

/** A `blob:` address of `origin`: `blob:https://erp.example.com/<id>`. */
function isBlobOf(url: string, origin: string): boolean {
  return originOf(url.slice("blob:".length)) === origin;
}

/**
 * The one decision about a navigation.
 *
 * Loads in the WebView: an http or https address of the stored origin, `about:blank`,
 * `about:srcdoc`, a `blob:` of that origin in the same window, and any load that is not the top
 * frame (iOS only; the back office embeds no frame). To the system: http and https of another
 * origin, `mailto:`, `tel:`, `sms:` and `geo:`. Everything else is dropped: `intent:`,
 * `javascript:`, `data:`, `file:`, `content:`, a `blob:` in a new window, an unknown scheme.
 */
export function decideNavigation(request: NavRequest): NavDecision {
  const { url, isTopFrame, newWindow, origin } = request;
  if (!isTopFrame && !newWindow) return "load";
  const scheme = schemeOf(url);
  if (scheme === null) return "refuse";
  if (scheme === "http" || scheme === "https") {
    return originOf(url) === origin ? "load" : "external";
  }
  if (SYSTEM_SCHEMES.includes(scheme)) return "external";
  if (newWindow) return "refuse";
  if (scheme === "about") return isEmptyDocument(url) ? "load" : "refuse";
  if (scheme === "blob") return isBlobOf(url, origin) ? "load" : "refuse";
  return "refuse";
}

export type TopVerdict =
  /** A document of the instance: an http or https address of the stored origin, or a `blob:` of it. */
  | "instance"
  /** No document of anybody: `about:blank`, or the WebView's own page for a failed load. */
  | "neutral"
  /** A document the host does not trust. */
  | "foreign";

/**
 * What a top-level address that the WebView reports (a load start, a navigation state) says about
 * the document on top. `foreign` is every address `decideNavigation` would not load in the top
 * frame, with one exception: the page Android's WebView shows for a failed load
 * (`chrome-error://chromewebdata/`) is the WebView's own and is followed by the error itself.
 */
export function topVerdict(url: string, origin: string): TopVerdict {
  if (isEmptyDocument(url) || /^chrome-error:\/\//i.test(url)) return "neutral";
  const decision = decideNavigation({ url, isTopFrame: true, newWindow: false, origin });
  return decision === "load" ? "instance" : "foreign";
}

/** The address of a document of the instance, apart from its fragment, is the start address. */
export function isStartDocument(url: string, start: string): boolean {
  const hash = url.indexOf("#");
  return (hash < 0 ? url : url.slice(0, hash)) === start;
}

/**
 * The script that sends the one WebView to an address of the instance that the page wanted in a
 * new window. The address is passed as a string literal; `<`, `>` and the two Unicode line
 * separators are escaped, as in `deliveryScript` of the bridge. A fragment address then changes
 * the route without a reload.
 */
export function assignScript(url: string): string {
  const literal = JSON.stringify(url).replace(
    /[<>\u2028\u2029]/g,
    (character) => `\\u${character.charCodeAt(0).toString(16).padStart(4, "0")}`,
  );
  return `(function(){try{window.location.assign(${literal});}catch(e){}})();true;`;
}
