import { runInNewContext } from "node:vm";
import { describe, expect, test, vi } from "vite-plus/test";
import { Refusal, createHostBridge, deliveryScript } from "./host.ts";
import type { Handlers } from "./host.ts";
import { MAX_ERROR_MESSAGE_CHARS, MAX_MESSAGE_CHARS } from "./protocol.ts";
import type { HostInfo } from "./protocol.ts";

const ORIGIN = "https://erp.example.com";
const PAGE = "https://erp.example.com/admin/#/contacts";

const info: HostInfo = {
  bridge: 1,
  platform: "android",
  app: { id: "org.obop.app.test", version: "0.1.0", cleartext: false, inspectable: false },
  commands: ["host.info", "host.theme", "host.settings"],
  signIn: { passkeys: false, microsoft: false },
};

/** A host with all three commands, each a spy unless a test brings its own. */
function host(overrides: Handlers = {}) {
  const spies = {
    "host.info": vi.fn(() => info),
    "host.theme": vi.fn(() => ({})),
    "host.settings": vi.fn(() => ({})),
  };
  const handlers: Handlers = { ...spies, ...overrides };
  return { bridge: createHostBridge({ origin: ORIGIN, handlers }), handlers: spies };
}

function request(fields: Record<string, unknown> = {}): string {
  return JSON.stringify({
    obop: 1,
    kind: "request",
    id: "p1",
    command: "host.info",
    params: {},
    ...fields,
  });
}

interface Answer {
  obop: number;
  kind: string;
  id: string;
  ok: boolean;
  result?: unknown;
  error?: { code: string; message: string };
}

async function ask(
  bridge: ReturnType<typeof createHostBridge>,
  data: string,
  sender: string | undefined = PAGE,
): Promise<Answer | null> {
  const text = await bridge.handle(data, sender);
  return text === null ? null : (JSON.parse(text) as Answer);
}

function called(handlers: Record<string, ReturnType<typeof vi.fn>>): number {
  return Object.values(handlers).reduce((sum, handler) => sum + handler.mock.calls.length, 0);
}

describe("a request that is answered", () => {
  test("host.info gives what the handler gives, under the id of the request", async () => {
    const { bridge, handlers } = host();
    expect(await ask(bridge, request({ id: "abc-1" }))).toEqual({
      obop: 1,
      kind: "response",
      id: "abc-1",
      ok: true,
      result: info,
    });
    expect(handlers["host.info"]).toHaveBeenCalledWith({});
  });

  test("host.theme hands the scheme and the colour to the handler", async () => {
    const { bridge, handlers } = host();
    const params = { scheme: "dark", background: "#0a0c11" };
    expect(await ask(bridge, request({ command: "host.theme", params }))).toMatchObject({
      ok: true,
      result: {},
    });
    expect(handlers["host.theme"]).toHaveBeenCalledWith(params);
  });

  test("host.settings is called, and a handler may answer later", async () => {
    const settings = vi.fn(async () => {
      await Promise.resolve();
      return {};
    });
    const { bridge } = host({ "host.settings": settings });
    expect(await ask(bridge, request({ command: "host.settings" }))).toMatchObject({ ok: true });
    expect(settings).toHaveBeenCalledTimes(1);
  });

  test("the sender may be the origin alone, as Android reports a frame", async () => {
    const { bridge } = host();
    expect(await ask(bridge, request(), ORIGIN)).toMatchObject({ ok: true });
    expect(await ask(bridge, request(), "HTTPS://ERP.example.com:443/x")).toMatchObject({
      ok: true,
    });
  });

  test("a field of the envelope that this version does not know is passed over", async () => {
    const { bridge } = host();
    expect(await ask(bridge, request({ trace: "t-1" }))).toMatchObject({ ok: true });
  });
});

