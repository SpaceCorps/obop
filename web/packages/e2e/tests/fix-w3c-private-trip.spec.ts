import { expect, test, type Api } from "../support/fixtures.ts";

// Marking a trip private in the back office (docs/spec/mobile.md 5.6, rule A of 2026-11-18): the
// save removes the route for good, so the form asks first, as the app does; the warning above the
// map was out of sight where the purpose is picked (second review of issues wave w3c).

function walk(trip: string, seq: number) {
  const s = seq * 5;
  return {
    trip,
    seq,
    recorded_at: `2026-10-15T09:${String(Math.floor(s / 60)).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}Z`,
    lat: (52 + 0.0002 * seq).toFixed(7),
    lon: "13.0000000",
    accuracy_m: "5",
  };
}

/** An ended business trip of drew with ten points, not submitted; returns its id. */
async function endedTrip(api: Api, name: string): Promise<string> {
  const client = name.split(" ").join("-");
  const drew = await api.token("drew");
  const made = await api.post(
    "/api/mileage_trip/bulk_create",
    {
      idempotency_key: `trip:${client}`,
      rows: [{ client_trip_id: client, started_at: "2026-10-15T09:00:00Z", purpose: "business" }],
    },
    drew,
  );
  expect(made.status, JSON.stringify(made.json)).toBe(200);
  const id = made.json.value.ids[0] as string;
  const sent = await api.post(
    "/api/mileage_point/bulk_create",
    {
      idempotency_key: `points:${client}:1-10`,
      rows: Array.from({ length: 10 }, (_, i) => walk(id, i + 1)),
    },
    drew,
  );
  expect(sent.status, JSON.stringify(sent.json)).toBe(200);
  const end = await api.post(
    "/api/mileage_trip/end",
    { id, expected_points: 10, last_seq: 10 },
    drew,
  );
  expect(end.status, JSON.stringify(end.json)).toBe(200);
  return id;
}

async function tripOf(api: Api, id: string) {
  const r = await api.post("/api/mileage_trip/get", { id }, await api.token("drew"));
  expect(r.status, JSON.stringify(r.json)).toBe(200);
  return r.json.value as { purpose: string; point_count: number | string };
}

test("saving a trip as private asks first, and Keep editing saves nothing", async ({
  page,
  api,
  signIn,
  uid,
}) => {
  const id = await endedTrip(api, uid("private"));
  await signIn("drew", `/mileage/mileage_trip?view=list&id=${id}`);
  const record = page.getByRole("complementary", { name: /record$/ });
  await expect(record.locator(".trip-map svg")).toBeVisible();

  await record.getByLabel("Purpose", { exact: true }).selectOption("private");
  await record.getByRole("button", { name: "Save", exact: true }).click();
  const ask = page.getByRole("alertdialog");
  await expect(ask).toContainText("removes the route for good");

  // Keep editing: nothing is saved, the route is still there.
  await ask.getByRole("button", { name: "Keep editing" }).click();
  await expect(ask).toHaveCount(0);
  let now = await tripOf(api, id);
  expect(now.purpose).toBe("business");
  expect(Number(now.point_count)).toBe(10);

  // Save: the trip is private and its route is gone.
  await record.getByRole("button", { name: "Save", exact: true }).click();
  await page.getByRole("alertdialog").getByRole("button", { name: "Save" }).click();
  await expect(page.getByRole("alertdialog")).toHaveCount(0);
  await expect.poll(async () => (await tripOf(api, id)).purpose).toBe("private");
  now = await tripOf(api, id);
  expect(Number(now.point_count)).toBe(0);
});
