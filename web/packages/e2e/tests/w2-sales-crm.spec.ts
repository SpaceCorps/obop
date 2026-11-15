import { expect, test } from "../support/fixtures.ts";

// From an opportunity to a confirmed order (UI plan 3.3): "New quotation" on a lead asks for the
// customer, makes the quotation linked to the lead and opens it in Sales; a product line takes its
// price; Confirm makes the draft invoice, which the order links to. Losing a deal asks for why.

test.describe("CRM to Sales", () => {
  test("an opportunity becomes a quotation, then a confirmed order with its invoice", async ({
    page,
    api,
    signIn,
    uid,
  }) => {
    const customer = await api.create("root", "party", { name: uid("Werft") });
    const product = await api.create("root", "product", {
      name: uid("Safety boots"),
      list_price_minor: 8900,
    });
    const lead = await api.create("root", "lead", {
      title: uid("PPE framework"),
      contact_name: "Birgit Lorenz",
    });

    await signIn("root", `/crm/lead?id=${lead.id}`);
    await page.getByRole("button", { name: "New quotation" }).click();
    const ask = page.getByRole("dialog", { name: "Who is the customer?" });
    await ask.getByLabel("Customer").fill(customer.name);
    await page.getByRole("option", { name: new RegExp(customer.name) }).click();
    await ask.getByRole("button", { name: "Create quotation" }).click();

    await expect(page).toHaveURL(/#\/sales\/sale_order\?id=/);
    const quote = (await api.list("root", "sale_order")).find((o) => o.lead_id === lead.id);
    expect(quote.party_id).toBe(customer.id);
    expect((await api.run("root", "lead.get", { id: lead.id })).party_id).toBe(customer.id);

    // A line from the product: its price fills in.
    await page.getByRole("button", { name: "Add a line" }).first().click();
    await page.keyboard.type(product.name);
    await page.getByRole("option", { name: new RegExp(product.name) }).click();
    // In the side panel a line is edited in its own sheet.
    const line = page.getByRole("dialog", { name: "New line" });
    await expect(line.getByLabel("Unit price")).toHaveValue("89.00");
    await line.getByRole("button", { name: "Add", exact: true }).click();
    await expect(page.locator(".record")).toContainText("€89.00");
    await expect
      .poll(async () => (await api.run("root", "sale_order.get", { id: quote.id })).total_minor)
      .toBe(8900);

    await page.getByRole("button", { name: "Confirm", exact: true }).click();
    await expect(page.getByText("A draft invoice is ready")).toBeVisible();
    await page.getByRole("button", { name: "Open invoice" }).click();
    await expect(page).toHaveURL(/#\/invoicing\/invoice\?id=/);
    const invoice = (await api.list("root", "invoice")).find((i) => i.sale_order_id === quote.id);
    expect(invoice.party_id).toBe(customer.id);
  });

  test("marking an opportunity lost asks for the reason and keeps it", async ({
    page,
    api,
    signIn,
    uid,
  }) => {
    const lead = await api.create("root", "lead", { title: uid("Lost deal") });
    await signIn("root", `/crm/lead?id=${lead.id}`);
    await page.getByRole("button", { name: /More actions/ }).click();
    await page.getByRole("menuitem", { name: "Mark lost" }).click();
    const dlg = page.getByRole("dialog", { name: "Mark lost" });
    await dlg.getByLabel("Lost reason").fill("Chose a cheaper offer");
    await dlg.getByRole("button", { name: "Mark lost" }).click();
    await expect
      .poll(async () => (await api.run("root", "lead.get", { id: lead.id })).stage)
      .toBe("lost");
    const after = await api.run("root", "lead.get", { id: lead.id });
    expect(after.lost_reason).toBe("Chose a cheaper offer");
  });
});
