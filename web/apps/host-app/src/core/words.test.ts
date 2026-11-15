import { readdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, test } from "vite-plus/test";
import { parseAddress, type AddressProblem } from "./address.ts";
import type { Build } from "./build.ts";
import type { ProbeProblem } from "./probe.ts";
import type { AddressNote, WebProblem } from "./shell.ts";
import { addressSentence, errorSentence } from "./words.ts";
import { en } from "../i18n/en.ts";
import { t, type MessageKey } from "../i18n/index.ts";

const PLAIN: Build = { cleartext: false, webviewDebug: false, defaultServer: "" };
const TEST: Build = { cleartext: true, webviewDebug: false, defaultServer: "" };

/** A sentence as the screen shows it; no placeholder may be left in it. */
function shown(sentence: { key: MessageKey; params: Record<string, string | number> } | null) {
  expect(sentence).not.toBeNull();
  if (sentence === null) return "";
  const text = t(sentence.key, sentence.params);
  expect(text, sentence.key).not.toMatch(/[{}]/);
  expect(text.length, sentence.key).toBeGreaterThan(10);
  return text;
}

describe("the catalogue", () => {
  test("fills the placeholders, and leaves one without a value as it is written", () => {
    expect(t("bar.label", { host: "erp.example.com" })).toBe("Server erp.example.com");
    expect(t("error.server_fault", { host: "h", status: 502 })).toBe(
      "h reports a fault (502). Try again in a moment.",
    );
    expect(t("bar.label")).toBe("Server {host}");
  });

  test("no sentence is empty", () => {
    for (const [key, text] of Object.entries(en)) expect(text.trim(), key).not.toBe("");
  });
});

describe("the sentence under the address field", () => {
  const PROBLEMS: AddressProblem[] = [
    "empty",
    "scheme",
    "extras",
    "non_ascii",
    "host",
    "path",
    "cleartext",
    "cleartext_public",
  ];
  const PROBES: ProbeProblem[] = ["no_answer", "not_obop", "busy", "forward_cleartext"];

  test("every refusal of the address rule has one, and each its own", () => {
    const texts = PROBLEMS.map((problem) =>
      shown(addressSentence({ kind: "address", problem }, PLAIN)),
    );
    expect(new Set(texts).size).toBe(PROBLEMS.length);
  });

  test("every refusal the rule can give is on that list", () => {
    const inputs = [
      "",
      "ftp://x",
      "https://a@b",
      "https://münchen.example",
      "https://",
      "https://h.example/A",
      "http://localhost",
      "http://shop.example.com",
    ];
    const given = new Set<string>();
    for (const input of inputs) {
      for (const cleartext of [false, true]) {
        const parsed = parseAddress(input, { cleartext });
        if (!parsed.ok) given.add(parsed.problem);
      }
    }
    expect([...given].sort()).toEqual([...PROBLEMS].sort());
  });

  test("every verdict of the probe has one, naming the host", () => {
    for (const problem of PROBES) {
      const text = shown(
        addressSentence({ kind: "probe", problem, host: "shop.example.com" }, PLAIN),
      );
      if (problem !== "busy") expect(text, problem).toContain("shop.example.com");
    }
  });

  test("a test build says that http is taken for a local server", () => {
    const plain = shown(addressSentence({ kind: "address", problem: "scheme" }, PLAIN));
    const test = shown(addressSentence({ kind: "address", problem: "scheme" }, TEST));
    expect(plain).not.toContain("http://");
    expect(test).toContain("http://");
  });

  test("a phone that could not store the address says so", () => {
    shown(addressSentence({ kind: "storage" }, PLAIN));
  });

  test("no note and an offered forward have no sentence: the forward is a banner", () => {
    expect(addressSentence(null, PLAIN)).toBeNull();
    const forwarded: AddressNote = {
      kind: "forwarded",
      host: "shop.example.com",
      address: "https://erp.example.org",
      origin: "https://erp.example.org",
    };
    expect(addressSentence(forwarded, PLAIN)).toBeNull();
    expect(t("address.forwarded.title", { origin: forwarded.origin })).toContain(
      "https://erp.example.org",
    );
    expect(t("address.forwarded.use", { origin: forwarded.origin })).not.toMatch(/[{}]/);
  });
});

describe("the sentence of the error screen", () => {
  const origin = "https://erp.example.com:8443";
  const cases: WebProblem[] = [
    { kind: "no_answer", detail: "net::ERR_CONNECTION_REFUSED" },
    { kind: "tls", detail: "SSL error: The certificate authority is not trusted" },
    { kind: "server_fault", status: 502 },
    { kind: "timeout" },
    { kind: "forwards", to: "https://evil.example.net" },
  ];

  test("every problem names the server, each in its own words", () => {
    const texts = cases.map((problem) => {
      const text = shown(errorSentence(problem, origin).main);
      expect(text, problem.kind).toContain("erp.example.com:8443");
      return text;
    });
    expect(new Set(texts).size).toBe(cases.length);
  });

  test("what the phone reported is shown when it said something", () => {
    expect(errorSentence(cases[0] as WebProblem, origin).detail).toBe(
      "net::ERR_CONNECTION_REFUSED",
    );
    expect(errorSentence({ kind: "no_answer", detail: "" }, origin).detail).toBeNull();
    expect(errorSentence({ kind: "timeout" }, origin).detail).toBeNull();
  });

  test("a certificate failure is said as one, with no way around it", () => {
    const text = shown(errorSentence(cases[1] as WebProblem, origin).main);
    expect(text).toContain("certificate");
    expect(text).toContain("does not open it");
  });

  test("the timeout says how long the app waited", () => {
    expect(shown(errorSentence({ kind: "timeout" }, origin).main)).toContain("30 seconds");
  });

  test("a start address that forwards names where to, and says what to do", () => {
    const sentence = errorSentence(cases[4] as WebProblem, origin);
    expect(shown(sentence.main)).toContain("https://evil.example.net");
    expect(shown(sentence.hint)).toContain("https://evil.example.net");
  });
});

describe("src/core", () => {
  test("is pure TypeScript: no react, react-native or expo import", () => {
    const folder = fileURLToPath(new URL(".", import.meta.url));
    const files = readdirSync(folder).filter((name) => name.endsWith(".ts"));
    expect(files.length).toBeGreaterThan(10);
    for (const file of files) {
      if (file.endsWith(".test.ts")) continue;
      const source = readFileSync(`${folder}${file}`, "utf8");
      const imports = [...source.matchAll(/\bfrom\s+"([^"]+)"|\bimport\s*\(\s*"([^"]+)"/g)].map(
        (match) => match[1] ?? match[2] ?? "",
      );
      for (const name of imports) {
        expect(name, `${file} imports ${name}`).not.toMatch(/^(react|react-native|expo)($|[-/])/);
        // Nothing of the native side or of the screens either.
        expect(name, `${file} imports ${name}`).not.toMatch(/\/(native|ui)\//);
      }
    }
  });
});
