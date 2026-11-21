import { expect, type Page, type Request } from "@playwright/test";

/**
 * Starts counting the page's requests whose address the predicate takes; the function it returns
 * waits until at least one of them was sent and every one sent so far has been answered (or has
 * failed). A screen that shows a skeleton until its data is in is then waited for by its own
 * answers, however slow the machine, and not by a fixed time (docs/ISSUES.md U51, U65). Call it
 * before the navigation that starts the requests.
 *
 * A screen that may need nothing from the server (its code is already in, its text is static: a
 * Customize tab on a second visit) names `mayBeNone`: when no request was sent by the time the
 * returned function has waited `NONE_GRACE_MS`, there is nothing to wait for. A screen that must
 * fetch leaves it off and fails when nothing was sent.
 */
const NONE_GRACE_MS = 1_500;

export function answered(
  page: Page,
  wanted: (url: string) => boolean,
  options: { mayBeNone?: boolean } = {},
): () => Promise<void> {
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
    const waitingSince = Date.now();
    const settled = () =>
      open.size === 0 &&
      (sent > 0 || (options.mayBeNone === true && Date.now() - waitingSince >= NONE_GRACE_MS));
    try {
      // The bound is the answer's, not the screen's: a request the server never answers still fails.
      // A timeout names what was still open (or that nothing was sent), so a failure on a machine
      // nobody can look at says which request it waited for.
      await expect
        .poll(settled, {
          message: `the screen's requests are answered (a request still open or none sent)`,
          timeout: 30_000,
        })
        .toBe(true)
        .catch((e: unknown) => {
          const waiting = [...open].map((r) => `${r.method()} ${r.url()}`);
          throw new Error(
            `the screen's requests are not answered: ${sent} sent, ${waiting.length} still open${
              waiting.length ? `: ${waiting.join(", ")}` : ""
            }`,
            { cause: e },
          );
        });
    } finally {
      page.off("request", start);
      page.off("requestfinished", end);
      page.off("requestfailed", end);
    }
  };
}
