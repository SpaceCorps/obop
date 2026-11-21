// Outbound mail, stream n6 (docs/spec/mail.md): Send and Send due messages are commands of the
// deliveries as a whole, so the list offers them and a delivery's panel does not (U66); the last
// test of a server is worded from a code with the server's answer as the detail (U63); no answer
// carries the sealed body, an administrator's included (U58). Nothing leaves the machine: the
// relay is a stand-in on loopback that refuses one address.
import { createServer, type Server, type Socket } from "node:net";
import { expect, test } from "../support/fixtures.ts";

type Stand = { server: Server; port: number };

/** A plain SMTP stand-in that accepts every message except one to refused@example.org. */
async function smtpStandIn(): Promise<Stand> {
  const server = createServer((socket: Socket) => {
    let buf = "";
    let data = false;
    const say = (t: string) => socket.write(`${t}\r\n`);
    say("220 stand-in ready");
    socket.on("data", (chunk) => {
      buf += chunk.toString("utf8");
      let i: number;
      while ((i = buf.indexOf("\r\n")) >= 0) {
        const line = buf.slice(0, i);
        buf = buf.slice(i + 2);
        if (data) {
          if (line === ".") {
            data = false;
            say("250 2.0.0 queued");
          }
          continue;
        }
        const up = line.toUpperCase();
        if (up.startsWith("EHLO")) say("250-stand-in\r\n250 8BITMIME");
        else if (up.startsWith("RCPT") && up.includes("REFUSED@")) say("550 5.1.1 no such user");
        else if (up === "DATA") {
          data = true;
          say("354 go on");
        } else if (up === "QUIT") {
          say("221 bye");
          socket.end();
        } else say("250 ok");
      }
    });
    socket.on("error", () => undefined);
  });
  await new Promise<void>((r) => server.listen(0, "127.0.0.1", r));
  const addr = server.address();
  return { server, port: typeof addr === "object" && addr ? addr.port : 0 };
}

test.describe("Mail, stream n6", () => {
  let stand: Stand;
  let id = "";
  let name = "";
  test.beforeAll(async () => {
    stand = await smtpStandIn();
  });
  test.afterAll(async () => {
    await new Promise((r) => stand.server.close(r));
  });
  test.beforeEach(async ({ api, uid }) => {
    name = uid("n6 relay");
    const made = await api.run("root", "mail_server.create", {
      name,
      kind: "smtp",
      enabled: true,
      host: "127.0.0.1",
      port: stand.port,
      security: "none",
      from_address: "rechnung@nordlicht.example",
    });
    id = made.id as string;
  });
  test.afterEach(async ({ api }) => {
    await api.run("root", "mail_server.update", { id, enabled: false, is_default: false });
  });

  test("the list offers Send and Send due messages, a delivery's panel does not", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    const subject = uid("Queued");
    const queued = await api.run("root", "outbound_mail.send", {
      to: "kunde@example.org",
      subject,
      body: "Guten Tag.",
      server: id,
    });
    const got = await api.run("root", "outbound_mail.get", { id: queued.id });
    expect(got).not.toHaveProperty("body");

    await signIn("root", `/mail/outbound_mail?id=${queued.id}`);
    const panel = page.locator("aside.record.panel");
    await expect(panel).toBeVisible();
    await expect(panel.locator("[data-model-action]")).toHaveCount(0);
    await expect(panel.getByRole("button", { name: "Send due messages" })).toHaveCount(0);
    await expect(panel.getByRole("button", { name: /^Send$/ })).toHaveCount(0);

    await page.goto("/#/mail/outbound_mail");
    const send = page.locator("[data-model-action='outbound_mail.send']");
    const due = page.locator("[data-model-action='outbound_mail.process']");
    await expect(send).toBeVisible();
    await expect(due).toBeVisible();

    // Send asks for the message and queues it.
    const second = uid("From the list");
    await send.click();
    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();
    await dialog.getByLabel("To", { exact: false }).first().fill("kunde@example.org");
    await dialog.getByLabel("Subject", { exact: false }).first().fill(second);
    await dialog.locator("textarea").first().fill("Hallo aus der Liste.");
    await dialog
      .getByRole("button", { name: /Send|Confirm|OK/ })
      .last()
      .click();
    await expect(async () => {
      const found = await api.run("root", "outbound_mail.search", {
        order: [{ field: "id", dir: "desc" }],
        limit: 50,
      });
      const rows = found.rows as { subject: string }[];
      expect(rows.filter((r) => r.subject === second)).toHaveLength(1);
      expect(rows.every((r) => !("body" in r))).toBe(true);
    }).toPass({ timeout: 15_000 });

    // Send due messages runs the worker now.
    await due.click();
    await expect(async () => {
      const g = await api.run("root", "outbound_mail.get", { id: queued.id });
      expect(g.status).toBe("sent");
    }).toPass({ timeout: 20_000 });
  });

  test("a mail sender who is not a platform administrator queues a message from the list", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    await signIn("mel", "/mail/outbound_mail");
    const send = page.locator("[data-model-action='outbound_mail.send']");
    await expect(send).toBeVisible();
    await send.click();
    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();
    const subject = uid("From mel");
    await dialog.getByLabel("To", { exact: false }).first().fill("kunde@example.org");
    await dialog.getByLabel("Subject", { exact: false }).first().fill(subject);
    // The text is asked for (before, only a holder of mail.system was asked for it).
    const text = dialog.locator("textarea");
    await expect(text).toHaveCount(1);
    await text.fill("Hallo von Mel.");
    await dialog.getByRole("button", { name: /Send/ }).last().click();
    await expect(dialog).toBeHidden();
    await expect(async () => {
      const found = await api.run("root", "outbound_mail.search", {
        filter: [{ field: "subject", op: "eq", value: subject }],
      });
      expect(found.rows as unknown[]).toHaveLength(1);
    }).toPass({ timeout: 15_000 });
    // The sealed text is never a filter, for her or for an administrator.
    const probe = await api.post(
      "/api/outbound_mail/search",
      { filter: [{ field: "body", op: "contains", value: "a" }] },
      await api.token("mel"),
    );
    expect(probe.status).toBe(400);
  });

  test("the last test is worded from its outcome with the server's answer as the detail", async ({
    page,
    signIn,
    api,
  }) => {
    await api
      .run("root", "mail_server.test_send", { id, test_recipient: "refused@example.org" })
      .catch(() => undefined);
    const srv = await api.run("root", "mail_server.get", { id });
    expect(srv.last_test_outcome).toBe("permanent");

    await signIn("root", "/customize/mail");
    const line = page
      .locator("[data-test-outcome=permanent]")
      .filter({ has: page.locator(".mail-detail") })
      .first();
    await expect(line).toContainText("Not sent (refused)");
    await expect(line).toContainText("550");
  });
});
