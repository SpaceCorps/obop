// @vitest-environment jsdom
// The page's side of the host, started as the entry starts it. The host on the other end is the
// real `createHostBridge` of `@obop/host-bridge` with recording handlers, so the page's code, the
// protocol and the host's rules run as they do in the app; only the WebView between them is a
// function call here.
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { createHostBridge, type CommandName, type HostInfo } from "@obop/host-bridge";
import { hostAllows, hostState, startHostEmbed, subscribe, type EmbedEnv } from "./embed.ts";
import { hostAnswers } from "./state.ts";
import type { Rgba } from "./theme.ts";

const ORIGIN = "http://localhost:18180";
const BROWSER = "Mozilla/5.0 (Linux; Android 16) AppleWebKit/537.36 Chrome/142.0.0.0 Mobile";
const HOST = `${BROWSER} ObopHost/1 (android; 0.1.0)`;

const INFO: HostInfo = {
  bridge: 1,
  platform: "android",
  app: { id: "org.obop.app.test", version: "0.1.0", cleartext: true, inspectable: true },
  commands: ["host.info", "host.theme", "host.settings"],
  signIn: { passkeys: false, microsoft: false },
};

type Call = { command: CommandName; params: unknown };

/** A host behind a function call: what it was sent, what its handlers ran, and the page's env. */
function standIn(options: { info?: HostInfo; sender?: string; userAgent?: string } = {}) {
  const posted: string[] = [];
  const calls: Call[] = [];
  const target: { __obopHost?: { receive(json: string): void } } = {};
  const note = (command: CommandName) => (params: unknown) => {
    calls.push({ command, params });
    return {};
  };
  const host = createHostBridge({
    origin: ORIGIN,
    handlers: {
      "host.info": (params) => {
        calls.push({ command: "host.info", params });
        return options.info ?? INFO;
      },
      "host.theme": note("host.theme"),
      "host.settings": note("host.settings"),
    },
  });
  const env: EmbedEnv = {
    userAgent: options.userAgent ?? HOST,
    post: (json) => {
      posted.push(json);
      void host.handle(json, options.sender ?? `${ORIGIN}/#/`).then((answer) => {
        if (answer !== null) target.__obopHost?.receive(answer);
      });
    },
    target,
    doc: document,
    // jsdom has no canvas: an `rgb()` text is read as it is written.
    colour: (css): Rgba | null => {
      const m = /^rgb\((\d+), (\d+), (\d+)\)$/.exec(css);
      return m ? [Number(m[1]), Number(m[2]), Number(m[3]), 255] : null;
    },
  };
  return { env, posted, calls, target };
}

const html = document.documentElement;
const commandsOf = (posted: string[]) =>
  posted.map((json) => (JSON.parse(json) as { command: string }).command);
const ownClick = HTMLAnchorElement.prototype.click;
let stop: () => void = () => {};

beforeEach(() => {
  document.body.innerHTML = '<div id="root"><div class="canvas"></div></div>';
  document.body.style.backgroundColor = "rgb(243, 244, 247)";
});
afterEach(() => {
  stop();
  stop = () => {};
  vi.useRealTimers();
  document.body.innerHTML = "";
  document.body.removeAttribute("style");
});

