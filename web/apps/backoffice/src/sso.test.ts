import { afterEach, describe, expect, it, vi } from "vitest";
import { forgetPlace, parseSsoReturn, rememberPlace, restorePlace } from "./api.ts";

const CODE = "obopsso_" + "0123456789abcdef".repeat(4);

describe("the return from Microsoft (D91)", () => {
  it("reads the one-time code of the fragment", () => {
    expect(parseSsoReturn(`#oidc=${CODE}`)).toEqual({ code: CODE });
  });

  it("reads the four words of a failure and turns an unknown word into the general one", () => {
    for (const w of ["cancelled", "unavailable", "busy", "failed"])
      expect(parseSsoReturn(`#oidc_error=${w}`)).toEqual({ error: w });
    expect(parseSsoReturn("#oidc_error=whatever")).toEqual({ error: "failed" });
  });

  it("ignores everything else, including a router address and a code of the wrong shape", () => {
    for (const h of [
      "",
      "#",
      "#/crm/lead",
      "#oidc=",
      "#oidc=abc",
      `#oidc=${CODE}&x=1`,
      `#oidc=${CODE}/../`,
      `#/oidc=${CODE}`,
      "#oidc_error=",
      "#oidc_error=Failed",
      "#oidc_error=failed&x=1",
      `#oidc=<script>${CODE}`,
    ])
      expect(parseSsoReturn(h)).toBeNull();
  });
});

function tab(hash: string) {
  const store = new Map<string, string>();
  const loc = { hash, pathname: "/", search: "" };
  vi.stubGlobal("location", loc);
  vi.stubGlobal("history", { replaceState: () => undefined });
  vi.stubGlobal("sessionStorage", {
    getItem: (k: string) => store.get(k) ?? null,
    setItem: (k: string, v: string) => void store.set(k, v),
    removeItem: (k: string) => void store.delete(k),
  });
  return { loc, store };
}

describe("the deep link across the round trip to Microsoft (D91)", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.resetModules();
  });

  it("keeps a router address in the tab and goes back to it once", () => {
    const { loc, store } = tab("#/sales/order?id=42");
    rememberPlace();
    expect(store.size).toBe(1);
    loc.hash = "";
    restorePlace();
    expect(loc.hash).toBe("#/sales/order?id=42");
    loc.hash = "";
    restorePlace();
    expect(loc.hash, "once").toBe("");
  });

  it("keeps nothing for an address that is not a router address", () => {
    for (const h of ["", "#", "#oidc=abc", "#/x y", "#/<script>", "#//" + "a".repeat(600)]) {
      const { loc, store } = tab(h);
      rememberPlace();
      expect(store.size, h).toBe(0);
      restorePlace();
      expect(loc.hash).toBe(h);
    }
  });

  it("forgets a place when the round trip failed, so a later password sign-in does not use it", () => {
    const { loc, store } = tab("#/crm/lead");
    rememberPlace();
    forgetPlace();
    expect(store.size).toBe(0);
    loc.hash = "";
    restorePlace();
    expect(loc.hash).toBe("");
  });

  it("drops a return that nobody handles, so it cannot surface after a sign-out", async () => {
    tab("#oidc=" + CODE);
    const api = await import("./api.ts");
    api.discardSsoReturn();
    expect(api.takeSsoReturn()).toBeNull();
    // The control: left alone, the return is handed out once.
    vi.resetModules();
    tab("#oidc=" + CODE);
    const other = await import("./api.ts");
    expect(other.takeSsoReturn()).toEqual({ code: CODE });
    expect(other.takeSsoReturn()).toBeNull();
  });
});
