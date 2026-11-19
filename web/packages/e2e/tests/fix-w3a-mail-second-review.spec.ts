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

  test("a test message shows in Deliveries at once", async ({ page, signIn, uid }) => {
    await signIn("root", "/customize/mail");
    const card = page.locator(".mail-server").filter({ hasText: name });
    await card.getByRole("button", { name: "Send test" }).click();
    const to = `${uid("probe")
      .toLowerCase()
      .replace(/[^a-z0-9]/g, "")}@example.org`;
    await card.getByLabel("Send a test message to").fill(to);
    await card.getByRole("button", { name: "Send test" }).last().click();
    // No Refresh and no reload: the test row is in the table.
    await expect(page.getByRole("row").filter({ hasText: to })).toBeVisible();
  });

  test("a burst within one second lists the last queued first", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    const tag = uid("burst");
    // queued_at has whole seconds: queued together, the five share one or two of them.
    await Promise.all(
      [0, 1, 2, 3, 4].map((i) =>
        api.run("root", "outbound_mail.send", {
          to: `burst${i}@example.org`,
          subject: `${tag} ${i}`,
          body: "x",
          server: id,
        }),
      ),
    );
    // The order the ids (UUIDv7) give, newest first.
    const found = await api.run("root", "outbound_mail.search", {
      order: [{ field: "id", dir: "desc" }],
      limit: 50,
    });
    const newest = (found.rows as { subject: string }[]).filter((r) => r.subject.startsWith(tag));
    expect(newest).toHaveLength(5);
    await signIn("root", "/customize/mail");
    const shown = page.getByRole("row").filter({ hasText: tag });
    await expect(shown).toHaveCount(5);
    await expect(shown.first()).toContainText(newest[0]!.subject);
    await expect(shown.last()).toContainText(newest[4]!.subject);
  });

  test("the server form takes the keyboard and gives it back", async ({ page, signIn }) => {
    await signIn("root", "/customize/mail");
    const add = page.getByRole("button", { name: "Add mail server" });
    await add.focus();
    await page.keyboard.press("Enter");
    const form = page.getByRole("form", { name: "Add mail server" });
    await expect(form.getByRole("textbox", { name: "Name", exact: true })).toBeFocused();
    await form.getByRole("button", { name: "Cancel" }).click();
    await expect(page.getByRole("button", { name: "Add mail server" })).toBeFocused();

    const card = page.locator(".mail-server").filter({ hasText: name });
    await card.getByRole("button", { name: "Edit" }).click();
    const edit = page.getByRole("form", { name: "Edit mail server" });
    await expect(edit.getByRole("textbox", { name: "Name", exact: true })).toBeFocused();
    await edit.getByRole("button", { name: "Cancel" }).click();
    await expect(card.getByRole("button", { name: "Edit" })).toBeFocused();
  });
});

test.describe("Mail on touch", () => {
  test.use({ viewport: { width: 320, height: 720 }, hasTouch: true, isMobile: true });

  test("Sends through is a 44 px target", async ({ page, signIn }) => {
    await signIn("root", "/customize/mail");
    await page.getByRole("button", { name: "Add mail server" }).click();
    const form = page.getByRole("form", { name: "Add mail server" });
    for (const name of ["SMTP relay", "Provider"]) {
      const box = await form.getByRole("radio", { name }).boundingBox();
      expect(box!.height, name).toBeGreaterThanOrEqual(44);
    }
    await form.getByRole("button", { name: "Cancel" }).click();
  });
});
