import { expect, test } from "../support/fixtures.ts";

// Fleet (wave 2 stage B, UI plan 3.6): the home lists the driving licences that expire within 30
// days or have expired, and the planned services soonest first, next to where the vehicles are.
// A vehicle sent to the workshop is counted under Maintenance. Every figure is a search an agent
// can make too.

const day = (offset: number) => {
  const d = new Date();
  d.setDate(d.getDate() + offset);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
};

test("the fleet home shows expiring licences, planned services and the workshop", async ({
  page,
  signIn,
  api,
  uid,
}) => {
  const root = await api.token("root");
  await api.post("/api/marketplace_listing/install", { id: "fleet" }, root);
  try {
    const soon = await api.create("root", "fleet_vehicle_driver", {
      name: uid("Sven Soon"),
      licence_expiry: day(10),
      state: "active",
    });
    const gone = await api.create("root", "fleet_vehicle_driver", {
      name: uid("Ella Expired"),
      licence_expiry: day(-3),
      state: "active",
    });
    const fine = await api.create("root", "fleet_vehicle_driver", {
      name: uid("Finn Fine"),
      licence_expiry: day(400),
      state: "active",
    });
    const van = await api.create("root", "fleet_vehicle", {
      plate: uid("HH-T"),
      model: "Test van",
      odometer_km: 1200,
      state: "available",
    });
    const service = await api.create("root", "fleet_vehicle_service", {
      vehicle_id: van.id,
      kind: "inspection",
      service_on: day(5),
      state: "planned",
    });
    // A new service takes the vehicle's odometer reading.
    expect(service.odometer_km).toBe(1200);
    await api.run("root", "fleet_vehicle.send_to_workshop", { id: van.id });

    await signIn("root", "/fleet/overview");
    const licences = page.getByRole("article", { name: "Licences expiring" });
    await expect(licences.getByRole("link", { name: new RegExp(soon.name) })).toContainText(
      "Expires in 10 days",
    );
    await expect(licences.getByRole("link", { name: new RegExp(gone.name) })).toContainText(
      "Expired 3 days ago",
    );
    await expect(licences.getByRole("link", { name: new RegExp(fine.name) })).toHaveCount(0);
    const services = page.getByRole("article", { name: "Planned services" });
    await expect(services.getByRole("link", { name: new RegExp(van.plate) })).toContainText(
      "In 5 days",
    );
    const workshop = page.getByRole("link", { name: /^Maintenance: \d+/ });
    await expect(workshop).toBeVisible();
    expect(Number((await workshop.getAttribute("aria-label"))?.match(/\d+/)?.[0])).toBeGreaterThan(
      0,
    );

    // A reminder opens the driver.
    await licences.getByRole("link", { name: new RegExp(gone.name) }).click();
    await expect(page).toHaveURL(new RegExp(`fleet_vehicle_driver\\?id=${gone.id}`));

    // On a phone the home has no sideways scroll.
    await page.setViewportSize({ width: 320, height: 700 });
    await page.goto("/#/fleet/overview");
    await expect(page.getByRole("article", { name: "Planned services" })).toBeVisible();
    const sideways = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(sideways).toBe(0);
  } finally {
    await api.post("/api/module/uninstall", { id: "fleet" }, root);
  }
});

test("on a tablet the four vehicle tiles stand two by two (R10)", async ({ page, signIn, api }) => {
  const root = await api.token("root");
  await api.post("/api/marketplace_listing/install", { id: "fleet" }, root);
  try {
    await page.setViewportSize({ width: 834, height: 1112 });
    await signIn("root", "/fleet/overview");
    const tiles = page.locator(".fleet-home .kpistrip > *");
    await expect(tiles).toHaveCount(4);
    await expect(page.locator(".fleet-home .kpistrip[aria-busy]")).toHaveCount(0);
    const boxes = await tiles.evaluateAll((els) =>
      els.map((e) => {
        const r = e.getBoundingClientRect();
        return { top: Math.round(r.top), left: Math.round(r.left) };
      }),
    );
    expect(new Set(boxes.map((b) => b.top)).size).toBe(2);
    expect(new Set(boxes.map((b) => b.left)).size).toBe(2);
    expect(boxes[0]!.top).toBe(boxes[1]!.top);
    expect(boxes[2]!.top).toBe(boxes[3]!.top);
  } finally {
    await api.post("/api/module/uninstall", { id: "fleet" }, root);
  }
});
