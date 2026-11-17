import { expect, test, type Api } from "../support/fixtures.ts";

// Reject on a trip in review (docs/ISSUES.md R9): what the reviewer typed in the form is saved
// first, as the action row saves before an action, and the reason typed in the form starts the
// question, so nothing typed is lost or typed twice.

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

test("a reason typed in the form is saved and starts the Reject question (R9)", async ({
  page,
  api,
  signIn,
  uid,
}) => {
  const id = await submittedTrip(api, uid("r9"));
  await signIn("mina", `/mileage/mileage_trip?view=list&id=${id}`);
  const record = page.getByRole("complementary", { name: /record$/ });
  await record.getByRole("button", { name: /^Review/ }).click();
  const reason = record.getByRole("textbox", { name: /reason/i });
  await reason.fill("Typed in the form first");
  await record.getByRole("button", { name: /^Reject/ }).click();
  const ask = page.getByRole("dialog").filter({ hasText: /reason/i });
  await expect(ask.getByRole("textbox").first()).toHaveValue("Typed in the form first");
  // The form was saved before the question: the reason is on the trip already.
  const mina = await api.token("mina");
  const before = await api.post("/api/mileage_trip/get", { id }, mina);
  expect(before.json.value.reject_reason).toBe("Typed in the form first");
  await ask.getByRole("button", { name: /^Reject/ }).click();
  await expect(record.getByText("Rejected", { exact: true }).first()).toBeVisible();
  const got = await api.post("/api/mileage_trip/get", { id }, mina);
  expect(got.json.value.state).toBe("rejected");
  expect(got.json.value.reject_reason).toBe("Typed in the form first");
});
