import { describe, expect, it } from "vitest";
import { parseSsoReturn } from "./api.ts";

const CODE = "obopsso_" + "0123456789abcdef".repeat(4);

describe("the return from Microsoft (D91)", () => {
  it("reads the one-time code of the fragment", () => {
    expect(parseSsoReturn(`#oidc=${CODE}`)).toEqual({ code: CODE });
  });

  it("reads the four words of a failure and turns an unknown word into the general one", () => {
    for (const w of ["cancelled", "unavailable", "busy", "failed"])
      expect(parseSsoReturn(`#oidc_error=${w}`)).toEqual({ error: w });
    expect(parseSsoReturn("#oidc_error=whatever")).toEqual({ error: "failed" });
  });

  it("ignores everything else, including a router address and a code of the wrong shape", () => {
    for (const h of [
      "",
      "#",
      "#/crm/lead",
      "#oidc=",
      "#oidc=abc",
      `#oidc=${CODE}&x=1`,
      `#oidc=${CODE}/../`,
      `#/oidc=${CODE}`,
      "#oidc_error=",
      "#oidc_error=Failed",
      "#oidc_error=failed&x=1",
      `#oidc=<script>${CODE}`,
    ])
      expect(parseSsoReturn(h)).toBeNull();
  });
});
