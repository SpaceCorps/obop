import { expect, test } from "../support/fixtures.ts";

// Helpdesk (wave 2, services, plan 3.5): a ticket goes from open to closed on a phone, one large
// button per step. Take assigns it to the person who takes it, Resolve asks how it was solved and
// dates it, Close asks for a confirmation and freezes the ticket. A ticket is named by its subject;
// its number sits above it on a card.

test.use({ viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true });

test("a ticket is taken, resolved and closed on a phone", async ({ page, signIn, api, uid }) => {
  const root = await api.token("root");
  await api.post("/api/marketplace_listing/install", { id: "helpdesk" }, root);
  try {
    const subject = uid("Ladder hinge cracked");
    const ticket = await api.create("root", "helpdesk_ticket", {
      subject,
      requester_name: "Claudia Brandt",
    });
    expect(ticket.priority).toBe("normal");
    expect(ticket.number).toMatch(/^TCK-\d{5}$/);

    // The board: the number above the subject.
    await signIn("root", "/helpdesk/helpdesk_ticket");
    const card = page.locator("article").filter({ hasText: subject });
    await expect(card).toContainText(ticket.number);
    await card.getByText(subject).click();

    const panel = page.locator("aside.record");
    await expect(panel.getByRole("heading", { name: subject })).toBeVisible();
    await expect(panel).toContainText(ticket.number);

    // Take: in progress and assigned to the person who took it.
    await panel.getByRole("button", { name: "Take", exact: true }).click();
    await expect(panel.getByRole("button", { name: "Resolve", exact: true })).toBeVisible();
    let now = await api.run("root", "helpdesk_ticket.get", { id: ticket.id });
    expect(now.state).toBe("in_progress");
    expect(now.assignee).toBe("root");

    // Resolve: asks for the resolution in a dialog.
    await panel.getByRole("button", { name: "Resolve", exact: true }).click();
    const dialog = page.getByRole("dialog", { name: "Resolve" });
    await dialog.getByLabel("Resolution").fill("Replaced the hinge under warranty.");
    await dialog.getByRole("button", { name: "Resolve" }).click();
    await expect(panel.getByRole("button", { name: /^Close ticket/ })).toBeVisible();
    now = await api.run("root", "helpdesk_ticket.get", { id: ticket.id });
    expect(now.state).toBe("resolved");
    expect(now.resolution).toBe("Replaced the hinge under warranty.");
    expect(now.resolved_on).toMatch(/^\d{4}-\d{2}-\d{2}$/);

    // Close: confirmed, then the ticket is frozen.
    await panel.getByRole("button", { name: /^Close ticket/ }).click();
    await page
      .getByRole("alertdialog")
      .getByRole("button", { name: /Close ticket|Confirm|Yes/ })
      .click();
    await expect
      .poll(async () => (await api.run("root", "helpdesk_ticket.get", { id: ticket.id })).state)
      .toBe("closed");
    await expect(panel).toContainText("A closed ticket cannot be changed.");
    await expect(panel.getByRole("textbox", { name: "Subject" })).toHaveCount(0);
    const sideways = await page.evaluate(
      () => document.documentElement.scrollWidth - window.innerWidth,
    );
    expect(sideways).toBe(0);
  } finally {
    await api.post("/api/module/uninstall", { id: "helpdesk" }, root);
  }
});
