import { describe, expect, test } from "vite-plus/test";
import type { Build } from "./build.ts";
import { encodeServerRecord, makeServerRecord, type ServerRecord } from "./server-record.ts";
import {
  answering,
  initialState,
  reduce,
  requestVerdict,
  webViewMounted,
  windowVerdict,
  type ShellEvent,
  type ShellState,
} from "./shell.ts";
import { createShellStore } from "./store.ts";

const PLAIN: Build = { cleartext: false, webviewDebug: false, defaultServer: "" };
const TEST: Build = {
  cleartext: true,
  webviewDebug: true,
  defaultServer: "http://localhost:18180",
};

const ORIGIN = "https://erp.example.com";
const START = `${ORIGIN}/admin/`;
const RECORD: ServerRecord = makeServerRecord(
  { origin: ORIGIN, path: "/admin/" },
  "0.2.1",
  1_763_200_000_000,
);
const FOREIGN = "https://evil.example.net/landing";

const run = (state: ShellState, ...events: ShellEvent[]): ShellState =>
  events.reduce(reduce, state);

/** The app with a stored server, right after its start: the first WebView is mounted. */
const started = (build: Build = PLAIN): ShellState =>
  reduce(initialState(build), { type: "booted", stored: encodeServerRecord(RECORD) });

/** The same after the page has loaded once. */
const loaded = (build: Build = PLAIN): ShellState => {
  const state = started(build);
  return run(
    state,
    { type: "nav", mount: state.mount, url: START, canGoBack: false },
    { type: "load_end", mount: state.mount, url: START },
  );
};

describe("the start", () => {
  test("without a stored address the address screen opens with the build's default in the field", () => {
    expect(reduce(initialState(PLAIN), { type: "booted", stored: null })).toMatchObject({
      screen: "address",
      field: "",
      note: null,
      server: null,
    });
    expect(reduce(initialState(TEST), { type: "booted", stored: null })).toMatchObject({
      screen: "address",
      field: "http://localhost:18180",
    });
  });

  test("a stored item that is not a record is no address", () => {
    for (const stored of ["", "{", '{"v":2}', '{"v":1,"origin":"https://EVIL.example"}']) {
      expect(reduce(initialState(PLAIN), { type: "booted", stored }), stored).toMatchObject({
        screen: "address",
        server: null,
      });
    }
  });

  test("a stored address opens directly: a WebView, its watchdog and the host bar", () => {
    const state = started();
    expect(state).toMatchObject({
      screen: "web",
      server: RECORD,
      mount: 1,
      top: "instance",
      committed: false,
      loadedOnce: false,
      settingsOpen: false,
      problem: null,
    });
    expect(state.watchdog).not.toBeNull();
    expect(state.bar.shown).toBe(true);
    expect(webViewMounted(state)).toBe(true);
  });

  test("the address rule runs on the stored address: a plain build does not open an http one", () => {
    const stored = encodeServerRecord(
      makeServerRecord({ origin: "http://localhost:18180", path: "/" }, "0.2.1", 1),
    );
    expect(reduce(initialState(PLAIN), { type: "booted", stored })).toMatchObject({
      screen: "address",
      field: "http://localhost:18180",
      note: { kind: "address", problem: "cleartext" },
      server: null,
    });
    expect(reduce(initialState(TEST), { type: "booted", stored })).toMatchObject({ screen: "web" });
  });

  test("the start happens once", () => {
    const state = started();
    expect(reduce(state, { type: "booted", stored: null })).toBe(state);
  });
});

