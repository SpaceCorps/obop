import type { Page } from "@playwright/test";
import { expect, test } from "../support/fixtures.ts";
import type { ModuleSpec } from "../support/modules.ts";
import { isCommand } from "../support/urls.ts";

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

/** What the page sends with every call of the action: the `lang` of the address, the header, the cookie. */
type Asked = { lang: string | null; header: string | null; cookie: string | null };

function watchFinish(page: Page, model: string): Asked[] {
  const asked: Asked[] = [];
  page.on("request", async (r) => {
    if (!isCommand(r.url(), model, "finish")) return;
    const url = new URL(r.url());
    const headers = await r.allHeaders();
    asked.push({
      lang: url.searchParams.get("lang"),
      header: headers["accept-language"] ?? null,
      cookie: headers["cookie"] ?? null,
    });
  });
  return asked;
}

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
    const asked = watchFinish(page, model);
    await signIn("root", `/${model}/${model}?view=list&id=${rec.id}`);
    const panel = page.locator("aside.record");
    await panel.getByRole("button", { name: "Finish" }).click();
    await expect(page.locator(".toast").filter({ hasText: EN })).toBeVisible();
    expect(asked).toMatchObject([{ lang: "en", header: "en" }]);
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
      const asked = watchFinish(page, model);
      await signIn("root", `/${model}/${model}?view=list&id=${rec.id}`);
      await page.locator("aside.record").getByRole("button", { name: "Avsluta" }).click();
      await expect(page.locator(".toast").filter({ hasText: SV })).toBeVisible();
      // The language of the interface went out, not the browser's German.
      expect(asked).toMatchObject([{ lang: "sv", header: "sv" }]);
      // And nothing moved.
      expect((await api.run("root", `${model}.get`, { id: rec.id })).state).toBe("open");
    });
  });

  // The cookie outranks Accept-Language on the server, and it can differ from what the interface
  // shows: an instance default the person never chose is not written to it, and a cookie the forum
  // set as HttpOnly cannot be overwritten by the page. The page therefore asks with ?lang=.
  test.describe("an instance default, a browser in another language and a cookie that says otherwise", () => {
    test.use({ language: null, locale: "de-DE" });

    test("the refusal is in the language the interface shows", async ({
      page,
      context,
      baseURL,
      signIn,
      api,
      scratchModule,
      allowHttpErrors,
    }) => {
      allowHttpErrors(400);
      const model = await scratchModule(spec("Assertion default"));
      const rec = await api.create("root", model, {});
      const before = (await api.run("root", "setting.get", { id: "instance" })) as {
        value: string;
      };
      try {
        const settings = { ...JSON.parse(before.value || "{}"), language: "sv" };
        await api.run("root", "setting.update", {
          id: "instance",
          value: JSON.stringify(settings),
        });
        // English, as the forum would have set it: the page cannot overwrite an HttpOnly cookie.
        await context.addCookies([
          { name: "obop_lang", value: "en", url: baseURL!, httpOnly: true },
        ]);
        const asked = watchFinish(page, model);
        await signIn("root", `/${model}/${model}?view=list&id=${rec.id}`);
        // No saved choice, a German browser: the interface is the administrator's Swedish.
        await expect(page.locator("html")).toHaveAttribute("lang", "sv");
        await page.locator("aside.record").getByRole("button", { name: "Avsluta" }).click();
        await expect(page.locator(".toast").filter({ hasText: SV })).toBeVisible();
        expect(asked).toHaveLength(1);
        // The conflict was real: the call carried the English cookie and the German browser's
        // language was not sent either.
        expect(asked[0]!.cookie).toContain("obop_lang=en");
        expect(asked[0]).toMatchObject({ lang: "sv", header: "sv" });
      } finally {
        await api.run("root", "setting.update", { id: "instance", value: before.value });
      }
    });
  });
});
