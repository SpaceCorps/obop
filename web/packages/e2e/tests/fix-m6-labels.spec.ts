import { expect, test } from "../support/fixtures.ts";

// A package's form sections, action questions and done notes are shown in the language of the back
// office (docs/ISSUES.md R17): helpdesk translates its sections, the Close question and the notes
// after Take and Close in its label files.

test.describe("in German", () => {
  test.use({ language: "de" });
  test.setTimeout(90_000);

  test("helpdesk sections, its Close question and its notes are German (R17)", async ({
    page,
    signIn,
    api,
    uid,
  }) => {
    const root = await api.token("root");
    const had = ((await api.run("root", "module.list")) as any[]).some((m) => m.id === "helpdesk");
    if (!had) await api.run("root", "marketplace_listing.install", { id: "helpdesk" });
    try {
      const ticket = await api.create("root", "helpdesk_ticket", { subject: uid("Drucker") });
      await signIn("root", `/helpdesk/helpdesk_ticket?id=${ticket.id}`);
      const panel = page.locator("aside");
      await expect(panel.getByText("Anliegen", { exact: true })).toBeVisible();
      await expect(panel.getByText("Request", { exact: true })).toHaveCount(0);

      await panel.getByRole("button", { name: "Übernehmen", exact: true }).click();
      await expect(page.getByText("Ticket übernommen")).toBeVisible();

      await api.run("root", "helpdesk_ticket.resolve", {
        id: ticket.id,
        resolution: "Neu gestartet",
      });
      await page.reload();
      await panel.getByRole("button", { name: /^Ticket: Abschließen/ }).click();
      const ask = page.getByRole("alertdialog");
      await expect(ask).toContainText("Dieses Ticket endgültig schließen?");
      await ask.getByRole("button", { name: /Abschließen/ }).click();
      await expect(page.getByText("Ticket geschlossen")).toBeVisible();
    } finally {
      if (!had) await api.post("/api/module/uninstall", { id: "helpdesk" }, root);
    }
  });
});
