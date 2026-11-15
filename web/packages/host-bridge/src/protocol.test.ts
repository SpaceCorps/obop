import { describe, expect, test } from "vite-plus/test";
import {
  BRIDGE_VERSION,
  COMMAND_NAMES,
  ERROR_CODES,
  MAX_MESSAGE_CHARS,
  PAGE_RECEIVER,
  REQUEST_TIMEOUT_MS,
  USER_AGENT_TOKEN,
  isCommandName,
  isErrorCode,
  isHexColour,
  isRecord,
  isRequestId,
  parseMessage,
  validParams,
  validResult,
} from "./protocol.ts";
import type { HostInfo } from "./protocol.ts";

const info: HostInfo = {
  bridge: 1,
  platform: "android",
  app: { id: "org.obop.app.test", version: "0.1.0", cleartext: false, inspectable: false },
  commands: ["host.info", "host.theme", "host.settings"],
  signIn: { passkeys: false, microsoft: false },
};

describe("the constants of version 1", () => {
  test("are what an installed app and a deployed page already speak", () => {
    expect(BRIDGE_VERSION).toBe(1);
    expect(USER_AGENT_TOKEN).toBe("ObopHost");
    expect(MAX_MESSAGE_CHARS).toBe(65_536);
    expect(REQUEST_TIMEOUT_MS).toBe(5_000);
    expect(PAGE_RECEIVER).toBe("__obopHost");
    expect(COMMAND_NAMES).toEqual(["host.info", "host.theme", "host.settings"]);
    expect(ERROR_CODES).toEqual([
      "unsupported_version",
      "unknown_command",
      "bad_params",
      "refused",
      "failed",
    ]);
  });
});

describe("the id of a request", () => {
  test("is 1 to 64 letters, digits, underscores and hyphens", () => {
    for (const good of ["p1", "A", "a_b-C9", "x".repeat(64)]) {
      expect(isRequestId(good), good).toBe(true);
    }
    for (const bad of ["", "x".repeat(65), "a b", "a.b", "é", "a\n", 1, null, undefined, {}]) {
      expect(isRequestId(bad), JSON.stringify(bad)).toBe(false);
    }
  });
});

describe("the names", () => {
  test("a command name is one of the three, and nothing that an object inherits", () => {
    for (const name of COMMAND_NAMES) expect(isCommandName(name)).toBe(true);
    for (const bad of ["host.Info", "host", "file.save", "constructor", "toString", "", 1, null]) {
      expect(isCommandName(bad), JSON.stringify(bad)).toBe(false);
    }
  });

  test("an error code is one of the five", () => {
    for (const code of ERROR_CODES) expect(isErrorCode(code)).toBe(true);
    for (const bad of ["error", "FAILED", "", 5, null]) expect(isErrorCode(bad)).toBe(false);
  });

  test("a colour is #rrggbb in either case", () => {
    for (const good of ["#000000", "#f1f2f6", "#F1F2F6", "#0a0C11"]) {
      expect(isHexColour(good), good).toBe(true);
    }
    for (const bad of ["#fff", "f1f2f6", "#f1f2f6ff", "#f1f2g6", "rgb(0,0,0)", " #f1f2f6", 0]) {
      expect(isHexColour(bad), JSON.stringify(bad)).toBe(false);
    }
  });

  test("a record is an object that is not null and not a list", () => {
    expect(isRecord({})).toBe(true);
    expect(isRecord({ a: 1 })).toBe(true);
    for (const bad of [null, [], "x", 1, undefined, true]) expect(isRecord(bad)).toBe(false);
  });
});

