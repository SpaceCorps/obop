// What the read-only scripts of a test instance share (check-trip.mjs, thinning-replay.mjs): the
// sign-in with a password and the reading of one trip with its points. They only read.

/**
 * Signs in on a test instance and gives back `call(path, payload)`, which posts a JSON payload to
 * the REST API as that user and answers the `value` of the reply.
 * @param {{ server: string, user: string, password: string }} who
 */
export async function connect({ server, user, password }) {
  const login = await fetch(`${server}/api/auth/login`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ username: user, password }),
  }).then((r) => r.json());
  if (typeof login.token !== "string") throw new Error("sign-in failed");
  return async (path, payload) => {
    const response = await fetch(`${server}${path}`, {
      method: "POST",
      headers: { authorization: `Bearer ${login.token}`, "content-type": "application/json" },
      body: JSON.stringify(payload),
    });
    const body = await response.json();
    if (!response.ok)
      throw new Error(`${path}: ${response.status} ${JSON.stringify(body).slice(0, 200)}`);
    return body.value;
  };
}

/**
 * The trip with this id, or the latest trip of this driver, or the latest trip there is.
 * @param {(path: string, payload: unknown) => Promise<any>} call
 * @param {{ tripId?: string, driver?: string }} which
 */
export async function findTrip(call, { tripId = "", driver = "" }) {
  const filter = tripId
    ? [{ field: "id", op: "eq", value: tripId }]
    : driver
      ? [{ field: "driver", op: "eq", value: driver }]
      : [];
  const found = await call("/api/mileage_trip/search", {
    filter,
    order: [{ field: "started_at", dir: "desc" }],
    limit: 1,
  });
  const trip = found.rows[0];
  if (!trip)
    throw new Error("no trip found (a reviewer sees only trips that are submitted or later)");
  return trip;
}

/**
 * Every point of a trip, in order of `seq`.
 * @param {(path: string, payload: unknown) => Promise<any>} call
 * @param {string} tripId
 */
export async function tripPoints(call, tripId) {
  let points = [];
  for (let offset = 0; ; offset += 100) {
    const page = await call("/api/mileage_point/search", {
      filter: [{ field: "trip", op: "eq", value: tripId }],
      order: [{ field: "seq", dir: "asc" }],
      limit: 100,
      offset,
    });
    points = points.concat(page.rows);
    if (page.rows.length < 100) break;
  }
  return points;
}
