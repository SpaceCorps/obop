// The synthetic reference route of the emulator test (docs/spec/mobile.md 8.7): a closed
// rectangle of 41 waypoints, 100 m apart, that starts and ends at one point. Nothing here is a
// real person's track. Run it to print the lengths:
//
//   node scripts/emulator/route.mjs
//
// Two lengths are printed. The haversine one is what the server computes (a sphere of the mean
// radius); the geodesic one (Vincenty's inverse formula on the WGS84 ellipsoid) is an independent
// reference, so a mistake in the shared formula would show.

export const START = { lat: 60.17, lon: 24.94 };
const LEG_M = 100;
// North 12 legs, east 8, south 12, west 8: 40 legs.
const LEGS = [
  ["n", 12],
  ["e", 8],
  ["s", 12],
  ["w", 8],
];

const A = 6378137;
const F = 1 / 298.257223563;
const B = A * (1 - F);
const MEAN_R = 6371008.8;
const rad = (d) => (d * Math.PI) / 180;

/** Metres per degree of latitude and longitude at a latitude (WGS84, series). */
function degreeLengths(lat) {
  const p = rad(lat);
  return {
    lat: 111132.92 - 559.82 * Math.cos(2 * p) + 1.175 * Math.cos(4 * p),
    lon: 111412.84 * Math.cos(p) - 93.5 * Math.cos(3 * p),
  };
}

export function waypoints() {
  const out = [{ ...START }];
  let { lat, lon } = START;
  for (const [dir, count] of LEGS) {
    for (let i = 0; i < count; i += 1) {
      const m = degreeLengths(lat);
      if (dir === "n") lat += LEG_M / m.lat;
      if (dir === "s") lat -= LEG_M / m.lat;
      if (dir === "e") lon += LEG_M / m.lon;
      if (dir === "w") lon -= LEG_M / m.lon;
      out.push({ lat, lon });
    }
  }
  return out;
}

export function haversine(a, b) {
  const dLat = rad(b.lat - a.lat);
  const dLon = rad(b.lon - a.lon);
  const h = Math.sin(dLat / 2) ** 2 + Math.cos(rad(a.lat)) * Math.cos(rad(b.lat)) * Math.sin(dLon / 2) ** 2;
  return 2 * MEAN_R * Math.asin(Math.min(1, Math.sqrt(h)));
}

/** Vincenty's inverse formula: the geodesic distance on the WGS84 ellipsoid, in metres. */
export function geodesic(p, q) {
  const L = rad(q.lon - p.lon);
  const U1 = Math.atan((1 - F) * Math.tan(rad(p.lat)));
  const U2 = Math.atan((1 - F) * Math.tan(rad(q.lat)));
  const sU1 = Math.sin(U1);
  const cU1 = Math.cos(U1);
  const sU2 = Math.sin(U2);
  const cU2 = Math.cos(U2);
  let lambda = L;
  let sigma = 0;
  let cosSqAlpha = 0;
  let cos2SigmaM = 0;
  let sinSigma = 0;
  let cosSigma = 0;
  for (let i = 0; i < 200; i += 1) {
    const sL = Math.sin(lambda);
    const cL = Math.cos(lambda);
    sinSigma = Math.hypot(cU2 * sL, cU1 * sU2 - sU1 * cU2 * cL);
    if (sinSigma === 0) return 0;
    cosSigma = sU1 * sU2 + cU1 * cU2 * cL;
    sigma = Math.atan2(sinSigma, cosSigma);
    const sinAlpha = (cU1 * cU2 * sL) / sinSigma;
    cosSqAlpha = 1 - sinAlpha * sinAlpha;
    cos2SigmaM = cosSqAlpha === 0 ? 0 : cosSigma - (2 * sU1 * sU2) / cosSqAlpha;
    const C = (F / 16) * cosSqAlpha * (4 + F * (4 - 3 * cosSqAlpha));
    const next = L + (1 - C) * F * sinAlpha * (sigma + C * sinSigma * (cos2SigmaM + C * cosSigma * (-1 + 2 * cos2SigmaM ** 2)));
    if (Math.abs(next - lambda) < 1e-12) {
      lambda = next;
      break;
    }
    lambda = next;
  }
  const uSq = (cosSqAlpha * (A * A - B * B)) / (B * B);
  const k1 = (Math.sqrt(1 + uSq) - 1) / (Math.sqrt(1 + uSq) + 1);
  const bigA = (1 + (k1 * k1) / 4) / (1 - k1);
  const bigB = k1 * (1 - (3 * k1 * k1) / 8);
  const dSigma =
    bigB *
    sinSigma *
    (cos2SigmaM +
      (bigB / 4) *
        (cosSigma * (-1 + 2 * cos2SigmaM ** 2) -
          (bigB / 6) * cos2SigmaM * (-3 + 4 * sinSigma ** 2) * (-3 + 4 * cos2SigmaM ** 2)));
  return B * bigA * (sigma - dSigma);
}

export function length(points, distance) {
  let total = 0;
  for (let i = 1; i < points.length; i += 1) total += distance(points[i - 1], points[i]);
  return total;
}

if (import.meta.url === `file:///${process.argv[1].replaceAll("\\", "/")}`) {
  const points = waypoints();
  console.log(`waypoints: ${points.length}`);
  console.log(`haversine (R = ${MEAN_R} m): ${length(points, haversine).toFixed(2)} m`);
  console.log(`geodesic (WGS84):            ${length(points, geodesic).toFixed(2)} m`);
}
