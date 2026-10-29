import { describe, expect, test } from "vite-plus/test";
import { START, haversine, length } from "../emulator/route.mjs";
import { DEFAULT_SPEED_MPS, routeTime, startArgs } from "./route-lib.mjs";

const parse = (args: string[]) =>
  args
    .filter((a) => /^-?\d+\.\d{7},-?\d+\.\d{7}$/.test(a))
    .map((a) => {
      const [lat, lon] = a.split(",").map(Number);
      return { lat: lat!, lon: lon! };
    });

describe("the command that moves the Simulator", () => {
  test("is simctl location start on the booted Simulator, with a speed, an interval and the waypoints", () => {
    const args = startArgs();
    expect(args.slice(0, 4)).toEqual(["simctl", "location", "booted", "start"]);
    expect(args).toContain("--speed=16.667");
    expect(args).toContain("--interval=1");
    expect(DEFAULT_SPEED_MPS).toBeCloseTo(100 / 6, 10);
    expect(parse(args)).toHaveLength(41);
  });

  test("the route is closed (within a metre), starts at the reference start and has the reference length", () => {
    const points = parse(startArgs());
    expect(points[0]).toEqual({ lat: START.lat, lon: START.lon });
    // The westward legs are measured at a lower latitude than the eastward ones, so the end is 0.3 m off the start.
    expect(haversine(points.at(-1)!, points[0]!)).toBeLessThan(1);
    // Seven decimals are 1 cm: the printed route is as long as the reference (3989.44 m).
    expect(length(points, haversine)).toBeCloseTo(3989.44, 1);
  });

  test("takes a Simulator by its UDID, a speed and an interval", () => {
    const udid = "A25ADB7A-D644-4FEC-88D8-2784C2AA2EAE";
    const args = startArgs({ device: udid, speed: 20, interval: 2 });
    expect(args[2]).toBe(udid);
    expect(args).toContain("--speed=20.000");
    expect(args).toContain("--interval=2");
  });

  test("refuses what is not a Simulator or a sensible pace", () => {
    expect(() => startArgs({ device: "booted; rm -rf /" })).toThrow(/device/);
    expect(() => startArgs({ device: "iPhone 17" })).toThrow(/device/);
    expect(() => startArgs({ device: "" })).toThrow(/device/);
    expect(() => startArgs({ speed: 0 })).toThrow(/speed/);
    expect(() => startArgs({ speed: 101 })).toThrow(/speed/);
    expect(() => startArgs({ speed: Number.NaN })).toThrow(/speed/);
    expect(() => startArgs({ interval: 0.1 })).toThrow(/interval/);
    expect(() => startArgs({ interval: 11 })).toThrow(/interval/);
  });

  test("the time of the route is its length over the speed", () => {
    const { metres, seconds } = routeTime();
    expect(metres).toBeCloseTo(3989.44, 1);
    expect(seconds).toBeCloseTo(239.37, 1);
    expect(routeTime(20).seconds).toBeCloseTo(metres / 20, 6);
  });
});
