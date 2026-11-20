import { expect, type Page, type Request } from "@playwright/test";

/**
 * Starts counting the page's requests whose address the predicate takes; the function it returns
 * waits until at least one of them was sent and every one sent so far has been answered (or has
 * failed). A screen that shows a skeleton until its data is in is then waited for by its own
 * answers, however slow the machine, and not by a fixed time (docs/ISSUES.md U51, U65). Call it
 * before the navigation that starts the requests.
 */
export function answered(page: Page, wanted: (url: string) => boolean): () => Promise<void> {
  const open = new Set<Request>();
  let sent = 0;
  const start = (r: Request) => {
    if (!wanted(r.url())) return;
    open.add(r);
    sent++;
  };
  const end = (r: Request) => void open.delete(r);
  page.on("request", start);
  page.on("requestfinished", end);
  page.on("requestfailed", end);
  return async () => {
    try {
      // The bound is the answer's, not the screen's: a request the server never answers still fails.
      await expect
        .poll(() => sent > 0 && open.size === 0, {
          message: "the screen's requests are answered",
          timeout: 30_000,
        })
        .toBe(true);
    } finally {
      page.off("request", start);
      page.off("requestfinished", end);
      page.off("requestfailed", end);
    }
  };
}
