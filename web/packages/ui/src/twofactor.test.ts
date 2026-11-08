import { describe, expect, test } from "vite-plus/test";
import { twoFactorMessage } from "./twofactor.tsx";

describe("the wording for a failed two-factor call", () => {
  test("a branding that cannot be read has its own sentence, and it says that trying again will not help", () => {
    // 503 `brand_unavailable`: the instance branding could not be read, so the authenticator entry
    // (or the passkey, whose message is held by `passkeyMessage.test.ts` of the back office) cannot
    // be written into the person's device. Waiting does not repair a stored name, so the generic
    // "Try again" would send the person round in circles.
    const message = twoFactorMessage({ status: 503, code: "brand_unavailable" });
    expect(message).toContain("branding");
    expect(message).toContain("administrator");
    expect(message).toContain("trying again will not help");
    expect(message).not.toBe(twoFactorMessage({ status: 503, code: "" }));
    expect(message).not.toBe(twoFactorMessage({ status: 503, code: "mfa_unavailable" }));
  });

  test("the other codes keep their wording", () => {
    const generic = twoFactorMessage({ status: 500, code: "" });
    expect(generic).toBe("Something went wrong. Try again.");
    expect(twoFactorMessage({ status: 503, code: "mfa_unavailable" })).toContain("server key");
    expect(twoFactorMessage({ status: 0, code: "network" })).toBe("Could not reach the server.");
    expect(twoFactorMessage({ status: 429, code: "rate_limited" })).toContain("Too many attempts");
    expect(twoFactorMessage({ status: 400, code: "never_heard_of_it" })).toBe(generic);
  });
});
