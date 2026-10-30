// Replays the thinning of the app over one recorded trip, so that the gap of `RECORDING_THINNING`
// (src/core/thin.ts) can be chosen from a drive (docs/spec/mobile.md 8.8 and 9.2). Read only.
//
// It is only meaningful for a trip that was recorded with the thinning OFF: build the test app with
// OBOP_RECORD_THINNING=off, and the server then holds every fix the phone delivered. A trip that
// the app thinned at write time has lost what the rules are to be compared on, and the script says
// so when the track looks like that.
//
//   vp run emulator:thinning -- --server http://127.0.0.1:18080 --user rita [--driver dave] \
//        [--trip ID] [--reference-m 3989.44] [--skip N] [--save-points FILE]
//   vp run emulator:thinning -- --points FILE.json [--skip N]   # an array of {recorded_at|t, lat, lon}
//
// (From web/apps/mileage-app, with OBOP_CHECK_PASSWORD set in the environment for the first form.)
// For each rule it prints the points kept and the distance of the kept track against the row "off",
// which is the distance over every fix after the first that is later than the one before it: the
// cost of the rule on this drive. The rule is the app's own `keepFix`. The distance is the haversine
// of the server (`distanceMeters` of @obop/trip-core) WITHOUT the server's two filters, a fix with
// an accuracy radius over 100 m and a step that needs more than 70 m/s
// (crates/modules/mileage/src/track.rs): the row "off" need not equal the `distance_m` of the trip.
//
// `--save-points FILE` (with --server) writes the time and position of every point it fetched to
// FILE as JSON, the form `--points FILE` reads, so that the drive can be replayed again after the
// test instance is deleted (the points exist nowhere else). It refuses to overwrite a file. The
// file holds the route that was driven, and nothing else of the trip: it is the owner's file, so
// give an absolute path outside any repository (on Windows for example
// $env:USERPROFILE\iphone-trip-points.json): a relative name is written to the folder the command
// runs in, which for `vp run` is web/apps/mileage-app inside the checkout. To replay it with other
// start phases run `--points FILE --skip N`, not this line again with --skip (it refuses to
// overwrite). The line it prints names the absolute path that was written.
//
// `--skip N` drops the first N fixes before the replay, which varies the phase of the feed. On a
// feed of one fix per second the 2 s rule keeps every second fix, and what it costs depends on
// which of the two parities it lands in; run it for N = 0, 1, 2, ... to see both. The dropped
// fixes take their part of the route with them, so with --skip the reference is not used and only
// the column "against all fixes" (against the fixes that were left) means anything.
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { connect, findTrip, tripPoints } from "./server-lib.mjs";
import {
  RULES,
  cadence,
  dropFirst,
  looksThinned,
  percent,
  replay,
  savedPoints,
  toFixes,
} from "./thinning-lib.mjs";

const args = process.argv.slice(2);
const option = (name, fallback) => {
  const at = args.indexOf(name);
  return at >= 0 ? args[at + 1] : fallback;
};
const pointsFile = option("--points", "");
const referenceOption = option("--reference-m", "");
const saveOption = option("--save-points", "");
const skipOption = option("--skip", "0");
const skip = /^\d+$/.test(skipOption) ? Number(skipOption) : Number.NaN;
if (Number.isNaN(skip)) {
  console.error(`--skip takes a whole number of fixes, got ${JSON.stringify(skipOption)}`);
  process.exit(2);
}
// The fixes that --skip drops take the distance of the start of the route with them, so the
// reference no longer fits the track: with --skip the comparison is against the fixes it was given.
const referenceM = referenceOption === "" || skip > 0 ? null : Number(referenceOption);

if (args.includes("--save-points")) {
  if (saveOption === undefined || saveOption === "" || saveOption.startsWith("--")) {
    console.error("--save-points takes the name of the file to write");
    process.exit(2);
  }
  if (pointsFile !== "") {
    console.error("--save-points saves what the server gives: use it with --server, not --points");
    process.exit(2);
  }
  if (existsSync(saveOption)) {
    console.error(
      `${resolve(saveOption)} exists already: give a name that does not exist yet (nothing was overwritten)`,
    );
    process.exit(2);
  }
}

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
      "usage: OBOP_CHECK_PASSWORD=... vp run emulator:thinning -- --server URL --user NAME [--driver NAME] [--trip ID] [--save-points FILE]\n   or: vp run emulator:thinning -- --points FILE.json",
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
    if (saveOption !== "") {
      writeFileSync(saveOption, `${JSON.stringify(savedPoints(rows), null, 1)}\n`, {
        // Never over an existing file. The check above (before the fetch) stops that case; this flag
        // only closes the gap between that check and this write, a race that no test can make.
        flag: "wx",
        // The file holds a route, so it is private to its owner. Honoured on macOS and Linux; Windows
        // does not apply it, so the test checks the mode only where it is honoured.
        mode: 0o600,
      });
      console.log(`saved ${rows.length} points of ${label} to ${resolve(saveOption)}`);
    }
  } catch (error) {
    console.error(error instanceof Error ? error.message : "error");
    process.exit(2);
  }
}

const fixes = dropFirst(toFixes(rows), skip);
if (fixes.length < 2) {
  console.error(
    `${label}: ${fixes.length} points${skip > 0 ? ` after dropping the first ${skip}` : ""}, nothing to replay`,
  );
  process.exit(2);
}
if (skip > 0) {
  console.log(
    `(the first ${skip} fixes of the trip are dropped before the replay${referenceOption === "" ? "" : "; --reference-m is not used, because the dropped fixes take their distance with them"})`,
  );
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
console.log(
  "(distance: haversine over the kept fixes, without the server's accuracy and speed filters, so the row 'off' need not equal the distance_m of the trip; 'all fixes' is the row 'off')",
);
for (const { name, thinning } of [...RULES].reverse()) {
  const r = replay(fixes, thinning);
  const against = (r.distanceM - all.distanceM) / all.distanceM;
  const reference =
    referenceM === null ? "" : `   ${percent((r.distanceM - referenceM) / referenceM)}`;
  console.log(
    `${name.padEnd(26)}${String(r.kept).padStart(8)}${r.distanceM.toFixed(2).padStart(13)}${percent(against).padStart(20)}${reference}`,
  );
}
