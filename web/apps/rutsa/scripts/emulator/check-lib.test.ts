import { describe, expect, test } from "vite-plus/test";
import { byId, byText, center, judgeKm, parseKm, parseNodes } from "./check-lib.mjs";
import { haversine, length, waypoints } from "./route.mjs";

// A dump as `uiautomator dump` writes it (shortened): the views of React Native carry their
// testID as resource-id; the buttons of a dialog of Android carry ids of the system.
const DUMP = `<?xml version='1.0' encoding='UTF-8' standalone='yes' ?>
<hierarchy rotation="0">
  <node index="0" text="" resource-id="" class="android.widget.FrameLayout" package="org.obop.rutsa" content-desc="" bounds="[0,0][1080,2400]">
    <node index="0" text="Untitled trip 80" resource-id="current-name" class="android.widget.TextView" content-desc="" bounds="[64,300][700,360]" />
    <node index="1" text="Ended" resource-id="current-state" class="android.widget.TextView" content-desc="" bounds="[880,300][1016,360]" />
    <node index="2" text="4.0 km" resource-id="current-km" class="android.widget.TextView" content-desc="" bounds="[64,500][300,570]" />
    <node index="3" text="" resource-id="start" class="android.view.ViewGroup" content-desc="Start" bounds="[386,1900][694,2208]" />
    <node index="4" text="Tom &amp; Jerry&apos;s" resource-id="org.obop.rutsa:id/note" class="android.widget.TextView" content-desc="" bounds="[0,0][10,10]" />
    <node index="5" text="STOP" resource-id="android:id/button1" class="android.widget.Button" content-desc="" bounds="[700,1400][950,1500]" />
  </node>
</hierarchy>`;

describe("a dump of the screen", () => {
  const nodes = parseNodes(DUMP);

  test("every node with its id, text, description and bounds, in order", () => {
    expect(nodes).toHaveLength(7);
    expect(nodes[3]).toEqual({
      id: "current-km",
      text: "4.0 km",
      desc: "",
      bounds: [64, 500, 300, 570],
    });
  });

  test("a node by the testID of React Native, or by an id with its package", () => {
    expect(byId(nodes, "current-km")?.text).toBe("4.0 km");
    expect(byId(nodes, "note")?.text).toBe("Tom & Jerry's");
    expect(byId(nodes, "button1")?.text).toBe("STOP");
    expect(byId(nodes, "stop")).toBeNull();
  });

  test("a node by its text or description, whatever the case", () => {
    expect(byText(nodes, "Stop")?.id).toBe("android:id/button1");
    expect(byText(nodes, "start")?.id).toBe("start");
    expect(byText(nodes, "Pause")).toBeNull();
  });

  test("a tap goes to the middle of a node", () => {
    const start = byId(nodes, "start");
    expect(start && center(start)).toEqual({ x: 540, y: 2054 });
  });

  test("a dump that is not one has no nodes", () => {
    expect(parseNodes("ERROR: could not get idle state.")).toEqual([]);
  });
});

describe("the km on the screen", () => {
  test("are read from their text", () => {
    expect(parseKm("4.0 km")).toBe(4);
    expect(parseKm(" 12.4 km ")).toBe(12.4);
    expect(parseKm("1,234.6 km")).toBe(1234.6);
    expect(parseKm("0:00:35")).toBeNull();
  });

  const REFERENCE_M = length(waypoints(), haversine);

  test("4.0 km fits the reference route of 3,989 m", () => {
    expect(REFERENCE_M).toBeCloseTo(3989.44, 2);
    expect(judgeKm(4.0, REFERENCE_M, 0.02).ok).toBe(true);
  });

  test("within the rounding of the screen and the tolerance, not beyond", () => {
    // 2 % of 3.99 km and 0.05 km of rounding: 0.13 km either way.
    expect(judgeKm(3.9, REFERENCE_M, 0.02).ok).toBe(true);
    expect(judgeKm(4.1, REFERENCE_M, 0.02).ok).toBe(true);
    expect(judgeKm(3.8, REFERENCE_M, 0.02).ok).toBe(false);
    expect(judgeKm(0, REFERENCE_M, 0.02).ok).toBe(false);
    expect(judgeKm(4.0, REFERENCE_M, 0).ok).toBe(true);
  });
});