describe("in a browser", () => {
  it("does nothing at all: no global, no attribute, no listener, no message", () => {
    const seen = vi.fn();
    const off = subscribe(seen);
    const { env, posted, target } = standIn({ userAgent: BROWSER });
    stop = startHostEmbed({ ...env, post: null });
    expect(Object.keys(target)).toEqual([]);
    expect("__obopHost" in target).toBe(false);
    expect(html.hasAttribute("data-host")).toBe(false);
    expect(html.hasAttribute("data-host-info")).toBe(false);
    expect(HTMLAnchorElement.prototype.click).toBe(ownClick);
    expect(hostState()).toEqual({ embedded: false, info: null });
    expect(hostAllows("passkeys")).toBe(true);
    expect(seen).not.toHaveBeenCalled();
    expect(posted).toEqual([]);
    off();
  });

  it("needs both facts: the token without the transport, or the transport without the token, is a browser", async () => {
    const tokenOnly = standIn();
    stop = startHostEmbed({ ...tokenOnly.env, post: null });
    expect(hostState().embedded).toBe(false);
    expect(html.hasAttribute("data-host")).toBe(false);

    const transportOnly = standIn({ userAgent: BROWSER });
    stop = startHostEmbed(transportOnly.env);
    await new Promise((done) => setTimeout(done, 20));
    expect(hostState().embedded).toBe(false);
    expect(transportOnly.posted).toEqual([]);
    expect("__obopHost" in transportOnly.target).toBe(false);
  });

  it("is where a test in Node is: no page, nothing started", () => {
    expect(() => startHostEmbed(null)()).not.toThrow();
    expect(hostState().embedded).toBe(false);
  });
});

describe("inside the host", () => {
  it("defines the receiver once, not writable, and marks the page", () => {
    const { env, target } = standIn();
    stop = startHostEmbed(env);
    const receiver = Object.getOwnPropertyDescriptor(target, "__obopHost")!;
    expect(receiver.writable).toBe(false);
    expect(receiver.configurable).toBe(false);
    expect(receiver.enumerable).toBe(false);
    expect(Object.isFrozen(receiver.value)).toBe(true);
    expect(typeof (receiver.value as { receive: unknown }).receive).toBe("function");
    expect(html.dataset.host).toBe("1");
    expect(hostState()).toEqual({ embedded: true, info: null });
    // Until the host has answered, the sign-in page offers neither.
    expect(hostAllows("passkeys")).toBe(false);
    expect(hostAllows("microsoft")).toBe(false);
  });

  it("asks host.info exactly once and then knows what the host said", async () => {
    const { env, posted, calls } = standIn({
      info: { ...INFO, signIn: { passkeys: false, microsoft: true } },
    });
    stop = startHostEmbed(env);
    await vi.waitFor(() => expect(html.dataset.hostInfo).toBe("ok"));
    expect(calls.filter((c) => c.command === "host.info")).toHaveLength(1);
    expect(commandsOf(posted).filter((c) => c === "host.info")).toHaveLength(1);
    expect(hostState().info?.app.id).toBe("org.obop.app.test");
    expect(hostAllows("passkeys")).toBe(false);
    expect(hostAllows("microsoft")).toBe(true);
    expect(hostAnswers("host.settings")).toBe(true);
  });

  it("tells the host its ground colour once the host has listed the command", async () => {
    const { env, calls } = standIn();
    stop = startHostEmbed(env);
    await vi.waitFor(() =>
      expect(calls.filter((c) => c.command === "host.theme")).toEqual([
        { command: "host.theme", params: { scheme: "light", background: "#f3f4f7" } },
      ]),
    );
    // In order: the first call of a page is host.info.
    expect(calls[0]!.command).toBe("host.info");
  });

  it("calls nothing that the host did not list", async () => {
    const { env, posted } = standIn({ info: { ...INFO, commands: ["host.info"] } });
    stop = startHostEmbed(env);
    await vi.waitFor(() => expect(html.dataset.hostInfo).toBe("ok"));
    await new Promise((done) => setTimeout(done, 80));
    expect(commandsOf(posted)).toEqual(["host.info"]);
    expect(hostAnswers("host.settings")).toBe(false);
  });

  it("starts once: a second call is the first one", async () => {
    const first = standIn();
    stop = startHostEmbed(first.env);
    const second = standIn();
    expect(startHostEmbed(second.env)).toBe(stop);
    await vi.waitFor(() => expect(html.dataset.hostInfo).toBe("ok"));
    expect(second.posted).toEqual([]);
    expect("__obopHost" in second.target).toBe(false);
  });
});

