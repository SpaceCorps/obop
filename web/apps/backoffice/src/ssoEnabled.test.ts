import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { ssoEnabled, ssoRetry } from "./api.ts";

const json = (status: number, body: unknown) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

beforeEach(() => {
  ssoRetry.delays = [0, 0, 0];
});
afterEach(() => vi.unstubAllGlobals());

describe("ssoEnabled", () => {
  it("asks again after a 429 instead of hiding the Microsoft button (N19)", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(json(429, { error: "too many requests" }))
      .mockResolvedValueOnce(json(429, { error: "too many requests" }))
      .mockResolvedValueOnce(json(200, { enabled: true }));
    vi.stubGlobal("fetch", fetchMock);
    expect(await ssoEnabled()).toBe(true);
    expect(fetchMock).toHaveBeenCalledTimes(3);
  });

  it("gives up after its retries and offers nothing on any other doubt", async () => {
    const busy = vi.fn().mockImplementation(async () => json(429, {}));
    vi.stubGlobal("fetch", busy);
    expect(await ssoEnabled()).toBe(false);
    expect(busy).toHaveBeenCalledTimes(4);
    const broken = vi.fn().mockResolvedValue(json(500, {}));
    vi.stubGlobal("fetch", broken);
    expect(await ssoEnabled()).toBe(false);
    expect(broken).toHaveBeenCalledTimes(1);
  });
});
