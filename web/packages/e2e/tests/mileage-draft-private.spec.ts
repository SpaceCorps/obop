import { expect, test, type Api } from "../support/fixtures.ts";

// Submit on an ended trip while the form holds an unsaved purpose "private" (docs/ISSUES.md U80):
// a private trip is never reviewed, so the button goes as soon as the form says private, before
// the save, and comes back when the purpose is changed back.

function walk(trip: string, seq: number) {
  const s = seq * 5;
  return {
    trip,
    seq,
    recorded_at: `2026-10-16T09:${String(Math.floor(s / 60)).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}Z`,
    lat: (52 + 0.0002 * seq).toFixed(7),
    lon: (13 + 0.0001 * (seq % 3)).toFixed(7),
    accuracy_m: "5",
  };
}

async function endedTrip(api: Api, name: string): Promise<string> {
  const client = name.split(" ").join("-");
  const drew = await api.token("drew");
  const made = await api.post(
    "/api/mileage_trip/bulk_create",
    {
      idempotency_key: `trip:${client}`,
      rows: [
        {
          client_trip_id: client,
          started_at: "2026-10-16T09:00:00Z",
          purpose: "business",
          vehicle: "HH-W 3",
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
  return id;
}

test("Submit goes while the form marks the trip private, before the save (U80)", async ({
  page,
  api,
  signIn,
  uid,
}) => {
  const id = await endedTrip(api, uid("u80"));
  await signIn("drew", `/mileage/mileage_trip?view=list&id=${id}`);
  const record = page.getByRole("complementary", { name: /record$/ });
  const submit = record.getByRole("button", { name: /^Submit/ });
  await expect(submit).toBeVisible();
  const purpose = record.getByRole("combobox", { name: /^Purpose/ });
  await purpose.selectOption("private");
  await expect(submit).toHaveCount(0);
  // Nothing was saved: the trip is still business on the server, with its route.
  const drew = await api.token("drew");
  const got = await api.post("/api/mileage_trip/get", { id }, drew);
  expect(got.json.value.purpose).toBe("business");
  expect(got.json.value.point_count).toBe(10);
  await purpose.selectOption("business");
  await expect(submit).toBeVisible();
});
