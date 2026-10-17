import { createServer, type IncomingMessage, type Server } from "node:http";
import { readFileSync } from "node:fs";
import type { AddressInfo } from "node:net";
import { expect, test, type Api } from "../support/fixtures.ts";

// The Connectors app, end to end in a real browser against the real server. The receiver below is a
// plain HTTP server on loopback that the test starts: nothing here talks to the internet.

type Hit = { url: string; headers: IncomingMessage["headers"]; body: string };

async function receiver(): Promise<{ url: string; hits: Hit[]; close: () => Promise<void> }> {
  const hits: Hit[] = [];
  const server: Server = createServer((req, res) => {
    const chunks: Buffer[] = [];
    req.on("data", (c) => chunks.push(c));
    req.on("end", () => {
      hits.push({
        url: req.url ?? "",
        headers: req.headers,
        body: Buffer.concat(chunks).toString(),
      });
      res.writeHead(200, { "content-type": "application/json" }).end("{}");
    });
  });
  await new Promise<void>((ok) => server.listen(0, "127.0.0.1", ok));
  const port = (server.address() as AddressInfo).port;
  return {
    url: `http://127.0.0.1:${port}/hook`,
    hits,
    close: () => new Promise((ok) => server.close(() => ok())),
  };
}

/** Make sure webhook_out is enabled for the instance and for people, with its agreement recorded. */
async function enableWebhook(api: Api) {
  const policies = await api.run("root", "provider_policy.list");
  const have = policies.find((p: any) => p.provider === "webhook_out");
  const body = {
    enabled: true,
    users_may_use: true,
    dpa_recorded: true,
    dpa_reference: "DPA signed for the e2e receiver",
    legal_basis_text: "We send the contact you choose to the receiver you named.",
  };
  if (have) await api.run("root", "provider_policy.update", { id: have.id, ...body });
  else await api.run("root", "provider_policy.create", { provider: "webhook_out", ...body });
}

const SECRET = "e2e-signing-secret-0123456789";

