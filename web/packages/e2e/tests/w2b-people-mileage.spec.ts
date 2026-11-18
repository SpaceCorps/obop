import { expect, test, type Api } from "../support/fixtures.ts";

// Mileage (wave 2 stage B, UI plan 3.6): the reviewer's screen. Mileage opens on the trips to review,
// without New and Import (trips come from the phone app); a trip shows its route drawn from its
// points, who drove when and how far, and the server's check of the track in a sentence; Reject asks
// for the reason in the same step (B4). The trip is sent through the API with the driver's token, as
// the app sends it.

function walk(trip: string, seq: number) {
  const s = seq * 5;
  return {
    trip,
    seq,
    recorded_at: `2026-10-15T09:${String(Math.floor(s / 60)).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}Z`,
    lat: (52 + 0.0002 * seq).toFixed(7),
    lon: (13 + 0.0001 * (seq % 3)).toFixed(7),
    accuracy_m: "5",
  };
}

async function submittedTrip(api: Api, name: string): Promise<string> {
  const client = name.split(" ").join("-");
  const drew = await api.token("drew");
  const made = await api.post(
    "/api/mileage_trip/bulk_create",
    {
      idempotency_key: `trip:${client}`,
      rows: [
        {
          client_trip_id: client,
          started_at: "2026-10-15T09:00:00Z",
          purpose: "business",
          vehicle: "HH-W 2",
        },
      ],
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
  const sub = await api.post("/api/mileage_trip/submit", { id }, drew);
  expect(sub.status, JSON.stringify(sub.json)).toBe(200);
  return id;
}

test("a reviewer reads a trip's route and check, and rejects it with a reason", async ({
  page,
  api,
  signIn,
  uid,
}) => {
  const id = await submittedTrip(api, uid("review"));

  // Mileage opens on the trips to review, and offers no New or Import.
  await signIn("mina", "/mileage");
  await expect(page).toHaveURL(/mileage_trip\?st=/);
  await expect(page.getByRole("button", { name: "New", exact: true })).toHaveCount(0);
  await expect(page.getByRole("button", { name: /^Import/ })).toHaveCount(0);
  const grid = page.getByRole("grid");
  await expect(grid.getByRole("row", { name: /drew/ }).first()).toBeVisible();

  await page.goto(`/#/mileage/mileage_trip?view=list&id=${id}`);
  const record = page.getByRole("complementary", { name: /record$/ });
  const review = record.getByRole("region", { name: "Trip review" });
  await expect(review.getByRole("img", { name: /route of the trip/ })).toBeVisible();
  await expect(review).toContainText("km");
  await expect(review).toContainText("drew");
  await expect(review).toContainText("1 min");
  await expect(review).toContainText("The track is clean: all 10 points were used");

  // Confirm and Reject stand side by side; Reject asks for the reason and sends it with the action.
  await expect(record.getByRole("button", { name: /^Confirm/ })).toBeVisible();
  await record.getByRole("button", { name: /^Reject/ }).click();
  const ask = page.getByRole("dialog").filter({ hasText: /reason/i });
  await ask.getByRole("textbox").first().fill("The trip was private");
  await ask.getByRole("button", { name: /^Reject/ }).click();
  await expect(record.getByText("Rejected", { exact: true }).first()).toBeVisible();
  const got = await api.post("/api/mileage_trip/get", { id }, await api.token("mina"));
  expect(got.json.value.state).toBe("rejected");
  expect(got.json.value.reject_reason).toBe("The trip was private");
});

test("an empty review queue says so and offers all trips (R10)", async ({
  page,
  api,
  signIn,
  uid,
}) => {
  // A trip of its own, so that "Show all trips" has a row whatever ran before (it is confirmed below).
  const own = await submittedTrip(api, uid("queue"));
  // Empty the queue: every trip in review is confirmed by the administrator (no reviewer of their own).
  const root = await api.token("root");
  const waiting = await api.post(
    "/api/mileage_trip/search",
    { filter: [{ field: "state", op: "eq", value: "in_review" }], limit: 1000 },
    root,
  );
  expect(waiting.status, JSON.stringify(waiting.json)).toBe(200);
  for (const t of waiting.json.value.rows as { id: string }[]) {
    const done = await api.post("/api/mileage_trip/confirm", { id: t.id }, root);
    expect(done.status, JSON.stringify(done.json)).toBe(200);
  }
  const confirmed = await api.post("/api/mileage_trip/get", { id: own }, root);
  expect(confirmed.json.value.state).toBe("confirmed");

  await signIn("mina", "/mileage");
  await expect(page).toHaveURL(/mileage_trip\?st=/);
  await expect(page.getByText("Nothing to review", { exact: true })).toBeVisible();
  await expect(
    page.getByText("Every trip sent to review has been confirmed or rejected."),
  ).toBeVisible();
  await page.getByRole("button", { name: "Show all trips" }).click();
  await expect(page.getByText("Nothing to review", { exact: true })).toHaveCount(0);
  await expect(page.getByRole("grid").getByRole("row", { name: /drew/ }).first()).toBeVisible();
});
