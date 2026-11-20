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

  it("lists the Microsoft sign-in kinds and has words for every one", () => {
    const oidc = listed.filter((k) => k.startsWith("oidc_"));
    expect(oidc.sort()).toEqual([
      "oidc_bind",
      "oidc_cancelled",
      "oidc_error",
      "oidc_login",
      "oidc_provider_cleared",
      "oidc_provider_configured",
      "oidc_refused",
      "oidc_sso_only",
      "oidc_start",
      "oidc_unbind",
    ]);
    for (const k of oidc) expect(EVENT_KINDS[k], `no label for ${k}`).toBeDefined();
  });

  it("lists the kinds of the instance identity and has words for both", () => {
    for (const k of ["brand_configured", "brand_cleared"]) {
      expect(listed, k).toContain(k);
      expect(EVENT_KINDS[k], `no label for ${k}`).toBeDefined();
    }
  });

  it("lists the five kinds of the approvals inbox and has words for each", () => {
    for (const k of [
      "plan_proposed",
      "plan_approved",
      "plan_rejected",
      "plan_failed",
      "plan_expired",
    ]) {
      expect(listed, k).toContain(k);
      expect(EVENT_KINDS[k], `no label for ${k}`).toBeDefined();
    }
  });

  it("lists the kinds of Sign and has words for every one", () => {
    const sign = listed.filter((k) => k.startsWith("sign_"));
    expect(sign.sort()).toEqual([
      "sign_link_issued",
      "sign_link_restored",
      "sign_link_revoked",
      "sign_refused",
      "sign_signed",
    ]);
    for (const k of sign) expect(EVENT_KINDS[k], `no label for ${k}`).toBeDefined();
  });

  it("has words for every device kind", () => {
    const device = listed.filter((k) => k.startsWith("device_"));
    expect(device.length).toBeGreaterThanOrEqual(4);
    for (const k of device) expect(EVENT_KINDS[k], `no label for ${k}`).toBeDefined();
  });
});
