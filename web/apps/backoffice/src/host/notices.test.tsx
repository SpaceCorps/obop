// @vitest-environment jsdom
// How the embed code speaks to the person: through the app's toasts where the screen draws them,
// and through a line of its own where it does not (the gates and the sign-in page).
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { hostNotice, useHostNotices } from "./notices.ts";
import { leaveHost, setHostState } from "./state.ts";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;
let renders: number;

type Notify = (text: string, tone?: "ok" | "error" | "info") => void;

/** A stand-in for the app: it hands its `notify` over and draws the toast region, or does not. */
function Shell({ notify, region }: { notify: Notify; region: boolean }) {
  useHostNotices(notify);
  renders += 1;
  return region ? <div className="toasts" role="status" /> : <main className="gate" />;
}

const own = () => document.querySelector("[data-host-notice]");

beforeEach(() => {
  renders = 0;
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});
afterEach(() => {
  act(() => root.unmount());
  vi.useRealTimers();
  document.body.innerHTML = "";
  leaveHost();
});

describe("a notice of the embed code", () => {
  it("goes to the app's notify, as a note, where the shell draws its toasts", async () => {
    const notify = vi.fn();
    await act(async () => root.render(<Shell notify={notify} region />));
    hostNotice("Downloads are not available in the app yet.");
    expect(notify).toHaveBeenCalledWith("Downloads are not available in the app yet.", "info");
    expect(own()).toBeNull();
  });

  it("is drawn by the embed code where the screen has no toast region", async () => {
    const notify = vi.fn();
    await act(async () => root.render(<Shell notify={notify} region={false} />));
    hostNotice("Copy the codes or write them down.");
    expect(notify).not.toHaveBeenCalled();
    const region = own()!;
    expect(region.getAttribute("role")).toBe("status");
    expect(region.className).toBe("toasts");
    expect(region.querySelector(".toast.panel.info .toast-text")?.textContent).toBe(
      "Copy the codes or write them down.",
    );
  });

  it("is drawn by the embed code when no app has handed a notify over", () => {
    hostNotice("One");
    expect(own()?.textContent).toBe("One");
  });

  it("shows the newest only, and goes away by itself", async () => {
    vi.useFakeTimers();
    hostNotice("One");
    vi.advanceTimersByTime(6000);
    hostNotice("Two");
    expect(document.querySelectorAll("[data-host-notice]")).toHaveLength(1);
    expect(own()?.textContent).toBe("Two");
    // The first one's clock does not take the second away early.
    vi.advanceTimersByTime(6000);
    expect(own()?.textContent).toBe("Two");
    vi.advanceTimersByTime(1500);
    expect(own()).toBeNull();
  });

  it("does not take its own line for the app's region", async () => {
    const notify = vi.fn();
    await act(async () => root.render(<Shell notify={notify} region={false} />));
    hostNotice("One");
    hostNotice("Two");
    expect(notify).not.toHaveBeenCalled();
    expect(own()?.textContent).toBe("Two");
  });

  it("forgets a notify when the app that gave it is gone", async () => {
    const notify = vi.fn();
    await act(async () => root.render(<Shell notify={notify} region />));
    act(() => root.unmount());
    root = createRoot(container);
    // A region left in the page without an app behind it.
    document.body.insertAdjacentHTML("beforeend", '<div class="toasts"></div>');
    hostNotice("One");
    expect(notify).not.toHaveBeenCalled();
    expect(own()?.textContent).toBe("One");
  });
});

describe("the app", () => {
  it("is drawn again when the host has answered, so that the account menu is up to date", async () => {
    await act(async () => root.render(<Shell notify={vi.fn()} region />));
    const before = renders;
    await act(async () => setHostState({ embedded: true, info: null }));
    expect(renders).toBeGreaterThan(before);
  });
});