describe("rule 1: the sender", () => {
  const strangers: (string | undefined)[] = [
    undefined,
    "",
    "null",
    "about:blank",
    "about:srcdoc",
    "data:text/html,x",
    "file:///android_asset/x.html",
    "blob:https://erp.example.com/3f0c",
    "http://erp.example.com/",
    "https://erp.example.com:8443/",
    "https://erp.example.com.evil.net/",
    "https://erp.example.com@evil.net/",
    "https://user@erp.example.com/",
    "https://evil.net/https://erp.example.com/",
    "https://example.com/",
  ];

  test.each(strangers)(
    "a message from %j gets no answer and reaches no handler",
    async (sender) => {
      const { bridge, handlers } = host();
      expect(await bridge.handle(request(), sender)).toBeNull();
      expect(called(handlers)).toBe(0);
    },
  );

  test("the sender is looked at before the message: a stranger's broken message gets nothing", async () => {
    const { bridge } = host();
    expect(await bridge.handle(request({ obop: 2 }), "https://evil.net/")).toBeNull();
    expect(await bridge.handle(request({ command: "x.y" }), "https://evil.net/")).toBeNull();
  });

  test("a host is made for an origin, and for nothing that only looks like one", () => {
    for (const bad of [
      "",
      "erp.example.com",
      "https://erp.example.com/",
      "https://ERP.example.com",
      "https://erp.example.com:443",
      "https://user@erp.example.com",
      "null",
    ]) {
      expect(() => createHostBridge({ origin: bad, handlers: {} }), bad).toThrow(/not an origin/);
    }
    expect(() =>
      createHostBridge({ origin: "http://localhost:18180", handlers: {} }),
    ).not.toThrow();
  });
});

describe("rule 2: what is not a request gets no answer", () => {
  test("too long, not JSON, not an object, no usable id, or another kind", async () => {
    const { bridge, handlers } = host();
    const silent: string[] = [
      request({ pad: "x".repeat(MAX_MESSAGE_CHARS) }),
      "",
      "host.info",
      "{",
      "null",
      "[]",
      '"request"',
      "1",
      request({ id: undefined }),
      request({ id: "" }),
      request({ id: "a b" }),
      request({ id: "x".repeat(65) }),
      request({ id: 7 }),
      request({ kind: "response" }),
      request({ kind: "event" }),
      request({ kind: undefined }),
    ];
    for (const data of silent) {
      expect(await bridge.handle(data, PAGE), data.slice(0, 60)).toBeNull();
    }
    expect(called(handlers)).toBe(0);
  });

  test("a message of exactly the longest length is still read", async () => {
    const { bridge } = host();
    const base = request({ pad: "" }).length;
    const data = request({ pad: "x".repeat(MAX_MESSAGE_CHARS - base) });
    expect(data).toHaveLength(MAX_MESSAGE_CHARS);
    expect(await ask(bridge, data)).toMatchObject({ ok: true });
  });

  test("something that is not text is dropped and nothing is thrown", async () => {
    const { bridge } = host();
    for (const data of [undefined, null, 5, {}, []]) {
      expect(await bridge.handle(data as unknown as string, PAGE)).toBeNull();
    }
  });
});

describe("rule 3: the version", () => {
  test.each([2, 0, "1", null, undefined, 1.5])("obop %j is unsupported_version", async (obop) => {
    const { bridge, handlers } = host();
    expect(await ask(bridge, request({ obop }))).toEqual({
      obop: 1,
      kind: "response",
      id: "p1",
      ok: false,
      error: { code: "unsupported_version", message: expect.any(String) },
    });
    expect(called(handlers)).toBe(0);
  });

  test("the version is looked at before the command and the parameters", async () => {
    const { bridge } = host();
    const answer = await ask(bridge, request({ obop: 2, command: "nope", params: 5 }));
    expect(answer?.error?.code).toBe("unsupported_version");
  });
});