describe("the address screen", () => {
  const typing = (text: string, build: Build = PLAIN): ShellState =>
    run(
      initialState(build),
      { type: "booted", stored: null },
      { type: "field_changed", text },
      { type: "connect" },
    );

  test("a refused address stays on the screen with its reason and is never probed", () => {
    expect(typing("http://shop.example.com")).toMatchObject({
      screen: "address",
      probe: null,
      note: { kind: "address", problem: "cleartext" },
    });
    expect(typing("http://shop.example.com", TEST)).toMatchObject({
      note: { kind: "address", problem: "cleartext_public" },
    });
    expect(typing("https://münchen.example")).toMatchObject({
      note: { kind: "address", problem: "non_ascii" },
    });
  });

  test("an address of the right form is probed", () => {
    const state = typing("shop.example.com/admin");
    expect(state).toMatchObject({
      screen: "probing",
      note: null,
      probe: { address: { origin: "https://shop.example.com", path: "/admin/" } },
    });
    // Connect does nothing while a probe runs.
    expect(reduce(state, { type: "connect" })).toBe(state);
  });

  test("typing takes the note away", () => {
    const refused = typing("ftp://x");
    expect(refused.note).not.toBeNull();
    expect(reduce(refused, { type: "field_changed", text: "x" })).toMatchObject({
      field: "x",
      note: null,
    });
  });

  test("a refused probe returns to the screen with the host and the reason", () => {
    const state = typing("shop.example.com");
    const probe = state.probe?.id ?? -1;
    for (const problem of ["no_answer", "not_obop", "busy", "forward_cleartext"] as const) {
      expect(
        reduce(state, { type: "probe_done", probe, verdict: { kind: "refused", problem } }),
      ).toMatchObject({
        screen: "address",
        probe: null,
        server: null,
        note: { kind: "probe", problem, host: "shop.example.com" },
      });
    }
  });

  test("an accepted address opens the page only as `connected`, after the phone stored it", () => {
    const state = typing("erp.example.com/admin");
    const probe = state.probe?.id ?? -1;
    const accepted = { kind: "accepted", coreVersion: "0.2.1" } as const;
    expect(reduce(state, { type: "probe_done", probe, verdict: accepted })).toBe(state);
    expect(reduce(state, { type: "connected", probe, record: RECORD })).toMatchObject({
      screen: "web",
      server: RECORD,
      probe: null,
      field: "",
    });
    expect(reduce(state, { type: "store_failed", probe })).toMatchObject({
      screen: "address",
      server: null,
      note: { kind: "storage" },
    });
  });

  test("the answer of another probe than the running one changes nothing", () => {
    const state = typing("erp.example.com");
    const stale = (state.probe?.id ?? 0) + 7;
    expect(reduce(state, { type: "connected", probe: stale, record: RECORD })).toBe(state);
    expect(reduce(state, { type: "store_failed", probe: stale })).toBe(state);
    expect(
      reduce(state, {
        type: "probe_done",
        probe: stale,
        verdict: { kind: "refused", problem: "busy" },
      }),
    ).toBe(state);
  });

  test("a forward is offered, and its button only types the address and probes it again", () => {
    const state = typing("shop.example.com/admin");
    const offered = reduce(state, {
      type: "probe_done",
      probe: state.probe?.id ?? -1,
      verdict: {
        kind: "forwarded",
        address: "https://erp.example.org/admin",
        origin: "https://erp.example.org",
      },
    });
    expect(offered).toMatchObject({
      screen: "address",
      server: null,
      note: { kind: "forwarded", host: "shop.example.com", origin: "https://erp.example.org" },
    });
    const again = reduce(offered, { type: "use_forwarded" });
    expect(again).toMatchObject({
      screen: "probing",
      server: null,
      field: "https://erp.example.org/admin",
      probe: { address: { origin: "https://erp.example.org", path: "/admin/" } },
    });
    expect(again.probe?.id).not.toBe(state.probe?.id);
    // Without an offer the button is nothing.
    expect(reduce(state, { type: "use_forwarded" })).toBe(state);
  });
});

