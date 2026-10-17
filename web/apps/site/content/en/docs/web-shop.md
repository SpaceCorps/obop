---
title: "Run a web shop"
description: "Publish products with pictures, take orders with stock, discount codes and taxes, and get paid by bank transfer or card, from one obop instance."
lead: "The public shop is part of every instance that serves the public site. Here is how to put it to work."
---

## Products and pictures

Install the `website` and `ecommerce` apps. In the backoffice, open a listing and add its price, stock and a main picture. Upload from the picture field: PNG, JPEG, GIF and WebP are accepted, up to 5 MiB by default. Every upload is re-encoded, so location data and camera details in the file are removed. SVG is refused because it can carry scripts.

Add alt text to every picture. The shop shows it to screen readers and uses it in social previews. Extra pictures go in the listing's gallery; pages and posts have a cover picture. Pictures become public only when their listing is published.

Uploaded files live in the directory named by `OBOP_BLOB_DIR`, or beside your SQLite file, or in the database on PostgreSQL.

## Stock

Set a listing's stock policy to track quantity. Orders take their units in the same step that creates the order, so two buyers cannot both get the last one. Cancelled, failed and expired orders put the units back, once.

## Discount codes and taxes

Create codes under Discount codes: a percentage or an amount, optional minimum, maximum uses and a per-customer limit. A buyer who types a wrong code gets one generic message, whatever the reason, and repeated guesses are slowed down.

Add tax rates by country and category, and choose whether prices include the tax. The shop computes every total on the server in whole cents; nothing the browser sends can change a price, a discount or a tax.

## Getting paid

Under Site settings choose `payment_methods`:

- `manual` shows your bank details (`payment_instructions`) and holds the stock for `payment_hold_days`. Mark the order paid when the money arrives.
- `stripe` sends the buyer to Stripe Checkout. Connect Stripe first (Connections), then add the webhook address `https://your-shop/hooks/pay/stripe` in your Stripe dashboard. The buyer's return page asks the server, so it cannot be forged.
- `fake` is a demo provider for trials, available only when the operator sets `OBOP_DEMO_PAYMENTS=1`.

Refund an order from the backoffice with the Refund action. The money goes back through the provider first; if the provider refuses, the order stays paid.

obop does not send email receipts. The order confirmation page is the receipt and can be printed.
