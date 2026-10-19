import type { APIRequestContext } from "@playwright/test";
import { expect, test } from "../support/fixtures.ts";

// A minimal Streamable HTTP MCP client (spec revision 2025-11-25) over Playwright's request context.
class McpClient {
  session = "";
  private next = 1;
  constructor(
    private request: APIRequestContext,
    private token: string,
  ) {}

  private headers(extra: Record<string, string> = {}) {
    return {
      authorization: `Bearer ${this.token}`,
      accept: "application/json, text/event-stream",
      "content-type": "application/json",
      ...(this.session ? { "mcp-session-id": this.session } : {}),
      ...extra,
    };
  }

  async initialize() {
    const res = await this.request.post("/mcp", {
      headers: this.headers(),
      data: {
        jsonrpc: "2.0",
        id: this.next++,
        method: "initialize",
        params: {
          protocolVersion: "2025-11-25",
          capabilities: {},
          clientInfo: { name: "obop-e2e", version: "0" },
        },
      },
    });
    expect(res.status()).toBe(200);
    this.session = res.headers()["mcp-session-id"] ?? "";
    expect(this.session).toMatch(/^[0-9a-f]{64}$/);
    const body = await res.json();
    const ack = await this.request.post("/mcp", {
      headers: this.headers(),
      data: { jsonrpc: "2.0", method: "notifications/initialized" },
    });
    expect(ack.status()).toBe(202);
    return body.result;
  }

  async rpc(method: string, params: object = {}) {
    const res = await this.request.post("/mcp", {
      headers: this.headers(),
      data: { jsonrpc: "2.0", id: this.next++, method, params },
    });
    return { status: res.status(), json: await res.json() };
  }

  async tools() {
    const names: any[] = [];
    let cursor: string | undefined;
    do {
      const r = await this.rpc("tools/list", cursor ? { cursor } : {});
      expect(r.status).toBe(200);
      names.push(...r.json.result.tools);
      cursor = r.json.result.nextCursor;
    } while (cursor);
    return names;
  }

  async call(name: string, args: object = {}) {
    const r = await this.rpc("tools/call", { name, arguments: args });
    expect(r.status).toBe(200);
    return r.json;
  }
}

async function agentToken(api: any, scopes: string[], label: string): Promise<string> {
  const sam = await api.token("sam");
  const issued = await api.post("/api/auth/agent-token", { scopes, label }, sam);
  expect(issued.status).toBe(200);
  return issued.json.token;
}

