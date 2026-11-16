// Inside the phone host app a download does nothing: Android's WebView drops a `blob:` download
// without a word, and the host has no command for saving a file yet (`file.save` is later). So
// that Export, a file chip or the recovery codes do not look as if they had worked, the embed code
// stops each one and says so.
//
// Every download of the page is an anchor with a `download` attribute and a `blob:` address that
// the page makes and clicks itself (`format.ts` `download`, `blobs.ts` `saveBlob`,
// `connectors/model.ts` `downloadText`, and the recovery codes in `@obop/ui`, whose anchor is
// never attached to the page). They are caught from outside, in `click` of the anchor's prototype;
// a link to a `blob:` address that a person taps (the file preview's "Open full size") is caught
// by one listener on the document. Outside a host none of this is installed.
import { pageBrand, recoveryFileName, t } from "@obop/ui";

/** What the person is told when `fileName` was not saved. */
export function downloadNotice(fileName: string): string {
  const first = t("Downloads are not available in the app yet.");
  // The recovery codes are shown only once, so the person is told what to do instead.
  return fileName !== "" && fileName === recoveryFileName(pageBrand())
    ? `${first} ${t("Copy the codes or write them down.")}`
    : first;
}

/**
 * Starts catching downloads in `doc`. `active` is asked at each one: it is false once the host has
 * said that it can save a file, and then everything goes through as in a browser. The return value
 * takes the catcher away again.
 */
export function startDownloadCatcher(deps: {
  doc: Document;
  active: () => boolean;
  notice: (text: string) => void;
}): () => void {
  const { doc, active, notice } = deps;
  const view = doc.defaultView;
  if (!view) return () => {};
  const Anchor = view.HTMLAnchorElement;
  const original = Anchor.prototype.click;

  function click(this: HTMLAnchorElement): void {
    if (active() && this.hasAttribute("download") && this.href.startsWith("blob:")) {
      notice(downloadNotice(this.download));
      return;
    }
    original.call(this);
  }
  Anchor.prototype.click = click;

  const tapped = (event: Event) => {
    if (!active()) return;
    const target = event.target;
    const link = target instanceof view.Element ? target.closest("a[href]") : null;
    if (!(link instanceof Anchor) || !link.href.startsWith("blob:")) return;
    event.preventDefault();
    notice(downloadNotice(link.download));
  };
  doc.addEventListener("click", tapped, true);

  return () => {
    if (Anchor.prototype.click === click) Anchor.prototype.click = original;
    doc.removeEventListener("click", tapped, true);
  };
}