describe("a host that does not answer", () => {
  it("leaves the page embedded with nothing allowed, and says how the question ended", async () => {
    vi.useFakeTimers();
    // The host's first rule: a message from another origin gets no answer.
    const { env, posted, calls } = standIn({ sender: "https://elsewhere.example/" });
    stop = startHostEmbed(env);
    await vi.advanceTimersByTimeAsync(4_900);
    expect(html.hasAttribute("data-host-info")).toBe(false);
    await vi.advanceTimersByTimeAsync(200);
    expect(html.dataset.hostInfo).toBe("failed");
    expect(html.dataset.host).toBe("1");
    expect(calls).toEqual([]);
    expect(commandsOf(posted)).toEqual(["host.info"]);
    expect(hostState()).toEqual({ embedded: true, info: null });
    expect(hostAllows("passkeys")).toBe(false);
    expect(hostAnswers("host.theme")).toBe(false);
  });

  it("says so too when the app speaks another version of the bridge", async () => {
    const { env, posted } = standIn({ userAgent: `${BROWSER} ObopHost/2 (android; 9.0.0)` });
    stop = startHostEmbed(env);
    await vi.waitFor(() => expect(html.dataset.hostInfo).toBe("unsupported_version"));
    expect(posted).toEqual([]);
    expect(hostAllows("passkeys")).toBe(false);
  });
});

describe("downloads inside the host", () => {
  const download = () => {
    const a = document.createElement("a");
    a.href = "blob:http://localhost:18180/1";
    a.download = "party.csv";
    const clicked = vi.fn((e: Event) => e.preventDefault());
    a.addEventListener("click", clicked);
    a.click();
    return clicked;
  };
  const said = () => document.querySelector("[data-host-notice]")?.textContent ?? null;

  it("are stopped and said from the first moment, before the host has answered", () => {
    const { env } = standIn();
    stop = startHostEmbed(env);
    expect(download()).not.toHaveBeenCalled();
    expect(said()).toBe("Downloads are not available in the app yet.");
  });

  it("go through once the host says that it can save a file", async () => {
    const withSave = ["host.info", "file.save"] as unknown as HostInfo["commands"];
    const { env } = standIn({ info: { ...INFO, commands: withSave } });
    stop = startHostEmbed(env);
    await vi.waitFor(() => expect(html.dataset.hostInfo).toBe("ok"));
    expect(download()).toHaveBeenCalledTimes(1);
    expect(said()).toBeNull();
  });
});

describe("a fault", () => {
  it("in the embed code does not stop the entry: the page behaves as in a browser", () => {
    const { env, target, posted } = standIn();
    // Something else has already put an object there that cannot be replaced.
    Object.defineProperty(target, "__obopHost", { value: {}, configurable: false });
    expect(() => (stop = startHostEmbed(env))).not.toThrow();
    expect(hostState()).toEqual({ embedded: false, info: null });
    expect(html.hasAttribute("data-host")).toBe(false);
    expect(HTMLAnchorElement.prototype.click).toBe(ownClick);
    expect(posted).toEqual([]);
  });
});

describe("stopped", () => {
  it("takes the marks, the catcher and the state away again", async () => {
    const { env, calls } = standIn();
    stop = startHostEmbed(env);
    await vi.waitFor(() => expect(calls.some((c) => c.command === "host.theme")).toBe(true));
    stop();
    expect(html.hasAttribute("data-host")).toBe(false);
    expect(html.hasAttribute("data-host-info")).toBe(false);
    expect(HTMLAnchorElement.prototype.click).toBe(ownClick);
    expect(hostState()).toEqual({ embedded: false, info: null });
    const before = calls.length;
    document.documentElement.dataset.theme = "dark";
    await new Promise((done) => setTimeout(done, 60));
    delete document.documentElement.dataset.theme;
    expect(calls).toHaveLength(before);
  });
});