test.describe("remote MCP over Streamable HTTP", () => {
  test("an agent token connects, lists typed tools and reads masked data with stable tokens", async ({
    api,
    request,
    uid,
  }) => {
    const name = uid("Mcp Party");
    await api.create("sam", "party", { name, email: "mcp.party@example.test" });
    const mcp = new McpClient(request, await agentToken(api, ["base.user"], "mcp e2e"));

    const init = await mcp.initialize();
    expect(init.protocolVersion).toBe("2025-11-25");
    expect(init.serverInfo.name).toBe("obop");
    expect(init.capabilities.tools.listChanged).toBe(false);
    expect(init.instructions).toContain("approval_required");

    const tools = await mcp.tools();
    const names = tools.map((t) => t.name);
    expect(names).toContain("obop__meta");
    expect(names).toContain("party__search");
    // The token is narrowed to base.user: no CRM tools, exactly like REST answers 404 there.
    expect(names.some((n) => n.startsWith("lead__"))).toBe(false);
    const search = tools.find((t) => t.name === "party__search");
    expect(search.inputSchema.properties.limit).toMatchObject({ type: "integer", maximum: 5000 });
    expect(search.annotations.readOnlyHint).toBe(true);
    expect(tools.find((t) => t.name === "party__delete").annotations.destructiveHint).toBe(true);
    for (const t of tools) expect(t.inputSchema.type).toBe("object");

    const first = await mcp.call("party__list");
    expect(first.result.isError).toBe(false);
    const rows = first.result.structuredContent.value as any[];
    expect(first.result.structuredContent.denied).toContain("party.tax_id");
    expect(rows.length).toBeGreaterThan(0);
    for (const p of rows) expect(p.name).toMatch(/^NAME_\d+$/);
    expect(JSON.stringify(first)).not.toContain(name);
    // Mask tokens are stable across calls of one session ...
    const second = await mcp.call("party__list");
    expect(second.result.structuredContent.value).toEqual(rows);
    // ... and a different session of the same token starts its own vault.
    const other = new McpClient(request, (mcp as any).token);
    await other.initialize();
    const third = await other.call("party__list");
    expect(third.result.structuredContent.value.length).toBe(rows.length);

    const meta = await mcp.call("obop__meta");
    expect(meta.result.structuredContent.modules.length).toBeGreaterThan(0);

    // Unknown and hidden tools are the same protocol error.
    const hidden = await mcp.rpc("tools/call", { name: "lead__list", arguments: {} });
    const missing = await mcp.rpc("tools/call", { name: "nothing__list", arguments: {} });
    expect(hidden.json.error.code).toBe(-32602);
    expect(missing.json.error.code).toBe(-32602);
  });

  test("an approval-needed call returns a plan; a human approves it through REST and it runs once", async ({
    api,
    request,
    uid,
  }) => {
    const title = uid("Mcp Lead");
    const lead = await api.create("sam", "lead", { title });
    const mcp = new McpClient(request, await agentToken(api, ["crm.user"], "mcp delete"));
    await mcp.initialize();

    const proposed = await mcp.call("lead__delete", { id: lead.id });
    const out = proposed.result.structuredContent;
    expect(proposed.result.isError).toBe(false);
    expect(out.status).toBe("approval_required");
    expect(out.instruction).toMatch(/human/);
    expect((await api.list("sam", "lead")).some((l) => l.id === lead.id)).toBe(true);

    // The agent cannot approve its own plan.
    const self = await api.post(`/api/approve/${out.plan_id}`, {}, (mcp as any).token);
    expect(self.status).toBe(404);

    const sam = await api.token("sam");
    const approved = await api.post(`/api/approve/${out.plan_id}`, {}, sam);
    expect(approved.status).toBe(200);
    expect((await api.list("sam", "lead")).some((l) => l.id === lead.id)).toBe(false);
    const again = await api.post(`/api/approve/${out.plan_id}`, {}, sam);
    expect(again.status).toBe(404);

    // A validation error is a tool error, not a protocol error.
    const bad = await mcp.call("lead__create", { title: 5 });
    expect(bad.result.isError).toBe(true);
    expect(bad.result.structuredContent.error).toBeTruthy();
  });

  test("auth, origin and session rules; discovery documents", async ({
    api,
    request,
    allowHttpErrors,
  }) => {
    allowHttpErrors(401, 403, 404);
    const ping = { jsonrpc: "2.0", id: 1, method: "ping" };
    const headers = { accept: "application/json, text/event-stream" };

    const anon = await request.post("/mcp", { data: ping, headers });
    expect(anon.status()).toBe(401);
    expect(anon.headers()["www-authenticate"]).toMatch(
      /^Bearer resource_metadata="http:\/\/127\.0\.0\.1:\d+\/\.well-known\/oauth-protected-resource\/mcp"$/,
    );
    const forged = await request.post("/mcp", {
      data: ping,
      headers: { ...headers, authorization: "Bearer obop_forged" },
    });
    expect(forged.status()).toBe(401);

    const token = await agentToken(api, ["base.user"], "mcp rules");
    const evil = await request.post("/mcp", {
      data: ping,
      headers: { ...headers, authorization: `Bearer ${token}`, origin: "https://evil.example" },
    });
    expect(evil.status()).toBe(403);

    const mcp = new McpClient(request, token);
    await mcp.initialize();
    expect((await mcp.rpc("ping")).status).toBe(200);
    const get = await request.get("/mcp", { headers: { authorization: `Bearer ${token}` } });
    expect(get.status()).toBe(405);
    expect(get.headers()["allow"]).toContain("POST");
    const end = await request.delete("/mcp", {
      headers: { authorization: `Bearer ${token}`, "mcp-session-id": mcp.session },
    });
    expect(end.status()).toBe(204);
    expect((await mcp.rpc("ping")).status).toBe(404);

    const prm = await (await request.get("/.well-known/oauth-protected-resource")).json();
    // The instance is its own authorization server (D64).
    expect(prm.authorization_servers).toEqual([new URL(prm.resource).origin]);
    expect(prm.bearer_methods_supported).toEqual(["header"]);
    const card = await (await request.get("/.well-known/mcp/server-card.json")).json();
    expect(card.remotes[0]).toMatchObject({ type: "streamable-http" });
    expect(card.remotes[0].url).toMatch(/\/mcp$/);
    const openapi = await (await request.get("/api/openapi.json")).json();
    expect(openapi["x-mcp"].url).toMatch(/\/mcp$/);
    const meta = await request.get("/api/meta", { headers: { authorization: `Bearer ${token}` } });
    expect(meta.headers()["link"]).toContain('rel="mcp"');
  });
});
