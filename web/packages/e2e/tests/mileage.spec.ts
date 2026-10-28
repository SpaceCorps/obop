import { expect, test, type Api } from "../support/fixtures.ts";

// Mileage (docs/spec/mobile.md 5 and 8.6): a driver's phone has sent a trip; the reviewer finds it in
// the back office, confirms it, and only then does it count. The trip is seeded through the API
// with the driver's own token, as the app would send it.

/** The second of ten points of a straight walk north: 0.0002 degrees (22.24 m) every five seconds. */
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

/** A trip of `drew`, sent and submitted the way the app does; returns its id. */
async function submittedTrip(api: Api, name: string, submit = true): Promise<string> {
  // A client id is a token: the unique names of the suite have spaces.
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
  if (submit) {
    const sub = await api.post("/api/mileage_trip/submit", { id }, drew);
    expect(sub.status, JSON.stringify(sub.json)).toBe(200);
  }
  return id;
}

/** What the reviewer's totals say for one driver: the summed distance of confirmed trips, or null. */
async function confirmedMetres(api: Api, driver: string): Promise<string | null> {
  const mina = await api.token("mina");
  const r = await api.post(
    "/api/mileage_trip/search",
    {
      filter: [
        { field: "state", op: "eq", value: "confirmed" },
        { field: "driver", op: "eq", value: driver },
      ],
      aggregate: {
        group_by: [{ field: "confirmed_at", bucket: "month" }],
        measures: [{ op: "sum", field: "distance_m" }],
      },
    },
    mina,
  );
  expect(r.status, JSON.stringify(r.json)).toBe(200);
  const groups = r.json.value.groups as { measures: Record<string, string> }[];
  return groups.length ? groups[0]!.measures["sum:distance_m"]! : null;
}

test.describe("mileage review", () => {
  test("a reviewer confirms a submitted trip and only then does its distance count", async ({
    page,
    api,
    signIn,
    uid,
    allowHttpErrors,
  }) => {
    // Reading a trip that is only recording is refused with a 4xx, and that is on purpose here.
    allowHttpErrors(400, 404);
    const submitted = await submittedTrip(api, uid("trip"));
    const stillRecording = await api.post(
      "/api/mileage_trip/bulk_create",
      {
        rows: [
          {
            client_trip_id: uid("rec").split(" ").join("-"),
            started_at: "2026-10-15T09:00:00Z",
            purpose: "business",
          },
        ],
      },
      await api.token("drew"),
    );
    const recordingId = stillRecording.json.value.ids[0] as string;

    const before = await confirmedMetres(api, "drew");
    expect(before).toBeNull();

    // The reviewer opens the trip from the list; a trip that is only recording is not theirs to see.
    await signIn("mina", `/mileage/mileage_trip?view=list&id=${submitted}`);
    const record = page.getByRole("complementary", { name: /record$/ });
    await expect(record).toBeVisible();
    // The distance is the server's; the status bar says where the trip stands.
    await expect(record.getByLabel("Distance m")).toHaveValue("200.15");
    await expect(record.locator("[aria-current=step]")).toContainText("In review");
    const hidden = await api.post(
      "/api/mileage_trip/get",
      { id: recordingId },
      await api.token("mina"),
    );
    expect(hidden.status).toBeGreaterThanOrEqual(400);

    // The driver cannot decide; the reviewer can.
    const byDriver = await api.post(
      "/api/mileage_trip/confirm",
      { id: submitted },
      await api.token("drew"),
    );
    expect(byDriver.status).toBeGreaterThanOrEqual(400);

    await record.getByRole("button", { name: "Confirm *", exact: true }).click();
    await expect(record.locator("[aria-current=step]")).toContainText("Confirmed");

    // The totals now hold the distance the server computed, and not before.
    await expect.poll(() => confirmedMetres(api, "drew")).toBe("200.15");
    const got = await api.post(
      "/api/mileage_trip/get",
      { id: submitted },
      await api.token("mina"),
    );
    expect(got.json.value.state).toBe("confirmed");
    expect(got.json.value.reviewed_by).toBe("mina");
    expect(got.json.value.confirmed_at).toMatch(/^20\d\d-\d\d-\d\dT/);
  });
});
