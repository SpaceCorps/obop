// The client against a real instance: `obop serve --dev-auth` on a free port and a throwaway
// SQLite database, started by this file and stopped when it ends. It runs only when OBOP_BIN names
// the binary (the e2e job of CI builds it; locally `cargo build`), so `vp test` on its own skips it:
//
//   OBOP_BIN=/path/to/obop vp test integration
import { spawn, type ChildProcess } from "node:child_process";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { afterAll, beforeAll, describe, expect, test, vi } from "vite-plus/test";
import {
  ObopApiError,
  ObopNetworkError,
  createObopClient,
  isSafeToRetry,
  type ObopClient,
} from "../src/index.ts";

const binary = process.env.OBOP_BIN ? resolve(process.env.OBOP_BIN) : undefined;

/** A port the operating system has just handed out and nobody holds. */
function freePort(): Promise<number> {
  return new Promise((ok, fail) => {
    const server = createServer();
    server.once("error", fail);
    server.listen(0, "127.0.0.1", () => {
      const { port } = server.address() as { port: number };
      server.close(() => ok(port));
    });
  });
}

describe.skipIf(!binary)("against a running instance", () => {
  let child: ChildProcess | undefined;
  let exited: Promise<void> = Promise.resolve();
  let dir = "";
  let baseUrl = "";
  let log = "";

  const make = (token: string, onUnauthorized?: () => void): ObopClient =>
    createObopClient({ baseUrl, getToken: () => token, onUnauthorized });

  beforeAll(async () => {
    if (!binary || !existsSync(binary)) throw new Error(`OBOP_BIN ${binary} does not exist`);
    dir = mkdtempSync(join(tmpdir(), "obop-client-"));
    const port = await freePort();
    baseUrl = `http://127.0.0.1:${port}`;
    const server = spawn(
      binary,
      [
        "serve",
        "--dev-auth",
        "--addr",
        `127.0.0.1:${port}`,
        "--db",
        `sqlite://${join(dir, "client.sqlite")}`,
      ],
      {
        stdio: ["ignore", "ignore", "pipe"],
        // The tests make a handful of bulk calls; the default allows 12 a minute.
        env: { ...process.env, OBOP_BULK_PER_MINUTE: "0" },
      },
    );
    child = server;
    server.stderr?.on("data", (chunk: Buffer) => {
      log += chunk.toString();
    });
    exited = new Promise((done) => server.once("exit", () => done()));
    // The server is up when it answers; until then the connection is refused.
    await vi.waitFor(
      async () => {
        if (server.exitCode !== null) throw new Error(`obop exited early:\n${log}`);
        const res = await fetch(`${baseUrl}/healthz`);
        if (!res.ok) throw new Error(`healthz answered ${res.status}`);
      },
      { timeout: 45_000, interval: 100 },
    );
  }, 60_000);

  afterAll(async () => {
    child?.kill();
    await exited;
    if (dir) rmSync(dir, { recursive: true, force: true, maxRetries: 10, retryDelay: 100 });
  });

  const count = async (client: ObopClient) =>
    ((await client.call<unknown[]>("party", "list")).value ?? []).length;

  test("getMeta lists the modules, and party can be created in bulk", async () => {
    const meta = await make("dev-admin").getMeta();
    expect(meta.core_version).toMatch(/^\d+\.\d+\.\d+/);
    const party = meta.modules.flatMap((m) => m.models).find((m) => m.name === "party");
    expect(party).toMatchObject({ can_create: true, can_bulk_create: true });
    expect(party?.fields.map((f) => f.name)).toContain("name");
  });

  test("a read: list answers the demo records", async () => {
    const { value, denied } = await make("dev-admin").call<{ id: string; name?: string }[]>(
      "party",
      "list",
    );
    expect(value.length).toBeGreaterThan(0);
    expect(typeof value[0]!.id).toBe("string");
    expect(denied).toEqual([]);
  });

  test("the typed openapi-fetch client reads the same list", async () => {
    const { data } = await make("dev-admin").api.POST("/api/party/list");
    // A 202 (an agent only proposing) has no `value`; this token is a person's.
    expect(data && "value" in data ? data.value?.length : 0).toBeGreaterThan(0);
  });

  test("a dry-run bulk_create checks the rows and creates nothing", async () => {
    const client = make("dev-admin");
    const before = await count(client);
    const report = await client.bulkCreate(
      "party",
      [{ name: "Client dry run one" }, { name: "Client dry run two" }],
      { dryRun: true },
    );
    // A dry run reports `valid` where a real call reports `created`, and has no ids yet.
    expect(report).toMatchObject({ ok: true, dry_run: true, total: 2, valid: 2, failed: 0 });
    expect(report.created).toBeUndefined();
    expect(report.ids).toBeUndefined();
    expect(await count(client)).toBe(before);
  });

  test("a dry run reports a bad row, and a real call with it writes nothing", async () => {
    const client = make("dev-admin");
    const before = await count(client);
    // `name` is required; the schema types it, so the row is sent through the untyped form.
    const rows = [{ name: "Client bad row ok" }, { email: "no.name@example.com" }];
    const dry = await client.bulkCreate("party", rows as never, { dryRun: true }).catch((e) => e);
    const real = await client.bulkCreate("party", rows as never).catch((e) => e);
    for (const outcome of [dry, real]) {
      const failed =
        outcome instanceof ObopApiError ? outcome.status === 400 : outcome.ok === false;
      expect(failed, JSON.stringify(outcome)).toBe(true);
    }
    expect(await count(client)).toBe(before);
  });

  test("an idempotency key makes a repeat return the first answer and create nothing more", async () => {
    const client = make("dev-admin");
    const before = await count(client);
    const key = `client-test-${crypto.randomUUID()}`;
    const rows = [{ name: `Client replay ${key}` }];
    const first = await client.bulkCreate("party", rows, { idempotencyKey: key });
    expect(first).toMatchObject({ ok: true, created: 1 });
    expect(first.replayed ?? false).toBe(false);
    expect(first.ids).toHaveLength(1);
    const again = await client.bulkCreate("party", rows, { idempotencyKey: key });
    expect(again.replayed).toBe(true);
    expect(again.ids).toEqual(first.ids);
    expect(await count(client)).toBe(before + 1);
    // The same key with other rows is refused, and says so.
    const other = await client
      .bulkCreate("party", [{ name: "Client other rows" }], { idempotencyKey: key })
      .catch((e) => e);
    expect(other).toBeInstanceOf(ObopApiError);
    expect(other.status).toBe(400);
    expect(other.message).toContain("idempotency_key");
  });

  test("a token the instance does not know is a 401 and signs the app out once", async () => {
    const onUnauthorized = vi.fn();
    const e = await make("not-a-token", onUnauthorized)
      .call("party", "list")
      .catch((x) => x);
    expect(e).toBeInstanceOf(ObopApiError);
    expect(e.status).toBe(401);
    expect(onUnauthorized).toHaveBeenCalledTimes(1);
    expect(isSafeToRetry(e)).toBe(false);
  });

  test("a model that does not exist is a 404", async () => {
    const e = await make("dev-admin")
      .call("no_such_model", "list")
      .catch((x) => x);
    expect(e).toBeInstanceOf(ObopApiError);
    expect(e).toMatchObject({ status: 404, message: "not found" });
  });

  test("an address nobody listens on is a network error that a read may repeat", async () => {
    const closed = `http://127.0.0.1:${await freePort()}`;
    const client = createObopClient({ baseUrl: closed, getToken: () => "dev-admin" });
    const read = await client.call("party", "list").catch((e) => e);
    expect(read).toBeInstanceOf(ObopNetworkError);
    expect(read).toMatchObject({ status: 0, code: "network_error", applied: "none" });
    expect(isSafeToRetry(read)).toBe(true);
    const write = await client.bulkCreate("party", [{ name: "x" }]).catch((e) => e);
    expect(write).toBeInstanceOf(ObopNetworkError);
    expect(write.applied).toBe("unknown");
    expect(isSafeToRetry(write)).toBe(false);
    expect(isSafeToRetry(write, { idempotent: true })).toBe(true);
  });
});