describe("the watchdog", () => {
  test("is armed by the first mount", () => {
    expect(started().watchdog).not.toBeNull();
  });

  test("is cleared by the next load end, and by a load error", () => {
    const state = started();
    expect(reduce(state, { type: "load_end", mount: state.mount, url: START }).watchdog).toBeNull();
    expect(
      reduce(state, { type: "load_error", mount: state.mount, code: -6, description: "x" })
        .watchdog,
    ).toBeNull();
  });

  test("is never armed by a navigation: a route change of the page is a load start with no end", () => {
    let state = loaded();
    expect(state.watchdog).toBeNull();
    for (const hash of ["#/crm", "#/crm/lead", "#/crm/lead/7"]) {
      state = reduce(state, {
        type: "nav",
        mount: state.mount,
        url: `${START}${hash}`,
        canGoBack: true,
      });
      expect(state.watchdog, hash).toBeNull();
      expect(state.screen).toBe("web");
    }
  });

  test("fires into the error screen only while it is the armed one", () => {
    const state = started();
    const ticket = state.watchdog ?? -1;
    expect(reduce(state, { type: "watchdog_fired", ticket })).toMatchObject({
      screen: "error",
      problem: { kind: "timeout" },
      watchdog: null,
    });
    expect(reduce(state, { type: "watchdog_fired", ticket: ticket + 1 })).toBe(state);
    const done = loaded();
    expect(reduce(done, { type: "watchdog_fired", ticket })).toBe(done);
  });

  test("is armed again by Retry and by a re-mount", () => {
    const failed = reduce(started(), {
      type: "load_error",
      mount: 1,
      code: -6,
      description: "net::ERR_CONNECTION_REFUSED",
    });
    expect(failed.watchdog).toBeNull();
    expect(reduce(failed, { type: "retry" }).watchdog).not.toBeNull();
    const gone = reduce(loaded(), { type: "process_gone", mount: 1 });
    expect(gone.watchdog).not.toBeNull();
    expect(gone.mount).toBe(2);
  });
});

