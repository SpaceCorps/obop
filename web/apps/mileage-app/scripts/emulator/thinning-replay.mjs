// Replays the thinning of the app over one recorded trip, so that the gap of `RECORDING_THINNING`
// (src/core/thin.ts) can be chosen from a drive (docs/spec/mobile.md 8.8 and 9.2). Read only.
//
// It is only meaningful for a trip that was recorded with the thinning OFF: build the test app with
// OBOP_RECORD_THINNING=off, and the server then holds every fix the phone delivered. A trip that
// the app thinned at write time has lost what the rules are to be compared on, and the script says
// so when the track looks like that.
//
//   vp run emulator:thinning -- --server http://127.0.0.1:18080 --user rita [--driver dave] \
//        [--trip ID] [--reference-m 3989.44]
//   vp run emulator:thinning -- --points FILE.json      # an array of {recorded_at|t, lat, lon}
//
// (From web/apps/mileage-app, with OBOP_CHECK_PASSWORD set in the environment for the first form.)
// For each rule it prints the points kept and the distance of the kept track against the sum over
// every delivered fix: the cost of the rule on this drive. The distance is the server's own formula
// (`distanceMeters` of @obop/trip-core), and the rule is the app's own `keepFix`.
import { readFileSync } from "node:fs";
import { connect, findTrip, tripPoints } from "./server-lib.mjs";
import { RULES, cadence, looksThinned, replay, toFixes } from "./thinning-lib.mjs";

const args = process.argv.slice(2);
const option = (name, fallback) => {
  const at = args.indexOf(name);
  return at >= 0 ? args[at + 1] : fallback;
};
const pointsFile = option("--points", "");
const referenceOption = option("--reference-m", "");
const referenceM = referenceOption === "" ? null : Number(referenceOption);

let rows;
let label;
if (pointsFile !== "") {
  rows = JSON.parse(readFileSync(pointsFile, "utf8"));
  label = pointsFile;
} else {
  const server = option("--server", "http://127.0.0.1:18080");
  const user = option("--user", "");
  const password = process.env.OBOP_CHECK_PASSWORD ?? "";
  if (user === "" || password === "") {
    console.error(
      "usage: OBOP_CHECK_PASSWORD=... vp run emulator:thinning -- --server URL --user NAME [--driver NAME] [--trip ID]\n   or: vp run emulator:thinning -- --points FILE.json",
    );
    process.exit(2);
  }
  try {
    const call = await connect({ server, user, password });
    const trip = await findTrip(call, {
      tripId: option("--trip", ""),
      driver: option("--driver", ""),
    });
    rows = await tripPoints(call, trip.id);
    label = `trip ${trip.id} of ${trip.driver}`;
  } catch (error) {
    console.error(error instanceof Error ? error.message : "error");
    process.exit(2);
  }
}

const fixes = toFixes(rows);
if (fixes.length < 2) {
  console.error(`${label}: ${fixes.length} points, nothing to replay`);
  process.exit(2);
}
const c = cadence(fixes);
console.log(
  `${label}: ${c.count} points over ${c.spanS.toFixed(0)} s (${c.perMinute.toFixed(1)} per minute); gap between neighbours: smallest ${c.gapsS.smallest.toFixed(1)} s, median ${c.gapsS.median.toFixed(1)} s, largest ${c.gapsS.largest.toFixed(1)} s`,
);
if (looksThinned(fixes)) {
  console.log(
    "note: no two neighbouring points are closer than 2 s. This track was probably thinned when it was stored (a build without OBOP_RECORD_THINNING=off), so it cannot show what a rule drops: the replay below only thins it again.",
  );
}

const all = replay(fixes, null);
console.log("");
console.log(
  `${"rule".padEnd(26)}${"points".padStart(8)}${"distance m".padStart(13)}${"against all fixes".padStart(20)}${referenceM === null ? "" : "   against the reference"}`,
);
for (const { name, thinning } of [...RULES].reverse()) {
  const r = replay(fixes, thinning);
  const against = (r.distanceM - all.distanceM) / all.distanceM;
  const reference =
    referenceM === null
      ? ""
      : `   ${(((r.distanceM - referenceM) / referenceM) * 100).toFixed(2)} %`;
  console.log(
    `${name.padEnd(26)}${String(r.kept).padStart(8)}${r.distanceM.toFixed(2).padStart(13)}${`${(against * 100).toFixed(2)} %`.padStart(20)}${reference}`,
  );
}
