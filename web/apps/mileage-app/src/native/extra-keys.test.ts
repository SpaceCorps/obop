import { beforeEach, describe, expect, test, vi } from "vite-plus/test";

// The two keys of the `extra` of the Expo config that the app reads at run time and that no other
// test reads: `cleartextAllowed` (the sign-in address check, `native/auth.ts`) and `defaultServer`
// (the field of the sign-in screen, `native/default-server.ts`). The names are written in
// `app.config.ts` (`config/app-config.test.ts`) and in the code that reads them; a build that
// writes one name and reads another would silently behave as a build without the key. The third
// key, `thinningOff`, and the second read of `cleartextAllowed` are held by `core/thin.test.ts`.

const constants = vi.hoisted(() => ({ extra: {} as Record<string, unknown> | undefined }));

vi.mock("expo-web-browser", () => ({}));
vi.mock("expo-constants", () => ({
  default: {
    get expoConfig() {
      return constants.extra === undefined ? null : { extra: constants.extra };
    },
  },
}));
vi.mock("./platform-deps.ts", () => ({
  oauthDeps: {},
  deviceInfo: () => ({ name: "Test phone", platform: "ios", appVersion: "0.1.0" }),
  appVersion: () => "0.1.0",
}));

const HTTP = "http://192.168.1.5:18080";

beforeEach(() => {
  constants.extra = {};
});

describe("extra.cleartextAllowed", () => {
  test("a build without the key refuses an http address, one with it true accepts the address", async () => {
    const { checkAddress, addressAllowed, SignInError } = await import("./auth.ts");
    expect(() => checkAddress(HTTP)).toThrowError(SignInError);
    expect(addressAllowed(HTTP)).toBe(false);

    constants.extra = { cleartextAllowed: true };
    expect(checkAddress(HTTP)).toBe(HTTP);
    expect(addressAllowed(HTTP)).toBe(true);
  });

  test("the key has to be true, and the name has to be this one", async () => {
    const { addressAllowed } = await import("./auth.ts");
    for (const extra of [
      { cleartextAllowed: false },
      { cleartextAllowed: "true" },
      { cleartextAllowed: 1 },
      { cleartextAllowedx: true },
      { cleartext_allowed: true },
      undefined,
    ]) {
      constants.extra = extra;
      expect(addressAllowed(HTTP), JSON.stringify(extra)).toBe(false);
    }
  });

  test("an https address is accepted either way", async () => {
    const { checkAddress } = await import("./auth.ts");
    expect(checkAddress("https://phone.example")).toBe("https://phone.example");
    constants.extra = { cleartextAllowed: true };
    expect(checkAddress("https://phone.example")).toBe("https://phone.example");
  });
});

describe("extra.defaultServer", () => {
  test("is the address the sign-in field starts from, and empty where the key is missing or not text", async () => {
    const { defaultServer } = await import("./default-server.ts");
    expect(defaultServer()).toBe("");

    constants.extra = { defaultServer: "https://phone.example" };
    expect(defaultServer()).toBe("https://phone.example");

    for (const extra of [
      { defaultServerx: "https://phone.example" },
      { default_server: "https://phone.example" },
      { defaultServer: 5 },
      { defaultServer: null },
      undefined,
    ]) {
      constants.extra = extra;
      expect(defaultServer(), JSON.stringify(extra)).toBe("");
    }
  });
});