describe("rule 4: the command", () => {
  test.each(["file.save", "host.Info", "host", "", 5, null, undefined, ["host.info"]])(
    "%j is unknown_command",
    async (command) => {
      const { bridge, handlers } = host();
      const answer = await ask(bridge, request({ command }));
      expect(answer).toMatchObject({ ok: false, error: { code: "unknown_command" } });
      expect(called(handlers)).toBe(0);
    },
  );

  test("a name that every object inherits is not a command", async () => {
    const { bridge } = host();
    for (const command of ["constructor", "toString", "hasOwnProperty", "__proto__", "valueOf"]) {
      const answer = await ask(bridge, request({ command }));
      expect(answer?.error?.code, command).toBe("unknown_command");
    }
  });

  test("the handler list is the allow-list: a command without a handler is unknown", async () => {
    const only = vi.fn(() => info);
    const bridge = createHostBridge({ origin: ORIGIN, handlers: { "host.info": only } });
    expect(await ask(bridge, request())).toMatchObject({ ok: true });
    for (const command of ["host.theme", "host.settings"]) {
      const answer = await ask(bridge, request({ command, params: {} }));
      expect(answer?.error?.code, command).toBe("unknown_command");
    }
  });

  test("a handler that the list only inherits is not on the list", async () => {
    const inherited = vi.fn(() => ({}));
    const handlers = Object.create({ "host.settings": inherited }) as Handlers;
    const bridge = createHostBridge({ origin: ORIGIN, handlers });
    const answer = await ask(bridge, request({ command: "host.settings" }));
    expect(answer?.error?.code).toBe("unknown_command");
    expect(inherited).not.toHaveBeenCalled();
  });

  test("a key of the list that is not a function is not a command, and a key that is no command of this version neither", async () => {
    const stray = vi.fn(() => ({}));
    const handlers = { "host.info": undefined, "file.save": stray } as unknown as Handlers;
    const bridge = createHostBridge({ origin: ORIGIN, handlers });
    expect((await ask(bridge, request()))?.error?.code).toBe("unknown_command");
    expect((await ask(bridge, request({ command: "file.save" })))?.error?.code).toBe(
      "unknown_command",
    );
    expect(stray).not.toHaveBeenCalled();
  });

  test("the command is looked at before the parameters", async () => {
    const { bridge } = host();
    const answer = await ask(bridge, request({ command: "nope", params: "bad" }));
    expect(answer?.error?.code).toBe("unknown_command");
  });
});

describe("rule 5: the parameters", () => {
  const cases: [string, unknown][] = [
    ["host.info", { extra: 1 }],
    ["host.info", null],
    ["host.info", []],
    ["host.info", "x"],
    ["host.settings", { now: true }],
    ["host.theme", {}],
    ["host.theme", { scheme: "dark" }],
    ["host.theme", { scheme: "dark", background: "#0a0c11", alpha: 1 }],
    ["host.theme", { scheme: "night", background: "#0a0c11" }],
    ["host.theme", { scheme: "dark", background: "black" }],
  ];

  test.each(cases)(
    "%s with %j is bad_params and the handler is not called",
    async (command, params) => {
      const { bridge, handlers } = host();
      const answer = await ask(bridge, request({ command, params }));
      expect(answer).toMatchObject({ ok: false, error: { code: "bad_params" } });
      expect(called(handlers)).toBe(0);
    },
  );

  test("missing parameters are bad_params too", async () => {
    const { bridge } = host();
    const answer = await ask(bridge, request({ params: undefined }));
    expect(answer?.error?.code).toBe("bad_params");
  });
});

describe("rule 6: what a handler throws", () => {
  test("a Refusal goes to the page in its own words, as refused or failed", async () => {
    for (const code of ["refused", "failed"] as const) {
      const { bridge } = host({
        "host.settings": () => {
          throw new Refusal(code, "Not while a server is being changed.");
        },
      });
      expect(await ask(bridge, request({ command: "host.settings" }))).toEqual({
        obop: 1,
        kind: "response",
        id: "p1",
        ok: false,
        error: { code, message: "Not while a server is being changed." },
      });
    }
  });

  test("a rejected promise counts as thrown", async () => {
    const { bridge } = host({
      "host.settings": () => Promise.reject(new Refusal("refused", "No.")),
    });
    const answer = await ask(bridge, request({ command: "host.settings" }));
    expect(answer?.error).toEqual({ code: "refused", message: "No." });
  });

  test("anything else is failed with a fixed text, and nothing of the error reaches the page", async () => {
    const secret = "token=s3cr3t at /data/user/0/org.obop.app.test/files";
    const thrown: unknown[] = [
      new Error(secret),
      new TypeError(secret),
      secret,
      { code: "refused", message: secret },
      null,
      undefined,
    ];
    for (const error of thrown) {
      const { bridge } = host({
        "host.info": () => {
          throw error;
        },
      });
      const text = await bridge.handle(request(), PAGE);
      expect(text).not.toBeNull();
      expect(text).not.toContain("s3cr3t");
      expect(JSON.parse(text ?? "")).toMatchObject({ ok: false, error: { code: "failed" } });
    }
  });

  test("a long text of a Refusal is cut", async () => {
    const { bridge } = host({
      "host.settings": () => {
        throw new Refusal("refused", "n".repeat(MAX_ERROR_MESSAGE_CHARS + 100));
      },
    });
    const answer = await ask(bridge, request({ command: "host.settings" }));
    expect(answer?.error?.message).toHaveLength(MAX_ERROR_MESSAGE_CHARS);
  });

  test("a Refusal is an Error that keeps its code", () => {
    const refusal = new Refusal("refused", "No.");
    expect(refusal).toBeInstanceOf(Error);
    expect(refusal.name).toBe("Refusal");
    expect(refusal.code).toBe("refused");
    expect(refusal.message).toBe("No.");
  });
});