test.describe("Connectors", () => {
  test.beforeEach(async ({ api }) => {
    await enableWebhook(api);
  });

  test("the wizard connects a receiver and what arrives is pseudonymised, never a denied class", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    const rx = await receiver();
    try {
      const name = uid("Ada Receiver");
      const email = `${name.toLowerCase().replace(/\W+/g, ".")}@example.test`;
      const taxId = `DE${Date.now()}`;
      const party = await api.create("root", "party", {
        name,
        email,
        tax_id: taxId,
        notes: "likes tea",
      });
      const label = uid("Billing hook");

      await signIn("root", "/connectors/overview");
      await expect(page.getByRole("heading", { name: "Connections" })).toBeVisible();
      await page.getByRole("button", { name: "Connect a service" }).first().click();
      const dialog = page.getByRole("dialog", { name: "Connect a service" });

      // 1. The service. A definition-only entry cannot be picked at all.
      await dialog.getByRole("radio", { name: /Webhook \(send\)/ }).check();
      await dialog.getByRole("button", { name: "Next" }).click();

      // 2. Settings: write-only secret.
      await dialog.getByLabel(/^Name\b/).fill(label);
      await dialog.getByLabel("Endpoint URL").fill(rx.url);
      const secret = dialog.getByLabel("Signing secret");
      await expect(secret).toHaveAttribute("type", "password");
      await secret.fill(SECRET);
      await dialog.getByRole("button", { name: "Next" }).click();

      // 3. Fields, each with its class and what happens to it.
      await expect(
        dialog.getByText("Nothing is selected, so nothing would be sent."),
      ).toBeVisible();
      await dialog.getByLabel("Add a model").selectOption("party");
      const fieldRow = (field: string) =>
        dialog.locator(".cx-fields li", { hasText: new RegExp(`^\\s*${field}`, "i") });
      for (const f of ["Name", "Email", "Tax ID"])
        await dialog.getByLabel(new RegExp(`^${f}$`, "i")).check();
      await expect(fieldRow("Name")).toContainText("Sent as a pseudonym");
      await expect(fieldRow("Tax ID")).toContainText("Never sent");
      await expect(fieldRow("Tax ID")).toContainText("Financial");
      // The privacy preview shows a real record after the policy.
      const preview = dialog.getByRole("region", { name: "Privacy preview" });
      await expect(preview.locator("pre, code").first()).toContainText("psn_");
      await expect(preview).not.toContainText(taxId);
      await expect(preview.getByRole("row", { name: /tax_id/ })).toContainText("Never sent");
      await dialog.getByRole("button", { name: "Next" }).click();

      // 4. Purpose, legal basis and the classes that would flow.
      await expect(dialog.getByText("Data classes that would flow")).toBeVisible();
      await expect(
        dialog.getByRole("region", { name: "Data classes that would flow" }),
      ).toContainText("Directory data");
      await expect(
        dialog.getByRole("region", { name: "Data classes that would flow" }),
      ).not.toContainText("Financial");
      await dialog
        .getByRole("textbox", { name: "Purpose" })
        .fill("Keep the billing tool up to date with our customers");
      await dialog.getByRole("combobox", { name: "Legal basis" }).selectOption("contract");
      const save = dialog.getByRole("button", { name: "Save and continue" });
      await expect(save).toBeDisabled();
      await dialog.getByRole("checkbox", { name: /I confirm the purpose/ }).check();
      await save.click();

      // 5. Test, then connect.
      await dialog.getByRole("button", { name: "Test connection" }).click();
      await expect(dialog.getByText("The test passed")).toBeVisible();
      expect(rx.hits.length).toBe(1);
      expect(JSON.parse(rx.hits[0]!.body).type).toBe("obop.test");
      await dialog.getByRole("button", { name: "Connect", exact: true }).click();

      // The detail opens; send now.
      const detail = page.getByRole("dialog", { name: label });
      await expect(detail.getByText("Connected", { exact: true }).first()).toBeVisible();
      await detail.getByRole("button", { name: "Send now" }).click();
      await expect(detail.getByText("Done.")).toBeVisible();

      // What arrived: pseudonyms, no plain personal data, no denied class, a signature.
      expect(rx.hits.length).toBe(2);
      const hit = rx.hits[1]!;
      expect(hit.headers["webhook-signature"]).toMatch(/^v1,/);
      expect(hit.headers["webhook-id"]).toMatch(/^msg_/);
      const body = JSON.parse(hit.body);
      const sent = body.records.find((r: any) => r.name?.startsWith("psn_"));
      expect(sent, hit.body).toBeTruthy();
      expect(Object.keys(sent).sort()).toEqual(["email", "name", "ref"]);
      for (const plain of [name, email, taxId, "likes tea", party.id])
        expect(hit.body).not.toContain(plain);
      for (const h of rx.hits) expect(h.body).not.toContain(taxId);

      // The audit trail has the flow, without the data.
      await detail.getByRole("button", { name: "Close" }).click();
      await page.goto("/#/connectors/data_flow_log");
      const logRow = page.getByRole("row", { name: new RegExp(label) });
      await expect(logRow.first()).toContainText("tax_id");
      await expect(logRow.first()).toContainText("name,email");
      const logs = await api.run("root", "data_flow_log.list");
      const released = logs.filter((l: any) => l.outcome === "released" && l.model === "party");
      expect(released.length).toBeGreaterThan(0);
      expect(released.at(-1).fields_denied).toContain("tax_id");
      expect(JSON.stringify(logs)).not.toContain(name);
    } finally {
      await rx.close();
    }
  });

  test("a person withdraws consent in one click and the connection and its credentials are gone", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    const rx = await receiver();
    try {
      const label = uid("My hook");
      const made = await api.run("cora", "connection.create", {
        name: label,
        provider: "webhook_out",
        config: JSON.stringify({ url: rx.url }),
        credentials: JSON.stringify({ signing_secret: SECRET }),
        field_allowlist: JSON.stringify({ party: ["name"] }),
      });
      await api.run("cora", "consent.create", {
        connection: made.id,
        purpose: "Send my contacts to my own tool",
        legal_basis: "contract",
      });
      await api.run("cora", "connection.connect", { id: made.id });
      // Root's connection is not hers: she never sees it.
      const others = await api.run("root", "connection.list");
      expect(others.some((c: any) => c.id === made.id)).toBe(true);

      await signIn("cora", "/connectors/overview");
      await expect(page.getByRole("heading", { name: label })).toBeVisible();
      const cards = page.locator(".cx-card");
      await expect(cards).toHaveCount((await api.run("cora", "connection.list")).length);
      await page
        .locator(".cx-card", { hasText: label })
        .getByRole("button", { name: "Open" })
        .click();
      const detail = page.getByRole("dialog", { name: label });
      await expect(detail.getByText("Send my contacts to my own tool")).toBeVisible();
      await detail.getByRole("button", { name: "Withdraw and disconnect" }).click();
      await page
        .getByRole("alertdialog")
        .getByRole("button", { name: "Withdraw and disconnect" })
        .click();
      await expect(detail.getByText("Revoked", { exact: true }).first()).toBeVisible();
      await expect(detail.getByText(/The legal basis was withdrawn on/)).toBeVisible();

      const after = (await api.run("cora", "connection.list")).find((c: any) => c.id === made.id);
      expect(after.state).toBe("revoked");
      expect(after.credentials_hint).toBe("revoked");
      // Nothing flows after the withdrawal.
      const refused = await api.post(
        "/api/connection/sync",
        { id: made.id },
        await api.token("cora"),
      );
      expect(refused.status).toBe(400);
      expect(rx.hits.length).toBe(0);
    } finally {
      await rx.close();
    }
  });

  test("people see only their own connections, and the administration pages are for administrators", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    const mine = uid("Cora only");
    await api.run("cora", "connection.create", { name: mine, provider: "webhook_out" });
    const rootOnly = uid("Root only");
    await api.run("root", "connection.create", { name: rootOnly, provider: "webhook_out" });

    await signIn("cora", "/connectors/connection");
    await expect(page.getByRole("link", { name: "Processing register" })).toHaveCount(0);
    await expect(page.getByRole("link", { name: "Compliance" })).toHaveCount(0);
    await expect(page.getByRole("link", { name: "Egress policy" })).toHaveCount(0);
    await expect(page.getByText(mine).first()).toBeVisible();
    await expect(page.getByText(rootOnly)).toHaveCount(0);
    // The address of an administration page opens nothing for her.
    await page.goto("/#/connectors/register");
    await expect(page.getByRole("heading", { name: "Processing register" })).toHaveCount(0);
    // The API agrees.
    const sees = await api.run("cora", "connection.list");
    expect(sees.every((c: any) => c.owner_user === "cora")).toBe(true);
    const direct = await api.post("/api/processing_register/list", {}, await api.token("cora"));
    expect(direct.status).toBe(404);
  });

  test("the processing register exports as CSV without secrets, and the checklist says what it is not", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    const label = uid("Register row");
    await api.run("root", "connection.create", {
      name: label,
      provider: "webhook_out",
      credentials: JSON.stringify({ signing_secret: SECRET }),
    });
    await signIn("root", "/connectors/register");
    await expect(
      page.getByText("A starting point for your record of processing activities"),
    ).toBeVisible();
    await expect(page.getByRole("row", { name: new RegExp(label) })).toBeVisible();
    const download = page.waitForEvent("download");
    await page.getByRole("button", { name: "Export as CSV" }).click();
    const file = await download;
    expect(file.suggestedFilename()).toBe("processing-register.csv");
    const csv = readFileSync(await file.path(), "utf8");
    expect(csv.split("\r\n")[0]).toContain("connection,name,state");
    expect(csv).toContain(label);
    expect(csv).not.toContain(SECRET);
    expect(csv).not.toContain("obs1:");

    await page.goto("/#/connectors/compliance");
    await expect(page.getByText("It is not legal advice", { exact: false }).first()).toBeVisible();
    await expect(page.getByRole("list", { name: "Checklist" })).toBeVisible();
    await expect(page.getByText("Credentials are encrypted with OBOP_SECRET_KEY")).toBeVisible();
  });

  test("the catalogue says how much of each service exists, and the privacy preview works on its own", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    const name = uid("Preview Person");
    await api.create("root", "party", {
      name,
      email: "preview@example.test",
      tax_id: "DE999",
      notes: "private",
    });
    await signIn("root", "/connectors/catalog");
    const slack = page.locator(".cx-card[data-provider='slack_webhook']");
    await expect(slack).toContainText("Generic adapter");
    await expect(slack).toContainText("verify", { ignoreCase: true });
    const google = page.locator(".cx-card[data-provider='google_oauth']");
    await expect(google).toContainText("Definition only");
    await expect(google.getByRole("button", { name: "Connect" })).toHaveCount(0);
    await expect(page.locator(".cx-card[data-provider='webhook_out']")).toContainText("Working");
    await page.getByLabel("How much exists").selectOption("definition_only");
    await expect(page.locator(".cx-card")).toHaveCount(2);

    await page.goto("/#/connectors/preview");
    await page.getByLabel("Add a model").selectOption("party");
    for (const f of ["Name", "Email", "Notes", "Tax ID"])
      await page.getByLabel(new RegExp(`^${f}$`, "i")).check();
    const preview = page.getByRole("region", { name: "Privacy preview" });
    await expect(preview.locator("pre, code").first()).toContainText("psn_");
    await expect(preview.getByRole("row", { name: /tax_id/ })).toContainText("Never sent");
    await expect(preview).not.toContainText("DE999");
  });
});
