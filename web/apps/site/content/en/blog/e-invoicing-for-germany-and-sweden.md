---
title: "E-invoicing for Germany and Sweden: what we are building"
description: "Germany makes structured e-invoices compulsory for every business from 2028, and Swedish public buyers already receive them. This is how obop plans to read, check and write them, and what exists today."
date: "2026-11-25"
---

From 2028-01-01 every business established in Germany must send its invoices to other businesses in Germany as structured e-invoices. A PDF is no longer enough. This is the main reason Germany is obop's first country, and e-invoicing is the work we have started now. Sweden asks for the same European standard on invoices to public buyers. This post says what the law asks for, what obop has today and what we are building.

## What the law asks for

German law (§ 14 UStG) defines an e-invoice as an invoice issued, sent and received in a structured electronic format. That format must follow the European standard for e-invoicing, EN 16931, or be another format both sides agree on that can still be processed electronically. Invoices must be kept for eight years, with their origin, content and readability assured (§ 14b UStG). obop keeps a received e-invoice's XML file unchanged. The duty comes in steps:

- **Since 2025-01-01:** every business must be able to receive e-invoices.
- **From 2027-01-01:** businesses with more than EUR 800,000 turnover in the previous year must issue them.
- **From 2028-01-01:** every business must issue them.

Exceptions include invoices up to EUR 250 (§ 33 UStDV) and, until the end of 2027, EDI procedures agreed with the recipient (§ 27 Abs. 38 UStG).

Germany accepts several formats. obop plans to handle three of them: XRechnung, ZUGFeRD 2.5.2 (the same as Factur-X 1.09.2, a PDF with the XML inside) and Peppol BIS Billing 3.0. Behind these formats are two XML syntaxes, UBL and CII, and both carry the same EN 16931 content.

## Sweden

In Sweden the duty covers invoices to the public sector. [Lag (2018:1277) om elektroniska fakturor till följd av offentlig upphandling](https://www.riksdagen.se/sv/dokument-och-lagar/dokument/svensk-forfattningssamling/lag-20181277-om-elektroniska-fakturor-till_sfs-2018-1277/) came into force on 2019-04-01. For procurements begun from that date, a supplier's invoices to a public buyer must be electronic invoices that follow the European standard, EN 16931 (4 §), and public buyers must receive and process such invoices (5 §). Supplier and buyer may agree on another standard. The law does not name a format. [Upphandlingsmyndigheten](https://www.upphandlingsmyndigheten.se/digitalisering-och-e-handel/e-handel/regler-for-e-handel-och-e-faktura/), responsible for the related regulations since 2026-07-01, recommends Peppol BIS Billing 3, an implementation of EN 16931, and [state agencies must support it](https://www.upphandlingsmyndigheten.se/digitalisering-och-e-handel/e-handel/e-faktura-fran-statliga-myndigheter/). Public buyers must be registered in Peppol as invoice recipients and must be able to receive both UBL and CII.

## What obop has today

obop has an Invoicing app. It keeps customer invoices and their lines, adds up the total and gives each posted invoice the next number of a gapless yearly series. An AI agent may only propose posting an invoice; a person posts it.

Invoicing does not do e-invoicing. It cannot produce an XRechnung, a Factur-X or a Peppol BIS file, cannot read one, and does not check anything against EN 16931. The invoice also has no VAT breakdown yet, and the standard requires one.

## What we are building

The work has two halves. We start with reading, because businesses have had to be ready to receive e-invoices since 2025:

1. **Read and check.** Read incoming UBL and CII invoices, Peppol BIS Billing 3.0 included, into one EN 16931 data model, covering the header (number, date, type, currency), seller and buyer, lines, the VAT breakdown and the totals, and check them against the standard's rules, the German XRechnung rules and the Peppol BIS rules. The tests use the official sample invoices, each with the result we expect.
2. **Write and send.** Turn an obop invoice into an XRechnung or a Factur-X file, or a Peppol BIS Billing 3.0 file for Swedish customers, and send it by email or as a file.

Sending through the Peppol network comes later, in the second wave of the plan. It will go through a certified access point that the business chooses, with its own credentials. SpaceCorps will not run an access point.

As with everything in obop, each step will be a command that works the same way in the CLI, the REST API and MCP, so an AI agent can read an incoming invoice or prepare an outgoing one as easily as a person can.

## The dates

The German pack has a target date of 2027-09-30 for its stable release, three months before the duty applies to everyone. The alpha (2027-03-31) and beta (2027-06-30) dates are proposals. The pack is called stable only when its test cases pass and an accountant has signed it off. If that has not happened, it stays beta and the date stays where it is.

## What comes next

The first step is the reader: UBL invoices, including Peppol BIS, read into the EN 16931 model and tested against the official samples. CII follows, then the checks. The next post will show the reader running on the official sample invoices.

You can follow the work in the [repository](https://github.com/SpaceCorps/obop), where the [plan for the German pack](https://github.com/SpaceCorps/obop/blob/main/docs/LOCALIZATION.md) and the [decisions](https://github.com/SpaceCorps/obop/blob/main/docs/DECISIONS.md) behind it are public.

## Sources

(retrieved 2026-11-25)

- [§ 14 UStG, Ausstellung von Rechnungen](https://www.gesetze-im-internet.de/ustg_1980/__14.html), Bundesministerium der Justiz (gesetze-im-internet.de)
- [§ 14b UStG, Aufbewahrung von Rechnungen](https://www.gesetze-im-internet.de/ustg_1980/__14b.html), Bundesministerium der Justiz (gesetze-im-internet.de)
- [§ 27 UStG, Allgemeine Übergangsvorschriften (Absatz 38)](https://www.gesetze-im-internet.de/ustg_1980/__27.html), Bundesministerium der Justiz (gesetze-im-internet.de)
- [§ 33 UStDV, Rechnungen über Kleinbeträge](https://www.gesetze-im-internet.de/ustdv_1980/__33.html), Bundesministerium der Justiz (gesetze-im-internet.de)
- [Lag (2018:1277) om elektroniska fakturor till följd av offentlig upphandling](https://www.riksdagen.se/sv/dokument-och-lagar/dokument/svensk-forfattningssamling/lag-20181277-om-elektroniska-fakturor-till_sfs-2018-1277/), Sveriges riksdag
- [Regler för e-handel och e-faktura](https://www.upphandlingsmyndigheten.se/digitalisering-och-e-handel/e-handel/regler-for-e-handel-och-e-faktura/), Upphandlingsmyndigheten
- [E-faktura från statliga myndigheter](https://www.upphandlingsmyndigheten.se/digitalisering-och-e-handel/e-handel/e-faktura-fran-statliga-myndigheter/), Upphandlingsmyndigheten
- [XRechnung](https://xeinkauf.de/xrechnung/), XStandards Einkauf (KoSIT)
- [ZUGFeRD/Factur-X](https://www.ferd-net.de/standards/zugferd), FeRD
- [ZUGFeRD 2.5.2 english](https://www.ferd-net.de/publikationen-produkte/publikationen/detailseite/zugferd-252-english), FeRD
- [Peppol BIS Billing 3.0](https://docs.peppol.eu/poacc/billing/3.0/), OpenPeppol
- [docs/LOCALIZATION.md](https://github.com/SpaceCorps/obop/blob/main/docs/LOCALIZATION.md), obop
- [docs/DECISIONS.md](https://github.com/SpaceCorps/obop/blob/main/docs/DECISIONS.md), obop
