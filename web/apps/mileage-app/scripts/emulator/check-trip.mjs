// Checks a trip on a test instance against a reference length, from the server's side: the points
// are all there once and in order, the distance the server computed, and (for a reviewer) the
// monthly total of confirmed trips. Used after the emulator runs and for the real-phone test
// (docs/spec/mobile.md 8.7 and 9.1). Read only: it signs in with a password on the test instance and
// reads (the session ends by itself after 12 hours; there is no sign-out route to call).
//
//   vp run emulator:check -- --server http://127.0.0.1:18080 --user rita \
//        [--driver dave] [--trip ID] [--reference-m 3989.44] [--tolerance 0.01]
//
// (From web/apps/mileage-app, with OBOP_CHECK_PASSWORD set in the environment. This is a test
// script, started through `vp run` like scripts/check-sdk.mjs and run by Node; it imports only Node's own modules and its sibling route.mjs,
// and it is not part of the app, its build, any image or the shipped toolchain. The mobile-toolchain
// exception of OR-34 (b) names only Metro, the Expo CLI, Gradle and Xcode and does not cover it,
// and none is claimed: the script is a `vp run` task.)
//
// The password is read from the environment variable OBOP_CHECK_PASSWORD (never from an argument,
// so that it does not land in a shell history). The exit status is 0 when everything holds and 1
// when a check failed.
import { haversine } from "./route.mjs";
import { connect, findTrip, tripPoints } from "./server-lib.mjs";

const args = process.argv.slice(2);
const option = (name, fallback) => {
  const at = args.indexOf(name);
  return at >= 0 ? args[at + 1] : fallback;
};
const server = option("--server", "http://127.0.0.1:18080");
const user = option("--user", "");
const driver = option("--driver", "");
const tripId = option("--trip", "");
const referenceM = option("--reference-m", "") === "" ? null : Number(option("--reference-m", ""));
const tolerance = Number(option("--tolerance", "0.01"));
const password = process.env.OBOP_CHECK_PASSWORD ?? "";
if (user === "" || password === "") {
  console.error(
    "usage: OBOP_CHECK_PASSWORD=... vp run emulator:check -- --server URL --user NAME [options]",
  );
  process.exit(2);
}

let failed = false;
const report = (ok, text) => {
  if (!ok) failed = true;
  console.log(`${ok ? "ok  " : "FAIL"} ${text}`);
};

let call;
try {
  call = await connect({ server, user, password });
} catch {
  console.error("sign-in failed");
  process.exit(2);
}

try {
  const trip = await findTrip(call, { tripId, driver });
  console.log(
    `trip ${trip.id} of ${trip.driver}: state ${trip.state}, started ${trip.started_at}, ended ${trip.ended_at}`,
  );

  const points = await tripPoints(call, trip.id);
  const seqs = points.map((p) => p.seq);
  const inOrder = seqs.every((s, i) => i === 0 || s > seqs[i - 1]);
  const gaps = seqs.filter((s, i) => i > 0 && s !== seqs[i - 1] + 1).length;
  const unique = new Set(seqs).size === seqs.length;
  report(points.length > 0, `${points.length} points stored`);
  report(
    inOrder && unique && gaps === 0 && seqs[0] === 1,
    `seq ${seqs[0]} to ${seqs.at(-1)}, in order, once each, no gap`,
  );
  report(
    trip.point_count === points.length && trip.max_seq === seqs.at(-1),
    `the trip says point_count ${trip.point_count}, max_seq ${trip.max_seq}`,
  );
  // The phone sends the number of points it holds and the last seq when it ends the trip, and the
  // server accepts the end only when all of them have arrived: this is the phone's own count.
  report(
    trip.expected_points === points.length && trip.last_seq === seqs.at(-1),
    `the phone said at Stop: expected_points ${trip.expected_points}, last_seq ${trip.last_seq}`,
  );
  const startedMs = Date.parse(trip.started_at);
  const early = points.filter((p) => Date.parse(p.recorded_at) < startedMs - 10_000).length;
  report(early === 0, `${early} points are older than the start by more than 10 s`);
  // Reported, not pass or fail: how dense the stored track is. These are the points the app
  // stored, so they show the cadence of the phone only for a build made with OBOP_RECORD_THINNING=off
  // (a normal build thins at write time); the gap itself is chosen from the replay of
  // scripts/emulator/thinning-replay.mjs (docs/spec/mobile.md 8.8 and 9.2).
  if (points.length > 1) {
    const times = points.map((p) => Date.parse(p.recorded_at));
    const spanS = (Math.max(...times) - Math.min(...times)) / 1000;
    let widest = 0;
    for (let i = 1; i < times.length; i += 1) widest = Math.max(widest, times[i] - times[i - 1]);
    console.log(
      `density: ${points.length} points over ${spanS.toFixed(0)} s (${((points.length - 1) / (spanS / 60)).toFixed(1)} per minute), largest gap between neighbouring points ${(widest / 1000).toFixed(0)} s`,
    );
  }

  const track = points.map((p) => ({ lat: Number(p.lat), lon: Number(p.lon) }));
  let own = 0;
  for (let i = 1; i < track.length; i += 1) own += haversine(track[i - 1], track[i]);
  const serverM = Number(trip.distance_m);
  console.log(
    `server distance ${trip.distance_m} m; the same formula over every stored point ${own.toFixed(2)} m`,
  );
  if (referenceM !== null) {
    const diff = (serverM - referenceM) / referenceM;
    report(
      Math.abs(diff) <= tolerance,
      `server distance against the reference ${referenceM} m: ${(diff * 100).toFixed(2)} % (tolerance ${(tolerance * 100).toFixed(1)} %)`,
    );
  }

  try {
    const total = await call("/api/mileage_trip/search", {
      filter: [
        { field: "state", op: "eq", value: "confirmed" },
        { field: "driver", op: "eq", value: trip.driver },
      ],
      aggregate: {
        group_by: [{ field: "confirmed_at", bucket: "month" }],
        measures: [{ op: "sum", field: "distance_m" }, { op: "count" }],
      },
    });
    console.log(`confirmed trips of ${trip.driver} by month: ${JSON.stringify(total.groups)}`);
  } catch (error) {
    console.log(`monthly total not read (${error instanceof Error ? error.message : "error"})`);
  }
} catch (error) {
  report(false, error instanceof Error ? error.message : "error");
}
process.exit(failed ? 1 : 0);