describe("failures", () => {
  test("a load error shows the error screen with what the phone reported", () => {
    const state = reduce(started(), {
      type: "load_error",
      mount: 1,
      code: -6,
      description: "net::ERR_CONNECTION_REFUSED",
    });
    expect(state).toMatchObject({
      screen: "error",
      problem: { kind: "no_answer", detail: "net::ERR_CONNECTION_REFUSED" },
      loadedOnce: false,
    });
    // The WebView stays mounted under the error screen.
    expect(webViewMounted(state)).toBe(true);
  });

  test("Android's simulated load end before the error does not hide the error", () => {
    const state = run(
      started(),
      { type: "load_end", mount: 1, url: START },
      { type: "load_error", mount: 1, code: -6, description: "net::ERR_CONNECTION_REFUSED" },
    );
    expect(state).toMatchObject({ screen: "error", loadedOnce: false });
  });

  test("a TLS failure is told apart, in Android's words and by the codes of iOS", () => {
    const tls = (code: number, description: string) =>
      reduce(started(), { type: "load_error", mount: 1, code, description }).problem?.kind;
    expect(tls(3, "SSL error: The certificate authority is not trusted")).toBe("tls");
    expect(tls(-1202, "The certificate for this server is invalid.")).toBe("tls");
    expect(tls(-1200, "An SSL error has occurred")).toBe("tls");
    expect(tls(-1009, "The Internet connection appears to be offline.")).toBe("no_answer");
    expect(tls(-2, "net::ERR_NAME_NOT_RESOLVED")).toBe("no_answer");
  });

  test("a long description is cut", () => {
    const state = reduce(started(), {
      type: "load_error",
      mount: 1,
      code: -1,
      description: "x".repeat(5_000),
    });
    expect(state.problem).toMatchObject({ kind: "no_answer" });
    expect(state.problem?.kind === "no_answer" && state.problem.detail.length).toBe(200);
  });

  test("a status of 500 or more for the start address is a fault; anything else is the page's", () => {
    const state = started();
    expect(reduce(state, { type: "http_error", mount: 1, url: START, status: 502 })).toMatchObject({
      screen: "error",
      problem: { kind: "server_fault", status: 502 },
    });
    expect(reduce(state, { type: "http_error", mount: 1, url: START, status: 404 })).toBe(state);
    expect(
      reduce(state, { type: "http_error", mount: 1, url: `${ORIGIN}/api/meta`, status: 500 }),
    ).toBe(state);
  });

  test("Retry reloads the page when a document of the instance was there", () => {
    const failed = run(loaded(), {
      type: "load_error",
      mount: 1,
      code: -6,
      description: "x",
    });
    const retrying = reduce(failed, { type: "retry" });
    expect(retrying).toMatchObject({ screen: "error", retrying: true, mount: 1, reloads: 1 });
    // A second tap while it runs is nothing.
    expect(reduce(retrying, { type: "retry" })).toBe(retrying);
  });

  test("Retry mounts a new WebView when there is nothing to reload", () => {
    const failed = reduce(started(), { type: "load_error", mount: 1, code: -6, description: "x" });
    expect(reduce(failed, { type: "retry" })).toMatchObject({
      screen: "error",
      retrying: true,
      mount: 2,
      reloads: 0,
    });
  });

  test("after Retry the error screen goes only when a load end has stood for a moment", () => {
    const retrying = run(
      loaded(),
      { type: "load_error", mount: 1, code: -6, description: "x" },
      { type: "retry" },
    );
    const ended = reduce(retrying, { type: "load_end", mount: 1, url: START });
    expect(ended).toMatchObject({ screen: "error", retrying: true });
    expect(ended.settle).not.toBeNull();
    const ticket = ended.settle ?? -1;
    expect(reduce(ended, { type: "settled", ticket })).toMatchObject({
      screen: "web",
      problem: null,
      retrying: false,
      settle: null,
      loadedOnce: true,
    });
    // The server is still down: Android's load end is followed by the error, and the screen stays.
    const still = reduce(ended, { type: "load_error", mount: 1, code: -6, description: "x" });
    expect(still).toMatchObject({ screen: "error", retrying: false, settle: null });
    expect(reduce(still, { type: "settled", ticket })).toBe(still);
  });

  test("a load end without Retry does not take the error screen away", () => {
    const fault = reduce(started(), { type: "http_error", mount: 1, url: START, status: 503 });
    const ended = reduce(fault, { type: "load_end", mount: 1, url: START });
    expect(ended).toMatchObject({ screen: "error", settle: null });
  });

  test("Android: a render process that is gone gets a new WebView", () => {
    const state = reduce(loaded(), { type: "process_gone", mount: 1 });
    expect(state).toMatchObject({
      screen: "web",
      mount: 2,
      committed: false,
      loadedOnce: false,
      canGoBack: false,
      top: "instance",
    });
    expect(state.bar.shown).toBe(true);
  });

  test("iOS: a content process that ended gets a reload, with the watchdog", () => {
    const state = reduce(loaded(), { type: "content_terminated", mount: 1 });
    expect(state).toMatchObject({ screen: "web", mount: 1, reloads: 1 });
    expect(state.watchdog).not.toBeNull();
  });
});

