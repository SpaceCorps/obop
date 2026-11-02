import { describe, expect, test } from "vite-plus/test";
import { PURPOSES, SERVER_STATES } from "../core/types.ts";
import { appSources, screenSources } from "../test-support/app-sources.ts";
import { en, PLURAL_CATEGORIES } from "./en.ts";
import { pluralBases } from "./check.ts";
import { bypassesOf, JSX_ONLY_PROPS, keyFamilies, mentions, TEXT_PROPS } from "./usage.ts";

const messages = en as Record<string, string>;

describe("the screens say nothing that is not in the catalogue", () => {
  test("no screen, and not App.tsx, has a sentence of its own", () => {
    const sources = screenSources();
    expect(sources.map((s) => s.path)).toContain("App.tsx");
    expect(sources.length).toBeGreaterThanOrEqual(9);
    for (const source of sources) expect(bypassesOf(source.text), source.path).toEqual([]);
  });

  test("the screens use the neutral words where nothing says the server wrote them, and tell when the language was not saved", () => {
    const detail = appSources().find((s) => s.path === "src/ui/screens/TripDetail.tsx")!;
    const flat = detail.text.replace(/\s+/g, " ");
    expect(flat).toContain(
      'isServerProblem(trip.problem.code) ? "trip.problem.title" : "trip.problem.title.app"',
    );
    expect(flat).toMatch(
      /isServerProblem\(trip\.problem\.code\) \? "trip\.problem\.details" : "trip\.problem\.details\.app"/,
    );
    const account = appSources().find((s) => s.path === "src/ui/screens/Account.tsx")!;
    const accountFlat = account.text.replace(/\s+/g, " ");
    // The note: set from the answer of the store, decided by `languageNoteKey` (tested in
    // language-note.test.ts), shown as a warning, and nothing else sets it.
    expect(accountFlat).toContain("void languageStore.choose(choice).then(setLanguageStored);");
    expect(accountFlat).toContain("const languageNote = languageNoteKey(languageStored);");
    expect(accountFlat).toContain(
      '{languageNote ? ( <Banner tone="warning" title={t(languageNote)} testID="language-unsaved" /> ) : null}',
    );
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
    // The authorization address asks for a language only when the app positively knows it
    // (`knownLanguage`, tested in language.test.ts), never for the English the app shows by default.
    expect(auth.text).toContain("lang: languageStore.known() ?? undefined");
    expect(auth.text).not.toMatch(/lang:\s*getLanguage\(\)/);
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

  test("it reads every argument of a dialog, not only the title", () => {
    const message = `Alert.alert(t('x'), 'Are you sure you want to stop the trip?', [{ text: t("y") }]);`;
    expect(bypassesOf(message)).toMatchObject([
      { kind: "call", text: "Are you sure you want to stop the trip?" },
    ]);
    const both = `Alert.alert("Stop the trip?", \`Delete \${n} trips?\`, [{ text: t("y") }]);`;
    expect(bypassesOf(both).map((f) => f.text)).toEqual(["Stop the trip?", "Delete trips?"]);
    expect(bypassesOf(`announce(busy ? "Saving" : t("a.b"))`).map((f) => f.text)).toEqual([
      "Saving",
    ]);
    // The title and the message come from the catalogue, the buttons too.
    expect(bypassesOf(`Alert.alert(t("a"), t("b"), [{ text: t("c"), style: "cancel" }])`)).toEqual(
      [],
    );
    expect(bypassesOf(`Alert.alert(title, message)`)).toEqual([]);
  });

  test("it reads the literals inside the value of a text prop, not only a plain literal", () => {
    const cases: [string, string[]][] = [
      ["<Button label={busy ? 'Waiting' : 'Start'} />", ["Waiting", "Start"]],
      ["<Button label={busy ? t('a') : 'Start'} />", ["Start"]],
      ["<Banner title='Problem' />", ["Problem"]],
      ["<Field error='Required' />", ["Required"]],
      ["<Field error={x ? 'Required' : undefined} />", ["Required"]],
      ['<Banner message={"Saved " + n} />', ["Saved "]],
      ['<Row title={name ?? "Unknown driver"} />', ["Unknown driver"]],
      ["<Row subtitle={`Saved ${count} trips`} />", ["Saved  trips"]],
      ['<Text>{busy ? "Saving" : t("a")}</Text>', ["Saving"]],
      ['<Text>{ok && "Done"}</Text>', ["Done"]],
    ];
    for (const [text, expected] of cases) {
      const found = bypassesOf(text);
      expect(
        found.map((f) => f.text),
        text,
      ).toEqual(expected.map((e) => e.trim().replace(/\s+/g, " ")));
      expect(
        found.every((f) => f.kind !== "unbalanced"),
        text,
      ).toBe(true);
    }
  });

  test("it leaves what is not a sentence alone", () => {
    const quiet = [
      '<Button label={os === "ios" ? t("a") : t("b")} />',
      "<Field error={err ? t('e') : undefined} />",
      '<Row title={t(busy ? "a.b" : "c.d")} subtitle={items.join(", ")} />',
      "const f = async () => { return 1; };",
      "const state = { error: 'failed', ok: false };",
      '<Text>{count > 1 ? t("a") : t("b")}</Text>',
      '<Button variant="ghost" testID="stop-button" />',
    ];
    for (const text of quiet) expect(bypassesOf(text), text).toEqual([]);
  });

  test("brackets or quotes that do not match are reported, not skipped", () => {
    const found = bypassesOf('<Button label={busy ? "Waiting" : />');
    expect(found.map((f) => f.kind)).toEqual(["unbalanced"]);
    expect(bypassesOf('Alert.alert("Stop?"').map((f) => f.kind)).toEqual(["unbalanced"]);
  });

  test("every text prop of the list is read, as an attribute and as an object key", () => {
    // The list is the scanner's own: removing a name from it fails here.
    expect(TEXT_PROPS).toEqual(
      expect.arrayContaining([
        "label",
        "title",
        "message",
        "hint",
        "helper",
        "subtitle",
        "placeholder",
        "value",
        "accessibilityLabel",
        "accessibilityHint",
        "notificationTitle",
        "notificationBody",
        "text",
      ]),
    );
    expect(JSX_ONLY_PROPS).toEqual(["error"]);
    for (const name of [...TEXT_PROPS, ...JSX_ONLY_PROPS]) {
      const attribute = bypassesOf(`<X ${name}="Some words here" />`);
      expect(attribute, name).toHaveLength(1);
      expect(attribute[0], name).toMatchObject({ kind: "prop", text: "Some words here" });
    }
    for (const name of TEXT_PROPS) {
      const key = bypassesOf(`const a = { ${name}: "Some words here" };`);
      expect(key, name).toHaveLength(1);
      expect(key[0], name).toMatchObject({ kind: "object", text: "Some words here" });
    }
    // `error` is also a state, so as an object key it is not read.
    expect(bypassesOf('const a = { error: "Some words here" };')).toEqual([]);
  });

  test("a child expression with a comparison is read; one that holds an element is not", () => {
    expect(
      bypassesOf('<Text>{count < 3 ? "Few trips" : "Many trips"}</Text>').map((f) => f.text),
    ).toEqual(["Few trips", "Many trips"]);
    expect(
      bypassesOf('<Text>{trips.length <= 0 && "No trips yet."}</Text>').map((f) => f.text),
    ).toEqual(["No trips yet."]);
    expect(
      bypassesOf('<Text>{trips.length > 0 && "Some trips"}</Text>').map((f) => f.text),
    ).toEqual(["Some trips"]);
    // An element in the expression: its own text is read when the scanner gets to that element.
    expect(bypassesOf('<View>{ok ? <Text>{t("a")}</Text> : null}</View>')).toEqual([]);
    expect(bypassesOf("<View>{items.map((x) => <Row title={t(x)} />)}</View>")).toEqual([]);
    // A comment as a child is skipped, with an apostrophe in it.
    expect(bypassesOf("<View>{/* the driver's phone */}</View>")).toEqual([]);
  });

  test("a single-quoted message with a comma and a bracket, and an unbalanced child expression", () => {
    const dialog = bypassesOf(`Alert.alert(t('a'), 'Really, stop (now)?', [{ text: t('b') }])`);
    expect(dialog.map((f) => [f.kind, f.text])).toEqual([["call", "Really, stop (now)?"]]);
    expect(bypassesOf('<Text>{busy ? "x" :</Text>').map((f) => f.kind)).toEqual(["unbalanced"]);
  });

  test("a type argument, an arrow and an entity are not read as an element or a sentence", () => {
    expect(bypassesOf('async function save(): Promise<void> { const mode = "edit"; }')).toEqual([]);
    expect(bypassesOf('const [a, b] = useState<string | null>(null); const k = "kind";')).toEqual(
      [],
    );
    expect(bypassesOf('const f = async (): Promise<Owner> { return await g("it\'s"); }')).toEqual(
      [],
    );
    expect(bypassesOf("<Text>&nbsp;</Text>")).toEqual([]);
    expect(bypassesOf("<Text>&nbsp;Hello</Text>")).toHaveLength(1);
  });

  test("what it cannot see stays unseen (ISSUES L55): the list of bypassesOf, one by one", () => {
    // A text that reaches a prop through a variable.
    expect(bypassesOf('const text = "Hello there"; <Banner message={text} />')).toEqual([]);
    // A literal inside brackets: a call, a parenthesised ternary.
    expect(bypassesOf('<Banner message={upper("Hello there")} />')).toEqual([]);
    expect(bypassesOf('<Banner message={(a ? "Hello" : "There")} />')).toEqual([]);
    // An object value that is a ternary.
    expect(bypassesOf('const b = { text: a ? "Hello" : "There" };')).toEqual([]);
    // A literal beside an element in a child expression (an element of its own is read by itself).
    expect(bypassesOf('<View>{ok ? <A /> : "Hello there"}</View>')).toEqual([]);
    // Text mixed with an expression, and text before an element.
    expect(bypassesOf("<Text>Distance: {km}</Text>")).toEqual([]);
    expect(bypassesOf("<Text>{n} trips driven</Text>")).toEqual([]);
    expect(bypassesOf('<Text>Press <B>{t("a")}</B></Text>')).toEqual([]);
    // A prop that is not in the lists.
    expect(bypassesOf('<Banner caption="Hello there" />')).toEqual([]);
  });

  test("a comment with a quote in a dialog is reported as unbalanced, not skipped", () => {
    expect(
      bypassesOf("Alert.alert(t('a'), /* the driver's choice */ t('b'))").map((f) => f.kind),
    ).toEqual(["unbalanced"]);
  });

  test("a template that only wraps the catalogue is not a sentence of its own", () => {
    expect(bypassesOf("<Banner message={`${problemText(code)} ${t(hint)}`} />")).toEqual([]);
  });

  test("a message typed into a dialog of a real screen is found", () => {
    const account = screenSources().find((s) => s.path === "src/ui/screens/Account.tsx")!;
    expect(account.text).toContain("Alert.alert(");
    const broken = account.text.replace(
      "Alert.alert(",
      'Alert.alert(t("account.signout"), "Typed English message", ',
    );
    expect(bypassesOf(broken).map((f) => f.text)).toEqual(["Typed English message"]);
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
