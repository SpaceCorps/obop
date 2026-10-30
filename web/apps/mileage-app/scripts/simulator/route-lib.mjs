// The command line that moves the iOS Simulator along the synthetic route (docs/spec/mobile.md
// 8.8). Pure, so that `vp test` checks it on any machine (scripts/simulator/route.test.ts).
import { haversine, length, waypoints } from "../emulator/route.mjs";

/** The Simulator the command is for: the booted one, or one by its UDID. */
const DEVICE =
  /^(booted|[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12})$/;

/** 100 m in 6 seconds, the pace of the Android emulator run (`emulator:drive`, 6000 ms per waypoint). */
export const DEFAULT_SPEED_MPS = 100 / 6;

/**
 * The arguments of `xcrun` that start the route: `simctl location <device> start`, with the speed
 * in metres per second, how often the Simulator issues a fix along the path (every `interval`
 * seconds, 1 by default, or every `distance` metres: not both) and the waypoints as `lat,lon` with
 * seven decimals.
 *
 * An interval of 1 second is a continuous drive (a fix every 16.7 m at the default speed). A
 * distance of 100 metres puts a fix on each waypoint, as the Android emulator run does (the
 * waypoints are 100 m apart), so the two platforms see the same positions.
 *
 * @param {{ device?: string, speed?: number, interval?: number, distance?: number }} options
 * @returns {string[]}
 */
export function startArgs(options = {}) {
  const device = options.device ?? "booted";
  const speed = options.speed ?? DEFAULT_SPEED_MPS;
  const { distance } = options;
  if (distance !== undefined && options.interval !== undefined) {
    throw new Error("give an interval or a distance for the fixes, not both");
  }
  const interval = options.interval ?? 1;
  if (!DEVICE.test(device)) {
    throw new Error(
      `device must be "booted" or the UDID of a Simulator, got ${JSON.stringify(device)}`,
    );
  }
  if (!Number.isFinite(speed) || speed < 1 || speed > 100) {
    throw new Error(`speed must be between 1 and 100 metres per second, got ${speed}`);
  }
  if (distance === undefined && (!Number.isFinite(interval) || interval < 0.5 || interval > 10)) {
    throw new Error(`interval must be between 0.5 and 10 seconds, got ${interval}`);
  }
  if (distance !== undefined && (!Number.isFinite(distance) || distance < 1 || distance > 1000)) {
    throw new Error(`distance must be between 1 and 1000 metres, got ${distance}`);
  }
  return [
    "simctl",
    "location",
    device,
    "start",
    `--speed=${speed.toFixed(3)}`,
    distance === undefined ? `--interval=${interval}` : `--distance=${distance}`,
    ...waypoints().map((p) => `${p.lat.toFixed(7)},${p.lon.toFixed(7)}`),
  ];
}

/** How long the Simulator needs for the route at a speed, in seconds, and how long it is, in metres (haversine, as the server). */
export function routeTime(speed = DEFAULT_SPEED_MPS) {
  const metres = length(waypoints(), haversine);
  return { metres, seconds: metres / speed };
}
