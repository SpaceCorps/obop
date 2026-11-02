import { describe, expect, test } from "vite-plus/test";
import { PURPOSES, SERVER_STATES } from "../core/types.ts";
import { appSources, screenSources } from "../test-support/app-sources.ts";
import { en, PLURAL_CATEGORIES } from "./en.ts";
import { pluralBases } from "./check.ts";
import { bypassesOf, keyFamilies, mentions } from "./usage.ts";

const messages = en as Record<string, string>;

describe("the screens say nothing that is not in the catalogue", () => {
  test("no screen, and not App.tsx, has a sentence of its own", () => {
    const sources = screenSources();
    expect(sources.map((s) => s.path)).toContain("App.tsx");
    expect(sources.length).toBeGreaterThanOrEqual(9);
    for (const source of sources) expect(bypassesOf(source.text), source.path).toEqual([]);
  });

  test("the text of the notification of the recording comes from the catalogue", () => {
    const recorder = appSources().find((s) => s.path === "src/native/recorder.ts")!;
    expect(bypassesOf(recorder.text)).toEqual([]);
    expect(recorder.text).toContain('notificationTitle: t("recording.notification.title")');
    expect(recorder.text).toContain('notificationBody: t("recording.notification.body")');
  });

  test("the language the server is told is the language of the app, in every client the app builds", () => {
    const native = appSources().filter((s) => /^src\/native\/.*\.ts$/.test(s.path));
    const clients = native.filter((s) => s.text.includes("createObopClient("));
    expect(clients.map((s) => s.path).sort()).toEqual([
      "src/native/auth.ts",
      "src/native/runtime.ts",
    ]);
    for (const source of clients) {
      // The function itself, so that it is asked at every call; not "en", not a copy taken once.
      expect(source.text, source.path).toMatch(/\bgetLanguage,/);
      expect(source.text, source.path).not.toMatch(/getLanguage:\s*\(\)\s*=>/);
    }
    const auth = native.find((s) => s.path === "src/native/auth.ts")!;
    expect(auth.text).toContain("lang: getLanguage()");
  });
});

describe("the scanner that guards the screens can fail", () => {
  const clean = `
    <Screen title={t("home.title")}>
      <Button label={t("home.start")} variant="primary" testID="start" tone="muted" />
      <Text>{t("home.none")}</Text>
      <Row value={formatDistance(d) ?? undefined} subtitle={a.join("\\n")} />
      {x > 3 ? <A/> : <B/>}
      Alert.alert(t("x"), t("y"), [{ text: t("common.cancel"), style: "cancel" }]);
      const f = (a: number) => a > 1 && b < 2;
      <Text>· / 12</Text>
    </Screen>`;

  test("it passes a screen that reads everything from the catalogue", () => {
    expect(bypassesOf(clean)).toEqual([]);
  });

  test("it reports each way of typing a sentence into a screen", () => {
    const cases: [string, string][] = [
      ["jsx text", "<Text>No trips yet.</Text>"],
      ["jsx text", '<Text tone="muted">\n  Waiting for the first fix\n</Text>'],
      ["prop", '<Button label="Start" />'],
      ["prop", '<Banner title={"Problem"} />'],
      ["prop", "<Banner message={'Stäng'} />"],
      ["prop", "<Banner message={`Saved ${count} trips`} />"],
      ["prop", '<Field placeholder="Name" />'],
      ["prop", '<Button accessibilityLabel="Stop recording" />'],
      ["object", '[{ text: "Cancel", style: "cancel" }]'],
      ["object", "{ label: 'Try again', onPress }"],
      ["object", 'notificationTitle: "Recording"'],
      ["call", 'Alert.alert("Stop the trip?", t("x"))'],
      ["call", 'announce("Saved")'],
    ];
    for (const [kind, text] of cases) {
      const found = bypassesOf(text);
      expect(found, text).toHaveLength(1);
      expect(found[0]!.kind, text).toBe(kind);
    }
  });

  test("a template that only wraps the catalogue is not a sentence of its own", () => {
    expect(bypassesOf("<Banner message={`${problemText(code)} ${t(hint)}`} />")).toEqual([]);
  });

  test("a sentence typed into a real screen is found, with its line", () => {
    const home = screenSources().find((s) => s.path === "src/ui/screens/Home.tsx")!;
    expect(home.text).toContain('{t("home.none")}');
    const broken = home.text.replace('{t("home.none")}', "No trips yet.");
    const found = bypassesOf(broken);
    expect(found).toHaveLength(1);
    expect(found[0]).toMatchObject({ kind: "jsx text", text: "No trips yet." });
    expect(broken.split("\n")[found[0]!.line - 1]).toContain("No trips yet.");
  });
});

