// @vitest-environment jsdom
// Downloads inside the host: each of the page's download places makes an anchor with a `download`
// attribute and a `blob:` address and clicks it. The catcher stops that click and says so; every
// other click, and every click once the host can save a file, goes through as before.
import { afterEach, beforeEach, describe, expect, it } from "vite-plus/test";
import { downloadNotice, startDownloadCatcher } from "./downloads.ts";

const SAID = "Downloads are not available in the app yet.";
const CODES = `${SAID} Copy the codes or write them down.`;

let notices: string[];
let active: boolean;
let stop: () => void;
/** The clicks that reached the page: the address of each anchor that was really clicked. */
let clicked: string[];
const onClick = (event: Event) => {
  const anchor = event.target as HTMLAnchorElement;
  clicked.push(anchor.getAttribute("href") ?? "");
  // jsdom would try to follow the link; the test only counts the click.
  event.preventDefault();
};

function anchor(href: string, download?: string): HTMLAnchorElement {
  const a = document.createElement("a");
  a.href = href;
  if (download !== undefined) a.download = download;
  a.addEventListener("click", onClick);
  return a;
}

function setBrand(slug: string | null) {
  document.head.querySelectorAll('meta[name="obop-brand"]').forEach((m) => m.remove());
  if (slug === null) return;
  const meta = document.createElement("meta");
  meta.name = "obop-brand";
  meta.content = JSON.stringify({ name: "MOHV", slug, messages: {}, v: "" });
  document.head.append(meta);
}

beforeEach(() => {
  notices = [];
  clicked = [];
  active = true;
  stop = startDownloadCatcher({
    doc: document,
    active: () => active,
    notice: (text) => void notices.push(text),
  });
});
afterEach(() => {
  stop();
  setBrand(null);
  document.body.innerHTML = "";
});

describe("a download the page starts", () => {
  it("is stopped and said, attached to the page or not", () => {
    // The CSV export attaches its anchor; the recovery codes never do.
    const attached = anchor("blob:http://localhost/1", "party.csv");
    document.body.append(attached);
    attached.click();
    anchor("blob:http://localhost/2", "notes.txt").click();
    expect(clicked).toEqual([]);
    expect(notices).toEqual([SAID, SAID]);
  });

  it("gets a second sentence when it is the recovery codes, by the name the page gives the file", () => {
    anchor("blob:http://localhost/3", "obop-recovery-codes.txt").click();
    expect(notices).toEqual([CODES]);
    setBrand("mohv");
    anchor("blob:http://localhost/4", "mohv-recovery-codes.txt").click();
    // With a brand the file of another name is an ordinary download.
    anchor("blob:http://localhost/5", "obop-recovery-codes.txt").click();
    expect(notices).toEqual([CODES, CODES, SAID]);
    expect(clicked).toEqual([]);
  });

  it("goes through when it is not a blob, or has no download attribute", () => {
    anchor("https://example.com/file.pdf", "file.pdf").click();
    anchor("blob:http://localhost/6").click();
    anchor("#/crm/lead").click();
    expect(clicked).toEqual([
      "https://example.com/file.pdf",
      "blob:http://localhost/6",
      "#/crm/lead",
    ]);
    expect(notices).toEqual([]);
  });

  it("goes through once the host can save a file", () => {
    active = false;
    anchor("blob:http://localhost/7", "party.csv").click();
    expect(clicked).toEqual(["blob:http://localhost/7"]);
    expect(notices).toEqual([]);
  });
});

describe("a blob link the person taps", () => {
  /** A tap as the browser delivers it: a click event on what was tapped, not `click()`. */
  function tap(target: Element): MouseEvent {
    const event = new MouseEvent("click", { bubbles: true, cancelable: true });
    target.dispatchEvent(event);
    return event;
  }

  it("is stopped and said, also when the tap lands on a picture inside it", () => {
    const link = document.createElement("a");
    link.href = "blob:http://localhost/8";
    link.target = "_blank";
    const picture = document.createElement("img");
    link.append(picture);
    document.body.append(link);
    expect(tap(picture).defaultPrevented).toBe(true);
    expect(tap(link).defaultPrevented).toBe(true);
    expect(notices).toEqual([SAID, SAID]);
  });

  it("is left alone when it is another link, or no link, or the host can save a file", () => {
    const link = document.createElement("a");
    link.href = "https://example.com/";
    const button = document.createElement("button");
    const blob = document.createElement("a");
    blob.href = "blob:http://localhost/9";
    document.body.append(link, button, blob);
    // jsdom would try to follow the link; a listener after the catcher's stops that.
    link.addEventListener("click", (e) => e.preventDefault());
    blob.addEventListener("click", (e) => e.preventDefault());
    tap(link);
    expect(tap(button).defaultPrevented).toBe(false);
    active = false;
    tap(blob);
    expect(notices).toEqual([]);
  });
});

describe("the catcher", () => {
  it("leaves the page as it was when it is taken away", () => {
    const before = HTMLAnchorElement.prototype.click;
    stop();
    expect(document.defaultView!.HTMLAnchorElement.prototype.click).not.toBe(before);
    anchor("blob:http://localhost/10", "party.csv").click();
    expect(clicked).toEqual(["blob:http://localhost/10"]);
    expect(notices).toEqual([]);
    const link = document.createElement("a");
    link.href = "blob:http://localhost/11";
    link.addEventListener("click", (e) => e.preventDefault());
    document.body.append(link);
    link.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));
    expect(notices).toEqual([]);
  });

  it("words the notice by the file name alone", () => {
    expect(downloadNotice("party.csv")).toBe(SAID);
    expect(downloadNotice("")).toBe(SAID);
    expect(downloadNotice("obop-recovery-codes.txt")).toBe(CODES);
  });
});
