import { expect, test } from "../support/fixtures.ts";
import type { ModuleSpec } from "../support/modules.ts";

// The message of an assertion in the language of the request (D52 amendment): a package gives an
// assertion a `key` and translates its message in a label table; a REST call is answered in the
// language of the request, and the back office sends the language it shows as Accept-Language.
const EN = "Give it a title first";
const SV = "Ge den en titel forst";
const DE = "Gib zuerst einen Titel an";

const spec = (title: string): Omit<ModuleSpec, "id"> => ({
  title,
  fields: [{ name: "title" }, { name: "state", kind: "select", options: ["open", "done"] }],
  actions: [
    {
      name: "finish",
      field: "state",
      from: "open",
      to: "done",
      assert: [{ expr: "is_set(title)", message: EN, key: "needs_title" }],
    },
  ],
  labels: (id) => `
[labels.sv.model.${id}.action.finish]
label = "Avsluta"

[labels.sv.model.${id}.action.finish.assert.needs_title]
message = ${JSON.stringify(SV)}

[labels.de.model.${id}.action.finish.assert.needs_title]
message = ${JSON.stringify(DE)}
`,
});

const suffix = "(assertion is_set(title) failed)";

test.describe("the message of an assertion in the language of the request", () => {
  test("the API answers in the language the request asks for, in the order query, cookie, header", async ({
    request,
    api,
    scratchModule,
    allowHttpErrors,
  }) => {
    allowHttpErrors(400);
    const model = await scratchModule(spec("Assertion language"));
    const rec = await api.create("root", model, {});
    const token = await api.token("root");
    const finish = async (query: string, headers: Record<string, string>) => {
      const res = await request.post(`/api/${model}/finish${query}`, {
        data: { id: rec.id },
        headers: { authorization: `Bearer ${token}`, ...headers },
      });
      expect(res.status()).toBe(400);
      return res.json();
    };

    // No language at all, or English: the manifest's text, as it always was.
    const plain = await finish("", { "accept-language": "" });
    expect(plain.error).toBe(`${EN} ${suffix}`);
    expect(plain.code).toBe("assertion_failed");
    expect(plain.assert_key).toBe("needs_title");
    expect((await finish("", { "accept-language": "en-GB,en;q=0.9" })).error).toBe(
      `${EN} ${suffix}`,
    );
    // The header, with a region and q-values.
    const sv = await finish("", { "accept-language": "de;q=0.4, sv-SE;q=0.9" });
    expect(sv.error).toBe(`${SV} ${suffix}`);
    expect(sv.assert_key).toBe("needs_title");
    // The cookie over the header, the query over both.
    expect((await finish("", { cookie: "obop_lang=sv", "accept-language": "de" })).error).toBe(
      `${SV} ${suffix}`,
    );
    expect(
      (await finish("?lang=de", { cookie: "obop_lang=sv", "accept-language": "fi" })).error,
    ).toBe(`${DE} ${suffix}`);
    // A language nobody ships falls through to the next source.
    expect((await finish("?lang=xx", { cookie: "obop_lang=sv" })).error).toBe(`${SV} ${suffix}`);
    // A shipped language the package did not translate reads as the manifest.
    expect((await finish("?lang=fi", { cookie: "obop_lang=sv" })).error).toBe(`${EN} ${suffix}`);
  });

  test("the back office shows the refusal of an English interface in English", async ({
    page,
    signIn,
    api,
    scratchModule,
    allowHttpErrors,
  }) => {
    allowHttpErrors(400);
    const model = await scratchModule(spec("Assertion screen"));
    const rec = await api.create("root", model, {});
    const asked: string[] = [];
    page.on("request", (r) => {
      if (r.url().endsWith(`/api/${model}/finish`))
        asked.push(r.headers()["accept-language"] ?? "(none)");
    });
    await signIn("root", `/${model}/${model}?view=list&id=${rec.id}`);
    const panel = page.locator("aside.record");
    await panel.getByRole("button", { name: "Finish" }).click();
    await expect(page.locator(".toast").filter({ hasText: EN })).toBeVisible();
    expect(asked).toEqual(["en"]);
  });

  test.describe("in Swedish, in a German browser", () => {
    test.use({ language: "sv", locale: "de-DE" });

    test("the refusal is the Swedish text", async ({
      page,
      signIn,
      api,
      scratchModule,
      allowHttpErrors,
    }) => {
      allowHttpErrors(400);
      const model = await scratchModule(spec("Assertion svenska"));
      const rec = await api.create("root", model, {});
      const asked: string[] = [];
      page.on("request", (r) => {
        if (r.url().endsWith(`/api/${model}/finish`))
          asked.push(r.headers()["accept-language"] ?? "(none)");
      });
      await signIn("root", `/${model}/${model}?view=list&id=${rec.id}`);
      await page.locator("aside.record").getByRole("button", { name: "Avsluta" }).click();
      await expect(page.locator(".toast").filter({ hasText: SV })).toBeVisible();
      // The language of the interface went out, not the browser's German.
      expect(asked).toEqual(["sv"]);
      // And nothing moved.
      expect((await api.run("root", `${model}.get`, { id: rec.id })).state).toBe("open");
    });
  });
});