/** The names of the catalogue: a plural key by its base, an iOS version by the key it belongs to. */
function names(): string[] {
  const bases = pluralBases(messages);
  const out = new Set<string>();
  for (const key of Object.keys(messages)) {
    const base = key.replace(new RegExp(`\\.(${PLURAL_CATEGORIES.join("|")})$`), "");
    out.add((bases.has(base) ? base : key).replace(/\.ios$/, ""));
  }
  return [...out].sort();
}

/** In the catalogue and read by no screen today. The review does not need to read them first. */
const NOT_USED_YET = ["common.close", "unit.minutes", "unit.trips"];

describe("the keys of the catalogue and the sources", () => {
  const sources = appSources();
  const families = new Set(sources.flatMap((s) => [...keyFamilies(s.text)]));
  const underAFamily = (name: string) =>
    [...families].some((f) => name.startsWith(`${f}.`) && !name.slice(f.length + 1).includes("."));
  const used = (name: string) => underAFamily(name) || sources.some((s) => mentions(s.text, name));

  test("every sentence of the catalogue is read by the app, except the ones listed", () => {
    const unused = names().filter((name) => !used(name));
    expect(unused).toEqual(NOT_USED_YET);
  });

  test("the list of sentences not used yet is exact: a name on it that is read is a stale entry", () => {
    for (const name of NOT_USED_YET) expect(names(), name).toContain(name);
    expect(names().filter((name) => NOT_USED_YET.includes(name) && used(name))).toEqual([]);
  });

  test("the families that the sources build keys from are the ones this test knows", () => {
    expect([...families].sort()).toEqual([
      "account.device.state",
      "purpose",
      "signin.error",
      "state",
      "trip.check",
      "trip.problem",
    ]);
  });

  /** The keys `prefix.*` of the catalogue, one level down, by their last part. */
  const under = (prefix: string): string[] =>
    names()
      .filter(
        (name) => name.startsWith(`${prefix}.`) && !name.slice(prefix.length + 1).includes("."),
      )
      .map((name) => name.slice(prefix.length + 1))
      .sort();

  test("a key that is built at run time exists for every value it can have", () => {
    // purpose.*: the purposes of a trip (types.ts).
    expect(under("purpose")).toEqual([...PURPOSES].sort());
    // state.*: the status of a trip is a state of the server, or one of the three of the phone.
    expect(under("state")).toEqual(
      [...new Set([...SERVER_STATES, "uploading", "waiting", "problem"])].sort(),
    );
    // signin.error.*: the words of SignInFailure (native/auth.ts).
    const auth = sources.find((s) => s.path === "src/native/auth.ts")!.text;
    const union = /export type SignInFailure =([^;]+);/.exec(auth)![1]!;
    expect(under("signin.error")).toEqual([...union.matchAll(/"(\w+)"/g)].map((m) => m[1]!).sort());
    // trip.check.*: the flags of the integrity check that present.ts knows.
    expect(under("trip.check")).toEqual(["low_accuracy", "seq_gap", "speed_jump"]);
    // account.device.state.*: the states of a device on the instance.
    expect(under("account.device.state")).toEqual(["active", "revoked"]);
  });

  test("the problem sentences that TripDetail picks by the code of the refusal exist", () => {
    const detail = sources.find((s) => s.path === "src/ui/screens/TripDetail.tsx")!.text;
    for (const code of ["state_conflict", "discarded", "gone", "unknown"]) {
      expect(detail, code).toContain(`case "${code}":`);
      expect(names(), code).toContain(`trip.problem.${code}`);
    }
  });
});