describe("a foreign document on top", () => {
  test("after the first load: silence, a cover, the system browser and a new WebView", () => {
    const before = loaded();
    const state = reduce(before, { type: "nav", mount: 1, url: FOREIGN, canGoBack: true });
    expect(state).toMatchObject({
      screen: "web",
      top: "foreign",
      mount: 2,
      committed: false,
      loadedOnce: false,
      canGoBack: false,
      external: { url: FOREIGN },
    });
    expect(state.watchdog).not.toBeNull();
    expect(answering(state, 1)).toBe(false);
    expect(answering(state, 2)).toBe(false);
    expect(webViewMounted(state)).toBe(true);
  });

  test("`top` returns to the instance only when a load of the stored origin has ended in the new WebView", () => {
    const foreign = reduce(loaded(), { type: "nav", mount: 1, url: FOREIGN, canGoBack: true });
    // Events of the old WebView are nothing.
    expect(reduce(foreign, { type: "load_end", mount: 1, url: START })).toBe(foreign);
    // A load start of the instance in the new one is not enough.
    const committed = reduce(foreign, { type: "nav", mount: 2, url: START, canGoBack: false });
    expect(committed.top).toBe("foreign");
    expect(answering(committed, 2)).toBe(false);
    // Nor is a load end of anything else.
    expect(reduce(committed, { type: "load_end", mount: 2, url: FOREIGN }).top).toBe("foreign");
    const back = reduce(committed, { type: "load_end", mount: 2, url: START });
    expect(back).toMatchObject({ top: "instance", loadedOnce: true });
    expect(answering(back, 2)).toBe(true);
  });

  test("only an http or https address is handed to the system browser", () => {
    const state = reduce(loaded(), {
      type: "nav",
      mount: 1,
      url: "data:text/html,<script>1</script>",
      canGoBack: false,
    });
    expect(state).toMatchObject({ top: "foreign", mount: 2, external: null });
  });

  test("the suffix attack is foreign", () => {
    const state = reduce(loaded(), {
      type: "nav",
      mount: 1,
      url: "https://erp.example.com.evil.net/",
      canGoBack: false,
    });
    expect(state.top).toBe("foreign");
  });

  test("before the first load has ended: the error screen, and nothing is mounted again", () => {
    const state = reduce(started(), { type: "nav", mount: 1, url: FOREIGN, canGoBack: false });
    expect(state).toMatchObject({
      screen: "error",
      problem: { kind: "forwards", to: "https://evil.example.net" },
      top: "foreign",
      mount: 1,
      external: null,
      watchdog: null,
    });
    expect(answering(state, 1)).toBe(false);
    // The foreign document is not kept running under the error screen.
    expect(webViewMounted(state)).toBe(false);
    // Retry starts over in a new WebView; the bridge stays silent until the instance has loaded.
    const retry = reduce(state, { type: "retry" });
    expect(retry).toMatchObject({ mount: 2, top: "foreign", retrying: true, reloads: 0 });
    expect(webViewMounted(retry)).toBe(false);
  });

  test("Android's own page for a failed load and an empty document are not foreign", () => {
    const state = started();
    for (const url of ["chrome-error://chromewebdata/", "about:blank"]) {
      expect(reduce(state, { type: "nav", mount: 1, url, canGoBack: false }), url).toMatchObject({
        screen: "web",
        top: "instance",
        mount: 1,
        committed: false,
      });
    }
  });
});

describe("the question whether an address may load", () => {
  const ask = (state: ShellState, url: string, mount = state.mount) =>
    requestVerdict(state, { mount, url, isTopFrame: true });

  test("the instance loads; another origin leaves once the instance is on top", () => {
    const state = loaded();
    expect(ask(state, `${START}#/crm`)).toBe("allow");
    expect(ask(state, "https://example.com/")).toBe("external");
    expect(ask(state, "https://erp.example.com.evil.net/")).toBe("external");
    expect(ask(state, "mailto:a@example.com")).toBe("external");
    expect(ask(state, "intent://x#Intent;end")).toBe("drop");
    expect(ask(state, "javascript:alert(1)")).toBe("drop");
  });

  test("before a document of the instance is on top, another origin is where the start address forwards to", () => {
    const state = started();
    expect(ask(state, START)).toBe("allow");
    expect(ask(state, FOREIGN)).toBe("forwarded");
    expect(ask(state, "mailto:a@example.com")).toBe("drop");
    expect(reduce(state, { type: "forwarded", mount: 1, url: FOREIGN })).toMatchObject({
      screen: "error",
      problem: { kind: "forwards", to: "https://evil.example.net" },
      top: "instance",
      watchdog: null,
    });
    // Once the instance is on top the event is nothing.
    const done = loaded();
    expect(reduce(done, { type: "forwarded", mount: 1, url: FOREIGN })).toBe(done);
  });

  test("a WebView that is no longer the mounted one gets no for an answer", () => {
    const state = reduce(loaded(), { type: "process_gone", mount: 1 });
    expect(ask(state, START, 1)).toBe("drop");
    expect(ask(initialState(PLAIN), START, 0)).toBe("drop");
  });

  test("iOS: a frame loads what it likes", () => {
    expect(
      requestVerdict(loaded(), { mount: 1, url: "https://example.com/", isTopFrame: false }),
    ).toBe("allow");
  });
});

