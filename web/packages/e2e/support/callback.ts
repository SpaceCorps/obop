import { createServer } from "node:http";
import type { AddressInfo } from "node:net";

/**
 * The client half of a loopback OAuth redirect (RFC 8252 section 7.3): a real HTTP listener on a
 * free port of 127.0.0.1, which is where an MCP client such as Claude Code waits for the browser.
 *
 * Do not play this part with `page.route()`. Playwright sends a request that is a redirect (here:
 * the 303 from the consent form to the client) on to the network without calling any route, so the
 * browser gets "connection refused". Desktop Chrome then reloads its error page about a second
 * later, and that second, direct request is routed; that is the only reason a route-based version
 * passed on a desktop and timed out on Playwright's headless shell in CI.
 */
export type Callback = {
  /** The redirect address to register, `http://127.0.0.1:PORT/callback`. */
  url: string;
  /** `127.0.0.1:PORT`, as the consent page and the connected apps table show it. */
  host: string;
  /** The next call the browser makes to the callback path; waits when none has arrived yet. */
  next(): Promise<URL>;
  close(): Promise<void>;
};

const PAGE =
  '<!doctype html><meta charset="utf-8"><link rel="icon" href="data:,"><title>Connected</title><p>You can close this tab.</p>';

export async function listenForCallback(): Promise<Callback> {
  const arrived: URL[] = [];
  const waiting: ((url: URL) => void)[] = [];
  const server = createServer((req, res) => {
    const url = new URL(req.url ?? "/", "http://127.0.0.1");
    if (url.pathname !== "/callback") {
      res.writeHead(404, { "cache-control": "no-store" }).end();
      return;
    }
    // A page, as real clients show one. Not a 204: Playwright's click waits for a navigation that
    // a 204 never commits. The empty icon keeps the browser from asking for /favicon.ico.
    res
      .writeHead(200, { "content-type": "text/html; charset=utf-8", "cache-control": "no-store" })
      .end(PAGE);
    const wait = waiting.shift();
    if (wait) wait(url);
    else arrived.push(url);
  });
  await new Promise<void>((ok, fail) => {
    server.once("error", fail);
    server.listen(0, "127.0.0.1", ok);
  });
  const host = `127.0.0.1:${(server.address() as AddressInfo).port}`;
  return {
    url: `http://${host}/callback`,
    host,
    next: () => {
      const url = arrived.shift();
      return url ? Promise.resolve(url) : new Promise<URL>((ok) => waiting.push(ok));
    },
    close: () =>
      new Promise<void>((ok) => {
        server.close(() => ok());
        server.closeAllConnections();
      }),
  };
}
