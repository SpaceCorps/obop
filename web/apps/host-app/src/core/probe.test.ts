import { describe, expect, test } from "vite-plus/test";
import {
  PROBE_MAX_BODY_CHARS,
  classifyProbe,
  probeUrl,
  probeVerdict,
  type ProbeAnswer,
} from "./probe.ts";

const requested = { origin: "https://shop.example.com", path: "/admin/" };

/** What an obop instance answers to every caller, signed in or not. */
const document = (changes: Record<string, unknown> = {}): string =>
  JSON.stringify({
    openapi: "3.1.0",
    info: { title: "obop API", version: "0.2.1" },
    "x-mcp": { authorization: "bearer" },
    paths: { "/api/auth/login": { post: {} }, "/api/meta": { get: {} } },
    ...changes,
  });

const answer = (changes: Partial<ProbeAnswer> = {}): ProbeAnswer => ({
  status: 200,
  contentType: "application/json",
  body: document(),
  final: "https://shop.example.com/api/openapi.json",
  ...changes,
});

const classify = (changes: Partial<ProbeAnswer> = {}, cleartext = false) =>
  classifyProbe({ ...answer(changes), requested, cleartext });

describe("the probe", () => {
  test("asks for the API description at the root of the origin, whatever the path", () => {
    expect(probeUrl(requested.origin)).toBe("https://shop.example.com/api/openapi.json");
  });

  test("accepts the document of an obop instance and keeps its core version", () => {
    expect(classify()).toEqual({ kind: "accepted", coreVersion: "0.2.1" });
    expect(classify({ contentType: "application/json; charset=utf-8" })).toMatchObject({
      kind: "accepted",
    });
    // A system that does not say where the answer came from says nothing about a forward.
    expect(classify({ final: "" })).toMatchObject({ kind: "accepted" });
  });

  test("no answer at all is its own verdict", () => {
    expect(probeVerdict({ answered: false }, requested, false)).toEqual({
      kind: "refused",
      problem: "no_answer",
    });
    expect(probeVerdict({ answered: true, ...answer() }, requested, false)).toMatchObject({
      kind: "accepted",
    });
  });

  test("a 503 is a busy server", () => {
    expect(classify({ status: 503, body: "" })).toEqual({ kind: "refused", problem: "busy" });
  });

  test("anything that is not the document is not an obop server", () => {
    const notObop = { kind: "refused", problem: "not_obop" };
    // What any service behind a container probe answers, and what any API answers without a token.
    expect(classify({ body: '{"status":"ok"}' })).toEqual(notObop);
    expect(classify({ status: 401, body: "" })).toEqual(notObop);
    expect(classify({ status: 404 })).toEqual(notObop);
    expect(classify({ status: 500 })).toEqual(notObop);
    // The right text under another content type, and a page of a website.
    expect(classify({ contentType: "text/html" })).toEqual(notObop);
    expect(
      classify({ contentType: "text/html", body: "<!doctype html><title>Shop</title>" }),
    ).toEqual(notObop);
    expect(classify({ body: "not json" })).toEqual(notObop);
    expect(classify({ body: "[1,2]" })).toEqual(notObop);
    expect(classify({ body: "null" })).toEqual(notObop);
  });

  test("each of the four marks of the document is needed", () => {
    const notObop = { kind: "refused", problem: "not_obop" };
    expect(classify({ body: document({ openapi: 3 }) })).toEqual(notObop);
    expect(classify({ body: document({ info: { title: "x" } }) })).toEqual(notObop);
    expect(classify({ body: document({ info: { version: 2 } }) })).toEqual(notObop);
    expect(classify({ body: document({ info: { version: "" } }) })).toEqual(notObop);
    expect(classify({ body: document({ info: { version: "1".repeat(65) } }) })).toEqual(notObop);
    expect(classify({ body: document({ "x-mcp": "yes" }) })).toEqual(notObop);
    expect(classify({ body: document({ "x-mcp": undefined }) })).toEqual(notObop);
    expect(classify({ body: document({ paths: { "/api/meta": {} } }) })).toEqual(notObop);
    expect(classify({ body: document({ paths: [] }) })).toEqual(notObop);
    expect(classify({ body: document({ paths: {} }) })).toEqual(notObop);
  });

  test("an answer too long to be the document is not read", () => {
    const body = document({ padding: "x".repeat(PROBE_MAX_BODY_CHARS) });
    expect(classify({ body })).toEqual({ kind: "refused", problem: "not_obop" });
  });
});

describe("a forwarded address is treated as newly typed", () => {
  test("https to https on another host is offered, with the path that was typed, and not accepted", () => {
    expect(classify({ final: "https://erp.example.org/api/openapi.json" })).toEqual({
      kind: "forwarded",
      address: "https://erp.example.org/admin",
      origin: "https://erp.example.org",
    });
    // The same host on another port is another origin.
    expect(classify({ final: "https://shop.example.com:8443/api/openapi.json" })).toMatchObject({
      kind: "forwarded",
      origin: "https://shop.example.com:8443",
    });
  });

  test("the offer is made whatever the forwarded answer holds: only its own probe can accept it", () => {
    expect(
      classify({ final: "https://erp.example.org/login", status: 200, body: document() }),
    ).toMatchObject({ kind: "forwarded" });
    expect(classify({ final: "https://erp.example.org/", status: 503, body: "" })).toMatchObject({
      kind: "forwarded",
    });
  });

  test("https to a local http address is refused in a plain build", () => {
    expect(classify({ final: "http://192.168.1.20/api/openapi.json" }, false)).toEqual({
      kind: "refused",
      problem: "forward_cleartext",
    });
  });

  test("a change from https to http is never followed, in a test build either", () => {
    for (const final of ["http://192.168.1.20/api/openapi.json", "http://shop.example.com/x"]) {
      expect(classify({ final }, true), final).toEqual({
        kind: "refused",
        problem: "forward_cleartext",
      });
    }
  });

  test("a local http address to a public http address is refused in a test build", () => {
    const local = { origin: "http://192.168.1.20:18180", path: "/" };
    expect(
      classifyProbe({
        ...answer({ final: "http://shop.example.com/api/openapi.json" }),
        requested: local,
        cleartext: true,
      }),
    ).toEqual({ kind: "refused", problem: "forward_cleartext" });
    // To another local address, or to https, it is offered.
    expect(
      classifyProbe({
        ...answer({ final: "http://192.168.1.21:18180/api/openapi.json" }),
        requested: local,
        cleartext: true,
      }),
    ).toEqual({
      kind: "forwarded",
      address: "http://192.168.1.21:18180",
      origin: "http://192.168.1.21:18180",
    });
    expect(
      classifyProbe({
        ...answer({ final: "https://shop.example.com/api/openapi.json" }),
        requested: local,
        cleartext: true,
      }),
    ).toMatchObject({ kind: "forwarded", address: "https://shop.example.com" });
  });

  test("a final address that is no http address at all is not an obop server", () => {
    for (const final of ["data:text/html,x", "https://user@evil.example/", "about:blank"]) {
      expect(classify({ final }), final).toEqual({ kind: "refused", problem: "not_obop" });
    }
  });

  test("another address on the same origin is no forward: the answer itself decides", () => {
    expect(
      classify({
        final: "https://shop.example.com/login",
        contentType: "text/html",
        body: "<html>",
      }),
    ).toEqual({ kind: "refused", problem: "not_obop" });
    expect(classify({ final: "https://SHOP.example.com:443/api/openapi.json" })).toMatchObject({
      kind: "accepted",
    });
  });
});
