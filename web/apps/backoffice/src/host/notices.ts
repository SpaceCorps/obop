// How the embed code tells the person something: through the app's own toasts, and where the
// screen draws none, through a line of its own.
//
// The app's `notify` puts a toast into the region that only the signed-in shell draws. The gates
// (set up two-factor first, choose a new password first) and the sign-in page draw no such region,
// and the gate for two-factor is where a new administrator meets the recovery codes: a Download
// that does nothing there, without a word, would leave them with no codes saved. So where the
// region is missing the notice is drawn here, with the classes of a toast and no rule of its own.
import { useEffect, useSyncExternalStore } from "react";
import { hostState, subscribe } from "./state.ts";

type Notify = (text: string, tone?: "ok" | "error" | "info") => void;

/** How long a notice of our own stays, as a note of the app does. */
const SHOWN_MS = 7000;
/** Marks the region this file draws, so that it is not taken for the app's. */
const OWN = "data-host-notice";

let appNotify: Notify | null = null;
let timer: ReturnType<typeof setTimeout> | undefined;

/**
 * Gives the embed code the app's `notify`. It also draws the app again when the host has answered
 * (or left), so that what reads the host while the shell renders, the account menu, is up to date.
 */
export function useHostNotices(notify: Notify): void {
  useSyncExternalStore(subscribe, hostState, hostState);
  useEffect(() => {
    appNotify = notify;
    return () => {
      if (appNotify === notify) appNotify = null;
    };
  }, [notify]);
}

function clearOwn(doc: Document): void {
  clearTimeout(timer);
  timer = undefined;
  for (const old of doc.querySelectorAll(`[${OWN}]`)) old.remove();
}

/** Tells the person `text`, as a note. One notice of our own at a time: the newest. */
export function hostNotice(text: string, doc: Document = document): void {
  if (appNotify !== null && doc.querySelector(`.toasts:not([${OWN}])`) !== null) {
    appNotify(text, "info");
    return;
  }
  clearOwn(doc);
  const region = doc.createElement("div");
  region.className = "toasts";
  region.setAttribute(OWN, "");
  region.setAttribute("role", "status");
  region.setAttribute("aria-live", "polite");
  const toast = doc.createElement("div");
  toast.className = "toast panel info";
  const words = doc.createElement("span");
  words.className = "toast-text";
  words.textContent = text;
  toast.append(words);
  region.append(toast);
  doc.body.append(region);
  timer = setTimeout(() => clearOwn(doc), SHOWN_MS);
}
