import { expect, test, type Api } from "../support/fixtures.ts";

// The signer's page of a signing link (D92, docs/SIGN.md; docs/ISSUES.md O10) in a real browser on a
// phone: the request, the full name, the consent tick and Sign, then the page that says it is signed;
// and, back in the back office, the evidence check of the signed request (O3), which compares the
// stored evidence with the security log entry of the signature. Everything here is made up: a
// synthetic PDF and an invented person.

async function withPackages(api: Api, ids: string[], run: () => Promise<void>) {
  const root = await api.token("root");
  const had = new Set(((await api.run("root", "module.list")) as any[]).map((m) => m.id));
  const added: string[] = [];
  try {
    for (const id of ids)
      if (!had.has(id)) {
        await api.run("root", "marketplace_listing.install", { id });
        added.push(id);
      }
    await run();
  } finally {
    for (const id of added.reverse()) await api.post("/api/module/uninstall", { id }, root);
  }
}

test("a signer signs on a phone and the request shows its evidence check (O10, O3)", async ({
  page,
  browser,
  request,
  api,
  signIn,
  uid,
  baseURL,
}) => {
  test.setTimeout(120_000);
  await withPackages(api, ["documents", "sign"], async () => {
    const root = await api.token("root");
    const title = uid("NDA m3");
    const upload = await request.post(
      "/api/blobs?field=documents_document.attachment&filename=nda.pdf",
      {
        data: Buffer.from(`%PDF-1.4\n% made-up test document ${title}\n%%EOF\n`),
        headers: { authorization: `Bearer ${root}`, "content-type": "application/pdf" },
      },
    );
    expect(upload.status()).toBe(201);
    const file = (await upload.json()).id as string;
    const doc = await api.create("root", "documents_document", {
      title: `${title} (file)`,
      kind: "contract",
      confidentiality: "internal",
      owner: "root",
      attachment: file,
    });
    const req = await api.create("root", "sign_request", {
      title,
      signer_name: "Jane Example",
      document_id: doc.id,
    });
    await api.run("root", "sign_request.send_document", { id: req.id });
    const link = (await api.run("root", "sign_link.create", { request_id: req.id })).link as string;
    const path = link.startsWith("/") ? link : new URL(link).pathname;

    // The signer: no account, a phone, the English page.
    const phone = await browser.newContext({
      viewport: { width: 390, height: 844 },
      isMobile: true,
      hasTouch: true,
      baseURL,
    });
    try {
      const signer = await phone.newPage();
      await signer.goto(`${path}?lang=en`);
      await expect(signer.getByText(title).first()).toBeVisible();
      await expect(signer.getByText(/Jane Example/).first()).toBeVisible();
      // Nothing on the page is wider than the phone.
      const wide = await signer.evaluate(
        () => document.documentElement.scrollWidth - window.innerWidth,
      );
      expect(wide).toBeLessThanOrEqual(0);
      await signer.getByLabel("Your full name").fill("Jane Example");
      await signer.getByRole("checkbox").check();
      await signer.getByRole("button", { name: "Sign", exact: true }).click();
      await expect(signer.getByRole("heading", { name: "Signed" })).toBeVisible();
      // The same link again shows the page that says it is signed, and no form.
      await signer.goto(`${path}?lang=en`);
      await expect(signer.getByRole("heading", { name: "Signed" })).toBeVisible();
      await expect(signer.getByRole("button", { name: "Sign", exact: true })).toHaveCount(0);
    } finally {
      await phone.close();
    }

    const got = await api.run("root", "sign_request.get", { id: req.id });
    expect(got.state).toBe("signed");
    expect(got.signed_name).toBe("Jane Example");
    expect(got.evidence_check).toBe("matches");

    // The back office shows the check in the panel of the signed request.
    await signIn("root", `/sign/sign_request?view=list&id=${req.id}`);
    const check = page.locator(".sign-evidence-check");
    await expect(check.getByText("Matches")).toBeVisible();
    // An administrator's edit of what the signer read shows as a difference.
    await api.run("root", "sign_request.update", { id: req.id, title: `${title} (edited)` });
    const after = await api.run("root", "sign_request.get", { id: req.id });
    expect(after.evidence_check).toBe("differs");
  });
});

test("a title edited while the signer reads the page is shown again before anything is signed (O59)", async ({
  browser,
  request,
  api,
  uid,
  baseURL,
}) => {
  test.setTimeout(120_000);
  await withPackages(api, ["documents", "sign"], async () => {
    const root = await api.token("root");
    const title = uid("Pay 100 EUR");
    const upload = await request.post(
      "/api/blobs?field=documents_document.attachment&filename=terms.pdf",
      {
        data: Buffer.from(`%PDF-1.4\n% made-up test document ${title}\n%%EOF\n`),
        headers: { authorization: `Bearer ${root}`, "content-type": "application/pdf" },
      },
    );
    expect(upload.status()).toBe(201);
    const doc = await api.create("root", "documents_document", {
      title: `${title} (file)`,
      kind: "contract",
      confidentiality: "internal",
      owner: "root",
      attachment: (await upload.json()).id,
    });
    const req = await api.create("root", "sign_request", {
      title,
      signer_name: "Jane Example",
      document_id: doc.id,
    });
    await api.run("root", "sign_request.send_document", { id: req.id });
    const link = (await api.run("root", "sign_link.create", { request_id: req.id })).link as string;
    const path = link.startsWith("/") ? link : new URL(link).pathname;
    const phone = await browser.newContext({
      viewport: { width: 390, height: 844 },
      isMobile: true,
      hasTouch: true,
      baseURL,
    });
    try {
      const signer = await phone.newPage();
      await signer.goto(`${path}?lang=en`);
      await expect(signer.getByText(title).first()).toBeVisible();
      // The sender changes the title while the page is open.
      const edited = `${title} (now 900 EUR)`;
      await api.run("root", "sign_request.update", { id: req.id, title: edited });
      await signer.getByLabel("Your full name").fill("Jane Example");
      await signer.getByRole("checkbox").check();
      await signer.getByRole("button", { name: "Sign", exact: true }).click();
      await expect(signer.getByRole("alert")).toContainText(
        "The page has changed since you opened it",
      );
      await expect(signer.getByText(edited).first()).toBeVisible();
      expect((await api.run("root", "sign_request.get", { id: req.id })).state).toBe("sent");
      // Signed from the page with the new title.
      await signer.getByRole("checkbox").check();
      await signer.getByRole("button", { name: "Sign", exact: true }).click();
      await expect(signer.getByRole("heading", { name: "Signed" })).toBeVisible();
    } finally {
      await phone.close();
    }
    const got = await api.run("root", "sign_request.get", { id: req.id });
    expect(got.state).toBe("signed");
    expect(got.title).toBe(`${title} (now 900 EUR)`);
    expect(got.evidence_check).toBe("matches");
  });
});
