// @vitest-environment jsdom
// The host as a component reads it: drawn again when the host's answer arrives, and not otherwise.
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it } from "vite-plus/test";
import type { HostInfo } from "@obop/host-bridge";
import { leaveHost, setHostState } from "./state.ts";
import { useHost, type Host } from "./useHost.ts";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const INFO: HostInfo = {
  bridge: 1,
  platform: "android",
  app: { id: "org.obop.app.test", version: "0.1.0", cleartext: false, inspectable: false },
  commands: ["host.info"],
  signIn: { passkeys: false, microsoft: true },
};

let container: HTMLDivElement;
let root: Root;
let seen: Host[];

function Probe() {
  const host = useHost();
  seen.push(host);
  return (
    <p>
      {host.embedded ? "embedded" : "browser"} {host.allows("passkeys") ? "passkeys" : ""}{" "}
      {host.allows("microsoft") ? "microsoft" : ""}
    </p>
  );
}

const words = () => (container.textContent ?? "").split(/\s+/).filter(Boolean);

beforeEach(async () => {
  seen = [];
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  await act(async () => root.render(<Probe />));
});
afterEach(() => {
  act(() => root.unmount());
  container.remove();
  leaveHost();
});

describe("useHost", () => {
  it("allows everything in a browser", () => {
    expect(words()).toEqual(["browser", "passkeys", "microsoft"]);
    expect(seen.at(-1)!.info).toBeNull();
  });

  it("draws the component again when the page is embedded, and again when the host has answered", async () => {
    await act(async () => setHostState({ embedded: true, info: null }));
    expect(words()).toEqual(["embedded"]);
    await act(async () => setHostState({ embedded: true, info: INFO }));
    expect(words()).toEqual(["embedded", "microsoft"]);
    expect(seen.at(-1)!.info).toBe(INFO);
  });

  it("gives the same object while nothing changed", async () => {
    const before = seen.at(-1)!;
    await act(async () => root.render(<Probe />));
    expect(seen.at(-1)).toBe(before);
  });
});
