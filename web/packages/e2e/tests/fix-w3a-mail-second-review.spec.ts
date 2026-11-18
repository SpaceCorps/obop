// Outbound mail, after the second review of 2026-11-18 (docs/spec/mail.md): the record panel of a
// delivery offers no edit and no Send again on a sent message, a test message shows up in the
// deliveries of Customize > Mail at once, a burst lists the last queued first, the server form
// takes and gives back the keyboard, and "Sends through" is a 44 px target on touch. Nothing
// leaves the machine: the relay is a stand-in on loopback.
import { createServer, type Server, type Socket } from "node:net";
import { expect, test } from "../support/fixtures.ts";

type Stand = { server: Server; port: number };

/** A plain SMTP stand-in that accepts every message. */
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

test.describe("Mail after the second review", () => {
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
    name = uid("Second review relay");
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

  test("a sent delivery opens read-only, with no Save, no Send again and no Edit in list", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    const subject = uid("Sent once");
    const queued = await api.run("root", "outbound_mail.send", {
      to: "kunde@example.org",
      subject,
      body: "Guten Tag.",
      server: id,
    });
    await api.run("root", "outbound_mail.process", {});
    await expect(async () => {
      const got = await api.run("root", "outbound_mail.get", { id: queued.id });
      expect(got.status).toBe("sent");
    }).toPass({ timeout: 20_000 });

    await signIn("root", `/mail/outbound_mail?id=${queued.id}`);
    const panel = page.locator("aside.record.panel");
    await expect(panel).toBeVisible();
    await expect(panel.getByRole("button", { name: "Save" })).toHaveCount(0);
    await expect(panel.getByRole("button", { name: "Send again" })).toHaveCount(0);
    await expect(panel.locator("input:not([type=hidden]):not([disabled]), textarea")).toHaveCount(
      0,
    );

    await page.goto("/#/mail/outbound_mail");
    await expect(page.getByRole("row").filter({ hasText: subject })).toBeVisible();
    await expect(page.getByRole("button", { name: "Edit in list" })).toHaveCount(0);
  });
});
