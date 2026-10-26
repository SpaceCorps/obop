import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { call, meta } from "./api.ts";
import { NETWORK_MESSAGE, NetworkError, backoff, request } from "./net.ts";

const json = (status: number, body: unknown) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

beforeEach(() => {
  backoff.delays = [0, 0];
});
afterEach(() => vi.unstubAllGlobals());

describe("request", () => {
  it("retries an idempotent read when the connection drops, then succeeds", async () => {
    const fetchMock = vi
      .fn()
      .mockRejectedValueOnce(new TypeError("fetch failed"))
      .mockResolvedValueOnce(json(200, { ok: true }));
    vi.stubGlobal("fetch", fetchMock);
    const res = await request("/x", {}, { retry: true });
    expect(res.status).toBe(200);
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it("retries a gateway error but never a client error", async () => {
    const gateway = vi
      .fn()
      .mockResolvedValueOnce(json(503, {}))
      .mockResolvedValueOnce(json(200, {}));
    vi.stubGlobal("fetch", gateway);
    expect((await request("/x", {}, { retry: true })).status).toBe(200);
    expect(gateway).toHaveBeenCalledTimes(2);

    const bad = vi.fn().mockResolvedValue(json(400, { error: "no" }));
    vi.stubGlobal("fetch", bad);
    expect((await request("/x", {}, { retry: true })).status).toBe(400);
    expect(bad).toHaveBeenCalledTimes(1);
  });

  it("waits for the Retry-After of a busy database before it reads again", async () => {
    const busy = () =>
      new Response(JSON.stringify({ error: "storage_busy", code: "storage_busy" }), {
        status: 503,
        headers: { "content-type": "application/json", "retry-after": "0.06" },
      });
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(busy())
      .mockResolvedValueOnce(json(200, { ok: true }));
    vi.stubGlobal("fetch", fetchMock);
    const t0 = Date.now();
    expect((await request("/x", {}, { retry: true })).status).toBe(200);
    expect(Date.now() - t0).toBeGreaterThanOrEqual(50);
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it("does not run a statement again that ran into its time limit", async () => {
    const timeout = () =>
      new Response(JSON.stringify({ error: "storage_timeout", code: "storage_timeout" }), {
        status: 503,
        headers: { "content-type": "application/json", "retry-after": "5" },
      });
    const fetchMock = vi.fn().mockResolvedValue(timeout());
    vi.stubGlobal("fetch", fetchMock);
    const res = await request("/x", {}, { retry: true });
    expect(res.status).toBe(503);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("gives up after the delays and throws a NetworkError", async () => {
    const fetchMock = vi.fn().mockRejectedValue(new TypeError("fetch failed"));
    vi.stubGlobal("fetch", fetchMock);
    await expect(request("/x", {}, { retry: true })).rejects.toBeInstanceOf(NetworkError);
    expect(fetchMock).toHaveBeenCalledTimes(3);
  });

  it("does not retry a write", async () => {
    const fetchMock = vi.fn().mockRejectedValue(new TypeError("fetch failed"));
    vi.stubGlobal("fetch", fetchMock);
    await expect(request("/x", { method: "POST" })).rejects.toBeInstanceOf(NetworkError);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });
});

describe("commands", () => {
  it("answer a dropped connection with an error reply, never a rejection", async () => {
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new TypeError("fetch failed")));
    expect(await call("tok", "lead.update", { id: "1" })).toEqual({
      kind: "error",
      status: 0,
      message: NETWORK_MESSAGE,
    });
  });

  it("reads are retried, writes are not", async () => {
    const fetchMock = vi
      .fn()
      .mockRejectedValueOnce(new TypeError("fetch failed"))
      .mockResolvedValueOnce(json(200, { value: { rows: [], total: 0 } }));
    vi.stubGlobal("fetch", fetchMock);
    expect((await call("tok", "lead.search", {})).kind).toBe("ok");
    expect(fetchMock).toHaveBeenCalledTimes(2);

    const write = vi.fn().mockRejectedValue(new TypeError("fetch failed"));
    vi.stubGlobal("fetch", write);
    await call("tok", "lead.create", {});
    expect(write).toHaveBeenCalledTimes(1);
  });

  it("describe a crash that answers with a page instead of JSON", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(new Response("<html>oops</html>", { status: 500 })),
    );
    const r = await call("tok", "lead.create", {});
    expect(r).toEqual({
      kind: "error",
      status: 500,
      message: "The server had a problem (status 500)",
    });
  });

  it("keep a client error's own message", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(json(400, { error: "lead.title is required" })),
    );
    const r = await call("tok", "lead.create", {});
    expect(r).toMatchObject({ status: 400, message: "lead.title is required" });
  });
});

describe("meta", () => {
  it("says failed, not empty, when the server is down or crashes", async () => {
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new TypeError("fetch failed")));
    expect(await meta("tok")).toBe("failed");
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response("boom", { status: 500 })));
    expect(await meta("tok")).toBe("failed");
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(json(401, {})));
    expect(await meta("tok")).toBeNull();
  });
});
