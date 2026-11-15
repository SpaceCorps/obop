import { runInNewContext } from "node:vm";
import { COMMAND_NAMES, createHostBridge, validResult } from "@obop/host-bridge";
import { describe, expect, test } from "vite-plus/test";
import type { Build } from "./build.ts";
import { createMessagePump, hostHandlers, hostInfo, type CommandDeps } from "./commands.ts";
import type { PageTheme } from "./theme.ts";

const ORIGIN = "http://localhost:18180";
const PAGE = "http://localhost:18180/#/crm/lead";
const TEST: Build = { cleartext: true, webviewDebug: true, defaultServer: ORIGIN };
const PLAIN: Build = { cleartext: false, webviewDebug: false, defaultServer: "" };

/** A host as the app wires it, with what the handlers did and what was injected written down. */
function wired(options: { build?: Build; answering?: () => boolean } = {}) {
  const themes: PageTheme[] = [];
  const calls = { settings: 0, asked: 0 };
  const injected: string[] = [];
  const deps: CommandDeps = {
    platform: "android",
    appId: "org.obop.app.test",
    version: "0.1.0",
    build: options.build ?? TEST,
    onTheme: (theme) => themes.push(theme),
    onSettings: () => {
      calls.settings += 1;
    },
  };
  const answering = options.answering ?? (() => true);
  const pump = createMessagePump({
    bridge: createHostBridge({ origin: ORIGIN, handlers: hostHandlers(deps) }),
    answering: () => {
      calls.asked += 1;
      return answering();
    },
    inject: (script) => injected.push(script),
  });
  return { pump, themes, calls, injected, deps };
}

const request = (command: string, params: unknown = {}, id = "p1"): string =>
  JSON.stringify({ obop: 1, kind: "request", id, command, params });

/** What the page receives when an injected script runs in it. */
function received(script: string): Record<string, unknown> {
  let text = "";
  const window = {
    __obopHost: {
      receive(json: string) {
        text = json;
      },
    },
  };
  expect(runInNewContext(script, { window })).toBe(true);
  return JSON.parse(text) as Record<string, unknown>;
}

describe("the three commands", () => {
  test("the handler list is exactly the commands of version 1", () => {
    const { deps } = wired();
    expect(Object.keys(hostHandlers(deps)).sort()).toEqual([...COMMAND_NAMES].sort());
  });

  test("host.info says what this host is and lists the handlers themselves", () => {
    const { deps } = wired();
    const info = hostInfo(deps, ["host.info"]);
    expect(info).toEqual({
      bridge: 1,
      platform: "android",
      app: { id: "org.obop.app.test", version: "0.1.0", cleartext: true, inspectable: true },
      commands: ["host.info"],
      signIn: { passkeys: false, microsoft: false },
    });
    expect(validResult("host.info", info)).toBe(true);
  });

  test("a plain build says so: neither flag is set", () => {
    const { deps } = wired({ build: PLAIN });
    expect(hostInfo(deps, []).app).toMatchObject({ cleartext: false, inspectable: false });
  });

  test("neither a passkey nor Microsoft is offered inside the host", () => {
    for (const build of [PLAIN, TEST]) {
      expect(hostInfo(wired({ build }).deps, []).signIn).toEqual({
        passkeys: false,
        microsoft: false,
      });
    }
  });
});

