/** A WGS84 position in decimal degrees. */
export interface LatLon {
  readonly lat: number;
  readonly lon: number;
}

/** Mean radius of the Earth in metres (IUGG mean radius R1 of the WGS84 ellipsoid). */
export const EARTH_MEAN_RADIUS_M = 6_371_008.8;

const toRadians = (degrees: number): number => (degrees * Math.PI) / 180;

/**
 * Great-circle distance between two positions in metres (haversine formula on a sphere with the
 * WGS84 mean radius). It is within about 0.5 percent of the geodesic on the ellipsoid, which is
 * enough for the device preview; the server computes the figure that counts (docs/spec/mobile.md
 * 5.4). Both positions must be finite degrees: a NaN or an infinite value returns NaN, and a
 * latitude outside -90 to 90 is not checked.
 */
export function distanceMeters(a: LatLon, b: LatLon): number {
  const dLat = toRadians(b.lat - a.lat);
  const dLon = toRadians(b.lon - a.lon);
  const h =
    Math.sin(dLat / 2) ** 2 +
    Math.cos(toRadians(a.lat)) * Math.cos(toRadians(b.lat)) * Math.sin(dLon / 2) ** 2;
  // Rounding can push h one ulp above 1 for antipodal points (for example (-58, -179) and
  // (58, 1)). Its square root still rounds to 1 for every pair tried, so the clamp is only a
  // guard against NaN from asin, not something a known input needs.
  return 2 * EARTH_MEAN_RADIUS_M * Math.asin(Math.min(1, Math.sqrt(h)));
}
