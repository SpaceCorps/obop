import { describe, expect, test } from "vite-plus/test";
import {
  PENDING_SIGN_IN_MS,
  isPendingSignIn,
  resumable,
  type PendingSignIn,
} from "./pending-sign-in.ts";

const REDIRECT = "org.obop.mileage:/oauth";
const NOW = 1_800_000_000_000;
const pending: PendingSignIn = {
  config: { baseUrl: "https://h", clientId: "c1" },
  verifier: "v".repeat(43),
  state: "s-1",
  startedAt: NOW - 30_000,
};

describe("a sign-in that the system ended the app in the middle of (L32)", () => {
  test("is finished from the redirect that started the app, with its state, while it is fresh", () => {
    expect(resumable(pending, `${REDIRECT}?code=x&state=s-1`, REDIRECT, NOW)).toBe(true);
    expect(resumable(pending, `${REDIRECT}?error=access_denied&state=s-1`, REDIRECT, NOW)).toBe(
      true,
    );
    // Up to the end of the instance's sign-in flow, and not after.
    const edge = { ...pending, startedAt: NOW - PENDING_SIGN_IN_MS };
    expect(resumable(edge, `${REDIRECT}?code=x&state=s-1`, REDIRECT, NOW)).toBe(true);
    expect(resumable(edge, `${REDIRECT}?code=x&state=s-1`, REDIRECT, NOW + 1)).toBe(false);
  });

  test("is not finished from anything else", () => {
    const url = `${REDIRECT}?code=x&state=s-1`;
    // Started from the launcher, or with nothing pending.
    expect(resumable(pending, null, REDIRECT, NOW)).toBe(false);
    expect(resumable(null, url, REDIRECT, NOW)).toBe(false);
    // Another state, none, or a state only in the fragment.
    expect(resumable(pending, `${REDIRECT}?code=x&state=s-2`, REDIRECT, NOW)).toBe(false);
    expect(resumable(pending, `${REDIRECT}?code=x`, REDIRECT, NOW)).toBe(false);
    expect(resumable(pending, `${REDIRECT}#state=s-1`, REDIRECT, NOW)).toBe(false);
    // Another address, or one that only begins like the redirect.
    expect(resumable(pending, `org.obop.other:/oauth?state=s-1`, REDIRECT, NOW)).toBe(false);
    expect(resumable(pending, `${REDIRECT}x?state=s-1`, REDIRECT, NOW)).toBe(false);
    expect(resumable(pending, `${REDIRECT}/more?state=s-1`, REDIRECT, NOW)).toBe(false);
    // Begun in the future (the clock went back).
    expect(resumable({ ...pending, startedAt: NOW + 1000 }, url, REDIRECT, NOW)).toBe(false);
  });

  test("a stored value is taken only when it has every part", () => {
    expect(isPendingSignIn(pending)).toBe(true);
    expect(isPendingSignIn(JSON.parse(JSON.stringify(pending)))).toBe(true);
    for (const broken of [
      null,
      "text",
      { ...pending, config: null },
      { ...pending, config: { baseUrl: "https://h" } },
      { ...pending, verifier: "" },
      { ...pending, state: 7 },
      { ...pending, startedAt: "now" },
      { ...pending, startedAt: Number.NaN },
    ]) {
      expect(isPendingSignIn(broken)).toBe(false);
    }
  });
});
