import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { ssoEnabled, ssoRetry } from "./api.ts";

const json = (status: number, body: unknown) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

beforeEach(() => {
  ssoRetry.delays = [0, 0, 0];
  ssoRetry.slow = 0;
  ssoRetry.slowTries = 2;
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
    // The first ask, three quick retries and the slower ones after them.
    expect(busy).toHaveBeenCalledTimes(6);
    const broken = vi.fn().mockResolvedValue(json(500, {}));
    vi.stubGlobal("fetch", broken);
    expect(await ssoEnabled()).toBe(false);
    expect(broken).toHaveBeenCalledTimes(1);
  });

  it("keeps asking at a slower pace after the quick retries (UB4)", async () => {
    const fetchMock = vi.fn();
    for (let i = 0; i < 5; i++) fetchMock.mockResolvedValueOnce(json(429, {}));
    fetchMock.mockResolvedValueOnce(json(200, { enabled: true }));
    vi.stubGlobal("fetch", fetchMock);
    expect(await ssoEnabled()).toBe(true);
    expect(fetchMock).toHaveBeenCalledTimes(6);
  });
});
