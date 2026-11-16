// The page's side of the phone host app (`web/apps/host-app`): one call from the entry, before the
// first render.
//
// A page is embedded when two facts hold, both needed: its user agent names the host
// (`ObopHost/<n>`) and the WebView's transport exists (`window.ReactNativeWebView.postMessage`).
// Then the page defines the object the host hands its answers to, marks `<html>` with
// `data-host="1"`, asks `host.info` once, and from the answer on tells the host its ground colour
// and stops the downloads the host cannot save. `data-host-info` on `<html>` says how that first
// question ended (`ok`, or the code of the failure), for whoever looks at the page from outside.
//
// Not embedded, it does nothing at all: no global, no attribute, no listener, no message.
import { BridgeError, PAGE_RECEIVER, createPageBridge } from "@obop/host-bridge";
import { startDownloadCatcher } from "./downloads.ts";
import { hostNotice } from "./notices.ts";
import { askHost, hostAnswers, leaveHost, setHostAsk, setHostState } from "./state.ts";
import { canvasColour, startThemeReporter, type ColourReader } from "./theme.ts";

export { hostAllows, hostState, subscribe } from "./state.ts";

/** What the embed code reads of the page. A test passes its own. */
export type EmbedEnv = {
  userAgent: string;
  /** `window.ReactNativeWebView.postMessage`, or `null` where there is none. */
  post: ((json: string) => void) | null;
  /** The object the receiver is defined on: the window. */
  target: object;
  doc: Document;
  /** How a colour is read; left out, a canvas of the document. */
  colour?: ColourReader;
};

/** The page as the browser gives it, or `null` where there is no page (a test in Node). */
function pageEnv(): EmbedEnv | null {
  if (typeof window === "undefined" || typeof document === "undefined") return null;
  const webView = (window as { ReactNativeWebView?: { postMessage?: unknown } }).ReactNativeWebView;
  const post =
    webView && typeof webView.postMessage === "function"
      ? // Called as a method: the function belongs to the object the WebView made.
        (json: string) => (webView.postMessage as (json: string) => void)(json)
      : null;
  return { userAgent: navigator.userAgent, post, target: window, doc: document };
}

let running: (() => void) | null = null;

/** Starts everything for an embedded page and gives the way to undo it; `null` in a browser. */
function start(env: EmbedEnv): (() => void) | null {
  const bridge = createPageBridge({ userAgent: env.userAgent, post: env.post });
  if (!bridge.embedded) return null;
  const { doc } = env;
  const html = doc.documentElement;

  // The host calls `window.__obopHost.receive(json)`. Defined once and not writable; the page
  // listens to no `message` event, so a frame cannot forge an answer.
  Object.defineProperty(env.target, PAGE_RECEIVER, {
    value: Object.freeze({ receive: (json: string) => bridge.receive(json) }),
    writable: false,
    configurable: false,
    enumerable: false,
  });
  html.dataset.host = "1";
  setHostAsk((command, params) => bridge.request(command, params));
  setHostState({ embedded: true, info: null });

  let stopped = false;
  let stopTheme = () => {};
  // Downloads are stopped from the first moment: a host that never answers saves no file either.
  const stopDownloads = startDownloadCatcher({
    doc,
    active: () => !hostAnswers("file.save"),
    notice: (text) => hostNotice(text, doc),
  });

  void bridge.request("host.info", {}).then(
    (info) => {
      if (stopped) return;
      html.dataset.hostInfo = "ok";
      setHostState({ embedded: true, info });
      if (hostAnswers("host.theme"))
        stopTheme = startThemeReporter({
          doc,
          colour: env.colour ?? canvasColour(doc),
          send: (theme) => askHost("host.theme", theme),
        });
    },
    (error: unknown) => {
      if (stopped) return;
      html.dataset.hostInfo = error instanceof BridgeError ? error.code : "failed";
    },
  );

  return () => {
    stopped = true;
    stopTheme();
    stopDownloads();
    delete html.dataset.host;
    delete html.dataset.hostInfo;
    leaveHost();
  };
}

/**
 * Starts the page's side of the host, once. In a browser it returns at the first check. Nothing
 * that goes wrong here may keep the back office from starting, so a fault ends in a page that
 * behaves as outside a host. The return value undoes everything but the receiver (tests).
 */
export function startHostEmbed(env: EmbedEnv | null = pageEnv()): () => void {
  if (env === null) return () => {};
  if (running !== null) return running;
  let stop: (() => void) | null;
  try {
    stop = start(env);
  } catch {
    leaveHost();
    return () => {};
  }
  if (stop === null) return () => {};
  const undo = stop;
  const stopOnce = () => {
    if (running !== stopOnce) return;
    running = null;
    undo();
  };
  running = stopOnce;
  return stopOnce;
}
