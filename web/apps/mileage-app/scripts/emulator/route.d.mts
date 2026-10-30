export interface LatLon {
  lat: number;
  lon: number;
}

export const START: LatLon;
export function waypoints(): LatLon[];
export function haversine(a: LatLon, b: LatLon): number;
export function geodesic(p: LatLon, q: LatLon): number;
export function length(points: LatLon[], distance: (a: LatLon, b: LatLon) => number): number;