describe("a new window", () => {
  test("goes to the one WebView or to the system, and only for the instance's document", () => {
    const state = loaded();
    expect(windowVerdict(state, { mount: 1, url: `${START}#/crm/lead/7` })).toBe("assign");
    expect(windowVerdict(state, { mount: 1, url: "https://example.com/" })).toBe("external");
    expect(windowVerdict(state, { mount: 1, url: "blob:https://erp.example.com/1" })).toBe("drop");
    expect(windowVerdict(state, { mount: 2, url: "https://example.com/" })).toBe("drop");
    const foreign = reduce(started(), { type: "nav", mount: 1, url: FOREIGN, canGoBack: false });
    expect(windowVerdict(foreign, { mount: 1, url: "https://example.com/" })).toBe("drop");
  });
});

describe("the host bar", () => {
  /** Everything a page can make the WebView report, and what its bridge commands turn into. */
  const pageEvents = (mount: number): ShellEvent[] => [
    { type: "nav", mount, url: `${START}#/crm`, canGoBack: true },
    { type: "nav", mount, url: "about:blank", canGoBack: false },
    { type: "load_end", mount, url: START },
    { type: "load_end", mount, url: FOREIGN },
    { type: "load_error", mount, code: -6, description: "x" },
    { type: "http_error", mount, url: START, status: 500 },
    { type: "http_error", mount, url: START, status: 404 },
    { type: "forwarded", mount, url: FOREIGN },
    { type: "nav", mount, url: FOREIGN, canGoBack: true },
    { type: "process_gone", mount },
    { type: "content_terminated", mount },
    { type: "theme", theme: { scheme: "dark", background: "#000000" } },
    { type: "open_settings" },
  ];

  test("is shown at every mount, at every return to the app and after Change server", () => {
    expect(started().bar.shown).toBe(true);
    const hidden = run(
      loaded(),
      { type: "bar_min_elapsed", ticket: loaded().bar.timer },
      {
        type: "page_touched",
      },
    );
    expect(hidden.bar.shown).toBe(false);
    expect(reduce(hidden, { type: "foreground" }).bar.shown).toBe(true);
    expect(reduce(hidden, { type: "process_gone", mount: 1 }).bar.shown).toBe(true);
    const changed = reduce(hidden, { type: "change_server" });
    expect(changed.bar.shown).toBe(false);
    const probing = run(
      changed,
      { type: "field_changed", text: "erp.example.com" },
      {
        type: "connect",
      },
    );
    expect(
      reduce(probing, { type: "connected", probe: probing.probe?.id ?? -1, record: RECORD }).bar
        .shown,
    ).toBe(true);
  });

  test("stays at least two seconds and then until the person next touches the page", () => {
    const shown = loaded();
    const ticket = shown.bar.timer;
    // A touch in the first two seconds: the bar goes when they are over.
    const early = reduce(shown, { type: "page_touched" });
    expect(early.bar.shown).toBe(true);
    expect(reduce(early, { type: "bar_min_elapsed", ticket }).bar.shown).toBe(false);
    // No touch: it stays, and goes with the next one.
    const waited = reduce(shown, { type: "bar_min_elapsed", ticket });
    expect(waited.bar.shown).toBe(true);
    expect(reduce(waited, { type: "page_touched" }).bar.shown).toBe(false);
    // The timer of an earlier showing is nothing.
    const again = reduce(early, { type: "foreground" });
    expect(reduce(again, { type: "bar_min_elapsed", ticket })).toBe(again);
    expect(again.bar.touched).toBe(false);
  });

  test("no event of a page hides it", () => {
    for (const first of [started(), loaded()]) {
      for (const event of pageEvents(first.mount)) {
        const next = reduce(first, event);
        expect(next.bar.shown, event.type).toBe(true);
        // Nor does a page event stand in for the two seconds or for the touch.
        expect(next.bar.minElapsed, event.type).toBe(false);
        expect(next.bar.touched, event.type).toBe(false);
      }
    }
  });

  test("no sequence of page events removes the way to This app", () => {
    let state = loaded();
    for (let round = 0; round < 3; round += 1) {
      for (const event of pageEvents(state.mount)) {
        if (event.type === "open_settings") continue;
        state = reduce(state, event);
        expect(state.screen === "web" || state.screen === "error", event.type).toBe(true);
        expect(reduce(state, { type: "open_settings" }).settingsOpen, event.type).toBe(true);
      }
    }
  });

  test("no event of a page closes This app", () => {
    const open = reduce(loaded(), { type: "open_settings" });
    for (const event of pageEvents(open.mount)) {
      expect(reduce(open, event).settingsOpen, event.type).toBe(true);
    }
    expect(reduce(open, { type: "close_settings" }).settingsOpen).toBe(false);
  });
});

