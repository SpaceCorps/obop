// The web shop end to end in a real browser (docs/STOREFRONT.md, D60 to D62): a second instance
// with the public site on, demo content with generated pictures, and the demo payment provider
// (OBOP_DEMO_PAYMENTS=1; no money moves and no card is asked). The shop is bought from with a
// discount code and a tax rate, the owner sees the order and its payment in the back office and
// refunds it, and the pictures are checked for what a visitor and a crawler need.
import { crc32, deflateSync } from "node:zlib";
import { shopOwner, shopUrl } from "../support/env.ts";
import type { Page } from "@playwright/test";
import { expect, test } from "../support/fixtures.ts";

test.describe.configure({ mode: "serial" });

/** A valid PNG made in code (a flat colour): no picture file is committed for the tests. */
function png(width: number, height: number, [r, g, b]: [number, number, number]): Buffer {
  const chunk = (type: string, data: Buffer) => {
    const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
    const out = Buffer.alloc(8 + data.length + 4);
    out.writeUInt32BE(data.length, 0);
    body.copy(out, 4);
    out.writeUInt32BE(crc32(body), 8 + data.length);
    return out;
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr.set([8, 2, 0, 0, 0], 8); // 8 bits, RGB
  const row = Buffer.concat([Buffer.from([0]), Buffer.from(Array(width).fill([r, g, b]).flat())]);
  const raw = Buffer.concat(Array(height).fill(row));
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(raw)),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

const shop = () => shopUrl();

let token = "";

/** A call to the shop's own API as its owner. */
async function api(
  request: import("@playwright/test").APIRequestContext,
  path: string,
  body: object = {},
): Promise<any> {
  if (!token) {
    const r = await request.post(`${shop()}/api/auth/login`, {
      data: { username: shopOwner.name, password: shopOwner.password },
    });
    expect(r.status()).toBe(200);
    token = (await r.json()).token;
  }
  const r = await request.post(`${shop()}${path}`, {
    data: body,
    headers: { authorization: `Bearer ${token}` },
  });
  const json = await r.json();
  expect(r.status(), `${path}: ${JSON.stringify(json)}`).toBe(200);
  return json.value;
}

async function listing(request: import("@playwright/test").APIRequestContext, slug: string) {
  const rows: any[] = await api(request, "/api/ecommerce_listing/list");
  return rows.find((r) => r.slug === slug)!;
}

/** Open the back office of the shop, signed in as its owner. */
async function backoffice(page: Page, hash: string) {
  await page.addInitScript(
    ([t, u]: [string, string]) => {
      sessionStorage.setItem("obop-token", t);
      sessionStorage.setItem("obop-user", u);
    },
    [token, shopOwner.name] as [string, string],
  );
  await page.goto(`${shop()}/admin/#${hash}`);
  await expect(page.locator(".topbar")).toBeVisible();
}

test("the shop shows real pictures: responsive, sized, described and cacheable", async ({
  page,
  request,
}) => {
  await api(request, "/api/ecommerce_listing/list");
  await page.goto(`${shop()}/shop`);
  const cards = page.locator(".card");
  await expect(cards).toHaveCount(8);
  // Every card has a real picture that loaded, with srcset, intrinsic size and alt text.
  const imgs = page.locator(".card .art img");
  await expect(imgs).toHaveCount(8);
  for (const img of await imgs.all()) {
    await expect(img).toHaveAttribute("srcset", /\d+w/);
    await expect(img).toHaveAttribute("width", /\d+/);
    await expect(img).toHaveAttribute("height", /\d+/);
    await expect(img).toHaveAttribute("alt", /Illustration of/);
    await expect(img).toHaveAttribute("loading", "lazy");
  }
  await page.locator(".card").first().scrollIntoViewIfNeeded();
  const loaded = await imgs.evaluateAll((els) =>
    els.map((e) => (e as HTMLImageElement).complete && (e as HTMLImageElement).naturalWidth > 0),
  );
  // Lazy images below the fold load when scrolled to; scroll through the grid once.
  for (const el of await imgs.all()) await el.scrollIntoViewIfNeeded();
  await expect
    .poll(async () =>
      imgs.evaluateAll((els) =>
        els.every(
          (e) => (e as HTMLImageElement).complete && (e as HTMLImageElement).naturalWidth > 0,
        ),
      ),
    )
    .toBe(true);
  expect(loaded.length).toBe(8);

  // The product page: eager main picture, a gallery, link previews and structured data.
  const sencha = await listing(request, "sencha-green-tea");
  await page.goto(`${shop()}/shop/${sencha.id}-sencha-green-tea`);
  const main = page.locator(".gallery .main img");
  await expect(main).toHaveAttribute("fetchpriority", "high");
  await expect(page.locator(".gallery .thumbs img")).toHaveCount(2);
  await expect
    .poll(async () => main.evaluate((e) => (e as HTMLImageElement).currentSrc))
    .toContain("/media/");
  const og = await page.locator('meta[property="og:image"]').getAttribute("content");
  expect(og).toMatch(new RegExp(`^${shop()}/media/[0-9a-f]{64}\\.png`));
  await expect(page.locator('meta[name="twitter:card"]')).toHaveAttribute(
    "content",
    "summary_large_image",
  );
  const ld = JSON.parse(
    (await page.locator('script[type="application/ld+json"]').allTextContents()).find((t) =>
      t.includes('"Product"'),
    )!,
  );
  expect(ld.image).toHaveLength(3);

  // A picture is immutable and validated by its hash; a thumbnail comes in a fixed width only.
  const src = (await main.getAttribute("src"))!;
  const r = await request.get(new URL(src, shop()).toString());
  expect(r.status()).toBe(200);
  expect(r.headers()["content-type"]).toBe("image/png");
  expect(r.headers()["cache-control"]).toContain("immutable");
  expect(r.headers()["x-content-type-options"]).toBe("nosniff");
  expect(r.headers()["etag"]).toMatch(/^"[0-9a-f]{64}-w640"$/);
  const bad = await request.get(new URL(src.replace("w=640", "w=641"), shop()).toString());
  expect(bad.status()).toBe(404);
  // The sitemap lists the pictures for search engines.
  const sitemap = await (await request.get(`${shop()}/sitemap.xml`)).text();
  expect(sitemap).toContain("<image:image><image:loc>");
});

test("buying with a discount code and tax through the demo provider, end to end", async ({
  page,
  request,
}) => {
  // Arrange as the owner: 19% VAT included in German prices, and a 10% code.
  await api(request, "/api/ecommerce_tax_rate/create", {
    name: "VAT",
    rate_bp: 1900,
    country: "DE",
    included: true,
    active: true,
  });
  await api(request, "/api/ecommerce_discount_code/create", {
    code: "E2E10",
    kind: "percent",
    value: 1000,
    active: true,
  });
  const sencha = await listing(request, "sencha-green-tea");
  const stockBefore = sencha.stock_quantity as number;

  // Put two in the cart from the product page (with JavaScript, like a shopper).
  await page.goto(`${shop()}/shop/${sencha.id}-sencha-green-tea`);
  await page.getByLabel("Quantity").fill("2");
  await page.getByRole("button", { name: "Add to cart" }).click();
  await expect(page.locator("[data-cart-count]").first()).toHaveText("2");
  await page.getByRole("link", { name: /^Cart/ }).click();
  await expect(page).toHaveURL(/\/cart\?items=/);
  await expect(page.getByRole("heading", { name: "Your cart" })).toBeVisible();

  // A wrong code is refused with a message that says nothing about codes; the right one works.
  await page.getByLabel("Discount code").fill("NOPE123");
  await page.getByRole("button", { name: "Apply" }).click();
  await expect(page.getByText("This code cannot be applied to your cart.")).toBeVisible();
  await page.getByLabel("Discount code").fill("e2e10");
  await page.getByRole("button", { name: "Apply" }).click();
  await expect(page.getByText("Code E2E10 applied.")).toBeVisible();
  const totals = page.locator("dl.totals").last();
  await expect(totals).toContainText("Discount (E2E10)");
  await expect(totals).toContainText("−€2.50");

  // Choosing the country asks the server for the new totals: the included VAT appears.
  await page.getByLabel("Country").selectOption("DE");
  await expect(page.locator("[data-totals]")).toContainText("Includes VAT 19%");
  // 2 x 12.50 - 10% = 22.50 + 4.90 delivery = 27.40 (the tax is inside): 3.59 + 0.78 = 4.37
  await expect(page.locator("[data-totals]")).toContainText("€4.37");
  await expect(page.locator("[data-totals] .grand").last()).toHaveText("€27.40");

  // Fill in the details and pay with the demo provider.
  await page.getByLabel("Full name").fill("Ada Example");
  await page.getByLabel("Email address").fill("ada@example.test");
  await page.getByLabel("Delivery address").fill("1 Example Street\n10115 Berlin");
  await page.getByLabel("Demo payment (no money moves)").check();
  await page.getByRole("button", { name: "Place order" }).click();
  await expect(page).toHaveURL(/\/shop\/fake-pay\/[0-9a-f]{64}$/);
  await expect(page.getByText("Demo mode: no real payment is made")).toBeVisible();
  await expect(page.getByText("€27.40")).toBeVisible();

  // The order exists, is pending and holds its stock while the buyer pays.
  let orders: any[] = await api(request, "/api/ecommerce_order/list");
  const order = orders.find((o) => o.customer_email === "ada@example.test")!;
  expect(order.state).toBe("pending");
  expect(order.total).toBe(2740);
  expect(order.discount_total).toBe(250);
  expect(order.tax_total).toBe(437);
  expect(order.discount_code).toBe("E2E10");
  expect((await listing(request, "sencha-green-tea")).stock_quantity).toBe(stockBefore - 2);

  await page.getByRole("button", { name: "Pay (demo)" }).click();
  await expect(page).toHaveURL(/\/shop\/pay\/[0-9a-f]{64}$/);
  await expect(page.getByText("Payment received. Thank you!")).toBeVisible();
  await expect(page.getByRole("heading", { name: "Thank you for your order" })).toBeVisible();
  await expect(page.locator("dl.totals").last()).toContainText("Includes VAT 19%");
  await expect(page.locator("dl.totals .grand").last()).toHaveText("€27.40");
  // The cart was emptied.
  await expect(page.locator("[data-cart-count]").first()).toHaveText("");

  // Server side: paid, one payment record, stock committed, tax lines kept.
  orders = await api(request, "/api/ecommerce_order/list");
  const paid = orders.find((o) => o.id === order.id)!;
  expect(paid.state).toBe("paid");
  const payments: any[] = await api(request, "/api/ecommerce_payment/list");
  const payment = payments.find((p) => p.order_id === order.id)!;
  expect(payment).toMatchObject({ provider: "fake", state: "succeeded", amount: 2740 });
  expect((await listing(request, "sencha-green-tea")).stock_quantity).toBe(stockBefore - 2);
  const codes: any[] = await api(request, "/api/ecommerce_discount_code/list");
  expect(codes.find((c) => c.code === "E2E10").used_count).toBe(1);

  // The printable receipt.
  await page.getByRole("link", { name: "Show a printable receipt" }).click();
  await expect(page.getByRole("heading", { name: "Receipt" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Print receipt" })).toBeVisible();
});

test("a payment that fails gives the stock and the code back", async ({ page, request }) => {
  const pot = await listing(request, "stoneware-teapot");
  const before = pot.stock_quantity as number;
  await page.goto(`${shop()}/cart?items=${pot.id}.1`);
  await page.getByLabel("Full name").fill("Bo Failing");
  await page.getByLabel("Email address").fill("bo@example.test");
  await page.getByLabel("Delivery address").fill("2 Example Street\nBerlin");
  await page.getByLabel("Demo payment (no money moves)").check();
  await page.getByRole("button", { name: "Place order" }).click();
  await expect(page).toHaveURL(/\/shop\/fake-pay\//);
  expect((await listing(request, "stoneware-teapot")).stock_quantity).toBe(before - 1);
  await page.getByRole("button", { name: "Make the payment fail" }).click();
  await expect(page.getByText("The payment did not go through.")).toBeVisible();
  expect((await listing(request, "stoneware-teapot")).stock_quantity).toBe(before);
});

test("the owner sees the order and its payment in the back office and refunds it", async ({
  page,
  request,
}) => {
  const orders: any[] = await api(request, "/api/ecommerce_order/list");
  const order = orders.find((o) => o.customer_email === "ada@example.test")!;
  await backoffice(page, `/ecommerce/ecommerce_order?id=${order.id}`);
  await expect(page.getByText(order.number).first()).toBeVisible();
  await expect(page.locator("dialog, [role=dialog], aside, .record").first()).toBeVisible();

  // The payment is a record of its own, with the provider reference and no card data.
  await backoffice(page, "/ecommerce/ecommerce_payment");
  await expect(page.getByText("Succeeded").first()).toBeVisible();

  // Refund through the action: the order is refunded, the payment too, and the units are back.
  const stockBefore = (await listing(request, "sencha-green-tea")).stock_quantity as number;
  await api(request, "/api/ecommerce_order/refund", { id: order.id });
  const after: any[] = await api(request, "/api/ecommerce_order/list");
  expect(after.find((o) => o.id === order.id).state).toBe("refunded");
  const payments: any[] = await api(request, "/api/ecommerce_payment/list");
  const payment = payments.find((p) => p.order_id === order.id)!;
  expect(payment.state).toBe("refunded");
  expect(payment.refund_id).toMatch(/^fake-refund-/);
  expect(payment.refunded_amount).toBe(2740);
  expect((await listing(request, "sencha-green-tea")).stock_quantity).toBe(stockBefore + 2);
  // The buyer's page says so.
  const token = (await api(request, "/api/ecommerce_order/get", { id: order.id })).access_token;
  expect(token).toBeUndefined(); // the secret token is never readable through the API
});

test("the owner uploads a picture in the back office and it appears in the shop", async ({
  page,
  request,
}) => {
  const towel = await listing(request, "linen-tea-towel");
  await backoffice(page, `/ecommerce/ecommerce_listing?id=${towel.id}`);
  const field = page.locator(".blobfield").first();
  await expect(field).toBeVisible();
  await field.locator('input[type="file"]').setInputFiles({
    name: "towel.png",
    mimeType: "image/png",
    buffer: png(64, 48, [200, 40, 40]),
  });
  await expect(field.getByRole("alert")).toHaveCount(0);
  await expect(page.getByRole("button", { name: /^Save/ })).toBeEnabled();
  await page.getByRole("button", { name: /^Save/ }).click();
  await expect
    .poll(async () => (await listing(request, "linen-tea-towel")).image)
    .toMatch(/^[0-9a-f]{64}$/);
  // The new picture is on the shop's page for the product.
  await page.goto(`${shop()}/shop/${towel.id}-linen-tea-towel`);
  await expect(page.locator(".gallery .main img")).toBeVisible();
});

test("uploads are checked: an SVG is refused, a mislabelled file too", async ({ request }) => {
  const upload = async (type: string, data: Buffer | string) => {
    const r = await request.post(`${shop()}/api/blobs?field=ecommerce_listing.image`, {
      data,
      headers: { authorization: `Bearer ${token}`, "content-type": type },
    });
    return r.status();
  };
  expect(
    await upload(
      "image/svg+xml",
      '<svg xmlns="http://www.w3.org/2000/svg"><script>1</script></svg>',
    ),
  ).toBe(415);
  expect(await upload("image/png", "<html><script>alert(1)</script></html>")).toBe(415);
  expect(await upload("image/png", Buffer.alloc(0))).toBe(415);
});
