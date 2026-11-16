// A stand-in for the phone host app (web/apps/host-app), so that the back office can be driven as
// the page inside its WebView without a phone.
//
// What is real: the page's code, the protocol and the host's rules. The context's user agent ends
// in the host's token, as the WebView writes it; the page finds `window.ReactNativeWebView` and
// posts to it; the message is answered in Node by `createHostBridge` of `@obop/host-bridge`, the
// same code the app runs; and the answer goes back the way the app hands it over, as the script
// of `deliveryScript` evaluated in the page. What stands in: the native handlers, which only write
// down what they were asked, and the WebView itself.
import {
  createHostBridge,
  deliveryScript,
  type CommandName,
  type HostInfo,
} from "@obop/host-bridge";
import type { Browser } from "@playwright/test";
import { appUrl } from "./env.ts";
import { test as base } from "./fixtures.ts";

export { expect } from "./fixtures.ts";

/** What the WebView of the app adds to the browser's user agent: bridge major, platform, version. */
export const HOST_USER_AGENT = "ObopHost/1 (android; 0.0.0-e2e)";

/** What the stand-in says about itself: the first slice of the app, as a test build. */
export const HOST_INFO: HostInfo = {
  bridge: 1,
  platform: "android",
  app: { id: "org.obop.app.test", version: "0.0.0-e2e", cleartext: true, inspectable: true },
  commands: ["host.info", "host.theme", "host.settings"],
  signIn: { passkeys: false, microsoft: false },
};

export type HostCall = { command: CommandName; params: unknown };

export type StandInHost = {
  /** Every command a handler ran, in order. A message the host's rules dropped is not here. */
  calls: HostCall[];
  /** Every text a page handed to the transport, answered or not. */
  posted: string[];
  /** The parameters of each call of `command`, in order. */
  paramsOf: (command: CommandName) => unknown[];
};

type HostOptions = {
  /**
   * `false`: the browser's own user agent, without the host's token. The transport is still
   * there, so a page that speaks to it anyway is caught.
   */
  hostUserAgent: boolean;
  /**
   * The address the stand-in names as the sender of every message; `null` for the address of the
   * frame that posted it, which is what the WebView reports.
   */
  hostSender: string | null;
};

let ownUserAgent: string | undefined;

/** The user agent this browser sends by itself, read once from a page of its own. */
async function browserUserAgent(browser: Browser): Promise<string> {
  if (ownUserAgent === undefined) {
    const page = await browser.newPage();
    try {
      ownUserAgent = await page.evaluate(() => navigator.userAgent);
    } finally {
      await page.close();
    }
  }
  return ownUserAgent;
}

export const test = base.extend<{ host: StandInHost } & HostOptions>({
  hostUserAgent: [true, { option: true }],
  hostSender: [null, { option: true }],

  userAgent: async ({ browser, hostUserAgent }, use) => {
    await use(hostUserAgent ? `${await browserUserAgent(browser)} ${HOST_USER_AGENT}` : undefined);
  },

  host: [
    async ({ context, hostSender }, use) => {
      const calls: HostCall[] = [];
      const posted: string[] = [];
      const note = (command: CommandName) => (params: unknown) => {
        calls.push({ command, params });
        return {};
      };
      const bridge = createHostBridge({
        origin: appUrl(),
        handlers: {
          "host.info": (params) => {
            calls.push({ command: "host.info", params });
            return HOST_INFO;
          },
          "host.theme": note("host.theme"),
          "host.settings": note("host.settings"),
        },
      });
      await context.exposeBinding("__e2eHostPost", async ({ page, frame }, json: unknown) => {
        if (typeof json !== "string") return;
        posted.push(json);
        const answer = await bridge.handle(json, hostSender ?? frame.url());
        if (answer === null) return;
        // The page may have gone on to another document, or the test may be over.
        await page.evaluate(deliveryScript(answer)).catch(() => {});
      });
      await context.addInitScript(() => {
        const page = window as unknown as {
          ReactNativeWebView?: { postMessage: (json: string) => void };
          __e2eHostPost?: (json: string) => unknown;
        };
        page.ReactNativeWebView = { postMessage: (json) => void page.__e2eHostPost?.(json) };
      });
      await use({
        calls,
        posted,
        paramsOf: (command) => calls.filter((c) => c.command === command).map((c) => c.params),
      });
    },
    { auto: true },
  ],
});