describe("the parameters of each command", () => {
  test("host.info and host.settings take an empty object and nothing else", () => {
    for (const command of ["host.info", "host.settings"] as const) {
      expect(validParams(command, {})).toBe(true);
      for (const bad of [{ a: 1 }, null, undefined, [], "", 0, { scheme: "light" }]) {
        expect(validParams(command, bad), JSON.stringify(bad)).toBe(false);
      }
    }
  });

  test("host.theme takes a scheme and a colour", () => {
    expect(validParams("host.theme", { scheme: "light", background: "#f1f2f6" })).toBe(true);
    expect(validParams("host.theme", { scheme: "dark", background: "#0A0C11" })).toBe(true);
  });

  test("host.theme refuses a missing, an extra and a wrong-typed field", () => {
    const bad: unknown[] = [
      {},
      { scheme: "light" },
      { background: "#f1f2f6" },
      { scheme: "light", background: "#f1f2f6", extra: 1 },
      { scheme: "system", background: "#f1f2f6" },
      { scheme: "light", background: "oklch(0.96 0.005 270)" },
      { scheme: "light", background: 0xf1f2f6 },
      { scheme: ["light"], background: "#f1f2f6" },
      null,
      [],
      "light",
    ];
    for (const params of bad) {
      expect(validParams("host.theme", params), JSON.stringify(params)).toBe(false);
    }
  });
});

describe("the result of each command", () => {
  test("host.info holds every field of version 1", () => {
    expect(validResult("host.info", info)).toBe(true);
    expect(validResult("host.info", { ...info, platform: "ios" })).toBe(true);
  });

  test("a field that a later host adds is not a reason to refuse the answer", () => {
    expect(validResult("host.info", { ...info, later: { anything: true } })).toBe(true);
    expect(validResult("host.info", { ...info, app: { ...info.app, build: 7 } })).toBe(true);
    expect(validResult("host.theme", { later: 1 })).toBe(true);
  });

  test("a command of a later host is listed by its form, so the page can see it is there", () => {
    const later = ["host.info", "file.save", "link.openExternal", "mileage.start"];
    expect(validResult("host.info", { ...info, commands: later })).toBe(true);
    for (const bad of [["host"], ["Host.info"], ["host.info "], [1], ["a.b.c"], "host.info"]) {
      expect(validResult("host.info", { ...info, commands: bad }), JSON.stringify(bad)).toBe(false);
    }
    const many = Array.from({ length: 129 }, (_, i) => `m${i}.verb`);
    expect(validResult("host.info", { ...info, commands: many })).toBe(false);
  });

  test("host.info refuses a missing or wrong-typed field", () => {
    const bad: unknown[] = [
      { ...info, bridge: 2 },
      { ...info, bridge: "1" },
      { ...info, platform: "windows" },
      { ...info, app: null },
      { ...info, app: { ...info.app, id: "" } },
      { ...info, app: { ...info.app, version: 1 } },
      { ...info, app: { ...info.app, cleartext: "false" } },
      { ...info, app: { id: "x", version: "1", cleartext: false } },
      { ...info, commands: undefined },
      { ...info, signIn: { passkeys: false } },
      { ...info, signIn: { passkeys: 0, microsoft: 0 } },
      { ...info, signIn: [] },
      null,
      [],
      "info",
    ];
    for (const result of bad) {
      expect(validResult("host.info", result), JSON.stringify(result)).toBe(false);
    }
  });

  test("host.theme and host.settings answer with an object", () => {
    for (const command of ["host.theme", "host.settings"] as const) {
      expect(validResult(command, {})).toBe(true);
      for (const bad of [null, undefined, [], "", 0, true]) {
        expect(validResult(command, bad), JSON.stringify(bad)).toBe(false);
      }
    }
  });
});

describe("one message", () => {
  test("is JSON text that is an object", () => {
    expect(parseMessage('{"obop":1}')).toEqual({ obop: 1 });
    for (const bad of ["", "x", "1", '"text"', "null", "[]", "[{}]", "{", 1, null, undefined, {}]) {
      expect(parseMessage(bad), JSON.stringify(bad)).toBeNull();
    }
  });

  test("is dropped when it is longer than the limit, and taken when it is exactly as long", () => {
    const envelope = (padding: number) => JSON.stringify({ obop: 1, pad: "x".repeat(padding) });
    const overhead = envelope(0).length;
    const fits = envelope(MAX_MESSAGE_CHARS - overhead);
    expect(fits).toHaveLength(MAX_MESSAGE_CHARS);
    expect(parseMessage(fits)).not.toBeNull();
    expect(parseMessage(envelope(MAX_MESSAGE_CHARS - overhead + 1))).toBeNull();
  });
});
