import { describe, expect, it } from "vitest";
import {
  assertionJson,
  creationJson,
  creationOptions,
  requestOptions,
  toBase64Url,
  toBuffer,
} from "./passkeys.ts";

const bytes = (b: ArrayBuffer) => [...new Uint8Array(b)];

describe("base64url", () => {
  it("round-trips every length and uses the URL alphabet without padding", () => {
    for (let n = 0; n < 40; n++) {
      const data = new Uint8Array(n).map((_, i) => (i * 37 + 11) & 255);
      const text = toBase64Url(data.buffer);
      expect(text).not.toMatch(/[+/=]/);
      expect(bytes(toBuffer(text))).toEqual([...data]);
    }
    expect(toBase64Url(new Uint8Array([0xfb, 0xff]).buffer)).toBe("-_8");
    expect(bytes(toBuffer("-_8"))).toEqual([0xfb, 0xff]);
  });
});

describe("options from the server", () => {
  it("turns the creation options' binary fields into buffers", () => {
    const o = creationOptions({
      publicKey: {
        rp: { id: "obop.test", name: "obop" },
        user: { id: "AQID", name: "ada", displayName: "ada" },
        challenge: "BAUG",
        pubKeyCredParams: [{ type: "public-key", alg: -7 }],
        attestation: "none",
        excludeCredentials: [{ type: "public-key", id: "BwgJ", transports: ["internal"] }],
      },
    }).publicKey!;
    expect(bytes(o.challenge as ArrayBuffer)).toEqual([4, 5, 6]);
    expect(bytes(o.user.id as ArrayBuffer)).toEqual([1, 2, 3]);
    expect(bytes(o.excludeCredentials![0]!.id as ArrayBuffer)).toEqual([7, 8, 9]);
    expect(o.excludeCredentials![0]!.transports).toEqual(["internal"]);
    expect(o.rp.id).toBe("obop.test");
    expect(o.attestation).toBe("none");
  });

  it("turns request options into buffers and an empty list when none is given", () => {
    const o = requestOptions({
      publicKey: { challenge: "BAUG", rpId: "obop.test", userVerification: "required" },
    }).publicKey!;
    expect(bytes(o.challenge as ArrayBuffer)).toEqual([4, 5, 6]);
    expect(o.allowCredentials).toEqual([]);
    expect(o.userVerification).toBe("required");
  });
});

describe("what goes back to the server", () => {
  const buf = (...n: number[]) => new Uint8Array(n).buffer;

  it("encodes a new credential", () => {
    const cred = {
      id: "AQID",
      rawId: buf(1, 2, 3),
      type: "public-key",
      getClientExtensionResults: () => ({ credProps: { rk: true } }),
      response: {
        clientDataJSON: buf(0xfb, 0xff),
        attestationObject: buf(9),
        getTransports: () => ["internal"],
      },
    } as unknown as PublicKeyCredential;
    expect(creationJson(cred)).toEqual({
      id: "AQID",
      rawId: "AQID",
      type: "public-key",
      clientExtensionResults: { credProps: { rk: true } },
      response: { clientDataJSON: "-_8", attestationObject: "CQ", transports: ["internal"] },
    });
  });

  it("encodes an assertion, with a null user handle when the authenticator sends none", () => {
    const cred = (handle: ArrayBuffer | null) =>
      ({
        id: "AQID",
        rawId: buf(1, 2, 3),
        type: "public-key",
        getClientExtensionResults: () => ({}),
        response: {
          clientDataJSON: buf(1),
          authenticatorData: buf(2),
          signature: buf(3),
          userHandle: handle,
        },
      }) as unknown as PublicKeyCredential;
    expect(assertionJson(cred(buf(4))).response).toEqual({
      clientDataJSON: "AQ",
      authenticatorData: "Ag",
      signature: "Aw",
      userHandle: "BA",
    });
    expect(assertionJson(cred(null)).response.userHandle).toBeNull();
  });
});
