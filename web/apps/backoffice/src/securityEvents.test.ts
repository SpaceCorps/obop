import { readFileSync } from "node:fs";
import { describe, expect, it } from "vite-plus/test";
import { EVENT_KINDS } from "./securityEvents.ts";

// The kinds of the security log are listed in the description of the `kind` field that the server
// publishes in its OpenAPI document. A person reads the log to find out why a sign-in was refused,
// so the kinds of the mobile sign-in must be in words and not shown as the raw key.
const doc = JSON.parse(
  readFileSync(new URL("../../../packages/client/openapi.json", import.meta.url), "utf8"),
) as {
  components: { schemas: { security_event: { properties: { kind: { description: string } } } } };
};

describe("the kinds of the security log", () => {
  const listed = doc.components.schemas.security_event.properties.kind.description
    .replace(/^What happened:\s*/, "")
    .split(".")[0]!
    .split(",")
    .map((k) => k.trim())
    .filter(Boolean);

  it("lists the device kinds the server writes", () => {
    for (const k of ["device_registered", "device_renamed", "device_revoked", "device_limit"])
      expect(listed, k).toContain(k);
  });

  it("has words for every device kind", () => {
    const device = listed.filter((k) => k.startsWith("device_"));
    expect(device.length).toBeGreaterThanOrEqual(4);
    for (const k of device) expect(EVENT_KINDS[k], `no label for ${k}`).toBeDefined();
  });
});
