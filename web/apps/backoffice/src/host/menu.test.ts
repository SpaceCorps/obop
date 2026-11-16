// The entry "This app" of the account menu: there only inside a host that answers `host.settings`,
// and pressing it asks the host for its own screen.
import { afterEach, describe, expect, it, vi } from "vite-plus/test";
import type { HostInfo } from "@obop/host-bridge";
import { hostMenuItems } from "./menu.ts";
import { leaveHost, setHostAsk, setHostState } from "./state.ts";

const info = (commands: HostInfo["commands"]): HostInfo => ({
  bridge: 1,
  platform: "ios",
  app: { id: "org.obop.app.test", version: "0.1.0", cleartext: false, inspectable: false },
  commands,
  signIn: { passkeys: false, microsoft: false },
});

afterEach(() => leaveHost());

describe("the account menu's entries of the host", () => {
  it("are none in a browser", () => {
    expect(hostMenuItems()).toEqual([]);
  });

  it("are none until the host has answered, and none when it does not list the command", () => {
    setHostState({ embedded: true, info: null });
    expect(hostMenuItems()).toEqual([]);
    setHostState({ embedded: true, info: info(["host.info", "host.theme"]) });
    expect(hostMenuItems()).toEqual([]);
  });

  it("are one entry, This app, which asks the host for its screen", async () => {
    const sent = vi.fn(async () => ({}));
    setHostAsk(sent as never);
    setHostState({ embedded: true, info: info(["host.info", "host.settings"]) });
    const items = hostMenuItems();
    expect(items).toHaveLength(1);
    const entry = items[0] as { label: string; icon?: string; onSelect: () => void };
    expect(entry.label).toBe("This app");
    expect(entry.icon).toBe("info");
    entry.onSelect();
    expect(sent).toHaveBeenCalledWith("host.settings", {});
  });

  it("says nothing and throws nothing when the host does not open it", async () => {
    setHostAsk((() => Promise.reject(new Error("refused"))) as never);
    setHostState({ embedded: true, info: info(["host.settings"]) });
    expect(() => (hostMenuItems()[0] as { onSelect: () => void }).onSelect()).not.toThrow();
    // A rejection that nobody handled would fail the run of this file.
    await new Promise((done) => setTimeout(done, 20));
  });
});