describe("the bridge", () => {
  test("is answered only while the document on top is the instance's, in the mounted WebView", () => {
    expect(answering(initialState(PLAIN), 0)).toBe(false);
    expect(answering(started(), 1)).toBe(true);
    expect(answering(loaded(), 1)).toBe(true);
    expect(answering(loaded(), 2)).toBe(false);
    const address = reduce(loaded(), { type: "change_server" });
    expect(answering(address, 1)).toBe(false);
  });

  test("the page's theme is kept for the ground, and forgotten with the WebView", () => {
    const theme = { scheme: "dark", background: "#0a0c11" } as const;
    const themed = reduce(loaded(), { type: "theme", theme });
    expect(themed.theme).toEqual(theme);
    expect(reduce(themed, { type: "process_gone", mount: 1 }).theme).toBeNull();
    // A foreign document on top reports nothing.
    const foreign = reduce(started(), { type: "nav", mount: 1, url: FOREIGN, canGoBack: false });
    expect(reduce(foreign, { type: "theme", theme })).toBe(foreign);
    expect(reduce(initialState(PLAIN), { type: "theme", theme }).theme).toBeNull();
  });
});

describe("history and Change server", () => {
  test("the WebView's navigation state says whether Back has somewhere to go", () => {
    const state = loaded();
    expect(state.canGoBack).toBe(false);
    const deeper = reduce(state, { type: "nav", mount: 1, url: `${START}#/crm`, canGoBack: true });
    expect(deeper).toMatchObject({ canGoBack: true, committed: true });
  });

  test("Change server forgets the server and opens the address screen", () => {
    const state = run(loaded(TEST), { type: "open_settings" }, { type: "change_server" });
    expect(state).toMatchObject({
      screen: "address",
      server: null,
      field: "http://localhost:18180",
      note: null,
      settingsOpen: false,
      watchdog: null,
      theme: null,
    });
    expect(webViewMounted(state)).toBe(false);
    // The next WebView has a new key.
    const probing = reduce(state, { type: "connect" });
    const next = reduce(probing, {
      type: "connected",
      probe: probing.probe?.id ?? -1,
      record: RECORD,
    });
    expect(next.mount).toBe(2);
  });

  test("events of a WebView mean nothing on the address screen", () => {
    const address = reduce(loaded(), { type: "change_server" });
    for (const event of [
      { type: "nav", mount: 1, url: START, canGoBack: true },
      { type: "load_end", mount: 1, url: START },
      { type: "load_error", mount: 1, code: -6, description: "x" },
      { type: "open_settings" },
      { type: "foreground" },
      { type: "retry" },
      { type: "change_server" },
    ] satisfies ShellEvent[]) {
      expect(reduce(address, event), event.type).toBe(address);
    }
  });
});

describe("the store", () => {
  test("applies events in order and tells its listeners only when the state changed", () => {
    const store = createShellStore(PLAIN);
    let calls = 0;
    const stop = store.subscribe(() => {
      calls += 1;
    });
    store.dispatch({ type: "booted", stored: encodeServerRecord(RECORD) });
    expect(store.get().screen).toBe("web");
    expect(calls).toBe(1);
    store.dispatch({ type: "booted", stored: null });
    expect(calls).toBe(1);
    stop();
    store.dispatch({ type: "open_settings" });
    expect(store.get().settingsOpen).toBe(true);
    expect(calls).toBe(1);
  });
});
