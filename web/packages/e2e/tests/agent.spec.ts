import { expect, test } from "../support/fixtures.ts";

test.describe("agent tokens over the REST API", () => {
  test("a human issues a scoped token; the agent sees masked personal data and withheld fields", async ({
    api,
    uid,
  }) => {
    const sam = await api.token("sam");
    const name = uid("Agent Party");
    await api.create("sam", "party", {
      name,
      email: "agent.party@example.test",
      tax_id: "DE123456789",
      notes: "private",
    });

    const issued = await api.post(
      "/api/auth/agent-token",
      { scopes: ["base.user"], label: "e2e agent" },
      sam,
    );
    expect(issued.status).toBe(200);
    const token: string = issued.json.token;
    expect(token).toMatch(/^obop_[0-9a-f]{64}$/);

    const agent = await api.post("/api/party/list", {}, token);
    expect(agent.status).toBe(200);
    // tax_id is withheld outright; the other personal fields come back as stable mask tokens.
    expect(agent.json.denied).toContain("party.tax_id");
    expect(agent.json.value.length).toBeGreaterThan(0);
    for (const p of agent.json.value) {
      expect(p).not.toHaveProperty("tax_id");
      expect(p.name).toMatch(/^NAME_\d+$/);
      expect(p.email).toMatch(/^EMAIL_\d+$/);
      expect(p.notes).toMatch(/^NOTES_\d+$/);
    }
    const again = await api.post("/api/party/list", {}, token);
    expect(again.json.value.map((p: any) => p.name)).toEqual(
      agent.json.value.map((p: any) => p.name),
    );
    expect(JSON.stringify(agent.json)).not.toContain(name);

    // The human who issued it sees the real values.
    const human = (await api.list("sam", "party")).find((p) => p.name === name);
    expect(human).toMatchObject({ email: "agent.party@example.test", tax_id: "DE123456789" });
  });

  test("a token only narrows its owner: out-of-scope commands are 404 and a wider scope cannot be issued", async ({
    api,
  }) => {
    const sam = await api.token("sam");
    const token = (
      await api.post("/api/auth/agent-token", { scopes: ["base.user"], label: "narrow" }, sam)
    ).json.token;
    expect((await api.post("/api/lead/list", {}, token)).status).toBe(404);
    expect((await api.post("/api/sale_order/list", {}, token)).status).toBe(404);
    expect((await api.post("/api/party/list", {}, token)).status).toBe(200);

    const wider = await api.post(
      "/api/auth/agent-token",
      { scopes: ["invoicing.user"], label: "too wide" },
      sam,
    );
    expect(wider.status).toBe(400);
    expect(wider.json.error).toMatch(/cannot delegate/);
    // An agent cannot mint another agent.
    const nested = await api.post("/api/auth/agent-token", { scopes: ["base.user"] }, token);
    expect(nested.status).toBeGreaterThanOrEqual(400);
  });

  test("an approval-needed command answers 202 for an agent, only a human can approve it, then it runs", async ({
    api,
  }) => {
    const root = await api.token("root");
    const agent = (
      await api.post(
        "/api/auth/agent-token",
        { scopes: ["platform.admin"], label: "installer" },
        root,
      )
    ).json.token;
    try {
      const proposed = await api.post(
        "/api/marketplace_listing/install",
        { id: "documents" },
        agent,
      );
      expect(proposed.status).toBe(202);
      expect(proposed.json.approval_required).toBe(true);
      const plan: string = proposed.json.plan_id;
      expect(plan).toBeTruthy();

      // Nothing happened yet, and the agent cannot approve its own plan.
      expect((await api.run("root", "module.list")).map((m: any) => m.id)).not.toContain(
        "documents",
      );
      expect((await api.post(`/api/approve/${plan}`, {}, agent)).status).toBe(404);
      expect((await api.run("root", "module.list")).map((m: any) => m.id)).not.toContain(
        "documents",
      );

      // The signed-in human approves; the module is installed.
      const approved = await api.post(`/api/approve/${plan}`, {}, root);
      expect(approved.status).toBe(200);
      expect(approved.json.value.id).toBe("documents");
      expect((await api.run("root", "module.list")).map((m: any) => m.id)).toContain("documents");
      // A plan runs once.
      expect((await api.post(`/api/approve/${plan}`, {}, root)).status).toBe(404);
    } finally {
      await api.post("/api/module/uninstall", { id: "documents" }, root);
    }
  });

  // BUG (not fixed here): `denied` gets one entry per row instead of one per field, so a list of N rows
  // returns the same withheld field N times (and the UI banner reads "Withheld from you: Tax, Tax").
  // Reproduce: as an agent token scoped base.user, POST /api/party/list when two parties exist.
  // Cause: obop-commands/src/lib.rs, Dispatcher::shape pushes into `denied` for every row.
  test("a list names each withheld field once, however many rows it has", async ({ api, uid }) => {
    await api.create("sam", "party", { name: uid("Dup A") });
    await api.create("sam", "party", { name: uid("Dup B") });
    const sam = await api.token("sam");
    const token = (
      await api.post("/api/auth/agent-token", { scopes: ["base.user"], label: "dup" }, sam)
    ).json.token;
    const r = await api.post("/api/party/list", {}, token);
    expect(r.json.denied).toEqual(["party.tax_id"]);
  });

  test("in the UI, an agent's session shows mask tokens and the list of withheld fields", async ({
    page,
    api,
  }) => {
    const sam = await api.token("sam");
    const token = (
      await api.post("/api/auth/agent-token", { scopes: ["base.user"], label: "ui agent" }, sam)
    ).json.token;
    await page.addInitScript(
      ([t, u]) => {
        sessionStorage.setItem("obop-token", t!);
        sessionStorage.setItem("obop-user", u!);
      },
      [token, "agent"],
    );
    await page.goto("/#/base/party?view=list");
    const grid = page.getByRole("grid", { name: "Party list" });
    await expect(grid.getByRole("gridcell", { name: /^NAME_\d+$/ }).first()).toBeVisible();
    await expect(page.getByText(/Withheld from you: Tax/)).toBeVisible();
    await grid
      .getByRole("gridcell", { name: /^NAME_\d+$/ })
      .first()
      .click();
    const panel = page.getByRole("complementary", { name: "Party record" });
    await expect(panel.getByText("Masked for this session").first()).toBeVisible();
  });
});