describe("a message of the page, through the bridge and back", () => {
  test("host.info is answered with the info, delivered to the page's receiver", async () => {
    const { pump, injected } = wired();
    await pump(request("host.info"), PAGE);
    expect(injected).toHaveLength(1);
    expect(received(injected[0] ?? "")).toEqual({
      obop: 1,
      kind: "response",
      id: "p1",
      ok: true,
      result: {
        bridge: 1,
        platform: "android",
        app: { id: "org.obop.app.test", version: "0.1.0", cleartext: true, inspectable: true },
        commands: ["host.info", "host.theme", "host.settings"],
        signIn: { passkeys: false, microsoft: false },
      },
    });
  });

  test("host.theme hands the ground and the scheme to the app", async () => {
    const { pump, themes, injected } = wired();
    await pump(request("host.theme", { scheme: "dark", background: "#0a0c11" }), PAGE);
    expect(themes).toEqual([{ scheme: "dark", background: "#0a0c11" }]);
    expect(received(injected[0] ?? "")).toMatchObject({ ok: true, result: {} });
  });

  test("a theme that is no colour never reaches the app", async () => {
    const { pump, themes, injected } = wired();
    for (const params of [
      { scheme: "dark", background: "red" },
      { scheme: "dark", background: "#fff" },
      { scheme: "pink", background: "#000000" },
      { scheme: "dark", background: "#000000", more: 1 },
      { scheme: "dark" },
    ]) {
      await pump(request("host.theme", params), PAGE);
    }
    expect(themes).toEqual([]);
    expect(injected).toHaveLength(5);
    for (const script of injected) {
      expect(received(script)).toMatchObject({ ok: false, error: { code: "bad_params" } });
    }
  });

  test("host.settings opens This app", async () => {
    const { pump, calls, injected } = wired();
    await pump(request("host.settings"), PAGE);
    expect(calls.settings).toBe(1);
    expect(received(injected[0] ?? "")).toMatchObject({ ok: true });
  });

  test("a command that is not on the list reaches nothing", async () => {
    const { pump, calls, themes, injected } = wired();
    for (const command of [
      "mileage.start",
      "host.reload",
      "__proto__",
      "constructor",
      "toString",
    ]) {
      await pump(request(command), PAGE);
    }
    expect(calls.settings).toBe(0);
    expect(themes).toEqual([]);
    for (const script of injected) {
      expect(received(script)).toMatchObject({ ok: false, error: { code: "unknown_command" } });
    }
  });

  test("a sender of another origin, or none, gets no answer and reaches no handler", async () => {
    const { pump, calls, injected } = wired();
    for (const sender of [
      "https://evil.example.net/",
      "http://localhost:18180.evil.net/",
      "http://localhost:18181/",
      "https://localhost:18180/",
      "about:blank",
      "",
      undefined,
    ]) {
      await pump(request("host.settings"), sender);
    }
    expect(calls.settings).toBe(0);
    expect(injected).toEqual([]);
  });

  test("something that is no request gets no answer", async () => {
    const { pump, injected } = wired();
    for (const data of [
      "",
      "hello",
      "{",
      "[]",
      JSON.stringify({ obop: 1, kind: "response", id: "p1" }),
    ]) {
      await pump(data, PAGE);
    }
    expect(injected).toEqual([]);
  });
});

describe("while a foreign document is or was on top", () => {
  test("the message is dropped before the bridge sees it", async () => {
    const { pump, calls, themes, injected } = wired({ answering: () => false });
    await pump(request("host.settings"), PAGE);
    await pump(request("host.theme", { scheme: "dark", background: "#000000" }), PAGE);
    await pump(request("host.info"), PAGE);
    expect(calls.settings).toBe(0);
    expect(themes).toEqual([]);
    expect(injected).toEqual([]);
  });

  test("an answer is not injected into a document that came on top while it was made", async () => {
    // The question is asked twice per message: before the handler and before the delivery.
    let top = true;
    const { pump, calls, injected } = wired({ answering: () => top });
    const asking = pump(request("host.info"), PAGE);
    top = false;
    await asking;
    expect(calls.asked).toBe(2);
    expect(injected).toEqual([]);
  });

  test("it answers again once the instance is back on top", async () => {
    let top = false;
    const { pump, injected } = wired({ answering: () => top });
    await pump(request("host.info"), PAGE);
    top = true;
    await pump(request("host.info", {}, "p2"), PAGE);
    expect(injected).toHaveLength(1);
    expect(received(injected[0] ?? "")).toMatchObject({ id: "p2", ok: true });
  });
});
