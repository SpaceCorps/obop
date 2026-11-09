import { describe, expect, it } from "vitest";
import { twoFactorMessage } from "@obop/ui";
import { passkeyMessage } from "./Passkeys.tsx";
import type { Failure } from "./passkeys.ts";

const server = (status: number, code: string, message = ""): Failure => ({
  ok: false,
  kind: "server",
  status,
  code,
  message,
});

describe("the wording for a failed passkey call", () => {
  it("a branding that cannot be read has the sentence of the authenticator setup, not the server's English", () => {
    // 503 `brand_unavailable`: the registration options were refused because the instance branding
    // could not be read (docs/spec/branding.md, section 4). The server's own sentence is kept for
    // the rules it states, but this one is a repair for an administrator, and trying again does not
    // help.
    const f = server(
      503,
      "brand_unavailable",
      "the instance branding could not be read: the stored name cannot be shown",
    );
    const message = passkeyMessage(f);
    expect(message).toContain("branding");
    expect(message).toContain("administrator");
    expect(message).toContain("trying again will not help");
    expect(message).not.toContain("the stored name cannot be shown");
    // The same string as the authenticator setup shows, so there is one sentence to translate.
    expect(message).toBe(twoFactorMessage({ status: 503, code: "brand_unavailable" }));
  });

  it("the other refusals keep their wording", () => {
    // 503 for a missing public address still carries the server's sentence.
    expect(passkeyMessage(server(503, "passkeys_unavailable", "Set OBOP_PUBLIC_URL."))).toBe(
      "Set OBOP_PUBLIC_URL.",
    );
    expect(passkeyMessage(server(503, "mfa_unavailable", "no key"))).toBe("no key");
    expect(passkeyMessage(server(500, "", ""))).toBe("Something went wrong. Try again.");
    expect(passkeyMessage(server(401, "invalid_credential"))).toContain("not accepted");
    expect(passkeyMessage(server(400, "invalid_password"))).toBe("That password is not right.");
    expect(passkeyMessage({ ok: false, kind: "network", status: 0, code: "", message: "" })).toBe(
      "Could not reach the server.",
    );
  });
});
