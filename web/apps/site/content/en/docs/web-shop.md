---
title: "Run a web shop"
description: "Publish products with pictures, take orders with stock, discount codes and taxes, and get paid by bank transfer or card, from one obop instance."
lead: "The public shop is part of every instance that serves the public site. Here is how to put it to work."
---

An instance serves the public site when it is started with `obop serve --public-site` (or `OBOP_PUBLIC_SITE=1`); the back office then moves to `/admin/`.

## Products and pictures

Install the `website` and `ecommerce` apps. In the backoffice, open a listing and add its price, stock and a main picture. Upload from the picture field: PNG, JPEG, GIF and WebP are accepted, up to 5 MiB by default. Every upload is re-encoded, so location data and camera details in the file are removed, and a GIF is stored as a PNG of its first frame (an animation keeps only that frame). SVG is refused because it can carry scripts.

Add alt text to every picture. The shop shows it to screen readers and uses it in social previews. Extra pictures go in the listing's gallery; pages and posts have a cover picture. Pictures become public only when their listing is published, and a gallery picture also only when it is marked published itself.

Uploaded files live in the directory named by `OBOP_BLOB_DIR`; without it, in a directory `<file>.blobs` beside your SQLite file, or in the database on PostgreSQL. `OBOP_BLOB_STORE=gdrive://FOLDER_ID` keeps them in a Google Drive folder instead (tried only against a stand-in, not against Google).

## Stock

Set a listing's stock policy to `block_when_out` and enter the quantity in stock. Orders take their units in the same step that creates the order, so two buyers cannot both get the last one. `allow_backorder` also takes units but lets the quantity go below zero; `always_available` does not track stock. Cancelled, failed and expired orders put the units back, once.

## Discount codes and taxes

Create codes under Discount codes: a percentage or an amount, optional minimum, maximum uses and a per-customer limit. A buyer who types a wrong code gets one generic message, whatever the reason, and repeated guesses are slowed down.

Add tax rates by country and category, and choose whether prices include the tax. The shop computes every total on the server in whole minor currency units (cents for euros); nothing the browser sends can change a price, a discount or a tax.

## Getting paid

Set the site settings with `obop site set KEY VALUE` (or the `setting.update` command). For payment, set `payment_methods` (a comma separated list):

- `manual` shows your bank details (`payment_instructions`) and holds the stock for `payment_hold_days`. Mark the order paid when the money arrives.
- `stripe` sends the buyer to Stripe Checkout. It has been tried only against a local stand-in, not against Stripe itself. Connect Stripe first as a connection owned by the instance (Connections, with a restricted API key and the webhook signing secret). Then add the webhook address `https://your-shop/hooks/pay/stripe` in your Stripe dashboard with the events `checkout.session.completed`, `checkout.session.async_payment_succeeded`, `checkout.session.async_payment_failed` and `checkout.session.expired`. The buyer's return page ignores what the browser sends and asks Stripe, so a forged return changes nothing.
- `fake` is a demo provider for trials, available only when the operator sets `OBOP_DEMO_PAYMENTS=1`.

Refund a paid or delivered order from the backoffice (this needs the `ecommerce.manager` group). For a card payment the money goes back through Stripe first; if Stripe refuses, the order is not refunded. For a bank-transfer order obop only records the refund; you send the money back yourself.

obop does not send email receipts. The order confirmation page is the receipt and can be printed.
