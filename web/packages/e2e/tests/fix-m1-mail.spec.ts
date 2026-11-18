// Outbound mail (docs/spec/mail.md): a mail server is added in Customize > Mail, a test message and
// a queued message reach an SMTP stand-in on loopback (the e2e server runs with
// OBOP_CONNECTORS_ALLOW_LOOPBACK=1), the deliveries log shows them, and the password is never
// returned. Nothing leaves the machine.
import { createServer, type Server, type Socket } from "node:net";
import { expect, test } from "../support/fixtures.ts";
import { layoutProblems } from "../support/layout.ts";

type Stand = { server: Server; port: number; messages: string[]; commands: string[] };

/** A plain SMTP stand-in that accepts every message and keeps what it got. */
async function smtpStandIn(): Promise<Stand> {
  const messages: string[] = [];
  const commands: string[] = [];
  const server = createServer((socket: Socket) => {
    let buf = "";
    let data: string | null = null;
    const say = (t: string) => socket.write(`${t}\r\n`);
    say("220 stand-in ready");
    socket.on("data", (chunk) => {
      buf += chunk.toString("utf8");
      let i: number;
      while ((i = buf.indexOf("\r\n")) >= 0) {
        const line = buf.slice(0, i);
        buf = buf.slice(i + 2);
        if (data !== null) {
          if (line === ".") {
            messages.push(data);
            data = null;
            say("250 2.0.0 queued as E2E1");
          } else data += `${line}\n`;
          continue;
        }
        commands.push(line);
        const up = line.toUpperCase();
        if (up.startsWith("EHLO")) say("250-stand-in\r\n250 8BITMIME");
        else if (up === "DATA") {
          data = "";
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
  const port = typeof addr === "object" && addr ? addr.port : 0;
  return { server, port, messages, commands };
}

test.describe("Customize > Mail", () => {
  let stand: Stand;
  test.beforeAll(async () => {
    stand = await smtpStandIn();
  });
  test.afterAll(async () => {
    await new Promise((r) => stand.server.close(r));
  });

  test("add an SMTP server, send a test message, queue one and see it delivered", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    const name = uid("Relay");
    let id = "";
    try {
      await signIn("root", "/customize/mail");
      await expect(page.getByRole("heading", { name: "Mail servers" })).toBeVisible();
      await page.getByRole("button", { name: "Add mail server" }).click();
      const form = page.getByRole("form", { name: "Add mail server" });
      await form.getByRole("textbox", { name: "Name", exact: true }).fill(name);
      await form.getByRole("textbox", { name: "Host", exact: true }).fill("127.0.0.1");
      await form.getByRole("combobox", { name: "Security" }).selectOption("none");
      await form.getByRole("textbox", { name: "Port", exact: true }).fill(String(stand.port));
      await form
        .getByRole("textbox", { name: "Sender address", exact: true })
        .fill("rechnung@nordlicht.example");
      await form
        .getByRole("textbox", { name: "Sender name", exact: true })
        .fill("Nordlicht Werkzeuge GmbH");
      await form.getByRole("button", { name: "Add mail server" }).click();
      await expect(page.locator(".toast").filter({ hasText: "Mail server added." })).toBeVisible();
      const card = page.locator(".mail-server").filter({ hasText: name });
      await expect(card).toContainText(`SMTP 127.0.0.1:${stand.port}`);
      const servers = await api.list("root", "mail_server");
      id = servers.find((s) => s.name === name).id;

      // A test message goes out at once and the answer is recorded.
      await card.getByRole("button", { name: "Send test" }).click();
      await card.getByLabel("Send a test message to").fill("ada@example.org");
      await card
        .getByRole("form", { name: "Send a test message" })
        .getByRole("button", { name: "Send test" })
        .click();
      await expect(card.getByRole("status")).toHaveText(
        "The test message was sent to ada@example.org.",
      );
      expect(stand.messages.length).toBe(1);
      expect(stand.messages[0]).toContain(
        "From: Nordlicht Werkzeuge GmbH <rechnung@nordlicht.example>",
      );
      await expect(card).toContainText("Last test");

      // A message queued through the command (as any module would) is sent by the worker.
      const subject = uid("Rechnung");
      const queued = await api.run("root", "outbound_mail.send", {
        to: "kunde@example.org",
        subject,
        body: "Guten Tag,\nanbei die Rechnung.",
        server: id,
        kind: "invoice",
      });
      expect(queued.status).toBe("queued");
      const row = page.getByRole("row").filter({ hasText: subject });
      await expect(async () => {
        await page.getByRole("button", { name: "Refresh" }).click();
        await expect(row.locator(".badge")).toHaveText("Sent", { timeout: 1000 });
      }).toPass({ timeout: 20_000 });
      expect(stand.commands).toContain("RCPT TO:<kunde@example.org>");
      // The log never carries the text.
      const got = await api.run("root", "outbound_mail.get", { id: queued.id });
      expect(JSON.stringify(got)).not.toContain("anbei die Rechnung");
      expect(got.status).toBe("sent");

      // Phone width: nothing sticks out.
      await page.setViewportSize({ width: 390, height: 844 });
      expect(await layoutProblems(page)).toEqual([]);
    } finally {
      // A server its log refers to is kept; switched off it sends nothing and is chosen for nothing.
      if (id)
        await api.run("root", "mail_server.update", { id, enabled: false, is_default: false });
    }
  });

  test("the password is sealed and never returned; a password without TLS is refused", async ({
    api,
    allowHttpErrors,
    uid,
  }) => {
    allowHttpErrors(400);
    const token = await api.token("root");
    const plain = await api.post(
      "/api/mail_server/create",
      {
        name: uid("Plain"),
        kind: "smtp",
        host: "smtp.example.com",
        security: "none",
        username: "u",
        password: "do-not-send-this",
        from_address: "a@example.com",
      },
      token,
    );
    expect(plain.status).toBe(400);
    expect(JSON.stringify(plain.json)).toContain("never sent without TLS");
    const made = await api.run("root", "mail_server.create", {
      name: uid("Sealed"),
      kind: "smtp",
      host: "smtp.example.com",
      security: "tls",
      username: "u",
      password: "e2e-secret-password-1",
      from_address: "a@example.com",
      enabled: false,
    });
    try {
      const got = await api.run("root", "mail_server.get", { id: made.id });
      expect(JSON.stringify(got)).not.toContain("e2e-secret-password-1");
      expect(got.password_set).toBe(true);
    } finally {
      await api.run("root", "mail_server.delete", { id: made.id });
    }
  });
});
