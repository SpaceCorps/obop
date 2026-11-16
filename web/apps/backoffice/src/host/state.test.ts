// What the page knows about the host: outside one everything is allowed and nothing is asked;
// inside one nothing is allowed and no command is sent until `host.info` has answered, and then
// only what the answer lists.
import { afterEach, describe, expect, it, vi } from "vite-plus/test";
import { BridgeError, type HostInfo } from "@obop/host-bridge";
import {
  allowsIn,
  answersIn,
  askHost,
  hostAllows,
  hostAnswers,
  hostSavesFiles,
  hostState,
  leaveHost,
  savesIn,
  setHostAsk,
  setHostState,
  subscribe,
} from "./state.ts";

const info = (over: Partial<HostInfo> = {}): HostInfo => ({
  bridge: 1,
  platform: "android",
  app: { id: "org.obop.app.test", version: "0.1.0", cleartext: false, inspectable: false },
  commands: ["host.info", "host.theme", "host.settings"],
  signIn: { passkeys: false, microsoft: false },
  ...over,
});

afterEach(() => leaveHost());

describe("outside a host", () => {
  it("allows every feature and answers no command", () => {
    expect(hostState()).toEqual({ embedded: false, info: null });
    expect(hostAllows("passkeys")).toBe(true);
    expect(hostAllows("microsoft")).toBe(true);
    expect(hostAnswers("host.settings")).toBe(false);
  });

  it("sends nothing: a request ends in unknown_command", async () => {
    const error = await askHost("host.settings", {}).catch((e: unknown) => e);
    expect(error).toBeInstanceOf(BridgeError);
    expect((error as BridgeError).code).toBe("unknown_command");
  });
});

describe("inside a host", () => {
  it("allows nothing and answers nothing until host.info has answered", () => {
    setHostState({ embedded: true, info: null });
    expect(hostAllows("passkeys")).toBe(false);
    expect(hostAllows("microsoft")).toBe(false);
    expect(hostAnswers("host.info")).toBe(false);
  });

  it("then allows what the answer says, feature by feature", () => {
    setHostState({
      embedded: true,
      info: info({ signIn: { passkeys: true, microsoft: false } }),
    });
    expect(hostAllows("passkeys")).toBe(true);
    expect(hostAllows("microsoft")).toBe(false);
  });

  it("answers the listed commands only, a later one by its name too", () => {
    const listed = info({
      commands: ["host.info", "file.save"] as unknown as HostInfo["commands"],
    });
    setHostState({ embedded: true, info: listed });
    expect(hostAnswers("host.info")).toBe(true);
    expect(hostAnswers("file.save")).toBe(true);
    expect(hostAnswers("host.settings")).toBe(false);
  });

  it("saves no file until the host lists file.save", () => {
    // A browser saves a download itself.
    expect(hostSavesFiles()).toBe(true);
    setHostState({ embedded: true, info: null });
    expect(hostSavesFiles()).toBe(false);
    setHostState({ embedded: true, info: info() });
    expect(hostSavesFiles()).toBe(false);
    const later = info({
      commands: ["host.info", "file.save"] as unknown as HostInfo["commands"],
    });
    setHostState({ embedded: true, info: later });
    expect(hostSavesFiles()).toBe(true);
    expect(savesIn({ embedded: false, info: null })).toBe(true);
    expect(savesIn({ embedded: true, info: info() })).toBe(false);
  });
  it("sends a listed command through the bridge and an unlisted one not at all", async () => {
    const sent = vi.fn(async () => ({}));
    setHostAsk(sent as never);
    setHostState({ embedded: true, info: info({ commands: ["host.info", "host.settings"] }) });
    await expect(askHost("host.settings", {})).resolves.toEqual({});
    expect(sent).toHaveBeenCalledWith("host.settings", {});
    const error = await askHost("host.theme", { scheme: "dark", background: "#000000" }).catch(
      (e: unknown) => e,
    );
    expect((error as BridgeError).code).toBe("unknown_command");
    expect(sent).toHaveBeenCalledTimes(1);
  });
});

describe("the store", () => {
  it("keeps one object until something changes and tells its listeners then", () => {
    const seen = vi.fn();
    const stop = subscribe(seen);
    const before = hostState();
    expect(hostState()).toBe(before);
    setHostState({ embedded: true, info: null });
    expect(seen).toHaveBeenCalledTimes(1);
    expect(hostState()).not.toBe(before);
    expect(Object.isFrozen(hostState())).toBe(true);
    stop();
    setHostState({ embedded: true, info: info() });
    expect(seen).toHaveBeenCalledTimes(1);
  });

  it("tells its listeners when the page leaves the host, and only when it was inside", () => {
    const seen = vi.fn();
    const stop = subscribe(seen);
    leaveHost();
    expect(seen).not.toHaveBeenCalled();
    setHostState({ embedded: true, info: null });
    leaveHost();
    expect(seen).toHaveBeenCalledTimes(2);
    expect(hostState().embedded).toBe(false);
    stop();
  });

  it("works on a state that is passed in as well as on the current one", () => {
    const pending = { embedded: true, info: null };
    expect(allowsIn(pending, "passkeys")).toBe(false);
    expect(allowsIn({ embedded: false, info: null }, "passkeys")).toBe(true);
    expect(answersIn({ embedded: true, info: info() }, "host.theme")).toBe(true);
    expect(answersIn({ embedded: false, info: info() }, "host.theme")).toBe(false);
  });
});