describe("what a handler gives back", () => {
  test("a result that is not the command's is failed, and is not passed on", async () => {
    const wrong: unknown[] = [undefined, null, "ok", { bridge: 1 }, { ...info, bridge: 2 }];
    for (const result of wrong) {
      const { bridge } = host({ "host.info": () => result as HostInfo });
      const answer = await ask(bridge, request());
      expect(answer, JSON.stringify(result)).toMatchObject({
        ok: false,
        error: { code: "failed" },
      });
    }
  });

  test("a result that cannot be written as JSON is failed", async () => {
    const loop: Record<string, unknown> = {};
    loop.self = loop;
    const { bridge } = host({ "host.settings": () => loop as Record<string, never> });
    const answer = await ask(bridge, request({ command: "host.settings" }));
    expect(answer?.error?.code).toBe("failed");
  });

  test("an answer that would be longer than a message may be is failed", async () => {
    const big = { ...info, app: { ...info.app }, note: "x".repeat(MAX_MESSAGE_CHARS) };
    const { bridge } = host({ "host.info": () => big });
    const text = await bridge.handle(request(), PAGE);
    expect(text?.length).toBeLessThan(1_000);
    expect(JSON.parse(text ?? "")).toMatchObject({ ok: false, error: { code: "failed" } });
  });
});

describe("the script that delivers an answer", () => {
  /** Runs the script as a page would, with `window` as given, and says what `receive` got. */
  function run(script: string, window: unknown): unknown {
    // As a WebView evaluates an injected script: in a context of its own, where `window` is the
    // only name the script reads, and the value of its last statement comes back.
    return runInNewContext(script, { window });
  }

  test("calls window.__obopHost.receive with the text, unchanged", () => {
    const awkward = [
      '{"a":"plain"}',
      '{"a":"quote \\" backslash \\\\ newline \\n"}',
      "</script><script>alert(1)</script>",
      `line${String.fromCharCode(0x2028)}separator${String.fromCharCode(0x2029)}end`,
      "'); alert(1); ('",
      "${x}`",
    ];
    for (const json of awkward) {
      const receive = vi.fn();
      run(deliveryScript(json), { __obopHost: { receive } });
      expect(receive).toHaveBeenCalledTimes(1);
      expect(receive).toHaveBeenCalledWith(json);
    }
  });

  test("holds no angle bracket and no line separator of the text, and ends in true", () => {
    const script = deliveryScript(
      `</script>${String.fromCharCode(0x2028)}${String.fromCharCode(0x2029)}<!--`,
    );
    const literal = script.slice(script.indexOf("h.receive(") + 10, script.lastIndexOf(");}catch"));
    expect(literal).not.toMatch(/[<>]/);
    expect(literal).not.toContain(String.fromCharCode(0x2028));
    expect(literal).not.toContain(String.fromCharCode(0x2029));
    expect(script.endsWith("true;")).toBe(true);
    expect(run(script, {})).toBe(true);
  });

  test("does nothing where the page has not defined the receiver", () => {
    for (const window of [
      {},
      { __obopHost: null },
      { __obopHost: {} },
      { __obopHost: { receive: 1 } },
    ]) {
      expect(() => run(deliveryScript("{}"), window)).not.toThrow();
    }
  });

  test("nothing comes out of it when the page's receive throws", () => {
    const window = {
      __obopHost: {
        receive: () => {
          throw new Error("page");
        },
      },
    };
    expect(() => run(deliveryScript("{}"), window)).not.toThrow();
  });
});

describe("never throws", () => {
  test("whatever the message and the sender are", async () => {
    const { bridge } = host();
    const odd = [request(), "\u0000", "{}", request({ params: { toString: 1 } })];
    for (const data of odd) {
      for (const sender of [PAGE, "x", "https://", undefined]) {
        await expect(bridge.handle(data, sender)).resolves.not.toThrow();
      }
    }
  });
});
