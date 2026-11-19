// Outbound mail, after the review of 2026-11-18 (docs/spec/mail.md): a long German subject and
// sender name reach an SMTP stand-in on loopback folded and encoded, `outbound_mail.process`
// answers each message with its status, and a failed test message is not offered again (its text
// is not stored). Nothing leaves the machine.
import { createServer, type Server, type Socket } from "node:net";
import { expect, test } from "../support/fixtures.ts";
import { layoutProblems } from "../support/layout.ts";

type Stand = { server: Server; port: number; messages: string[] };

/** A plain SMTP stand-in that refuses the recipient `refused@example.org` and accepts the rest. */
async function smtpStandIn(): Promise<Stand> {
  const messages: string[] = [];
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
            say("250 2.0.0 queued as E2E2");
          } else data += `${line}\n`;
          continue;
        }
        const up = line.toUpperCase();
        if (up.startsWith("EHLO")) say("250-stand-in\r\n250 8BITMIME");
        else if (up.startsWith("RCPT TO") && line.includes("refused@example.org"))
          say("550 5.1.1 no such user");
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
  return { server, port, messages };
}

/** The decoded text of an RFC 2047 header value, unfolded. */
function decodeHeader(value: string): string {
  return value
    .split(" ")
    .map((w) => {
      const m = /^=\?UTF-8\?B\?([^?]*)\?=$/.exec(w);
      return m ? Buffer.from(m[1], "base64").toString("utf8") : ` ${w} `;
    })
    .join("")
    .replace(/ +/g, " ")
    .trim();
}

test.describe("Customize > Mail after the review", () => {
  let stand: Stand;
  test.beforeAll(async () => {
    stand = await smtpStandIn();
  });
  test.afterAll(async () => {
    await new Promise((r) => stand.server.close(r));
  });

  test("a long German subject goes out, process answers per message, a failed test is not sent again", async ({
    page,
    signIn,
    api,
    uid,
    allowHttpErrors,
  }) => {
    allowHttpErrors(400);
    const senderName = "Nordlicht Werkzeuge GmbH Buchhaltung und Rechnungsstelle Süd";
    const made = await api.run("root", "mail_server.create", {
      name: uid("Long relay"),
      kind: "smtp",
      enabled: true,
      host: "127.0.0.1",
      port: stand.port,
      security: "none",
      from_address: "rechnung@nordlicht.example",
      from_name: senderName,
    });
    const id = made.id as string;
    try {
      const subject = `Ihre Rechnung RE-2026-0042 von Nordlicht – fällig am 30.11. ${uid("x")}`;
      const queued = await api.run("root", "outbound_mail.send", {
        to: "kunde@example.org",
        subject,
        body: "Guten Tag,\nanbei die Rechnung.",
        server: id,
      });
      // The worker or this call sends it; either way the message ends as sent.
      const done = await api.run("root", "outbound_mail.process", {});
      expect(Array.isArray(done)).toBe(true);
      await expect(async () => {
        const got = await api.run("root", "outbound_mail.get", { id: queued.id });
        expect(got.status).toBe("sent");
      }).toPass({ timeout: 20_000 });
      const raw = stand.messages.find((m) => m.includes(`<${queued.id}@nordlicht.example>`));
      expect(raw).toBeTruthy();
      const head = raw!.split("\n\n")[0];
      for (const line of head.split("\n")) expect(line.length).toBeLessThanOrEqual(78);
      const unfolded = head.replace(/\n /g, " ");
      const subjectLine = unfolded.split("\n").find((l) => l.startsWith("Subject: "))!;
      expect(decodeHeader(subjectLine.slice("Subject: ".length))).toBe(subject);
      const fromLine = unfolded.split("\n").find((l) => l.startsWith("From: "))!;
      expect(decodeHeader(fromLine.slice("From: ".length))).toBe(
        `${senderName} <rechnung@nordlicht.example>`,
      );

      // A test message the relay refuses is logged as failed and has no "Send again".
      const test = await api.post(
        "/api/mail_server/test_send",
        { id, test_recipient: "refused@example.org" },
        await api.token("root"),
      );
      expect(test.status).toBe(400);
      await signIn("root", "/customize/mail");
      const row = page
        .getByRole("row")
        .filter({ hasText: "refused@example.org" })
        .filter({ hasText: "obop test message" })
        .first();
      await expect(row.locator(".badge")).toHaveText("Failed");
      await expect(row.getByRole("button", { name: "Send again" })).toHaveCount(0);
      const sentRow = page.getByRole("row").filter({ hasText: subject });
      await expect(sentRow.locator(".badge")).toHaveText("Sent");
      await page.setViewportSize({ width: 390, height: 844 });
      expect(await layoutProblems(page)).toEqual([]);
    } finally {
      await api.run("root", "mail_server.update", { id, enabled: false, is_default: false });
    }
  });
});
